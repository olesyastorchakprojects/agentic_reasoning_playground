#![cfg(feature = "postgres-integration")]

use std::sync::OnceLock;

use distributed_diagnostics::api_clients::postgres::incident_card_store::PostgresIncidentCardStore;
use distributed_diagnostics::api_clients::qdrant::cards_collection::{
    CardsCollection, QdrantCardsCollectionDense, QdrantCardsCollectionHybrid,
};
use distributed_diagnostics::api_clients::qdrant::practice_chunks_collection::{
    PracticeChunksCollection, QdrantPracticeChunksCollectionDense,
    QdrantPracticeChunksCollectionHybrid,
};
use distributed_diagnostics::config::{self, CollectionSettings, Settings};
use distributed_diagnostics::request_pipeline::candidate_card_retrieval::CandidateCardRetrieval;
use distributed_diagnostics::request_pipeline::card_hydration::CardHydration;
use distributed_diagnostics::request_pipeline::incident_evidence_retrieval::IncidentEvidenceRetrieval;
use distributed_diagnostics::shared_types::{IncidentCard, NormalizedUserRequest};
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use tokio::sync::{Mutex, MutexGuard};

fn init_env() {
    static ENV_INIT: OnceLock<()> = OnceLock::new();
    ENV_INIT.get_or_init(|| {
        let _ = dotenvy::dotenv();
        let _ = dotenvy::from_filename("Execution/distributed_diagnostics/.env.test");
        let _ = dotenvy::from_filename(".env.test");
    });
}

fn required_env(key: &str) -> String {
    init_env();
    std::env::var(key).unwrap_or_else(|_| panic!("{key} must be set for live retrieval integration test"))
}

fn live_postgres_url() -> String {
    required_env("POSTGRES_URL")
}

fn load_live_settings() -> Settings {
    init_env();

    let _ = required_env("POSTGRES_URL");
    let _ = required_env("QDRANT_URL");
    let _ = required_env("OLLAMA_URL");

    if std::env::var("TRACING_ENDPOINT").is_err() {
        std::env::set_var("TRACING_ENDPOINT", "http://localhost:4317");
    }
    if std::env::var("METRICS_ENDPOINT").is_err() {
        std::env::set_var("METRICS_ENDPOINT", "http://localhost:4318");
    }

    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("crate directory must have repository parent");
    std::env::set_current_dir(repo_root).expect("switch current directory to repository root");
    let runtime_path = manifest_dir.join("runtime.toml");
    let ingest_path = manifest_dir.join("ingest.toml");

    config::load(&runtime_path, &ingest_path)
    .expect("load live settings")
}

async fn live_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().await
}

async fn connect_seed_pool(database_url: &str) -> sqlx::PgPool {
    PgPoolOptions::new()
        .connect(database_url)
        .await
        .expect("connect seed pool")
}

async fn fetch_seed_cards(
    pool: &sqlx::PgPool,
    card_store: &PostgresIncidentCardStore,
) -> Vec<IncidentCard> {
    let rows = sqlx::query(
        r#"
        SELECT case_id
        FROM diagnostics.incident_cards
        ORDER BY case_id
        LIMIT 12
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("fetch seed case_ids");

    let case_ids: Vec<String> = rows
        .into_iter()
        .map(|row| row.get::<String, _>("case_id"))
        .collect();

    assert!(
        !case_ids.is_empty(),
        "seed database must contain at least one incident card",
    );

    card_store
        .get_cards_by_case_ids(&case_ids)
        .await
        .expect("load seed cards from postgres store")
}

fn candidate_queries(card: &IncidentCard) -> Vec<String> {
    let mut queries = Vec::new();
    queries.push(card.title.clone());
    queries.push(format!("{} {}", card.title, card.short_summary));

    if !card.canonical_symptoms.is_empty() {
        queries.push(format!(
            "{} {}",
            card.title,
            card.canonical_symptoms
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }

    queries.retain(|query| !query.trim().is_empty());
    queries
}

fn normalized_request(query: String) -> NormalizedUserRequest {
    let token_count = query.split_whitespace().count().max(1);
    NormalizedUserRequest {
        query,
        input_token_count: token_count,
    }
}

async fn build_cards_collection(
    settings: &Settings,
) -> Arc<dyn CardsCollection> {
    use std::sync::Arc;

    match &settings.retrieval.cards.collection {
        CollectionSettings::Dense(_) => Arc::new(
            QdrantCardsCollectionDense::from_settings(
                &settings.retrieval.cards,
                &settings.embedding_model,
                &settings.retrieval.qdrant_url,
            )
            .expect("build dense cards collection"),
        ),
        CollectionSettings::Hybrid(_) => Arc::new(
            QdrantCardsCollectionHybrid::from_settings(
                &settings.retrieval.cards,
                &settings.embedding_model,
                &settings.retrieval.qdrant_url,
            )
            .await
            .expect("build hybrid cards collection"),
        ),
    }
}

async fn build_practice_collection(
    settings: &Settings,
) -> Arc<dyn PracticeChunksCollection> {
    use std::sync::Arc;

    match &settings.retrieval.practice.collection {
        CollectionSettings::Dense(_) => Arc::new(
            QdrantPracticeChunksCollectionDense::from_settings(
                &settings.retrieval.practice,
                &settings.embedding_model,
                &settings.retrieval.qdrant_url,
            )
            .expect("build dense practice collection"),
        ),
        CollectionSettings::Hybrid(_) => Arc::new(
            QdrantPracticeChunksCollectionHybrid::from_settings(
                &settings.retrieval.practice,
                &settings.embedding_model,
                &settings.retrieval.qdrant_url,
            )
            .await
            .expect("build hybrid practice collection"),
        ),
    }
}

use std::sync::Arc;

#[tokio::test]
async fn candidate_cards_hydration_and_practice_chunks_are_linked_live() {
    let _guard = live_lock().await;
    let settings = load_live_settings();
    let database_url = live_postgres_url();

    let card_store = Arc::new(
        PostgresIncidentCardStore::from_settings(&settings.postgres)
            .await
            .expect("connect postgres incident card store"),
    );
    let seed_pool = connect_seed_pool(&database_url).await;
    let seed_cards = fetch_seed_cards(&seed_pool, &card_store).await;

    let cards_collection = build_cards_collection(&settings).await;
    let practice_collection = build_practice_collection(&settings).await;

    let candidate_card_retrieval =
        CandidateCardRetrieval::new(settings.retrieval.cards.clone(), Arc::clone(&cards_collection))
            .expect("build candidate retrieval");
    let card_hydration = CardHydration::new(Arc::clone(&card_store));
    let incident_evidence_retrieval = IncidentEvidenceRetrieval::new(
        Arc::clone(&practice_collection),
        settings.retrieval.practice.clone(),
    )
    .expect("build incident evidence retrieval");

    let mut selected = None;
    let mut attempted_queries = 0usize;
    let mut queries_with_primary_candidate = 0usize;
    let mut queries_with_hydration = 0usize;
    let mut queries_with_evidence = 0usize;
    let mut first_hydration_error: Option<String> = None;
    let mut first_evidence_error: Option<String> = None;

    'outer: for card in &seed_cards {
        for query in candidate_queries(card) {
            attempted_queries += 1;
            let request = normalized_request(query.clone());
            let candidates = candidate_card_retrieval
                .retrieve(&request)
                .await
                .expect("candidate retrieval must succeed for live query");

            if candidates.primary.is_none() {
                continue;
            }
            queries_with_primary_candidate += 1;

            let hydration = match card_hydration.hydrate(&candidates).await {
                Ok(output) => output,
                Err(error) => {
                    if first_hydration_error.is_none() {
                        first_hydration_error = Some(error.to_string());
                    }
                    continue;
                }
            };
            queries_with_hydration += 1;

            let evidence = match incident_evidence_retrieval.retrieve(&request, &candidates).await {
                Ok(output) => output,
                Err(error) => {
                    if first_evidence_error.is_none() {
                        first_evidence_error = Some(error.to_string());
                    }
                    continue;
                }
            };

            if evidence.primary_chunks.is_empty() && evidence.alternative_chunks.is_empty() {
                continue;
            }
            queries_with_evidence += 1;

            selected = Some((request, candidates, hydration, evidence));
            break 'outer;
        }
    }

    let (request, candidates, hydration, evidence) = selected.expect(
        &format!(
            "no live query produced candidate cards, hydrated cards, and practice chunks; \
             attempted_queries={attempted_queries}, \
             with_primary_candidate={queries_with_primary_candidate}, \
             with_hydration={queries_with_hydration}, \
             with_nonempty_evidence={queries_with_evidence}, \
             first_hydration_error={:?}, \
             first_evidence_error={:?}",
            first_hydration_error,
            first_evidence_error
        ),
    );

    assert!(
        !request.query.trim().is_empty(),
        "normalized live query must not be empty",
    );

    let primary = candidates
        .primary
        .as_ref()
        .expect("selected live candidate set must have a primary card");
    assert!(
        !primary.case_id.trim().is_empty(),
        "candidate primary case_id must not be empty",
    );

    let hydrated_primary = hydration
        .primary
        .as_ref()
        .expect("hydration must return the primary card");
    assert_eq!(
        hydrated_primary.case_id, primary.case_id,
        "primary candidate case_id must resolve to the same postgres incident card",
    );
    assert!(
        !hydrated_primary.title.trim().is_empty(),
        "hydrated primary card must have a title",
    );

    let alternative_ids: Vec<String> = candidates
        .alternatives
        .iter()
        .map(|candidate| candidate.case_id.clone())
        .collect();
    let hydrated_alternative_ids: Vec<String> = hydration
        .alternatives
        .iter()
        .map(|card| card.case_id.clone())
        .collect();
    assert_eq!(
        hydrated_alternative_ids, alternative_ids,
        "hydrated alternative cards must preserve candidate case_id order",
    );

    assert!(
        !evidence.primary_chunks.is_empty() || !evidence.alternative_chunks.is_empty(),
        "practice evidence retrieval must return at least one chunk",
    );

    for chunk in &evidence.primary_chunks {
        assert!(
            !chunk.chunk_id.trim().is_empty(),
            "primary evidence chunk_id must not be empty",
        );
        assert!(
            !chunk.text.trim().is_empty(),
            "primary evidence chunk text must not be empty",
        );
        assert_eq!(
            chunk.case_id, primary.case_id,
            "primary evidence chunk case_id must point to the retrieved primary card",
        );
    }

    for chunk in &evidence.alternative_chunks {
        assert!(
            !chunk.chunk_id.trim().is_empty(),
            "alternative evidence chunk_id must not be empty",
        );
        assert!(
            !chunk.text.trim().is_empty(),
            "alternative evidence chunk text must not be empty",
        );
        assert!(
            alternative_ids.iter().any(|id| id == &chunk.case_id),
            "alternative evidence chunk case_id must point to one of the retrieved alternative cards",
        );
    }
}
