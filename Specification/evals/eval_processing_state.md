## 1) Purpose

This document defines the eval-owned processing-state contract for the new
diagnostics eval engine.

`eval_processing_state` is the canonical scheduling and resume table for
eval-run work.

It exists because the eval engine must support:

- frozen eval-run scope;
- resumable stage progress;
- idempotent replay after interruption;
- status tracking independent of runtime-owned `RunState`.

`RunState` remains the runtime source of truth for diagnostic execution
artifacts, but it is not the eval engine's scheduling ledger.

## 2) Granularity

One `eval_processing_state` row represents one eval-owned subject:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`

The current MVP uses one row per evaluated iteration subject.

## 3) Required Fields

Each row must contain at least:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`
- `subject_received_at`
- `current_stage`
- `status`
- `attempt_count`
- `started_at`
- `completed_at`
- `updated_at`
- `last_error`

## 4) Required Enums

### 4.1) `current_stage`

Allowed MVP stages:

- `judge_request_suites`
- `build_eval_summary`

### 4.2) `status`

Allowed statuses:

- `pending`
- `running`
- `completed`
- `failed`

## 5) Bootstrap Rule

At eval-run bootstrap:

- one processing-state row must be created for every
  `(eval_run_id, runtime_run_id, iteration_id)` subject in the frozen scope;
- new rows must be initialized with:
  - `current_stage = judge_request_suites`
  - `status = pending`
  - `attempt_count = 0`

For the current MVP, bootstrap rows must be created from the manifest's frozen
subject scope rather than by re-deriving iteration selection ad hoc on resume.

The bootstrap logic must not create rows for runtime runs outside the frozen
scope of the current eval run.

## 6) FIFO Rule

Eligible rows must be processed in stable FIFO order of:

1. `subject_received_at ASC`
2. `runtime_run_id ASC`
3. `iteration_id ASC`

Rows with `pending`, `running`, or `failed` status must be considered in that
same source order.

`status` is part of eligibility filtering, not FIFO ordering priority.

## 7) Resume Rule

The processing-state table is the primary stage-progress ledger used during
resume.

On resume:

- rows in `pending`, `running`, or `failed` state remain eligible;
- rows in `completed` state remain satisfied and must not be re-created;
- stage workers must still inspect downstream result tables before reissuing
  judge calls or rebuilding summaries.

This means resume uses both:

- `eval_processing_state` for scheduling intent;
- downstream result tables for idempotent completion checks.

## 8) Stage Ownership

`judge_request_suites` owns transitions for rows where
`current_stage = judge_request_suites`.

`build_eval_summary` owns transitions for rows where
`current_stage = build_eval_summary`.

The orchestrator owns:

- initial bootstrap creation;
- promotion of rows from one completed stage to the next stage's pending state;
- final run-completion checks across the frozen eval-run scope.

## 9) Promotion Rule

When a subject completes `judge_request_suites`, the orchestrator must promote
that subject to:

- `current_stage = build_eval_summary`
- `status = pending`

unless the current pipeline shape later changes to support additional
intermediate stages.

The original subject identity must remain unchanged across stage promotion.

## 10) Terminal Subject Success

For the current MVP, a subject reaches terminal eval-stage success when:

- `current_stage = build_eval_summary`
- `status = completed`

This does not by itself mark the whole eval run completed; the orchestrator
must still verify that every subject in the frozen scope has reached that
terminal state.

## 11) Failure Rule

If a stage fails for a subject:

- that stage must update the row to `status = failed`;
- `last_error` must be populated;
- `completed_at` must remain unset for non-terminal unsuccessful work;
- the row must remain resumable.

The processing-state table must preserve enough failure detail for later resume
and diagnosis.
