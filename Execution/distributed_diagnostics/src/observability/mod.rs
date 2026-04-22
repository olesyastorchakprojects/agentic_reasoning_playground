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
        .with_resource(build_resource("rag_runtime"))
        .build();

    let tracer = provider.tracer("rag_runtime".to_string());
    let telemetry_layer = tracing_opentelemetry::OpenTelemetryLayer::new(tracer);
    let subscriber = Registry::default()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
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
        .with_resource(build_resource("rag_runtime"))
        .build();

    global::set_meter_provider(provider.clone());

    Ok(MetricsGuard { provider })
}
