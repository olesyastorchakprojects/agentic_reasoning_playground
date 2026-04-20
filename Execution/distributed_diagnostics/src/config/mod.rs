mod load;
mod settings;

pub use load::load;
pub use settings::{
    BagOfWordsSettings, Bm25LikeSettings, CollectionRetrievalSettings, CollectionSettings,
    DenseCollectionSettings, EmbeddingModelSettings, HybridCollectionSettings,
    InputNormalizationSettings, ModelSettings, ModelTransportSettings, ObservabilitySettings,
    OllamaModelSettings, PostgresSettings, QueryStructuringSettings, RetrievalSettings,
    RuntimeSettings, Settings, SparsePreprocessingSettings, SparseSettings,
    SparseStrategySettings, TogetherModelSettings, TokenizerSettings,
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
