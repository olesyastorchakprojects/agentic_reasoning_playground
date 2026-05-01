## 1) Purpose

This document defines the configuration contract for the new diagnostics eval
engine crate.

The eval config is the canonical file-based source for:

- judge runtime settings;
- storage connectivity;
- suite selection;
- artifact output settings;
- offline batch-eval behavior defaults.

The config must be stable enough for reproducible eval runs and explicit enough
to support resume and report auditability.

## 2) Configuration Boundary

The eval engine must use one dedicated config file rather than reusing the
runtime's `runtime.toml` directly.

Reasoning:

- the eval crate has different concerns from the runtime crate;
- eval settings should evolve independently of request-execution settings;
- the same runtime outputs may later be evaluated by different judge settings
  without mutating runtime configuration.

The eval config may still reference runtime-owned assets such as:

- suite catalog paths;
- prompt files;
- report artifact roots;
- shared database connection sources.

## 3) Required Top-Level Sections

The current MVP config should contain at least:

- `[eval]`
- `[judge]`
- `[postgres]`
- `[artifacts]`
- `[suites]`
- `[observability]`

Optional sections may be added later, but these are the current contract
minimum.

## 4) `[eval]` Section

The `[eval]` section defines batch-level engine behavior.

It must contain at least:

- `config_version`
- `run_type`

It may also contain:

- `mode`
- `resume_eval_run_id`
- `batch_label`
- `max_runtime_runs_per_new_eval_run`

### 4.1) `config_version`

- required
- string
- used only for config evolution and explicit versioning

### 4.2) `run_type`

- required
- string
- identifies the semantic batch type for the eval run, such as:
  - `golden_dataset`
  - `offline_validation`
  - `local_dev_eval`

### 4.3) `mode`

Recommended MVP default:

- `batch_golden`

This field exists to leave room for later modes such as:

- `single_run`
- `adhoc_run_id_list`

## 5) `[judge]` Section

The `[judge]` section defines model transport and pricing inputs for judge
calls.

It must contain at least:

- `provider`
- `model_name`
- `tokenizer_source`
- `input_cost_per_million_tokens`
- `output_cost_per_million_tokens`

It should also contain one provider-specific subsection for the active
transport.

This mirrors the useful parts of the previous eval engine config while staying
small for MVP.

### 5.1) Supported MVP Providers

The config should support at least:

- `ollama`
- `together`

### 5.2) Provider-Specific Subsections

Recommended pattern:

```toml
[judge]
provider = "together"
model_name = "openai/gpt-oss-20b"
tokenizer_source = "Qwen/Qwen2.5-1.5B-Instruct"
input_cost_per_million_tokens = 0.05
output_cost_per_million_tokens = 0.20

[judge.together]
base_url = "https://api.together.xyz"
timeout_sec = 120
retry_max_attempts = 3
retry_backoff = "exponential"
```

For `ollama`, the subsection should similarly include:

- `base_url`
- `timeout_sec`
- retry settings

### 5.3) Secrets

Provider secrets such as API keys must not be committed into the TOML file.

They should be sourced from environment variables and resolved during config
loading.

The config contract must document which environment variables are required for
which providers.

## 6) `[postgres]` Section

The `[postgres]` section defines the eval engine's database connection source.

It must contain at least:

- `url_env`

It may optionally allow:

- `url`

Recommended rule:

- prefer environment-driven URL resolution for local and CI safety;
- allow explicit `url` only for controlled local development if desired.

Recommended shape:

```toml
[postgres]
url_env = "POSTGRES_URL"
```

## 7) `[artifacts]` Section

The `[artifacts]` section defines where eval-run artifacts are written.

It must contain at least:

- `root_dir`

Recommended MVP default:

- `Evidence/evals/runs`

It may also contain:

- `write_markdown_report = true`
- `write_manifest = true`

The current MVP expects both manifest and markdown report output.

## 8) `[suites]` Section

The `[suites]` section defines suite-catalog loading and enablement.

It must contain at least:

- `catalog_path`

It may also contain:

- `enabled`
- `required_for_mvp_only`

Recommended shape:

```toml
[suites]
catalog_path = "Specification/evals/prompts.json"
required_for_mvp_only = true
enabled = [
  "query_structuring_field_boundary_correctness",
  "query_structuring_grounding_conservatism",
  "evidence_pack_role_fit",
  "evidence_pack_sufficiency",
  "final_no_root_cause_claim",
  "final_first_check_discriminates",
  "final_hypothesis_source_alignment",
  "final_alternative_context_handling",
  "final_result_interpretation_usefulness",
]
```

If `required_for_mvp_only = true`, the implementation may validate that the
enabled set matches the required suite set from the catalog.

## 9) `[observability]` Section

The eval engine should have its own observability subsection even if it reuses
shared observability infrastructure patterns.

It should contain at least:

- `tracing_enabled`
- `metrics_enabled`

It may also contain:

- `service_name`
- `trace_batch_scheduled_delay_ms`
- `metrics_export_interval_ms`

The intent is to keep eval-engine observability configurable without coupling
it to runtime request-execution settings.

## 10) Configuration Loading Rules

The loader must:

1. read the TOML file;
2. resolve environment-backed secrets and connection values;
3. validate provider-specific required fields;
4. validate suite-catalog path existence;
5. return a fully resolved typed settings struct.

Configuration loading must fail early if:

- the provider is unknown;
- a required environment variable is missing;
- the suite catalog path is invalid;
- a required section is missing.

## 11) Example MVP Config

```toml
[eval]
config_version = "v1"
run_type = "golden_dataset"
mode = "batch_golden"

[judge]
provider = "together"
model_name = "openai/gpt-oss-20b"
tokenizer_source = "Qwen/Qwen2.5-1.5B-Instruct"
input_cost_per_million_tokens = 0.05
output_cost_per_million_tokens = 0.20

[judge.together]
base_url = "https://api.together.xyz"
timeout_sec = 120
retry_max_attempts = 3
retry_backoff = "exponential"

[postgres]
url_env = "POSTGRES_URL"

[artifacts]
root_dir = "Evidence/evals/runs"
write_manifest = true
write_markdown_report = true

[suites]
catalog_path = "Specification/evals/prompts.json"
required_for_mvp_only = true

[observability]
tracing_enabled = true
metrics_enabled = true
service_name = "distributed_diagnostics_eval"
```

