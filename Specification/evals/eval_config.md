## 1) Purpose

This document defines the current configuration contract for the diagnostics
eval engine crate.

The eval config is the canonical file-based source for:

- judge runtime settings;
- storage connectivity;
- suite selection;
- artifact output settings;
- offline batch-eval behavior defaults;
- eval-engine observability settings.

## 2) Configuration Boundary

The eval engine uses one dedicated config file rather than reusing the runtime's
`runtime.toml` directly.

The config loader is also responsible for:

- resolving env-backed secrets and endpoints;
- applying CLI overrides;
- resolving relative paths against the config file directory;
- validating that required fields are non-empty.

## 3) Required Top-Level Sections

The current config contains at least:

- `[eval]`
- `[judge]`
- `[postgres]`
- `[artifacts]`
- `[suites]`
- `[observability]`

## 4) `[eval]` Section

The `[eval]` section defines batch-level engine behavior.

Required fields:

- `config_version`
- `run_type`

Optional fields:

- `mode`
- `resume_eval_run_id`
- `batch_label`
- `max_runtime_runs_per_new_eval_run`

### 4.1) `config_version`

- required string;
- used for explicit config evolution.

### 4.2) `run_type`

- required non-empty string;
- identifies the semantic batch type for the eval run.

### 4.3) `mode`

Current default:

- `batch_golden`

This field remains forward-compatible with later modes, but the current code
still operates as a batch eval engine.

### 4.4) `max_runtime_runs_per_new_eval_run`

- optional integer;
- limits new-run bootstrap discovery;
- may be overridden by CLI `--limit`.

## 5) `[judge]` Section

The `[judge]` section defines model transport and pricing inputs for judge
calls.

Required fields:

- `provider`
- `model_name`
- `tokenizer_source`
- `input_cost_per_million_tokens`
- `output_cost_per_million_tokens`

### 5.1) Supported Provider Set

Current implementation supports exactly:

- `together`

If another provider name is supplied, config loading must fail early.

### 5.2) `[judge.together]`

The active provider-specific subsection is:

```toml
[judge]
provider = "together"
model_name = "openai/gpt-oss-20b"
tokenizer_source = "Qwen/Qwen2.5-1.5B-Instruct"
input_cost_per_million_tokens = 0.05
output_cost_per_million_tokens = 0.20

[judge.together]
base_url = "https://api.together.xyz"
api_key_env = "TOGETHER_API_KEY"
timeout_sec = 120
retry_max_attempts = 3
retry_backoff = "exponential"
```

Required semantic behavior:

- `api_key` must be resolved from `api_key_env`;
- `retry_max_attempts` defaults to `3`;
- `retry_backoff` defaults to `exponential`.

## 6) `[postgres]` Section

Required fields:

- `url_env`

Optional fields:

- `url`

Resolution rules:

- if explicit `url` is present, it wins;
- otherwise the loader resolves `url_env`;
- missing env vars must fail config loading.

## 7) `[artifacts]` Section

Required fields:

- `root_dir`

Optional fields:

- `write_manifest`
- `write_markdown_report`

Current defaults:

- `write_manifest = true`
- `write_markdown_report = true`

Path semantics:

- artifact paths are resolved relative to the config file directory unless
  overridden by CLI.

## 8) `[suites]` Section

Required fields:

- `catalog_path`

Optional fields:

- `enabled`
- `required_for_mvp_only`

Resolution rules:

- `catalog_path` is resolved relative to the config file directory;
- the path must exist or config loading fails;
- `enabled` may be overridden by repeatable CLI `--enabled-suite`;
- empty suite names are invalid.

## 9) `[observability]` Section

Current required field set:

- `tracing_enabled`

Current optional fields:

- `service_name`
- `tracing_endpoint`
- `tracing_endpoint_env`

Current defaults:

- `service_name = "distributed_diagnostics_eval"`
- `tracing_endpoint_env = "TRACING_ENDPOINT"`
- if neither explicit endpoint nor env var is present, fallback endpoint is
  `http://localhost:4317`

The current config contract does not include a separate metrics subsection.

## 10) Loader Behavior

The loader must:

1. read the TOML file;
2. load `.env` when present;
3. resolve CLI overrides;
4. resolve relative paths against the config file directory;
5. resolve env-backed secrets and connection values;
6. validate provider-specific required fields;
7. validate non-empty strings for required fields;
8. validate suite-catalog path existence;
9. return a fully resolved typed settings struct.

## 11) Effective Precedence

The effective precedence order is:

1. explicit CLI overrides;
2. config-file values;
3. documented defaults;
4. env-backed fallback resolution where explicitly defined by field contract.

For resume, this precedence still applies to operational settings, while frozen
eval-run identity and subject scope remain artifact-owned rather than
config-owned.
