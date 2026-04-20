## 1) Purpose / Scope

This document defines the mandatory generated unit-test contract for the Qdrant runtime API-client slice.

This document is the single source of truth for:
- required module-level unit-test cases for Qdrant runtime API-client modules;
- required Qdrant transport-client request-shape assertions;
- required collection-level mapping and validation tests.

This document must be read together with:
- `Specification/runtime/unit_tests_common.md`

Code generation for the Qdrant runtime API-client slice is incomplete if the generated Rust source omits any required unit tests defined by this document.

## 2) Covered Modules

The current Qdrant runtime API-client test scope covers:

- `embedding_client`
- `qdrant.dense_search_client`
- `qdrant.hybrid_search_client`
- `qdrant.cards_collection`
- `qdrant.practice_chunks_collection`
- `qdrant.theory_chunks_collection`
- `qdrant.sparse_preparation`

## 3) Required Unit Tests By Module

### 3.1) `embedding_client`

Generated unit tests for `embedding_client` must include all of the following cases:

- valid embedding-service response with exactly one embedding of the configured dimension returns `Embedding`;
- request body sent to the mock embedding server contains exactly:
  - `model = EmbeddingConfig.model_name`
  - `input` contains exactly one string
  - that string equals the unchanged `NormalizedUserQuery.0`;
- HTTP or network transport failure returns the exact `EmbeddingClientError::Transport` variant;
- non-2xx HTTP response returns the exact `EmbeddingClientError::UnexpectedStatus` variant;
- response body without `embeddings` returns the exact `EmbeddingClientError::InvalidResponse` variant;
- response body with zero embeddings returns the exact `EmbeddingClientError::InvalidResponse` variant;
- response body with more than one embedding returns the exact `EmbeddingClientError::InvalidResponse` variant;
- returned embedding with wrong dimension returns the exact `EmbeddingClientError::InvalidResponse` variant;
- returned embedding containing invalid float values returns the exact `EmbeddingClientError::InvalidResponse` variant;
- constructor fails when `EmbeddingConfig.base_url` has invalid runtime shape according to the spec;
- constructor fails when `retry_policy.max_attempts == 0`.

### 3.2) `qdrant.dense_search_client`

Generated unit tests for `QdrantDenseSearchClient` must include all of the following cases:

- successful dense search response returns `DenseSearchResponse`, preserving Qdrant hit order in `hits`;
- request body sent to the mock Qdrant server contains exactly:
  - `query = Embedding.values`
  - `limit`
  - `score_threshold`
  - `with_payload = true`
  - `with_vector = false`;
- when `vector_name = None`, outbound request omits `using`;
- when `vector_name = Some(...)`, outbound request sets `using` to that value;
- when `filter = None`, outbound request omits `filter`;
- when `filter = Some(...)`, outbound request encodes `QdrantFilter` into the exact Qdrant `filter.must` JSON shape;
- empty result returns `DenseSearchResponse { hits: vec![] }`;
- HTTP or network transport failure returns the exact `DenseSearchClientError::Transport` variant;
- non-2xx HTTP response returns the exact `DenseSearchClientError::UnexpectedStatus` variant;
- response without `result.points` returns the exact `DenseSearchClientError::InvalidResponse` variant;
- hit without `score` returns the exact `DenseSearchClientError::InvalidResponse` variant;
- hit without `payload` returns the exact `DenseSearchClientError::InvalidResponse` variant;
- unsupported payload shape returns the exact `DenseSearchClientError::InvalidResponse` variant;
- constructor fails when `retry_policy.max_attempts == 0`.

### 3.3) `qdrant.hybrid_search_client`

Generated unit tests for `QdrantHybridSearchClient` must include all of the following cases:

- successful hybrid search response returns `HybridSearchResponse`, preserving Qdrant fused hit order in `hits`;
- request body sent to the mock Qdrant server contains exactly:
  - top-level `query = { "fusion": "rrf" }`
  - `prefetch` contains exactly two branches
  - dense prefetch branch is first
  - sparse prefetch branch is second
  - dense prefetch uses `vector_name`
  - sparse prefetch uses `sparse_vector_name`
  - both prefetch branch limits equal top-level `limit`
  - top-level `with_payload = true`
  - top-level `with_vector = false`;
- when `filter = None`, outbound request omits `filter`;
- when `filter = Some(...)`, outbound request encodes `QdrantFilter` into the exact Qdrant `filter.must` JSON shape;
- empty result returns `HybridSearchResponse { hits: vec![] }`;
- HTTP or network transport failure returns the exact `HybridSearchClientError::Transport` variant;
- non-2xx HTTP response returns the exact `HybridSearchClientError::UnexpectedStatus` variant;
- response without `result.points` returns the exact `HybridSearchClientError::InvalidResponse` variant;
- hit without `score` returns the exact `HybridSearchClientError::InvalidResponse` variant;
- hit without `payload` returns the exact `HybridSearchClientError::InvalidResponse` variant;
- unsupported payload shape returns the exact `HybridSearchClientError::InvalidResponse` variant;
- constructor fails when `retry_policy.max_attempts == 0`.

### 3.4) `qdrant.cards_collection`

Generated unit tests for `CardsCollection` implementations must include all of the following cases:

- dense implementation validates request before embedding generation;
- hybrid implementation validates request before embedding generation;
- empty `user_query` fails with the exact `CardsCollectionError::InvalidRequest` variant;
- `limit == 0` fails with the exact `CardsCollectionError::InvalidRequest` variant;
- invalid threshold values fail with the exact `CardsCollectionError::InvalidRequest` variant;
- embedding length mismatch fails with the exact `CardsCollectionError::IncorrectEmbeddingShape` variant before any Qdrant request is sent;
- dense implementation builds `DenseSearchRequest` with `filter = None`;
- hybrid implementation builds `HybridSearchRequest` with `filter = None`;
- dense transport error is wrapped into the exact `CardsCollectionError::QdrantDense(...)` variant;
- hybrid transport error is wrapped into the exact `CardsCollectionError::QdrantHybrid(...)` variant;
- embedding client error is wrapped into the exact `CardsCollectionError::Embedding(...)` variant;
- payload with valid `doc_id` maps into `CardSearchHit.case_id`;
- payload with empty `doc_id` fails with the exact `CardsCollectionError::PayloadMapping` variant;
- extra payload fields are ignored;
- one invalid hit fails the whole mapping step;
- empty transport result returns `CardSearchResult { hits: vec![] }`;
- hybrid constructor fails when required sparse artifacts are absent or invalid.

### 3.5) `qdrant.practice_chunks_collection`

Generated unit tests for `PracticeChunksCollection` implementations must include all of the following cases:

- dense implementation validates request before embedding generation;
- hybrid implementation validates request before embedding generation;
- empty `user_query` fails with the exact `PracticeChunksCollectionError::InvalidRequest` variant;
- empty `case_ids` fails with the exact `PracticeChunksCollectionError::InvalidRequest` variant;
- empty `chunk_tags` fails with the exact `PracticeChunksCollectionError::InvalidRequest` variant;
- `limit == 0` fails with the exact `PracticeChunksCollectionError::InvalidRequest` variant;
- invalid threshold values fail with the exact `PracticeChunksCollectionError::InvalidRequest` variant;
- embedding length mismatch fails with the exact `PracticeChunksCollectionError::IncorrectEmbeddingShape` variant before any Qdrant request is sent;
- dense implementation maps `PracticeChunkFilter` into the exact expected `QdrantFilter`;
- hybrid implementation maps `PracticeChunkFilter` into the exact expected `QdrantFilter`;
- dense transport error is wrapped into the exact `PracticeChunksCollectionError::QdrantDense(...)` variant;
- hybrid transport error is wrapped into the exact `PracticeChunksCollectionError::QdrantHybrid(...)` variant;
- embedding client error is wrapped into the exact `PracticeChunksCollectionError::Embedding(...)` variant;
- valid payload maps into `PracticeChunkSearchHit`;
- payload with valid `doc_id` maps into `PracticeChunkSearchHit.case_id`;
- payload with empty `doc_id` fails with the exact `PracticeChunksCollectionError::PayloadMapping` variant;
- invalid `doc_id` type fails with the exact `PracticeChunksCollectionError::PayloadMapping` variant;
- extra payload fields are ignored;
- one invalid hit fails the whole mapping step;
- empty transport result returns `PracticeChunkSearchResult { hits: vec![] }`;
- hybrid constructor fails when required sparse artifacts are absent or invalid;
- hybrid constructor validates sparse-artifact compatibility according to the spec.

### 3.6) `qdrant.theory_chunks_collection`

Generated unit tests for `TheoryChunksCollection` implementations must include all of the following cases:

- dense implementation validates request before embedding generation;
- hybrid implementation validates request before embedding generation;
- empty `user_query` fails with the exact `TheoryChunksCollectionError::InvalidRequest` variant;
- `limit == 0` fails with the exact `TheoryChunksCollectionError::InvalidRequest` variant;
- invalid threshold values fail with the exact `TheoryChunksCollectionError::InvalidRequest` variant;
- embedding length mismatch fails with the exact `TheoryChunksCollectionError::IncorrectEmbeddingShape` variant before any Qdrant request is sent;
- dense implementation builds `DenseSearchRequest` with `filter = None`;
- hybrid implementation builds `HybridSearchRequest` with `filter = None`;
- dense transport error is wrapped into the exact `TheoryChunksCollectionError::QdrantDense(...)` variant;
- hybrid transport error is wrapped into the exact `TheoryChunksCollectionError::QdrantHybrid(...)` variant;
- embedding client error is wrapped into the exact `TheoryChunksCollectionError::Embedding(...)` variant;
- valid payload maps into `TheoryChunkSearchHit`;
- empty `chunk_id` fails with the exact `TheoryChunksCollectionError::PayloadMapping` variant;
- empty `text` fails with the exact `TheoryChunksCollectionError::PayloadMapping` variant;
- extra payload fields are ignored;
- one invalid hit fails the whole mapping step;
- empty transport result returns `TheoryChunkSearchResult { hits: vec![] }`;
- hybrid constructor fails when required sparse artifacts are absent or invalid;
- hybrid constructor validates sparse-artifact compatibility according to the spec.

### 3.7) `qdrant.sparse_preparation`

Generated unit tests for `qdrant.sparse_preparation` must include all of the following cases:

- loading sparse artifacts for `bag_of_words` succeeds when tokenizer config, tokenizer artifact, and vocabulary artifact are valid and mutually compatible;
- tokenizer utility behavior such as cache reuse and download/caching must follow `Specification/runtime/utils/tokenizer.md` and does not need to be redefined by this document;
- loading sparse artifacts fails when tokenizer library is unsupported for the current runtime contract;
- loading sparse artifacts for `bm25_like` fails when BM25 term-stats artifacts are missing when required by the effective sparse strategy;
- loading sparse artifacts for `bm25_like` fails when BM25 term-stats metadata is incompatible with the expected vocabulary identity or collection identity according to the spec;
- sparse tokenization discards unknown placeholder tokens such as `[UNK]` and `unk` during sparse query preparation;
- sparse tokenization preserves configured normalization semantics used by the runtime sparse text-space contract;
- `bag_of_words` sparse-vector construction deduplicates repeated retained vocabulary terms;
- sparse-vector construction sorts emitted token ids ascending and keeps `indices` and `values` aligned;
- sparse-vector construction fails when no canonical tokens remain after normalization and vocabulary lookup;
- `bm25_like` sparse-vector construction succeeds when compatible corpus statistics are available and produces a non-empty aligned sparse vector;
- `bm25_like` sparse-vector construction fails when required corpus statistics are absent.
