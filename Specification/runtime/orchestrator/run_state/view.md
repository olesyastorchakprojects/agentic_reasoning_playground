## 1) Purpose

This document defines the public borrowed read views generated for
`orchestrator::run_state::view`.

## 2) Generated Rust Artifact

The generated Rust crate must include:

- `src/orchestrator/run_state/view.rs`

Parent module exposure:

- `src/orchestrator/run_state/mod.rs` must expose `view`.

All view types and methods defined by this spec must be public.

## 3) Imports

The generated module requires:

```rust
use chrono::{DateTime, Utc};

use crate::orchestrator::run_state::model::{
    FinishedStepRecord,
    PendingStepRecord,
    RunId,
    RunIteration,
    RunIterationId,
    RunIterationStatus,
    RunState,
    RunStatus,
    StepError,
    StepKind,
    StepRecord,
    StepRecordId,
    StepResultEnvelope,
};
```

Exact import paths may be adjusted by the generator to match the generated
crate layout.

## 4) Public View Types

The generated module must define:

```rust
#[derive(Debug, Clone, Copy)]
pub struct RunStateView<'a> {
    state: &'a RunState,
}

#[derive(Debug, Clone, Copy)]
pub struct IterationView<'a> {
    iteration: &'a RunIteration,
}

#[derive(Debug, Clone, Copy)]
pub enum StepView<'a> {
    Pending(PendingStepView<'a>),
    Finished(FinishedStepView<'a>),
}

#[derive(Debug, Clone, Copy)]
pub struct PendingStepView<'a> {
    record: &'a PendingStepRecord,
}

#[derive(Debug, Clone, Copy)]
pub struct FinishedStepView<'a> {
    record: &'a FinishedStepRecord,
}
```

## 5) Constructors

The generated module must define:

```rust
impl<'a> RunStateView<'a> {
    pub fn new(state: &'a RunState) -> Self;
}
```

## 6) Run State View API

The generated module must define:

```rust
impl<'a> RunStateView<'a> {
    pub fn run_id(&self) -> RunId;

    pub fn status(&self) -> RunStatus;

    pub fn iteration_count(&self) -> usize;

    pub fn iteration(
        &self,
        iteration_id: RunIterationId,
    ) -> Option<&'a RunIteration>;

    pub fn iterations(
        &self,
    ) -> impl DoubleEndedIterator<Item = IterationView<'a>>;

    pub fn normal_iterations(
        &self,
    ) -> impl DoubleEndedIterator<Item = IterationView<'a>>;

    pub fn short_iterations(
        &self,
    ) -> impl DoubleEndedIterator<Item = IterationView<'a>>;

    pub fn last_iteration(&self) -> Option<IterationView<'a>>;
}
```

`iterations()` must preserve `RunState.iterations` order. Because the returned
iterator is double-ended, callers may use `.rev()` to iterate from newest
iteration to oldest iteration.

`normal_iterations()` must preserve the relative order of the subset of
`iterations()` that satisfy `IterationView::is_normal_iteration()`.

`short_iterations()` must preserve the relative order of the subset of
`iterations()` that satisfy `IterationView::is_short_iteration()`.

## 7) Iteration View API

The generated module must define:

```rust
impl<'a> IterationView<'a> {
    pub fn iteration_id(&self) -> RunIterationId;

    pub fn status(&self) -> RunIterationStatus;

    pub fn step_count(&self) -> usize;

    pub fn steps(
        &self,
    ) -> impl DoubleEndedIterator<Item = StepView<'a>>;

    pub fn finished_steps(
        &self,
    ) -> impl DoubleEndedIterator<Item = FinishedStepView<'a>>;

    pub fn pending_step(&self) -> Option<PendingStepView<'a>>;

    pub fn finished_step(&self, kind: StepKind) -> Option<FinishedStepView<'a>>;

    pub fn is_normal_iteration(&self) -> bool;

    pub fn is_short_iteration(&self) -> bool;
}
```

`steps()` must preserve `RunIteration.step_records` order.

`finished_steps()` must preserve the relative order of finished records from
`RunIteration.step_records`.

Because both returned iterators are double-ended, callers may use `.rev()` to
iterate from newest step record to oldest step record within the iteration.

`pending_step()` returns the pending step for this iteration when present.

`finished_step(kind)` returns the last finished step with the requested
`StepKind` in this iteration.

Classification rules:

- `status()` must return the underlying stored `RunIteration.status`;
- `is_normal_iteration()` must return `true` when
  `status() == RunIterationStatus::Active` or
  `status() == RunIterationStatus::FinishedWithSuccess`;
- `is_short_iteration()` must return `true` when
  `status() == RunIterationStatus::FinishedWithWaitInput`;
- `status() == RunIterationStatus::FinishedWithError` must make both
  classification predicates return `false`;
- `is_normal_iteration()` and `is_short_iteration()` must never both return
  `true` for the same iteration.

## 8) Step View API

The generated module must define:

```rust
impl<'a> PendingStepView<'a> {
    pub fn record_id(&self) -> StepRecordId;
    pub fn kind(&self) -> StepKind;
    pub fn started_at(&self) -> DateTime<Utc>;
    pub fn to_owned(&self) -> PendingStepRecord;
}

impl<'a> FinishedStepView<'a> {
    pub fn record_id(&self) -> StepRecordId;
    pub fn kind(&self) -> StepKind;
    pub fn started_at(&self) -> DateTime<Utc>;
    pub fn finished_at(&self) -> DateTime<Utc>;
    pub fn result(&self) -> &'a Result<StepResultEnvelope, StepError>;
    pub fn to_owned(&self) -> FinishedStepRecord;
}

impl<'a> StepView<'a> {
    pub fn to_owned(&self) -> StepRecord;
}
```

`iteration_count()` must equal `RunState.iterations.len()`.

`iteration(iteration_id)` must return the underlying stored `RunIteration`
borrow when the requested id exists and `None` otherwise.

`status()` must equal `RunIteration.status`.

`step_count()` must equal `RunIteration.step_records.len()`.

## 9) View Invariants

Borrowing view methods such as `result()`, `iterations()`, `steps()`,
`finished_steps()`, `pending_step()`, and `finished_step(kind)` must not clone
owned step payloads.

The explicit `to_owned()` helpers are the only view methods allowed to clone
underlying owned records for orchestration persistence handoff and test setup.

View methods that expose ids, kinds, and timestamps may return copied values.

`normal_iterations()` and `short_iterations()` are classification helpers for
history-derived projections. They must not clone step payloads or rewrite
iteration contents.

`StepView` must mirror the underlying `StepRecord` variant:

- `StepRecord::Pending(record)` maps to
  `StepView::Pending(PendingStepView { record })`;
- `StepRecord::Finished(record)` maps to
  `StepView::Finished(FinishedStepView { record })`.

## 10) Ownership Boundaries

- `view.md` owns borrowed read access over `model.md` types.
- `view.md` must not define step-specific request-pipeline input views.
- `view.md` must not define diagnostic-history projections.
