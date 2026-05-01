## 1) Purpose

This document defines the observability contract for the new diagnostics eval
engine.

The eval engine must emit traces and operator-facing logs that make long-running
batch judge work understandable without dumping full payloads into telemetry.

## 2) Required Signal Types

For the current MVP the eval engine must emit:

- traces through OTLP;
- operator-facing console logs.

Metrics may be added later, but traces and logs are required now.

## 3) Trace Goals

Eval traces exist to support exactly these goals:

- reconstruct one eval run end-to-end;
- show which runtime run / iteration / suite is currently in progress;
- show where the engine is blocked or failing;
- support debugging of resume behavior and partial completion.

## 4) Required Span Hierarchy

The required span hierarchy is:

- `eval.run`
  - one root span per eval run
- `eval.judge_request_suites.subject`
  - one child span per evaluated subject
- `eval.judge_request_suites.suite`
  - one child span per executed suite call
- `eval.build_eval_summary`
  - one child span for summary/report construction

## 5) Required Span Attributes

### 5.1) On `eval.run`

Required attributes:

- `eval_run_id`
- `run_type`
- `status`
- `runtime_run_count`
- `judge_model`

### 5.2) On `eval.judge_request_suites.subject`

Required attributes:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`
- `subject_status`

### 5.3) On `eval.judge_request_suites.suite`

Required attributes:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`
- `suite_name`
- `suite_category`
- `suite_scope`
- `judge_model`

If available after call completion, the suite span should also record:

- `prompt_tokens`
- `completion_tokens`
- `total_tokens`
- `total_cost_usd`

### 5.4) On `eval.build_eval_summary`

Required attributes:

- `eval_run_id`
- `runtime_run_count`
- `iterations_evaluated_count`

## 6) Console Logging Goals

Console logs exist to:

- keep the CLI visibly alive during long-running eval work;
- show which eval run, runtime run, iteration, or suite is currently active;
- show whether the engine is selecting work, waiting on the judge model,
  writing results, building summaries, or failing.

## 7) Required Console Events

At minimum the engine must log:

- eval run start;
- eval run completion;
- eval run failure;
- subject evaluation start;
- subject evaluation completion;
- suite execution start;
- suite execution completion;
- summary build start;
- summary build completion.

## 8) Sensitive Data Rule

The following must not be written into trace attributes:

- full raw prompt payloads;
- full snapshots;
- full raw judge responses;
- full `RunState` dumps.

Those artifacts may be preserved in storage where required, but not as broad
trace attribute payloads.
