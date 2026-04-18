# 1) Purpose / Scope

This document defines the required Rust implementation contract for runtime observability initialization.

It defines:
- required crates and versions;
- required runtime components;
- required typed settings type;
- required initialization order;
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
- graceful shutdown ownership

The implementation keeps this internal observability API inside `src/observability/mod.rs`.

Business modules do not construct exporter stacks, provider objects, or global subscribers directly.

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

# 11) Root Request Instrumentation Pattern

The request root span pattern is fixed:

- create the root request span at the request entrypoint;
- enter the root span before awaiting downstream request execution;
- keep the root span active across the full request path;
- close the request span after final success or terminal failure.

Working example reference:

- `Execution/otel_runtime_smoke/` is the confirmed working example in this repository for the required root-span organization pattern;
- it is the operational reference for:
  - creating the root span at request entrypoint
  - entering the root span before downstream `await`
  - keeping child stage spans under the active root span
- generated `rag_runtime` code must follow the contract defined in this document, and `Execution/otel_runtime_smoke/` should be used as the working comparison target when validating root-span behavior.

# 12) Async Stage Instrumentation Pattern

Stage instrumentation in async code follows these rules:

- stage logic is implemented as explicit `async fn` boundaries;
- mandatory stage spans are created explicitly at the async function boundary;
- stage functions run under the active root request span;
- nested dependency spans are created inside the owning stage.

Manual ad hoc instrumentation of nested `async move` blocks is forbidden as the stage instrumentation strategy.

The required pattern is:
- root request span entered before `await`;
- stage functions defined as separate async functions;
- mandatory stage spans created explicitly at the async function boundary;
- dependency spans created inside the owning stage function.

========================
13) Async Block Rules
========================

Instrumentation of async blocks follows these rules:

- an entered root request span remains active across `await`;
- nested async stages are represented by separate instrumented async functions;
- application code must not implement custom parent-child trace stitching for stage spans while the root request span is active;
- async block instrumentation does not rely on implicit argument capture.

If an explicit OTEL parent context is created, child OTEL spans must be created with `start_with_context`.

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
- `Specification/codegen/rag_runtime/observability/spans.md`
- `Measurement/observability/tempo/tempo.yaml`

The only high-cardinality root-span field in the current contract is:
- `request_id`

`request_id` is written during root span creation.

Child spans must not duplicate `request_id`.

Application code must not introduce additional high-cardinality root-span fields outside that fixed set.

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

- the root request span `rag.request` is created with manual `tracing::info_span!(...)`;
- mandatory stage spans are created with manual `tracing::info_span!(...)` on the owning async stage function;
- internal method-entry spans are created with `#[tracing::instrument(name = \"...\", skip_all)]` for every internal method;
- child stage spans do not duplicate `request_id`;
- the explicit dependency spans are:
  - `retrieval.embedding`
  - `retrieval.vector_search`
  - `generation.chat`
- those dependency spans are created explicitly inside their owning stage;
- OpenInference spans are created explicitly inside the owning stage;
- application code must not use `#[tracing::instrument]` for dependency spans;
- application code must not use `#[tracing::instrument]` for OpenInference spans.
- one function boundary must not create both a `#[tracing::instrument]` span and a manual same-boundary span with the same semantic role;
- one function boundary uses exactly one span-construction strategy for that semantic boundary:
  - manual `tracing::info_span!(...)`; or
  - `#[tracing::instrument(name = \"...\", skip_all)]`
- application code must not combine `#[tracing::instrument]` with an explicit same-name manual child span for the same function boundary.

Required stage span attribute pattern:

- `span.module`, `span.stage`, and `status` are declared explicitly in `tracing::info_span!(...)` when the mandatory stage span is created;
- method input values are not encoded as stage span attributes.

========================
17) Log Filter Contract
========================

Runtime log filtering is controlled through:
- `RUST_LOG`

Rules:
- the process startup layer resolves `RUST_LOG` before observability initialization;
- the validated default filter is `rag_runtime=debug,info`;
- the default filter must keep `rag_runtime` business logs at `debug` while suppressing transport-level dependency noise below `info`;
- transport stack debug noise from dependencies such as HTTP clients, HTTP/2 internals, and OTEL transport internals must not be enabled by default.
- when tracing initialization is enabled, the tracing subscriber must include `tracing_subscriber::EnvFilter`;
- the tracing subscriber must resolve its filter from `RUST_LOG` through `tracing_subscriber::EnvFilter::from_default_env()`;
- if `RUST_LOG` is unset, the tracing subscriber must use the exact fallback filter string `rag_runtime=debug,info`;
- the generated implementation must not install a tracing subscriber that ignores `RUST_LOG`;
- the generated implementation must not replace `RUST_LOG` handling with a hardcoded filter when `RUST_LOG` is present;
- the generated implementation must not construct the tracing subscriber without an explicit env-filter layer.

Rationale:
- unrestricted global `debug` produces low-signal output during validation and smoke runs;
- `rag_runtime=debug,info` preserves application-level diagnostics while keeping telemetry validation readable.

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

    let tracer = tracer_provider.tracer("rag_runtime");

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("rag_runtime=debug,info"));

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

async fn input_validation_stage(
    settings: &InputValidationSettings,
) -> Result<ValidatedUserRequest, RagRuntimeError> {
    let span = tracing::info_span!(
        "input_validation",
        span.module = "input_validation",
        span.stage = "input_validation",
        status = tracing::field::Empty,
    );
    let _enter = span.enter();

    // stage-specific logic computes normalized_query

    tracing::info!(
        trim_whitespace = settings.trim_whitespace,
        collapse_internal_whitespace = settings.collapse_internal_whitespace,
        normalized_query_length = normalized_query_length,
        "query_normalized"
    );

    // return stage result
}

async fn handle_request(
    request: UserRequest,
    settings: &Settings,
) -> Result<UserResponse, RagRuntimeError> {
    let request_id = Uuid::new_v4().to_string();

    let root_span = tracing::info_span!(
        "rag.request",
        request_id = %request_id,
        span.module = "orchestration",
        span.stage = "request",
        status = tracing::field::Empty,
    );

    let _enter = root_span.enter();

    // create one UUID v4 request_id for this request before root span creation
    // write request_id into the root span during root span creation
    // call the instrumented async stage functions for input_validation, retrieval, and generation under the active root span
    // record final root span status as "ok" or "error"
    // return the final request result
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
- root request span is entered before awaiting stage execution;
- mandatory stage spans are created explicitly;
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
