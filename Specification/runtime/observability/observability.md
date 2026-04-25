# 1) Purpose / Scope

This document defines the top-level observability model for the runtime crate.

It defines:
- signal architecture;
- observability document structure;
- backend routing model;
- artifact placement;
- top-level safety constraints.

This document does not define:
- detailed span contracts;
- detailed metric contracts;
- infra bootstrap;
- evals.

# 2) Observability Objectives

Observability in the current runtime stage exists for exactly two purposes:
- initialize OTLP-based tracing and metrics pipelines at process startup;
- define the required orchestration trace shape for run execution.

# 3) Signal Architecture

The runtime emits exactly two required signal types:
- traces;
- metrics.

Logs are outside the required observability contract.

Signal routing is fixed:
- the runtime exports traces and metrics through OTLP;
- OTEL Collector is the single ingress point for both signal types;
- OTEL Collector routes traces to Tempo and Phoenix;
- OTEL Collector routes metrics to Prometheus;
- Grafana reads traces from Tempo and metrics from Prometheus.

Direct export from `distributed_diagnostics` to multiple backends is forbidden.

# 4) Document Structure

The observability specification is split into the following documents:

- `observability.md`
  - top-level observability model;
- `spans.md`
  - root span contract, span hierarchy, span ownership, span attributes, and root-span lifecycle rules;
- `references.md`
  - local reference artifact contract;
- `implementation.md`
  - validated Rust implementation pattern for startup-time observability initialization.

Generation must follow this document split.
Detailed metric, OpenInference, and dashboard contracts are intentionally out of scope for the current runtime stage.
`references.md` is limited to read-only local wiring/provisioning templates.

# 5) Configuration Model

Observability configuration belongs to the typed runtime configuration model.

Observability settings are read from `Settings.observability`.

Business modules:
- do not read raw environment variables for observability;
- do not define sampling behavior;
- do not override exporter routing.

# 6) Safety Constraints

The following values must not be written to telemetry during the current stage:
- secrets;
- API keys;
- authorization headers;
- environment variable values;
- raw prompt text;
- raw retrieved document text;
- raw model output text;

# 7) Troubleshooting Missing Telemetry

If expected telemetry is missing during validation, troubleshooting must start with:
- effective `TRACING_ENDPOINT`;
- effective `METRICS_ENDPOINT`;
- OTEL collector health;
- backend health for Tempo, Phoenix, and Prometheus;
- the current `.env` values used during startup.

# 8) Artifact Placement

Observability specification documents belong in:
- `Specification/runtime/observability/`

Generated observability artifacts belong in:
- `Measurement/observability/` when later runtime stages require them.
