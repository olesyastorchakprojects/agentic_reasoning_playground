## 1) Purpose / Scope

`cards_collection` defines the collection-level runtime interface for searching incident cards.

This module:
- exposes a domain-facing collection interface for incident-card search;
- accepts user query text rather than prepared vectors;
- prepares dense or hybrid query inputs depending on the concrete implementation;
- calls the corresponding Qdrant transport-level search client;
- maps raw Qdrant payloads into `CardSearchHit`.

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
pub trait CardsCollection {
    async fn search(
        &self,
        request: &CardSearchRequest,
    ) -> Result<CardSearchResult, CardsCollectionError>;
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
pub struct CardSearchRequest {
    pub user_query: NormalizedUserQuery,
    pub limit: usize,
    pub score_threshold: f32,
}

pub struct CardSearchHit {
    pub card_id: String,
    pub score: f32,
}

pub struct CardSearchResult {
    pub hits: Vec<CardSearchHit>,
}
```

Type rules:
- `user_query` must satisfy the rules of `NormalizedUserQuery`;
- `limit` must be greater than zero;
- negative `score_threshold` is an invalid request;
- `NaN`, `+inf`, and `-inf` threshold values are invalid requests;
- `CardSearchResult.hits` must preserve Qdrant response order.
- the current version does not support collection-level filters for card search requests.

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
- if sparse query vector construction produces zero sparse terms after normalization and vocabulary lookup, hybrid implementation must fail with `CardsCollectionError::QueryPreparation`.

## 6) Concrete Implementations

The module must expose these concrete structures:

```rust
pub struct QdrantCardsCollectionDense {
    embedding: EmbeddingConfig,
    qdrant: QdrantDenseCollectionConfig,
    embedding_client: EmbeddingClient,
    qdrant_client: QdrantDenseSearchClient,
}

pub struct QdrantCardsCollectionHybrid {
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

`QdrantCardsCollectionDense` must:
- validate `CardSearchRequest` immediately on entry to `search`;
- create a dense embedding from `request.user_query`;
- validate that embedding length matches `embedding.embedding_dimension`;
- if the returned embedding length does not match `embedding.embedding_dimension`, fail with `CardsCollectionError::IncorrectEmbeddingShape`;
- build `DenseSearchRequest`;
- build `DenseSearchRequest` with `filter = None`;
- call `QdrantDenseSearchClient::search`;
- map `RawQdrantHit` payloads into `CardSearchHit`.

Dense implementation validation rules:
- empty `request.user_query.0` after trimming must fail with `CardsCollectionError::InvalidRequest`;
- `request.limit == 0` must fail with `CardsCollectionError::InvalidRequest`;
- negative `request.score_threshold` must fail with `CardsCollectionError::InvalidRequest`;
- `NaN`, `+inf`, and `-inf` threshold values must fail with `CardsCollectionError::InvalidRequest`;
- `qdrant.vector_name = None` is allowed and means default-vector behavior for the dense transport client.

### 6.2 Hybrid Implementation Responsibility

`QdrantCardsCollectionHybrid` must:
- validate `CardSearchRequest` immediately on entry to `search`;
- create a dense embedding from `request.user_query`;
- validate that embedding length matches `embedding.embedding_dimension`;
- if the returned embedding length does not match `embedding.embedding_dimension`, fail with `CardsCollectionError::IncorrectEmbeddingShape`;
- build a sparse vector from `request.user_query` according to `SparseStrategyConfig`;
- build `HybridSearchRequest`;
- build `HybridSearchRequest` with `filter = None`;
- call `QdrantHybridSearchClient::search`;
- map `RawQdrantHit` payloads into `CardSearchHit`.

Hybrid implementation validation rules:
- empty `request.user_query.0` after trimming must fail with `CardsCollectionError::InvalidRequest`;
- `request.limit == 0` must fail with `CardsCollectionError::InvalidRequest`;
- negative `request.score_threshold` must fail with `CardsCollectionError::InvalidRequest`;
- `NaN`, `+inf`, and `-inf` threshold values must fail with `CardsCollectionError::InvalidRequest`.

## 7) Constructor Rules

Each concrete implementation must expose a public constructor that accepts typed
settings slices rather than raw TOML values or whole crate-level `Settings`.

Dense constructor:

```rust
impl QdrantCardsCollectionDense {
    pub fn from_settings(
        collection_settings: &CollectionRetrievalSettings,
        embedding_model: &EmbeddingModelSettings,
        qdrant_url: &str,
    ) -> Result<Self, CardsCollectionError>;
}
```

Hybrid constructor:

```rust
impl QdrantCardsCollectionHybrid {
    pub fn from_settings(
        collection_settings: &CollectionRetrievalSettings,
        embedding_model: &EmbeddingModelSettings,
        qdrant_url: &str,
    ) -> Result<Self, CardsCollectionError>;
}
```

Constructor rules:
- shared constructor rules must follow `Specification/runtime/api_clients/qdrant/collections_common.md`;
- hybrid implementation constructor must fail if required sparse artifacts are absent or invalid;
- constructors must reject invalid configuration through `CardsCollectionError`.
- constructors must not accept raw TOML maps or the whole crate-level `Settings` object;
- constructors must convert the supplied typed settings slices into:
  - `EmbeddingConfig`
  - `QdrantDenseCollectionConfig` or `QdrantHybridCollectionConfig`
  - `SparseStrategyConfig` when the active collection variant is hybrid
- dense constructor path requires:
  - `collection_settings.collection = CollectionSettings::Dense(...)`
- hybrid constructor path requires:
  - `collection_settings.collection = CollectionSettings::Hybrid(...)`
- a mismatch between the expected constructor path and the supplied collection variant must fail with `CardsCollectionError::InvalidRequest`.

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
- when `SparseStrategyConfig::Bm25Like` is selected, loaded `Bm25TermStatsArtifact.vocabulary_name` must match loaded `SparseVocabularyArtifact.vocabulary_name`;
- when `SparseStrategyConfig::Bm25Like` is selected, loaded `Bm25TermStatsArtifact.collection_name` must match `qdrant.collection_name.0`.

## 8) Payload Mapping Contract

Each raw hit payload must contain:
- `card_id` as string.

Mapping rules:
- `CardSearchHit.card_id` must be built from payload `card_id`;
- empty `card_id` string is a payload mapping failure;
- `CardSearchHit.score` must be built from raw Qdrant hit score;
- extra payload fields must be ignored.

Payload validation rules:
- missing required payload fields is a mapping failure;
- invalid field types is a mapping failure;
- unsupported payload shapes from the transport client must already have been rejected before mapping;
- shared strict-mapping rules must follow `Specification/runtime/api_clients/qdrant/collections_common.md`.

## 9) Empty-Result Rule

- zero-hit success behavior must follow `Specification/runtime/api_clients/qdrant/collections_common.md`;
- if the transport client returns zero hits, the collection implementation must return `CardSearchResult { hits: vec![] }`.

## 10) Error Model

The module must define:

```rust
pub enum CardsCollectionError {
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
- `PayloadMapping` covers failure to convert transport-level payload into `CardSearchHit`.

Error-conversion rules:
- shared error-wrapping rules must follow `Specification/runtime/api_clients/qdrant/collections_common.md`;
- request validation must happen in `search()` before embedding generation or Qdrant calls.
