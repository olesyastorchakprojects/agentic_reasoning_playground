## 1) Purpose / Scope

`practice_chunks_collection` defines the collection-level runtime interface for searching practice chunks.

This module:
- exposes a domain-facing collection interface for practice-chunk search;
- accepts user query text rather than prepared vectors;
- applies practice-chunk-specific filters;
- prepares dense or hybrid query inputs depending on the concrete implementation;
- calls the corresponding Qdrant transport-level search client;
- maps raw Qdrant payloads into `PracticeChunkSearchHit`.

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
pub trait PracticeChunksCollection {
    async fn search(
        &self,
        request: &PracticeChunkSearchRequest,
    ) -> Result<PracticeChunkSearchResult, PracticeChunksCollectionError>;
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

Source of truth:
- `Specification/runtime/api_clients/qdrant/shared_types.md`
- `DenseSearchClientError` source of truth is `Specification/runtime/api_clients/qdrant/dense_search_client.md`
- `HybridSearchClientError` source of truth is `Specification/runtime/api_clients/qdrant/hybrid_search_client.md`
- `EmbeddingClientError` source of truth is `Specification/runtime/api_clients/embedding_client.md`
- shared collection-level rules are defined in `Specification/runtime/api_clients/qdrant/collections_common.md`

Collection-specific public types:

```rust
pub struct PracticeChunkFilter {
    pub card_ids: Vec<String>,
    pub chunk_tags: Vec<String>,
}

pub struct PracticeChunkSearchRequest {
    pub user_query: NormalizedUserQuery,
    pub filter: PracticeChunkFilter,
    pub limit: usize,
    pub score_threshold: f32,
}

pub struct PracticeChunkSearchHit {
    pub chunk_id: String,
    pub score: f32,
    pub card_id: Option<String>,
    pub chunk_tags: Vec<String>,
    pub text: String,
}

pub struct PracticeChunkSearchResult {
    pub hits: Vec<PracticeChunkSearchHit>,
}
```

Type rules:
- `user_query` must satisfy the rules of `NormalizedUserQuery`;
- `filter.card_ids` must not be empty in the current version;
- `filter.chunk_tags` must not be empty in the current version;
- `limit` must be greater than zero;
- negative `score_threshold` is an invalid request;
- `NaN`, `+inf`, and `-inf` threshold values are invalid requests;
- `PracticeChunkSearchResult.hits` must preserve Qdrant response order.

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
- if sparse query vector construction produces zero sparse terms after normalization and vocabulary lookup, hybrid implementation must fail with `PracticeChunksCollectionError::QueryPreparation`.

## 6) Concrete Implementations

The module must expose these concrete structures:

```rust
pub struct QdrantPracticeChunksCollectionDense {
    embedding: EmbeddingConfig,
    qdrant: QdrantDenseCollectionConfig,
    embedding_client: EmbeddingClient,
    qdrant_client: QdrantDenseSearchClient,
}

pub struct QdrantPracticeChunksCollectionHybrid {
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

### 6.1 Dense Implementation Responsibility

`QdrantPracticeChunksCollectionDense` must:
- validate `PracticeChunkSearchRequest` immediately on entry to `search`;
- create a dense embedding from `request.user_query`;
- validate that embedding length matches `embedding.embedding_dimension`;
- if the returned embedding length does not match `embedding.embedding_dimension`, fail with `PracticeChunksCollectionError::IncorrectEmbeddingShape`;
- map `PracticeChunkFilter` into `QdrantFilter`;
- build `DenseSearchRequest`;
- call `QdrantDenseSearchClient::search`;
- map `RawQdrantHit` payloads into `PracticeChunkSearchHit`.

Dense implementation validation rules:
- empty `request.user_query.0` after trimming must fail with `PracticeChunksCollectionError::InvalidRequest`;
- empty `request.filter.card_ids` must fail with `PracticeChunksCollectionError::InvalidRequest`;
- empty `request.filter.chunk_tags` must fail with `PracticeChunksCollectionError::InvalidRequest`;
- `request.limit == 0` must fail with `PracticeChunksCollectionError::InvalidRequest`;
- negative `request.score_threshold` must fail with `PracticeChunksCollectionError::InvalidRequest`;
- `NaN`, `+inf`, and `-inf` threshold values must fail with `PracticeChunksCollectionError::InvalidRequest`;
- `qdrant.vector_name = None` is allowed and means default-vector behavior for the dense transport client.

### 6.2 Hybrid Implementation Responsibility

`QdrantPracticeChunksCollectionHybrid` must:
- validate `PracticeChunkSearchRequest` immediately on entry to `search`;
- create a dense embedding from `request.user_query`;
- validate that embedding length matches `embedding.embedding_dimension`;
- if the returned embedding length does not match `embedding.embedding_dimension`, fail with `PracticeChunksCollectionError::IncorrectEmbeddingShape`;
- build a sparse vector from `request.user_query` according to `SparseStrategyConfig`;
- map `PracticeChunkFilter` into `QdrantFilter`;
- build `HybridSearchRequest`;
- call `QdrantHybridSearchClient::search`;
- map `RawQdrantHit` payloads into `PracticeChunkSearchHit`.

Hybrid implementation validation rules:
- empty `request.user_query.0` after trimming must fail with `PracticeChunksCollectionError::InvalidRequest`;
- empty `request.filter.card_ids` must fail with `PracticeChunksCollectionError::InvalidRequest`;
- empty `request.filter.chunk_tags` must fail with `PracticeChunksCollectionError::InvalidRequest`;
- `request.limit == 0` must fail with `PracticeChunksCollectionError::InvalidRequest`;
- negative `request.score_threshold` must fail with `PracticeChunksCollectionError::InvalidRequest`;
- `NaN`, `+inf`, and `-inf` threshold values must fail with `PracticeChunksCollectionError::InvalidRequest`;

## 7) Constructor Rules

Each concrete implementation must expose a public constructor.

Dense constructor:

```rust
impl QdrantPracticeChunksCollectionDense {
    pub fn new(
        embedding: EmbeddingConfig,
        qdrant: QdrantDenseCollectionConfig,
        retry_policy: RetryPolicyConfig,
    ) -> Result<Self, PracticeChunksCollectionError>;
}
```

Hybrid constructor:

```rust
impl QdrantPracticeChunksCollectionHybrid {
    pub fn new(
        embedding: EmbeddingConfig,
        qdrant: QdrantHybridCollectionConfig,
        sparse: SparseStrategyConfig,
        retry_policy: RetryPolicyConfig,
    ) -> Result<Self, PracticeChunksCollectionError>;
}
```

Constructor rules:
- shared constructor rules must follow `Specification/runtime/api_clients/qdrant/collections_common.md`;
- hybrid implementation constructor must fail if required sparse artifacts are absent or invalid;
- constructors must reject invalid configuration through `PracticeChunksCollectionError`.

Hybrid sparse-artifact compatibility rules:
- loaded `SparseVocabularyArtifact` must be compatible with the configured sparse-vocabulary artifact path;
- when `SparseStrategyConfig::Bm25Like` is selected, loaded `Bm25TermStatsArtifact.vocabulary_name` must match loaded `SparseVocabularyArtifact.vocabulary_name`;
- when `SparseStrategyConfig::Bm25Like` is selected, loaded `Bm25TermStatsArtifact.collection_name` must match `qdrant.collection_name.0`;

## 8) Filter Mapping Rules

`PracticeChunkFilter` must be mapped into `QdrantFilter` as follows:
- `card_ids` maps to one `QdrantMatchAnyFilter` with `field_name = "card_id"`;
- `chunk_tags` maps to one `QdrantMatchAnyFilter` with `field_name = "chunk_tags"`.

Structural mapping rules:
- `PracticeChunkFilter.card_ids` must map to `QdrantMatchAnyFilter { field_name: "card_id".to_string(), values: request.filter.card_ids.clone() }`;
- `PracticeChunkFilter.chunk_tags` must map to `QdrantMatchAnyFilter { field_name: "chunk_tags".to_string(), values: request.filter.chunk_tags.clone() }`.

Outbound filter JSON must therefore produce:

```json
{
  "must": [
    {
      "key": "card_id",
      "match": {
        "any": ["mongodb_4_2_6", "mysql_8_0_34"]
      }
    },
    {
      "key": "chunk_tags",
      "match": {
        "any": ["chunk_role:symptom", "chunk_role:diagnostic_step"]
      }
    }
  ]
}
```

## 9) Payload Mapping Contract

Each raw hit payload must contain:
- `chunk_id` as string;
- `chunk_tags` as array of strings;
- `text` as string.

Payload may contain:
- `card_id` as string.

Mapping rules:
- `PracticeChunkSearchHit.chunk_id` must be built from payload `chunk_id`;
- `PracticeChunkSearchHit.score` must be built from raw Qdrant hit score;
- `PracticeChunkSearchHit.card_id` must be `Some(...)` when payload contains a valid `card_id`;
- `PracticeChunkSearchHit.card_id` must be `None` when payload has no `card_id`;
- `PracticeChunkSearchHit.chunk_tags` must be built from payload `chunk_tags`;
- `PracticeChunkSearchHit.text` must be built from payload `text`;
- extra payload fields must be ignored.

Payload validation rules:
- missing required payload fields is a mapping failure;
- invalid field types is a mapping failure;
- invalid optional `card_id` field type is a mapping failure, not `None`;
- unsupported payload shapes from the transport client must already have been rejected before mapping;
- shared strict-mapping rules must follow `Specification/runtime/api_clients/qdrant/collections_common.md`.

## 10) Empty-Result Rule

- zero-hit success behavior must follow `Specification/runtime/api_clients/qdrant/collections_common.md`;
- if the transport client returns zero hits, the collection implementation must return `PracticeChunkSearchResult { hits: vec![] }`.

## 11) Error Model

The module must define:

```rust
pub enum PracticeChunksCollectionError {
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
- `QdrantDense(...)` wraps errors surfaced from the underlying dense Qdrant transport client;
- `QdrantHybrid(...)` wraps errors surfaced from the underlying hybrid Qdrant transport client;
- `Embedding(...)` wraps errors surfaced from the underlying embedding client;
- `PayloadMapping` covers failure to convert transport-level payload into `PracticeChunkSearchHit`.

Error-conversion rules:
- shared error-wrapping rules must follow `Specification/runtime/api_clients/qdrant/collections_common.md`;
- request validation must happen in `search()` before embedding generation or Qdrant calls.
