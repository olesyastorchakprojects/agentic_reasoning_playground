use crate::shared_types::IncidentChunkTag;
use crate::utils::retry::RetryPolicyConfig;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub runtime: RuntimeSettings,
    pub input_normalization: InputNormalizationSettings,
    pub query_structuring: QueryStructuringSettings,
    pub llm_structured_generation: LlmStructuredGenerationSettings,
    pub prompt_context: PromptContextSettings,
    pub retrieval: RetrievalSettings,
    pub model: ModelSettings,
    pub embedding_model: EmbeddingModelSettings,
    pub observability: ObservabilitySettings,
    pub postgres: PostgresSettings,
    pub chunk_audit_log_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PromptContextSettings {
    pub prompt_asset_path: String,
    pub chunk_packing: ChunkPackingSettings,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChunkPackingSettings {
    pub evidence_for_match: ChunkRolePackingSettings,
    pub first_check_hint: ChunkRolePackingSettings,
    pub supporting_explanation: ChunkRolePackingSettings,
    pub alternative_context: ChunkRolePackingSettings,
    pub mechanism_explanation: ChunkRolePackingSettings,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChunkRolePackingSettings {
    pub source: ChunkPackingSource,
    pub limit: usize,
    pub per_case_limit: Option<usize>,
    pub fallback_to_any_chunk: bool,
    pub tag_priority: Vec<IncidentChunkTag>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkPackingSource {
    PrimaryIncident,
    AlternativeIncident,
    Theory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputNormalizationSettings {
    pub max_input_tokens: usize,
    pub tokenizer_source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryStructuringSettings {
    pub controlled_vocabulary_path: String,
    pub prompt_asset_path: String,
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSettings {
    pub config_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingModelSettings {
    pub url: String,
    pub name: String,
    pub dimension: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservabilitySettings {
    pub tracing_enabled: bool,
    pub metrics_enabled: bool,
    pub tracing_endpoint: String,
    pub metrics_endpoint: String,
    pub trace_batch_scheduled_delay_ms: u64,
    pub metrics_export_interval_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresSettings {
    pub url: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalSettings {
    pub qdrant_url: String,
    pub cards: CollectionRetrievalSettings,
    pub practice: CollectionRetrievalSettings,
    pub theory: CollectionRetrievalSettings,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CollectionRetrievalSettings {
    pub top_k: usize,
    pub score_threshold: f32,
    pub max_alternatives: usize,
    pub embedding_retry: RetryPolicyConfig,
    pub qdrant_retry: RetryPolicyConfig,
    pub collection: CollectionSettings,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CollectionSettings {
    Dense(DenseCollectionSettings),
    Hybrid(HybridCollectionSettings),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DenseCollectionSettings {
    pub name: String,
    pub vector_name: String,
    pub corpus_version: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HybridCollectionSettings {
    pub dense_vector_name: String,
    pub sparse_vector_name: String,
    pub corpus_version: String,
    pub sparse: SparseSettings,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SparseSettings {
    pub tokenizer: TokenizerSettings,
    pub preprocessing: SparsePreprocessingSettings,
    pub strategy: SparseStrategySettings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenizerSettings {
    pub library: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SparsePreprocessingSettings {
    pub kind: String,
    pub lowercase: bool,
    pub min_token_length: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SparseStrategySettings {
    BagOfWords(BagOfWordsSettings),
    Bm25Like(Bm25LikeSettings),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BagOfWordsSettings {
    pub name: String,
    pub query: String,
    pub sparse_vocabulary_path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bm25LikeSettings {
    pub name: String,
    pub query: String,
    pub sparse_vocabulary_path: String,
    pub bm25_term_stats_path: String,
    pub k1: f32,
    pub b: f32,
    pub idf_smoothing: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmStructuredGenerationSettings {
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSettings {
    pub transport: ModelTransportSettings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelTransportSettings {
    Ollama(OllamaModelSettings),
    Together(TogetherModelSettings),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OllamaModelSettings {
    pub url: String,
    pub model_name: String,
    pub timeout_sec: u64,
    pub retry: RetryPolicyConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TogetherModelSettings {
    pub url: String,
    pub api_key: String,
    pub model_name: String,
    pub timeout_sec: u64,
    pub retry: RetryPolicyConfig,
}
