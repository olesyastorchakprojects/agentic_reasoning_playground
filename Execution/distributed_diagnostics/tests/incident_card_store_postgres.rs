#![cfg(feature = "postgres-integration")]

use distributed_diagnostics::api_clients::postgres::{
    IncidentCardStoreError, PostgresIncidentCardStore, PostgresIncidentCardStoreConfig,
};
use distributed_diagnostics::shared_types::{IncidentCard, IncidentPhase};
use sqlx::postgres::PgPoolOptions;
use std::sync::OnceLock;
use url::Url;

fn valid_card(case_id: &str) -> IncidentCard {
    IncidentCard {
        case_id: case_id.into(),
        title: "Test Incident".into(),
        source_type: "blog".into(),
        source_name: "engineering-blog".into(),
        source_path: format!("/posts/{case_id}"),
        vendor_or_project: Some("ExampleCorp".into()),
        system_type: None,
        version_tested: None,
        report_date: Some("2024-01-15".into()),
        short_summary: "A brief summary of the incident.".into(),
        canonical_symptoms: vec!["high latency".into()],
        affected_components: vec![],
        failure_mode_candidates: vec![],
        observed_phases: vec![],
        incident_phases: vec![IncidentPhase {
            phase_name: "Detection".into(),
            context: "Alert fired".into(),
            symptoms: vec![],
            user_visible_impact: vec![],
            observations: vec![],
            actions_taken: vec![],
            changes_after_actions: vec![],
        }],
        turning_points: vec![],
        candidate_explanations: vec![],
        diagnostic_patterns: vec![],
        discriminating_checks: vec![],
        expected_observations: vec![],
        investigation_steps: vec![],
        root_cause_summary: None,
        reasoning_summary: None,
        mitigations_or_workarounds: vec![],
        prevention_or_design_followups: vec![],
        claimed_guarantees: vec![],
        violated_properties: vec![],
        resolution_status: None,
        fix_versions: vec![],
        confidence_notes: vec![],
        source_refs: vec!["https://example.com/incident".into()],
    }
}

fn test_database_url() -> String {
    static ENV_INIT: OnceLock<()> = OnceLock::new();
    ENV_INIT.get_or_init(|| {
        let _ = dotenvy::from_filename("Execution/distributed_diagnostics/.env.test");
        let _ = dotenvy::from_filename(".env.test");
    });

    std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must point to postgres://.../distributed_diagnostics_test")
}

async fn connect_test_store() -> PostgresIncidentCardStore {
    let url = test_database_url();
    let parsed = Url::parse(&url).expect("TEST_DATABASE_URL must be a valid URL");
    assert_eq!(
        parsed.path().trim_start_matches('/'),
        "distributed_diagnostics_test",
        "TEST_DATABASE_URL must point to the dedicated distributed_diagnostics_test database",
    );

    PostgresIncidentCardStore::new(PostgresIncidentCardStoreConfig { postgres_url: url })
        .await
        .expect("connect to test database")
}

async fn cleanup_card(database_url: &str, case_id: &str) {
    let pool = PgPoolOptions::new()
        .connect(database_url)
        .await
        .expect("connect cleanup pool");
    let _ = sqlx::query("DELETE FROM diagnostics.incident_cards WHERE case_id = $1")
        .bind(case_id)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn put_card_persists_and_get_card_by_case_id_round_trips() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let card = valid_card("case-pg-001");

    store.put_card(&card).await.expect("put_card");
    let loaded = store
        .get_card_by_case_id(&card.case_id)
        .await
        .expect("get_card_by_case_id")
        .expect("card must exist");

    assert_eq!(loaded, card);
    cleanup_card(&database_url, &card.case_id).await;
}

#[tokio::test]
async fn put_card_fails_with_duplicate_case_id_on_second_insert() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let card = valid_card("case-pg-duplicate");

    store.put_card(&card).await.expect("first put_card");
    let err = store.put_card(&card).await.unwrap_err();

    assert!(matches!(err, IncidentCardStoreError::DuplicateCaseId(_)));
    cleanup_card(&database_url, &card.case_id).await;
}

#[tokio::test]
async fn get_card_by_case_id_returns_none_when_card_is_absent() {
    let store = connect_test_store().await;
    let loaded = store
        .get_card_by_case_id("case-pg-missing")
        .await
        .expect("query must succeed");
    assert!(loaded.is_none());
}

#[tokio::test]
async fn get_cards_by_case_ids_returns_only_existing_cards() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let card_a = valid_card("case-pg-batch-a");
    let card_b = valid_card("case-pg-batch-b");

    store.put_card(&card_a).await.expect("put card_a");
    store.put_card(&card_b).await.expect("put card_b");

    let loaded = store
        .get_cards_by_case_ids(&[
            card_a.case_id.clone(),
            "case-pg-missing".to_string(),
            card_b.case_id.clone(),
        ])
        .await
        .expect("batch query");

    assert_eq!(loaded.len(), 2);
    assert!(loaded.iter().any(|card| card.case_id == card_a.case_id));
    assert!(loaded.iter().any(|card| card.case_id == card_b.case_id));

    cleanup_card(&database_url, &card_a.case_id).await;
    cleanup_card(&database_url, &card_b.case_id).await;
}

#[tokio::test]
async fn get_cards_by_case_ids_deduplicates_duplicate_inputs() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let card = valid_card("case-pg-dedup");

    store.put_card(&card).await.expect("put_card");

    let loaded = store
        .get_cards_by_case_ids(&[
            card.case_id.clone(),
            card.case_id.clone(),
            card.case_id.clone(),
        ])
        .await
        .expect("batch query");

    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0], card);

    cleanup_card(&database_url, "case-pg-dedup").await;
}

#[tokio::test]
async fn integration_helper_requires_dedicated_test_database() {
    let url = test_database_url();
    let parsed = Url::parse(&url).unwrap();
    assert_eq!(parsed.path().trim_start_matches('/'), "distributed_diagnostics_test");
}
