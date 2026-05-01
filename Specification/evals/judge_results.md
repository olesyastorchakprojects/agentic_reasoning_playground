## 1) Purpose

This document defines the normalized semantic verdict storage contract for the
diagnostic eval engine.

`judge_results` is the canonical eval-owned table for suite verdicts after
successful normalization.

It must be distinct from:

- runtime-owned `RunState`;
- factual `judge_llm_calls` usage rows;
- aggregate summary tables.

## 2) Granularity

One `judge_results` row represents exactly one:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`
- `suite_name`

combination.

The table must not store multiple semantic suite verdicts in one row.

## 3) Required Key

The canonical uniqueness key must be:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`
- `suite_name`

This uniqueness rule is required for idempotent resume.

## 4) Required Fields

Each row must contain at least:

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

## 5) Field Semantics

### 5.1) `score`

For the current MVP, every normalized suite verdict must expose one score in:

- `0`
- `1`
- `2`

The table must preserve this score exactly as normalized.

### 5.2) `normalized_result_json`

This field is the canonical machine-readable payload for suite-specific details.

It must preserve the normalized structured output for that suite, such as:

- `wrong_fields`
- `unsupported_terms`
- `bad_chunks`
- `violations`
- `is_single_check`
- `misaligned_hypotheses`

The current MVP should prefer JSON-first storage here rather than immediate
wide denormalization of suite-specific details.

### 5.3) `explanation`

- must contain the suite-level short explanation returned by the normalized
  judge output;
- may duplicate a field inside `normalized_result_json`, but must remain
  queryable as a first-class textual field.

### 5.4) `failure_code`

- may be `NULL`;
- should be populated when the normalized suite verdict exposes a stable
  high-signal failure taxonomy code;
- exists to support later aggregate breakdowns without text clustering.

### 5.5) `raw_response`

- must preserve the normalized suite's underlying raw judge response payload
  used for audit and debugging;
- is eval-owned storage, not a trace attribute.

## 6) Write Rules

The eval engine must write a `judge_results` row only after:

- the judge response has been parsed;
- the response has been normalized into the suite's expected shape.

If normalization fails:

- the engine must not invent a fallback semantic score;
- the engine must treat the suite execution as failed for that subject;
- the factual `judge_llm_calls` row must still be preserved separately.

## 7) Idempotency Rules

The engine must not create semantically duplicate verdict rows for the same:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`
- `suite_name`

On resume, already-existing rows for that key must be treated as already
satisfied unless the design later introduces an explicit invalidation policy.

## 8) Summary Ownership

`judge_results` is the raw semantic verdict layer.

It must not be overloaded with:

- eval-run-level aggregate rollups;
- token or cost totals;
- dashboard comparison rows.

Those belong to separate summary tables.
