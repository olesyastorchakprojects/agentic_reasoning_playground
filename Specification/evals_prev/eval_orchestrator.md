## 1) Purpose / Scope

`eval_orchestrator` coordinates one complete eval run.

The observability contract for the current eval engine is defined in:

- `Specification/codegen/evals/observability.md`

This module:

- creates one `run_id` for the run;
- resumes the same `run_id` when continuing a failed run;
- creates and maintains `run_manifest.json`;
- bootstraps eligible requests from `request_captures` into `eval_processing_state`;
- invokes stage workers in pipeline order;
- promotes completed requests into the next stage's pending state;
- detects run completion;
- builds the final `run_report.md`.

This module does not:

- perform generation judging itself;
- perform retrieval judging itself;
- derive request summaries itself;
- rewrite completed judge-result rows;
- own stage-local recovery logic inside worker stages.

Worker-form boundary for the current version:

- `eval_orchestrator` is the only top-level CLI entrypoint for the eval pipeline;
- worker stages must be implemented as in-process Python modules with explicit callable entrypoints;
- the orchestrator must invoke worker modules directly, not through nested worker CLIs.

For the current version, this module must comply with both:

- the trace requirements in `Specification/codegen/evals/observability.md`;
- the required console logging contract in `Specification/codegen/evals/observability.md`.

## 2) Public Interface

`eval_orchestrator` must expose one run-level entrypoint.

For the current version, the required public interface is:

- one top-level Python CLI entrypoint for the whole eval run

The canonical executable module path for the current version is:

- `Execution/evals/eval_orchestrator.py`

The canonical current-version invocation form is:

- `python -m Execution.evals.eval_orchestrator ...`

The entrypoint must:

- accept explicit run parameters;
- start one full eval run;
- terminate only after the run reaches a terminal state or a run-level failure.

For the current version, the CLI contract must expose explicit arguments for:

- `--postgres-url`
- `--run-type`
- `--chunks-path`
- `--tracing-endpoint`
- `--eval-config`
- `--resume-run-id` as an optional resume-only argument

CLI-to-parameter mapping rules:

- `--postgres-url` maps to `EvalOrchestratorParams.postgres_url`;
- `--run-type` maps to `EvalOrchestratorParams.run_type`;
- `--chunks-path` maps to `EvalOrchestratorParams.chunks_path`;
- `--tracing-endpoint` maps to `EvalOrchestratorParams.tracing_endpoint`;
- `--eval-config` maps to `EvalOrchestratorParams.eval_config_path`;
- `--resume-run-id` maps to `EvalOrchestratorParams.resume_run_id`.

The current CLI contract must not:

- hide these required inputs behind implicit environment-only defaults;
- require interactive prompts at runtime;
- infer these values from previous run artifacts.

## 3) Configuration Usage

`eval_orchestrator` must receive one explicit parameter object:

- `EvalOrchestratorParams`

For the current version, required fields are:

- `postgres_url`
- `run_type`
- `chunks_path`
- `tracing_endpoint`
- `eval_config_path`
- `resume_run_id`

Field-source rules:

- `postgres_url` points to the eval storage PostgreSQL instance;
- `run_type` identifies the current run category;
- `chunks_path` points to the canonical `chunks.jsonl` artifact used by generation and retrieval worker stages;
- `tracing_endpoint` points to the OTLP trace collector ingress used by the eval engine;
- `eval_config_path` points to the canonical `eval_engine.toml` file from which judge runtime settings are loaded;
- `resume_run_id`, when present, identifies an existing failed run that must be resumed with the same frozen run scope.

The orchestrator must not:

- read raw environment variables directly as its primary configuration interface;
- infer `run_id` from database state;
- infer run scope from prior run manifests.

Judge runtime loading rules:

- the orchestrator must load judge runtime configuration from `EvalOrchestratorParams.eval_config_path`;
- the canonical judge config table is `[judge]` in `Execution/evals/eval_engine.toml`;
- `judge.provider` must be one of:
  - `ollama`
  - `together`
- `judge.model_name` must be non-empty;
- `judge.tokenizer_source` must be non-empty;
- `judge.input_cost_per_million_tokens` must be a number `>= 0`;
- `judge.output_cost_per_million_tokens` must be a number `>= 0`;
- `judge.provider = ollama` requires `[judge.ollama]` and env var `OLLAMA_URL`;
- `judge.provider = together` requires `[judge.together]` and env vars `OPENAI_COMPATIBLE_URL` and `TOGETHER_API_KEY`;
- the orchestrator must resolve one effective judge runtime:
  - `judge_provider`
  - `judge_base_url`
  - `judge_model`
- these effective values must be recorded in `run_manifest.json` and used for both judge worker stages.

Parameter-passing rules:

- for Python implementation, explicit parameters are preferred over a crate-style nested settings model;
- the top-level orchestrator boundary is the required CLI contract defined above;
- inside the implementation, validated CLI inputs may be represented as a small typed parameter object or another equally explicit internal boundary.

Current-version callable boundary:

- the canonical Python function boundary is `run_eval_orchestrator(params: EvalOrchestratorParams) -> str`

## 4) Run Bootstrap Contract

At run bootstrap for a new run, the orchestrator must:

1. generate one new `run_id`;
2. record `started_at`;
3. create the initial `run_manifest.json`;
4. discover which `request_captures` rows belong to the run scope;
5. create missing `eval_processing_state` rows for those requests.

Bootstrap row-creation rules:

- one processing-state row must exist per `request_id`;
- new rows must be created as:
  - `current_stage = judge_generation`
  - `status = pending`
  - `attempt_count = 0`
- `request_received_at` must be copied from `request_captures.received_at`.

Run-scope rule:

- for the current version, run scope consists of:
  - all `request_captures` rows whose `request_id` is not yet present in `eval_processing_state`;
- requests already present in `eval_processing_state` must not be absorbed into a new run;
- `request_count` in `run_manifest.json` must equal the number of requests included in this frozen run scope.
- `run_scope_request_ids` must be written into `run_manifest.json` and treated as immutable for the life of the run.

Resume rule:

- if `resume_run_id` is provided, the orchestrator must resume the same `run_id`;
- it must load the prior `run_manifest.json`;
- it must reuse the exact `run_scope_request_ids` stored in that manifest;
- it must not add newly captured requests to the resumed run scope;
- it must not generate a new `run_id` during resume;
- it must resume requests from their current stage in `eval_processing_state`.

Operational precondition:

- the PostgreSQL eval schema, including `request_captures`, `eval_processing_state`, `judge_generation_results`, `judge_retrieval_results`, and `request_summaries`, must already exist before the orchestrator is invoked.

## 5) Run Manifest Contract

The semantic contract for the manifest is defined in:

- `Specification/contracts/evals/run_manifest.md`

The machine-readable schema is:

- `Execution/schemas/evals/run_manifest.schema.json`

Current-version artifact-folder rule:

- run artifacts must be written under `Evidence/evals/runs/<run_started_at>_<run_id>/`;
- `<run_started_at>` must be derived from the run `started_at` timestamp in a filesystem-safe UTC form;
- `run_id` remains the canonical run identifier and must not be inferred from the folder name.

The orchestrator owns:

- creating the initial `run_manifest.json`;
- validating its structure before writing;
- updating run-level status fields during the run;
- writing terminal run status on completion or failure.

For the current version:

- `run_manifest.json` must be created before worker stages begin;
- `status = running` must be written at bootstrap;
- `run_scope_request_ids` must be written once the frozen run scope is known;
- `status = completed` or `status = failed` must be written at terminal run end;
- `completed_at` must be written for terminal run states.

## 6) Stage Invocation Contract

The current pipeline stage order is:

1. `judge_generation`
2. `judge_retrieval`
3. `build_request_summary`

The orchestrator must invoke workers in this order.

For the current version, the orchestrator must not skip a stage in the middle of a normal run.

Worker invocation rules:

- `judge_generation` receives the explicit params it needs, including `run_id`;
- `judge_retrieval` receives the explicit params it needs, including `run_id`;
- `build_request_summary` receives the explicit params it needs, including `run_id`;
- worker-local stage logic remains owned by the worker, not by the orchestrator;
- worker invocation must be in-process through explicit Python-callable module boundaries.

Current-version worker-parameter mapping:

- `judge_generation` must receive:
  - `postgres_url`
  - `run_id`
  - `judge_settings`
  - `chunks_path`
- `judge_retrieval` must receive:
  - `postgres_url`
  - `run_id`
  - `judge_settings`
  - `chunks_path`
- `build_request_summary` must receive:
  - `postgres_url`
  - `run_id`

The orchestrator must construct these worker parameters explicitly from:

- the validated top-level CLI inputs;
- the generated `run_id`.

The orchestrator may invoke workers repeatedly until no work remains for the current stage.

Current-version stage-drain rule:

- no work remains for the current stage iff there are no rows in `eval_processing_state` for that `current_stage` with:
  - `status = pending`
  - or `status = running`
  - or `status = failed`

Current-version stage-drain policy:

- if a stage still has remaining rows after a worker pass and the worker does not make forward progress, the orchestrator must fail the run rather than loop forever;
- the current version therefore uses fail-fast behavior for undrainable stage state.

## 7) Stage Promotion Contract

The orchestrator owns promotion between stages.

For the current version, required promotion rules are:

- `judge_generation / completed -> judge_retrieval / pending`
- `judge_retrieval / completed -> build_request_summary / pending`

Promotion rules:

- promotion must happen only after the current stage is `completed`;
- promotion must preserve the same `request_id`;
- promotion must not modify already terminal `build_request_summary / completed` rows;
- promotion must not overwrite worker-owned downstream result rows.

Current-version promotion mode:

- stage workers may complete many requests over time;
- after a pass of a given worker stage, the orchestrator must promote all rows where:
  - `current_stage = <that worker stage>`
  - in `status = completed`
- the orchestrator must set those rows to the next stage with:
  - `current_stage = <next_stage>`
  - `status = pending`
- promotion is therefore batch-oriented at the orchestrator boundary, even though each worker processes requests one at a time.

The orchestrator must not:

- mark a request `completed` for a worker stage before the worker has done so;
- synthesize stage success without downstream results;
- change worker-owned `running` or `failed` states in order to bypass worker recovery logic.

## 8) Completion Detection

The run is complete iff:

- every request in run scope has reached:
  - `current_stage = build_request_summary`
  - `status = completed`

Completion rules:

- the orchestrator must detect this condition from `eval_processing_state`;
- run completion must trigger terminal manifest update;
- run completion must trigger `run_report.md` generation.

## 9) Run Report Contract

The semantic contract for the run report is defined in:

- `Specification/contracts/evals/run_report.md`

The orchestrator owns:

- invoking report construction after run completion;
- ensuring the report is built for the same `run_id`;
- writing the report to the canonical run artifact path.

Report-source rules:

- report data must be run-scoped by `run_id`;
- the report must use `run_manifest.json` as run metadata input for run identity and status fields only (`run_id`, `run_type`, `status`, `started_at`, `completed_at`, `request_count`, `generation_suite_versions`, `retrieval_suite_versions`);
- pipeline configuration data (retriever, reranker, generation) must be loaded from `request_captures` at report-build time, not from the manifest;
- judge configuration (`judge_provider`, `judge_model`, `judge_base_url`) must be sourced from the resolved judge settings loaded from `EvalOrchestratorParams.eval_config_path`, not from ad hoc CLI overrides;
- future eval token-usage and cost aggregation by `run_id` must read from `judge_llm_calls`, not from `run_manifest.json`.
- the report must not rely on `request_summaries` alone as its only source.

Pipeline config loading rules:

- `_load_run_pipeline_configs(connection, run_scope_request_ids)` must query `request_captures` for one representative row from the run scope;
- it must return `retriever_kind`, `retriever_config`, `reranker_kind`, `reranker_config`, `generation_config`;
- on empty result it must raise `RunReportError`.

`_build_run_report` signature must accept resolved `judge_settings` in addition to `connection`, `run_id`, and `manifest`.

Run Metadata section builders:

- `_build_retriever_section(retriever_kind, retriever_config)` — returns `list[str]`; renders `### Retriever` H3 and field bullets
- `_build_reranker_section(reranker_kind, reranker_config)` — returns `list[str]`; renders `### Reranker` H3 and kind-specific fields
- `_build_generation_section(generation_config)` — returns `list[str]`; renders `### Generation` H3 and 4 config fields
- `_build_judge_section(judge_settings)` — returns `list[str]`; renders `### Judge` H3 with `provider`, `model`, and `endpoint`

Retrieval Quality section construction rules:

- the Retrieval Quality section must be built by querying `request_run_summaries` filtered by `run_id`;
- the query must compute `avg()` for all 12 metric columns: `retrieval_recall_soft`, `retrieval_recall_strict`, `retrieval_rr_soft`, `retrieval_rr_strict`, `retrieval_ndcg`, `reranking_recall_soft`, `reranking_recall_strict`, `reranking_rr_soft`, `reranking_rr_strict`, `reranking_ndcg`, `retrieval_context_loss_soft`, `retrieval_context_loss_strict`, and `avg()` for the 4 num-relevant columns: `retrieval_num_relevant_soft`, `retrieval_num_relevant_strict`, `reranking_num_relevant_soft`, `reranking_num_relevant_strict`;
- the query must also select the representative `retrieval_evaluated_k` and `reranking_evaluated_k` values using `min()` (both are configuration-fixed and uniform within a run);
- if the query returns zero rows, or if the resulting averages for all metric columns are NULL, the Retrieval Quality section must be omitted from the report;
- the table row labels must incorporate the integer k values: `top{retrieval_evaluated_k}` and `top{reranking_evaluated_k}`;
- the scalar bullet labels must similarly incorporate the k values as specified in `Specification/contracts/evals/run_report.md`;
- `request_run_summaries` is the required source for this section because it is run-scoped; `request_summaries` must not be used as the source for the Retrieval Quality section.

Conditional Retrieval→Generation Aggregates section construction rules:

Source table: `request_run_summaries` filtered by `run_id`.

DB column mapping (spec name → db column name):
  num_relevant_in_top{retrieval_k}_soft    → retrieval_num_relevant_soft
  num_relevant_in_top{retrieval_k}_strict  → retrieval_num_relevant_strict
  num_relevant_in_top{reranking_k}_soft    → reranking_num_relevant_soft
  num_relevant_in_top{reranking_k}_strict  → reranking_num_relevant_strict
  first_relevant_rank_top{retrieval_k}_soft   → retrieval_first_relevant_rank_soft
  first_relevant_rank_top{retrieval_k}_strict → retrieval_first_relevant_rank_strict
  first_relevant_rank_top{reranking_k}_soft   → reranking_first_relevant_rank_soft
  first_relevant_rank_top{reranking_k}_strict → reranking_first_relevant_rank_strict
  groundedness      → groundedness_score
  answer_completeness → answer_completeness_score
  answer_relevance  → answer_relevance_score

Required query: one SQL SELECT with 20 conditional aggregate expressions
using PostgreSQL AVG(...) FILTER (WHERE ...) syntax, filtered by run_id.

The base WHERE clause is: WHERE run_id = %s

The 20 expressions are: 5 metrics × 4 conditions (retrieval soft/strict,
reranking soft/strict). Full list with required SQL aliases:

For (num_col=retrieval_num_relevant_soft, rank_col=retrieval_first_relevant_rank_soft):
  AVG(groundedness_score)      FILTER (WHERE retrieval_num_relevant_soft > 0)
      AS groundedness_retrieval_soft
  AVG(answer_completeness_score) FILTER (WHERE retrieval_num_relevant_soft > 0)
      AS answer_completeness_retrieval_soft
  AVG(answer_relevance_score)  FILTER (WHERE retrieval_num_relevant_soft > 0)
      AS answer_relevance_retrieval_soft
  AVG(CASE WHEN groundedness_score < 1.0 THEN 1.0 ELSE 0.0 END)
      FILTER (WHERE retrieval_first_relevant_rank_soft IS DISTINCT FROM 1)
      AS hallucination_retrieval_soft
  AVG(CASE WHEN groundedness_score = 1.0 AND answer_completeness_score = 1.0
           THEN 1.0 ELSE 0.0 END)
      FILTER (WHERE retrieval_num_relevant_soft > 0)
      AS success_retrieval_soft

For (num_col=retrieval_num_relevant_strict, rank_col=retrieval_first_relevant_rank_strict):
  AVG(groundedness_score)      FILTER (WHERE retrieval_num_relevant_strict > 0)
      AS groundedness_retrieval_strict
  AVG(answer_completeness_score) FILTER (WHERE retrieval_num_relevant_strict > 0)
      AS answer_completeness_retrieval_strict
  AVG(answer_relevance_score)  FILTER (WHERE retrieval_num_relevant_strict > 0)
      AS answer_relevance_retrieval_strict
  AVG(CASE WHEN groundedness_score < 1.0 THEN 1.0 ELSE 0.0 END)
      FILTER (WHERE retrieval_first_relevant_rank_strict IS DISTINCT FROM 1)
      AS hallucination_retrieval_strict
  AVG(CASE WHEN groundedness_score = 1.0 AND answer_completeness_score = 1.0
           THEN 1.0 ELSE 0.0 END)
      FILTER (WHERE retrieval_num_relevant_strict > 0)
      AS success_retrieval_strict

For (num_col=reranking_num_relevant_soft, rank_col=reranking_first_relevant_rank_soft):
  AVG(groundedness_score)      FILTER (WHERE reranking_num_relevant_soft > 0)
      AS groundedness_reranking_soft
  AVG(answer_completeness_score) FILTER (WHERE reranking_num_relevant_soft > 0)
      AS answer_completeness_reranking_soft
  AVG(answer_relevance_score)  FILTER (WHERE reranking_num_relevant_soft > 0)
      AS answer_relevance_reranking_soft
  AVG(CASE WHEN groundedness_score < 1.0 THEN 1.0 ELSE 0.0 END)
      FILTER (WHERE reranking_first_relevant_rank_soft IS DISTINCT FROM 1)
      AS hallucination_reranking_soft
  AVG(CASE WHEN groundedness_score = 1.0 AND answer_completeness_score = 1.0
           THEN 1.0 ELSE 0.0 END)
      FILTER (WHERE reranking_num_relevant_soft > 0)
      AS success_reranking_soft

For (num_col=reranking_num_relevant_strict, rank_col=reranking_first_relevant_rank_strict):
  AVG(groundedness_score)      FILTER (WHERE reranking_num_relevant_strict > 0)
      AS groundedness_reranking_strict
  AVG(answer_completeness_score) FILTER (WHERE reranking_num_relevant_strict > 0)
      AS answer_completeness_reranking_strict
  AVG(answer_relevance_score)  FILTER (WHERE reranking_num_relevant_strict > 0)
      AS answer_relevance_reranking_strict
  AVG(CASE WHEN groundedness_score < 1.0 THEN 1.0 ELSE 0.0 END)
      FILTER (WHERE reranking_first_relevant_rank_strict IS DISTINCT FROM 1)
      AS hallucination_reranking_strict
  AVG(CASE WHEN groundedness_score = 1.0 AND answer_completeness_score = 1.0
           THEN 1.0 ELSE 0.0 END)
      FILTER (WHERE reranking_num_relevant_strict > 0)
      AS success_reranking_strict

Also select k values (for column header substitution):
  MIN(retrieval_evaluated_k)  AS retrieval_evaluated_k
  MIN(reranking_evaluated_k)  AS reranking_evaluated_k

Zero-denominator rule:
  PostgreSQL AVG(...) FILTER (WHERE ...) naturally returns NULL when the filter
  matches zero rows. Python must render NULL as "n/a" — never as 0.0000.

Omission rule:
  The section must be omitted if all 20 aggregate expressions resolve to NULL.

Internal builder function name: _build_conditional_retrieval_generation_section
  Accepts: one dict (the single aggregated query row) or None
  Returns: list[str] (Markdown lines) or empty list when section is omitted
  Must be a separate callable from _build_retrieval_quality_section.

## 10) Failure Model

The orchestrator must expose a clear run-level failure boundary.

For Python implementation, failure categories may be represented via:

- exception classes;
- structured error records;
- another explicit run-level error boundary.

Required failure categories:

- run bootstrap failure;
- manifest creation or validation failure;
- processing-state bootstrap failure;
- worker invocation failure;
- stage-promotion failure;
- terminal completion detection failure;
- report generation failure;
- unexpected internal state.

Failure rules:

- a run-level failure must update `run_manifest.json` to `status = failed`;
- `last_error` in `run_manifest.json` must be non-empty on terminal run failure;
- worker-local failures must not be silently swallowed at run level;
- the orchestrator must preserve enough detail for later diagnosis of the failed run.

## 11) Implementation Notes

The generated implementation should use:

- `psycopg` for PostgreSQL access;
- Python `json` support for manifest handling;
- Python filesystem APIs for writing `run_manifest.json` and `run_report.md`.
- `jsonschema` for validating `run_manifest.json` against `Execution/schemas/evals/run_manifest.schema.json`.

Tracing configuration rule:

- the orchestrator must receive `tracing_endpoint` as an explicit validated input;
- the current version must not rely on ambient environment variables or fallback defaults for OTLP trace export endpoint selection.

Implementation rules:

- database writes must use parameterized SQL;
- handwritten SQL interpolation is forbidden;
- `run_id` must be generated once and then passed explicitly to all worker invocations;
- manifest updates and stage promotion logic should be kept separate from worker invocation logic;
- the orchestrator should treat worker stages as bounded modules with explicit interfaces, not as inlined business logic blocks;
- one run may reuse one PostgreSQL connection for bootstrap, promotion, completion checks, and report construction;
- the initial `run_manifest.json` must be written before worker execution begins;
- the terminal `run_manifest.json` must be written before `run_report.md` generation begins;
- `run_report.md` generation must read the terminal manifest state for final run metadata.
