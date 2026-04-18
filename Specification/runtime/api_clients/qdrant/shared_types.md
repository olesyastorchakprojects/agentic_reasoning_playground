## 1) Purpose / Scope

This document defines shared Rust types used across Qdrant runtime API client specifications.

These shared types are used by:
- `dense_search_client.md`
- `hybrid_search_client.md`
- future collection-layer Qdrant specs.

This document defines only reusable type shapes.
It does not define:
- client behavior;
- payload mapping into domain entities;
- request execution rules.
- full crate-level runtime settings ownership.

## 2) Shared Types

```rust
pub struct QdrantCollectionName(pub String);

pub struct QdrantVectorName(pub String);

pub struct NormalizedUserQuery(pub String);

pub struct RetryPolicyConfig {
    pub max_attempts: u32,
    pub backoff: RetryBackoffKind,
}

pub struct EmbeddingConfig {
    pub base_url: url::Url,
    pub model_name: String,
    pub embedding_dimension: usize,
}

pub struct QdrantDenseCollectionConfig {
    pub qdrant_base_url: url::Url,
    pub collection_name: QdrantCollectionName,
    pub vector_name: Option<QdrantVectorName>,
}

pub struct QdrantHybridCollectionConfig {
    pub qdrant_base_url: url::Url,
    pub collection_name: QdrantCollectionName,
    pub vector_name: QdrantVectorName,
    pub sparse_vector_name: QdrantVectorName,
}

pub enum SparseStrategyConfig {
    BagOfWords {
        sparse_vocabulary_path: String,
    },
    Bm25Like {
        sparse_vocabulary_path: String,
        bm25_term_stats_path: String,
        k1: f32,
        b: f32,
        idf_smoothing: f32,
    },
}

pub struct SparseVocabularyArtifact {
    pub vocabulary_name: String,
    pub token_id_by_token: std::collections::BTreeMap<String, u32>,
}

pub struct Bm25TermStatsArtifact {
    pub collection_name: String,
    pub vocabulary_name: String,
    pub document_count: u64,
    pub average_document_length: f64,
    pub document_frequency_by_token_id: std::collections::BTreeMap<u32, u64>,
}

pub struct Embedding {
    pub values: Vec<f32>,
}

pub struct SparseVector {
    pub indices: Vec<u32>,
    pub values: Vec<f32>,
}

pub struct QdrantMatchAnyFilter {
    pub field_name: String,
    pub values: Vec<String>,
}

pub struct QdrantFilter {
    pub must_match_any: Vec<QdrantMatchAnyFilter>,
}

pub struct RawQdrantPayload {
    pub fields: std::collections::BTreeMap<String, QdrantPayloadValue>,
}

pub enum QdrantPayloadValue {
    String(String),
    StringList(Vec<String>),
    Number(f64),
    Bool(bool),
    Null,
}

pub struct RawQdrantHit {
    pub score: f32,
    pub payload: RawQdrantPayload,
}
```

## 3) Type Rules

- `NormalizedUserQuery.0` must not be empty after trimming;
- collection-layer modules must use `NormalizedUserQuery.0` unchanged as the source text for embedding requests;
- collection-layer modules must not rewrite, paraphrase, or otherwise mutate `NormalizedUserQuery.0` before embedding generation;
- `Embedding.values` must not be empty when used in outbound search requests;
- `SparseVector.indices` and `SparseVector.values` must be aligned arrays of equal length;
- `SparseVector.indices` and `SparseVector.values` must not be empty when sparse search is requested;
- all URL fields in shared config types must already be valid `url::Url` values;
- `EmbeddingConfig.embedding_dimension` must be greater than zero;
- `SparseStrategyConfig::Bm25Like` requires a BM25 term-stats artifact path;
- `SparseStrategyConfig::Bm25Like` requires `k1`, `b`, and `idf_smoothing`;
- `SparseStrategyConfig::BagOfWords` does not require a BM25 term-stats artifact path;
- `SparseVocabularyArtifact` must be loaded once in collection-constructor logic and cached for full collection lifetime;
- `Bm25TermStatsArtifact` must be loaded once in collection-constructor logic and cached for full collection lifetime;
- `Bm25TermStatsArtifact` is required only for `SparseStrategyConfig::Bm25Like`;
- invalid sparse-artifact content must cause constructor failure in collection-level modules that depend on it;
- transport-level Qdrant clients support only the listed scalar and string-list payload shapes;
- nested objects, nested arrays, mixed arrays, and unsupported payload shapes must be treated as invalid response data;
- `RawQdrantHit` preserves transport-level score and payload only;
- collection-layer modules are responsible for domain-specific payload mapping.

## 4) Settings Propagation Rules

Crate-level runtime settings must not be used directly as the config type for
leaf Qdrant transport clients.

Collection-layer runtime modules must convert crate-level retrieval settings
slices into the module-owned config types defined in this file before
constructing leaf clients, whenever such higher-level wiring is added.

The current crate-level source settings are reached through these full paths:
- `Settings.retrieval.qdrant_url`
- `Settings.retrieval.cards`
- `Settings.retrieval.practice`
- `Settings.retrieval.theory`
- `Settings.retrieval.<collection>.collection`
- `Settings.retrieval.<collection>.collection = CollectionSettings::Dense(DenseCollectionSettings)`
- `Settings.retrieval.<collection>.collection = CollectionSettings::Hybrid(HybridCollectionSettings)`
- `Settings.retrieval.<collection>.collection = CollectionSettings::Hybrid(HybridCollectionSettings { sparse, .. })`
- `Settings.retrieval.<collection>.collection = CollectionSettings::Hybrid(HybridCollectionSettings { sparse: SparseSettings { strategy, .. }, .. })`
- `Settings.embedding_model`

### Embedding Config Mapping

Required field mappings:
- `EmbeddingConfig.base_url` <- `Settings.embedding_model.url`
- `EmbeddingConfig.model_name` <- `Settings.embedding_model.name`
- `EmbeddingConfig.embedding_dimension` <- `Settings.embedding_model.dimension`

Rules:
- `EmbeddingConfig` must be constructed once per collection implementation constructor path;
- leaf embedding clients must not depend on crate-level `Settings`.

### Dense Collection Config Mapping

When `Settings.retrieval.<collection>.collection = CollectionSettings::Dense(settings)`,
a future collection-layer module may construct:
- `QdrantDenseCollectionConfig`
- `RetryPolicyConfig` for embedding requests
- `RetryPolicyConfig` for Qdrant requests

Required field mappings:
- `QdrantDenseCollectionConfig.qdrant_base_url` <- `Settings.retrieval.qdrant_url`
- `QdrantDenseCollectionConfig.collection_name` <- `Settings.retrieval.<collection>.collection.DenseCollectionSettings.name`
- `QdrantDenseCollectionConfig.vector_name` <- `Settings.retrieval.<collection>.collection.DenseCollectionSettings.vector_name`
- embedding retry policy <- `Settings.retrieval.<collection>.embedding_retry`
- Qdrant retry policy <- `Settings.retrieval.<collection>.qdrant_retry`

### Hybrid Collection Config Mapping

When `Settings.retrieval.<collection>.collection = CollectionSettings::Hybrid(settings)`,
a future collection-layer module may construct:
- `QdrantHybridCollectionConfig`
- `SparseStrategyConfig`
- `RetryPolicyConfig` for embedding requests
- `RetryPolicyConfig` for Qdrant requests

Required field mappings:
- `QdrantHybridCollectionConfig.qdrant_base_url` <- `Settings.retrieval.qdrant_url`
- `QdrantHybridCollectionConfig.vector_name` <- `Settings.retrieval.<collection>.collection.HybridCollectionSettings.dense_vector_name`
- `QdrantHybridCollectionConfig.sparse_vector_name` <- `Settings.retrieval.<collection>.collection.HybridCollectionSettings.sparse_vector_name`
- embedding retry policy <- `Settings.retrieval.<collection>.embedding_retry`
- Qdrant retry policy <- `Settings.retrieval.<collection>.qdrant_retry`

Required sparse strategy mappings:
- when `Settings.retrieval.<collection>.collection.HybridCollectionSettings.sparse.strategy = SparseStrategySettings::BagOfWords(settings)`, construct:
  - `SparseStrategyConfig::BagOfWords { sparse_vocabulary_path }`
- when `Settings.retrieval.<collection>.collection.HybridCollectionSettings.sparse.strategy = SparseStrategySettings::Bm25Like(settings)`, construct:
  - `SparseStrategyConfig::Bm25Like { sparse_vocabulary_path, bm25_term_stats_path, k1, b, idf_smoothing }`

Required artifact-path field mappings:
- `SparseStrategyConfig::BagOfWords.sparse_vocabulary_path` <- `BagOfWordsSettings.sparse_vocabulary_path`
- `SparseStrategyConfig::Bm25Like.sparse_vocabulary_path` <- `Bm25LikeSettings.sparse_vocabulary_path`
- `SparseStrategyConfig::Bm25Like.bm25_term_stats_path` <- `Bm25LikeSettings.bm25_term_stats_path`

Rules:
- the collection name used in `QdrantHybridCollectionConfig.collection_name` must come from the active sparse strategy settings variant because physical hybrid collection names are strategy-specific;
- the vocabulary and BM25 term-stats artifact paths must come from typed settings rather than being derived from repository conventions at runtime;
- collection-layer modules must load sparse artifacts only from the paths stored inside `SparseStrategyConfig`;
- collection-layer modules must not accept sparse artifact paths as separate constructor arguments once `SparseStrategyConfig` has been constructed from settings;
- leaf Qdrant transport clients must not depend on crate-level `Settings`;
- the current minimal crate-skeleton stage does not require production code that performs this wiring;
- whenever a higher-level runtime layer performs this wiring, conversion from crate-level retrieval settings into module-owned config types must happen before concrete Qdrant transport clients are constructed.
