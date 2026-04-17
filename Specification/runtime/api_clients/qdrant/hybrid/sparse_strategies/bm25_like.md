# BM25-Like Sparse Strategy Contract

This document defines the execution semantics of the sparse strategy
`bm25_like`.

It is a shared strategy contract.

Any component that builds sparse vectors with
`sparse.strategy.kind = "bm25_like"` must use this document as the source of
truth.

## Required Library

For the current version, `bm25_like` must use the following libraries:

- Python ingest implementation:
  - `rank_bm25`
- Rust runtime retrieval implementation:
  - `bm25`

The implementation must not replace these with a handwritten BM25-like formula
while still claiming conformance to the current `bm25_like` strategy contract.

## Strategy Meaning

`bm25_like` means:

- sparse vectors are built from canonical token strings;
- token order in the original text is used only during token extraction and
  counting;
- the final sparse vector stores unique token ids with BM25-like weights rather
  than raw counts;
- BM25-like weighting depends on corpus-level statistics in addition to the
  current text.

## Inputs

`bm25_like` consumes:

- canonical tokens produced by:
  - `Specification/contracts/ingest/common/sparse_text_space.md`
- token ids provided by:
  - `Specification/contracts/ingest/common/sparse_vocabulary.md`
- corpus-level sparse statistics produced for the current collection and sparse
  strategy identity

## Required Parameters

For the current version, `bm25_like` requires:

- `k1`
- `b`
- `idf_smoothing`

These parameters belong to:

- `[sparse.bm25_like]`

## Execution Contract

Implementation contract for `bm25_like`:

1. tokenize input text using the configured tokenizer artifact;
2. normalize emitted token strings according to the sparse text-space contract;
3. discard tokens rejected by normalization rules;
4. map every remaining canonical token string to a vocabulary token id;
5. ignore out-of-vocabulary tokens in the current version;
6. compute per-document token frequency for each retained token id;
7. load corpus-level statistics for:
   - document count
   - average document length
   - document frequency per token id
8. compute BM25-like weight for each retained token id using the required
   language-appropriate BM25 library and current config parameters;
9. sort resulting token ids ascending;
10. emit aligned `indices` and `values`.

## Weighting Rules For Current Version

For the current version:

- point-side weighting mode `bm25_document_weight` means sparse value is a
  BM25-like score computed from token frequency, document length, average
  document length, and inverse document frequency
- query-side weighting mode `bm25_query_weight` means query vector uses the same
  vocabulary and corpus statistics, but query-term weighting remains query-side
  and strategy-owned rather than fixed to raw count or binary presence

## Additional Artifact Requirement

`bm25_like` requires corpus-level sparse statistics to be persisted at the
derived BM25 term-stats artifact path for the effective collection name.

That artifact must minimally contain:

- `document_count`
- `average_document_length`
- `document_frequency_by_token_id`
- sparse strategy identity
- vocabulary identity

Its semantic contract is defined by:

- `Specification/contracts/ingest/common/bm25_term_stats.md`

Its machine-readable schema is:

- `Execution/ingest/schemas/common/bm25_term_stats.schema.json`

## Unsupported Variants

Any BM25-like variant whose semantics differ from this document must be encoded
through a new `sparse.strategy.version` or a separate strategy contract, not by
quietly changing the meaning of `bm25_like`.
