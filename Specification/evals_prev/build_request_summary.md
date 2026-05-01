## 1) Purpose / Scope

`build_request_summary` derives one request-level summary row from completed upstream eval results and writes it into `request_summaries`.

The derived summary row also copies reranking metadata from `request_captures` so downstream analysis can compare reranker usage without re-reading the raw capture table.

The observability contract for the current eval engine is defined in:

- `Specification/codegen/evals/observability.md`

This stage:

- selects the next FIFO-eligible request from `eval_processing_state`;
- reads the corresponding `request_captures` row;
- reads the completed generation-judge rows for that request and run;
- reads the completed retrieval-judge rows for that request and run;
- derives one current request-level summary row;
- upserts that row into `request_summaries`;
- upserts the same row into `request_run_summaries` keyed by `(request_id, run_id)`;
- marks its own stage as completed only after both summary rows have been written successfully.

This stage does not:

- write `request_captures`;
- write `judge_generation_results`;
- write `judge_retrieval_results`;
- generate new judge outputs;
- skip incomplete upstream requests silently;
- advance a request to another stage after success.

For the current version, this stage must comply with both:

- the trace requirements in `Specification/codegen/evals/observability.md`;
- the required console logging contract in `Specification/codegen/evals/observability.md`.

## 2) Stage Operation Contract

`build_request_summary` is a request-level stage.

For the current version, the stage must be implemented as an in-process Python module entrypoint invoked by `eval_orchestrator`.

The canonical executable module path for the current version is:

- `Execution/evals/build_request_summary.py`

One stage invocation must process at most one request.

The stage must:

- select the next FIFO-eligible row where:
  - `current_stage = build_request_summary`
  - `status != completed`
- process that request only if all required upstream inputs are fully available.

If no eligible work exists, the invocation must terminate without side effects.

For the current version, a FIFO-eligible request means:

- a row in `eval_processing_state` where:
  - `current_stage = build_request_summary`
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

## 3) Configuration Usage

`build_request_summary` must receive one explicit parameter object:

- `BuildRequestSummaryParams`

For the current version, required fields are:

- `postgres_url`
- `run_id`

Field-source rules:

- `postgres_url` points to the eval storage PostgreSQL instance;
- `run_id` identifies the current eval run and must match the current `run_manifest.json`.

The stage must not:

- read raw environment variables directly;
- hardcode `run_id`;
- infer `run_id` from database state.

Parameter-passing rules:

- for Python implementation, explicit parameters are preferred over a crate-style nested settings model;
- parameters may be provided through a small typed parameter object or another equally explicit in-process boundary;
- the stage must not depend on ambient process state as its primary configuration mechanism.

Current-version callable boundary:

- the canonical Python entrypoint is `run_build_request_summary(params: BuildRequestSummaryParams) -> bool`

## 4) Input Records And Required Fields

The stage reads:

- one `eval_processing_state` row for scheduling and state transition;
- one `request_captures` row for request-level source data;
- generation-judge rows from `judge_generation_results` for the same `(request_id, run_id)`;
- retrieval-judge rows from `judge_retrieval_results` for the same `(request_id, run_id)`.

From `eval_processing_state`, the stage uses:

- `request_id`
- `request_received_at`
- `current_stage`
- `status`
- `attempt_count`

From `request_captures`, the stage uses:

- `request_id`
- `trace_id`
- `received_at`
- `raw_query`
- `normalized_query`
- `input_token_count`
- `pipeline_config_version`
- `corpus_version`
- `retriever_version`
- `embedding_model`
- `reranker_kind`
- `reranker_config`
- `prompt_template_id`
- `prompt_template_version`
- `generation_model`
- `top_k_requested`
- `final_answer`
- `prompt_tokens`
- `completion_tokens`
- `total_tokens`
- `retrieval_stage_metrics`
- `reranking_stage_metrics`

From `judge_generation_results`, the stage uses:

- `suite_name`
- `score`
- `label`

From `judge_retrieval_results`, the stage uses:

- `score`
- `label`
- `retrieval_rank`
- `selected_for_generation`

## 5) Readiness Rule

`build_request_summary` must build a summary row only from fully satisfied upstream inputs.

For the current version, a request is ready for summary build iff:

- all required generation suite rows exist for the same `(request_id, run_id)`:
  - `answer_completeness`
  - `groundedness`
  - `answer_relevance`
  - `correct_refusal`
- all required retrieval rows exist for the exact expected retrieval key set for the same `(request_id, run_id)`.

Expected retrieval key set rule:

- the expected retrieval key set is constructed from the full ordered list `request_captures.retrieval_results`;
- for the current version, that means one expected row for each retrieval item using the key:
  - `(request_id, run_id, retrieval_relevance, chunk_id)`

If upstream inputs are incomplete, the stage must not write a partial summary row.

## 6) Processing Algorithm

The required stage algorithm is:

1. select the next FIFO-eligible `eval_processing_state` row for `build_request_summary`;
2. if no row exists, terminate without side effects;
3. transition the state row to:
   - `current_stage = build_request_summary`
   - `status = running`
   - `attempt_count = attempt_count + 1`
   - `started_at = now()`
   - `updated_at = now()`
4. load the corresponding `request_captures` row by `request_id`;
5. load generation-judge rows for the same `(request_id, run_id)`;
6. load retrieval-judge rows for the same `(request_id, run_id)`;
7. verify that the request satisfies the readiness rule;
8. derive one request-level summary row;
9. upsert the summary row into `request_summaries`;
10. transition the state row to:
    - `current_stage = build_request_summary`
    - `status = completed`
    - `updated_at = now()`
11. clear `last_error` on successful stage completion.

The stage must not advance the request into another stage.

`build_request_summary / completed` is the terminal success state for the current pipeline.

State-update boundary rule:

- the stage must mutate `eval_processing_state` only for the single `request_id` currently being processed;
- the stage must not bulk-update state for other eligible requests before those requests are actually processed.

## 7) Output Mapping Contract

The canonical storage contracts are defined in:

- `Specification/contracts/storage/request_summaries_storage.md`
- `Specification/contracts/storage/request_run_summaries_storage.md`

The executable SQL schemas are defined in:

- `Execution/docker/postgres/init/004_request_summaries.sql`
- `Execution/docker/postgres/init/006_request_run_summaries.sql`

The stage must write one row per:

- `request_id`

Scalar field mapping rules:

- `request_summaries.request_id <- request_captures.request_id`
- `request_summaries.trace_id <- request_captures.trace_id`
- `request_summaries.source_received_at <- request_captures.received_at`
- `request_summaries.pipeline_config_version <- request_captures.pipeline_config_version`
- `request_summaries.corpus_version <- request_captures.corpus_version`
- `request_summaries.retriever_version <- request_captures.retriever_version`
- `request_summaries.embedding_model <- request_captures.embedding_model`
- `request_summaries.reranker_kind <- request_captures.reranker_kind`
- `request_summaries.reranker_config <- request_captures.reranker_config`
- `request_summaries.prompt_template_id <- request_captures.prompt_template_id`
- `request_summaries.prompt_template_version <- request_captures.prompt_template_version`
- `request_summaries.generation_model <- request_captures.generation_model`
- `request_summaries.raw_query <- request_captures.raw_query`
- `request_summaries.normalized_query <- request_captures.normalized_query`
- `request_summaries.input_token_count <- request_captures.input_token_count`
- `request_summaries.top_k_requested <- request_captures.top_k_requested`
- `request_summaries.final_answer <- request_captures.final_answer`
- `request_summaries.prompt_tokens <- request_captures.prompt_tokens`
- `request_summaries.completion_tokens <- request_captures.completion_tokens`
- `request_summaries.total_tokens <- request_captures.total_tokens`

Generation summary mapping rules:

- `answer_completeness_score` and `answer_completeness_label` come from the `judge_generation_results` row where `suite_name = answer_completeness`
- `groundedness_score` and `groundedness_label` come from the `judge_generation_results` row where `suite_name = groundedness`
- `answer_relevance_score` and `answer_relevance_label` come from the `judge_generation_results` row where `suite_name = answer_relevance`
- `correct_refusal_score` and `correct_refusal_label` come from the `judge_generation_results` row where `suite_name = correct_refusal`

Retrieval summary mapping rules:

- `retrieval_chunk_count` = total number of `judge_retrieval_results` rows for `suite_name = retrieval_relevance`
- `retrieval_relevance_mean` = arithmetic mean of all non-null `score` values for `suite_name = retrieval_relevance`
- `retrieval_relevance_selected_mean` = arithmetic mean of all non-null `score` values where `selected_for_generation = true`
- `retrieval_relevance_topk_mean` = arithmetic mean of all non-null `score` values across the retrieved top-k rows for the request
- `retrieval_relevance_weighted_topk` = reciprocal-rank-weighted mean of all non-null `score` values, using `weight(rank) = 1 / rank`
- `retrieval_relevance_relevant_count` = count of rows where `label = relevant`
- `retrieval_relevance_selected_count` = count of retrieved rows where `selected_for_generation = true`, regardless of relevance label

Weighted-topk rule:

- reciprocal-rank weights use 1-based `retrieval_rank`
- the normalized weighted mean is:
  - `sum(score_i * weight_i) / sum(weight_i)`

Current-version note:

- because `judge_retrieval_results` currently contains only the final reranked top-k rows for the request, `retrieval_relevance_topk_mean` and `retrieval_relevance_mean` are expected to be numerically identical in the current version;
- both fields are still retained because the summary schema keeps separate names for the generic retrieval mean and the explicit top-k mean.

Retrieval quality metric mapping rules:

The following columns are populated directly from `request_captures` fields and are not derived from `judge_*_results`.
They do not affect the readiness rule defined in section 5.

When `request_captures.retrieval_stage_metrics` is non-null:

- `retrieval_evaluated_k` <- `request_captures.retrieval_stage_metrics.evaluated_k`
- `retrieval_recall_soft` <- `request_captures.retrieval_stage_metrics.recall_soft`
- `retrieval_recall_strict` <- `request_captures.retrieval_stage_metrics.recall_strict`
- `retrieval_rr_soft` <- `request_captures.retrieval_stage_metrics.rr_soft`
- `retrieval_rr_strict` <- `request_captures.retrieval_stage_metrics.rr_strict`
- `retrieval_ndcg` <- `request_captures.retrieval_stage_metrics.ndcg`
- `retrieval_first_relevant_rank_soft` <- `request_captures.retrieval_stage_metrics.first_relevant_rank_soft`
- `retrieval_first_relevant_rank_strict` <- `request_captures.retrieval_stage_metrics.first_relevant_rank_strict`
- `retrieval_num_relevant_soft` <- `request_captures.retrieval_stage_metrics.num_relevant_soft`
- `retrieval_num_relevant_strict` <- `request_captures.retrieval_stage_metrics.num_relevant_strict`

When `request_captures.retrieval_stage_metrics` is null, all 10 `retrieval_*` metric columns must be written as `NULL`.

When `request_captures.reranking_stage_metrics` is non-null:

- `reranking_evaluated_k` <- `request_captures.reranking_stage_metrics.evaluated_k`
- `reranking_recall_soft` <- `request_captures.reranking_stage_metrics.recall_soft`
- `reranking_recall_strict` <- `request_captures.reranking_stage_metrics.recall_strict`
- `reranking_rr_soft` <- `request_captures.reranking_stage_metrics.rr_soft`
- `reranking_rr_strict` <- `request_captures.reranking_stage_metrics.rr_strict`
- `reranking_ndcg` <- `request_captures.reranking_stage_metrics.ndcg`
- `reranking_first_relevant_rank_soft` <- `request_captures.reranking_stage_metrics.first_relevant_rank_soft`
- `reranking_first_relevant_rank_strict` <- `request_captures.reranking_stage_metrics.first_relevant_rank_strict`
- `reranking_num_relevant_soft` <- `request_captures.reranking_stage_metrics.num_relevant_soft`
- `reranking_num_relevant_strict` <- `request_captures.reranking_stage_metrics.num_relevant_strict`

When `request_captures.reranking_stage_metrics` is null, all 10 `reranking_*` metric columns must be written as `NULL`.

Derived context loss mapping rules:

- `retrieval_context_loss_soft` = `retrieval_recall_soft - reranking_recall_soft` when both are non-null; `NULL` otherwise
- `retrieval_context_loss_strict` = `retrieval_recall_strict - reranking_recall_strict` when both are non-null; `NULL` otherwise

`request_run_summaries` mapping rules:

The stage must also upsert one row into `request_run_summaries` for the same request.

The `request_run_summaries` row contains all fields from the `request_summaries` row plus:

- `run_id` <- the current `run_id` passed to the stage
- `judge_generation_model` <- `judge_model` from any `judge_generation_results` row for the same `(request_id, run_id)`; taken from the `answer_completeness` suite row
- `judge_generation_prompt_version` <- `judge_prompt_version` from the same `judge_generation_results` row
- `judge_retrieval_model` <- `judge_model` from the first `judge_retrieval_results` row for the same `(request_id, run_id)`; `NULL` when no retrieval judge rows exist
- `judge_retrieval_prompt_version` <- `judge_prompt_version` from the same `judge_retrieval_results` row; `NULL` when no retrieval judge rows exist

All 22 retrieval quality metric columns and both context loss columns apply identically to `request_run_summaries`.

Upsert rules for `request_summaries`:

- the stage must upsert the current row for the same `request_id`;
- the current version treats `request_summaries` as the current per-request derived table rather than a historical per-run table.

Upsert rules for `request_run_summaries`:

- the stage must upsert the row for the same `(request_id, run_id)`;
- on conflict the row must be fully overwritten with the current derived values;
- this allows safe reprocessing when a run is resumed.

## 8) State Transition Contract

This stage owns only the `build_request_summary` transition.

Allowed successful transition:

- `build_request_summary / pending -> build_request_summary / running -> build_request_summary / completed`

Allowed failure transition:

- `build_request_summary / pending -> build_request_summary / running -> build_request_summary / failed`

On failure:

- `current_stage` must remain `build_request_summary`;
- `status` must become `failed`;
- `last_error` must be updated with a non-empty description;
- `updated_at` must be refreshed.

Recovery rules:

- the stage may pick rows in `build_request_summary` with `status = pending`, `status = running`, or `status = failed`;
- before writing a summary row, the stage must re-read upstream generation and retrieval rows for the same `(request_id, run_id)`;
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
- generation-result lookup failure;
- retrieval-result lookup failure;
- upstream incompleteness detection;
- summary row mapping failure;
- database upsert failure;
- state-transition write failure;
- unexpected internal state.

Error rules:

- failures must preserve enough detail for later retry and debugging;
- raw database client exceptions must not become the only persisted error signal;
- the stage must not write a partial summary row when required upstream inputs are incomplete.

## 10) Implementation Notes

The generated implementation should use:

- `psycopg` for PostgreSQL access;
- SQL aggregation when it keeps formulas explicit and reviewable;
- Python numeric helpers for weighted-topk calculation when done outside SQL.

Operational precondition:

- the PostgreSQL eval schema, including `eval_processing_state`, `request_summaries`, `judge_generation_results`, and `judge_retrieval_results`, must already exist before the stage is invoked.

Implementation rules:

- one stage invocation may reuse one PostgreSQL connection for the whole request;
- database writes must use parameterized SQL;
- handwritten SQL interpolation is forbidden;
- aggregation formulas must match the mapping contract exactly;
- the stage should keep readiness checks separate from summary-row construction logic;
- the stage may compute aggregates using Python `Decimal` values and rely on PostgreSQL numeric coercion when writing into `numeric(6,4)` columns.
