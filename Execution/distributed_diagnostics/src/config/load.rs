use std::collections::BTreeMap;
use std::path::Path;
use std::str::FromStr;

use serde::Deserialize;

use crate::shared_types::IncidentChunkTag;
use crate::utils::retry::{RetryBackoffKind, RetryPolicyConfig};

use super::{
    BagOfWordsSettings, Bm25LikeSettings, ChunkPackingSettings, ChunkPackingSource,
    ChunkRolePackingSettings, CollectionRetrievalSettings, CollectionSettings, ConfigError,
    DenseCollectionSettings, DiagnosticUpdateChunkPackingSettings,
    DiagnosticUpdatePromptContextSettings, EmbeddingModelSettings, HybridCollectionSettings,
    IncidentEvidenceRetrievalProfiles, IncidentEvidenceRetrievalSettings,
    IncidentEvidenceTagProfile, InputNormalizationSettings, ModelPricingSettings,
    ModelSettings, ObservabilitySettings,
    ObservationBoundaryResolverRuntimeSettings, ObservationExtractionRuntimeSettings,
    OllamaProviderSettings, PostgresSettings, PromptContextSettings,
    QueryStructuringRuntimeSettings, RetrievalSettings, RuntimeSettings, Settings,
    SparsePreprocessingSettings, SparseSettings, SparseStrategySettings,
    TogetherProviderSettings, TokenizerSettings, LlmStructuredGenerationRuntimeSettings,
};


// ---------------------------------------------------------------------------
// Raw intermediate structs mirroring the merged TOML structure
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawConfig {
    runtime: RawRuntime,
    input_normalization: RawInputNormalization,
    query_structuring: RawQueryStructuring,
    llm_structured_generation: RawLlmStructuredGeneration,
    observation_boundary_resolver: RawObservationBoundaryResolver,
    observation_extraction: RawObservationExtraction,
    incident_evidence_retrieval: RawIncidentEvidenceRetrieval,
    prompt_context: RawPromptContext,
    diagnostic_update_prompt_context: RawDiagnosticUpdatePromptContext,
    retrieval: RawRetrieval,
    model: RawModel,
    embedding: RawEmbedding,
    qdrant: RawQdrant,
    observability: RawObservability,
    #[serde(default)]
    chunk_audit_log: Option<RawChunkAuditLog>,
}

#[derive(Debug, Deserialize)]
struct RawChunkAuditLog {
    path: String,
}

#[derive(Debug, Deserialize)]
struct RawObservationBoundaryResolver {
    provider: String,
    model: String,
    prompt_asset_path: String,
    max_output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct RawObservationExtraction {
    provider: String,
    model: String,
    prompt_asset_path: String,
    max_output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct RawIncidentEvidenceRetrieval {
    retrieval: RawCollectionRetrieval,
    profiles: RawIncidentEvidenceRetrievalProfiles,
}

#[derive(Debug, Deserialize)]
struct RawIncidentEvidenceRetrievalProfiles {
    initial: RawIncidentEvidenceTagProfile,
    continuation: RawIncidentEvidenceTagProfile,
}

#[derive(Debug, Deserialize)]
struct RawIncidentEvidenceTagProfile {
    primary_tags: Vec<String>,
    alternative_tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawDiagnosticUpdatePromptContext {
    prompt_asset_path: String,
    chunk_packing: RawDiagnosticUpdateChunkPacking,
}

#[derive(Debug, Deserialize)]
struct RawDiagnosticUpdateChunkPacking {
    evidence_for_match: RawChunkRolePacking,
    next_check_hint: RawChunkRolePacking,
    supporting_explanation: RawChunkRolePacking,
    alternative_context: RawChunkRolePacking,
    mechanism_explanation: RawChunkRolePacking,
}

#[derive(Debug, Deserialize)]
struct RawLlmStructuredGeneration {
    provider: String,
    model: String,
    max_output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct RawInputNormalization {
    max_input_tokens: usize,
    tokenizer_source: String,
}

#[derive(Debug, Deserialize)]
struct RawQueryStructuring {
    provider: String,
    model: String,
    controlled_vocabulary_path: String,
    prompt_asset_path: String,
    max_output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct RawRuntime {
    config_version: String,
}

#[derive(Debug, Deserialize)]
struct RawRetrieval {
    cards: RawCollectionRetrieval,
    practice: RawCollectionRetrieval,
    theory: RawCollectionRetrieval,
}

#[derive(Debug, Deserialize)]
struct RawCollectionRetrieval {
    top_k: usize,
    score_threshold: f32,
    max_alternatives: usize,
    embedding_retry: RawRetryPolicy,
    qdrant_retry: RawRetryPolicy,
}

#[derive(Debug, Deserialize)]
struct RawRetryPolicy {
    max_attempts: u32,
    backoff: String,
}

#[derive(Debug, Deserialize)]
struct RawModel {
    ollama: RawOllamaProvider,
    together: RawTogetherProvider,
}

#[derive(Debug, Deserialize)]
struct RawOllamaProvider {
    timeout_sec: u64,
    retry: RawRetryPolicy,
    models: BTreeMap<String, RawModelPricing>,
}

#[derive(Debug, Deserialize)]
struct RawTogetherProvider {
    timeout_sec: u64,
    retry: RawRetryPolicy,
    models: BTreeMap<String, RawModelPricing>,
}

#[derive(Debug, Deserialize)]
struct RawModelPricing {
    input_cost_per_million_tokens: f64,
    output_cost_per_million_tokens: f64,
}

#[derive(Debug, Deserialize)]
struct RawEmbedding {
    model: RawEmbeddingModel,
}

#[derive(Debug, Deserialize)]
struct RawEmbeddingModel {
    name: String,
    dimension: usize,
}

#[derive(Debug, Deserialize)]
struct RawQdrant {
    collections: RawCollections,
}

#[derive(Debug, Deserialize)]
struct RawCollections {
    cards: RawCollection,
    practice: RawCollection,
    theory: RawCollection,
}

#[derive(Debug, Deserialize)]
struct RawCollection {
    kind: String,
    corpus_version: String,
    dense: Option<RawDenseCollection>,
    hybrid: Option<RawHybridCollection>,
}

#[derive(Debug, Deserialize)]
struct RawDenseCollection {
    name: String,
    vector_name: String,
}

#[derive(Debug, Deserialize)]
struct RawHybridCollection {
    dense_vector_name: String,
    sparse_vector_name: String,
    sparse: RawSparse,
}

#[derive(Debug, Deserialize)]
struct RawSparse {
    strategy: RawSparseStrategy,
    tokenizer: RawTokenizer,
    preprocessing: RawPreprocessing,
    bag_of_words: Option<RawBagOfWords>,
    bm25_like: Option<RawBm25Like>,
}

#[derive(Debug, Deserialize)]
struct RawSparseStrategy {
    kind: String,
}

#[derive(Debug, Deserialize)]
struct RawTokenizer {
    library: String,
    source: String,
}

#[derive(Debug, Deserialize)]
struct RawPreprocessing {
    kind: String,
    lowercase: bool,
    min_token_length: usize,
}

#[derive(Debug, Deserialize)]
struct RawBagOfWords {
    name: String,
    query: String,
    sparse_vocabulary_path: String,
}

#[derive(Debug, Deserialize)]
struct RawBm25Like {
    name: String,
    query: String,
    sparse_vocabulary_path: String,
    bm25_term_stats_path: String,
    k1: f32,
    b: f32,
    idf_smoothing: f32,
}

#[derive(Debug, Deserialize)]
struct RawObservability {
    tracing_enabled: bool,
    metrics_enabled: bool,
    trace_batch_scheduled_delay_ms: u64,
    metrics_export_interval_ms: u64,
}

#[derive(Debug, Deserialize)]
struct RawPromptContext {
    prompt_asset_path: String,
    chunk_packing: RawChunkPacking,
}

#[derive(Debug, Deserialize)]
struct RawChunkPacking {
    evidence_for_match: RawChunkRolePacking,
    first_check_hint: RawChunkRolePacking,
    supporting_explanation: RawChunkRolePacking,
    alternative_context: RawChunkRolePacking,
    mechanism_explanation: RawChunkRolePacking,
}

#[derive(Debug, Deserialize)]
struct RawChunkRolePacking {
    source: String,
    limit: usize,
    #[serde(default)]
    per_case_limit: Option<usize>,
    fallback_to_any_chunk: bool,
    tag_priority: Vec<String>,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn load(runtime_path: &Path, ingest_path: &Path) -> Result<Settings, ConfigError> {
    let _ = dotenvy::dotenv();
    load_inner(runtime_path, ingest_path, &|key| std::env::var(key).ok())
}

fn load_inner(
    runtime_path: &Path,
    ingest_path: &Path,
    env_fn: &impl Fn(&str) -> Option<String>,
) -> Result<Settings, ConfigError> {
    let raw: RawConfig = config::Config::builder()
        .add_source(config::File::from(runtime_path).format(config::FileFormat::Toml))
        .add_source(config::File::from(ingest_path).format(config::FileFormat::Toml))
        .build()
        .map_err(|e| ConfigError::Load(e.to_string()))?
        .try_deserialize()
        .map_err(|e| ConfigError::Load(e.to_string()))?;

    // Resolve discriminators before reading env vars so missing/invalid
    // discriminator values fail early with a typed error.
    let cards =
        resolve_collection_retrieval(&raw.retrieval.cards, &raw.qdrant.collections.cards, "cards")?;
    let practice = resolve_collection_retrieval(
        &raw.retrieval.practice,
        &raw.qdrant.collections.practice,
        "practice",
    )?;
    let theory = resolve_collection_retrieval(
        &raw.retrieval.theory,
        &raw.qdrant.collections.theory,
        "theory",
    )?;

    let prompt_context = resolve_prompt_context(&raw.prompt_context)?;
    let diagnostic_update_prompt_context =
        resolve_diagnostic_update_prompt_context(&raw.diagnostic_update_prompt_context)?;
    let incident_evidence_retrieval =
        resolve_incident_evidence_retrieval(&raw.incident_evidence_retrieval, &raw.qdrant)?;

    let ollama_url = require_env_fn(env_fn, "OLLAMA_URL")?;
    let qdrant_url = require_env_fn(env_fn, "QDRANT_URL")?;
    let postgres_url = require_env_fn(env_fn, "POSTGRES_URL")?;
    let tracing_endpoint = require_env_fn(env_fn, "TRACING_ENDPOINT")?;
    let metrics_endpoint = require_env_fn(env_fn, "METRICS_ENDPOINT")?;

    let model_settings = resolve_model_settings(&raw.model, &ollama_url, env_fn)?;
    validate_stage_model_binding(
        &model_settings,
        "query_structuring",
        &raw.query_structuring.provider,
        &raw.query_structuring.model,
    )?;
    validate_stage_model_binding(
        &model_settings,
        "observation_boundary_resolver",
        &raw.observation_boundary_resolver.provider,
        &raw.observation_boundary_resolver.model,
    )?;
    validate_stage_model_binding(
        &model_settings,
        "observation_extraction",
        &raw.observation_extraction.provider,
        &raw.observation_extraction.model,
    )?;
    validate_stage_model_binding(
        &model_settings,
        "llm_structured_generation",
        &raw.llm_structured_generation.provider,
        &raw.llm_structured_generation.model,
    )?;

    Ok(Settings {
        runtime: RuntimeSettings {
            config_version: raw.runtime.config_version,
        },
        retrieval: RetrievalSettings {
            qdrant_url,
            cards,
            practice,
            theory,
        },
        input_normalization: InputNormalizationSettings {
            max_input_tokens: raw.input_normalization.max_input_tokens,
            tokenizer_source: raw.input_normalization.tokenizer_source,
        },
        query_structuring: QueryStructuringRuntimeSettings {
            provider: raw.query_structuring.provider,
            model: raw.query_structuring.model,
            controlled_vocabulary_path: raw.query_structuring.controlled_vocabulary_path,
            prompt_asset_path: raw.query_structuring.prompt_asset_path,
            max_output_tokens: raw.query_structuring.max_output_tokens,
        },
        llm_structured_generation: LlmStructuredGenerationRuntimeSettings {
            provider: raw.llm_structured_generation.provider,
            model: raw.llm_structured_generation.model,
            max_output_tokens: raw.llm_structured_generation.max_output_tokens,
        },
        observation_boundary_resolver: ObservationBoundaryResolverRuntimeSettings {
            provider: raw.observation_boundary_resolver.provider,
            model: raw.observation_boundary_resolver.model,
            prompt_asset_path: raw.observation_boundary_resolver.prompt_asset_path,
            max_output_tokens: raw.observation_boundary_resolver.max_output_tokens,
        },
        observation_extraction: ObservationExtractionRuntimeSettings {
            provider: raw.observation_extraction.provider,
            model: raw.observation_extraction.model,
            prompt_asset_path: raw.observation_extraction.prompt_asset_path,
            max_output_tokens: raw.observation_extraction.max_output_tokens,
        },
        incident_evidence_retrieval,
        prompt_context,
        diagnostic_update_prompt_context,
        model: model_settings,
        embedding_model: EmbeddingModelSettings {
            url: ollama_url,
            name: raw.embedding.model.name,
            dimension: raw.embedding.model.dimension,
        },
        observability: ObservabilitySettings {
            tracing_enabled: raw.observability.tracing_enabled,
            metrics_enabled: raw.observability.metrics_enabled,
            tracing_endpoint,
            metrics_endpoint,
            trace_batch_scheduled_delay_ms: raw.observability.trace_batch_scheduled_delay_ms,
            metrics_export_interval_ms: raw.observability.metrics_export_interval_ms,
        },
        postgres: PostgresSettings { url: postgres_url },
        chunk_audit_log_path: raw.chunk_audit_log.map(|c| c.path),
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn require_env_fn(
    env_fn: &impl Fn(&str) -> Option<String>,
    key: &str,
) -> Result<String, ConfigError> {
    env_fn(key).ok_or_else(|| ConfigError::MissingEnvironment {
        key: key.to_string(),
    })
}

fn resolve_retry(raw: &RawRetryPolicy, field: &str) -> Result<RetryPolicyConfig, ConfigError> {
    let backoff = match raw.backoff.as_str() {
        "exponential" => RetryBackoffKind::Exponential,
        other => {
            return Err(ConfigError::InvalidValue {
                field: format!("{field}.backoff"),
                reason: format!("unknown backoff kind '{other}'"),
            })
        }
    };
    Ok(RetryPolicyConfig {
        max_attempts: raw.max_attempts,
        backoff,
    })
}

fn resolve_model_settings(
    raw: &RawModel,
    ollama_url: &str,
    env_fn: &impl Fn(&str) -> Option<String>,
) -> Result<ModelSettings, ConfigError> {
    let together_url = require_env_fn(env_fn, "OPENAI_COMPATIBLE_URL")?;
    let together_api_key = require_env_fn(env_fn, "TOGETHER_API_KEY")?;

    Ok(ModelSettings {
        ollama: OllamaProviderSettings {
            url: ollama_url.to_string(),
            timeout_sec: raw.ollama.timeout_sec,
            retry: resolve_retry(&raw.ollama.retry, "model.ollama.retry")?,
            models: resolve_model_catalog(&raw.ollama.models, "model.ollama.models")?,
        },
        together: TogetherProviderSettings {
            url: together_url,
            api_key: together_api_key,
            timeout_sec: raw.together.timeout_sec,
            retry: resolve_retry(&raw.together.retry, "model.together.retry")?,
            models: resolve_model_catalog(&raw.together.models, "model.together.models")?,
        },
    })
}

fn resolve_model_catalog(
    raw: &BTreeMap<String, RawModelPricing>,
    field_prefix: &str,
) -> Result<BTreeMap<String, ModelPricingSettings>, ConfigError> {
    let mut resolved = BTreeMap::new();
    for (model_name, pricing) in raw {
        if model_name.trim().is_empty() {
            return Err(ConfigError::InvalidValue {
                field: field_prefix.to_string(),
                reason: "model id must not be empty".to_string(),
            });
        }
        if pricing.input_cost_per_million_tokens < 0.0 {
            return Err(ConfigError::InvalidValue {
                field: format!("{field_prefix}.{model_name}.input_cost_per_million_tokens"),
                reason: "must be >= 0".to_string(),
            });
        }
        if pricing.output_cost_per_million_tokens < 0.0 {
            return Err(ConfigError::InvalidValue {
                field: format!("{field_prefix}.{model_name}.output_cost_per_million_tokens"),
                reason: "must be >= 0".to_string(),
            });
        }
        resolved.insert(
            model_name.clone(),
            ModelPricingSettings {
                input_cost_per_million_tokens: pricing.input_cost_per_million_tokens,
                output_cost_per_million_tokens: pricing.output_cost_per_million_tokens,
            },
        );
    }
    Ok(resolved)
}

fn validate_stage_model_binding(
    model_settings: &ModelSettings,
    stage_name: &str,
    provider: &str,
    model_name: &str,
) -> Result<(), ConfigError> {
    if provider.trim().is_empty() {
        return Err(ConfigError::InvalidValue {
            field: format!("{stage_name}.provider"),
            reason: "must not be empty".to_string(),
        });
    }
    if model_name.trim().is_empty() {
        return Err(ConfigError::InvalidValue {
            field: format!("{stage_name}.model"),
            reason: "must not be empty".to_string(),
        });
    }
    if model_settings.resolve(provider, model_name).is_none() {
        return Err(ConfigError::InvalidValue {
            field: format!("{stage_name}.model"),
            reason: format!(
                "model '{model_name}' is not declared in provider catalog '{provider}'"
            ),
        });
    }
    Ok(())
}

fn resolve_collection_retrieval(
    raw_ret: &RawCollectionRetrieval,
    raw_col: &RawCollection,
    name: &str,
) -> Result<CollectionRetrievalSettings, ConfigError> {
    let collection = resolve_collection(raw_col, name)?;
    Ok(CollectionRetrievalSettings {
        top_k: raw_ret.top_k,
        score_threshold: raw_ret.score_threshold,
        max_alternatives: raw_ret.max_alternatives,
        embedding_retry: resolve_retry(
            &raw_ret.embedding_retry,
            &format!("retrieval.{name}.embedding_retry"),
        )?,
        qdrant_retry: resolve_retry(
            &raw_ret.qdrant_retry,
            &format!("retrieval.{name}.qdrant_retry"),
        )?,
        collection,
    })
}

fn resolve_collection(raw: &RawCollection, name: &str) -> Result<CollectionSettings, ConfigError> {
    match raw.kind.as_str() {
        "dense" => {
            let cfg = raw.dense.as_ref().ok_or_else(|| {
                ConfigError::Load(format!(
                    "qdrant.collections.{name}.kind = \"dense\" but [dense] section is missing"
                ))
            })?;
            Ok(CollectionSettings::Dense(DenseCollectionSettings {
                name: cfg.name.clone(),
                vector_name: cfg.vector_name.clone(),
                corpus_version: raw.corpus_version.clone(),
            }))
        }
        "hybrid" => {
            let cfg = raw.hybrid.as_ref().ok_or_else(|| {
                ConfigError::Load(format!(
                    "qdrant.collections.{name}.kind = \"hybrid\" but [hybrid] section is missing"
                ))
            })?;
            let sparse = resolve_sparse(&cfg.sparse, name)?;
            Ok(CollectionSettings::Hybrid(HybridCollectionSettings {
                dense_vector_name: cfg.dense_vector_name.clone(),
                sparse_vector_name: cfg.sparse_vector_name.clone(),
                corpus_version: raw.corpus_version.clone(),
                sparse,
            }))
        }
        other => Err(ConfigError::InvalidValue {
            field: format!("qdrant.collections.{name}.kind"),
            reason: format!("unknown collection kind '{other}'"),
        }),
    }
}

fn resolve_prompt_context(raw: &RawPromptContext) -> Result<PromptContextSettings, ConfigError> {
    Ok(PromptContextSettings {
        prompt_asset_path: raw.prompt_asset_path.clone(),
        chunk_packing: ChunkPackingSettings {
            evidence_for_match: resolve_chunk_role(
                &raw.chunk_packing.evidence_for_match,
                "evidence_for_match",
            )?,
            first_check_hint: resolve_chunk_role(
                &raw.chunk_packing.first_check_hint,
                "first_check_hint",
            )?,
            supporting_explanation: resolve_chunk_role(
                &raw.chunk_packing.supporting_explanation,
                "supporting_explanation",
            )?,
            alternative_context: resolve_chunk_role(
                &raw.chunk_packing.alternative_context,
                "alternative_context",
            )?,
            mechanism_explanation: resolve_chunk_role(
                &raw.chunk_packing.mechanism_explanation,
                "mechanism_explanation",
            )?,
        },
    })
}

fn parse_incident_chunk_tags(
    raw_tags: &[String],
    field_context: &str,
) -> Result<Vec<IncidentChunkTag>, ConfigError> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::with_capacity(raw_tags.len());
    for raw_tag in raw_tags {
        let tag =
            IncidentChunkTag::from_str(raw_tag).map_err(|_| ConfigError::InvalidValue {
                field: field_context.to_string(),
                reason: format!("unknown tag '{raw_tag}'"),
            })?;
        if !seen.insert(tag) {
            return Err(ConfigError::InvalidValue {
                field: field_context.to_string(),
                reason: format!("duplicate tag '{raw_tag}'"),
            });
        }
        result.push(tag);
    }
    Ok(result)
}

fn resolve_incident_evidence_retrieval(
    raw: &RawIncidentEvidenceRetrieval,
    qdrant: &RawQdrant,
) -> Result<IncidentEvidenceRetrievalSettings, ConfigError> {
    let retrieval = resolve_collection_retrieval(
        &raw.retrieval,
        &qdrant.collections.practice,
        "incident_evidence_retrieval",
    )?;

    let initial = IncidentEvidenceTagProfile {
        primary_tags: parse_incident_chunk_tags(
            &raw.profiles.initial.primary_tags,
            "incident_evidence_retrieval.profiles.initial.primary_tags",
        )?,
        alternative_tags: parse_incident_chunk_tags(
            &raw.profiles.initial.alternative_tags,
            "incident_evidence_retrieval.profiles.initial.alternative_tags",
        )?,
    };
    let continuation = IncidentEvidenceTagProfile {
        primary_tags: parse_incident_chunk_tags(
            &raw.profiles.continuation.primary_tags,
            "incident_evidence_retrieval.profiles.continuation.primary_tags",
        )?,
        alternative_tags: parse_incident_chunk_tags(
            &raw.profiles.continuation.alternative_tags,
            "incident_evidence_retrieval.profiles.continuation.alternative_tags",
        )?,
    };

    Ok(IncidentEvidenceRetrievalSettings {
        retrieval,
        profiles: IncidentEvidenceRetrievalProfiles { initial, continuation },
    })
}

fn resolve_diagnostic_update_prompt_context(
    raw: &RawDiagnosticUpdatePromptContext,
) -> Result<DiagnosticUpdatePromptContextSettings, ConfigError> {
    Ok(DiagnosticUpdatePromptContextSettings {
        prompt_asset_path: raw.prompt_asset_path.clone(),
        chunk_packing: DiagnosticUpdateChunkPackingSettings {
            evidence_for_match: resolve_chunk_role(
                &raw.chunk_packing.evidence_for_match,
                "diagnostic_update_prompt_context.chunk_packing.evidence_for_match",
            )?,
            next_check_hint: resolve_chunk_role(
                &raw.chunk_packing.next_check_hint,
                "diagnostic_update_prompt_context.chunk_packing.next_check_hint",
            )?,
            supporting_explanation: resolve_chunk_role(
                &raw.chunk_packing.supporting_explanation,
                "diagnostic_update_prompt_context.chunk_packing.supporting_explanation",
            )?,
            alternative_context: resolve_chunk_role(
                &raw.chunk_packing.alternative_context,
                "diagnostic_update_prompt_context.chunk_packing.alternative_context",
            )?,
            mechanism_explanation: resolve_chunk_role(
                &raw.chunk_packing.mechanism_explanation,
                "diagnostic_update_prompt_context.chunk_packing.mechanism_explanation",
            )?,
        },
    })
}

fn resolve_chunk_role(
    raw: &RawChunkRolePacking,
    role_name: &str,
) -> Result<ChunkRolePackingSettings, ConfigError> {
    let source = match raw.source.as_str() {
        "primary_incident" => ChunkPackingSource::PrimaryIncident,
        "alternative_incident" => ChunkPackingSource::AlternativeIncident,
        "theory" => ChunkPackingSource::Theory,
        other => {
            return Err(ConfigError::InvalidValue {
                field: format!("prompt_context.chunk_packing.{role_name}.source"),
                reason: format!("unknown source '{other}'"),
            })
        }
    };

    let mut seen_tags = std::collections::HashSet::new();
    let mut tag_priority = Vec::with_capacity(raw.tag_priority.len());
    for raw_tag in &raw.tag_priority {
        let tag = IncidentChunkTag::from_str(raw_tag).map_err(|_| ConfigError::InvalidValue {
            field: format!("prompt_context.chunk_packing.{role_name}.tag_priority"),
            reason: format!("unknown tag '{raw_tag}'"),
        })?;
        if !seen_tags.insert(tag) {
            return Err(ConfigError::InvalidValue {
                field: format!("prompt_context.chunk_packing.{role_name}.tag_priority"),
                reason: format!("duplicate tag '{raw_tag}'"),
            });
        }
        tag_priority.push(tag);
    }

    Ok(ChunkRolePackingSettings {
        source,
        limit: raw.limit,
        per_case_limit: raw.per_case_limit,
        fallback_to_any_chunk: raw.fallback_to_any_chunk,
        tag_priority,
    })
}

fn resolve_sparse(raw: &RawSparse, col_name: &str) -> Result<SparseSettings, ConfigError> {
    let strategy = match raw.strategy.kind.as_str() {
        "bag_of_words" => {
            let cfg = raw.bag_of_words.as_ref().ok_or_else(|| {
                ConfigError::Load(format!(
                    "qdrant.collections.{col_name}.hybrid.sparse.strategy.kind = \"bag_of_words\" \
                     but [bag_of_words] section is missing"
                ))
            })?;
            SparseStrategySettings::BagOfWords(BagOfWordsSettings {
                name: cfg.name.clone(),
                query: cfg.query.clone(),
                sparse_vocabulary_path: cfg.sparse_vocabulary_path.clone(),
            })
        }
        "bm25_like" => {
            let cfg = raw.bm25_like.as_ref().ok_or_else(|| {
                ConfigError::Load(format!(
                    "qdrant.collections.{col_name}.hybrid.sparse.strategy.kind = \"bm25_like\" \
                     but [bm25_like] section is missing"
                ))
            })?;
            SparseStrategySettings::Bm25Like(Bm25LikeSettings {
                name: cfg.name.clone(),
                query: cfg.query.clone(),
                sparse_vocabulary_path: cfg.sparse_vocabulary_path.clone(),
                bm25_term_stats_path: cfg.bm25_term_stats_path.clone(),
                k1: cfg.k1,
                b: cfg.b,
                idf_smoothing: cfg.idf_smoothing,
            })
        }
        other => {
            return Err(ConfigError::InvalidValue {
                field: format!("qdrant.collections.{col_name}.hybrid.sparse.strategy.kind"),
                reason: format!("unknown sparse strategy kind '{other}'"),
            })
        }
    };

    Ok(SparseSettings {
        tokenizer: TokenizerSettings {
            library: raw.tokenizer.library.clone(),
            source: raw.tokenizer.source.clone(),
        },
        preprocessing: SparsePreprocessingSettings {
            kind: raw.preprocessing.kind.clone(),
            lowercase: raw.preprocessing.lowercase,
            min_token_length: raw.preprocessing.min_token_length,
        },
        strategy,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    // Builds the standard env map used by most tests.
    fn default_env() -> HashMap<&'static str, &'static str> {
        let mut m = HashMap::new();
        m.insert("OLLAMA_URL", "http://ollama-test:11434");
        m.insert("OPENAI_COMPATIBLE_URL", "https://api.together.xyz/v1");
        m.insert("TOGETHER_API_KEY", "test-key");
        m.insert("QDRANT_URL", "http://qdrant-test:6333");
        m.insert("POSTGRES_URL", "postgres://pg-test/db");
        m.insert("TRACING_ENDPOINT", "http://otel-test:4317");
        m.insert("METRICS_ENDPOINT", "http://otel-test:4318");
        m
    }

    fn env_fn<'a>(map: &'a HashMap<&'a str, &'a str>) -> impl Fn(&str) -> Option<String> + 'a {
        move |key: &str| map.get(key).map(|v| v.to_string())
    }

    fn load_test(
        rt: &NamedTempFile,
        ing: &NamedTempFile,
        env: &HashMap<&str, &str>,
    ) -> Result<Settings, ConfigError> {
        load_inner(rt.path(), ing.path(), &env_fn(env))
    }

    const RUNTIME_TOML: &str = r#"
[runtime]
config_version = "v1"

[input_normalization]
max_input_tokens = 3000
tokenizer_source = "Qwen/Qwen3-Embedding-0.6B"

[query_structuring]
provider = "ollama"
model = "qwen2.5:1.5b-instruct"
controlled_vocabulary_path = "Specification/runtime/request_pipeline/query_structuring_controlled_vocabulary.manual_test.json"
prompt_asset_path = "Specification/runtime/request_pipeline/query_structuring_prompt_baseline_v2.manual_test.json"
max_output_tokens = 2200

[llm_structured_generation]
provider = "ollama"
model = "qwen2.5:1.5b-instruct"
max_output_tokens = 1200

[observation_boundary_resolver]
provider = "together"
model = "openai/gpt-oss-20b"
prompt_asset_path = "/tmp/obr_prompt.json"
max_output_tokens = 600

[observation_extraction]
provider = "together"
model = "openai/gpt-oss-20b"
prompt_asset_path = "/tmp/oe_prompt.json"
max_output_tokens = 600

[incident_evidence_retrieval.retrieval]
top_k = 12
score_threshold = 0.2
max_alternatives = 2

[incident_evidence_retrieval.retrieval.embedding_retry]
max_attempts = 3
backoff = "exponential"

[incident_evidence_retrieval.retrieval.qdrant_retry]
max_attempts = 3
backoff = "exponential"

[incident_evidence_retrieval.profiles.initial]
primary_tags = ["chunk_role:symptom", "chunk_role:impact"]
alternative_tags = ["chunk_role:failure_mode"]

[incident_evidence_retrieval.profiles.continuation]
primary_tags = ["chunk_role:symptom_change", "chunk_role:investigation"]
alternative_tags = ["chunk_role:uncertainty"]

[prompt_context]
prompt_asset_path = "Specification/runtime/request_pipeline/prompt_context_assembly/diagnostic_response_prompt_baseline.manual_test.json"

[prompt_context.chunk_packing.evidence_for_match]
source = "primary_incident"
limit = 1
fallback_to_any_chunk = true
tag_priority = ["chunk_role:symptom", "chunk_role:failure_mode"]

[prompt_context.chunk_packing.first_check_hint]
source = "primary_incident"
limit = 1
fallback_to_any_chunk = true
tag_priority = ["chunk_role:diagnostic_step"]

[prompt_context.chunk_packing.supporting_explanation]
source = "primary_incident"
limit = 1
fallback_to_any_chunk = false
tag_priority = ["chunk_role:contributing_factor"]

[prompt_context.chunk_packing.alternative_context]
source = "alternative_incident"
limit = 2
per_case_limit = 1
fallback_to_any_chunk = false
tag_priority = ["chunk_role:failure_mode"]

[prompt_context.chunk_packing.mechanism_explanation]
source = "theory"
limit = 1
fallback_to_any_chunk = false
tag_priority = []

[diagnostic_update_prompt_context]
prompt_asset_path = "/tmp/diagnostic_update_prompt.json"

[diagnostic_update_prompt_context.chunk_packing.evidence_for_match]
source = "primary_incident"
limit = 1
fallback_to_any_chunk = true
tag_priority = ["chunk_role:symptom"]

[diagnostic_update_prompt_context.chunk_packing.next_check_hint]
source = "primary_incident"
limit = 1
fallback_to_any_chunk = true
tag_priority = ["chunk_role:diagnostic_step"]

[diagnostic_update_prompt_context.chunk_packing.supporting_explanation]
source = "primary_incident"
limit = 1
fallback_to_any_chunk = false
tag_priority = []

[diagnostic_update_prompt_context.chunk_packing.alternative_context]
source = "alternative_incident"
limit = 2
per_case_limit = 1
fallback_to_any_chunk = false
tag_priority = []

[diagnostic_update_prompt_context.chunk_packing.mechanism_explanation]
source = "theory"
limit = 1
fallback_to_any_chunk = false
tag_priority = []

[retrieval.cards]
top_k = 8
score_threshold = 0.2
max_alternatives = 2

[retrieval.cards.embedding_retry]
max_attempts = 3
backoff = "exponential"

[retrieval.cards.qdrant_retry]
max_attempts = 3
backoff = "exponential"

[retrieval.practice]
top_k = 12
score_threshold = 0.2
max_alternatives = 2

[retrieval.practice.embedding_retry]
max_attempts = 3
backoff = "exponential"

[retrieval.practice.qdrant_retry]
max_attempts = 3
backoff = "exponential"

[retrieval.theory]
top_k = 12
score_threshold = 0.2
max_alternatives = 2

[retrieval.theory.embedding_retry]
max_attempts = 3
backoff = "exponential"

[retrieval.theory.qdrant_retry]
max_attempts = 3
backoff = "exponential"

[model.ollama]
timeout_sec = 120

[model.ollama.retry]
max_attempts = 3
backoff = "exponential"

[model.ollama.models."qwen2.5:1.5b-instruct"]
input_cost_per_million_tokens = 0.0
output_cost_per_million_tokens = 0.0

[model.together]
timeout_sec = 120

[model.together.retry]
max_attempts = 3
backoff = "exponential"

[model.together.models."openai/gpt-oss-20b"]
input_cost_per_million_tokens = 0.05
output_cost_per_million_tokens = 0.20

[observability]
tracing_enabled = true
metrics_enabled = true
trace_batch_scheduled_delay_ms = 5000
metrics_export_interval_ms = 60000
"#;

    const INGEST_TOML_HYBRID: &str = r#"
[pipeline]
ingest_config_version = "v1"

[embedding.model]
name = "qwen3-embedding:0.6b"
dimension = 1024

[qdrant.collections.cards]
kind = "hybrid"
corpus_version = "v1"

[qdrant.collections.cards.dense]
name = "cards_dense"
vector_name = "dense"

[qdrant.collections.cards.hybrid]
dense_vector_name = "dense"
sparse_vector_name = "sparse"

[qdrant.collections.cards.hybrid.sparse.strategy]
kind = "bm25_like"

[qdrant.collections.cards.hybrid.sparse.tokenizer]
library = "tokenizers"
source = "Qwen/Qwen3-Embedding-0.6B"

[qdrant.collections.cards.hybrid.sparse.preprocessing]
kind = "basic_word_v1"
lowercase = true
min_token_length = 2

[qdrant.collections.cards.hybrid.sparse.bag_of_words]
name = "cards_bow"
query = "binary_presence"
sparse_vocabulary_path = "Execution/artifacts/vocabularies/cards__sparse_vocabulary.json"

[qdrant.collections.cards.hybrid.sparse.bm25_like]
name = "cards_bm25"
query = "bm25_query_weight"
sparse_vocabulary_path = "Execution/artifacts/vocabularies/cards_bm25__sparse_vocabulary.json"
bm25_term_stats_path = "Execution/artifacts/term_stats/cards_bm25__term_stats.json"
k1 = 1.2
b = 0.75
idf_smoothing = 0.5

[qdrant.collections.practice]
kind = "hybrid"
corpus_version = "v1"

[qdrant.collections.practice.dense]
name = "practice_dense"
vector_name = "dense"

[qdrant.collections.practice.hybrid]
dense_vector_name = "dense"
sparse_vector_name = "sparse"

[qdrant.collections.practice.hybrid.sparse.strategy]
kind = "bm25_like"

[qdrant.collections.practice.hybrid.sparse.tokenizer]
library = "tokenizers"
source = "Qwen/Qwen3-Embedding-0.6B"

[qdrant.collections.practice.hybrid.sparse.preprocessing]
kind = "basic_word_v1"
lowercase = true
min_token_length = 2

[qdrant.collections.practice.hybrid.sparse.bag_of_words]
name = "practice_bow"
query = "binary_presence"
sparse_vocabulary_path = "Execution/artifacts/vocabularies/practice__sparse_vocabulary.json"

[qdrant.collections.practice.hybrid.sparse.bm25_like]
name = "practice_bm25"
query = "bm25_query_weight"
sparse_vocabulary_path = "Execution/artifacts/vocabularies/practice_bm25__sparse_vocabulary.json"
bm25_term_stats_path = "Execution/artifacts/term_stats/practice_bm25__term_stats.json"
k1 = 1.2
b = 0.75
idf_smoothing = 0.5

[qdrant.collections.theory]
kind = "hybrid"
corpus_version = "v1"

[qdrant.collections.theory.dense]
name = "theory_dense"
vector_name = "dense"

[qdrant.collections.theory.hybrid]
dense_vector_name = "dense"
sparse_vector_name = "sparse"

[qdrant.collections.theory.hybrid.sparse.strategy]
kind = "bm25_like"

[qdrant.collections.theory.hybrid.sparse.tokenizer]
library = "tokenizers"
source = "Qwen/Qwen3-Embedding-0.6B"

[qdrant.collections.theory.hybrid.sparse.preprocessing]
kind = "basic_word_v1"
lowercase = true
min_token_length = 2

[qdrant.collections.theory.hybrid.sparse.bag_of_words]
name = "theory_bow"
query = "binary_presence"
sparse_vocabulary_path = "Execution/artifacts/vocabularies/theory__sparse_vocabulary.json"

[qdrant.collections.theory.hybrid.sparse.bm25_like]
name = "theory_bm25"
query = "bm25_query_weight"
sparse_vocabulary_path = "Execution/artifacts/vocabularies/theory_bm25__sparse_vocabulary.json"
bm25_term_stats_path = "Execution/artifacts/term_stats/theory_bm25__term_stats.json"
k1 = 1.2
b = 0.75
idf_smoothing = 0.5
"#;

    const INGEST_TOML_DENSE: &str = r#"
[pipeline]
ingest_config_version = "v1"

[embedding.model]
name = "test-embedding"
dimension = 512

[qdrant.collections.cards]
kind = "dense"
corpus_version = "v2"

[qdrant.collections.cards.dense]
name = "cards_dense_col"
vector_name = "dense"

[qdrant.collections.practice]
kind = "dense"
corpus_version = "v2"

[qdrant.collections.practice.dense]
name = "practice_dense_col"
vector_name = "dense"

[qdrant.collections.theory]
kind = "dense"
corpus_version = "v2"

[qdrant.collections.theory.dense]
name = "theory_dense_col"
vector_name = "dense"
"#;

    #[test]
    fn merges_toml_and_env_into_settings() {
        let env = default_env();
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let s = load_test(&rt, &ing, &env).expect("load should succeed");

        assert_eq!(s.runtime.config_version, "v1");
        assert_eq!(s.embedding_model.url, "http://ollama-test:11434");
        assert_eq!(s.embedding_model.name, "qwen3-embedding:0.6b");
        assert_eq!(s.embedding_model.dimension, 1024);
        assert_eq!(s.postgres.url, "postgres://pg-test/db");
        assert_eq!(s.retrieval.qdrant_url, "http://qdrant-test:6333");
        assert_eq!(s.observability.tracing_endpoint, "http://otel-test:4317");
        assert_eq!(s.observability.metrics_endpoint, "http://otel-test:4318");
    }

    #[test]
    fn ollama_url_maps_to_embedding_model_url() {
        let env = default_env();
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let s = load_test(&rt, &ing, &env).unwrap();
        assert_eq!(s.embedding_model.url, "http://ollama-test:11434");
    }

    #[test]
    fn tracing_endpoint_maps_to_observability_settings() {
        let env = default_env();
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let s = load_test(&rt, &ing, &env).unwrap();
        assert_eq!(s.observability.tracing_endpoint, "http://otel-test:4317");
    }

    #[test]
    fn metrics_endpoint_maps_to_observability_settings() {
        let env = default_env();
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let s = load_test(&rt, &ing, &env).unwrap();
        assert_eq!(s.observability.metrics_endpoint, "http://otel-test:4318");
    }

    #[test]
    fn missing_observability_section_fails_with_load_error() {
        let env = default_env();
        let bad_rt = RUNTIME_TOML.replace(
            "\n[observability]\ntracing_enabled = true\nmetrics_enabled = true\ntrace_batch_scheduled_delay_ms = 5000\nmetrics_export_interval_ms = 60000\n",
            "",
        );
        let rt = write_temp(&bad_rt);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let err = load_test(&rt, &ing, &env).expect_err("should fail");
        assert!(
            matches!(err, ConfigError::Load(_)),
            "expected Load error, got: {err}"
        );
    }

    #[test]
    fn missing_env_var_fails_with_missing_environment_error() {
        let env: HashMap<&str, &str> = HashMap::new(); // no vars at all
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let err = load_test(&rt, &ing, &env).expect_err("should fail");
        assert!(
            matches!(err, ConfigError::MissingEnvironment { .. }),
            "expected MissingEnvironment, got: {err}"
        );
    }

    #[test]
    fn query_structuring_binding_is_preserved() {
        let env = default_env();
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let s = load_test(&rt, &ing, &env).unwrap();
        assert_eq!(s.query_structuring.provider, "ollama");
        assert_eq!(s.query_structuring.model, "qwen2.5:1.5b-instruct");
    }

    #[test]
    fn llm_structured_generation_binding_is_preserved() {
        let env = default_env();
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let s = load_test(&rt, &ing, &env).unwrap();
        assert_eq!(s.llm_structured_generation.provider, "ollama");
        assert_eq!(s.llm_structured_generation.model, "qwen2.5:1.5b-instruct");
    }

    #[test]
    fn unknown_provider_binding_fails_with_invalid_value() {
        let env = default_env();
        let bad_rt = RUNTIME_TOML.replace(
            "provider = \"ollama\"",
            "provider = \"unknown_provider\"",
        );
        let rt = write_temp(&bad_rt);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let err = load_test(&rt, &ing, &env).expect_err("should fail");
        assert!(
            matches!(err, ConfigError::InvalidValue { ref field, .. } if field == "query_structuring.model"),
            "expected InvalidValue for provider/model binding, got: {err}"
        );
    }

    #[test]
    fn dense_collection_kind_resolves_to_dense_variant() {
        let env = default_env();
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(INGEST_TOML_DENSE);

        let s = load_test(&rt, &ing, &env).unwrap();
        assert!(
            matches!(s.retrieval.cards.collection, CollectionSettings::Dense(_)),
            "expected Dense variant for cards"
        );
    }

    #[test]
    fn hybrid_collection_kind_resolves_to_hybrid_variant() {
        let env = default_env();
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let s = load_test(&rt, &ing, &env).unwrap();
        assert!(
            matches!(s.retrieval.cards.collection, CollectionSettings::Hybrid(_)),
            "expected Hybrid variant for cards"
        );
    }

    #[test]
    fn unknown_collection_kind_fails_with_invalid_value() {
        let env = default_env();
        let bad_ing = INGEST_TOML_DENSE.replace("kind = \"dense\"", "kind = \"unknown_kind\"");
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(&bad_ing);

        let err = load_test(&rt, &ing, &env).expect_err("should fail");
        assert!(
            matches!(err, ConfigError::InvalidValue { .. }),
            "expected InvalidValue for collection kind, got: {err}"
        );
    }

    #[test]
    fn bm25_like_sparse_strategy_resolves_to_bm25_like_variant() {
        let env = default_env();
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(INGEST_TOML_HYBRID); // strategy.kind = "bm25_like"

        let s = load_test(&rt, &ing, &env).unwrap();
        if let CollectionSettings::Hybrid(h) = &s.retrieval.cards.collection {
            assert!(
                matches!(h.sparse.strategy, SparseStrategySettings::Bm25Like(_)),
                "expected Bm25Like variant"
            );
        } else {
            panic!("expected Hybrid collection");
        }
    }

    #[test]
    fn bag_of_words_sparse_strategy_resolves_to_bag_of_words_variant() {
        let env = default_env();
        let bow_ing = INGEST_TOML_HYBRID.replace("kind = \"bm25_like\"", "kind = \"bag_of_words\"");
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(&bow_ing);

        let s = load_test(&rt, &ing, &env).unwrap();
        if let CollectionSettings::Hybrid(h) = &s.retrieval.cards.collection {
            assert!(
                matches!(h.sparse.strategy, SparseStrategySettings::BagOfWords(_)),
                "expected BagOfWords variant"
            );
        } else {
            panic!("expected Hybrid collection");
        }
    }

    #[test]
    fn sparse_artifact_paths_are_preserved_in_resolved_settings() {
        let env = default_env();
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let s = load_test(&rt, &ing, &env).unwrap();

        let cards = match &s.retrieval.cards.collection {
            CollectionSettings::Hybrid(hybrid) => hybrid,
            other => panic!("expected Hybrid collection, got {other:?}"),
        };

        match &cards.sparse.strategy {
            SparseStrategySettings::Bm25Like(settings) => {
                assert_eq!(
                    settings.sparse_vocabulary_path,
                    "Execution/artifacts/vocabularies/cards_bm25__sparse_vocabulary.json"
                );
                assert_eq!(
                    settings.bm25_term_stats_path,
                    "Execution/artifacts/term_stats/cards_bm25__term_stats.json"
                );
            }
            other => panic!("expected Bm25Like strategy, got {other:?}"),
        }
    }

    #[test]
    fn input_normalization_settings_are_preserved() {
        let env = default_env();
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let s = load_test(&rt, &ing, &env).unwrap();
        assert_eq!(s.input_normalization.max_input_tokens, 3000);
        assert_eq!(
            s.input_normalization.tokenizer_source,
            "Qwen/Qwen3-Embedding-0.6B"
        );
    }

    #[test]
    fn query_structuring_settings_are_preserved() {
        let env = default_env();
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let s = load_test(&rt, &ing, &env).unwrap();
        assert_eq!(
            s.query_structuring.controlled_vocabulary_path,
            "Specification/runtime/request_pipeline/query_structuring_controlled_vocabulary.manual_test.json"
        );
        assert_eq!(
            s.query_structuring.prompt_asset_path,
            "Specification/runtime/request_pipeline/query_structuring_prompt_baseline_v2.manual_test.json"
        );
        assert_eq!(s.query_structuring.max_output_tokens, 2200);
    }

    #[test]
    fn unknown_sparse_strategy_kind_fails_with_invalid_value() {
        let env = default_env();
        let bad_ing =
            INGEST_TOML_HYBRID.replace("kind = \"bm25_like\"", "kind = \"unknown_sparse\"");
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(&bad_ing);

        let err = load_test(&rt, &ing, &env).expect_err("should fail");
        assert!(
            matches!(err, ConfigError::InvalidValue { .. }),
            "expected InvalidValue for sparse strategy kind, got: {err}"
        );
    }

    #[test]
    fn collection_retrieval_settings_preserve_all_fields() {
        let env = default_env();
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let s = load_test(&rt, &ing, &env).unwrap();
        let cards = &s.retrieval.cards;
        assert_eq!(cards.top_k, 8);
        assert!((cards.score_threshold - 0.2).abs() < f32::EPSILON);
        assert_eq!(cards.embedding_retry.max_attempts, 3);
        assert_eq!(cards.qdrant_retry.max_attempts, 3);
    }

    #[test]
    fn corpus_version_is_on_collection_not_retrieval() {
        let env = default_env();
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let s = load_test(&rt, &ing, &env).unwrap();
        match &s.retrieval.cards.collection {
            CollectionSettings::Hybrid(h) => assert_eq!(h.corpus_version, "v1"),
            CollectionSettings::Dense(d) => assert_eq!(d.corpus_version, "v1"),
        }
    }

    #[test]
    fn prompt_context_asset_path_is_preserved() {
        let env = default_env();
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let s = load_test(&rt, &ing, &env).unwrap();
        assert_eq!(
            s.prompt_context.prompt_asset_path,
            "Specification/runtime/request_pipeline/prompt_context_assembly/diagnostic_response_prompt_baseline.manual_test.json"
        );
    }

    #[test]
    fn prompt_context_chunk_role_fields_are_preserved() {
        use crate::config::ChunkPackingSource;
        use crate::shared_types::IncidentChunkTag;
        let env = default_env();
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let s = load_test(&rt, &ing, &env).unwrap();
        let cp = &s.prompt_context.chunk_packing;

        assert_eq!(
            cp.evidence_for_match.source,
            ChunkPackingSource::PrimaryIncident
        );
        assert_eq!(cp.evidence_for_match.limit, 1);
        assert!(cp.evidence_for_match.fallback_to_any_chunk);
        assert_eq!(
            cp.evidence_for_match.tag_priority,
            vec![IncidentChunkTag::Symptom, IncidentChunkTag::FailureMode]
        );

        assert_eq!(
            cp.alternative_context.source,
            ChunkPackingSource::AlternativeIncident
        );
        assert_eq!(cp.alternative_context.limit, 2);
        assert_eq!(cp.alternative_context.per_case_limit, Some(1));

        assert_eq!(cp.mechanism_explanation.source, ChunkPackingSource::Theory);
        assert_eq!(cp.mechanism_explanation.limit, 1);
        assert!(cp.mechanism_explanation.tag_priority.is_empty());
    }

    #[test]
    fn prompt_context_supporting_explanation_default_limit_is_one() {
        let env = default_env();
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let s = load_test(&rt, &ing, &env).unwrap();
        assert_eq!(
            s.prompt_context.chunk_packing.supporting_explanation.limit,
            1
        );
    }

    #[test]
    fn prompt_context_accepts_full_canonical_chunk_tags() {
        let env = default_env();
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let s = load_test(&rt, &ing, &env).unwrap();
        let tags = &s
            .prompt_context
            .chunk_packing
            .evidence_for_match
            .tag_priority;
        assert!(tags.contains(&crate::shared_types::IncidentChunkTag::Symptom));
    }

    #[test]
    fn prompt_context_rejects_short_tag_alias() {
        let env = default_env();
        let bad_rt = RUNTIME_TOML.replace(
            r#"tag_priority = ["chunk_role:symptom", "chunk_role:failure_mode"]"#,
            r#"tag_priority = ["symptom"]"#,
        );
        let rt = write_temp(&bad_rt);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let err = load_test(&rt, &ing, &env).expect_err("should fail on short tag alias");
        assert!(
            matches!(err, ConfigError::InvalidValue { .. }),
            "expected InvalidValue for short tag alias, got: {err}"
        );
    }

    #[test]
    fn prompt_context_rejects_unknown_chunk_tag() {
        let env = default_env();
        let bad_rt = RUNTIME_TOML.replace(
            r#"tag_priority = ["chunk_role:symptom", "chunk_role:failure_mode"]"#,
            r#"tag_priority = ["chunk_role:unknown_tag_xyz"]"#,
        );
        let rt = write_temp(&bad_rt);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let err = load_test(&rt, &ing, &env).expect_err("should fail on unknown tag");
        assert!(
            matches!(err, ConfigError::InvalidValue { .. }),
            "expected InvalidValue for unknown tag, got: {err}"
        );
    }

    #[test]
    fn prompt_context_rejects_duplicate_tags_in_priority_list() {
        let env = default_env();
        let bad_rt = RUNTIME_TOML.replace(
            r#"tag_priority = ["chunk_role:symptom", "chunk_role:failure_mode"]"#,
            r#"tag_priority = ["chunk_role:symptom", "chunk_role:symptom"]"#,
        );
        let rt = write_temp(&bad_rt);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let err = load_test(&rt, &ing, &env).expect_err("should fail on duplicate tags");
        assert!(
            matches!(err, ConfigError::InvalidValue { .. }),
            "expected InvalidValue for duplicate tags, got: {err}"
        );
    }

    #[test]
    fn prompt_context_rejects_invalid_source_value() {
        let env = default_env();
        let bad_rt = RUNTIME_TOML.replace(
            r#"source = "primary_incident""#,
            r#"source = "unknown_source_kind""#,
        );
        let rt = write_temp(&bad_rt);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let err = load_test(&rt, &ing, &env).expect_err("should fail on invalid source");
        assert!(
            matches!(err, ConfigError::InvalidValue { .. }),
            "expected InvalidValue for invalid source, got: {err}"
        );
    }

    #[test]
    fn llm_structured_generation_max_output_tokens_is_preserved() {
        let env = default_env();
        let rt = write_temp(RUNTIME_TOML);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let s = load_test(&rt, &ing, &env).unwrap();
        assert_eq!(s.llm_structured_generation.max_output_tokens, 1200);
    }

    #[test]
    fn missing_llm_structured_generation_section_fails_with_load_error() {
        let env = default_env();
        let bad_rt = RUNTIME_TOML.replace(
            "\n[llm_structured_generation]\nmax_output_tokens = 1200\n",
            "",
        );
        let rt = write_temp(&bad_rt);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let err = load_test(&rt, &ing, &env).expect_err("should fail");
        assert!(
            matches!(err, ConfigError::Load(_)),
            "expected Load error, got: {err}"
        );
    }
}
