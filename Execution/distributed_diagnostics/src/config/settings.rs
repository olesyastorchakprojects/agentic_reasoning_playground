use crate::utils::retry::RetryPolicyConfig;

#[derive(Debug, Clone)]
pub struct Settings {
    pub runtime: RuntimeSettings,
    pub retrieval: RetrievalSettings,
    pub model: ModelSettings,
    pub embedding_model: EmbeddingModelSettings,
    pub observability: ObservabilitySettings,
    pub postgres: PostgresSettings,
}

#[derive(Debug, Clone)]
pub struct RuntimeSettings {
    pub config_version: String,
}

#[derive(Debug, Clone)]
pub struct EmbeddingModelSettings {
    pub url: String,
    pub name: String,
    pub dimension: usize,
}

#[derive(Debug, Clone)]
pub struct ObservabilitySettings {
    pub tracing_enabled: bool,
    pub metrics_enabled: bool,
    pub tracing_endpoint: String,
    pub metrics_endpoint: String,
    pub trace_batch_scheduled_delay_ms: u64,
    pub metrics_export_interval_ms: u64,
}

#[derive(Debug, Clone)]
pub struct PostgresSettings {
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct RetrievalSettings {
    pub qdrant_url: String,
    pub cards: CollectionRetrievalSettings,
    pub practice: CollectionRetrievalSettings,
    pub theory: CollectionRetrievalSettings,
}

#[derive(Debug, Clone)]
pub struct CollectionRetrievalSettings {
    pub top_k: usize,
    pub score_threshold: f32,
    pub embedding_retry: RetryPolicyConfig,
    pub qdrant_retry: RetryPolicyConfig,
    pub collection: CollectionSettings,
}

#[derive(Debug, Clone)]
pub enum CollectionSettings {
    Dense(DenseCollectionSettings),
    Hybrid(HybridCollectionSettings),
}

#[derive(Debug, Clone)]
pub struct DenseCollectionSettings {
    pub name: String,
    pub vector_name: String,
    pub corpus_version: String,
}

#[derive(Debug, Clone)]
pub struct HybridCollectionSettings {
    pub dense_vector_name: String,
    pub sparse_vector_name: String,
    pub corpus_version: String,
    pub sparse: SparseSettings,
}

#[derive(Debug, Clone)]
pub struct SparseSettings {
    pub tokenizer: TokenizerSettings,
    pub preprocessing: SparsePreprocessingSettings,
    pub strategy: SparseStrategySettings,
}

#[derive(Debug, Clone)]
pub struct TokenizerSettings {
    pub library: String,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct SparsePreprocessingSettings {
    pub kind: String,
    pub lowercase: bool,
    pub min_token_length: usize,
}

#[derive(Debug, Clone)]
pub enum SparseStrategySettings {
    BagOfWords(BagOfWordsSettings),
    Bm25Like(Bm25LikeSettings),
}

#[derive(Debug, Clone)]
pub struct BagOfWordsSettings {
    pub name: String,
    pub query: String,
    pub sparse_vocabulary_path: String,
}

#[derive(Debug, Clone)]
pub struct Bm25LikeSettings {
    pub name: String,
    pub query: String,
    pub sparse_vocabulary_path: String,
    pub bm25_term_stats_path: String,
    pub k1: f32,
    pub b: f32,
    pub idf_smoothing: f32,
}

#[derive(Debug, Clone)]
pub struct ModelSettings {
    pub transport: ModelTransportSettings,
}

#[derive(Debug, Clone)]
pub enum ModelTransportSettings {
    Ollama(OllamaModelSettings),
    Together(TogetherModelSettings),
}

#[derive(Debug, Clone)]
pub struct OllamaModelSettings {
    pub url: String,
    pub model_name: String,
    pub timeout_sec: u64,
    pub retry: RetryPolicyConfig,
}

#[derive(Debug, Clone)]
pub struct TogetherModelSettings {
    pub url: String,
    pub api_key: String,
    pub model_name: String,
    pub timeout_sec: u64,
    pub retry: RetryPolicyConfig,
}
