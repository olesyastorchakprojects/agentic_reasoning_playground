## 1) Purpose

This document defines the physical PostgreSQL contract for
`eval_run_summaries`.

This table stores one aggregate row per eval run for reporting and dashboards.

## 2) Table Shape

Recommended minimum columns:

- `eval_run_id`
- `run_type`
- `status`
- `started_at`
- `completed_at`
- `runtime_run_count`
- `iterations_evaluated_count`
- `judge_provider`
- `judge_model`
- `suite_versions`
- `usable_first_response_rate`
- `query_structuring_judge_score`
- `evidence_pack_judge_score`
- `final_answer_judge_score`
- `query_structuring_strict_pass_rate`
- `evidence_pack_strict_pass_rate`
- `final_answer_strict_pass_rate`
- `diagnostic_move_hard_fail_rate`
- `gate_pass_rate`
- `bad_final_due_to_query_rate`
- `bad_final_due_to_evidence_rate`
- `bad_final_with_good_query_and_evidence_rate`
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

- identifiers and status fields: `text`
- `suite_versions`: `jsonb`
- rates and averages: `numeric` or `double precision`
- token counts: `integer`
- costs: `numeric`
- timestamps: `timestamptz`

## 3) Uniqueness

The canonical uniqueness key must be:

- `eval_run_id`

Recommended constraint:

```sql
unique (eval_run_id)
```

## 4) Required Indexes

Recommended indexes:

1. unique eval-run row
```sql
unique index eval_run_summaries_eval_run_uq
on eval_run_summaries (eval_run_id);
```

2. dashboard time-range listing
```sql
index eval_run_summaries_started_at_idx
on eval_run_summaries (started_at desc);
```

3. status filtering
```sql
index eval_run_summaries_status_idx
on eval_run_summaries (status, started_at desc);
```

4. quality comparison helper
```sql
index eval_run_summaries_quality_idx
on eval_run_summaries (usable_first_response_rate, run_total_cost_usd);
```

## 5) Upsert Contract

The summary builder may refresh this row incrementally during stage drain, and
the orchestrator may rely on the final row at terminal success.

Recommended behavior:

```sql
insert into eval_run_summaries (...)
values (...)
on conflict (eval_run_id)
do update
set
  run_type = excluded.run_type,
  status = excluded.status,
  started_at = excluded.started_at,
  completed_at = excluded.completed_at,
  runtime_run_count = excluded.runtime_run_count,
  iterations_evaluated_count = excluded.iterations_evaluated_count,
  judge_provider = excluded.judge_provider,
  judge_model = excluded.judge_model,
  suite_versions = excluded.suite_versions,
  usable_first_response_rate = excluded.usable_first_response_rate,
  query_structuring_judge_score = excluded.query_structuring_judge_score,
  evidence_pack_judge_score = excluded.evidence_pack_judge_score,
  final_answer_judge_score = excluded.final_answer_judge_score,
  query_structuring_strict_pass_rate = excluded.query_structuring_strict_pass_rate,
  evidence_pack_strict_pass_rate = excluded.evidence_pack_strict_pass_rate,
  final_answer_strict_pass_rate = excluded.final_answer_strict_pass_rate,
  diagnostic_move_hard_fail_rate = excluded.diagnostic_move_hard_fail_rate,
  gate_pass_rate = excluded.gate_pass_rate,
  bad_final_due_to_query_rate = excluded.bad_final_due_to_query_rate,
  bad_final_due_to_evidence_rate = excluded.bad_final_due_to_evidence_rate,
  bad_final_with_good_query_and_evidence_rate = excluded.bad_final_with_good_query_and_evidence_rate,
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

## 6) Integrity Rules

Recommended checks:

- counts and token totals must be non-negative;
- cost totals must be non-negative;
- `run_total_tokens = runtime_total_tokens + judge_total_tokens`;
- `run_total_cost_usd = runtime_total_cost_usd + judge_total_cost_usd`.

## 7) Dashboard Role

This table is the primary relational dashboard source for:

- eval-run listings;
- quality/cost comparisons;
- aggregate usage totals.

It must stay materialized and query-friendly rather than forcing dashboard
queries to rebuild everything from raw subject rows every time.
