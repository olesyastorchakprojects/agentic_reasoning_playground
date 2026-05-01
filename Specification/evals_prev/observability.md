## 1) Purpose / Scope

This document defines the current observability contract for the eval engine.

It defines:

- the required observability signal type;
- the required backend routing model;
- the required console logging contract;
- the required span hierarchy for the current eval pipeline;
- span attributes and safety rules;
- placement rules for eval observability specification.

This document does not define:

- metrics;
- Grafana dashboards;
- OpenInference/Phoenix annotation payloads.

## 2) Signal Model

For the current version, the eval engine emits exactly one required observability signal type:

- traces

The current version also requires:

- console logs for immediate operator-facing progress visibility

The current version does not require:

- metrics

## 3) Backend Routing

The eval engine must export traces through OTLP.

Signal routing is fixed:

- eval workers and the eval orchestrator export traces through OTLP;
- the OTEL Collector is the single ingress point for eval traces;
- the OTEL Collector routes eval traces to Tempo.

For the current version:

- Tempo is the only required trace backend for eval observability;
- direct export from eval modules to Tempo is forbidden;
- direct export from eval modules to multiple tracing backends is forbidden.

## 4) Observability Objectives

Eval-engine traces exist to support exactly these goals:

- reconstruct one eval run end-to-end;
- show where long-running judge work is currently blocked or progressing;
- provide request-level drill-down across orchestrator and worker stages;
- support debugging of stage failures, retries, and incomplete runs.

Eval-engine console logs exist to support exactly these goals:

- keep the CLI visibly alive during long-running eval work;
- show which run, request, suite, or chunk is currently executing;
- show whether the engine is selecting work, waiting on the judge model, writing results, or failing.

## 5) Required Span Hierarchy

For the current version, the canonical span hierarchy is:

- `eval.run`
  - one root span per eval run, owned by `eval_orchestrator`
- `eval.judge_generation.request`
  - one child span per processed request in `judge_generation`
- `eval.judge_generation.suite`
  - one child span per executed generation suite for that request
- `eval.judge_retrieval.request`
  - one child span per processed request in `judge_retrieval`
- `eval.judge_retrieval.chunk`
  - one child span per executed retrieval chunk judgment for that request
- `eval.build_request_summary.request`
  - one child span per processed request in `build_request_summary`

The current version does not require a dedicated span per summary field or per SQL statement.

## 6) Required Span Attributes

### Required On `eval.run`

- `run_id`
- `run_type`
- `request_count`
- `status`

### Required On Worker Request Spans

- `run_id`
- `request_id`
- `stage`
- `status`
- `attempt_count`

### Required On `eval.judge_generation.suite`

- `run_id`
- `request_id`
- `stage`
- `suite_name`
- `judge_model`
- `status`

### Required On `eval.judge_retrieval.chunk`

- `run_id`
- `request_id`
- `stage`
- `suite_name`
- `chunk_id`
- `retrieval_rank`
- `selected_for_generation`
- `judge_model`
- `status`

### Optional Attributes

The following attributes are optional when useful:

- `trace_id`
- `error_type`
- `error_message`
- `rows_written`
- `missing_suite_count`
- `missing_chunk_count`

## 7) Long-Running Work Visibility

Because one judge call may take tens of seconds, span timing must make long-running work visible.

Required rules:

- every request-level stage execution must have its own request span;
- every generation suite call must have its own suite span;
- every retrieval chunk judgment must have its own chunk span;
- span start and end must bracket the real model call duration, not only local parsing or DB write time.

This requirement exists so that Tempo can show whether a worker is:

- selecting work;
- waiting on the judge model;
- writing results;
- or failing during state transition.

## 8) Failure Visibility

On failure:

- the active span for the failing run/request/suite/chunk must be marked failed;
- the span must include non-empty failure detail through attributes or recorded exception metadata;
- failures must remain visible in traces even if the corresponding stage later retries.

## 9) Safety Constraints

The following values must not be written into eval traces:

- secrets;
- API keys;
- raw database passwords;
- raw prompt template text;
- raw retrieved chunk text;
- raw full judge response payloads;
- raw final answer text unless separately approved later.

Identifiers and compact operational metadata are allowed.

## 10) Module Integration Rules

This observability contract applies to:

- `Execution/evals/eval_orchestrator.py`
- `Execution/evals/judge_generation.py`
- `Execution/evals/judge_retrieval.py`
- `Execution/evals/build_request_summary.py`

For the current version:

- `eval_orchestrator` owns the root `eval.run` span;
- worker modules own the request-level and nested work spans for their own stage;
- worker stages must not create spans for another stage.

## 11) Required Console Logging

For the current version, eval modules must emit operator-facing console logs.

Required log-channel rules:

- console logging is required even though traces remain the canonical observability signal;
- the same major execution steps must be visible in both traces and console logs;
- console logs exist to make long-running CLI execution observable without opening Tempo.

Required log events:

- eval run start;
- eval run completion;
- eval run failure;
- stage drain start;
- stage drain completion;
- stage promotion summary;
- request selection;
- request start;
- request completion;
- request failure;
- generation-suite call start;
- generation-suite call completion;
- retrieval-chunk judgment start;
- retrieval-chunk judgment completion;
- summary upsert completion.

Required log fields when available:

- `run_id`
- `request_id`
- `stage`
- `status`
- `suite_name`
- `chunk_id`
- `retrieval_rank`
- `attempt_count`

Required logging behavior:

- console logs must be written before and after long-running judge calls;
- long-running judge work must remain visible even when no database state changes occur for tens of seconds;
- failures must log non-empty error detail;
- logs must not leak values forbidden by the trace safety constraints above.

## 12) Placement Rules

Eval observability specification belongs in:

- `Specification/codegen/evals/observability.md`
