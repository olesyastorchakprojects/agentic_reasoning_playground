## 1) Purpose / Scope

This document defines shared sparse-query preparation rules for runtime hybrid Qdrant collection implementations.

This document is the source of truth for:
- query-text tokenization and normalization before sparse-vector construction;
- sparse vocabulary loading and lookup;
- sparse vector assembly rules;
- BM25 term-stats usage when BM25-like sparse weighting is selected.

This document does not define:
- dense embedding generation;
- Qdrant hybrid HTTP request shape;
- collection-specific payload mapping.

## 2) Related Contracts

Sparse query preparation must follow:
- `Specification/runtime/api_clients/qdrant/hybrid/sparse_text_space.md`
- `Specification/runtime/api_clients/qdrant/hybrid/sparse_vocabulary.md`
- `Specification/runtime/api_clients/qdrant/hybrid/sparse_strategies/bag_of_words.md`
- `Specification/runtime/api_clients/qdrant/hybrid/sparse_strategies/bm25_like.md`
- `Specification/runtime/api_clients/qdrant/hybrid/bm25_term_stats.md`

Configuration types are defined in:
- `Specification/runtime/api_clients/qdrant/shared_types.md`

## 3) Shared Sparse Query Preparation Rules

Hybrid collection implementations must build sparse vectors from the unchanged user-query text.

Rules:
- the source text for sparse-vector construction must be the unchanged `NormalizedUserQuery.0` string;
- hybrid collection implementations must tokenize and normalize query text according to the sparse text-space contract;
- `SparseStrategyConfig` paths are repository-root-relative artifact paths;
- hybrid collection implementations must resolve sparse artifact paths against repository root before opening them;
- hybrid collection implementations must load the sparse vocabulary artifact from the configured sparse-vocabulary path;
- hybrid collection implementations must load the tokenizer according to the configured `tokenizer.source` recorded in the sparse vocabulary artifact and the utility contract defined in `Specification/runtime/utils/tokenizer.md`;
- hybrid collection implementations must map canonical query tokens through that vocabulary;
- hybrid collection implementations must ignore out-of-vocabulary tokens in the current version;
- sparse query vector shape must contain aligned `indices` and `values`;
- `indices` must be sorted ascending;
- sparse query vector weighting must follow the concrete strategy selected in `SparseStrategyConfig`;
- if sparse query vector construction produces zero sparse terms after normalization and vocabulary lookup, the implementation must fail with its collection-level query-preparation error;
- when `SparseStrategyConfig::Bm25Like` is selected, the implementation must load the BM25 term-stats artifact from the configured BM25 term-stats path;
- BM25 term-stats loading and validation must follow `bm25_term_stats.md`.
