## 1) Purpose / Scope

This document defines the `CardSelectionContext` shared type and its
supporting types for the multi-iteration diagnostic pipeline.

`CardSelectionContext` is an ordered projection of card-selection history from
`RunState`. It captures, for each contributing iteration, which incident-card
identifiers were assigned to the `primary`, `alternative`, and `challenger`
branches.

`CardSelectionContext` is not retrieval output and is not a persisted storage
artifact. It is a derived runtime view used by downstream card-branch
selection, retrieval-policy logic, and prompt-assembly logic that need stable
continuity across iterations.

This document is the source of truth for:
- the `CardSelectionContext` struct and its supporting snapshot type;
- construction rules from a `RunState`, including exact field mappings to
  source step result types;
- ordering rules and invariants for historical branch assignments.

This document does not define:
- how candidate cards are retrieved;
- how later reranking policy decides which cards become alternatives or
  challengers;
- prompt construction or chunk-retrieval policy;
- persistence of card-selection history as an independent artifact.

The `RunState`, `RunIterationId`, `StepKind`, and `StepResultEnvelope` types
used here are defined by:
- `Specification/runtime/orchestrator/run_state/model.md`
- `Specification/runtime/orchestrator/run_state/view.md`

The `CandidateCardRetrievalOutput` and `CardBranchRerankingOutput` shared types
used here are defined by:
- `Specification/runtime/runtime.md`

The generated Rust module file for the current version is:
- `src/shared_types/card_selection_context.rs`

---

## 2) Generated Rust Artifact

The generated Rust crate must include:

- `src/shared_types/card_selection_context.rs`

Parent module exposure:

- `src/shared_types/mod.rs` must declare `mod card_selection_context;` as a
  **private** submodule (not `pub mod`) and re-export all public items from
  this module via `pub use card_selection_context::{...}`.
- External code must import `CardSelectionContext`, `CardSelectionContextError`,
  and `CardSelectionSnapshot` only through `crate::shared_types`, never through
  `crate::shared_types::card_selection_context` directly.

All types and methods defined by this spec must be public within the module.

---

## 3) Required Imports

The generated module resides in `src/shared_types/card_selection_context.rs`.
Sibling shared types are imported via `super::`. Orchestrator types are
imported via `crate::`.

```rust
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::orchestrator::run_state::model::{
    RunIterationId,
    RunState,
    StepKind,
    StepResultEnvelope,
};
use crate::orchestrator::run_state::view::RunStateView;
use super::{
    CandidateCardRetrievalOutput,
    CardBranchRerankingOutput,
    PrimaryCardStatus,
};
```

Exact import paths may be adjusted by the generator to match the generated
crate layout.

---

## 4) Prerequisites

The following step results are required as projection sources:

- `StepResultEnvelope::CandidateCardRetrieval(CandidateCardRetrievalOutput)`
  for iteration `0`;
- `StepResultEnvelope::CardBranchReranking(CardBranchRerankingOutput)` for
  iterations `1+`.

`StepKind::CardBranchReranking` is not required for iteration `0`.

---

## 5) Public Types

### 5.1 CardSelectionContext

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardSelectionContext {
    pub history: Vec<CardSelectionSnapshot>,
}
```

Fields:
- `history` — ordered card-selection history, one snapshot per contributing
  iteration, in iteration order from earliest to latest.

### 5.2 CardSelectionSnapshot

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardSelectionSnapshot {
    pub iteration_id: RunIterationId,
    pub primary_card_id: String,
    pub primary_card_status: PrimaryCardStatus,
    pub alternative_card_ids: Vec<String>,
    pub challenger_card_ids: Vec<String>,
}
```

Fields:
- `iteration_id` — the iteration this snapshot belongs to;
- `primary_card_id` — the selected primary-card identifier for this iteration;
- `primary_card_status` — the selected primary-card anchor status for this
  iteration;
- `alternative_card_ids` — selected alternative-card identifiers for this
  iteration in preserved branch order;
- `challenger_card_ids` — selected challenger-card identifiers for this
  iteration in preserved branch order.

### 5.3 CardSelectionContextError

```rust
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CardSelectionContextError {
    #[error("missing candidate-card retrieval result for initial iteration {iteration_id}")]
    MissingInitialCandidateCardRetrieval { iteration_id: RunIterationId },

    #[error("missing card-branch reranking result for iteration {iteration_id}")]
    MissingCardBranchReranking { iteration_id: RunIterationId },

    #[error("initial iteration {iteration_id} produced no primary candidate card")]
    MissingInitialPrimaryCard { iteration_id: RunIterationId },

    #[error("duplicate card id '{card_id}' across branches in iteration {iteration_id}")]
    DuplicateCardAcrossBranches {
        iteration_id: RunIterationId,
        card_id: String,
    },
}
```

---

## 6) Construction from RunState

```rust
impl CardSelectionContext {
    pub fn from_run_state(run_state: &RunState) -> Result<Self, CardSelectionContextError>;
}
```

`from_run_state` constructs `CardSelectionContext` by traversing
`RunStateView::new(run_state).normal_iterations()` in order and projecting the
relevant successful step results into ordered branch snapshots.

An empty `RunState` (no iterations) must produce a valid empty
`CardSelectionContext`:

```rust
CardSelectionContext { history: vec![] }
```

This is not an error.

### 6.1 First Normal Iteration Mapping

For the first normal iteration, `from_run_state` must project:

- `StepResultEnvelope::CandidateCardRetrieval(CandidateCardRetrievalOutput)`

into:

```rust
CardSelectionSnapshot {
    iteration_id: iteration_0.iteration_id,
    primary_card_id: primary_candidate.case_id.clone(),
    primary_card_status: PrimaryCardStatus::Tentative,
    alternative_card_ids: candidate_output
        .alternatives
        .iter()
        .map(|card| card.case_id.clone())
        .collect(),
    challenger_card_ids: vec![],
}
```

Rules:
- if the first normal iteration has
  `status = RunIterationStatus::Active` and contains no successful
  `CandidateCardRetrieval` result yet, it must be skipped without error;
- otherwise, if the first normal iteration contains no successful
  `CandidateCardRetrieval` result, return
  `Err(CardSelectionContextError::MissingInitialCandidateCardRetrieval { ... })`;
- if `CandidateCardRetrievalOutput.primary` is `None`, return
  `Err(CardSelectionContextError::MissingInitialPrimaryCard { ... })`;
- otherwise, `primary_candidate` in the mapping above refers to the validated
  `CandidateCardRetrievalOutput.primary` value after that explicit `None`
  check;
- `primary_card_status` must always be `PrimaryCardStatus::Tentative` for the
  first normal iteration;
- `challenger_card_ids` must be empty for the first normal iteration.

### 6.2 Subsequent Normal Iterations Mapping

For each normal iteration after the first normal iteration, `from_run_state`
must project:

- `StepResultEnvelope::CardBranchReranking(CardBranchRerankingOutput)`

into:

```rust
CardSelectionSnapshot {
    iteration_id: iteration_n.iteration_id,
    primary_card_id: reranking_output.primary_card_id.clone(),
    primary_card_status: reranking_output.primary_card_status,
    alternative_card_ids: reranking_output.alternative_card_ids.clone(),
    challenger_card_ids: reranking_output.challenger_card_ids.clone(),
}
```

Rules:
- if a later normal iteration has `status = RunIterationStatus::Active` and
  contains no successful `CardBranchReranking` result yet, it must be skipped
  without error;
- otherwise, if the iteration contains no successful
  `CardBranchReranking` result, return
  `Err(CardSelectionContextError::MissingCardBranchReranking { ... })`.

### 6.3 Step-Result Selection Rule

For each iteration, `from_run_state` must use the successful result stored in
that iteration's `steps` collection for the required step kind.

Failed or not-yet-finished steps do not contribute a snapshot.

If the required successful step result is absent for an iteration that should
contribute a snapshot, `from_run_state` must return the corresponding typed
error from `CardSelectionContextError`.

An active current iteration whose card-selection output step has not run yet is
not considered a missing-snapshot error in the current version. It must be
silently skipped until the required successful selection output exists.

Iterations whose `IterationView::status()` is
`RunIterationStatus::FinishedWithError` or
`RunIterationStatus::FinishedWithWaitInput` must be skipped entirely. They must
not contribute card-selection snapshots.

---

## 7) Ordering Rules

- `history` must preserve iteration order from earliest to latest, matching
  the order of `RunStateView::normal_iterations()`;
- `alternative_card_ids` must preserve the branch order stored in the source
  step result;
- `challenger_card_ids` must preserve the branch order stored in the source
  step result;
- `from_run_state` must not sort card ids alphabetically, by score, or by any
  derived heuristic.

---

## 8) Invariants

The following invariants must hold for every `CardSelectionContext` produced by
`from_run_state`:

- every `CardSelectionSnapshot.primary_card_id` is non-empty;
- every `CardSelectionSnapshot` for the first normal iteration has
  `primary_card_status = PrimaryCardStatus::Tentative`;
- no card id may appear more than once within the same branch list of one
  snapshot;
- one card id must not appear in more than one branch of the same snapshot;
- `history` must be in iteration order;
- every `CardSelectionSnapshot.iteration_id` must be unique within `history`.
- only normal iterations may contribute snapshots to `history`;
- short iterations must contribute no `CardSelectionSnapshot`.

If a duplicate card id appears across branches in the same snapshot,
`from_run_state` must return:

```rust
Err(CardSelectionContextError::DuplicateCardAcrossBranches {
    iteration_id,
    card_id,
})
```

---

## 9) Relation to DiagnosticContext

`CardSelectionContext` is a separate derived projection over `RunState`.

It must not be embedded into `DiagnosticContext` in the current version.

The separation is intentional:
- `DiagnosticContext` captures diagnostic reasoning state;
- `CardSelectionContext` captures incident-card branch-selection history.

Modules that need both views must construct them separately from the same
`RunState`.

---

## 10) Unit-Test Coverage

Unit-test requirements for this type and its projection behavior are defined by:
- `Specification/runtime/unit_tests.md`
