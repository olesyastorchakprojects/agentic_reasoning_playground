## 1) Purpose / Scope

`theory_chunks_collection` defines the collection-level runtime interface for searching theory chunks.

This module:
- exposes a domain-facing collection interface for theory-chunk search;
- accepts user query text rather than prepared vectors;
- prepares dense or hybrid query inputs depending on the concrete implementation;
- calls the corresponding Qdrant transport-level search client;
- maps raw Qdrant payloads into `TheoryChunkSearchHit`.

This module does not:
- expose raw Qdrant HTTP request shapes to callers;
- create collections, write points, or manage aliases;
- perform cross-collection ranking;
- decide which collection implementation should be wired by higher layers.

Shared collection-level rules:
- `Specification/runtime/api_clients/qdrant/collections_common.md`

## 2) Public Trait

The module must expose the collection trait:

```rust
#[async_trait::async_trait]
pub trait TheoryChunksCollection {
    async fn search(
        &self,
        request: &TheoryChunkSearchRequest,
    ) -> Result<TheoryChunkSearchResult, TheoryChunksCollectionError>;
}
```

Trait rules:
- callers pass user query text, not prepared embeddings or sparse vectors;
- trait implementations hide whether the underlying search mode is dense or hybrid;
- the public return type must be the same for all concrete implementations.

## 3) Public Types

Required shared types:
- `QdrantCollectionName`
- `QdrantVectorName`
- `NormalizedUserQuery`
- `RetryPolicyConfig`
- `EmbeddingConfig`
- `QdrantDenseCollectionConfig`
- `QdrantHybridCollectionConfig`
- `SparseStrategyConfig`
- `SparseVocabularyArtifact`
- `Bm25TermStatsArtifact`
- `Embedding`
- `SparseVector`
- `CollectionRetrievalSettings`
- `CollectionSettings`
- `EmbeddingModelSettings`

Source of truth:
- `Specification/runtime/api_clients/qdrant/shared_types.md`
- `DenseSearchClientError` source of truth is `Specification/runtime/api_clients/qdrant/dense_search_client.md`
- `HybridSearchClientError` source of truth is `Specification/runtime/api_clients/qdrant/hybrid_search_client.md`
- `EmbeddingClientError` source of truth is `Specification/runtime/api_clients/embedding_client.md`
- shared collection-level rules are defined in `Specification/runtime/api_clients/qdrant/collections_common.md`

Collection-specific public types:

```rust
pub struct TheoryChunkSearchRequest {
    pub user_query: NormalizedUserQuery,
    pub limit: usize,
    pub score_threshold: f32,
}

pub struct TheoryChunkSearchHit {
    pub chunk_id: String,
    pub score: f32,
    pub text: String,
}

pub struct TheoryChunkSearchResult {
    pub hits: Vec<TheoryChunkSearchHit>,
}
```

Type rules:
- `user_query` must satisfy the rules of `NormalizedUserQuery`;
- `limit` must be greater than zero;
- negative `score_threshold` is an invalid request;
- `NaN`, `+inf`, and `-inf` threshold values are invalid requests;
- `TheoryChunkSearchResult.hits` must preserve Qdrant response order;
- the current version does not support collection-level filters for theory search requests.

## 4) Embedding Usage

Embedding generation rules for this module must follow:
- `Specification/runtime/api_clients/embedding_client.md`

In particular:
- the embedding request input must be the unchanged `request.user_query.0` string;
- this module must not rewrite `request.user_query.0` before embedding generation.

## 5) Sparse Query Preparation

Hybrid sparse-query preparation rules must follow:
- `Specification/runtime/api_clients/qdrant/hybrid/sparse_query_preparation.md`

In particular:
- the source text for sparse-vector construction must be the unchanged `request.user_query.0` string;
- if sparse query vector construction produces zero sparse terms after normalization and vocabulary lookup, hybrid implementation must fail with `TheoryChunksCollectionError::QueryPreparation`.

## 6) Concrete Implementations

The module must expose these concrete structures:

```rust
pub struct QdrantTheoryChunksCollectionDense {
    embedding: EmbeddingConfig,
    qdrant: QdrantDenseCollectionConfig,
    embedding_client: EmbeddingClient,
    qdrant_client: QdrantDenseSearchClient,
}

pub struct QdrantTheoryChunksCollectionHybrid {
    embedding: EmbeddingConfig,
    qdrant: QdrantHybridCollectionConfig,
    sparse: SparseStrategyConfig,
    embedding_client: EmbeddingClient,
    qdrant_client: QdrantHybridSearchClient,
    sparse_vocabulary: SparseVocabularyArtifact,
    bm25_term_stats: Option<Bm25TermStatsArtifact>,
}
```

Field rules:
- shared collection-construction and lifecycle rules must follow `Specification/runtime/api_clients/qdrant/collections_common.md`;
- when `SparseStrategyConfig::BagOfWords` is selected, `bm25_term_stats` must be `None`;
- when `SparseStrategyConfig::Bm25Like` is selected, `bm25_term_stats` must be `Some(...)`.
- concrete implementations must not store crate-level `Settings` directly.

### 6.1 Dense Implementation Responsibility

`QdrantTheoryChunksCollectionDense` must:
- validate `TheoryChunkSearchRequest` immediately on entry to `search`;
- create a dense embedding from `request.user_query`;
- validate that embedding length matches `embedding.embedding_dimension`;
- if the returned embedding length does not match `embedding.embedding_dimension`, fail with `TheoryChunksCollectionError::IncorrectEmbeddingShape`;
- on incorrect embedding shape, fail before building or executing any Qdrant search request;
- build `DenseSearchRequest`;
- build `DenseSearchRequest` with `filter = None`;
- call `QdrantDenseSearchClient::search`;
- map `RawQdrantHit` payloads into `TheoryChunkSearchHit`.

Dense implementation validation rules:
- empty `request.user_query.0` after trimming must fail with `TheoryChunksCollectionError::InvalidRequest`;
- `request.limit == 0` must fail with `TheoryChunksCollectionError::InvalidRequest`;
- negative `request.score_threshold` must fail with `TheoryChunksCollectionError::InvalidRequest`;
- `NaN`, `+inf`, and `-inf` threshold values must fail with `TheoryChunksCollectionError::InvalidRequest`;
- `qdrant.vector_name = None` is allowed and means default-vector behavior for the dense transport client.

### 6.2 Hybrid Implementation Responsibility

`QdrantTheoryChunksCollectionHybrid` must:
- validate `TheoryChunkSearchRequest` immediately on entry to `search`;
- create a dense embedding from `request.user_query`;
- validate that embedding length matches `embedding.embedding_dimension`;
- if the returned embedding length does not match `embedding.embedding_dimension`, fail with `TheoryChunksCollectionError::IncorrectEmbeddingShape`;
- on incorrect embedding shape, fail before building or executing any Qdrant search request;
- build a sparse vector from `request.user_query` according to `SparseStrategyConfig`;
- build `HybridSearchRequest`;
- build `HybridSearchRequest` with `filter = None`;
- call `QdrantHybridSearchClient::search`;
- map `RawQdrantHit` payloads into `TheoryChunkSearchHit`.

Hybrid implementation validation rules:
- empty `request.user_query.0` after trimming must fail with `TheoryChunksCollectionError::InvalidRequest`;
- `request.limit == 0` must fail with `TheoryChunksCollectionError::InvalidRequest`;
- negative `request.score_threshold` must fail with `TheoryChunksCollectionError::InvalidRequest`;
- `NaN`, `+inf`, and `-inf` threshold values must fail with `TheoryChunksCollectionError::InvalidRequest`.

## 7) Constructor Rules

Each concrete implementation must expose a public constructor that accepts typed
settings slices rather than raw TOML values or whole crate-level `Settings`.

Dense constructor:

```rust
impl QdrantTheoryChunksCollectionDense {
    pub fn from_settings(
        collection_settings: &CollectionRetrievalSettings,
        embedding_model: &EmbeddingModelSettings,
        qdrant_url: &str,
    ) -> Result<Self, TheoryChunksCollectionError>;
}
```

Hybrid constructor:

```rust
impl QdrantTheoryChunksCollectionHybrid {
    pub fn from_settings(
        collection_settings: &CollectionRetrievalSettings,
        embedding_model: &EmbeddingModelSettings,
        qdrant_url: &str,
    ) -> Result<Self, TheoryChunksCollectionError>;
}
```

Constructor rules:
- shared constructor rules must follow `Specification/runtime/api_clients/qdrant/collections_common.md`;
- constructors must reject invalid configuration through `TheoryChunksCollectionError`.
- constructors must not accept raw TOML maps or the whole crate-level `Settings` object;
- constructors must convert the supplied typed settings slices into:
  - `EmbeddingConfig`
  - `QdrantDenseCollectionConfig` or `QdrantHybridCollectionConfig`
  - `SparseStrategyConfig` when the active collection variant is hybrid
- dense constructor path requires:
  - `collection_settings.collection = CollectionSettings::Dense(...)`
- hybrid constructor path requires:
  - `collection_settings.collection = CollectionSettings::Hybrid(...)`
- a mismatch between the expected constructor path and the supplied collection variant must fail with `TheoryChunksCollectionError::InvalidRequest`.

Required settings-slice field mappings:
- `EmbeddingConfig.base_url` <- `embedding_model.url`
- `EmbeddingConfig.model_name` <- `embedding_model.name`
- `EmbeddingConfig.embedding_dimension` <- `embedding_model.dimension`
- embedding retry policy <- `collection_settings.embedding_retry`
- Qdrant retry policy <- `collection_settings.qdrant_retry`
- dense or hybrid Qdrant config fields <- `collection_settings.collection`
- `qdrant_url` <- `Settings.retrieval.qdrant_url`

Hybrid sparse-artifact compatibility rules:
- loaded `SparseVocabularyArtifact` must be compatible with the configured sparse-vocabulary artifact path;
- loaded sparse artifacts must be compatible with the selected `SparseStrategyConfig` kind;
- when `SparseStrategyConfig::Bm25Like` is selected, loaded `Bm25TermStatsArtifact.vocabulary_name` must match loaded `SparseVocabularyArtifact.vocabulary_name`;
- when `SparseStrategyConfig::Bm25Like` is selected, loaded `Bm25TermStatsArtifact.collection_name` must match `qdrant.collection_name.0`.

## 8) Payload Mapping Contract

Each raw hit payload must contain:
- `chunk_id` as string;
- `text` as string.

Mapping rules:
- `TheoryChunkSearchHit.chunk_id` must be built from payload `chunk_id`;
- empty `chunk_id` string is a payload mapping failure;
- `TheoryChunkSearchHit.score` must be built from raw Qdrant hit score;
- `TheoryChunkSearchHit.text` must be built from payload `text`;
- empty `text` string is a payload mapping failure;
- extra payload fields must be ignored.

Payload validation rules:
- missing required payload fields is a mapping failure;
- invalid field types is a mapping failure;
- unsupported payload shapes from the transport client must already have been rejected before mapping;
- shared strict-mapping rules must follow `Specification/runtime/api_clients/qdrant/collections_common.md`.

## 9) Empty-Result Rule

- zero-hit success behavior must follow `Specification/runtime/api_clients/qdrant/collections_common.md`;
- if the transport client returns zero hits, the collection implementation must return `TheoryChunkSearchResult { hits: vec![] }`.

## 10) Error Model

The module must define:

```rust
pub enum TheoryChunksCollectionError {
    InvalidRequest(&'static str),
    QueryPreparation(&'static str),
    IncorrectEmbeddingShape,
    QdrantDense(DenseSearchClientError),
    QdrantHybrid(HybridSearchClientError),
    Embedding(EmbeddingClientError),
    PayloadMapping(&'static str),
}
```

Failure-category rules:
- `InvalidRequest` covers invalid user request shape or invalid collection configuration;
- `QueryPreparation` covers embedding creation failure or sparse-vector preparation failure;
- `IncorrectEmbeddingShape` covers embedding responses whose vector length does not match `embedding.embedding_dimension`;
- both dense and hybrid implementations must use `IncorrectEmbeddingShape` for embedding-length mismatch;
- `QdrantDense(...)` wraps errors surfaced from the underlying dense Qdrant transport client;
- `QdrantHybrid(...)` wraps errors surfaced from the underlying hybrid Qdrant transport client;
- `Embedding(...)` wraps errors surfaced from the underlying embedding client;
- `PayloadMapping` covers failure to convert transport-level payload into `TheoryChunkSearchHit`.

Error-conversion rules:
- shared error-wrapping rules must follow `Specification/runtime/api_clients/qdrant/collections_common.md`;
- request validation must happen in `search()` before embedding generation or Qdrant calls.
