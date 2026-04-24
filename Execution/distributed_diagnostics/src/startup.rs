use std::sync::Arc;

use thiserror::Error;

use crate::api_clients::model::{ModelClientError, OllamaModelClient, TogetherModelClient};
use crate::api_clients::postgres::incident_card_store::{
    IncidentCardStoreError, PostgresIncidentCardStore, PostgresIncidentCardStoreConfig,
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
use crate::config::{CollectionSettings, ModelTransportSettings, Settings};
use crate::orchestrator::orchestrator::Orchestrator;
use crate::orchestrator::run_repository::RunRepository;
use crate::orchestrator::step_executor::{StepExecutor, StepExecutorModules};
use crate::orchestrator::transition_policy::LinearPipelineTransitionPolicy;
use crate::request_pipeline::candidate_card_retrieval::{
    CandidateCardRetrieval, CandidateCardRetrievalError,
};
use crate::request_pipeline::incident_evidence_retrieval::{
    IncidentEvidenceRetrieval, IncidentEvidenceRetrievalError,
};
use crate::request_pipeline::input_normalization::{InputNormalization, InputNormalizationError};
use crate::request_pipeline::llm_structured_generation::{
    LlmStructuredGeneration, LlmStructuredGenerationError,
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
}

pub async fn build_orchestrator(
    settings: &Settings,
) -> Result<Orchestrator<LinearPipelineTransitionPolicy>, StartupError> {
    let model_client: Arc<dyn crate::api_clients::model::ModelClient> =
        match &settings.model.transport {
            ModelTransportSettings::Ollama(ollama) => {
                Arc::new(OllamaModelClient::from_settings(ollama)?)
            }
            ModelTransportSettings::Together(together) => {
                Arc::new(TogetherModelClient::from_settings(together)?)
            }
        };

    let incident_card_store = Arc::new(
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

    let query_structuring =
        QueryStructuring::new(settings.query_structuring.clone(), Arc::clone(&model_client))?;

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

    let llm_structured_generation = LlmStructuredGeneration::new(
        settings.llm_structured_generation.clone(),
        Arc::clone(&model_client),
    )?;

    let response_validation_and_normalization = ResponseValidationAndNormalization::new();

    let executor = StepExecutor::new(StepExecutorModules {
        input_normalization,
        query_structuring,
        candidate_card_retrieval,
        card_hydration,
        incident_evidence_retrieval,
        theory_evidence_retrieval,
        prompt_context_assembly,
        llm_structured_generation,
        response_validation_and_normalization,
    });

    let run_repository = RunRepository::new(run_state_store);
    let policy = LinearPipelineTransitionPolicy::new();

    Ok(Orchestrator::new(policy, executor, run_repository))
}
