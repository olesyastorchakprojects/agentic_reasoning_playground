## 1) Purpose / Scope

`run_state_store` persists and reads canonical run-state hierarchy from
PostgreSQL.

This module:
- receives canonical `RunState`, `RunIteration`, and `StepRecord` values;
- writes run-state hierarchy into PostgreSQL tables defined by the canonical
  storage contract;
- loads one full `RunState` by `run_id`;
- preserves iteration and step-record ordering across round-trip persistence.

This module does not:
- decide orchestration transitions;
- execute steps;
- mutate `RunState` in memory;
- define orchestration loop behavior;
- expose SQLx row structs as public API.

Shared-type source of truth:
- `Specification/runtime/orchestrator/run_state/model.md`

Canonical storage source of truth:
- `Specification/contracts/storage/run_state_storage.md`

## 2) Public Interface

The generated Rust module must define:

```rust
#[derive(Debug)]
pub struct PostgresRunStateStoreConfig {
    pub postgres_url: String,
}

#[derive(Debug)]
pub struct PostgresRunStateStore {
    pool: sqlx::PgPool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunSummaryRow {
    pub run_id: RunId,
    pub status: RunStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub revision: u64,
    pub initial_user_query: Option<String>,
    pub final_problem_understanding: Option<String>,
}

pub struct PostgresRunStateStoreTx<'tx> {
    // private transaction wrapper over sqlx::Transaction<'tx, sqlx::Postgres>
}

impl PostgresRunStateStore {
    pub async fn new(
        config: PostgresRunStateStoreConfig,
    ) -> Result<Self, RunStateStoreError>;

    pub async fn with_transaction<T, F>(
        &self,
        f: F,
    ) -> Result<T, RunStateStoreError>
    where
        F: for<'tx> FnOnce(
            &'tx mut PostgresRunStateStoreTx<'tx>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<T, RunStateStoreError>> + 'tx
            >
        >;

    pub async fn insert_run(
        &self,
        run: &RunState,
    ) -> Result<(), RunStateStoreError>;

    pub async fn insert_iteration(
        &self,
        run_id: RunId,
        sequence_no: u64,
        iteration: &RunIteration,
    ) -> Result<(), RunStateStoreError>;

    pub async fn insert_step_record(
        &self,
        iteration_id: RunIterationId,
        sequence_no: u64,
        step_record: &StepRecord,
    ) -> Result<(), RunStateStoreError>;

    pub async fn finish_step_record(
        &self,
        record_id: StepRecordId,
        finished_record: &FinishedStepRecord,
    ) -> Result<(), RunStateStoreError>;

    pub async fn update_run_header(
        &self,
        run_id: RunId,
        status: RunStatus,
        updated_at: chrono::DateTime<chrono::Utc>,
        revision: u64,
    ) -> Result<(), RunStateStoreError>;

    pub async fn load_run(
        &self,
        run_id: RunId,
    ) -> Result<Option<RunState>, RunStateStoreError>;

    pub async fn list_run_ids(
        &self,
    ) -> Result<Vec<RunId>, RunStateStoreError>;

    pub async fn list_run_summaries(
        &self,
    ) -> Result<Vec<RunSummaryRow>, RunStateStoreError>;
}

impl<'tx> PostgresRunStateStoreTx<'tx> {
    pub async fn insert_iteration(
        &mut self,
        run_id: RunId,
        sequence_no: u64,
        iteration: &RunIteration,
    ) -> Result<(), RunStateStoreError>;

    pub async fn insert_step_record(
        &mut self,
        iteration_id: RunIterationId,
        sequence_no: u64,
        step_record: &StepRecord,
    ) -> Result<(), RunStateStoreError>;

    pub async fn finish_step_record(
        &mut self,
        record_id: StepRecordId,
        finished_record: &FinishedStepRecord,
    ) -> Result<(), RunStateStoreError>;

    pub async fn update_run_header(
        &mut self,
        run_id: RunId,
        status: RunStatus,
        updated_at: chrono::DateTime<chrono::Utc>,
        revision: u64,
    ) -> Result<(), RunStateStoreError>;
}
```

Interface rules:

- all public methods must be async;
- `with_transaction(...)` must start one SQL transaction, execute the supplied
  callback inside it, commit on `Ok(...)`, and roll back on `Err(...)`;
- `insert_run` writes only the `diagnostics.runs` row and does not implicitly
  insert child iterations or step records;
- `insert_iteration` writes one child iteration row and does not implicitly
  write step records;
- `insert_step_record` writes one child step-record row;
- `finish_step_record` updates one existing persisted pending step record into
  its finished representation;
- `load_run` reconstructs the full hierarchical `RunState`;
- `list_run_ids` returns persisted run ids ordered by `created_at desc`;
- `list_run_summaries` returns one repository-facing summary row per stored run
  ordered by `created_at desc`;
- public methods must consume or return canonical run-state model types rather
  than storage row structs.

Transaction-boundary rules:

- `PostgresRunStateStoreTx<'tx>` is the only allowed write surface inside
  `with_transaction(...)`;
- tx-scoped methods must have the same persistence semantics and error mapping
  as their non-transactional store counterparts;
- `insert_run(...)`, `load_run(...)`, and `list_run_ids(...)` remain
  store-level methods on `PostgresRunStateStore` and are not required on the
  tx-scoped type in the current version.

## 3) Input And Output Types

The generated Rust module must import:

```rust
use chrono::{DateTime, Utc};

use crate::orchestrator::run_state::model::{
    FinishedStepRecord,
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

Type rules:

- `sequence_no` parameters are zero-based ordinals within the parent collection;
- `run_id`, `iteration_id`, and `record_id` are stable identities from the
  runtime model and must not be regenerated by the store;
- `load_run` returns `Ok(None)` when the requested `run_id` does not exist.

## 4) Configuration Usage

The current version requires only:

- `PostgresRunStateStoreConfig.postgres_url`

Configuration rules:

- `postgres_url` must be non-empty after trimming;
- the module must not read raw TOML directly;
- the module must not read raw environment variables directly;
- the module must not accept crate-level `Settings` as its config type.

## 5) Storage Contract Dependency

This module must implement the storage contract defined in:

- `Specification/contracts/storage/run_state_storage.md`

The executable SQL schema is:

- `Execution/docker/postgres/init/102_diagnostics_run_state.sql`

## 6) Write Behavior

`insert_run(run)` must:

- insert one row into `diagnostics.runs`;
- map only the run header fields from `RunState`;
- fail with a duplicate-run error when `run_id` already exists;
- not implicitly insert iterations or step records.

`insert_iteration(run_id, sequence_no, iteration)` must:

- insert one row into `diagnostics.run_iterations`;
- preserve the supplied `sequence_no`;
- fail when the parent `run_id` does not exist;
- fail when `(run_id, sequence_no)` already exists.

`insert_step_record(iteration_id, sequence_no, step_record)` must:

- insert one row into `diagnostics.run_step_records`;
- preserve the supplied `sequence_no`;
- when `step_record` is `StepRecord::Pending(_)`, write:
  - `record_status = 'pending'`
  - `finished_at = NULL`
  - `result_json = NULL`
  - `error_json = NULL`;
- serialize `StepResultEnvelope` into `result_json` when writing a finished
  success record;
- serialize `StepError` into `error_json` when writing a finished error record;
- fail when the parent `iteration_id` does not exist;
- fail when `(iteration_id, sequence_no)` already exists.

`finish_step_record(record_id, finished_record)` must:

- update one existing row in `diagnostics.run_step_records`;
- locate the target row by `record_id`;
- require that the stored row is currently a pending step record;
- preserve the existing `record_id`, `iteration_id`, `sequence_no`, `step`, and
  `started_at`;
- set `record_status = 'finished'`;
- set `finished_at` from `finished_record.finished_at`;
- serialize `finished_record.result` into `result_json` or `error_json`
  according to whether the finished record is successful or failed;
- fail when `record_id` does not exist;
- fail when the target row is already finished;
- fail when `finished_record.step` does not match the stored step kind.

`update_run_header(run_id, status, updated_at, revision)` must:

- update only the header row in `diagnostics.runs`;
- fail with `RunStateStoreError::MissingParentRun(run_id)` when the target
  `run_id` does not exist;
- not implicitly update child rows.

## 7) Read Behavior

`load_run(run_id)` must:

- read the `diagnostics.runs` header row;
- read child iterations ordered by `sequence_no asc`;
- read child step records ordered by `sequence_no asc`;
- reconstruct one `RunState` with the same hierarchy and order;
- return `Ok(None)` when the run header row does not exist.

The current version may perform multiple SQL queries during `load_run` as long
as the reconstructed hierarchy is correct and deterministic.

`list_run_ids()` must:

- read persisted runs ordered by `created_at desc`;
- return only `RunId` values;
- not load full iteration and step-record hierarchy;
- not expose storage row structs through the public interface.

`list_run_summaries()` must:

- return one `RunSummaryRow` per stored run ordered by `created_at desc`;
- populate header fields from persisted run header state;
- populate `initial_user_query` from the successful
  `StepKind::UserInputReceived` step in the first iteration, when present;
- populate `final_problem_understanding` from the successful
  `StepKind::ResponseValidationAndNormalization` step in the first iteration,
  when present;
- return `final_problem_understanding = None` when the first iteration has no
  successful final response;
- use only first-iteration summary fields and ignore later iterations for this
  projection.

`with_transaction(f)` must:

- begin one PostgreSQL transaction from the store-owned pool;
- provide the callback with a mutable `PostgresRunStateStoreTx<'tx>`;
- commit when the callback returns `Ok(...)`;
- roll back when the callback returns `Err(...)`;
- return the callback result on successful commit;
- map transaction begin / commit / rollback failures into
  `RunStateStoreError::Update(...)`.

## 8) Validation Rules

Before write, the module must reject:

- empty config values;
- invalid run-header timestamps;
- invalid pending-step rows, meaning row shapes that violate the pending-record
  contract from `Specification/contracts/storage/run_state_storage.md` section
  `8)`:
  - `record_status` must be `pending`
  - `finished_at` must be `NULL`
  - `result_json` must be `NULL`
  - `error_json` must be `NULL`;
- invalid finished-step rows, meaning row shapes that violate the finished-row
  contract from `Specification/contracts/storage/run_state_storage.md` section
  `8)`:
  - `record_status` must be `finished`
  - `finished_at` must be non-`NULL`
  - exactly one of `result_json` and `error_json` must be non-`NULL`;
- successful result payloads whose `StepResultEnvelope` variant does not match
  the row `step`;
- step-specific errors whose `StepError` variant does not match the row `step`.

During read, the module must reject:

- unknown `status` strings;
- unknown `step` strings;
- malformed `result_json`;
- malformed `error_json`;
- inconsistent pending / finished row combinations;
- payload variant mismatches between `step` and deserialized payload.

## 9) Error Model

The generated Rust module must define:

```rust
#[derive(Debug, Error)]
pub enum RunStateStoreError {
    #[error("invalid config: {0}")]
    InvalidConfig(&'static str),

    #[error("invalid run state: {0}")]
    InvalidRunState(&'static str),

    #[error("serialization failure: {0}")]
    Serialization(String),

    #[error("deserialization failure: {0}")]
    Deserialization(String),

    #[error("connection failure: {0}")]
    Connection(String),

    #[error("insert failure: {0}")]
    Insert(String),

    #[error("update failure: {0}")]
    Update(String),

    #[error("query failure: {0}")]
    Query(String),

    #[error("duplicate run: {0:?}")]
    DuplicateRun(RunId),

    #[error("duplicate iteration for run {run_id:?} at sequence {sequence_no}")]
    DuplicateIteration {
        run_id: RunId,
        sequence_no: u64,
    },

    #[error("duplicate step record for iteration {iteration_id:?} at sequence {sequence_no}")]
    DuplicateStepRecord {
        iteration_id: RunIterationId,
        sequence_no: u64,
    },

    #[error("step record not found: {0:?}")]
    StepRecordNotFound(StepRecordId),

    #[error("step record already finished: {0:?}")]
    StepRecordAlreadyFinished(StepRecordId),

    #[error(
        "step kind mismatch for record {record_id:?}: expected {expected}, actual {actual}"
    )]
    StepKindMismatch {
        record_id: StepRecordId,
        expected: StepKind,
        actual: StepKind,
    },

    #[error("missing parent run: {0:?}")]
    MissingParentRun(RunId),

    #[error("missing parent iteration: {0:?}")]
    MissingParentIteration(RunIterationId),

    #[error("invalid stored row: {0}")]
    InvalidStoredRow(&'static str),
}
```

Required failure categories:

- invalid config;
- invalid in-memory run-state value before write;
- serialization failure;
- deserialization failure;
- connection failure;
- insert failure;
- update failure;
- query failure;
- duplicate run;
- duplicate child sequence inside one parent;
- missing step record;
- attempt to finish an already finished step record;
- step kind mismatch during pending-to-finished transition;
- missing parent row;
- invalid stored row shape.

## 10) Internal Mapping Helpers

The module may define internal mapping helpers such as:

- `RunHeaderRowMapper`
- `RunIterationRowMapper`
- `StepRecordRowMapper`
- `RunStateReadAssembler`
- `PostgresRunStateStoreTx`
- `RunSummaryRowBuilder`

Responsibilities:

- write mappers convert canonical model values into SQL-bound row payloads;
- read assembler reconstructs one `RunState` from queried rows;
- internal helpers must remain private.

## 11) Ownership Boundaries

- `run_state_store.md` owns the PostgreSQL persistence boundary for canonical
  run-state hierarchy.
- `run_state_store.md` must not define orchestration policy or execution
  semantics.
- `run_state_store.md` must not redefine the run-state model itself.
- `run_state_store.md` must remain aligned with
  `Specification/contracts/storage/run_state_storage.md`.

## 12) Unit-Test Ownership

Required unit-test coverage for this module is owned by:

- `Specification/runtime/unit_tests.md`
- `Specification/runtime/unit_tests_common.md`
