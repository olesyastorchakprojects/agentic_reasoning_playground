use std::path::Path;

use serde::Deserialize;

use crate::utils::retry::{RetryBackoffKind, RetryPolicyConfig};

use super::{
    BagOfWordsSettings, Bm25LikeSettings, CollectionRetrievalSettings, CollectionSettings,
    ConfigError, DenseCollectionSettings, EmbeddingModelSettings, HybridCollectionSettings,
    InputNormalizationSettings, ModelSettings, ModelTransportSettings, ObservabilitySettings,
    OllamaModelSettings, PostgresSettings, RetrievalSettings, RuntimeSettings, Settings,
    SparsePreprocessingSettings, SparseSettings, SparseStrategySettings, TogetherModelSettings,
    TokenizerSettings,
};

// ---------------------------------------------------------------------------
// Raw intermediate structs mirroring the merged TOML structure
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawConfig {
    runtime: RawRuntime,
    input_normalization: RawInputNormalization,
    retrieval: RawRetrieval,
    model: RawModel,
    embedding: RawEmbedding,
    qdrant: RawQdrant,
    observability: RawObservability,
}

#[derive(Debug, Deserialize)]
struct RawInputNormalization {
    max_input_tokens: usize,
    tokenizer_source: String,
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
    transport_kind: String,
    ollama: Option<RawOllamaModel>,
    together: Option<RawTogetherModel>,
}

#[derive(Debug, Deserialize)]
struct RawOllamaModel {
    model_name: String,
    timeout_sec: u64,
    retry: RawRetryPolicy,
}

#[derive(Debug, Deserialize)]
struct RawTogetherModel {
    model_name: String,
    timeout_sec: u64,
    retry: RawRetryPolicy,
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
    let cards = resolve_collection_retrieval(
        &raw.retrieval.cards,
        &raw.qdrant.collections.cards,
        "cards",
    )?;
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

    let ollama_url = require_env_fn(env_fn, "OLLAMA_URL")?;
    let qdrant_url = require_env_fn(env_fn, "QDRANT_URL")?;
    let postgres_url = require_env_fn(env_fn, "POSTGRES_URL")?;
    let tracing_endpoint = require_env_fn(env_fn, "TRACING_ENDPOINT")?;
    let metrics_endpoint = require_env_fn(env_fn, "METRICS_ENDPOINT")?;

    let model_transport = resolve_model_transport(&raw.model, &ollama_url, env_fn)?;

    Ok(Settings {
        runtime: RuntimeSettings {
            config_version: raw.runtime.config_version,
        },
        input_normalization: InputNormalizationSettings {
            max_input_tokens: raw.input_normalization.max_input_tokens,
            tokenizer_source: raw.input_normalization.tokenizer_source,
        },
        retrieval: RetrievalSettings {
            qdrant_url,
            cards,
            practice,
            theory,
        },
        model: ModelSettings {
            transport: model_transport,
        },
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

fn resolve_model_transport(
    raw: &RawModel,
    ollama_url: &str,
    env_fn: &impl Fn(&str) -> Option<String>,
) -> Result<ModelTransportSettings, ConfigError> {
    match raw.transport_kind.as_str() {
        "ollama" => {
            let cfg = raw.ollama.as_ref().ok_or_else(|| ConfigError::Load(
                "model.transport_kind = \"ollama\" but [model.ollama] section is missing".into(),
            ))?;
            Ok(ModelTransportSettings::Ollama(OllamaModelSettings {
                url: ollama_url.to_string(),
                model_name: cfg.model_name.clone(),
                timeout_sec: cfg.timeout_sec,
                retry: resolve_retry(&cfg.retry, "model.ollama.retry")?,
            }))
        }
        "together" => {
            let cfg = raw.together.as_ref().ok_or_else(|| ConfigError::Load(
                "model.transport_kind = \"together\" but [model.together] section is missing"
                    .into(),
            ))?;
            let url = require_env_fn(env_fn, "OPENAI_COMPATIBLE_URL")?;
            let api_key = require_env_fn(env_fn, "TOGETHER_API_KEY")?;
            Ok(ModelTransportSettings::Together(TogetherModelSettings {
                url,
                api_key,
                model_name: cfg.model_name.clone(),
                timeout_sec: cfg.timeout_sec,
                retry: resolve_retry(&cfg.retry, "model.together.retry")?,
            }))
        }
        other => Err(ConfigError::InvalidValue {
            field: "model.transport_kind".to_string(),
            reason: format!("unknown transport kind '{other}'"),
        }),
    }
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
            let cfg = raw.dense.as_ref().ok_or_else(|| ConfigError::Load(format!(
                "qdrant.collections.{name}.kind = \"dense\" but [dense] section is missing"
            )))?;
            Ok(CollectionSettings::Dense(DenseCollectionSettings {
                name: cfg.name.clone(),
                vector_name: cfg.vector_name.clone(),
                corpus_version: raw.corpus_version.clone(),
            }))
        }
        "hybrid" => {
            let cfg = raw.hybrid.as_ref().ok_or_else(|| ConfigError::Load(format!(
                "qdrant.collections.{name}.kind = \"hybrid\" but [hybrid] section is missing"
            )))?;
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
                field: format!(
                    "qdrant.collections.{col_name}.hybrid.sparse.strategy.kind"
                ),
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

[retrieval.cards]
top_k = 8
score_threshold = 0.2

[retrieval.cards.embedding_retry]
max_attempts = 3
backoff = "exponential"

[retrieval.cards.qdrant_retry]
max_attempts = 3
backoff = "exponential"

[retrieval.practice]
top_k = 12
score_threshold = 0.2

[retrieval.practice.embedding_retry]
max_attempts = 3
backoff = "exponential"

[retrieval.practice.qdrant_retry]
max_attempts = 3
backoff = "exponential"

[retrieval.theory]
top_k = 12
score_threshold = 0.2

[retrieval.theory.embedding_retry]
max_attempts = 3
backoff = "exponential"

[retrieval.theory.qdrant_retry]
max_attempts = 3
backoff = "exponential"

[model]
transport_kind = "ollama"

[model.ollama]
model_name = "qwen2.5:1.5b-instruct"
timeout_sec = 120

[model.ollama.retry]
max_attempts = 3
backoff = "exponential"

[model.together]
model_name = "openai/gpt-oss-20b"
timeout_sec = 120

[model.together.retry]
max_attempts = 3
backoff = "exponential"

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
    fn ollama_transport_kind_resolves_to_ollama_variant() {
        let env = default_env();
        let rt = write_temp(RUNTIME_TOML); // transport_kind = "ollama"
        let ing = write_temp(INGEST_TOML_HYBRID);

        let s = load_test(&rt, &ing, &env).unwrap();
        assert!(
            matches!(s.model.transport, ModelTransportSettings::Ollama(_)),
            "expected Ollama variant"
        );
    }

    #[test]
    fn together_transport_kind_resolves_to_together_variant() {
        let mut env = default_env();
        env.insert("OPENAI_COMPATIBLE_URL", "https://api.together.xyz/v1");
        env.insert("TOGETHER_API_KEY", "test-key");

        let together_rt = RUNTIME_TOML.replace(
            "transport_kind = \"ollama\"",
            "transport_kind = \"together\"",
        );
        let rt = write_temp(&together_rt);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let s = load_test(&rt, &ing, &env).unwrap();
        assert!(
            matches!(s.model.transport, ModelTransportSettings::Together(_)),
            "expected Together variant"
        );
    }

    #[test]
    fn unknown_transport_kind_fails_with_invalid_value() {
        let env = default_env();
        let bad_rt = RUNTIME_TOML.replace(
            "transport_kind = \"ollama\"",
            "transport_kind = \"unknown_transport\"",
        );
        let rt = write_temp(&bad_rt);
        let ing = write_temp(INGEST_TOML_HYBRID);

        let err = load_test(&rt, &ing, &env).expect_err("should fail");
        assert!(
            matches!(err, ConfigError::InvalidValue { ref field, .. } if field == "model.transport_kind"),
            "expected InvalidValue for transport_kind, got: {err}"
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
        let bow_ing = INGEST_TOML_HYBRID.replace(
            "kind = \"bm25_like\"",
            "kind = \"bag_of_words\"",
        );
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
        assert_eq!(s.input_normalization.tokenizer_source, "Qwen/Qwen3-Embedding-0.6B");
    }

    #[test]
    fn unknown_sparse_strategy_kind_fails_with_invalid_value() {
        let env = default_env();
        let bad_ing = INGEST_TOML_HYBRID.replace(
            "kind = \"bm25_like\"",
            "kind = \"unknown_sparse\"",
        );
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
}
