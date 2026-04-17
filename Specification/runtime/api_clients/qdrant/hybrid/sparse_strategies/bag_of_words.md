# Bag Of Words Sparse Strategy Contract

This document defines the execution semantics of the sparse strategy
`bag_of_words`.

It is a shared strategy contract.

Any component that builds sparse vectors with
`sparse.strategy.kind = "bag_of_words"` must use this document as the source of
truth.

## Strategy Meaning

`bag_of_words` means:

- sparse vectors are built from canonical token strings;
- token order in the original text is used only during token extraction and
  counting;
- the final sparse vector stores unique token ids with aggregated weights;
- the final sparse vector does not preserve full sequence order.

## Inputs

`bag_of_words` consumes:

- canonical tokens produced by:
  - `Specification/contracts/ingest/common/sparse_text_space.md`
- token ids provided by:
  - `Specification/contracts/ingest/common/sparse_vocabulary.md`

## Execution Contract

Implementation contract for `bag_of_words`:

1. tokenize input text using the configured tokenizer artifact;
2. normalize emitted token strings according to the sparse text-space contract;
3. discard tokens rejected by normalization rules;
4. map every remaining canonical token string to a vocabulary token id;
5. ignore out-of-vocabulary tokens in the current version;
6. aggregate repeated token ids into one numeric weight per id;
7. sort resulting token ids ascending;
8. emit aligned `indices` and `values`.

## Weighting Rules For Current Version

For the current version:

- point-side weighting mode `term_frequency` means sparse value equals raw
  occurrence count of the token inside the text
- query-side weighting mode `binary_presence` means sparse value equals `1.0`
  if the token appears at least once in the text

## Scope Boundary

This document defines only the `bag_of_words` strategy.

Other sparse strategies, when supported, must be defined by their own shared
strategy contracts rather than being inferred from this document.
