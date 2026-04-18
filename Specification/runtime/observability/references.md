# 1) Purpose / Scope

This document defines the local observability reference artifacts used during generation.

The reference artifacts in `references/` are read-only templates.

Generation rules:
- generation reads local reference artifacts from this directory;
- generation adapts local reference artifacts for this repository;
- generation does not invent a new layout instead of using the local references.
- generation must treat these files as wiring and provisioning references only;
- the required runtime dashboard set, filenames, panel composition, and queries are defined in `grafana_dashboards.md`, not in `references/`.

# 2) Reference Artifact Set

The required local reference set is:
- `references/todo_app_docker_compose_observability.yml`
- `references/todo_app_otel_collector_config.yaml`
- `references/todo_app_prometheus.yml`
- `references/todo_app_tempo.yaml`
- `references/todo_app_grafana_dashboards.yml`
- `references/todo_app_grafana_prometheus_datasource.yml`
- `references/todo_app_grafana_tempo_datasource.yml`

# 3) Usage Rules

Reference artifacts are:
- templates;
- local sources of truth for wiring patterns;
- not runtime artifacts of `rag_runtime`.
- references do not define the required runtime dashboard inventory or panel layout.

Generated project artifacts belong under `Measurement/observability/`.
