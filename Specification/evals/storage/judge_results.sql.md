## 1) Purpose

This document defines the physical PostgreSQL contract for `judge_results`.

`judge_results` stores one normalized semantic verdict per suite and subject.

## 2) Table Shape

Recommended minimum columns:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`
- `suite_name`
- `suite_id`
- `suite_version`
- `category`
- `scope`
- `judge_model`
- `judge_prompt_version`
- `score`
- `normalized_result_json`
- `explanation`
- `failure_code`
- `raw_response`
- `created_at`
- `updated_at`

Recommended PostgreSQL-oriented types:

- ids and names: `text`
- `score`: `smallint`
- `normalized_result_json`: `jsonb`
- `raw_response`: `jsonb`
- `created_at`, `updated_at`: `timestamptz`

## 3) Uniqueness

The canonical uniqueness key must be:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`
- `suite_name`

Recommended constraint:

```sql
unique (eval_run_id, runtime_run_id, iteration_id, suite_name)
```

## 4) Required Indexes

Recommended indexes:

1. subject-scope lookup
```sql
unique index judge_results_subject_suite_uq
on judge_results (
  eval_run_id,
  runtime_run_id,
  iteration_id,
  suite_name
);
```

2. summary-builder scan
```sql
index judge_results_subject_idx
on judge_results (eval_run_id, runtime_run_id, iteration_id);
```

3. suite analytics / debugging
```sql
index judge_results_suite_idx
on judge_results (eval_run_id, suite_name, score);
```

4. category aggregation
```sql
index judge_results_category_idx
on judge_results (eval_run_id, category);
```

Optional if PostgreSQL JSON querying becomes important:

```sql
index judge_results_normalized_gin_idx
on judge_results using gin (normalized_result_json);
```

## 5) Insert And Upsert Contract

The engine must write a row only after successful normalization.

Recommended semantics:

- first successful normalization inserts the row;
- repeated resume attempts for the same subject and suite should treat the
  existing row as already satisfied;
- if an explicit overwrite path is needed later, it must be intentional and
  version-aware.

Recommended MVP behavior:

```sql
insert into judge_results (...)
values (...)
on conflict (eval_run_id, runtime_run_id, iteration_id, suite_name)
do update
set
  suite_id = excluded.suite_id,
  suite_version = excluded.suite_version,
  category = excluded.category,
  scope = excluded.scope,
  judge_model = excluded.judge_model,
  judge_prompt_version = excluded.judge_prompt_version,
  score = excluded.score,
  normalized_result_json = excluded.normalized_result_json,
  explanation = excluded.explanation,
  failure_code = excluded.failure_code,
  raw_response = excluded.raw_response,
  updated_at = excluded.updated_at;
```

This upsert is acceptable because the row is semantic state, not factual call
history.

## 6) Nullability Rules

Recommended nullability:

- `failure_code` may be `NULL`
- `explanation` should not be `NULL`
- `normalized_result_json` should not be `NULL`
- `raw_response` should not be `NULL`

## 7) Score Constraint

Recommended MVP constraint:

```sql
check (score in (0, 1, 2))
```

## 8) Deletion Rule

The engine should not delete `judge_results` rows during normal resume.

If invalidation is ever introduced, it should be modeled explicitly rather than
by silent hard deletion.
