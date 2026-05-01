## 1) Purpose

This document defines the physical PostgreSQL contract for
`eval_processing_state`.

This table is the canonical eval-owned scheduling ledger for resumable
subject-level work.

## 2) Table Shape

Recommended minimum columns:

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

Recommended PostgreSQL-oriented types:

- ids: `text` or `uuid`
- timestamps: `timestamptz`
- `current_stage`: `text`
- `status`: `text`
- `attempt_count`: `integer`
- `last_error`: `text`

## 3) Uniqueness

The canonical uniqueness key must be:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`

Recommended constraint:

```sql
unique (eval_run_id, runtime_run_id, iteration_id)
```

This key enforces one scheduling row per frozen eval subject.

## 4) Allowed Values

Recommended stage values for MVP:

- `judge_request_suites`
- `build_eval_summary`

Recommended status values for MVP:

- `pending`
- `running`
- `completed`
- `failed`

The implementation may use `CHECK` constraints or application-level enum
validation.

## 5) Required Indexes

Recommended indexes:

1. primary lookup / uniqueness
```sql
unique index eps_subject_uq
on eval_processing_state (eval_run_id, runtime_run_id, iteration_id);
```

2. FIFO work selection
```sql
index eps_stage_queue_idx
on eval_processing_state (
  eval_run_id,
  current_stage,
  status,
  subject_received_at,
  runtime_run_id,
  iteration_id
);
```

3. run-level completion checks
```sql
index eps_run_completion_idx
on eval_processing_state (eval_run_id, current_stage, status);
```

## 6) Insert Contract

At bootstrap, the orchestrator must insert one row per frozen subject.

Recommended semantics:

- if the row does not exist, insert it;
- if the row already exists for the same subject during resume bootstrap, do
  not create a duplicate;
- resume must not mutate subject identity.

The implementation may use:

```sql
insert into eval_processing_state (...)
values (...)
on conflict (eval_run_id, runtime_run_id, iteration_id) do nothing;
```

## 7) Update Contract

Stage workers and the orchestrator may update only:

- `current_stage`
- `status`
- `attempt_count`
- `started_at`
- `completed_at`
- `updated_at`
- `last_error`

They must never rewrite:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`
- `subject_received_at`

## 8) Resume Semantics

The table must preserve enough information to distinguish:

- never-started work;
- in-flight work interrupted mid-stage;
- failed but resumable work;
- terminal subject success.

Recommended operational rule:

- rows with `status in ('pending', 'running', 'failed')` remain eligible after
  downstream idempotency checks;
- rows with `status = 'completed'` are considered satisfied for the current
  `current_stage`.

## 9) Write Pattern

`eval_processing_state` is an upsert-and-update table, not an append-only
ledger.

The engine should prefer:

- insert-once bootstrap;
- targeted updates for transitions;
- no delete-on-retry behavior.
