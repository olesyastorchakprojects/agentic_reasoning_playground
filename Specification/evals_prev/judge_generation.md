## 1) Purpose / Scope

`judge_generation` evaluates generation quality for one eligible request, writes factual judge-call usage rows into `judge_llm_calls`, and writes normalized generation-judge rows into `judge_generation_results`.

The observability contract for the current eval engine is defined in:

- `Specification/codegen/evals/observability.md`

This stage:

- selects the next FIFO-eligible request from `eval_processing_state`;
- reads the corresponding `request_captures` row;
- evaluates the request against the configured generation judge suites;
- writes one factual usage row per executed generation-suite judge call;
- writes one normalized result row per executed generation suite;
- marks its own stage as completed only after all required generation-suite rows have been persisted successfully.

This stage does not:

- write `request_captures`;
- write `judge_retrieval_results`;
- write `request_summaries`;
- skip failed requests silently;
- advance a request to the next stage after partial success.

For the current version, this stage must comply with both:

- the trace requirements in `Specification/codegen/evals/observability.md`;
- the required console logging contract in `Specification/codegen/evals/observability.md`.

## 2) Stage Operation Contract

`judge_generation` is a request-level stage.

For the current version, the stage must be implemented as an in-process Python module entrypoint invoked by `eval_orchestrator`.

The canonical executable module path for the current version is:

- `Execution/evals/judge_generation.py`

One stage invocation must process at most one request.

The stage must:

- select the next FIFO-eligible row where:
  - `current_stage = judge_generation`
  - `status != completed`
- process that request completely for all required generation judge suites that are still missing.

If no eligible work exists, the invocation must terminate without side effects.

For the current version, a FIFO-eligible request means:

- a row in `eval_processing_state` where:
  - `current_stage = judge_generation`
  - `status != completed`
- eligible rows are ordered in ascending FIFO order of:
  - `request_received_at`
  - `request_id`

`status != completed` is the eligibility filter only.

It does not define FIFO order by itself.

The FIFO ordering key is always:

1. `request_received_at ASC`
2. `request_id ASC`

Rows with `pending`, `running`, or `failed` status must be considered in this same source order, not in status-priority order.

`judge_generation` owns only FIFO selection for its own stage.

It must not infer readiness for later stages.

## 3) Configuration Usage

`judge_generation` must receive one explicit parameter object:

- `JudgeGenerationParams`

For the current version, required fields are:

- `postgres_url`
- `run_id`
- `judge_settings`
- `chunks_path`

Field-source rules:

- `postgres_url` points to the eval storage PostgreSQL instance;
- `run_id` identifies the current eval run and must match the current `run_manifest.json`;
- `judge_settings` contains the resolved judge runtime for the current run;
- `judge_settings.provider` must be one of:
  - `ollama`
  - `together`
- `judge_settings.model_name` is the judge model identifier used for `chat.completions.create(...)`;
- `judge_settings.base_url` is the effective OpenAI-compatible base URL used for judge calls;
- `chunks_path` points to the canonical `chunks.jsonl` file for the corpus version being evaluated.

The stage must not:

- read raw environment variables directly;
- hardcode `run_id`;
- infer `run_id` from database state;
- infer chunk text from storage rows that do not contain it.

Parameter-passing rules:

- for Python implementation, explicit parameters are preferred over a crate-style nested settings model;
- parameters may be provided through a small typed parameter object or another equally explicit in-process boundary;
- the stage must not depend on ambient process state as its primary configuration mechanism.

Current-version callable boundary:

- the canonical Python entrypoint is `run_judge_generation(params: JudgeGenerationParams) -> bool`

## 4) Input Records And Required Fields

The stage reads:

- one `eval_processing_state` row for scheduling and state transition;
- one `request_captures` row for request-level evaluation input;
- chunk records resolved from `chunks_path` when a generation suite requires chunk text.

From `eval_processing_state`, the stage uses:

- `request_id`
- `request_received_at`
- `current_stage`
- `status`
- `attempt_count`

From `request_captures`, the stage uses:

- `request_id`
- `trace_id`
- `normalized_query`
- `final_answer`
- `retrieval_results`

The stage ignores per-item `rerank_score`; that field is preserved in the capture payload for downstream analysis and is not part of generation judge input construction.

The stage must not depend on Phoenix traces or span payloads.

## 5) Suite Catalog And Input Construction

For the current version, the required generation suite catalog matches `judge_generation_suite`:

- `answer_completeness`
- `groundedness`
- `answer_relevance`
- `correct_refusal`

Each suite must have its own:

- stable `suite_name`;
- prompt template;
- `judge_prompt_version`.

The canonical prompt source of truth for the current generation suites is:

- `Specification/codegen/evals/prompts.json`

The observability contract for the current eval engine is defined in:

- `Specification/codegen/evals/observability.md`

Prompt rules:

- each generation suite must read its `id`, `version`, `prompt_template`, and `input_variables` from `Specification/codegen/evals/prompts.json`;
- `judge_prompt_version` written to storage must come from that file's suite entry;
- the stage must not hardcode alternative prompt templates or prompt content that diverges from `Specification/codegen/evals/prompts.json`.

Per-suite input construction rules:

- `answer_completeness` uses:
  - `normalized_query`
  - `final_answer`
- `answer_relevance` uses:
  - `normalized_query`
  - `final_answer`
- `correct_refusal` uses:
  - `normalized_query`
  - `final_answer`
- `groundedness` uses:
  - `normalized_query`
  - `final_answer`
  - the text of all retrieval items where `selected_for_generation = true`

Groundedness chunk-resolution rules:

- the stage must resolve selected chunks by `chunk_id` from `request_captures.retrieval_results`;
- the stage must load chunk text from `JudgeGenerationParams.chunks_path`;
- selected chunk order must follow `request_captures.retrieval_results` rank order;
- if any selected `chunk_id` cannot be resolved to chunk text, the request must fail the `judge_generation` stage and must not be marked `completed`.

## 6) Processing Algorithm

The required stage algorithm is:

1. select the next FIFO-eligible `eval_processing_state` row for `judge_generation`;
2. if no row exists, terminate without side effects;
3. transition the state row to:
   - `current_stage = judge_generation`
   - `status = running`
   - `attempt_count = attempt_count + 1`
   - `started_at = now()`
   - `updated_at = now()`
4. load the corresponding `request_captures` row by `request_id`;
5. inspect existing `judge_generation_results` rows for the same `(request_id, run_id)`;
6. determine which required generation suites are still missing;
7. construct per-suite judge inputs only for missing suites;
8. call the configured judge model only for missing suites;
9. extract token usage and cost for each successful judge-model response and persist one storage row in `judge_llm_calls`;
   - the stored factual call row must include the full serialized raw judge-model response payload;
   - this row must be written even when downstream verdict normalization later fails for that response;
10. normalize each missing-suite response into one storage row for `judge_generation_results`;
11. persist only missing suite rows for the request;
11. verify that all required generation suite rows now exist for the same `(request_id, run_id)`;
12. transition the state row to:
    - `current_stage = judge_generation`
    - `status = completed`
    - `updated_at = now()`
13. clear `last_error` on successful stage completion.

The stage must not advance the request into another stage.

Advancement from `judge_generation / completed` into the next stage is owned by eval-pipeline state-machine logic outside this module.

State-update boundary rule:

- the stage must mutate `eval_processing_state` only for the single `request_id` currently being processed;
- the stage must not bulk-update state for other eligible requests before those requests are actually processed.

Completion criterion:

- `judge_generation` is complete for a request iff all required generation-suite rows exist for that `(request_id, run_id)`:
  - `answer_completeness`
  - `groundedness`
  - `answer_relevance`
  - `correct_refusal`

## 7) Output Mapping Contract

The canonical storage contract is defined in:

- `Specification/contracts/storage/judge_llm_calls_storage.md`
- `Specification/contracts/storage/judge_generation_results_storage.md`

The executable SQL schema is defined in:

- `Execution/docker/postgres/init/007_judge_llm_calls.sql`
- `Execution/docker/postgres/init/002_judge_generation_results.sql`

Field-source rules for `judge_llm_calls`:

- one row must be written for each factual generation-suite judge call that returns a response;
- `stage_name` must be `judge_generation`;
- `suite_name` must equal the evaluated generation suite;
- `chunk_id` must be null;
- `judge_provider`, `judge_model`, `judge_prompt_version`, and pricing fields come from the resolved `judge_settings` and suite prompt metadata;
- token counts must be sourced in this priority order:
  - provider usage counts from `usage.prompt_tokens` and `usage.completion_tokens`
  - Ollama-native counts from `prompt_eval_count` and `eval_count`
  - local fallback token counting using the configured judge tokenizer
- `judge_llm_calls` is the canonical future source for eval token-usage and cost aggregation by `run_id`.

The stage must write one row per:

- `request_id`
- `run_id`
- `suite_name`

Field-source rules for `judge_generation_results`:

- `request_id` comes from `request_captures.request_id`;
- `run_id` comes from `JudgeGenerationParams.run_id`;
- `trace_id` comes from `request_captures.trace_id`;
- `suite_name` comes from the current generation judge suite;
- `judge_model` comes from `JudgeGenerationParams.judge_model`;
- `judge_prompt_version` comes from the suite-owned prompt-version constant;
- `score`, `label`, `explanation`, and `raw_response` come from normalized judge output for the suite.

Current-version label-to-score mapping:

- `answer_completeness.complete -> 1.0`
- `answer_completeness.partial -> 0.5`
- `answer_completeness.incomplete -> 0.0`
- `groundedness.grounded -> 1.0`
- `groundedness.partially_grounded -> 0.5`
- `groundedness.ungrounded -> 0.0`
- `answer_relevance.relevant -> 1.0`
- `answer_relevance.partially_relevant -> 0.5`
- `answer_relevance.irrelevant -> 0.0`
- `correct_refusal.correct_refusal -> 1.0`
- `correct_refusal.unnecessary_refusal -> 0.0`
- `correct_refusal.non_refusal -> 0.0`

If the judge response cannot be normalized into one of the expected labels for the current suite, the stage must treat that suite evaluation as failed rather than inventing another score.

`raw_response` rules:

- `raw_response` must be stored as a JSON object;
- it must preserve the raw judge output needed for audit and debugging;
- it must not be stored as an untyped plain string column.

Idempotency rules:

- the stage must not create semantically duplicate rows for the same `(request_id, run_id, suite_name)`;
- the stage must inspect existing rows before issuing new suite writes;
- if a suite row already exists for the same `(request_id, run_id, suite_name)`, the stage must treat it as already satisfied and must not rewrite it;
- rerunning the stage for the same request must keep the resulting stored state logically idempotent.

## 8) State Transition Contract

This stage owns only the `judge_generation` transition.

Allowed successful transition:

- `judge_generation / pending -> judge_generation / running -> judge_generation / completed`

Allowed failure transition:

- `judge_generation / pending -> judge_generation / running -> judge_generation / failed`

On failure:

- `current_stage` must remain `judge_generation`;
- `status` must become `failed`;
- `last_error` must be updated with a non-empty description;
- `updated_at` must be refreshed.

Recovery rules:

- the stage may pick rows in `judge_generation` with `status = pending`, `status = running`, or `status = failed`;
- before issuing new judge calls, the stage must inspect downstream `judge_generation_results` rows for the same `(request_id, run_id)`;
- the stage must compute and write only missing suite rows;
- `running` must be treated as resumable incomplete work, not as a permanently locked state.

## 9) Error Model

The stage must expose a clear stage-level failure boundary.

For Python implementation, failure categories may be represented via:

- exception classes;
- structured error records;
- another explicit stage-level error boundary.

Required failure categories:

- FIFO work-selection failure;
- request-capture lookup failure;
- chunk-resolution failure for groundedness;
- judge transport failure;
- judge response parsing failure;
- generation-result row mapping failure;
- database write failure;
- state-transition write failure;
- unexpected internal state.

Error rules:

- failures must preserve enough detail for later retry and debugging;
- raw client exceptions must not become the only persisted error signal;
- a request must not advance to another stage after any generation-suite failure.

## 10) Implementation Notes

The generated implementation should use:

- `psycopg` for PostgreSQL access;
- the Python `openai` client for OpenAI-compatible judge calls;
- Python `json` support for judge-response parsing and `raw_response` construction.

Operational precondition:

- the PostgreSQL eval schema, including `eval_processing_state` and `judge_generation_results`, must already exist before the stage is invoked.

Implementation rules:

- the stage should construct one chunk index from `chunks_path` and reuse it during the invocation;
- one stage invocation may reuse one PostgreSQL connection for the whole request;
- one stage invocation may reuse one OpenAI client for the whole request;
- database writes must use parameterized SQL;
- handwritten SQL interpolation is forbidden;
- judge prompts must be suite-owned constants or templates, not ad hoc inline strings assembled at call sites;
- the stage should keep request-level scheduling logic separate from per-suite evaluation logic;
- if the OpenAI client response is not directly JSON-serializable, the stage must convert it into a plain JSON object before writing `raw_response`.
- judge response parsing must tolerate:
  - raw JSON objects;
  - JSON objects wrapped in markdown code fences;
  - near-valid truncated JSON objects that are missing only trailing closing `}` characters.
