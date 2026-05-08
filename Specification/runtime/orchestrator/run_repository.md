## 1) Purpose

This document defines the orchestration-facing persistence boundary for
`orchestrator::run_repository`.

`run_repository` persists and loads canonical `RunState` hierarchy with the same
granularity used by orchestration-layer mutations:

- create one run;
- append one iteration;
- append one step record;
- finish one pending step record;
- update one run header;
- load one full run;
- list runs for user-facing selection and resume flows.

This module does not:

- execute steps;
- decide transitions;
- mutate `RunState` in memory;
- own PostgreSQL table layout directly.

## 2) Generated Rust Artifact

The generated Rust crate must include:

- `src/orchestrator/run_repository.rs`

Parent module exposure:

- `src/orchestrator/mod.rs` must expose `run_repository`.

All public structs, enums, and methods defined by this spec must be
public.

## 3) Imports

The generated module requires:

```rust
use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::api_clients::postgres::run_state_store::{
    PostgresRunStateStore,
    PostgresRunStateStoreTx,
    RunStateStoreError,
    RunSummaryRow,
};
use crate::orchestrator::run_state::model::{
    FinishedStepRecord,
    RunId,
    RunIteration,
    RunIterationId,
    RunIterationStatus,
    RunState,
    RunStatus,
    StepKind,
    StepRecord,
    StepRecordId,
    StepResultEnvelope,
};
```

Exact import paths may be adjusted by the generator to match the generated
crate layout.

## 4) Public Types

The generated module must define:

```rust
#[derive(Debug)]
pub struct RunRepository {
    run_state_store: PostgresRunStateStore,
}

impl RunRepository {
    pub async fn create_run(
        &self,
        run: &RunState,
    ) -> Result<(), RunRepositoryError>;

    pub async fn load_run(
        &self,
        run_id: RunId,
    ) -> Result<Option<RunState>, RunRepositoryError>;

    pub async fn append_iteration(
        &self,
        run: &RunState,
        iteration_sequence_no: u64,
        iteration: &RunIteration,
    ) -> Result<(), RunRepositoryError>;

    pub async fn append_step_record(
        &self,
        run: &RunState,
        iteration_id: RunIterationId,
        step_sequence_no: u64,
        step_record: &StepRecord,
    ) -> Result<(), RunRepositoryError>;

    pub async fn finish_step_record(
        &self,
        run: &RunState,
        record_id: StepRecordId,
        finished_record: &FinishedStepRecord,
    ) -> Result<(), RunRepositoryError>;

    pub async fn update_run_header(
        &self,
        run: &RunState,
    ) -> Result<(), RunRepositoryError>;

    pub async fn update_iteration_status(
        &self,
        run: &RunState,
        iteration_id: RunIterationId,
        status: RunIterationStatus,
    ) -> Result<(), RunRepositoryError>;

    pub async fn list_runs(
        &self,
    ) -> Result<Vec<RunListItem>, RunRepositoryError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunListItem {
    pub run_id: RunId,
    pub status: RunStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: u64,
    pub initial_user_query: String,
    pub final_problem_understanding: Option<String>,
}

#[derive(Debug, Error)]
pub enum RunRepositoryError {
    #[error("run already exists: {run_id:?}")]
    DuplicateRun { run_id: RunId },

    #[error("run state is invalid for persistence: {message}")]
    InvalidRunState { message: String },

    #[error("run list row is missing the initial user query for run {run_id:?}")]
    MissingInitialUserQuery { run_id: RunId },

    #[error(transparent)]
    Store(#[from] RunStateStoreError),
}
```

## 5) Constructor

The generated module must define:

```rust
impl RunRepository {
    pub fn new(run_state_store: PostgresRunStateStore) -> Self;
}
```

`new` must only wrap the supplied store dependency.

## 6) Repository Boundary Rules

`run_repository` must remain orchestration-facing.

Rules:

- `RunRepository` methods must use the same mutation granularity as the
  orchestration-layer components that produce state changes;
- `run_repository` must not collapse all persistence into one generic
  `save_run(...)` method;
- `run_repository` may compose multiple lower-level store operations in one
  repository method;
- `run_repository` owns repository-facing error mapping;
- PostgreSQL table layout and row-shape details remain owned by
  `api_clients/postgres/run_state_store`.

## 7) `create_run`

`create_run(run)` must:

- persist the supplied run header through the lower-level store;
- require that `run.iterations` is empty in the current version;
- return `RunRepositoryError::InvalidRunState { ... }` when the supplied run is
  not a valid empty run header for creation;
- map duplicate-run persistence failure into
  `RunRepositoryError::DuplicateRun { run_id }`.

The current version does not allow `create_run` to implicitly persist child
iterations or step records.

## 8) `load_run`

`load_run(run_id)` must:

- load the full `RunState` hierarchy by delegating to the lower-level store;
- return `Ok(None)` when the run does not exist;
- otherwise return the fully reconstructed canonical `RunState`.

## 9) `append_iteration`

`append_iteration(run, iteration_sequence_no, iteration)` must:

- persist exactly one child iteration for the supplied run;
- preserve the supplied `iteration_sequence_no`;
- update the run header to match `run.status`, `run.updated_at`, and
  `run.revision`;
- use the supplied `run.run_id` as the parent run identity;
- not implicitly persist additional iterations;
- not implicitly persist step records other than those already handled through
  separate repository calls.

This method must correspond to the same granularity as an in-memory
`begin_iteration(...)` mutation.

Canonical store-call sequence for `append_iteration(...)`:

1. call `run_state_store.with_transaction(...)`;
2. inside the callback call
   `tx.insert_iteration(run.run_id, iteration_sequence_no, iteration)`;
3. then call
   `tx.update_run_header(run.run_id, run.status, run.updated_at, run.revision)`;
4. return `Ok(())` from the callback only after both writes succeed.

## 10) `append_step_record`

`append_step_record(run, iteration_id, step_sequence_no, step_record)` must:

- persist exactly one child step record for the supplied iteration;
- preserve the supplied `step_sequence_no`;
- update the run header to match `run.status`, `run.updated_at`, and
  `run.revision`;
- not implicitly append additional step records.

This method must correspond to the same granularity as an in-memory
`begin_step(...)` mutation.

Canonical store-call sequence for `append_step_record(...)`:

1. call `run_state_store.with_transaction(...)`;
2. inside the callback call
   `tx.insert_step_record(iteration_id, step_sequence_no, step_record)`;
3. then call
   `tx.update_run_header(run.run_id, run.status, run.updated_at, run.revision)`;
4. return `Ok(())` from the callback only after both writes succeed.

## 11) `finish_step_record`

`finish_step_record(run, record_id, finished_record)` must:

- persist one pending-to-finished step transition using the lower-level store;
- update the run header to match `run.status`, `run.updated_at`, and
  `run.revision`;
- not create a new step record row;
- preserve the existing step-record identity and ordinal position.
- require that `finished_record.record_id` equals the supplied `record_id`.

This method must correspond to the same granularity as an in-memory
`record_success(...)` or `record_failure(...)` mutation.

Canonical store-call sequence for `finish_step_record(...)`:

1. call `run_state_store.with_transaction(...)`;
2. inside the callback call
   `tx.finish_step_record(record_id, finished_record)`;
3. then call
   `tx.update_run_header(run.run_id, run.status, run.updated_at, run.revision)`;
4. return `Ok(())` from the callback only after both writes succeed.

## 12) `update_run_header`

`update_run_header(run)` must:

- persist only the run header fields:
  - `status`
  - `updated_at`
  - `revision`
- not insert or update iterations;
- not insert or update step records.

This method exists for pure run-header mutations such as `archive_run()` that
do not append, finish, or reclassify child records.

## 13) `update_iteration_status`

`update_iteration_status(run, iteration_id, status)` must:

- persist only the target iteration row's `status` field;
- update the run header to match `run.status`, `run.updated_at`, and
  `run.revision`;
- not insert or update step records;
- not insert additional iterations;
- when used for a wait-for-user pause, persist that pause through the target
  iteration status plus the matching run-header update, without creating any
  synthetic iteration marker row or synthetic step-record row.

Canonical store-call sequence for `update_iteration_status(...)`:

1. call `run_state_store.with_transaction(...)`;
2. inside the callback call
   `tx.update_iteration_status(iteration_id, status)`;
3. then call
   `tx.update_run_header(run.run_id, run.status, run.updated_at, run.revision)`;
4. return `Ok(())` from the callback only after both writes succeed.

## 14) `list_runs`

`list_runs()` must return a user-facing run summary projection:

- ordered by `created_at desc`;
- one `RunListItem` per stored run;
- a run with `status = RunStatus::Active` may already contain a previously
  surfaced user-facing response from an earlier orchestration invocation and
  still remain selectable for continue-versus-new-run UX;
- by calling `run_state_store.list_run_summaries()` as the canonical lower-level
  projection source in the current version.

For each returned run:

- `initial_user_query` must be taken from the
  `StepKind::UserInputReceived` successful step in the first iteration;
- `final_problem_understanding` must be taken from the successful
  `StepKind::ResponseValidationAndNormalization` step in the first iteration,
  when present;
- when no successful final response exists for the first iteration,
  `final_problem_understanding` must be `None`.

For `list_runs()`, a successful step means:

- `StepRecord::Finished(finished)` with `finished.step` equal to the expected
  `StepKind`; and
- `finished.result` equal to `Ok(...)` with the matching
  `StepResultEnvelope` variant for that same `StepKind`.

If a stored run cannot provide the required first-iteration initial user query,
`list_runs()` must return:

```rust
Err(RunRepositoryError::MissingInitialUserQuery { run_id })
```

The current version does not define filtering parameters for `list_runs(...)`.

## 15) Projection Read Rules For `list_runs`

The repository must derive `RunListItem` from information already persisted in
storage.

Rules:

- `run_id`, `status`, `created_at`, `updated_at`, and `revision` come from the
  persisted run header;
- `initial_user_query` comes from the first iteration only;
- `final_problem_understanding` comes from the final successful step of the
  first iteration only;
- later iterations must not replace the first-iteration summary fields for
  `list_runs()`.

The current version does not require a dedicated storage table for run-list
projection rows.

Canonical store-call sequence for `list_runs()`:

1. call `run_state_store.list_run_summaries()`;
2. map each returned `RunSummaryRow` into one `RunListItem`;
3. if `summary.initial_user_query` is `None`, return
   `Err(RunRepositoryError::MissingInitialUserQuery { run_id: summary.run_id })`;
4. otherwise use `summary.final_problem_understanding` as-is.

Canonical extraction shape for projection builders:

```rust
fn extract_initial_user_query(run: &RunState) -> Option<String> {
    let first_iteration = run.iterations.first()?;

    first_iteration.step_records.iter().find_map(|record| match record {
        StepRecord::Finished(finished)
            if finished.step == StepKind::UserInputReceived =>
        {
            match &finished.result {
                Ok(StepResultEnvelope::UserInputReceived(request)) => {
                    Some(request.query.clone())
                }
                _ => None,
            }
        }
        _ => None,
    })
}

fn extract_final_problem_understanding(run: &RunState) -> Option<String> {
    let first_iteration = run.iterations.first()?;

    first_iteration.step_records.iter().rev().find_map(|record| match record {
        StepRecord::Finished(finished)
            if finished.step == StepKind::ResponseValidationAndNormalization =>
        {
            match &finished.result {
                Ok(StepResultEnvelope::ResponseValidationAndNormalization(output)) => {
                    Some(output.response.problem_understanding.clone())
                }
                _ => None,
            }
        }
        _ => None,
    })
}
```

The current version must use the last successful
`ResponseValidationAndNormalization` record from the first iteration when more
than one such successful record is present.

## 16) Transactionality

Repository methods may compose lower-level store operations.

When a repository method performs both:

- a child-row persistence operation; and
- a run-header update,

the repository implementation must execute those writes in one database
transaction through `PostgresRunStateStore::with_transaction(...)`.

In particular, these methods must compose child-row persistence with run-header
update in one transaction:

- `append_iteration(...)`
- `append_step_record(...)`
- `finish_step_record(...)`
- `update_iteration_status(...)`

Canonical transaction shape:

```rust
self.run_state_store
    .with_transaction(|tx: &mut PostgresRunStateStoreTx<'_>| {
        Box::pin(async move {
            // one child write
            // one matching header update
            Ok(())
        })
    })
    .await?;
```

Repository rollback semantics:

- if the child-row write succeeds and the matching `tx.update_run_header(...)`
  call fails, the repository method must return the propagated error and must
  not leave the child-row write committed;
- if the child-row write itself fails, the repository method must return the
  propagated error and must not persist the header update;
- repository methods must not emulate partial success by retrying the header
  update outside the failed transaction.

The current version does not require repository-level optimistic concurrency
beyond the invariants already preserved in the runtime model and lower-level
store.

## 16) Private Helper Allowances

The generated implementation may define private helpers for:

- validating that `create_run(...)` receives an empty initial run;
- mapping store errors into repository errors;
- composing store operations inside one transaction;
- decoding `final_problem_understanding` from the stored final response payload;
- building `RunListItem` values.

Private helpers must not expose new public runtime APIs.

## 17) Ownership Boundaries

- `run_repository.md` owns the orchestration-facing persistence boundary.
- `run_repository.md` must not redefine PostgreSQL storage-row contracts already
  owned by `run_state_store`.
- `run_repository.md` must not define transition policy or step execution
  behavior.
- `run_repository.md` must preserve the same persistence granularity as the
  orchestration-layer mutations that produce run-state changes.

## 18) Unit-Test Ownership

Required unit-test coverage for this module is owned by:

- `Specification/runtime/unit_tests.md`
- `Specification/runtime/unit_tests_common.md`
