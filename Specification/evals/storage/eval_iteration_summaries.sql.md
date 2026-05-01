## 1) Purpose

This document defines the physical PostgreSQL contract for
`eval_iteration_summaries`.

This table stores one denormalized iteration-level summary row per evaluated
subject.

## 2) Table Shape

Recommended minimum columns:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`
- `query_structuring_judge_score`
- `evidence_pack_judge_score`
- `final_answer_judge_score`
- `query_structuring_no_hard_fail`
- `evidence_pack_no_hard_fail`
- `final_answer_no_hard_fail`
- `usable_first_response`
- `no_root_cause_gate_passed`
- `single_check_gate_passed`
- `source_alignment_gate_passed`
- `field_boundary_gate_passed`
- `evidence_pack_gate_passed`
- explicit per-suite score columns for the MVP suite set
- `runtime_prompt_tokens`
- `runtime_completion_tokens`
- `runtime_total_tokens`
- `runtime_total_cost_usd`
- `judge_prompt_tokens`
- `judge_completion_tokens`
- `judge_total_tokens`
- `judge_total_cost_usd`
- `run_total_tokens`
- `run_total_cost_usd`
- `created_at`
- `updated_at`

Recommended PostgreSQL-oriented types:

- ids: `text`
- score fields: `numeric` or `double precision` if averages are fractional
- per-suite scores: `smallint`
- booleans: `boolean`
- token counts: `integer`
- costs: `numeric`
- timestamps: `timestamptz`

## 3) Uniqueness

The canonical uniqueness key must be:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`

Recommended constraint:

```sql
unique (eval_run_id, runtime_run_id, iteration_id)
```

## 4) Required Indexes

Recommended indexes:

1. unique subject row
```sql
unique index eval_iteration_summaries_subject_uq
on eval_iteration_summaries (eval_run_id, runtime_run_id, iteration_id);
```

2. run-level aggregation
```sql
index eval_iteration_summaries_run_idx
on eval_iteration_summaries (eval_run_id);
```

3. worst-case preview helpers
```sql
index eval_iteration_summaries_final_score_idx
on eval_iteration_summaries (eval_run_id, final_answer_judge_score);
```

4. usability and gate filtering
```sql
index eval_iteration_summaries_usable_idx
on eval_iteration_summaries (eval_run_id, usable_first_response);
```

## 5) Upsert Contract

The summary builder must upsert one row after all required suites exist for the
subject.

Recommended behavior:

```sql
insert into eval_iteration_summaries (...)
values (...)
on conflict (eval_run_id, runtime_run_id, iteration_id)
do update
set
  query_structuring_judge_score = excluded.query_structuring_judge_score,
  evidence_pack_judge_score = excluded.evidence_pack_judge_score,
  final_answer_judge_score = excluded.final_answer_judge_score,
  query_structuring_no_hard_fail = excluded.query_structuring_no_hard_fail,
  evidence_pack_no_hard_fail = excluded.evidence_pack_no_hard_fail,
  final_answer_no_hard_fail = excluded.final_answer_no_hard_fail,
  usable_first_response = excluded.usable_first_response,
  no_root_cause_gate_passed = excluded.no_root_cause_gate_passed,
  single_check_gate_passed = excluded.single_check_gate_passed,
  source_alignment_gate_passed = excluded.source_alignment_gate_passed,
  field_boundary_gate_passed = excluded.field_boundary_gate_passed,
  evidence_pack_gate_passed = excluded.evidence_pack_gate_passed,
  runtime_prompt_tokens = excluded.runtime_prompt_tokens,
  runtime_completion_tokens = excluded.runtime_completion_tokens,
  runtime_total_tokens = excluded.runtime_total_tokens,
  runtime_total_cost_usd = excluded.runtime_total_cost_usd,
  judge_prompt_tokens = excluded.judge_prompt_tokens,
  judge_completion_tokens = excluded.judge_completion_tokens,
  judge_total_tokens = excluded.judge_total_tokens,
  judge_total_cost_usd = excluded.judge_total_cost_usd,
  run_total_tokens = excluded.run_total_tokens,
  run_total_cost_usd = excluded.run_total_cost_usd,
  updated_at = excluded.updated_at;
```

The actual statement may include the explicit per-suite score columns as well.

## 6) Integrity Rules

Recommended checks:

- token counts must be non-negative;
- costs must be non-negative;
- `run_total_tokens = runtime_total_tokens + judge_total_tokens`;
- `run_total_cost_usd = runtime_total_cost_usd + judge_total_cost_usd`.

Average score fields may remain unconstrained beyond sane numeric types because
they are derived aggregates rather than discrete labels.

## 7) Rewrite Rule

This table is intentionally materialized and overwriteable.

Recomputing one subject-level summary on resume is allowed as long as the
subject identity key stays stable.
