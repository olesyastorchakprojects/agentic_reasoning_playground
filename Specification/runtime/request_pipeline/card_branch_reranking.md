## 1) Purpose / Scope

This document defines the runtime leaf-module contract for
`card_branch_reranking`.

This module exists to:
- accept the fresh candidate-card selection from the current iteration;
- accept the ordered historical card-selection view from prior iterations;
- deterministically choose the current `primary` card branch;
- deterministically choose the current `alternative` card branch;
- return the shared `CardBranchRerankingOutput`;
- keep `challenger_card_ids` reserved but inactive in the current version.

This document is the source of truth for:
- the `card_branch_reranking` leaf-module boundary;
- the module public interface;
- the deterministic v1 branch-selection algorithm;
- the module-owned validation and error boundary.

This document does not define:
- card retrieval from Qdrant;
- semantic interpretation of observations;
- card hydration from PostgreSQL;
- prompt assembly or evidence retrieval;
- future challenger-branch policy beyond the reserved output field.

Shared input and output types are defined by:
- `Specification/runtime/runtime.md`

`CardSelectionContext` projection behavior is defined by:
- `Specification/runtime/request_pipeline/card_selection_context.md`

The generated Rust module file for the current version is:
- `src/request_pipeline/card_branch_reranking.rs`

## 2) Required Shared Types

This module must use the shared runtime types:
- `CandidateCard`
- `CandidateCardRetrievalOutput`
- `PrimaryCardStatus`
- `CardBranchRerankingOutput`
- `CardSelectionContext`
- `CardSelectionSnapshot`

These shared types are defined in:
- `Specification/runtime/runtime.md`

Shared-type rules:
- the module must import these types from `crate::shared_types`;
- `CardSelectionContext.history` is the only historical source used by the
  current version of the algorithm;
- the module must treat the last snapshot in `CardSelectionContext.history` as
  the current historical anchor state;
- `CardBranchRerankingOutput.challenger_card_ids` must be returned as an empty
  vector in the current version.

## 3) Current-Version Preconditions

This module is intended to run only for continuation iterations, not for the
initial iteration.

Current-version preconditions:
- `CardSelectionContext.history` must be non-empty;
- `CandidateCardRetrievalOutput.primary` must be `Some(...)`;
- `CandidateCardRetrievalOutput.ranked_candidates` must preserve the full fresh
  ordered candidate window from upstream retrieval;
- the current continuation iteration must already have completed candidate-card
  retrieval.

The current module runs only for continuation iterations (`1+`).
Therefore, `CardSelectionContext.history` is expected to contain at least the
initial iteration snapshot and may contain any number of additional prior
historical snapshots.

The module must not validate whether the last snapshot corresponds to the
immediately preceding iteration.
It must operate on the historical input exactly as supplied.

The current deterministic algorithm assumes access to a fresh ordered candidate
window deep enough to support:
- tentative-primary retention within the top 2 fresh ranks;
- sticky-primary retention within the top 4 fresh ranks;
- alternative retention within the top 5 fresh ranks.

For the current version, insufficient fresh-candidate depth is defined
deterministically as:
- `ranked_candidates.len() < 5`

If `ranked_candidates.len() < 5`, the module must return
`CardBranchRerankingError::InsufficientFreshCandidateWindow`.

## 4) Public Interface

The generated Rust module must define a public boundary equivalent in ownership
to:

```rust
#[derive(Debug, Default)]
pub struct CardBranchReranking {}

impl CardBranchReranking {
    pub fn new() -> Self;

    pub fn rerank(
        &self,
        fresh_candidates: &CandidateCardRetrievalOutput,
        card_selection_context: &CardSelectionContext,
    ) -> Result<CardBranchRerankingOutput, CardBranchRerankingError>;
}
```

Rules:
- the current version is stateless;
- the generated `CardBranchReranking` struct must store no constructor-owned
  fields in the current version;
- the current version has no constructor-owned external dependencies;
- the current version has no runtime settings slice;
- `new()` must be infallible in the current version;
- `rerank(...)` must be deterministic for the same inputs;
- `rerank(...)` must not mutate its inputs;
- `rerank(...)` must not perform network calls, storage access, or model calls.

## 5) Error Boundary

The generated Rust module must define a typed parent error equivalent in
ownership to:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum CardBranchRerankingError {
    #[error("card selection context history must not be empty")]
    EmptyCardSelectionHistory,

    #[error("fresh candidate retrieval output must contain a primary card")]
    MissingFreshPrimary,

    #[error("duplicate card id '{card_id}' in fresh candidate list")]
    DuplicateFreshCandidate { card_id: String },

    #[error("fresh primary card does not match ranked_candidates[0]")]
    FreshPrimaryMismatch,

    #[error("fresh candidate window is too shallow for reranking policy")]
    InsufficientFreshCandidateWindow,
}
```

Validation order rules (checked in this exact sequence, first match wins):

1. `EmptyCardSelectionHistory` — when `CardSelectionContext.history.is_empty()`;
2. `MissingFreshPrimary` — when `CandidateCardRetrievalOutput.primary = None`;
3. `FreshPrimaryMismatch` — when `CandidateCardRetrievalOutput.primary = Some(card)`,
   `ranked_candidates.first()` exists, and `ranked_candidates[0].case_id != card.case_id`;
4. `DuplicateFreshCandidate` — when the same card id appears more than once in
   `ranked_candidates`; the returned `card_id` is the first duplicate encountered
   in iteration order;
5. `InsufficientFreshCandidateWindow` — when `ranked_candidates.len() < 5`.

If `ranked_candidates.first()` does not exist, `FreshPrimaryMismatch` is not
triggered; `InsufficientFreshCandidateWindow` will fire instead once check 5 is
reached.

## 6) Fresh Candidate List

Before applying the reranking algorithm, the module must construct one ordered
fresh candidate list from `CandidateCardRetrievalOutput`.

Construction rules:
- the fresh candidate list must be taken from
  `CandidateCardRetrievalOutput.ranked_candidates`;
- the first entry in `ranked_candidates` must correspond to
  `CandidateCardRetrievalOutput.primary`, which must be present;
- `CandidateCardRetrievalOutput` does not itself validate this correspondence at
  construction time; this module is the boundary that must check it before
  reranking;
- if this correspondence does not hold, the module must return
  `CardBranchRerankingError::FreshPrimaryMismatch`;
- each entry contributes only its `case_id` to the branch-selection algorithm;
- no card id may appear twice in the fresh list;
- the module must preserve the fresh retrieval order exactly as supplied by
  upstream candidate retrieval.

The current version must reason only over this ordered fresh candidate list and
the last historical snapshot from `CardSelectionContext.history`.

## 7) Deterministic Branch-Selection Algorithm

The current version uses exactly these deterministic constants:
- tentative primary retention window: top 2;
- sticky primary retention window: top 4;
- alternative retention window: top 5;
- maximum alternatives: 2.

Ranking rule:
- fresh rank is the 1-based position of a card inside
  `CandidateCardRetrievalOutput.ranked_candidates`.

### 7.1 Historical Inputs

The module must read the last snapshot from `CardSelectionContext.history` and
treat it as the previous branch state:
- `previous_primary_card_id`
- `previous_primary_card_status`
- `previous_alternative_card_ids`

The current version must ignore historical `challenger_card_ids`.

### 7.2 Primary Selection

The module must determine the new `primary` and `primary_card_status` using the
previous primary-card status.

If `previous_primary_card_status = Tentative`:
- if the previous primary card is absent from the fresh candidate list, the new
  `primary_card_id` is the fresh rank-1 card and the new
  `primary_card_status = Tentative`;
- else if the previous primary card appears below fresh rank 2, the new
  `primary_card_id` is the fresh rank-1 card and the new
  `primary_card_status = Tentative`;
- else the new `primary_card_id` remains the previous primary card and the new
  `primary_card_status = Sticky`.

If `previous_primary_card_status = Sticky`:
- if the previous primary card is absent from the fresh candidate list, the new
  `primary_card_id` is the fresh rank-1 card and the new
  `primary_card_status = Tentative`;
- else if the previous primary card appears below fresh rank 4, the new
  `primary_card_id` is the fresh rank-1 card and the new
  `primary_card_status = Tentative`;
- else the new `primary_card_id` remains the previous primary card and the new
  `primary_card_status = Sticky`.

### 7.3 Alternative Selection

After primary selection, the module must select `alternative_card_ids`.

Step 1: preserve historical alternatives
- iterate through `previous_alternative_card_ids` in their historical order;
- keep a historical alternative only if:
  - it appears in the fresh candidate list;
  - it appears within the first 5 fresh ranks;
  - it is not equal to the newly selected `primary_card_id`;
- preserved historical alternatives remain in their historical order.

Step 2: fill remaining alternative slots
- if fewer than 2 alternatives were preserved, iterate through the fresh
  candidate list from top to bottom;
- add a fresh card to `alternative_card_ids` only if:
  - it is not equal to the newly selected `primary_card_id`;
  - it is not already present in `alternative_card_ids`;
- stop once `alternative_card_ids.len() == 2`.

If Step 1 already preserved 2 alternatives, Step 2 must be skipped.

Step 3: final trimming
- the final `alternative_card_ids` must contain at most 2 card ids.

### 7.4 Challenger Branch

The current version must not actively compute challengers.

Rules:
- `challenger_card_ids` must always be returned as `vec![]`;
- the reserved challenger branch remains present only for future compatibility
  with the shared output type.

## 8) Output Construction

On success, the module must return:

```rust
CardBranchRerankingOutput {
    primary_card_id,
    primary_card_status,
    alternative_card_ids,
    challenger_card_ids: vec![],
}
```

Output rules:
- `primary_card_id` must be non-empty;
- `primary_card_id` must not appear in `alternative_card_ids`;
- `primary_card_id` must not appear in `challenger_card_ids`;
- one card id must not appear more than once in `alternative_card_ids`;
- `challenger_card_ids` must be empty in the current version.

## 9) Invariants

The following invariants must hold for every successful output:
- exactly one `primary_card_id` is returned;
- `primary_card_status` is always either `Tentative` or `Sticky`;
- `alternative_card_ids.len() <= 2`;
- every returned alternative is distinct from the primary;
- no card id appears in more than one branch;
- `challenger_card_ids` is always empty in the current version.

## 10) Relation to CardSelectionContext

This module does not construct `CardSelectionContext`.

It consumes `CardSelectionContext` only as an ordered historical input and
produces one new `CardBranchRerankingOutput` which will later become the source
of truth for the next iteration's `CardSelectionSnapshot`.

The relationship is therefore:
- `CardSelectionContext` provides historical branch state;
- `card_branch_reranking` computes the next branch state;
- `CardSelectionContext::from_run_state(...)` later projects that branch state
  back into ordered history.

## 11) Unit-Test Coverage

Unit-test requirements for this module are defined by:
- `Specification/runtime/unit_tests.md`
