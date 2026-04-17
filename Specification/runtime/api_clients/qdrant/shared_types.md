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
- `SparseStrategyConfig::BagOfWords` does not require a BM25 term-stats artifact path;
- `SparseVocabularyArtifact` must be loaded once in collection-constructor logic and cached for full collection lifetime;
- `Bm25TermStatsArtifact` must be loaded once in collection-constructor logic and cached for full collection lifetime;
- `Bm25TermStatsArtifact` is required only for `SparseStrategyConfig::Bm25Like`;
- invalid sparse-artifact content must cause constructor failure in collection-level modules that depend on it;
- transport-level Qdrant clients support only the listed scalar and string-list payload shapes;
- nested objects, nested arrays, mixed arrays, and unsupported payload shapes must be treated as invalid response data;
- `RawQdrantHit` preserves transport-level score and payload only;
- collection-layer modules are responsible for domain-specific payload mapping.
