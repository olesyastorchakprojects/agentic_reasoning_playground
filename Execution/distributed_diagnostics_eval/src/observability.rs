use std::time::Duration;

use opentelemetry::global;
use opentelemetry::trace::{Status, TracerProvider};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::{
    BatchConfigBuilder, BatchSpanProcessor, Sampler, SdkTracerProvider,
};
use opentelemetry_sdk::Resource;
use thiserror::Error;
use tracing::field;
use tracing_subscriber::{layer::SubscriberExt, EnvFilter, Registry};

use crate::config::ObservabilitySettings;

#[derive(Debug, Error)]
pub enum ObservabilityError {
    #[error("observability initialization failed: {message}")]
    Initialization { message: String },
}

pub struct ObservabilityRuntime {
    tracing_guard: Option<TracingGuard>,
}

struct TracingGuard {
    provider: SdkTracerProvider,
}

impl ObservabilityRuntime {
    pub fn initialize(settings: &ObservabilitySettings) -> Result<Self, ObservabilityError> {
        let tracing_guard = if settings.tracing_enabled {
            Some(init_tracing(settings)?)
        } else {
            None
        };
        Ok(Self { tracing_guard })
    }

    pub fn flush(&self) {
        if let Some(guard) = &self.tracing_guard {
            let _ = guard.provider.force_flush();
        }
    }
}

impl Drop for TracingGuard {
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
        .map_err(|e| ObservabilityError::Initialization {
            message: format!("failed to build trace exporter: {e}"),
        })?;

    let batch_processor = BatchSpanProcessor::builder(otlp_exporter)
        .with_batch_config(
            BatchConfigBuilder::default()
                .with_scheduled_delay(Duration::from_millis(250))
                .build(),
        )
        .build();

    let provider = SdkTracerProvider::builder()
        .with_span_processor(batch_processor)
        .with_sampler(Sampler::AlwaysOn)
        .with_resource(build_resource(&settings.service_name))
        .build();

    let tracer = provider.tracer(settings.service_name.clone());
    let telemetry_layer = tracing_opentelemetry::OpenTelemetryLayer::new(tracer);
    let subscriber = Registry::default()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with(telemetry_layer);

    tracing::subscriber::set_global_default(subscriber).map_err(|e| {
        ObservabilityError::Initialization {
            message: format!("failed to install global tracing subscriber: {e}"),
        }
    })?;

    global::set_tracer_provider(provider.clone());

    Ok(TracingGuard { provider })
}

pub fn record_error(span: &tracing::Span, error_type: &str, error_message: &str) {
    use tracing_opentelemetry::OpenTelemetrySpanExt;
    span.record("error.type", error_type);
    span.record("error.message", error_message);
    span.set_status(Status::Error {
        description: error_message.to_string().into(),
    });
}

// ---------------------------------------------------------------------------
// Eval span helpers
// ---------------------------------------------------------------------------

pub fn eval_run_span(
    eval_run_id: &str,
    run_type: &str,
    judge_model: &str,
) -> tracing::Span {
    tracing::info_span!(
        "eval.run",
        openinference.span.kind = "CHAIN",
        eval.run_id = eval_run_id,
        eval.run_type = run_type,
        eval.judge_model = judge_model,
        eval.status = field::Empty,
        eval.runtime_run_count = field::Empty,
        eval.iterations_evaluated_count = field::Empty,
        error.type = field::Empty,
        error.message = field::Empty,
    )
}

pub fn eval_subject_span(
    eval_run_id: &str,
    runtime_run_id: &str,
    iteration_id: &str,
) -> tracing::Span {
    tracing::info_span!(
        "eval.judge_request_suites.subject",
        openinference.span.kind = "CHAIN",
        eval.run_id = eval_run_id,
        eval.runtime_run_id = runtime_run_id,
        eval.iteration_id = iteration_id,
        eval.subject_status = field::Empty,
        error.type = field::Empty,
        error.message = field::Empty,
    )
}

pub fn eval_suite_span(
    eval_run_id: &str,
    runtime_run_id: &str,
    iteration_id: &str,
    suite_name: &str,
    suite_category: &str,
    suite_scope: &str,
    judge_model: &str,
) -> tracing::Span {
    tracing::info_span!(
        "eval.judge_request_suites.suite",
        openinference.span.kind = "LLM",
        eval.run_id = eval_run_id,
        eval.runtime_run_id = runtime_run_id,
        eval.iteration_id = iteration_id,
        eval.suite_name = suite_name,
        eval.suite_category = suite_category,
        eval.suite_scope = suite_scope,
        llm.model_name = judge_model,
        llm.token_count.prompt = field::Empty,
        llm.token_count.completion = field::Empty,
        llm.token_count.total = field::Empty,
        eval.total_cost_usd = field::Empty,
        eval.score = field::Empty,
        error.type = field::Empty,
        error.message = field::Empty,
    )
}

pub fn eval_summary_span(
    eval_run_id: &str,
    runtime_run_count: usize,
) -> tracing::Span {
    tracing::info_span!(
        "eval.build_eval_summary",
        openinference.span.kind = "CHAIN",
        eval.run_id = eval_run_id,
        eval.runtime_run_count = runtime_run_count,
        eval.iterations_evaluated_count = field::Empty,
        error.type = field::Empty,
        error.message = field::Empty,
    )
}
