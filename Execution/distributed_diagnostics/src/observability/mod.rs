use std::time::Duration;

use opentelemetry::global;
use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};
use opentelemetry_sdk::trace::{
    BatchConfigBuilder, BatchSpanProcessor, Sampler, SdkTracerProvider,
};
use opentelemetry_sdk::Resource;
use thiserror::Error;
use tracing_subscriber::{layer::SubscriberExt, EnvFilter, Registry};

use crate::config::ObservabilitySettings;

#[derive(Debug, Error)]
pub enum ObservabilityError {
    #[error("observability initialization failed: {message}")]
    Initialization { message: String },
}

pub struct ObservabilityRuntime {
    tracing_guard: Option<TracingGuard>,
    metrics_guard: Option<MetricsGuard>,
}

struct TracingGuard {
    provider: SdkTracerProvider,
}

struct MetricsGuard {
    provider: SdkMeterProvider,
}

impl ObservabilityRuntime {
    pub fn initialize(settings: &ObservabilitySettings) -> Result<Self, ObservabilityError> {
        let tracing_guard = if settings.tracing_enabled {
            Some(init_tracing(settings)?)
        } else {
            None
        };

        let metrics_guard = if settings.metrics_enabled {
            Some(init_metrics(settings)?)
        } else {
            None
        };

        Ok(Self {
            tracing_guard,
            metrics_guard,
        })
    }

    pub fn flush(&self) {
        if let Some(guard) = &self.tracing_guard {
            let _ = guard.provider.force_flush();
        }
        if let Some(guard) = &self.metrics_guard {
            let _ = guard.provider.force_flush();
        }
    }
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

fn build_resource(service_name: &str) -> Resource {
    Resource::builder()
        .with_service_name(service_name.to_string())
        .build()
}

fn init_tracing(settings: &ObservabilitySettings) -> Result<TracingGuard, ObservabilityError> {
    let otlp_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&settings.tracing_endpoint)
        .build()
        .map_err(|error| ObservabilityError::Initialization {
            message: format!("failed to build trace exporter: {error}"),
        })?;

    let batch_processor = BatchSpanProcessor::builder(otlp_exporter)
        .with_batch_config(
            BatchConfigBuilder::default()
                .with_scheduled_delay(Duration::from_millis(
                    settings.trace_batch_scheduled_delay_ms,
                ))
                .build(),
        )
        .build();

    let provider = SdkTracerProvider::builder()
        .with_span_processor(batch_processor)
        .with_sampler(Sampler::AlwaysOn)
        .with_resource(build_resource("distributed_diagnostics"))
        .build();

    let tracer = provider.tracer("distributed_diagnostics");
    let telemetry_layer = tracing_opentelemetry::OpenTelemetryLayer::new(tracer);
    let subscriber = Registry::default()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("distributed_diagnostics=debug,info")))
        .with(telemetry_layer);

    tracing::subscriber::set_global_default(subscriber).map_err(|error| {
        ObservabilityError::Initialization {
            message: format!("failed to install global tracing subscriber: {error}"),
        }
    })?;

    global::set_tracer_provider(provider.clone());

    Ok(TracingGuard { provider })
}

fn init_metrics(settings: &ObservabilitySettings) -> Result<MetricsGuard, ObservabilityError> {
    let exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_endpoint(&settings.metrics_endpoint)
        .build()
        .map_err(|error| ObservabilityError::Initialization {
            message: format!("failed to build metric exporter: {error}"),
        })?;

    let provider = SdkMeterProvider::builder()
        .with_reader(
            PeriodicReader::builder(exporter)
                .with_interval(Duration::from_millis(settings.metrics_export_interval_ms))
                .build(),
        )
        .with_resource(build_resource("distributed_diagnostics"))
        .build();

    global::set_meter_provider(provider.clone());

    Ok(MetricsGuard { provider })
}

pub(crate) fn run_span(run_id: &str, entrypoint: &'static str) -> tracing::Span {
    tracing::info_span!(
        "diagnostics.run",
        run.id = run_id,
        run.entrypoint = entrypoint,
        span.module = "orchestrator",
        span.stage = "run",
        status = tracing::field::Empty,
    )
}

pub(crate) fn iteration_span(run_id: &str, iteration_id: &str, sequence_no: u64) -> tracing::Span {
    tracing::info_span!(
        "diagnostics.iteration",
        run.id = run_id,
        iteration.id = iteration_id,
        iteration.sequence_no = sequence_no,
        span.module = "orchestrator",
        span.stage = "iteration",
        status = tracing::field::Empty,
    )
}

pub(crate) fn policy_transition_span(run_id: &str, iteration_id: &str) -> tracing::Span {
    tracing::info_span!(
        "orchestrator.policy.next_transition",
        run.id = run_id,
        iteration.id = iteration_id,
        span.module = "transition_policy",
        span.stage = "next_transition",
        status = tracing::field::Empty,
        transition.kind = tracing::field::Empty,
        step.kind = tracing::field::Empty,
    )
}

pub(crate) fn step_span(run_id: &str, iteration_id: &str, step_kind: &str) -> tracing::Span {
    tracing::info_span!(
        "orchestrator.step",
        run.id = run_id,
        iteration.id = iteration_id,
        step.kind = step_kind,
        span.module = "orchestrator",
        span.stage = "step",
        status = tracing::field::Empty,
    )
}

pub(crate) fn append_pending_span(
    run_id: &str,
    iteration_id: &str,
    step_kind: &str,
    record_id: &str,
) -> tracing::Span {
    tracing::info_span!(
        "repository.step.append_pending",
        run.id = run_id,
        iteration.id = iteration_id,
        step.kind = step_kind,
        record.id = record_id,
        span.module = "run_repository",
        span.stage = "append_pending",
        status = tracing::field::Empty,
    )
}

pub(crate) fn dispatch_span(run_id: &str, iteration_id: &str, step_kind: &str) -> tracing::Span {
    tracing::info_span!(
        "step_executor.dispatch",
        run.id = run_id,
        iteration.id = iteration_id,
        step.kind = step_kind,
        span.module = "step_executor",
        span.stage = "dispatch",
        status = tracing::field::Empty,
    )
}

pub(crate) fn finish_step_span(
    run_id: &str,
    iteration_id: &str,
    step_kind: &str,
    record_id: &str,
) -> tracing::Span {
    tracing::info_span!(
        "repository.step.finish",
        run.id = run_id,
        iteration.id = iteration_id,
        step.kind = step_kind,
        record.id = record_id,
        span.module = "run_repository",
        span.stage = "finish",
        status = tracing::field::Empty,
    )
}
