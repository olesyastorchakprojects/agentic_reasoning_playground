# Ingest Config Contract

This document defines the contract for:
- `Execution/distributed_diagnostics/ingest.toml`

## Format

`ingest.toml` is a TOML ingest-compatibility config file.

It is the source of truth for:
- embedding compatibility settings shared by retrieval collections;
- collection kind selection (`dense` or `hybrid`);
- collection names and vector names;
- sparse tokenizer and preprocessing settings;
- sparse strategy selection and sparse-strategy-specific settings.

Invalid `ingest.toml`:
- `ingest.toml` is invalid if TOML does not parse, if a required section or field is missing, if a value type does not match the contract, or if a value violates the constraints of this contract;
- this is a startup error;
- runtime initialization must fail before request processing begins.

## Expected Structure

`[pipeline]`
- `ingest_config_version`: version string for the current ingest config shape

`[embedding.model]`
- `name`: embedding model name shared by runtime retrieval
- `dimension`: embedding vector dimension shared by runtime retrieval

`[qdrant.collections.cards]`
- `kind`: active retrieval mode for cards; allowed values:
  - `dense`
  - `hybrid`
- `corpus_version`

`[qdrant.collections.cards.dense]`
- `name`
- `vector_name`

`[qdrant.collections.cards.hybrid]`
- `dense_vector_name`
- `sparse_vector_name`

`[qdrant.collections.cards.hybrid.sparse.strategy]`
- `kind`: active sparse strategy; allowed values:
  - `bag_of_words`
  - `bm25_like`

`[qdrant.collections.cards.hybrid.sparse.tokenizer]`
- `library`
- `source`

`[qdrant.collections.cards.hybrid.sparse.preprocessing]`
- `kind`
- `lowercase`
- `min_token_length`

`[qdrant.collections.cards.hybrid.sparse.bag_of_words]`
- `name`
- `query`
- `sparse_vocabulary_path`

`[qdrant.collections.cards.hybrid.sparse.bm25_like]`
- `name`
- `query`
- `sparse_vocabulary_path`
- `bm25_term_stats_path`
- `k1`
- `b`
- `idf_smoothing`

The same structure applies to:
- `qdrant.collections.practice`
- `qdrant.collections.theory`

## Semantics Of Config Values

`embedding.model.name`
- identifies the embedding model whose output space must remain compatible with runtime retrieval.

`embedding.model.dimension`
- dense vector length expected by retrieval clients and collections.

`qdrant.collections.<collection>.kind`
- selects the active collection variant in resolved runtime settings;
- `dense` must resolve into `CollectionSettings::Dense(...)`;
- `hybrid` must resolve into `CollectionSettings::Hybrid(...)`.

`qdrant.collections.<collection>.corpus_version`
- version identifier of the corpus behind that logical collection;
- this is collection-local because different runtime collections may be built from different sources.

`qdrant.collections.<collection>.dense.name`
- physical Qdrant collection name used for dense retrieval for the given logical collection.

`qdrant.collections.<collection>.hybrid.dense_vector_name`
- dense vector field name used inside hybrid retrieval requests.

`qdrant.collections.<collection>.hybrid.sparse_vector_name`
- sparse vector field name used inside hybrid retrieval requests.

`qdrant.collections.<collection>.hybrid.sparse.strategy.kind`
- selects the active sparse strategy in resolved runtime settings;
- `bag_of_words` must resolve into `SparseStrategySettings::BagOfWords(...)`;
- `bm25_like` must resolve into `SparseStrategySettings::Bm25Like(...)`.

`qdrant.collections.<collection>.hybrid.sparse.tokenizer.library`
- tokenizer implementation family used for sparse tokenization;
- for the current version, `tokenizers` is the supported value.

`qdrant.collections.<collection>.hybrid.sparse.tokenizer.source`
- tokenizer identity string used by runtime tokenizer initialization.

`qdrant.collections.<collection>.hybrid.sparse.preprocessing`
- defines sparse token normalization rules shared by all sparse strategies of the given collection.

`qdrant.collections.<collection>.hybrid.sparse.bag_of_words.name`
- physical Qdrant collection name used when the active sparse strategy is `bag_of_words`.

`qdrant.collections.<collection>.hybrid.sparse.bag_of_words.sparse_vocabulary_path`
- repository-relative or runtime-readable path to the sparse vocabulary artifact used by `bag_of_words`.

`qdrant.collections.<collection>.hybrid.sparse.bm25_like.name`
- physical Qdrant collection name used when the active sparse strategy is `bm25_like`.

`qdrant.collections.<collection>.hybrid.sparse.bm25_like.sparse_vocabulary_path`
- repository-relative or runtime-readable path to the sparse vocabulary artifact used by `bm25_like`.

`qdrant.collections.<collection>.hybrid.sparse.bm25_like.bm25_term_stats_path`
- repository-relative or runtime-readable path to the BM25 term-stats artifact used by `bm25_like`.

`qdrant.collections.<collection>.hybrid.sparse.bm25_like.k1`
`qdrant.collections.<collection>.hybrid.sparse.bm25_like.b`
`qdrant.collections.<collection>.hybrid.sparse.bm25_like.idf_smoothing`
- runtime parameters for BM25-like sparse query weighting.

## Ownership Rules

`ingest.toml` owns:
- embedding compatibility settings;
- collection kind selection;
- collection names and vector names;
- collection-local `corpus_version`;
- sparse tokenizer settings;
- sparse preprocessing settings;
- sparse strategy settings.
- sparse artifact paths.

`ingest.toml` must not redefine:
- per-collection `top_k`;
- per-collection `score_threshold`;
- runtime retry settings;
- active model transport;
- model transport timeout and retry settings.

Those fields belong to:
- `Execution/distributed_diagnostics/runtime.toml`
