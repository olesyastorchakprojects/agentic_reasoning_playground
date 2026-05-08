## 1) Purpose / Scope

This document defines the canonical storage contract for persisted run state.

It specifies:
- the PostgreSQL namespace used by run-state storage;
- the canonical storage target tables;
- how `RunState`, `RunIteration`, and `StepRecord` map into storage fields;
- ordering and versioning fields used by storage;
- storage-level invariants that must hold independently of runtime code.

This document does not define:
- runtime store-module public interface;
- SQL migration file layout beyond the required executable schema path;
- orchestration policy or step execution behavior;
- machine-readable JSON schemas for database rows.

## 2) Canonical Storage Targets

The canonical storage targets for run state are:

- schema: `diagnostics`
- tables:
  - `diagnostics.runs`
  - `diagnostics.run_iterations`
  - `diagnostics.run_step_records`

Namespace rule:
- run-state tables must not be created in `public`;
- the `diagnostics` schema is the required namespace for the current version.

## 3) Canonical Storage Shape

The canonical storage contract is:

```text
diagnostics.runs
  run_id uuid primary key
  status text not null
  created_at timestamptz not null
  updated_at timestamptz not null
  revision bigint not null

diagnostics.run_iterations
  iteration_id uuid primary key
  run_id uuid not null references diagnostics.runs(run_id)
  status text not null
  sequence_no bigint not null

diagnostics.run_step_records
  record_id uuid primary key
  iteration_id uuid not null references diagnostics.run_iterations(iteration_id)
  sequence_no bigint not null
  step text not null
  record_status text not null
  started_at timestamptz not null
  finished_at timestamptz null
  result_json jsonb null
  error_json jsonb null
```

Uniqueness requirements:

- `diagnostics.run_iterations` must enforce unique `(run_id, sequence_no)`;
- `diagnostics.run_step_records` must enforce unique
  `(iteration_id, sequence_no)`.

## 4) Why This Shape

This storage shape intentionally keeps:

- the run / iteration / step-record hierarchy relational and explicit;
- step payloads flexible through `jsonb`;
- ordering explicit through `sequence_no` rather than inferred from
  timestamps;
- aggregate versioning explicit through `revision`.

Rationale:

- the relational skeleton preserves the same hierarchy already modeled by
  `RunState`, `RunIteration`, and `StepRecord`;
- `StepResultEnvelope` and `StepError` payloads are expected to evolve more
  frequently than the hierarchy itself, so they should not require one table per
  step kind;
- explicit ordering makes read-back deterministic even when timestamps are
  equal.

## 5) Source Semantic Objects

The source semantic objects are defined by:

- `Specification/runtime/orchestrator/run_state/model.md`

The relevant domain types are:

- `RunState`
- `RunIteration`
- `RunIterationStatus`
- `StepRecord`
- `PendingStepRecord`
- `FinishedStepRecord`
- `RunStatus`
- `StepKind`
- `StepResultEnvelope`
- `StepError`

## 6) Mapping Contract

Run-level mapping:

- `RunState.run_id` -> `diagnostics.runs.run_id`
- `RunState.status` -> `diagnostics.runs.status`
- `RunState.created_at` -> `diagnostics.runs.created_at`
- `RunState.updated_at` -> `diagnostics.runs.updated_at`
- `RunState.revision` -> `diagnostics.runs.revision`

Iteration-level mapping:

- each `RunState.iterations[i]` maps to one row in `diagnostics.run_iterations`
- `RunIteration.iteration_id` -> `diagnostics.run_iterations.iteration_id`
- parent `RunState.run_id` -> `diagnostics.run_iterations.run_id`
- `RunIteration.status` -> `diagnostics.run_iterations.status`
- iteration ordinal `i` -> `diagnostics.run_iterations.sequence_no`

Step-record mapping:

- each `RunIteration.step_records[j]` maps to one row in
  `diagnostics.run_step_records`
- `StepRecord.record_id` -> `diagnostics.run_step_records.record_id`
- parent `RunIteration.iteration_id` ->
  `diagnostics.run_step_records.iteration_id`
- step-record ordinal `j` -> `diagnostics.run_step_records.sequence_no`
- `StepKind` -> `diagnostics.run_step_records.step`
- `Pending` / `Finished` variant -> `diagnostics.run_step_records.record_status`
- `started_at` -> `diagnostics.run_step_records.started_at`
- `finished_at` -> `diagnostics.run_step_records.finished_at`
- successful `StepResultEnvelope` -> `diagnostics.run_step_records.result_json`
- failed `StepError` -> `diagnostics.run_step_records.error_json`

## 7) Ordering And Versioning Semantics

`sequence_no` means sequence number.

Rules:

- `diagnostics.run_iterations.sequence_no` is the zero-based ordinal of one
  iteration within `RunState.iterations`;
- `diagnostics.run_step_records.sequence_no` is the zero-based ordinal of one
  step record within `RunIteration.step_records`;
- `sequence_no` must be monotonically increasing within its parent collection;
- storage read-back must reconstruct iteration and step-record order by sorting
  ascending on `sequence_no`.

`revision` means aggregate version number.

Rules:

- `diagnostics.runs.revision` stores the current `RunState.revision`;
- `revision` is aggregate-wide, not per iteration and not per step;
- storage must persist the value supplied by the runtime model rather than
  recalculating it from row counts.

## 8) Step-Record Status Contract

`record_status` must be one of:

- `pending`
- `finished`

Pending-record contract:

- `record_status = 'pending'`
- `finished_at is null`
- `result_json is null`
- `error_json is null`

Finished-success contract:

- `record_status = 'finished'`
- `finished_at is not null`
- `result_json is not null`
- `error_json is null`

Finished-error contract:

- `record_status = 'finished'`
- `finished_at is not null`
- `result_json is null`
- `error_json is not null`

The current version does not allow one finished row to contain both
`result_json` and `error_json`.

## 9) JSON Payload Contract

Rules:

- `result_json` must contain the complete serialized `StepResultEnvelope`;
- `error_json` must contain the complete serialized `StepError`;
- storage must not define one table per `StepKind` output payload;
- `step` must remain a first-class column and must agree semantically with the
  stored JSON payload;
- payload JSON must be stored exactly as runtime serialization produces it,
  without custom lossy flattening.

The current version does not require a standalone JSON Schema file for
`result_json` or `error_json`.

Rationale:

- these payload shapes are internal persistence payloads rather than external
  configuration or artifact contracts;
- their evolution should be driven by runtime model types and validated on
  read/write rather than through a separate schema artifact.

## 10) Storage-Level Invariants

The storage layer must preserve:

- one row per run;
- one ordered iteration sequence per run;
- one ordered step-record sequence per iteration;
- at most one pending step record in a reconstructed `RunState`;
- semantic compatibility between `step` and the stored `result_json` or
  `error_json`.

Required consistency rules:

- `updated_at >= created_at`;
- `revision >= 0`;
- `sequence_no >= 0`;
- if `record_status = 'pending'`, then `finished_at`, `result_json`, and
  `error_json` must all be `NULL`;
- if `record_status = 'finished'`, then exactly one of `result_json` and
  `error_json` must be non-`NULL`;
- if `finished_at` is non-`NULL`, then `finished_at >= started_at`.

## 11) Read Behavior Expectations

Canonical read behavior:

- `load by run_id` must reconstruct one complete `RunState`;
- reconstruction must include all iterations belonging to the run;
- reconstruction must include all step records belonging to each iteration;
- reconstruction must preserve canonical order through `sequence_no`;
- storage read behavior must not silently repair inconsistent rows.

Read-time validation requirements:

- `status` text must deserialize into `RunStatus`;
- iteration-row `status` text must deserialize into `RunIterationStatus`;
- `step` text must deserialize into `StepKind`;
- `result_json` must deserialize into `StepResultEnvelope` when present;
- `error_json` must deserialize into `StepError` when present;
- successful `StepResultEnvelope` variant must match `step`;
- step-specific `StepError` variant must match `step`.

If any required validation fails, the storage layer must return a typed storage
error rather than a partially reconstructed `RunState`.

## 12) Write Behavior Expectations

Canonical write behavior:

- inserts and updates must preserve the run / iteration / step-record hierarchy;
- storage must not reorder iterations or step records during persistence;
- storage must not overwrite one step payload with another incompatible payload
  shape;
- storage must not silently coerce `pending` and `finished` rows into one
  another.

The current contract does not require table-per-step write paths.

## 13) Executable SQL Schema

The executable SQL schema for this storage contract must live at:

- `Execution/docker/postgres/init/102_diagnostics_run_state.sql`

That SQL file must stay semantically aligned with this contract.
