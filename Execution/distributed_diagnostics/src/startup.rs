use std::sync::Arc;

use thiserror::Error;

use crate::api_clients::model::{ModelClientError, OllamaModelClient, TogetherModelClient};
use crate::api_clients::postgres::incident_card_store::{
    IncidentCardStore, IncidentCardStoreError, PostgresIncidentCardStore,
    PostgresIncidentCardStoreConfig,
};
use crate::api_clients::postgres::run_state_store::{
    PostgresRunStateStore, PostgresRunStateStoreConfig, RunStateStoreError,
};
use crate::api_clients::qdrant::cards_collection::{
    CardsCollectionError, QdrantCardsCollectionDense, QdrantCardsCollectionHybrid,
};
use crate::api_clients::qdrant::practice_chunks_collection::{
    PracticeChunksCollectionError, QdrantPracticeChunksCollectionDense,
    QdrantPracticeChunksCollectionHybrid,
};
use crate::api_clients::qdrant::theory_chunks_collection::{
    QdrantTheoryChunksCollectionDense, QdrantTheoryChunksCollectionHybrid,
    TheoryChunksCollectionError,
};
use crate::config::{
    CollectionSettings, LlmStructuredGenerationSettings, QueryStructuringSettings,
    ResolvedModelSettings, Settings,
};
use crate::orchestrator::orchestrator::Orchestrator;
use crate::orchestrator::run_repository::RunRepository;
use crate::orchestrator::step_executor::{StepExecutor, StepExecutorModules};
use crate::orchestrator::transition_policy::DiagnosticLoopTransitionPolicy;
use crate::request_pipeline::candidate_card_retrieval::{
    CandidateCardRetrieval, CandidateCardRetrievalError,
};
use crate::request_pipeline::card_branch_reranking::CardBranchReranking;
use crate::request_pipeline::diagnostic_update_prompt_context_assembly::{
    DiagnosticUpdatePromptContextAssembly, DiagnosticUpdatePromptContextAssemblyError,
};
use crate::request_pipeline::incident_evidence_retrieval::{
    IncidentEvidenceRetrieval, IncidentEvidenceRetrievalError,
};
use crate::request_pipeline::input_normalization::{InputNormalization, InputNormalizationError};
use crate::request_pipeline::llm_structured_generation::{
    LlmStructuredGeneration, LlmStructuredGenerationError,
};
use crate::request_pipeline::observation_boundary_resolver::{
    ObservationBoundaryResolver, ObservationBoundaryResolverError,
    ObservationBoundaryResolverSettings,
};
use crate::request_pipeline::observation_extraction::{
    ObservationExtraction, ObservationExtractionError, ObservationExtractionSettings,
};
use crate::request_pipeline::prompt_context_assembly::{
    PromptContextAssembly, PromptContextAssemblyError,
};
use crate::request_pipeline::query_structuring::{QueryStructuring, QueryStructuringError};
use crate::request_pipeline::response_validation_and_normalization::ResponseValidationAndNormalization;
use crate::request_pipeline::theory_evidence_retrieval::{
    TheoryEvidenceRetrieval, TheoryEvidenceRetrievalError,
};

#[derive(Debug, Error)]
pub enum StartupError {
    #[error("model client: {0}")]
    ModelClient(#[from] ModelClientError),

    #[error("incident card store: {0}")]
    IncidentCardStore(#[from] IncidentCardStoreError),

    #[error("run state store: {0}")]
    RunStateStore(#[from] RunStateStoreError),

    #[error("cards collection: {0}")]
    CardsCollection(#[from] CardsCollectionError),

    #[error("practice chunks collection: {0}")]
    PracticeChunksCollection(#[from] PracticeChunksCollectionError),

    #[error("theory chunks collection: {0}")]
    TheoryChunksCollection(#[from] TheoryChunksCollectionError),

    #[error("input normalization: {0}")]
    InputNormalization(#[from] InputNormalizationError),

    #[error("query structuring: {0}")]
    QueryStructuring(#[from] QueryStructuringError),

    #[error("llm structured generation: {0}")]
    LlmStructuredGeneration(#[from] LlmStructuredGenerationError),

    #[error("prompt context assembly: {0}")]
    PromptContextAssembly(#[from] PromptContextAssemblyError),

    #[error("candidate card retrieval: {0}")]
    CandidateCardRetrieval(#[from] CandidateCardRetrievalError),

    #[error("incident evidence retrieval: {0}")]
    IncidentEvidenceRetrieval(#[from] IncidentEvidenceRetrievalError),

    #[error("theory evidence retrieval: {0}")]
    TheoryEvidenceRetrieval(#[from] TheoryEvidenceRetrievalError),

    #[error("observation boundary resolver: {0}")]
    ObservationBoundaryResolver(#[from] ObservationBoundaryResolverError),

    #[error("observation extraction: {0}")]
    ObservationExtraction(#[from] ObservationExtractionError),

    #[error("diagnostic update prompt context assembly: {0}")]
    DiagnosticUpdatePromptContextAssembly(#[from] DiagnosticUpdatePromptContextAssemblyError),
}

pub async fn build_orchestrator(
    settings: &Settings,
) -> Result<Orchestrator<DiagnosticLoopTransitionPolicy>, StartupError> {
    let query_structuring_model_client = build_model_client(
        settings,
        "query_structuring",
        &settings.query_structuring.provider,
        &settings.query_structuring.model,
    )?;
    let observation_boundary_resolver_model_client = build_model_client(
        settings,
        "observation_boundary_resolver",
        &settings.observation_boundary_resolver.provider,
        &settings.observation_boundary_resolver.model,
    )?;
    let observation_extraction_model_client = build_model_client(
        settings,
        "observation_extraction",
        &settings.observation_extraction.provider,
        &settings.observation_extraction.model,
    )?;
    let llm_structured_generation_model_client = build_model_client(
        settings,
        "llm_structured_generation",
        &settings.llm_structured_generation.provider,
        &settings.llm_structured_generation.model,
    )?;

    let incident_card_store: Arc<dyn IncidentCardStore> = Arc::new(
        PostgresIncidentCardStore::new(PostgresIncidentCardStoreConfig {
            postgres_url: settings.postgres.url.clone(),
        })
        .await?,
    );

    let run_state_store = PostgresRunStateStore::new(PostgresRunStateStoreConfig {
        postgres_url: settings.postgres.url.clone(),
    })
    .await?;

    let qdrant_url = &settings.retrieval.qdrant_url;
    let embedding_model = &settings.embedding_model;

    let cards_collection: Arc<dyn crate::api_clients::qdrant::cards_collection::CardsCollection> =
        match &settings.retrieval.cards.collection {
            CollectionSettings::Dense(_) => Arc::new(QdrantCardsCollectionDense::from_settings(
                &settings.retrieval.cards,
                embedding_model,
                qdrant_url,
            )?),
            CollectionSettings::Hybrid(_) => Arc::new(
                QdrantCardsCollectionHybrid::from_settings(
                    &settings.retrieval.cards,
                    embedding_model,
                    qdrant_url,
                )
                .await?,
            ),
        };

    let practice_collection: Arc<
        dyn crate::api_clients::qdrant::practice_chunks_collection::PracticeChunksCollection,
    > = match &settings.retrieval.practice.collection {
        CollectionSettings::Dense(_) => {
            Arc::new(QdrantPracticeChunksCollectionDense::from_settings(
                &settings.retrieval.practice,
                embedding_model,
                qdrant_url,
            )?)
        }
        CollectionSettings::Hybrid(_) => Arc::new(
            QdrantPracticeChunksCollectionHybrid::from_settings(
                &settings.retrieval.practice,
                embedding_model,
                qdrant_url,
            )
            .await?,
        ),
    };

    let theory_collection: Arc<
        dyn crate::api_clients::qdrant::theory_chunks_collection::TheoryChunksCollection + Send + Sync,
    > = match &settings.retrieval.theory.collection {
        CollectionSettings::Dense(_) => {
            Arc::new(QdrantTheoryChunksCollectionDense::from_settings(
                &settings.retrieval.theory,
                embedding_model,
                qdrant_url,
            )?)
        }
        CollectionSettings::Hybrid(_) => Arc::new(
            QdrantTheoryChunksCollectionHybrid::from_settings(
                &settings.retrieval.theory,
                embedding_model,
                qdrant_url,
            )
            .await?,
        ),
    };

    let input_normalization =
        InputNormalization::new(settings.input_normalization.clone()).await?;

    let query_structuring = QueryStructuring::new(
        QueryStructuringSettings {
            controlled_vocabulary_path: settings.query_structuring.controlled_vocabulary_path.clone(),
            prompt_asset_path: settings.query_structuring.prompt_asset_path.clone(),
            max_output_tokens: settings.query_structuring.max_output_tokens,
        },
        Arc::clone(&query_structuring_model_client),
    )?;

    let candidate_card_retrieval = CandidateCardRetrieval::new(
        settings.retrieval.cards.clone(),
        Arc::clone(&cards_collection),
    )?;

    let card_hydration = crate::request_pipeline::card_hydration::CardHydration::new(
        Arc::clone(&incident_card_store),
    );

    let incident_evidence_retrieval = IncidentEvidenceRetrieval::new(
        Arc::clone(&practice_collection),
        settings.retrieval.practice.clone(),
    )?;

    let theory_evidence_retrieval = TheoryEvidenceRetrieval::new(
        Arc::clone(&theory_collection),
        settings.retrieval.theory.clone(),
    )?;

    let prompt_context_assembly = PromptContextAssembly::new(settings.prompt_context.clone())?;

    let observation_boundary_resolver = ObservationBoundaryResolver::new(
        ObservationBoundaryResolverSettings {
            prompt_asset_path: settings.observation_boundary_resolver.prompt_asset_path.clone(),
            max_output_tokens: settings.observation_boundary_resolver.max_output_tokens,
        },
        Arc::clone(&observation_boundary_resolver_model_client),
    )?;

    let observation_extraction = ObservationExtraction::new(
        ObservationExtractionSettings {
            prompt_asset_path: settings.observation_extraction.prompt_asset_path.clone(),
            max_output_tokens: settings.observation_extraction.max_output_tokens,
        },
        Arc::clone(&observation_extraction_model_client),
    )?;

    let card_branch_reranking = CardBranchReranking::new();

    let diagnostic_update_prompt_context_assembly = DiagnosticUpdatePromptContextAssembly::new(
        settings.diagnostic_update_prompt_context.clone(),
    )?;

    let llm_structured_generation = LlmStructuredGeneration::new(
        LlmStructuredGenerationSettings {
            max_output_tokens: settings.llm_structured_generation.max_output_tokens,
        },
        Arc::clone(&llm_structured_generation_model_client),
    )?;

    let response_validation_and_normalization = ResponseValidationAndNormalization::new();

    let mut executor = StepExecutor::new(StepExecutorModules {
        input_normalization,
        query_structuring,
        information_adequacy_analyzer: crate::request_pipeline::information_adequacy_analyzer::InformationAdequacyAnalyzer::new(),
        observation_boundary_resolver,
        observation_extraction,
        candidate_card_retrieval,
        card_branch_reranking,
        card_hydration,
        incident_evidence_retrieval,
        theory_evidence_retrieval,
        prompt_context_assembly,
        diagnostic_update_prompt_context_assembly,
        llm_structured_generation,
        response_validation_and_normalization,
    });

    if let Some(path) = &settings.chunk_audit_log_path {
        match crate::chunk_audit_log::ChunkAuditLog::open(path) {
            Ok(log) => {
                executor = executor.with_chunk_audit_log(log);
                tracing::info!("chunk audit log enabled: {path}");
            }
            Err(e) => {
                tracing::warn!("chunk audit log: failed to open '{path}': {e}");
            }
        }
    }

    let run_repository = RunRepository::new(run_state_store);
    let policy = DiagnosticLoopTransitionPolicy::new();
    let config_snapshot = settings.build_run_config_snapshot();

    Ok(Orchestrator::new(policy, executor, run_repository)
        .with_config_snapshot(config_snapshot))
}

fn build_model_client(
    settings: &Settings,
    stage_name: &str,
    provider: &str,
    model_name: &str,
) -> Result<Arc<dyn crate::api_clients::model::ModelClient>, StartupError> {
    let resolved = settings
        .model
        .resolve(provider, model_name)
        .unwrap_or_else(|| panic!("stage {stage_name} has unresolved model binding"));
    match resolved {
        ResolvedModelSettings::Ollama(ollama) => Ok(Arc::new(OllamaModelClient::from_settings(
            &ollama,
        )?)),
        ResolvedModelSettings::Together(together) => Ok(Arc::new(
            TogetherModelClient::from_settings(&together)?,
        )),
    }
}
