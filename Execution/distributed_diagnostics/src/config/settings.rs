use std::collections::BTreeMap;
use std::fs;

use crate::shared_types::{IncidentChunkTag, RunConfigSnapshot, RuntimeLlmStageConfigSnapshot};
use crate::utils::retry::RetryPolicyConfig;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub runtime: RuntimeSettings,
    pub retrieval: RetrievalSettings,
    pub input_normalization: InputNormalizationSettings,
    pub query_structuring: QueryStructuringRuntimeSettings,
    pub llm_structured_generation: LlmStructuredGenerationRuntimeSettings,
    pub observation_boundary_resolver: ObservationBoundaryResolverRuntimeSettings,
    pub observation_extraction: ObservationExtractionRuntimeSettings,
    pub incident_evidence_retrieval: IncidentEvidenceRetrievalSettings,
    pub prompt_context: PromptContextSettings,
    pub diagnostic_update_prompt_context: DiagnosticUpdatePromptContextSettings,
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
pub struct QueryStructuringRuntimeSettings {
    pub provider: String,
    pub model: String,
    pub controlled_vocabulary_path: String,
    pub prompt_asset_path: String,
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationBoundaryResolverRuntimeSettings {
    pub provider: String,
    pub model: String,
    pub prompt_asset_path: String,
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationExtractionRuntimeSettings {
    pub provider: String,
    pub model: String,
    pub prompt_asset_path: String,
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryStructuringSettings {
    pub controlled_vocabulary_path: String,
    pub prompt_asset_path: String,
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IncidentEvidenceRetrievalSettings {
    pub retrieval: CollectionRetrievalSettings,
    pub profiles: IncidentEvidenceRetrievalProfiles,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IncidentEvidenceRetrievalProfiles {
    pub initial: IncidentEvidenceTagProfile,
    pub continuation: IncidentEvidenceTagProfile,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IncidentEvidenceTagProfile {
    pub primary_tags: Vec<IncidentChunkTag>,
    pub alternative_tags: Vec<IncidentChunkTag>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiagnosticUpdatePromptContextSettings {
    pub prompt_asset_path: String,
    pub chunk_packing: DiagnosticUpdateChunkPackingSettings,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiagnosticUpdateChunkPackingSettings {
    pub evidence_for_match: ChunkRolePackingSettings,
    pub next_check_hint: ChunkRolePackingSettings,
    pub supporting_explanation: ChunkRolePackingSettings,
    pub alternative_context: ChunkRolePackingSettings,
    pub mechanism_explanation: ChunkRolePackingSettings,
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
pub struct LlmStructuredGenerationRuntimeSettings {
    pub provider: String,
    pub model: String,
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmStructuredGenerationSettings {
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelSettings {
    pub ollama: OllamaProviderSettings,
    pub together: TogetherProviderSettings,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OllamaProviderSettings {
    pub url: String,
    pub timeout_sec: u64,
    pub retry: RetryPolicyConfig,
    pub models: BTreeMap<String, ModelPricingSettings>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TogetherProviderSettings {
    pub url: String,
    pub api_key: String,
    pub timeout_sec: u64,
    pub retry: RetryPolicyConfig,
    pub models: BTreeMap<String, ModelPricingSettings>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelPricingSettings {
    pub input_cost_per_million_tokens: f64,
    pub output_cost_per_million_tokens: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedModelSettings {
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

impl ModelSettings {
    pub fn resolve(&self, provider: &str, model_name: &str) -> Option<ResolvedModelSettings> {
        match provider {
            "ollama" => self.ollama.models.get(model_name).map(|pricing| {
                ResolvedModelSettings::Ollama(OllamaModelSettings {
                    url: self.ollama.url.clone(),
                    model_name: model_name.to_string(),
                    timeout_sec: self.ollama.timeout_sec,
                    retry: self.ollama.retry.clone(),
                    input_cost_per_million_tokens: pricing.input_cost_per_million_tokens,
                    output_cost_per_million_tokens: pricing.output_cost_per_million_tokens,
                })
            }),
            "together" => self.together.models.get(model_name).map(|pricing| {
                ResolvedModelSettings::Together(TogetherModelSettings {
                    url: self.together.url.clone(),
                    api_key: self.together.api_key.clone(),
                    model_name: model_name.to_string(),
                    timeout_sec: self.together.timeout_sec,
                    retry: self.together.retry.clone(),
                    input_cost_per_million_tokens: pricing.input_cost_per_million_tokens,
                    output_cost_per_million_tokens: pricing.output_cost_per_million_tokens,
                })
            }),
            _ => None,
        }
    }

    fn resolve_required_stage(
        &self,
        stage_name: &str,
        provider: &str,
        model_name: &str,
    ) -> RuntimeLlmStageConfigSnapshot {
        let resolved = self
            .resolve(provider, model_name)
            .unwrap_or_else(|| panic!("stage {stage_name} has unresolved model binding"));
        match resolved {
            ResolvedModelSettings::Ollama(cfg) => RuntimeLlmStageConfigSnapshot {
                provider: "ollama".to_string(),
                model_name: cfg.model_name,
                input_cost_per_million_tokens: cfg.input_cost_per_million_tokens,
                output_cost_per_million_tokens: cfg.output_cost_per_million_tokens,
            },
            ResolvedModelSettings::Together(cfg) => RuntimeLlmStageConfigSnapshot {
                provider: "together".to_string(),
                model_name: cfg.model_name,
                input_cost_per_million_tokens: cfg.input_cost_per_million_tokens,
                output_cost_per_million_tokens: cfg.output_cost_per_million_tokens,
            },
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
        RunConfigSnapshot {
            query_structuring: self.model.resolve_required_stage(
                "query_structuring",
                &self.query_structuring.provider,
                &self.query_structuring.model,
            ),
            observation_boundary_resolver: self.model.resolve_required_stage(
                "observation_boundary_resolver",
                &self.observation_boundary_resolver.provider,
                &self.observation_boundary_resolver.model,
            ),
            observation_extraction: self.model.resolve_required_stage(
                "observation_extraction",
                &self.observation_extraction.provider,
                &self.observation_extraction.model,
            ),
            llm_structured_generation: self.model.resolve_required_stage(
                "llm_structured_generation",
                &self.llm_structured_generation.provider,
                &self.llm_structured_generation.model,
            ),
            query_structuring_prompt_version: load_prompt_version(
                "query_structuring.prompt_asset_path",
                &self.query_structuring.prompt_asset_path,
            ),
            observation_boundary_resolver_prompt_version: load_prompt_version(
                "observation_boundary_resolver.prompt_asset_path",
                &self.observation_boundary_resolver.prompt_asset_path,
            ),
            observation_extraction_prompt_version: load_prompt_version(
                "observation_extraction.prompt_asset_path",
                &self.observation_extraction.prompt_asset_path,
            ),
            prompt_context_prompt_version: load_prompt_version(
                "prompt_context.prompt_asset_path",
                &self.prompt_context.prompt_asset_path,
            ),
            diagnostic_update_prompt_context_prompt_version: load_prompt_version(
                "diagnostic_update_prompt_context.prompt_asset_path",
                &self.diagnostic_update_prompt_context.prompt_asset_path,
            ),
            retrieval_cards_top_k: self.retrieval.cards.top_k,
            retrieval_cards_collection: self.retrieval.cards.collection.collection_name().to_string(),
            retrieval_practice_top_k: self.retrieval.practice.top_k,
            retrieval_practice_collection: self.retrieval.practice.collection.collection_name().to_string(),
            retrieval_theory_top_k: self.retrieval.theory.top_k,
            retrieval_theory_collection: self.retrieval.theory.collection.collection_name().to_string(),
        }
    }
}

fn load_prompt_version(field_name: &str, path: &str) -> String {
    let body = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{field_name}: failed to read prompt asset at '{path}': {e}"));
    let json: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|e| {
        panic!("{field_name}: failed to parse prompt asset at '{path}' as json: {e}")
    });
    json.get("version")
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| panic!("{field_name}: prompt asset at '{path}' has no non-empty version"))
        .to_string()
}
