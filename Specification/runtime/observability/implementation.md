# 1) Purpose / Scope

This document defines the required Rust implementation contract for runtime observability initialization and span wiring.

It defines:
- required crates and versions;
- required runtime components;
- required typed settings type;
- required initialization order;
- root span implementation pattern;
- failure propagation pattern;
- leaf-module event pattern;
- trace exporter strategy;
- metric exporter strategy;
- provider lifecycle and graceful shutdown requirements;
- reference implementation example.

# 2) Required Library Stack

The required Rust observability stack is:
- `tracing`;
- `tracing-subscriber`;
- `opentelemetry`;
- `opentelemetry_sdk`;
- `opentelemetry-otlp`;
- `tracing-opentelemetry`;
- `tokio`.

The required OpenTelemetry crate line is:
- `opentelemetry = 0.31`;
- `opentelemetry_sdk = 0.31`;
- `opentelemetry-otlp = 0.31.1`;
- `tracing-opentelemetry = 0.32.1`.

The required transport stack is:
- OTLP gRPC via `tonic` for traces;
- OTLP gRPC via `tonic` for metrics.

# 3) Required Typed Settings Type

Observability initialization works with exactly one typed settings type:
- `ObservabilitySettings`

`ObservabilitySettings` is a field of the crate-wide `Settings` type:
- `Settings.observability: ObservabilitySettings`

`ObservabilitySettings` contains exactly these fields:
- `tracing_enabled: bool`
- `metrics_enabled: bool`
- `tracing_endpoint: String`
- `metrics_endpoint: String`
- `trace_batch_scheduled_delay_ms: u64`
- `metrics_export_interval_ms: u64`

`ObservabilitySettings.tracing_endpoint` is the OTLP collector gRPC ingress URL used by the trace exporter.

`ObservabilitySettings.metrics_endpoint` is the OTLP collector gRPC ingress URL used by the metric exporter.

The startup layer resolves these fields from exactly these environment variables:
- `TRACING_ENDPOINT` -> `ObservabilitySettings.tracing_endpoint`
- `METRICS_ENDPOINT` -> `ObservabilitySettings.metrics_endpoint`

Business modules do not read these environment variables directly.

# 4) Required Runtime Components

The implementation must construct the following runtime components:

- `SpanExporter`
  - OTLP gRPC trace exporter
- `MetricExporter`
  - OTLP gRPC metric exporter
- `SdkTracerProvider`
  - owns the trace export pipeline
- `SdkMeterProvider`
  - owns the metric export pipeline
- tracing subscriber
  - `tracing_subscriber::registry()` composed with a `tracing-opentelemetry` layer
- RAII guard objects
  - long-lived ownership objects that keep tracer and meter providers alive until graceful shutdown and run deterministic shutdown logic on drop

The implementation must keep both providers alive for the full process lifetime.

# 4.1) Required Internal Observability API

The generated Rust implementation defines exactly one internal observability runtime type:

- `ObservabilityRuntime`

`ObservabilityRuntime` is internal to the crate.
It is not part of the public crate API.

The implementation defines internal observability methods and helpers for:

- initialization from `&ObservabilitySettings`
- root run span creation
- iteration span creation
- policy decision span creation
- step orchestration span creation
- repository persistence span creation
- executor dispatch span creation
- leaf request-pipeline span creation
- leaf dependency span creation
- leaf diagnostic event emission
- stable error classification
- graceful shutdown ownership

The implementation keeps this internal observability API inside `src/observability/mod.rs`.

Business modules do not construct exporter stacks, provider objects, or global subscribers directly.
Business modules create spans only through the internal observability API or their owning module-local wrappers.

Tracing primitive rule:
- orchestration spans are created with `tracing::span!`;
- `tracing_opentelemetry` is the export bridge layer;
- the internal observability API does not create orchestration spans through OpenTelemetry SDK APIs directly.
- where UI readability benefits from it, the internal observability API may set `otel.name` on `tracing` spans before export.

Leaf-module tracing rule:
- leaf request-pipeline modules also create their spans with `tracing::span!` or `tracing::info_span!(...)`;
- leaf modules may emit compact diagnostic events inside the active leaf span;
- orchestrator lifecycle events remain forbidden even when leaf-module events are allowed.

# 5) Initialization Pattern

Observability initialization occurs exactly once at process startup.

Initialization order is fixed:

1. Read `Settings.observability` from the already-constructed `Settings` object.
2. Build `SdkTracerProvider`.
3. Build tracing subscriber with `tracing-opentelemetry` layer.
4. Install the process-global tracing subscriber.
5. Register the process-global tracer provider.
6. Build `SdkMeterProvider`.
7. Register the process-global meter provider.
8. Return an initialized runtime object and then start later runtime work.

Business modules do not initialize observability.

Working example reference:

- local repository observability reference artifacts under `Specification/runtime/observability/references/` are the working configuration templates for this repository;
- it is the operational reference for:
  - OTLP exporter construction
  - tracing subscriber installation
  - tracer and meter provider setup
- future generation and debugging should compare runtime initialization against those repository-owned references before inventing alternative stack wiring.

Initialization failure rules:
- when tracing initialization is enabled, failure to install the process-global tracing subscriber is a startup error;
- when tracing initialization is enabled, failure to register the process-global tracer provider is a startup error;
- when metrics initialization is enabled, failure to register the process-global meter provider is a startup error;
- those failures must not be ignored or downgraded to best-effort behavior.

Disabled-mode initialization rules:
- when `tracing_enabled = false`, tracing exporter construction does not run;
- when `metrics_enabled = false`, metric exporter construction does not run;
- when both `tracing_enabled = false` and `metrics_enabled = false`, initialization returns a concrete `ObservabilityRuntime`;
- disabled-mode initialization is successful;
- disabled-mode runtime ownership remains valid;
- disabled-mode initialization is no-op safe.

# 6) Exporter And Transport Pattern

Rules:
- trace exporter uses OTLP gRPC;
- metric exporter uses OTLP gRPC;
- long-running runtime mode uses batch trace export;
- meter export uses `PeriodicReader`.

Rationale:
- batch trace export is required for long-running runtime processes because it amortizes export overhead and avoids synchronous export on every completed span;
- simple trace export is forbidden for the runtime service because it couples request completion to export latency;
- OTLP gRPC is required because the validated stack uses `tonic` exporters successfully for both traces and metrics;
- OTLP HTTP is forbidden in the required runtime contract.

# 7) Batch Trace Export Requirements

The trace provider uses a batch span processor.

Batch processor rules:
- the batch span processor is constructed explicitly and attached during `SdkTracerProvider` construction;
- the batch span processor scheduled delay is set from `Settings.observability.trace_batch_scheduled_delay_ms`;
- `Settings.observability.trace_batch_scheduled_delay_ms` is converted to `Duration::from_millis(...)` and passed into batch processor construction explicitly;
- application code must not implement custom trace flushing during normal request execution;
- graceful shutdown is achieved by deterministic tracer provider shutdown owned by the tracing RAII guard.

Rationale:
- default batch timing can delay trace visibility unnecessarily;
- explicit use of `Settings.observability.trace_batch_scheduled_delay_ms` removes ambiguity from runtime behavior;

# 8) Metrics Export Requirements

The meter provider uses `PeriodicReader`.

`PeriodicReader` rules:
- the metric exporter is attached through `PeriodicReader`;
- the export interval is set from `Settings.observability.metrics_export_interval_ms`;
- `Settings.observability.metrics_export_interval_ms` is converted to `Duration::from_millis(...)` and passed into `PeriodicReader::builder(...).with_interval(...)` explicitly;
- the meter provider is global;
- application code must not export metrics on request completion paths.

Duration histogram construction rule:
- `rag_request_duration_ms`
- `rag_stage_duration_ms`
- `rag_dependency_duration_ms`

These histograms must be built with explicit millisecond bucket boundaries rather than relying on default histogram boundaries.

The boundary set must cover at least:
- microsecond-scale and low-millisecond spans for validation and payload mapping;
- sub-second and low-second spans for retrieval and embedding;
- multi-second and multi-minute spans for generation, chat, and full-request latency.

Rationale:
- metrics are periodic aggregate signals;
- metrics export must not be tied to request completion;
- `PeriodicReader` provides the validated push model for OTLP metrics.

# 9) Provider Lifecycle And Graceful Shutdown

The implementation must own explicit RAII shutdown objects for tracing and metrics.

Those shutdown objects are RAII guards that own the providers for the full application lifetime.

Lifecycle and shutdown rules:
- providers are created before request handling starts;
- providers remain alive for the full process lifetime;
- the application keeps provider guards alive until graceful shutdown;
- providers are shut down during the application shutdown sequence;
- graceful shutdown triggers tracer provider shutdown exactly once;
- graceful shutdown triggers meter provider shutdown exactly once;
- the application stop sequence completes before provider shutdown begins.

Rationale:
- batch trace processors need process-lifetime ownership;
- periodic metric readers need process-lifetime ownership;
- RAII guards make provider shutdown deterministic.

The implementation must not rely on implicit drop order inside business modules.

# 11) Root Run Instrumentation Pattern

The root run span pattern is fixed.

The implementation must follow `Specification/runtime/observability/spans.md`.

Required pattern:
- create `diagnostics.run` in each of:
  - `Orchestrator::run(...)`
  - `Orchestrator::resume(...)`
  - `Orchestrator::resume_with_input(...)`
- set `run.entrypoint` to:
  - `run`
  - `resume`
  - `resume_with_input`
- create the root span before any downstream async orchestration work starts;
- enter the root span before awaiting downstream orchestration execution;
- keep the root span active across the full awaited orchestration path;
- set final root-span `status` from the final invocation outcome;
- close the root span only after the awaited orchestration future resolves.

Iteration-span rule:
- `diagnostics.iteration` is created when the invocation has a current iteration to operate on;
- `resume(...)` creates `diagnostics.iteration` for the loaded current iteration when one exists;
- `resume(...)` does not synthesize a new iteration span when no current iteration exists.

Working example reference:

- repository-owned OTEL initialization and smoke references are the working comparison target for exporter and subscriber wiring;
- it is the operational reference for:
  - creating the root span at the orchestrator entrypoint
  - entering the root span before downstream `await`
  - keeping child spans under the active root span

Implementation safety rules:
- the root span must not be created inside `drive_to_outcome(...)`;
- the root span must not be recreated inside policy, repository, executor, or request-pipeline layers;
- lower layers must assume the root span already exists and is active;
- if the root span is created in the entrypoint but awaited work happens outside the entered scope, that implementation is invalid even if spans are still emitted.
- parentage must come from the active `tracing` scope, not from explicitly passing OpenTelemetry span context through function signatures.

Rationale:
- this is the most fragile part of async trace wiring;
- getting this wrong produces orphan child spans, broken parentage, or multiple traces for one run;
- the implementation contract is intentionally strict to prevent those failure modes.

# 12) Async Orchestration Instrumentation Pattern

Orchestration instrumentation in async code follows these rules:

- entrypoint logic is implemented as explicit async orchestrator boundaries;
- mandatory orchestration spans are created explicitly in the module that owns them;
- orchestration functions run under the active root run span;
- nested pipeline and dependency spans are created inside the owning module.

Manual ad hoc instrumentation of nested `async move` blocks is forbidden as the orchestration instrumentation strategy.

The required pattern is:
- root run span entered before `await`;
- orchestration helper functions defined as separate async functions when they own a mandatory span boundary;
- mandatory spans created explicitly in the owning function or method;
- dependency spans created inside the owning module function.

Parentage propagation rule:
- repository and executor code rely on the active entered parent `tracing` span for parent-child relationship formation;
- explicit span-context plumbing between orchestrator, repository, executor, and request-pipeline layers is forbidden.

Leaf visibility rule:
- leaf modules may record raw user query, normalized user query, structured query JSON, and final structured output JSON as allowed by `Specification/runtime/observability/spans.md`;
- leaf modules must not record full prompt text, raw retrieved chunk text, or large raw model output into spans or events;
- when structured serialized payloads become too large, the implementation must omit, summarize, or explicitly truncate them.
- leaf modules must not synthesize unavailable raw-input fields; for example, a module that receives only a normalized request records `query.normalized` and does not fabricate `query.raw`.

Leaf identity propagation rule:
- leaf modules inherit run, iteration, step, sequence, and record identity through span parentage from `step_executor.dispatch` and higher orchestration spans;
- leaf modules do not duplicate those orchestration identity attributes on every leaf span unless an explicit querying requirement has been added to the contract.

Sequence and policy-context rule:
- the implementation records `step.sequence_no` on mandatory step-oriented spans when that step sequence is known;
- the implementation records compact policy context through:
  - `policy.finished_steps_count`
  - `policy.pending_step_present`
  - `policy.last_finished_step.kind` when present
- the implementation must not dump full `RunState`, full finished-step lists, or full `StepRecord` payloads into policy spans.

========================
13) Async Block Rules
========================

Instrumentation of async blocks follows these rules:

- an entered root run span remains active across `await`;
- nested async orchestration work is represented by separate instrumented functions when that work owns a mandatory span;
- application code must not implement custom parent-child trace stitching for orchestration spans while the root run span is active;
- async block instrumentation does not rely on implicit argument capture.
- application code must not create explicit OTEL parent contexts for normal orchestration span parenting.

========================
14) `#[tracing::instrument]` Rules
========================

`#[tracing::instrument]` is allowed only in the following form:
- explicit `name = "..."`
- `skip_all`

`#[tracing::instrument]` without `skip_all` is forbidden.

Automatic capture of function parameters into span attributes is forbidden.

`fields(...)` inside `#[tracing::instrument]` is forbidden.

Application code must not add span attributes through `#[tracing::instrument]`.

The set of high-cardinality root-span fields is fixed by:
- `Specification/runtime/observability/spans.md`
- `Measurement/observability/tempo/tempo.yaml`

The only high-cardinality root-span field in the current contract is:
- `run.id`

`run.id` is written during root span creation.

Child spans may include `run.id` when required by `Specification/runtime/observability/spans.md`.
Application code must not introduce additional high-cardinality root-span fields outside that fixed set.

`run.entrypoint` is required on `diagnostics.run`, but it is not the high-cardinality identifier field.

Selected high-cardinality values must not include:
- raw prompt text;
- raw model output text;
- raw retrieved document text;
- raw request bodies;
- raw response bodies.

Method-entry input values are written to the trace only through explicit tracing events inside the method-entry span.

Method-entry events must not include:
- raw prompt text;
- raw model output text;
- raw retrieved document text;
- raw request bodies;
- raw response bodies.

Failure-attribute rule:
- use `error.type` for stable error classification;
- use `error.message` for the human-readable failure text;
- full error text is allowed in `error.message` for this demo project when it does not violate the explicit safety constraints in `Specification/runtime/observability/spans.md`;
- do not introduce a parallel `error.kind` field.

========================
15) Settings Usage Rules
========================

Observability code works only with typed settings objects.

Rules:
- the main startup layer reads config files and environment variables;
- observability initialization receives `&ObservabilitySettings`;
- business modules receive typed settings references;
- observability code does not parse config files;
- observability code does not read raw environment variables directly.

`ObservabilitySettings` field sources are fixed:
- `tracing_enabled`
  - source: `Settings.observability.tracing_enabled`
- `metrics_enabled`
  - source: `Settings.observability.metrics_enabled`
- `tracing_endpoint`
  - source: environment variable `TRACING_ENDPOINT`, resolved into `Settings.observability.tracing_endpoint`
- `metrics_endpoint`
  - source: environment variable `METRICS_ENDPOINT`, resolved into `Settings.observability.metrics_endpoint`
- `trace_batch_scheduled_delay_ms`
  - source: `Settings.observability.trace_batch_scheduled_delay_ms`
- `metrics_export_interval_ms`
  - source: `Settings.observability.metrics_export_interval_ms`

========================
16) Instrumentation Ownership Rules
========================

Instrumentation ownership is fixed:

- the root run span `diagnostics.run` is created with manual `tracing::span!(...)` or `tracing::info_span!(...)` in the orchestrator entrypoint;
- mandatory orchestration spans are created with manual `tracing::span!(...)` or `tracing::info_span!(...)` in the owning orchestrator, repository, executor, or request-pipeline function;
- internal method-entry spans are created with `#[tracing::instrument(name = \"...\", skip_all)]` for every internal method;
- the mandatory orchestration spans are the spans defined in `Specification/runtime/observability/spans.md`;
- dependency spans such as `llm.call` and `qdrant.search` are created explicitly inside their owning module;
- child spans follow the attribute contract defined in `Specification/runtime/observability/spans.md`;
- application code must not use `#[tracing::instrument]` for dependency spans;
- application code must not use `#[tracing::instrument]` for mandatory orchestration spans.
- one function boundary must not create both a `#[tracing::instrument]` span and a manual same-boundary span with the same semantic role;
- one function boundary uses exactly one span-construction strategy for that semantic boundary:
  - manual `tracing::span!(...)` or `tracing::info_span!(...)`; or
  - `#[tracing::instrument(name = \"...\", skip_all)]`
- application code must not combine `#[tracing::instrument]` with an explicit same-name manual child span for the same function boundary.

Required mandatory span attribute pattern:

- `span.module`, `span.stage`, and `status` are declared explicitly when the mandatory span is created;
- method input values are not encoded as mandatory span attributes.
- when a step span becomes associated with a persisted step record, the implementation writes `record.id` onto the parent step span;
- when a failed business-step result is successfully persisted, `repository.step.finish` records `persisted.step.outcome = "failure"` while keeping repository `status = "ok"`.

Failure propagation pattern:
- when `step_executor.dispatch` ends in failure, it records:
  - `status = "error"`
  - `step.outcome = "failure"`
  - `error.type`
  - `error.message`
- the owning `orchestrator.step` records the same business-step failure outcome;
- the active `diagnostics.iteration` records `status = "error"`;
- the root `diagnostics.run` records:
  - `status = "error"`
  - `run.outcome = "failure"`
  - `failed_step.kind` when the failed step is known
- if the invocation ends normally through `FinishWithResult`, `diagnostics.run` records:
  - `status = "ok"`
  - `run.outcome = "success"`
  - `terminal.transition = "FinishWithResult"`
- if the invocation ends through `FinishWithError`, `diagnostics.run` records:
  - `status = "error"`
  - `run.outcome = "failure"`
  - `terminal.transition = "FinishWithError"`

OpenTelemetry status-code rule:
- when the project stack supports it cleanly, failed spans should also emit the corresponding OpenTelemetry error status code;
- this does not replace the required `status = "error"` attribute contract.

Event suppression rule:
- do not add duplicate step lifecycle events for pending-opened, pending-persisted, execution-started, execution-finished, or finished-persisted boundaries;
- the contract uses explicit spans for those boundaries instead of duplicate lifecycle events.

Leaf event rule:
- leaf-module diagnostic events are allowed only for internal module decisions and observations;
- leaf-module events must remain compact and must not duplicate orchestrator lifecycle boundaries that are already represented by spans;
- application code must not emit a leaf event unless the event's required payload fields are explicitly defined in `Specification/runtime/observability/spans.md`;
- generic `*_completed`, `*_received`, and `*_checked` events that only restate span status or existing span attributes should be omitted.

========================
17) Log Filter Contract
========================

Runtime log filtering is controlled through:
- `RUST_LOG`

Rules:
- the process startup layer resolves `RUST_LOG` before observability initialization;
- the validated default filter is `distributed_diagnostics=debug,info`;
- the default filter must keep `distributed_diagnostics` business logs at `debug` while suppressing transport-level dependency noise below `info`;
- transport stack debug noise from dependencies such as HTTP clients, HTTP/2 internals, and OTEL transport internals must not be enabled by default.
- when tracing initialization is enabled, the tracing subscriber must include `tracing_subscriber::EnvFilter`;
- the tracing subscriber must resolve its filter from `RUST_LOG` through `tracing_subscriber::EnvFilter::from_default_env()`;
- if `RUST_LOG` is unset, the tracing subscriber must use the exact fallback filter string `distributed_diagnostics=debug,info`;
- the generated implementation must not install a tracing subscriber that ignores `RUST_LOG`;
- the generated implementation must not replace `RUST_LOG` handling with a hardcoded filter when `RUST_LOG` is present;
- the generated implementation must not construct the tracing subscriber without an explicit env-filter layer.

Rationale:
- unrestricted global `debug` produces low-signal output during validation and smoke runs;
- `distributed_diagnostics=debug,info` preserves application-level diagnostics while keeping telemetry validation readable.

========================
18) Reference Implementation Example
========================

The following pseudocode is a reference implementation example for the required contract defined above.

It is a reference example and must not be copied literally into generated code:

The repository also contains a working concrete example under:

- `Execution/otel_runtime_smoke/`

That working example is the preferred comparison target when validating OTEL stack initialization and root-span behavior in real runs.

```rust
struct TracingGuard {
    provider: SdkTracerProvider,
}

struct MetricsGuard {
    provider: SdkMeterProvider,
}

impl Drop for TracingGuard {
    fn drop(&mut self) {
        let _ = self.provider.shutdown();
    }
}

impl Drop for MetricsGuard {
    fn drop(&mut self) {
        let _ = self.provider.shutdown();
    }
}

fn init_tracing(settings: &ObservabilitySettings) -> Result<TracingGuard, InitError> {
    let span_exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&settings.tracing_endpoint)
        .build()?;

    let batch_processor = BatchSpanProcessor::builder(span_exporter)
        .with_batch_config(
            BatchConfigBuilder::default()
                .with_scheduled_delay(Duration::from_millis(
                    settings.trace_batch_scheduled_delay_ms,
                ))
                .build(),
        )
        .build();

    let tracer_provider = SdkTracerProvider::builder()
        .with_span_processor(batch_processor)
        .build();

    let tracer = tracer_provider.tracer("distributed_diagnostics");

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("distributed_diagnostics=debug,info"));

    let subscriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_opentelemetry::layer().with_tracer(tracer));

    tracing::subscriber::set_global_default(subscriber)?;
    opentelemetry::global::set_tracer_provider(tracer_provider.clone());

    Ok(TracingGuard {
        provider: tracer_provider,
    })
}

fn init_metrics(settings: &ObservabilitySettings) -> Result<MetricsGuard, InitError> {
    let metric_exporter = MetricExporter::builder()
        .with_tonic()
        .with_endpoint(&settings.metrics_endpoint)
        .build()?;

    let meter_provider = SdkMeterProvider::builder()
        .with_reader(
            PeriodicReader::builder(metric_exporter)
                .with_interval(Duration::from_millis(
                    settings.metrics_export_interval_ms,
                ))
                .build(),
        )
        .build();

    opentelemetry::global::set_meter_provider(meter_provider.clone());

    Ok(MetricsGuard {
        provider: meter_provider,
    })
}

async fn append_pending_step_record(
    run_id: RunId,
    iteration_id: RunIterationId,
    step: StepKind,
) -> Result<(), RuntimeError> {
    let span = tracing::info_span!(
        "repository.step.append_pending",
        run.id = %run_id,
        iteration.id = %iteration_id,
        step.kind = %step.as_ref(),
        span.module = "run_repository",
        span.stage = "append_pending",
        status = tracing::field::Empty,
    );
    let _enter = span.enter();

    // persist the pending step record
}

async fn handle_run(
    entrypoint: &'static str,
    run_id: RunId,
    settings: &Settings,
) -> Result<RunOutcome, RuntimeError> {
    let root_span = tracing::info_span!(
        "diagnostics.run",
        run.id = %run_id,
        run.entrypoint = entrypoint,
        span.module = "orchestrator",
        span.stage = "run",
        status = tracing::field::Empty,
    );

    let _enter = root_span.enter();

    // write run.id and run.entrypoint during root span creation
    // call orchestration work under the active root span
    // create diagnostics.iteration when the invocation has a current iteration
    // create orchestrator.policy.next_transition, orchestrator.step,
    // repository.step.append_pending, step_executor.dispatch,
    // and repository.step.finish in their owning modules
    // record final root span status as "ok" or "error"
    // return the final run outcome
}

async fn async_main(settings: Settings) -> Result<(), InitError> {
    let tracing_guard = init_tracing(&settings.observability)?;
    let metrics_guard = init_metrics(&settings.observability)?;

    // start the long-running runtime process
    // await the graceful shutdown signal
    // stop request processing
    // keep tracing_guard alive until the application stop sequence completes
    // keep metrics_guard alive until the application stop sequence completes

    Ok(())
}
```

The example illustrates these required rules:
- tracing initialization happens before request handling;
- metrics initialization happens before request handling;
- root run span is entered before awaiting orchestration execution;
- mandatory orchestration spans are created explicitly;
- provider shutdown happens during the application shutdown sequence;
- RAII guards own provider shutdown behavior.

========================
19) Metrics Shutdown Follow-Up Note
========================

The validated stack emits a `PeriodicReader` shutdown timeout warning during short-lived validation runs in some environments even when metrics export has already succeeded.

This warning is technical debt and follow-up work, not a failure of the required runtime contract.

Current interpretation rules:
- successful metric visibility in the configured backend is the primary validation signal;
- a shutdown-time `PeriodicReader` warning in a short-lived validation run does not by itself invalidate successful metric export already observed in the backend;
- mitigation of this warning belongs to follow-up observability hardening work, not to business request logic.

========================
20) Artifact Placement
========================

Generated observability artifacts belong in:
- `Measurement/observability/`
