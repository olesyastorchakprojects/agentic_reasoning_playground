## 1) Purpose / Scope

This document defines the recommended code-generation order for the Qdrant runtime API-client specification set.

It exists to make generation dependency-aware.

This document defines:
- which specification files belong to the Qdrant runtime API-client slice;
- what code artifacts should be generated from them;
- the recommended generation order;
- when unit-test generation should run.

This document does not define:
- runtime behavior contracts;
- payload mapping rules;
- business orchestration outside the Qdrant API-client layer.

## 2) Qdrant Spec Set

The current Qdrant runtime API-client specification set consists of:

- `Specification/runtime/api_clients/client_common.md`
- `Specification/runtime/api_clients/embedding_client.md`
- `Specification/runtime/unit_tests_common.md`
- `Specification/runtime/api_clients/qdrant/unit_tests.md`
- `Specification/runtime/api_clients/qdrant/shared_types.md`
- `Specification/runtime/api_clients/qdrant/collections_common.md`
- `Specification/runtime/api_clients/qdrant/dense_search_client.md`
- `Specification/runtime/api_clients/qdrant/hybrid_search_client.md`
- `Specification/runtime/api_clients/qdrant/cards_collection.md`
- `Specification/runtime/api_clients/qdrant/practice_chunks_collection.md`
- `Specification/runtime/api_clients/qdrant/theory_chunks_collection.md`
- `Specification/runtime/api_clients/qdrant/hybrid/sparse_query_preparation.md`
- `Specification/runtime/api_clients/qdrant/hybrid/sparse_text_space.md`
- `Specification/runtime/api_clients/qdrant/hybrid/sparse_vocabulary.md`
- `Specification/runtime/api_clients/qdrant/hybrid/sparse_strategies/bag_of_words.md`
- `Specification/runtime/api_clients/qdrant/hybrid/sparse_strategies/bm25_like.md`
- `Specification/runtime/api_clients/qdrant/hybrid/bm25_term_stats.md`

## 3) Target Generated Files

The Qdrant runtime API-client slice should generate code for these Rust files first:

- `distributed_diagnostics::api_clients::embedding_client`
- `distributed_diagnostics::api_clients::qdrant::shared_types`
- `distributed_diagnostics::api_clients::qdrant::dense_search_client`
- `distributed_diagnostics::api_clients::qdrant::hybrid_search_client`
- `distributed_diagnostics::api_clients::qdrant::cards_collection`
- `distributed_diagnostics::api_clients::qdrant::practice_chunks_collection`
- `distributed_diagnostics::api_clients::qdrant::theory_chunks_collection`

The current version does not require generating dedicated Rust modules from:
- `client_common.md`
- `collections_common.md`
- `unit_tests_common.md`
- `qdrant/unit_tests.md`
- hybrid sparse-support documents

Those files are dependency and behavior contracts used while generating the modules above.

## 4) Recommended Generation Order

Generate in this order:

1. `shared_types`
2. `embedding_client`
3. `dense_search_client`
4. `hybrid_search_client`
5. `cards_collection`
6. `practice_chunks_collection`
7. `theory_chunks_collection`
8. generate unit tests for all modules above using:
   - `Specification/runtime/unit_tests_common.md`
   - `Specification/runtime/api_clients/qdrant/unit_tests.md`

## 5) Why This Order

### Step 1: `shared_types`

Generate first because it provides:
- shared newtypes;
- shared config structs;
- shared payload and artifact structs.

All other Qdrant modules depend on these types.

### Step 2: `embedding_client`

Generate second because collection implementations depend on:
- `EmbeddingClient`
- `EmbeddingClientError`

### Step 3: `dense_search_client`

Generate third because dense-backed collection implementations depend on:
- `QdrantDenseSearchClient`
- `DenseSearchClientError`

### Step 4: `hybrid_search_client`

Generate fourth because hybrid-backed collection implementations depend on:
- `QdrantHybridSearchClient`
- `HybridSearchClientError`

### Step 5-7: Collection Modules

Generate collection modules only after shared, embedding, and transport layers exist.

Order among collection modules:
- `cards_collection` first because it has the smallest payload contract;
- `practice_chunks_collection` second because it is the richest collection contract and exercises filter mapping;
- `theory_chunks_collection` third because it is structurally similar to practice chunks but simpler.

## 6) Generation Rules

Rules:
- generation must respect type dependencies from earlier steps;
- collection modules must import generated shared and client modules, not regenerate duplicate local types;
- behavior-only spec files must be treated as source-of-truth references during generation, not as standalone module outputs;
- unit tests must be generated only after the corresponding runtime modules already exist;
- generated collection modules must preserve the layering defined by the specs:
  - embedding client layer
  - Qdrant transport client layer
  - collection layer
