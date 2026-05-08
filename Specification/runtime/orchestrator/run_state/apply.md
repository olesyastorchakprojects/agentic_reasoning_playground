## 1) Purpose

This document defines the public state mutation API generated for
`orchestrator::run_state::apply`.

## 2) Generated Rust Artifact

The generated Rust crate must include:

- `src/orchestrator/run_state/apply.rs`

Parent module exposure:

- `src/orchestrator/run_state/mod.rs` must expose `apply`.

All writer types, error types, and methods defined by this spec must be public.

## 3) Imports

The generated module requires:

```rust
use chrono::Utc;
use thiserror::Error;
use uuid::Uuid;

use crate::orchestrator::run_state::model::{
    FinishedStepRecord,
    PendingStepRecord,
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
use crate::shared_types::UserRequest;
```

Exact import paths may be adjusted by the generator to match the generated
crate layout.

## 4) Public Types

The generated module must define:

```rust
pub struct RunStateWriter<'a> {
    state: &'a mut RunState,
}

pub struct CurrentIterationWriter<'a> {
    state: &'a mut RunState,
    iteration_index: usize,
}

pub struct PendingStepWriter<'a> {
    state: &'a mut RunState,
    iteration_index: usize,
    step_index: usize,
}

#[derive(Debug, Error)]
pub enum StateApplyError {
    #[error("run is archived")]
    RunArchived,

    #[error("no current iteration")]
    NoCurrentIteration,

    #[error("pending step already exists")]
    PendingStepAlreadyExists,

    #[error("current iteration is closed")]
    CurrentIterationClosed,

    #[error("pending step handle is stale")]
    StalePendingStep,

    #[error("step result does not match step kind: {step:?}")]
    StepResultKindMismatch { step: StepKind },

    #[error("step error does not match step kind: {step:?}")]
    StepErrorKindMismatch { step: StepKind },
}
```

## 5) Constructors

The generated module must define:

```rust
impl<'a> RunStateWriter<'a> {
    pub fn new(state: &'a mut RunState) -> Self;
}
```

`new` must only wrap the mutable `RunState` reference.

## 6) Run State Writer API

The generated module must define:

```rust
impl<'a> RunStateWriter<'a> {
    pub fn begin_iteration(
        &mut self,
        user_input: UserRequest,
    ) -> Result<CurrentIterationWriter<'_>, StateApplyError>;

    pub fn current_iteration(
        &mut self,
    ) -> Result<CurrentIterationWriter<'_>, StateApplyError>;

    pub fn finish_current_iteration_success(&mut self) -> Result<(), StateApplyError>;

    pub fn finish_current_iteration_error(&mut self) -> Result<(), StateApplyError>;

    pub fn wait_for_user(&mut self) -> Result<(), StateApplyError>;

    pub fn archive_run(&mut self) -> Result<(), StateApplyError>;
}
```

`RunStateWriter` mutates only run-level state and creates or selects the current
iteration writer.

## 7) Current Iteration Writer API

The generated module must define:

```rust
impl<'a> CurrentIterationWriter<'a> {
    pub fn iteration_id(&self) -> RunIterationId;

    pub fn begin_step(
        &mut self,
        step: StepKind,
    ) -> Result<PendingStepWriter<'_>, StateApplyError>;

    pub fn pending_step(&mut self) -> Option<PendingStepWriter<'_>>;
}
```

`CurrentIterationWriter` mutates only the current iteration.

## 8) Pending Step Writer API

The generated module must define:

```rust
impl<'a> PendingStepWriter<'a> {
    pub fn record_id(&self) -> StepRecordId;

    pub fn kind(&self) -> StepKind;

    pub fn record_success(
        self,
        result: StepResultEnvelope,
    ) -> Result<(), StateApplyError>;

    pub fn record_failure(
        self,
        error: StepError,
    ) -> Result<(), StateApplyError>;
}
```

`PendingStepWriter` mutates only the pending step identified by its stored
`iteration_index` and `step_index`.

## 9) Shared Mutation Rules

Every successful mutating method must:

- set `state.updated_at` to `Utc::now()`;
- increment `state.revision` by one.

Calling `archive_run()` when `state.status == RunStatus::Archived` is a pure
no-op. In that case, `updated_at` and `revision` must not be modified.

Every mutating method except `archive_run` must return
`StateApplyError::RunArchived` when `state.status == RunStatus::Archived`.

Generated ids must use `Uuid::new_v4()`.

Generated timestamps must use `Utc::now()`.

## 10) begin_iteration

`begin_iteration(user_input)` must:

- return `StateApplyError::PendingStepAlreadyExists` when any
  `StepRecord::Pending(_)` exists in the whole `RunState`;
- create a new `RunIterationId`;
- create a new `StepRecordId` for the user input record;
- create a finished `StepRecord` with:
  - `step = StepKind::UserInputReceived`;
  - `started_at = Utc::now()`;
  - `finished_at = started_at`;
  - `result = Ok(StepResultEnvelope::UserInputReceived(user_input))`;
- append a new `RunIteration` to `state.iterations`;
- set `state.status = RunStatus::Active`;
- return `CurrentIterationWriter` for the new iteration.

The new iteration's `step_records` must contain only the generated
`UserInputReceived` finished step.

The new iteration must set:

- `status = RunIterationStatus::Active`.

## 11) current_iteration

`current_iteration()` must:

- return `StateApplyError::NoCurrentIteration` when `state.iterations` is empty;
- return `CurrentIterationWriter` for the last iteration in
  `state.iterations`.

`current_iteration()` must not update `updated_at` or increment `revision`.

## 12) begin_step

`CurrentIterationWriter::begin_step(step)` must:

- return `StateApplyError::PendingStepAlreadyExists` when any
  `StepRecord::Pending(_)` exists in the whole `RunState`;
- return `StateApplyError::CurrentIterationClosed` when the current iteration
  status is not `RunIterationStatus::Active`;
- create a new `StepRecordId`;
- append `StepRecord::Pending(PendingStepRecord { ... })` to the current
  iteration;
- set `state.status = RunStatus::Active`;
- return `PendingStepWriter` for the new pending step.

The pending record must store:

- `record_id = generated StepRecordId`;
- `step = step`;
- `started_at = Utc::now()`.

## 13) pending_step

`CurrentIterationWriter::pending_step()` must:

- return `Some(PendingStepWriter)` when the current iteration contains a
  `StepRecord::Pending(_)`;
- return `None` when the current iteration contains no pending step.

`pending_step()` must not update `updated_at` or increment `revision`.

## 14) record_success

`PendingStepWriter::record_success(result)` must:

- validate that the stored `iteration_index` and `step_index` still identify a
  `StepRecord::Pending(_)`;
- return `StateApplyError::StalePendingStep` when the stored indices no longer
  identify a pending step;
- validate that `result` matches the pending record's `step`;
- return `StateApplyError::StepResultKindMismatch { step }` when the result
  variant does not match;
- replace the pending record with `StepRecord::Finished(FinishedStepRecord {
  ... })`;
- set `state.status = RunStatus::Active`.

`record_success(result)` must not change the current iteration status. The
iteration remains `RunIterationStatus::Active` until orchestration explicitly
marks it as success or wait-input at invocation termination time.

The finished record must preserve:

- `record_id`;
- `step`;
- `started_at`.

The finished record must set:

- `finished_at = Utc::now()`;
- `result = Ok(result)`.

`record_success(result)` must use the same `StepKind` ↔
`StepResultEnvelope` compatibility contract defined in
`Specification/runtime/orchestrator/run_state/model.md`.

In particular, the current version must accept these success mappings in
addition to the earlier initial-flow steps:

| pending `step` | accepted success `result` |
| --- | --- |
| `StepKind::ObservationBoundaryResolver` | `StepResultEnvelope::ObservationBoundaryResolver(_)` |
| `StepKind::ObservationExtraction` | `StepResultEnvelope::ObservationExtraction(_)` |
| `StepKind::InformationAdequacyInitial` | `StepResultEnvelope::InformationAdequacy(_)` |
| `StepKind::InformationAdequacySupportedObservation` | `StepResultEnvelope::InformationAdequacy(_)` |
| `StepKind::InformationAdequacyUnsupportedObservation` | `StepResultEnvelope::InformationAdequacy(_)` |
| `StepKind::CardBranchReranking` | `StepResultEnvelope::CardBranchReranking(_)` |
| `StepKind::DiagnosticUpdatePromptContextAssembly` | `StepResultEnvelope::DiagnosticUpdatePromptContextAssembly(_)` |

## 15) record_failure

`PendingStepWriter::record_failure(error)` must:

- validate that the stored `iteration_index` and `step_index` still identify a
  `StepRecord::Pending(_)`;
- return `StateApplyError::StalePendingStep` when the stored indices no longer
  identify a pending step;
- validate that step-specific `StepError` variants match the pending record's
  `step`;
- return `StateApplyError::StepErrorKindMismatch { step }` when the error
  variant does not match;
- replace the pending record with `StepRecord::Finished(FinishedStepRecord {
  ... })`;
- set `state.status = RunStatus::Error`.
- set the current iteration status to `RunIterationStatus::FinishedWithError`.

The finished record must preserve:

- `record_id`;
- `step`;
- `started_at`.

The finished record must set:

- `finished_at = Utc::now()`;
- `result = Err(error)`.

Non-step-specific `StepError` variants may be recorded for any `StepKind`.

`record_failure(error)` must use the same step-specific `StepKind` ↔
`StepError` compatibility contract defined in
`Specification/runtime/orchestrator/run_state/model.md`.

In particular, the current version must accept these step-specific error
mappings in addition to the earlier initial-flow steps:

| pending `step` | accepted step-specific `error` |
| --- | --- |
| `StepKind::ObservationBoundaryResolver` | `StepError::ObservationBoundaryResolver(_)` |
| `StepKind::ObservationExtraction` | `StepError::ObservationExtraction(_)` |
| `StepKind::InformationAdequacyInitial` | `StepError::InformationAdequacy(_)` |
| `StepKind::InformationAdequacySupportedObservation` | `StepError::InformationAdequacy(_)` |
| `StepKind::InformationAdequacyUnsupportedObservation` | `StepError::InformationAdequacy(_)` |
| `StepKind::CardBranchReranking` | `StepError::CardBranchReranking(_)` |
| `StepKind::DiagnosticUpdatePromptContextAssembly` | `StepError::DiagnosticUpdatePromptContextAssembly(_)` |

## 16) finish_current_iteration_success

`finish_current_iteration_success()` must:

- return `StateApplyError::NoCurrentIteration` when `state.iterations` is
  empty;
- return `StateApplyError::PendingStepAlreadyExists` when any
  `StepRecord::Pending(_)` exists in the whole `RunState`;
- return `StateApplyError::CurrentIterationClosed` when the current iteration
  status is not `RunIterationStatus::Active`;
- set the current iteration status to
  `RunIterationStatus::FinishedWithSuccess`;
- set `state.status = RunStatus::Active`.

## 16a) finish_current_iteration_error

`finish_current_iteration_error()` must:

- return `StateApplyError::NoCurrentIteration` when `state.iterations` is
  empty;
- return `StateApplyError::PendingStepAlreadyExists` when any
  `StepRecord::Pending(_)` exists in the whole `RunState`;
- return `StateApplyError::CurrentIterationClosed` when the current iteration
  status is not `RunIterationStatus::Active`;
- set the current iteration status to
  `RunIterationStatus::FinishedWithError`;
- set `state.status = RunStatus::Error`.

## 17) wait_for_user

`wait_for_user()` must:

- return `StateApplyError::PendingStepAlreadyExists` when any
  `StepRecord::Pending(_)` exists in the whole `RunState`;
- return `StateApplyError::CurrentIterationClosed` when the current iteration
  status is not `RunIterationStatus::Active`;
- set `state.status = RunStatus::WaitingForUser`;
- set the current iteration status to
  `RunIterationStatus::FinishedWithWaitInput`;
- leave the current last iteration structurally unchanged except for the
  iteration-status mutation and the matching run-header mutation;
- treat the current last iteration as closed for further step execution by
  later orchestration logic.

## 18) archive_run

`archive_run()` must:

- set `state.status = RunStatus::Archived`.

`archive_run()` does not check for a pending step. Archiving is allowed
regardless of whether a step is in flight.

Calling `archive_run()` when the run is already archived is a pure no-op:

- `state.status` remains `RunStatus::Archived`;
- `state.updated_at` is not modified;
- `state.revision` is not incremented.

## 19) Validation Helpers

The generated module may define private helpers for:

- finding the pending step within the current iteration;
- checking whether any pending step exists in the whole run;
- checking `StepKind` and `StepResultEnvelope` compatibility;
- checking `StepKind` and step-specific `StepError` compatibility;
- applying shared successful mutation bookkeeping.

## 19) Ownership Boundaries

- `apply.md` owns mutation/update operations over `model.md` types.
- `apply.md` must not define read projections beyond private helpers needed by
  writer methods.
