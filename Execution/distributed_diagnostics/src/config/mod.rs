mod load;
mod settings;

pub use load::load;
pub use settings::{
    BagOfWordsSettings, Bm25LikeSettings, ChunkPackingSettings, ChunkPackingSource,
    ChunkRolePackingSettings, CollectionRetrievalSettings, CollectionSettings,
    DenseCollectionSettings, DiagnosticUpdateChunkPackingSettings,
    DiagnosticUpdatePromptContextSettings, EmbeddingModelSettings, HybridCollectionSettings,
    IncidentEvidenceRetrievalProfiles, IncidentEvidenceRetrievalSettings,
    IncidentEvidenceTagProfile, InputNormalizationSettings,
    LlmStructuredGenerationRuntimeSettings, LlmStructuredGenerationSettings,
    ModelPricingSettings, ModelSettings, ObservabilitySettings,
    ObservationBoundaryResolverRuntimeSettings, ObservationExtractionRuntimeSettings,
    OllamaModelSettings, OllamaProviderSettings, PostgresSettings, PromptContextSettings,
    QueryStructuringRuntimeSettings, QueryStructuringSettings, ResolvedModelSettings,
    RetrievalSettings, RuntimeSettings, Settings, SparsePreprocessingSettings,
    SparseSettings, SparseStrategySettings, TogetherModelSettings,
    TogetherProviderSettings, TokenizerSettings,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config load: {0}")]
    Load(String),
    #[error("missing required environment variable: {key}")]
    MissingEnvironment { key: String },
    #[error("invalid value for '{field}': {reason}")]
    InvalidValue { field: String, reason: String },
}
