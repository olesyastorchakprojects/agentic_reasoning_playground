use crate::shared_types::{IncidentChunkTag, RunConfigSnapshot};
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

#[derive(Debug, Clone, PartialEq)]
pub struct ModelSettings {
    pub transport: ModelTransportSettings,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModelTransportSettings {
    Ollama(OllamaModelSettings),
    Together(TogetherModelSettings),
}

#[derive(Debug, Clone, PartialEq)]
pub struct OllamaModelSettings {
    pub url: String,
    pub model_name: String,
    pub timeout_sec: u64,
    pub retry: RetryPolicyConfig,
    pub input_cost_per_million_tokens: f64,
    pub output_cost_per_million_tokens: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TogetherModelSettings {
    pub url: String,
    pub api_key: String,
    pub model_name: String,
    pub timeout_sec: u64,
    pub retry: RetryPolicyConfig,
    pub input_cost_per_million_tokens: f64,
    pub output_cost_per_million_tokens: f64,
}

impl ModelTransportSettings {
    pub fn active_model_name(&self) -> &str {
        match self {
            ModelTransportSettings::Ollama(s) => &s.model_name,
            ModelTransportSettings::Together(s) => &s.model_name,
        }
    }

    pub fn transport_kind(&self) -> &'static str {
        match self {
            ModelTransportSettings::Ollama(_) => "ollama",
            ModelTransportSettings::Together(_) => "together",
        }
    }

    pub fn cost_per_million_tokens(&self) -> (f64, f64) {
        match self {
            ModelTransportSettings::Ollama(s) => {
                (s.input_cost_per_million_tokens, s.output_cost_per_million_tokens)
            }
            ModelTransportSettings::Together(s) => {
                (s.input_cost_per_million_tokens, s.output_cost_per_million_tokens)
            }
        }
    }
}

impl CollectionSettings {
    pub fn collection_name(&self) -> &str {
        match self {
            CollectionSettings::Dense(d) => &d.name,
            CollectionSettings::Hybrid(_) => "hybrid",
        }
    }
}

impl Settings {
    pub fn build_run_config_snapshot(&self) -> RunConfigSnapshot {
        let (input_cost, output_cost) = self.model.transport.cost_per_million_tokens();
        RunConfigSnapshot {
            model_name: self.model.transport.active_model_name().to_string(),
            transport_kind: self.model.transport.transport_kind().to_string(),
            input_cost_per_million_tokens: input_cost,
            output_cost_per_million_tokens: output_cost,
            retrieval_cards_top_k: self.retrieval.cards.top_k,
            retrieval_cards_collection: self.retrieval.cards.collection.collection_name().to_string(),
            retrieval_practice_top_k: self.retrieval.practice.top_k,
            retrieval_practice_collection: self.retrieval.practice.collection.collection_name().to_string(),
            retrieval_theory_top_k: self.retrieval.theory.top_k,
            retrieval_theory_collection: self.retrieval.theory.collection.collection_name().to_string(),
        }
    }
}
