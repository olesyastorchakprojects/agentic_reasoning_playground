## 1) Purpose / Scope

This document defines shared collection-level rules for Qdrant-backed runtime collections.

It applies to:
- `cards_collection.md`
- `practice_chunks_collection.md`
- `theory_chunks_collection.md`

This document defines:
- shared implementation dependencies;
- shared constructor behavior;
- shared request-validation timing;
- shared embedding and sparse-artifact lifecycle rules;
- shared transport-client ownership rules;
- shared error-wrapping rules;
- shared strict-mapping rules;
- shared zero-hit success behavior.

This document does not define:
- collection-specific request/result shapes;
- collection-specific payload mapping;
- collection-specific filters.

## 2) Shared Dependencies

Collection-level Qdrant modules depend on:
- `Specification/runtime/api_clients/embedding_client.md`
- `Specification/runtime/api_clients/qdrant/dense_search_client.md`
- `Specification/runtime/api_clients/qdrant/hybrid_search_client.md`
- `Specification/runtime/api_clients/qdrant/shared_types.md`
- `Specification/runtime/api_clients/qdrant/hybrid/sparse_query_preparation.md`

## 3) Shared Construction Rules

Collection implementations must create and own their internal helper clients in the constructor.

Rules:
- dense collection implementations must create and own `EmbeddingClient`;
- hybrid collection implementations must create and own `EmbeddingClient`;
- dense collection implementations must create and own `QdrantDenseSearchClient`;
- hybrid collection implementations must create and own `QdrantHybridSearchClient`;
- created internal clients must be held for the full lifetime of the collection implementation;
- `search()` must not recreate transport clients or embedding client per request.

## 4) Shared Sparse-Artifact Lifecycle Rules

Hybrid collection implementations must load sparse artifacts in the constructor.

Rules:
- sparse artifacts must be loaded once in constructor logic;
- loaded sparse artifacts must be cached for the full lifetime of the collection implementation;
- hybrid collection implementations must not reread sparse artifacts from disk during `search()`;
- when `SparseStrategyConfig::BagOfWords` is selected, `bm25_term_stats` must be `None`;
- when `SparseStrategyConfig::Bm25Like` is selected, `bm25_term_stats` must be `Some(...)`;
- when `SparseStrategyConfig::BagOfWords` is selected, the constructor must not attempt to load BM25 term stats;
- constructor logic must fail if required sparse artifacts are absent or invalid.

## 5) Shared Request Validation Rules

Collection implementations must validate incoming request objects immediately on entry to `search()`.

Rules:
- request validation must happen before embedding generation or Qdrant calls;
- invalid request shape must fail with the collection-level `InvalidRequest` variant;
- embedding-shape mismatch must fail with the collection-level `IncorrectEmbeddingShape` variant;
- on incorrect embedding shape, the implementation must fail before building or executing any Qdrant search request.

## 6) Shared Error-Wrapping Rules

Collection-level modules must wrap service-layer client errors rather than flatten them into strings.

Rules:
- `DenseSearchClientError` must be converted into a collection-level dense-Qdrant error variant;
- `HybridSearchClientError` must be converted into a collection-level hybrid-Qdrant error variant;
- `EmbeddingClientError` must be converted into a collection-level embedding error variant.

## 7) Shared Mapping Rules

Collection-level mapping must be strict.

Rules:
- one invalid hit must fail the entire mapping step;
- partial success by dropping invalid hits is forbidden;
- unread payload fields must be ignored by collection-level mapping;
- this remains true even when an unread field would have an unsupported shape, as long as the transport client did not need to parse it into a typed value.

## 8) Shared Empty-Result Rule

- zero hits is a valid successful result.
