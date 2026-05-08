## 1) Purpose / Scope

This document defines the runtime leaf-module contract for
`observation_boundary_resolver`.

This module exists to:
- decide whether a continuation user input can safely be treated as a new diagnostic observation;
- resolve explicit references in that input against a compact diagnostic context view;
- return a strict typed boundary-decision output for storage in `RunState`.

This module does not:
- update hypotheses;
- interpret what an observation means diagnostically;
- extract multiple atomic observations;
- mutate `DiagnosticContext`, which remains a derived projection over `RunState`;
- choose the active model provider or construct provider-specific clients.

## 2) Shared Dependencies

This module depends on:
- `Specification/runtime/api_clients/model/model_client.md`
- `Specification/runtime/runtime.md`
- `Specification/runtime/request_pipeline/diagnostic_context.md`
- `Specification/runtime/observability/open_inference_spans.md`

Required shared runtime input types:
- `DiagnosticContext`
- `NormalizedUserRequest`

Required shared runtime output types:
- `ResolvedObservation`
- `ObservationBoundaryResolution`
- `ObservationBoundaryResolverOutput`
- `Confidence`

Required model-client types:
- `ModelClient`
- `ModelClientError`
- `ModelGenerationRequest`
- `ModelMessage`
- `ModelMessageRole`
- `ModelResponseMode`
- `ModelFinishReason`

## 3) Generated Rust Artifact

The generated Rust crate must include:

- `src/request_pipeline/observation_boundary_resolver.rs`

Parent module exposure:
- `src/request_pipeline/mod.rs` must expose `observation_boundary_resolver`.

## 4) Shared Output And Settings Types

The shared output types are defined in `src/shared_types/mod.rs` by
`Specification/runtime/runtime.md`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservationBoundaryResolverOutput {
    pub normalized_user_input: String,
    pub confidence: Confidence,
    pub reason: String,
    pub resolution: ObservationBoundaryResolution,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ObservationBoundaryResolution {
    Supported(ResolvedObservation),
    Unsupported,
}
```

The generated Rust crate must define a narrow module settings type equivalent in
ownership to:

```rust
pub struct ObservationBoundaryResolverSettings {
    pub prompt_asset_path: String,
    pub max_output_tokens: u32,
}
```

Settings rules:
- `ObservationBoundaryResolverSettings` is the single typed constructor settings slice used by this module;
- this module must not receive provider selection or model-name fields in its constructor settings;
- `prompt_asset_path` must be a non-empty absolute path to the JSON prompt asset;
- `max_output_tokens` must be greater than zero.

Runtime-settings integration:
- startup/wiring code must read `Settings.observation_boundary_resolver` from crate-level config;
- startup/wiring code must use `provider` and `model` from that runtime slice to choose and construct the correct `Arc<dyn ModelClient>`;
- when `provider = "ollama"`, wiring must use URL, timeout, and retry from `[model.ollama]`, and must pass `Settings.observation_boundary_resolver.model` as the `model_name` of a module-specific `OllamaModelClient`;
- when `provider = "together"`, wiring must use URL, API key, timeout, and retry from `[model.together]`, and must pass `Settings.observation_boundary_resolver.model` as the `model_name` of a module-specific `TogetherModelClient`;
- if the selected `provider` does not match the active `Settings.model.transport` variant, startup must fail before runtime request processing begins;
- after client selection, wiring code may derive `ObservationBoundaryResolverSettings` using only `prompt_asset_path` and `max_output_tokens`.

## 5) Public Interface

The generated Rust module must define a public module boundary equivalent in
ownership to:

```rust
pub struct ObservationBoundaryResolver {
    // implementation-owned fields
}

impl ObservationBoundaryResolver {
    pub fn new(
        settings: ObservationBoundaryResolverSettings,
        model_client: std::sync::Arc<dyn ModelClient>,
    ) -> Result<Self, ObservationBoundaryResolverError>;

    pub async fn resolve(
        &self,
        request: &NormalizedUserRequest,
        diagnostic_context: &DiagnosticContext,
    ) -> Result<ObservationBoundaryResolverOutput, ObservationBoundaryResolverError>;

    pub async fn resolve_with_context(
        &self,
        request: &NormalizedUserRequest,
        diagnostic_context: &DiagnosticContext,
        context: &Context,
    ) -> Result<ObservationBoundaryResolverOutput, ObservationBoundaryResolverError>;
}
```

For the current version, implementation-owned fields must contain exactly:
- `model_client: Arc<dyn ModelClient>`
- `prompt_asset: ObservationBoundaryResolverPromptAsset`
- `max_output_tokens: u32`

Rules:
- `new(...)` must validate constructor settings once;
- `resolve(...)` must delegate to `resolve_with_context(..., &Context::noop())`;
- `resolve_with_context(...)` is the context-aware request-time entrypoint used by the orchestrator;
- `resolve_with_context(...)` must treat `context.open_inference.root_span` as the parent span for the module-owned OpenInference LLM span `oi.llm.observation_boundary_resolver`;
- this module must store the model client behind `Arc<dyn ModelClient>`;
- provider selection remains outside this module and belongs to upstream runtime wiring.

## 6) Prompt Asset Rules

The prompt must be stored as a JSON asset rather than hardcoded as one Rust
string constant.

The current prompt JSON shape is:

```json
{
  "version": "v1",
  "system_prompt": "string",
  "user_template": "string",
  "response_schema": {}
}
```

The generated runtime prompt asset type must be equivalent in ownership to:

```rust
struct ObservationBoundaryResolverPromptAsset {
    pub version: String,
    pub system_prompt: String,
    pub user_template: String,
    pub response_schema: serde_json::Value,
}
```

Rules:
- `ObservationBoundaryResolverPromptAsset` is module-private;
- the prompt asset must be loaded from `ObservationBoundaryResolverSettings.prompt_asset_path`;
- the schema path must be derived from `prompt_asset_path` by replacing `.manual_test.json` with `.schema.json`, or `.json` with `.schema.json` otherwise;
- constructor failure caused by unreadable files, invalid JSON, or schema-validation failure must surface through the module's typed error boundary;
- `system_prompt` must be non-empty;
- `user_template` must be non-empty;
- `response_schema` must be present and must be a JSON object;
- `user_template` must contain exactly these placeholders:
  - `{{diagnostic_context}}`
  - `{{normalized_user_input}}`
- each required placeholder must appear exactly once;
- any additional `{{...}}` placeholder-like construct in `user_template` is invalid.

## 7) Prompt Context Rules

`resolve_with_context(request, diagnostic_context, context)` must build a compact
prompt-facing diagnostic-context view using only `DiagnosticContext` view methods:
- `diagnostic_context.current_problem_understanding()`
- `diagnostic_context.active_hypotheses()`
- `diagnostic_context.last_check()`

The prompt-facing diagnostic-context view must contain only:
- `problem_understanding`: the current closed problem-understanding text;
- `active_hypotheses`: the current active hypotheses as a JSON array of strings using hypothesis text only;
- `latest_suggested_check`: the most recent suggested-check text.

Rules:
- `resolve_with_context(...)` must fail through the module-owned typed error boundary when `current_problem_understanding()` returns `None`;
- `resolve_with_context(...)` must fail through the module-owned typed error boundary when `last_check()` returns `None`;
- `resolve_with_context(...)` must fail through the module-owned typed error boundary when `current_problem_understanding().text` is `None`;
- for continuation iterations, `problem_understanding` must come from `diagnostic_context.current_problem_understanding()` and therefore correspond to the previous closed iteration's `ProblemUnderstanding.text`;
- the prompt-facing context must not include hypothesis ids, hypothesis statuses, confidence values, rejected hypotheses, observation history, raw `RunState`, or internal step metadata;
- prompt assembly must substitute `request.query` into `{{normalized_user_input}}`;
- prompt assembly must be deterministic and must not semantically rewrite `request.query`.

## 8) Model Call Rules

This module must call the shared `ModelClient` trait defined by:
- `Specification/runtime/api_clients/model/model_client.md`

The current call must use:
- `response_mode = ModelResponseMode::JsonSchema(prompt_asset.response_schema.clone())`
- `temperature = 0.0`
- `max_output_tokens = settings.max_output_tokens`

Rules:
- the module must build exactly two model messages:
  - one `system` message from `prompt_asset.system_prompt`;
  - one `user` message from the substituted `user_template`;
- the module must not accept per-request temperature overrides;
- the module must not accept per-request output-token overrides;
- if `finish_reason` is present and is anything other than `stop`, the current version must treat the response as invalid model output;
- `finish_reason = length` must be treated as truncation and must fail without salvage.

## 9) Model JSON Contract

The model must return exactly one JSON object with this top-level shape:

```json
{
  "supported": true,
  "confidence": "low | medium | high",
  "reason": "string",
  "full_query": "string | null"
}
```

Contract rules:
- `supported` must be a boolean;
- `confidence` must be one of `low`, `medium`, or `high`;
- `reason` must be a non-empty string;
- `full_query` must be either a string or `null`;
- `supported = true` requires `full_query` to be a non-empty string after trimming;
- `supported = false` requires `full_query = null`.

## 10) Output Parsing And Mapping Rules

The module must treat the model output as strict JSON and parse it into
`ObservationBoundaryResolverOutput`.

Mapping rules:
- `normalized_user_input` in successful output must equal `request.query` exactly;
- `confidence` must parse into the shared `Confidence` enum;
- `reason` must be preserved as returned after string-shape validation;
- `supported = true` maps into:

```rust
ObservationBoundaryResolverOutput {
    normalized_user_input: request.query.clone(),
    confidence,
    reason,
    resolution: ObservationBoundaryResolution::Supported(
        ResolvedObservation { text: full_query }
    ),
}
```

- `supported = false` maps into:

```rust
ObservationBoundaryResolverOutput {
    normalized_user_input: request.query.clone(),
    confidence,
    reason,
    resolution: ObservationBoundaryResolution::Unsupported,
}
```

Business-rule violations:
- `supported = true` with empty or null `full_query` must fail with the module-owned invalid-model-output error;
- `supported = false` with non-null `full_query` must fail with the module-owned invalid-model-output error;
- unknown `confidence` values must fail with the module-owned invalid-model-output error.

## 11) DiagnosticContext Integration Notes

This module does not itself project anything into `DiagnosticContext`.

Integration rules for downstream projection:
- `StepResultEnvelope::ObservationBoundaryResolver` stores `ObservationBoundaryResolverOutput` exactly;
- only `ObservationBoundaryResolution::Supported(...)` contributes an `Observation` entry when `DiagnosticContext::from_run_state` later builds its view;
- `ObservationBoundaryResolution::Unsupported` contributes no `Observation` entry.

## 12) Error Boundary

The generated Rust module must define a typed error boundary equivalent in
ownership to:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
pub enum ObservationBoundaryResolverError {
    InvalidSettings(String),
    InvalidPromptAsset(String),
    InvalidContext(String),
    ModelClient(ModelClientError),
    InvalidModelOutput {
        reason: String,
        finish_reason: Option<ModelFinishReason>,
        token_usage: ModelTokenUsage,
    },
}
```

Rules:
- constructor failures must not be flattened into bare strings;
- model-client failures must be wrapped through one typed dependency variant;
- invalid JSON shape, invalid `confidence`, and invalid supported/full-query combinations must fail as `InvalidModelOutput`.

## 13) Non-Goals

For the current version, this module must not:
- read raw `UserRequest` input instead of `NormalizedUserRequest`;
- persist its own results directly to storage;
- decide orchestration transitions for unsupported inputs;
- select provider-specific retry behavior;
- repair malformed model output.

## 14) Testing Ownership

Unit-test ownership for runtime modules is defined by:
- `Specification/runtime/unit_tests.md`

Required unit-test cases for this module must be defined in:
- `Specification/runtime/unit_tests.md` section `4.17) observation_boundary_resolver`
