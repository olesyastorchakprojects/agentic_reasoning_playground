use std::time::Duration;

use opentelemetry::global;
use opentelemetry::trace::{Status, TracerProvider};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};
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
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("distributed_diagnostics=debug,info")),
        )
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

pub(crate) fn record_error(span: &tracing::Span, error_type: &str, error_message: &str) {
    use tracing_opentelemetry::OpenTelemetrySpanExt;
    span.record("error.type", error_type);
    span.record("error.message", error_message);
    span.set_status(Status::Error {
        description: error_message.to_string().into(),
    });
}

pub(crate) fn run_span(run_id: &str, entrypoint: &'static str) -> tracing::Span {
    tracing::info_span!(
        "diagnostics.run",
        run.id = run_id,
        run.entrypoint = entrypoint,
        span.module = "orchestrator",
        span.stage = "run",
        status = tracing::field::Empty,
        run.outcome = tracing::field::Empty,
        terminal.transition = tracing::field::Empty,
        failed_step.kind = tracing::field::Empty,
        error.type = tracing::field::Empty,
        error.message = tracing::field::Empty,
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

pub(crate) fn policy_transition_span(
    run_id: &str,
    iteration_id: &str,
    finished_steps_count: u64,
    pending_step_present: bool,
    last_finished_step_kind: Option<&str>,
) -> tracing::Span {
    let span = tracing::info_span!(
        "orchestrator.policy.next_transition",
        run.id = run_id,
        iteration.id = iteration_id,
        span.module = "transition_policy",
        span.stage = "next_transition",
        policy.finished_steps_count = finished_steps_count,
        policy.pending_step_present = pending_step_present,
        policy.last_finished_step.kind = tracing::field::Empty,
        status = tracing::field::Empty,
        transition.kind = tracing::field::Empty,
        step.kind = tracing::field::Empty,
        step.sequence_no = tracing::field::Empty,
        error.type = tracing::field::Empty,
        error.message = tracing::field::Empty,
    );
    if let Some(kind) = last_finished_step_kind {
        span.record("policy.last_finished_step.kind", kind);
    }
    span
}

pub(crate) fn step_span(run_id: &str, iteration_id: &str, step_kind: &str) -> tracing::Span {
    let otel_name = format!("step.{step_kind}");
    tracing::info_span!(
        "orchestrator.step",
        run.id = run_id,
        iteration.id = iteration_id,
        step.kind = step_kind,
        otel.name = otel_name.as_str(),
        span.module = "orchestrator",
        span.stage = "step",
        status = tracing::field::Empty,
        step.sequence_no = tracing::field::Empty,
        record.id = tracing::field::Empty,
        step.outcome = tracing::field::Empty,
        error.type = tracing::field::Empty,
        error.message = tracing::field::Empty,
    )
}

pub(crate) fn append_pending_span(
    run_id: &str,
    iteration_id: &str,
    step_kind: &str,
    record_id: &str,
    step_sequence_no: u64,
) -> tracing::Span {
    tracing::info_span!(
        "repository.step.append_pending",
        run.id = run_id,
        iteration.id = iteration_id,
        step.kind = step_kind,
        record.id = record_id,
        step.sequence_no = step_sequence_no,
        span.module = "run_repository",
        span.stage = "append_pending",
        status = tracing::field::Empty,
        error.type = tracing::field::Empty,
        error.message = tracing::field::Empty,
    )
}

pub(crate) fn dispatch_span(
    run_id: &str,
    iteration_id: &str,
    step_kind: &str,
    step_sequence_no: u64,
) -> tracing::Span {
    let otel_name = format!("executor.{step_kind}");
    tracing::info_span!(
        "step_executor.dispatch",
        run.id = run_id,
        iteration.id = iteration_id,
        step.kind = step_kind,
        otel.name = otel_name.as_str(),
        step.sequence_no = step_sequence_no,
        span.module = "step_executor",
        span.stage = "dispatch",
        status = tracing::field::Empty,
        step.outcome = tracing::field::Empty,
        error.type = tracing::field::Empty,
        error.message = tracing::field::Empty,
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
        step.sequence_no = tracing::field::Empty,
        persisted.step.outcome = tracing::field::Empty,
        span.module = "run_repository",
        span.stage = "finish",
        status = tracing::field::Empty,
        error.type = tracing::field::Empty,
        error.message = tracing::field::Empty,
    )
}

pub(crate) fn oi_iteration_chain_span(
    parent: &tracing::Span,
    run_id: &str,
    iteration_id: &str,
    sequence_no: u64,
) -> tracing::Span {
    tracing::info_span!(
        parent: parent,
        "oi.chain.diagnostic_iteration",
        openinference.span.kind = "CHAIN",
        run.id = run_id,
        iteration.id = iteration_id,
        iteration.sequence_no = sequence_no,
        input.value = field::Empty,
        input.mime_type = field::Empty,
        output.value = field::Empty,
        output.mime_type = field::Empty,
        run.outcome = field::Empty,
        status = field::Empty,
        error.type = field::Empty,
        error.message = field::Empty,
    )
}

pub(crate) fn oi_llm_query_structuring_span(parent: &tracing::Span) -> tracing::Span {
    tracing::info_span!(
        parent: parent,
        "oi.llm.query_structuring",
        openinference.span.kind = "LLM",
        input.value = field::Empty,
        input.mime_type = field::Empty,
        output.value = field::Empty,
        output.mime_type = field::Empty,
        llm.raw_response = field::Empty,
        llm.model_name = field::Empty,
        llm.provider = field::Empty,
        llm.invocation_parameters = field::Empty,
        llm.token_count.prompt = field::Empty,
        llm.token_count.completion = field::Empty,
        llm.token_count.total = field::Empty,
        status = field::Empty,
        error.type = field::Empty,
        error.message = field::Empty,
    )
}

pub(crate) fn oi_llm_diagnostic_response_span(parent: &tracing::Span) -> tracing::Span {
    tracing::info_span!(
        parent: parent,
        "oi.llm.diagnostic_response",
        openinference.span.kind = "LLM",
        input.value = field::Empty,
        input.mime_type = field::Empty,
        output.value = field::Empty,
        output.mime_type = field::Empty,
        llm.model_name = field::Empty,
        llm.provider = field::Empty,
        llm.invocation_parameters = field::Empty,
        llm.token_count.prompt = field::Empty,
        llm.token_count.completion = field::Empty,
        llm.token_count.total = field::Empty,
        llm.raw_response = field::Empty,
        status = field::Empty,
        error.type = field::Empty,
        error.message = field::Empty,
    )
}

pub(crate) fn oi_guardrail_response_validation_span(parent: &tracing::Span) -> tracing::Span {
    tracing::info_span!(
        parent: parent,
        "oi.guardrail.response_validation",
        openinference.span.kind = "GUARDRAIL",
        input.value = field::Empty,
        input.mime_type = field::Empty,
        output.value = field::Empty,
        output.mime_type = field::Empty,
        status = field::Empty,
        error.type = field::Empty,
        error.message = field::Empty,
    )
}

pub(crate) fn oi_llm_observation_boundary_resolver_span(parent: &tracing::Span) -> tracing::Span {
    tracing::info_span!(
        parent: parent,
        "oi.llm.observation_boundary_resolver",
        openinference.span.kind = "LLM",
        input.value = field::Empty,
        input.mime_type = field::Empty,
        output.value = field::Empty,
        output.mime_type = field::Empty,
        llm.raw_response = field::Empty,
        llm.model_name = field::Empty,
        llm.provider = field::Empty,
        llm.invocation_parameters = field::Empty,
        llm.token_count.prompt = field::Empty,
        llm.token_count.completion = field::Empty,
        llm.token_count.total = field::Empty,
        status = field::Empty,
        error.type = field::Empty,
        error.message = field::Empty,
    )
}

pub(crate) fn oi_llm_observation_extraction_span(parent: &tracing::Span) -> tracing::Span {
    tracing::info_span!(
        parent: parent,
        "oi.llm.observation_extraction",
        openinference.span.kind = "LLM",
        input.value = field::Empty,
        input.mime_type = field::Empty,
        output.value = field::Empty,
        output.mime_type = field::Empty,
        llm.raw_response = field::Empty,
        llm.model_name = field::Empty,
        llm.provider = field::Empty,
        llm.invocation_parameters = field::Empty,
        llm.token_count.prompt = field::Empty,
        llm.token_count.completion = field::Empty,
        llm.token_count.total = field::Empty,
        status = field::Empty,
        error.type = field::Empty,
        error.message = field::Empty,
    )
}

pub(crate) fn oi_retriever_candidate_cards_span(parent: &tracing::Span) -> tracing::Span {
    tracing::info_span!(
        parent: parent,
        "oi.retriever.candidate_cards",
        openinference.span.kind = "RETRIEVER",
        input.value = field::Empty,
        input.mime_type = field::Empty,
        output.value = field::Empty,
        output.mime_type = field::Empty,
        status = field::Empty,
        error.type = field::Empty,
        error.message = field::Empty,
    )
}

pub(crate) fn oi_retriever_incident_primary_span(parent: &tracing::Span) -> tracing::Span {
    tracing::info_span!(
        parent: parent,
        "oi.retriever.incident_evidence.primary",
        openinference.span.kind = "RETRIEVER",
        input.value = field::Empty,
        input.mime_type = field::Empty,
        output.value = field::Empty,
        output.mime_type = field::Empty,
        status = field::Empty,
        error.type = field::Empty,
        error.message = field::Empty,
    )
}

pub(crate) fn oi_retriever_incident_alternatives_span(parent: &tracing::Span) -> tracing::Span {
    tracing::info_span!(
        parent: parent,
        "oi.retriever.incident_evidence.alternatives",
        openinference.span.kind = "RETRIEVER",
        input.value = field::Empty,
        input.mime_type = field::Empty,
        output.value = field::Empty,
        output.mime_type = field::Empty,
        status = field::Empty,
        error.type = field::Empty,
        error.message = field::Empty,
    )
}

pub(crate) fn oi_retriever_theory_span(parent: &tracing::Span) -> tracing::Span {
    tracing::info_span!(
        parent: parent,
        "oi.retriever.theory_evidence",
        openinference.span.kind = "RETRIEVER",
        input.value = field::Empty,
        input.mime_type = field::Empty,
        output.value = field::Empty,
        output.mime_type = field::Empty,
        status = field::Empty,
        error.type = field::Empty,
        error.message = field::Empty,
    )
}

pub(crate) fn oi_chain_query_structuring_metrics_span(parent: &tracing::Span) -> tracing::Span {
    tracing::info_span!(
        parent: parent,
        "oi.chain.query_structuring_metrics",
        openinference.span.kind = "CHAIN",
        qs.metrics.present = true,
        qs.metrics.version = "v1",
        input.value = field::Empty,
        input.mime_type = field::Empty,
        output.value = field::Empty,
        output.mime_type = field::Empty,
        status = field::Empty,
        error.type = field::Empty,
        error.message = field::Empty,
    )
}

pub(crate) fn oi_chain_candidate_card_retrieval_metrics_span(
    parent: &tracing::Span,
) -> tracing::Span {
    tracing::info_span!(
        parent: parent,
        "oi.chain.candidate_card_retrieval_metrics",
        openinference.span.kind = "CHAIN",
        rt.metrics.present = true,
        rt.metrics.version = "v1",
        input.value = field::Empty,
        input.mime_type = field::Empty,
        output.value = field::Empty,
        output.mime_type = field::Empty,
        status = field::Empty,
        error.type = field::Empty,
        error.message = field::Empty,
    )
}

pub(crate) fn oi_chain_incident_evidence_retrieval_metrics_span(
    parent: &tracing::Span,
) -> tracing::Span {
    tracing::info_span!(
        parent: parent,
        "oi.chain.incident_evidence_retrieval_metrics",
        openinference.span.kind = "CHAIN",
        rt.metrics.present = true,
        rt.metrics.version = "v1",
        input.value = field::Empty,
        input.mime_type = field::Empty,
        output.value = field::Empty,
        output.mime_type = field::Empty,
        status = field::Empty,
        error.type = field::Empty,
        error.message = field::Empty,
    )
}

pub(crate) fn oi_chain_theory_evidence_retrieval_metrics_span(
    parent: &tracing::Span,
) -> tracing::Span {
    tracing::info_span!(
        parent: parent,
        "oi.chain.theory_evidence_retrieval_metrics",
        openinference.span.kind = "CHAIN",
        rt.metrics.present = true,
        rt.metrics.version = "v1",
        input.value = field::Empty,
        input.mime_type = field::Empty,
        output.value = field::Empty,
        output.mime_type = field::Empty,
        status = field::Empty,
        error.type = field::Empty,
        error.message = field::Empty,
    )
}

pub(crate) fn oi_chain_prompt_context_assembly_span(parent: &tracing::Span) -> tracing::Span {
    tracing::info_span!(
        parent: parent,
        "oi.chain.prompt_context_assembly",
        openinference.span.kind = "CHAIN",
        input.value = field::Empty,
        input.mime_type = field::Empty,
        output.value = field::Empty,
        output.mime_type = field::Empty,
        status = field::Empty,
        error.type = field::Empty,
        error.message = field::Empty,
    )
}

pub(crate) fn oi_chain_diagnostic_update_prompt_context_assembly_span(
    parent: &tracing::Span,
) -> tracing::Span {
    tracing::info_span!(
        parent: parent,
        "oi.chain.diagnostic_update_prompt_context_assembly",
        openinference.span.kind = "CHAIN",
        input.value = field::Empty,
        input.mime_type = field::Empty,
        output.value = field::Empty,
        output.mime_type = field::Empty,
        status = field::Empty,
        error.type = field::Empty,
        error.message = field::Empty,
    )
}
