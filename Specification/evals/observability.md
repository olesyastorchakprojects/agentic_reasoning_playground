## 1) Purpose

This document defines the current observability contract for the diagnostics
eval engine.

The eval engine emits OTLP traces and lightweight console-visible progress so
that long-running batch work can be reconstructed and debugged.

## 2) Required Signal Types

The current required signal types are:

- traces through OTLP;
- console-visible progress output.

The current implementation does not expose a separate metrics pipeline in eval
config.

## 3) Required Span Hierarchy

The current span hierarchy is:

- `eval.run`
- `eval.judge_request_suites.subject`
- `eval.judge_request_suites.suite`
- `eval.build_eval_summary`

The spans are emitted with OpenInference-compatible attributes.

## 4) Current Span Attribute Names

### 4.1) `eval.run`

Current attributes include:

- `openinference.span.kind = "CHAIN"`
- `eval.run_id`
- `eval.run_type`
- `eval.judge_model`
- `eval.status`
- `eval.runtime_run_count`
- `eval.iterations_evaluated_count`
- `error.type`
- `error.message`

### 4.2) `eval.judge_request_suites.subject`

Current attributes include:

- `openinference.span.kind = "CHAIN"`
- `eval.run_id`
- `eval.runtime_run_id`
- `eval.iteration_id`
- `eval.subject_status`
- `error.type`
- `error.message`

### 4.3) `eval.judge_request_suites.suite`

Current attributes include:

- `openinference.span.kind = "LLM"`
- `eval.run_id`
- `eval.runtime_run_id`
- `eval.iteration_id`
- `eval.suite_name`
- `eval.suite_category`
- `eval.suite_scope`
- `llm.model_name`
- `llm.token_count.prompt`
- `llm.token_count.completion`
- `llm.token_count.total`
- `eval.total_cost_usd`
- `eval.score`
- `input.value`
- `input.mime_type`
- `output.value`
- `output.mime_type`
- `error.type`
- `error.message`

### 4.4) `eval.build_eval_summary`

Current attributes include:

- `openinference.span.kind = "CHAIN"`
- `eval.run_id`
- `eval.runtime_run_count`
- `eval.iterations_evaluated_count`
- `error.type`
- `error.message`

## 5) Runtime Initialization

Current observability initialization behavior:

- tracing is enabled only when `observability.tracing_enabled = true`;
- service name comes from config;
- OTLP endpoint comes from resolved observability settings;
- the tracer provider uses `AlwaysOn` sampling;
- batch export scheduled delay is currently `250ms`;
- the runtime force-flushes on process completion and shuts down on drop.

## 6) Sensitive Data Note

The current implementation records prompt text and model output content into
suite-span attributes through `input.value` and `output.value`.

This is the current implementation truth and therefore must be documented here,
even though a stricter future policy may choose to reduce payload visibility in
trace attributes.
