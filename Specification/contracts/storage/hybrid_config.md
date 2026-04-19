# Hybrid Ingest Config Contract

This document defines the contract for `CONFIG_PATH` used by `hybrid_ingest.py`.

## Purpose

`CONFIG_PATH` is the source of truth for hybrid-ingest behavior.
It extends dense-ingest config with sparse-space configuration, hybrid Qdrant
collection naming, and artifact paths for singleton vocabulary plus run
manifest.

## Expected Structure

`[pipeline]`
- `name`
- `chunk_schema_version`
- `ingest_config_version`
- `corpus_version`
- `chunking_strategy`

`[embedding.model]`
- `name`
- `dimension`

`[embedding.input]`
- `text_source`

`[embedding.transport]`
- `timeout_sec`
- `max_batch_size`

`[embedding.retry]`
- `max_attempts`
- `backoff`

`[sparse.strategy]`
- `kind`
- `version`

`[sparse.input]`
- `text_source`

`[sparse.tokenizer]`
- `library`
- `source`
- `revision`, when present

`[sparse.preprocessing]`
- `kind`
- `lowercase`
- `min_token_length`

`[sparse.bag_of_words]`
- `document`
- `query`

`[sparse.bm25_like]`
- `document`
- `query`
- `k1`
- `b`
- `idf_smoothing`

`[artifacts]`
- `manifest_path`

`[qdrant.collection]`
- `name`
- `distance`
- `dense_vector_name`
- `sparse_vector_name`
- `create_if_missing`

`[qdrant.point_id]`
- `strategy`
- `namespace_uuid`
- `format`

`[qdrant.transport]`
- `timeout_sec`
- `upsert_batch_size`

`[qdrant.retry]`
- `max_attempts`
- `backoff`

`[idempotency]`
- `strategy`
- `fingerprint_fields`
- `on_fingerprint_change`
- `on_metadata_change`

`[logging]`
- `failed_chunk_log_path`
- `skipped_chunk_log_path`

## Fixed Allowed Values For Current Version

### `sparse.strategy.kind`

For the current version:

- allowed values:
  - `bag_of_words`
  - `bm25_like`

Execution semantics are defined by:

- `bag_of_words`:
  - `Specification/contracts/ingest/common/sparse_strategies/bag_of_words.md`
- `bm25_like`:
  - `Specification/contracts/ingest/common/sparse_strategies/bm25_like.md`

Required library for `bm25_like` is defined by:

- `Specification/contracts/ingest/common/sparse_strategies/bm25_like.md`

### `sparse.strategy.version`

For the current version:

- allowed value: `v1`

Meaning:

- current stable sparse-space contract version for tokenizer, normalization,
  vocabulary bootstrap, and weight assembly.

### `sparse.tokenizer.library`

For the current version:

- allowed value: `tokenizers`

Meaning:

- sparse text segmentation must use a Hugging Face compatible tokenizer loaded
  through the Python `tokenizers` library.

### `sparse.tokenizer.source`

- non-empty tokenizer artifact source identifier

### `sparse.tokenizer.revision`

- optional tokenizer artifact revision identifier

### `sparse.preprocessing.kind`

For the current version:

- allowed value: `basic_word_v1`

Its semantics are defined in:

- `Specification/contracts/ingest/common/sparse_text_space.md`

### `sparse.bag_of_words`

For the current version:

- this block is required when `sparse.strategy.kind = "bag_of_words"`

Expected fields:

- `document`
- `query`

Rules:

- `document` must be `term_frequency`
- `query` must be `binary_presence`
- this block must not coexist with `[sparse.bm25_like]`

### `sparse.bm25_like`

This block is required when `sparse.strategy.kind = "bm25_like"`.

Expected fields:

- `document`
- `query`
- `k1`
- `b`
- `idf_smoothing`

Rules:

- `document` must be `bm25_document_weight`
- `query` must be `bm25_query_weight`
- `k1` must be a positive float
- `b` must be a float in the inclusive range `[0.0, 1.0]`
- `idf_smoothing` must be a non-empty string
- term-stats artifact path must be derived by the implementation as the
  repository-root-relative path:
  - `Execution/ingest/hybrid/artifacts/term_stats/<effective_collection_name>__term_stats.json`
- where `<effective_collection_name>` means the strategy-derived Qdrant
  collection name used at runtime
- the implementation must resolve that repository-root-relative path against the
  repository root, not against the current working directory
- that artifact is defined by:
  - `Specification/contracts/ingest/common/bm25_term_stats.md`
- and validated by:
  - `Execution/ingest/schemas/common/bm25_term_stats.schema.json`
- this block must not coexist with `[sparse.bag_of_words]`

## Artifact Rules

### Derived vocabulary artifact path

- path to the singleton vocabulary artifact for this collection must be derived
  by the implementation as the repository-root-relative path:
  - `Execution/ingest/hybrid/artifacts/vocabularies/<base_collection_name>__sparse_vocabulary.json`
- where `<base_collection_name>` means `qdrant.collection.name`
- the implementation must resolve that repository-root-relative path against the
  repository root, not against the current working directory

### `artifacts.manifest_path`

- repository-root-relative path to the run manifest artifact
- the implementation must resolve `artifacts.manifest_path` against the
  repository root, not against the current working directory
- basename must be exactly `run_manifest.json`
- parent directory name must equal:
  - `<started_at>_<run_id>`

## Qdrant Collection Rules

### `qdrant.collection.name`

- canonical base collection name configured by the user
- hybrid ingest must derive the effective Qdrant collection name from this base
  name and the selected sparse strategy

Naming rule:

- `qdrant.collection.name` itself must not include a sparse-strategy suffix
- effective collection name must be derived as:
  - `<qdrant.collection.name>_<derived_strategy_suffix>`

Derived strategy-suffix mapping for the current version:

- if `sparse.strategy.kind = "bag_of_words"`, the derived strategy suffix must
  be `bow`
- if `sparse.strategy.kind = "bm25_like"`, the derived strategy suffix must be
  `bm25`

Suffix rule:

- the suffix is selected only through the explicit strategy-kind to suffix
  mapping above
- the suffix encodes sparse strategy kind only
- effective collection naming must be automatic and must not require the user to
  duplicate strategy identity in config
- for the current contract this is sufficient because only
  `sparse.strategy.version = "v1"` is supported
- if another sparse strategy version is introduced later, collection naming
  rules must be revised or a different base collection name must be used
- all runtime Qdrant API calls must use the derived effective collection name,
  not the unsuffixed `qdrant.collection.name`

Examples:

- config base name: `chunks_hybrid_qwen3`
- selected strategy `bag_of_words` -> suffix `bow` -> effective collection name
  `chunks_hybrid_qwen3_bow`
- selected strategy `bm25_like` -> suffix `bm25` -> effective collection name
  `chunks_hybrid_qwen3_bm25`
- config base name: `chunks_hybrid_qwen3_fixed`
- selected strategy `bag_of_words` -> suffix `bow` -> effective collection name
  `chunks_hybrid_qwen3_fixed_bow`

### `qdrant.collection.dense_vector_name`

- named dense vector slot used in the hybrid collection

### `qdrant.collection.sparse_vector_name`

- named sparse vector slot used in the hybrid collection

Rule:

- `dense_vector_name` and `sparse_vector_name` must be different strings
- this must be validated during config loading before any Qdrant call is made

### `qdrant.collection.distance`

- dense-vector distance metric
- sparse vector distance is not configured here; Qdrant sparse-vector handling is
  used as provided by Qdrant

## Vocabulary Identity Rule

Vocabulary identity for the current version is:

- `collection_name`
- `sparse.tokenizer.library`
- `sparse.tokenizer.source`
- `sparse.tokenizer.revision`, when present
- `sparse.preprocessing.kind`
- `sparse.preprocessing.lowercase`
- `sparse.preprocessing.min_token_length`

If any of these values differ from the persisted vocabulary artifact, the run is
invalid and must fail before chunk processing.

## Strategy Migration Rule

Changing sparse strategy must be treated as a collection migration event.

Rules:

- changing `sparse.strategy.kind` or `sparse.strategy.version` must use a new
  effective collection name
- the recommended way to express that is by switching strategy and letting the
  implementation derive another effective collection name automatically
- for the current contract, strategy version changes are theoretical because
  only `v1` is supported
- hybrid ingest must fail compatibility checks rather than silently reusing a
  collection created for another sparse strategy

## Reused Dense Rules

The following semantics are inherited unchanged from:

- `Specification/contracts/ingest/dense_config.md`

Inherited sections:

- `pipeline` (including `chunking_strategy` — allowed values, metadata write, and compatibility check)
- `embedding`
- `qdrant.point_id`
- `qdrant.transport`
- `qdrant.retry`
- `idempotency`
- `logging`
