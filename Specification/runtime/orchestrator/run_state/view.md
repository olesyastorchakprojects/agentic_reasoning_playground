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

    pub fn iterations(
        &self,
    ) -> impl DoubleEndedIterator<Item = IterationView<'a>>;

    pub fn last_iteration(&self) -> Option<IterationView<'a>>;
}
```

`iterations()` must preserve `RunState.iterations` order. Because the returned
iterator is double-ended, callers may use `.rev()` to iterate from newest
iteration to oldest iteration.

## 7) Iteration View API

The generated module must define:

```rust
impl<'a> IterationView<'a> {
    pub fn iteration_id(&self) -> RunIterationId;

    pub fn steps(
        &self,
    ) -> impl DoubleEndedIterator<Item = StepView<'a>>;

    pub fn finished_steps(
        &self,
    ) -> impl DoubleEndedIterator<Item = FinishedStepView<'a>>;

    pub fn pending_step(&self) -> Option<PendingStepView<'a>>;

    pub fn finished_step(&self, kind: StepKind) -> Option<FinishedStepView<'a>>;
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

## 8) Step View API

The generated module must define:

```rust
impl<'a> PendingStepView<'a> {
    pub fn record_id(&self) -> StepRecordId;
    pub fn kind(&self) -> StepKind;
    pub fn started_at(&self) -> DateTime<Utc>;
}

impl<'a> FinishedStepView<'a> {
    pub fn record_id(&self) -> StepRecordId;
    pub fn kind(&self) -> StepKind;
    pub fn started_at(&self) -> DateTime<Utc>;
    pub fn finished_at(&self) -> DateTime<Utc>;
    pub fn result(&self) -> &'a Result<StepResultEnvelope, StepError>;
}
```

## 9) View Invariants

View methods must not clone owned step payloads.

View methods that expose ids, kinds, and timestamps may return copied values.

`StepView` must mirror the underlying `StepRecord` variant:

- `StepRecord::Pending(record)` maps to
  `StepView::Pending(PendingStepView { record })`;
- `StepRecord::Finished(record)` maps to
  `StepView::Finished(FinishedStepView { record })`.

## 10) Ownership Boundaries

- `view.md` owns borrowed read access over `model.md` types.
- `view.md` must not define step-specific request-pipeline input views.
- `view.md` must not define diagnostic-history projections.
