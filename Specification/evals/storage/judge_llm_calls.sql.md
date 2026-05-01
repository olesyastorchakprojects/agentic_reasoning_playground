## 1) Purpose

This document defines the physical PostgreSQL contract for `judge_llm_calls`.

`judge_llm_calls` is the factual append-oriented usage and cost ledger for
judge-model requests.

## 2) Table Shape

Recommended minimum columns:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`
- `suite_name`
- `stage_name`
- `call_id`
- `judge_provider`
- `judge_model`
- `judge_base_url`
- `judge_prompt_version`
- `token_count_source`
- `prompt_tokens`
- `completion_tokens`
- `total_tokens`
- `input_cost_per_million_tokens`
- `output_cost_per_million_tokens`
- `prompt_cost_usd`
- `completion_cost_usd`
- `total_cost_usd`
- `raw_response`
- `created_at`

Recommended PostgreSQL-oriented types:

- ids and names: `text`
- token counts: `integer`
- pricing/cost: `numeric`
- `raw_response`: `jsonb`
- `created_at`: `timestamptz`

## 3) Identity And Uniqueness

The table is factual and append-oriented.

The recommended uniqueness rule is:

- `call_id` is globally unique inside this table

Recommended constraint:

```sql
unique (call_id)
```

If the implementation prefers a scoped identifier, a stronger composite unique
key may be used:

```sql
unique (eval_run_id, runtime_run_id, iteration_id, suite_name, call_id)
```

## 4) Required Indexes

Recommended indexes:

1. global factual uniqueness
```sql
unique index judge_llm_calls_call_id_uq
on judge_llm_calls (call_id);
```

2. iteration rollups
```sql
index judge_llm_calls_subject_idx
on judge_llm_calls (eval_run_id, runtime_run_id, iteration_id);
```

3. suite-level usage inspection
```sql
index judge_llm_calls_suite_idx
on judge_llm_calls (eval_run_id, suite_name, created_at);
```

4. stage-level usage aggregation
```sql
index judge_llm_calls_stage_idx
on judge_llm_calls (eval_run_id, stage_name);
```

## 5) Insert Contract

The engine must insert one row whenever one judge call returns a response
payload.

This must happen even if:

- normalization fails;
- `judge_results` upsert fails later;
- summary building fails later.

Recommended write behavior:

- insert factual row first after usage extraction is available;
- never depend on successful semantic normalization before persisting usage.

## 6) Update Contract

`judge_llm_calls` should be treated as append-only for normal runtime behavior.

The MVP should avoid semantic in-place updates except for narrowly scoped
operational corrections such as backfilling a missing derived cost field during
the same transaction boundary.

## 7) Cost Constraints

Recommended integrity checks:

- token counts must be non-negative;
- USD amounts must be non-negative.

Illustrative examples:

```sql
check (prompt_tokens >= 0)
check (completion_tokens >= 0)
check (total_tokens >= 0)
check (prompt_cost_usd >= 0)
check (completion_cost_usd >= 0)
check (total_cost_usd >= 0)
```

The implementation may also choose to enforce:

```sql
check (total_tokens = prompt_tokens + completion_tokens)
```

if every provider path guarantees that equality.

## 8) Retention Rule

Because this table is the canonical factual usage ledger, rows must be
retained across resume and report regeneration.

Normal resume behavior must not delete prior factual rows.
