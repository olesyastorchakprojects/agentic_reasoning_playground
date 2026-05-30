## 1) Purpose

This document defines the current module contract for the eval crate's
`storage` module.

This module is the database access boundary for eval-owned relational state and
the source of truth for scheduling semantics used by the current eval engine.

## 2) Responsibilities

The storage module owns:

- PostgreSQL connection setup for the eval crate;
- row/domain types for eval-owned tables;
- subject discovery queries;
- processing-state bootstrap and updates;
- FIFO-like subject selection for stage workers;
- idempotent upserts for judge rows and summary rows;
- list/read helpers used by CLI finalization and reporting.

## 3) Non-Responsibilities

The storage module must not own:

- `RunState` interpretation beyond query predicates;
- judge prompt construction;
- markdown formatting;
- process-wide CLI wiring.

## 4) Required Public Row Types

The current module exposes row/domain types including:

- `EvalSubjectKey`
- `FrozenEvalSubject`
- `EvalProcessingStateRow`
- `JudgeResultRow`
- `JudgeLlmCallRow`
- `EvalIterationSummaryRow`
- `EvalRunSummaryRow`

## 5) Discovery Semantics

The current storage module owns frozen-subject discovery for new eval runs.

Current query semantics:

- discover runtime runs that contain at least one iteration with a finished
  `ResponseValidationAndNormalization` result;
- order those runtime runs by runtime-run creation time descending, then
  `runtime_run_id` descending;
- apply the optional limit at the runtime-run level;
- from those selected runtime runs, return every iteration that satisfies the
  same final-output condition as a frozen eval subject.

Each discovered subject is materialized as:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`
- `subject_received_at`

## 6) Processing-State Ownership

The storage module owns persistence for `eval_processing_state`, including:

- bootstrap inserts for frozen subjects;
- next-subject selection for a stage;
- stage/status/attempt updates;
- ordered listing for inspection and debugging.

Current status set:

- `pending`
- `running`
- `completed`
- `failed`

Current stage set:

- `judge_request_suites`
- `build_eval_summary`

## 7) Scheduling Rule

The current `fetch_next_subject_for_stage(...)` behavior selects the next
eligible subject when:

- `eval_run_id` matches;
- `current_stage` matches;
- `status` is in `pending|running|failed`;
- `attempt_count < MAX_ATTEMPTS_PER_STAGE`.

Current retry ceiling:

- `MAX_ATTEMPTS_PER_STAGE = 2`

Current ordering:

- `subject_received_at ASC`
- `runtime_run_id ASC`
- `sequence_no ASC`
- `iteration_id ASC`

This is the current scheduling contract even though the bootstrap discovery
order is descending by recency.

## 8) Idempotency Rules

The storage module owns idempotent writes for:

- `judge_results`
- `judge_llm_calls`
- `eval_iteration_summaries`
- `eval_run_summaries`

Current write semantics include:

- `judge_results` upsert on subject key plus `suite_name`;
- `judge_llm_calls` insert with `ON CONFLICT (call_id) DO NOTHING`;
- summary rows are upserted by their natural eval-owned keys.

## 9) Important Current Limitation

The current new-run discovery query only checks that the subject is absent from
`eval_processing_state` for the freshly generated `eval_run_id`.

That means the query does not yet fully enforce the stronger design goal of
excluding subjects already absorbed into any other eval run.

The spec should describe this as the current implementation truth rather than
claiming stronger behavior that does not yet exist in code.
