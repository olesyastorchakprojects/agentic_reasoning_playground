## 1) Purpose

This document defines the module contract for the eval crate's `config`
module.

This module is the typed configuration boundary for the eval engine.

## 2) Responsibilities

The config module owns:

- eval config structs;
- TOML loading;
- environment-variable resolution for secrets and connection strings;
- provider-specific validation;
- suite-catalog path validation;
- CLI override application where the implementation chooses to centralize it.

## 3) Non-Responsibilities

The config module must not own:

- orchestration lifecycle logic;
- SQL writes;
- report rendering;
- judge prompt execution.

## 4) Public Types

The module should expose at least:

- `EvalSettings`
- `EvalConfigError`
- `JudgeSettings`
- `PostgresSettings`
- `ArtifactsSettings`
- `SuitesSettings`
- `ObservabilitySettings`

It may also expose smaller provider-specific settings types, such as:

- `OllamaJudgeSettings`
- `TogetherJudgeSettings`

## 5) Public Interfaces

The main entrypoint should be conceptually equivalent to:

```rust
fn load_eval_settings(
    config_path: &Path,
    cli_overrides: &EvalCliOverrides,
) -> Result<EvalSettings, EvalConfigError>
```

The implementation may internally split loading into:

- raw TOML deserialization
- env resolution
- semantic validation
- override application

## 6) Inputs

The config module consumes:

- one eval TOML path
- environment variables
- optional CLI override values

It should not need database access or runtime state access.

## 7) Outputs

The config module outputs one fully resolved typed settings value that is safe
for the rest of the crate to consume without repeated stringly-typed checks.

That resolved settings object should already include:

- effective judge provider/model settings
- effective postgres connection source
- effective artifact root
- effective enabled suite set

## 8) Dependency Rules

The config module should sit near the bottom of the dependency graph.

It may be used by:

- `main.rs`
- `orchestrator`
- `storage`
- `judge`
- `observability`

It should not depend on those modules in return.

## 9) Validation Ownership

The config module is the owner of startup validation for:

- missing required environment variables
- unknown judge provider
- invalid suite-catalog path
- missing required top-level config sections
- invalid retry / timeout / pricing values

These errors should fail early before the orchestrator starts mutating eval
state.

