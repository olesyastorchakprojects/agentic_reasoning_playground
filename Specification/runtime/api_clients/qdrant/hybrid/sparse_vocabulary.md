# Sparse Vocabulary Contract

This document defines the canonical on-disk contract for the singleton sparse
vocabulary artifact used by `hybrid_ingest.py` and future sparse-query builders.

## Purpose

The sparse vocabulary artifact gives stable integer token ids for one hybrid
collection family.

It exists so that:

- every chunk in the collection uses the same token-id mapping
- later query-time sparse-vector creation can reuse the exact same mapping
- sparse-space identity is explicit and reproducible

## Singleton Rule

There is exactly one sparse vocabulary artifact per configured base collection
name.

For the current version:

- vocabulary key = configured base collection name `qdrant.collection.name`
- vocabulary name = `<collection_name>__sparse_vocabulary`
- vocabulary filename = `<collection_name>__sparse_vocabulary.json`

## Required Top-Level Fields

The vocabulary JSON object must contain:

- `vocabulary_name`
- `collection_name`
- `text_processing`
- `tokenizer`
- `created_at`
- `tokens`

## Field Semantics

### `vocabulary_name`

- canonical vocabulary identity string
- must equal `<collection_name>__sparse_vocabulary`

### `collection_name`

- configured base collection name for which this vocabulary is valid
- this is the unsuffixed name from config, not the derived strategy-specific
  Qdrant collection name

### `text_processing`

- object containing:
  - `lowercase`
  - `min_token_length`

Its semantics are defined by:

- `Specification/contracts/ingest/common/sparse_text_space.md`

### `tokenizer`

- object containing:
  - `library`
  - `source`
  - `revision`, when present

Rule:

- these fields identify the concrete tokenizer artifact used during vocabulary
  bootstrap

### `created_at`

- UTC timestamp of first successful vocabulary creation

### `tokens`

- ordered array of vocabulary entries
- order is the canonical token-id order

## Token Entry Structure

Each entry of `tokens` must contain:

- `token`
- `token_id`

Rules:

- `token` is the canonical normalized token string
- `token_id` is the stable integer id for that token
- `token_id` values must start at `0`
- `token_id` values must be contiguous
- `token_id` equals the zero-based array position of the entry
- the same token string must not appear more than once

## Bootstrap Rule

When the vocabulary is created for the first time:

- chunks are read in `CHUNKS_PATH` order
- each chunk is tokenized using the sparse text-space contract
- each canonical token is considered in left-to-right order within the chunk
- first time a canonical token is seen globally, it receives the next integer id

This bootstrap rule defines stable token-id assignment.

## Immutability Rule

After first successful creation:

- the vocabulary must be treated as immutable
- new token ids must not be appended
- existing token ids must not be reassigned
- token order must not be rewritten

If later ingest input contains tokens absent from the existing vocabulary:

- those tokens are ignored for sparse-vector generation in the current version
- ingest must log the out-of-vocabulary count in the run manifest
- ingest must not mutate the vocabulary artifact
