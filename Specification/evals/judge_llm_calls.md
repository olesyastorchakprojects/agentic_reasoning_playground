## 1) Purpose

This document defines the factual judge-call usage storage contract for the new
diagnostic eval engine.

`judge_llm_calls` is the canonical source for:

- judge token accounting;
- judge cost accounting;
- judge provider/model metadata by suite execution;
- raw factual call audit rows.

This table is the usage and cost foundation for:

- eval run reports;
- dashboard queries;
- run-level and iteration-level rollups.

## 2) Granularity

One `judge_llm_calls` row represents one factual judge-model call that returned
one model response payload.

The table must not aggregate multiple judge calls into one row.

## 3) Required Identity Fields

Each row must contain at least:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`
- `suite_name`
- `stage_name`
- `call_id`
- `created_at`

The current stage name for MVP is:

- `judge_request_suites`

## 4) Required Usage Fields

Each factual usage row must contain at least:

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

## 5) Token Count Semantics

The engine must extract prompt and completion usage conservatively and
deterministically.

Accepted sources include:

- provider-native usage fields when present;
- provider-specific native fields such as Ollama-native counts when applicable;
- local tokenizer-based estimates when provider usage is absent and the engine
  has enough information to compute a stable estimate.

The actual source used must be written into `token_count_source`.

The engine must not silently invent token counts without recording how they
were produced.

## 6) Cost Semantics

The cost formula must be:

- `prompt_cost_usd = prompt_tokens * input_cost_per_million_tokens / 1_000_000`
- `completion_cost_usd = completion_tokens * output_cost_per_million_tokens / 1_000_000`
- `total_cost_usd = prompt_cost_usd + completion_cost_usd`

The table must preserve both pricing inputs and computed outputs.

## 7) Write Rule

The engine must write the `judge_llm_calls` row whenever the judge model
returns a response payload, even if:

- normalization of that response later fails;
- downstream semantic persistence later fails.

This rule exists because factual usage and cost must not be lost when semantic
post-processing fails.

## 8) Aggregate Ownership

`judge_llm_calls` is the canonical judge usage ledger.

All judge usage rollups in reports and dashboards must be derivable from this
table.

The engine may materialize rollups elsewhere, but those rollups must be
derived views over this canonical source rather than independent competing
truth sources.
