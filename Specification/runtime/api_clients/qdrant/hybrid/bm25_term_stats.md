# BM25 Term Stats Contract

This document defines the canonical on-disk contract for the artifact referenced
by:

- the derived BM25 term-stats artifact path for hybrid ingest

It is used only when:

- `sparse.strategy.kind = "bm25_like"`

## Purpose

This artifact stores the corpus-level statistics required to build BM25-like
sparse vectors for one hybrid collection and one sparse strategy identity.

It exists so that:

- document-side BM25 weights are reproducible
- query-side BM25 weights use the same corpus statistics
- strategy compatibility is explicit

## Required Top-Level Fields

The JSON object must contain:

- `collection_name`
- `sparse_strategy`
- `vocabulary_name`
- `document_count`
- `average_document_length`
- `document_frequency_by_token_id`
- `created_at`

## Field Semantics

### `collection_name`

- derived strategy-specific Qdrant collection name for which these BM25
  statistics are valid

### `sparse_strategy`

- object containing:
  - `kind`
  - `version`

For the current version:

- `kind = "bm25_like"`
- `version = "v1"`

### `vocabulary_name`

- vocabulary identity string
- must match the vocabulary artifact used by the same collection

### `document_count`

- total number of documents used to compute BM25-like corpus statistics
- must be an integer greater than `0`

### `average_document_length`

- average retained-token count across those documents after sparse text-space
  normalization
- must be a positive number

### `document_frequency_by_token_id`

- object whose keys are decimal string token ids and whose values are document
  frequencies for those token ids

Rules:

- every key must be parseable as a non-negative integer token id
- every value must be an integer greater than `0`
- every value must be less than or equal to `document_count`

### `created_at`

- UTC timestamp of first successful term-stats creation

## Naming Rule

- basename should equal:
  - `<collection_name>__term_stats.json`

## Scope Boundary

This artifact stores only corpus-level statistics required by `bm25_like`.

It must not be used for:

- vocabulary storage
- per-point sparse vectors
- collection metadata snapshots
