# Sparse Text Space Contract

This document defines the canonical text-to-token rules for sparse vector
construction.

It is module-agnostic.

Any component that builds sparse vectors from raw text must use this contract as
the source of truth.

## Sparse Strategy Integration

This document defines the shared text-to-token layer used by sparse strategies.

Strategy-specific sparse weighting and vector-construction rules are defined in
separate strategy contracts.

For the current version:

- shared sparse strategy contract:
  - `Specification/contracts/ingest/common/sparse_strategies/bag_of_words.md`
  - `Specification/contracts/ingest/common/sparse_strategies/bm25_like.md`

## Current Tokenizer

For the current version:

- tokenizer library = `tokenizers`
- tokenizer kind = `basic_word_v1`

## `basic_word_v1` Semantics

Tokenization must use a Hugging Face compatible tokenizer loaded through the
Python `tokenizers` library, following the same library choice already used by
the fixed chunker.

Current tokenizer loading model:

- `tokenizer.library` must be `tokenizers`
- tokenizer artifacts must be loaded from an explicit configured source
- one tokenizer instance must be initialized once and reused for the whole run

Tokenizer artifact selection for sparse text space must be defined by config,
not by hardcoded model names inside implementation code.

## Token Extraction Rule

The tokenizer library output is not used as token ids for sparse vectors.

Instead:

1. text is segmented by the configured `tokenizers` tokenizer;
2. each emitted token string is normalized to a canonical sparse token string;
3. the canonical token string is mapped to a singleton vocabulary token id.

This keeps sparse-vocabulary identity independent from opaque model-internal
token ids while still reusing a library tokenizer instead of a handwritten one.

## Normalization

Tokenizer-emitted token strings are normalized as follows:

1. If `lowercase = true`, convert token to lowercase.
2. Strip leading and trailing tokenizer marker characters used by the tokenizer
   artifact when they are not part of the lexical token itself.
3. If token length is less than `min_token_length`, discard it.
4. Discard tokens that contain no alphanumeric characters after normalization.
5. Keep the remaining token byte-for-byte as the canonical token string.

No stemming, lemmatization, stop-word removal, accent folding, or synonym
expansion is allowed in the current version.

## Output Order

Canonical tokens must preserve left-to-right order of appearance in the source text.

This order is used for vocabulary bootstrap and token counting.

Sparse-vector `indices` order is defined later after token-id mapping and
aggregation.

## Token-To-Vocabulary Rule

When sparse-vector construction begins:

- every canonical token is mapped through the singleton vocabulary
- out-of-vocabulary tokens are ignored in the current version

## Sparse Vector Assembly

After strategy-specific aggregation:

- each retained token id appears at most once in the vector
- `indices` must be sorted ascending
- `values[i]` must correspond to `indices[i]`

This defines the canonical sparse-vector shape used by the project.
