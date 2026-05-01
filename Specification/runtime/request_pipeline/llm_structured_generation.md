## 1) Purpose / Scope

This document defines the runtime leaf-module contract for
`llm_structured_generation`.

This module exists to:
- accept a rendered prompt inside the shared prompt-context input type;
- call the shared model-client trait with JSON-object response mode enabled;
- parse the model response content as strict JSON;
- return the parsed JSON object plus model-call execution metadata.

This module does not:
- assemble prompt context from cards, chunks, or query state;
- load prompt assets;
- know the domain schema of the diagnostic response beyond requiring one JSON
  object;
- validate business rules for the diagnostic response;
- normalize the response into the trusted final response shape;
- choose the active model provider.

Business-schema validation and trusted response normalization belong to the
downstream `response_validation_and_normalization` module.

## 2) Shared Dependencies

This module depends on:
- `Specification/runtime/api_clients/model/model_client.md`
- `Specification/runtime/api_clients/model/shared_types.md`
- `Specification/runtime/runtime.md`
- `Specification/runtime/observability/open_inference_spans.md`

Required shared runtime input type:
- `PromptContextAssemblyOutput`

Required shared runtime output type:
- `LlmStructuredGenerationOutput`

Required shared runtime metadata type:
- `ModelTokenUsage`

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

- `src/request_pipeline/llm_structured_generation.rs`

Parent module exposure:
- `src/request_pipeline/mod.rs` must expose `llm_structured_generation`.

## 4) Shared Output And Settings Types

The generated Rust crate must define the shared output type in
`src/shared_types.rs`, equivalent in ownership to:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct LlmStructuredGenerationOutput {
    pub response_json: serde_json::Value,
    pub token_usage: ModelTokenUsage,
}
```

Output rules:
- `LlmStructuredGenerationOutput` is the shared cross-module output of
  `llm_structured_generation`;
- `response_json` must contain the parsed assistant response JSON;
- `response_json` must be a JSON object in the current version;
- `response_json` must preserve all fields returned by the model exactly as
  represented by `serde_json`;
- this module must not map `response_json` into a domain-specific diagnostic
  response struct;
- this module must not drop unknown JSON fields after successful object parsing;
- `token_usage` must contain model token-usage metadata and must not be merged
  into `response_json`.

Shared-type placement rules:
- `LlmStructuredGenerationOutput` must be imported by this module from
  `crate::shared_types`;
- downstream modules and the pipeline orchestrator must not import
  `LlmStructuredGenerationOutput` from
  `crate::request_pipeline::llm_structured_generation`;
- before code generation for this module, `PromptContextAssemblyOutput`,
  `LlmStructuredGenerationOutput`, and `ModelTokenUsage` must exist in
  `src/shared_types.rs`.

The generated Rust settings model must define a module settings slice equivalent
in ownership to:

```rust
pub struct LlmStructuredGenerationSettings {
    pub max_output_tokens: u32,
}
```

Settings rules:
- `LlmStructuredGenerationSettings` is the single typed settings slice used by
  this module;
- this module must receive the typed settings slice rather than reading raw TOML
  values;
- `max_output_tokens` must be greater than zero.

Crate-level settings integration:
- `Settings` must contain:

```rust
pub struct Settings {
    // existing fields omitted
    pub llm_structured_generation: LlmStructuredGenerationSettings,
}
```

Runtime config ownership:
- runtime TOML must contain a dedicated section named
  `[llm_structured_generation]`;
- `[llm_structured_generation]` must contain exactly the current module-owned
  field:
  - `max_output_tokens`;
- `Settings.llm_structured_generation.max_output_tokens` must be loaded from
  `runtime.toml [llm_structured_generation].max_output_tokens`;
- the shipped default `Execution/distributed_diagnostics/runtime.toml` must set:

```toml
[llm_structured_generation]
max_output_tokens = 1200
```

Config-loading rules:
- missing `[llm_structured_generation]` must cause config loading to fail before
  runtime request processing begins;
- missing `max_output_tokens` must cause config loading to fail before runtime
  request processing begins;
- `max_output_tokens = 0` must cause config loading or module construction to
  fail before the model is called;
- this settings slice must not contain prompt asset paths, schema paths,
  provider selection, temperature, or retry configuration.

## 5) Public Interface

The generated Rust module must define a public module boundary equivalent in
ownership to:

```rust
pub struct LlmStructuredGeneration {
    // implementation-owned fields
}

impl LlmStructuredGeneration {
    pub fn new(
        settings: LlmStructuredGenerationSettings,
        model_client: std::sync::Arc<dyn ModelClient>,
    ) -> Result<Self, LlmStructuredGenerationError>;

    pub async fn generate(
        &self,
        prompt_context: &PromptContextAssemblyOutput,
    ) -> Result<LlmStructuredGenerationOutput, LlmStructuredGenerationError>;

    pub async fn generate_with_context(
        &self,
        prompt_context: &PromptContextAssemblyOutput,
        context: &Context,
    ) -> Result<LlmStructuredGenerationOutput, LlmStructuredGenerationError>;
}
```

For the current version, implementation-owned fields must contain exactly:
- `model_client: Arc<dyn ModelClient>`
- `max_output_tokens: u32`

Rules:
- `new(...)` must validate constructor settings once;
- `generate(...)` must delegate to
  `generate_with_context(prompt_context, &Context::noop())`;
- `generate_with_context(...)` is the context-aware request-time entrypoint used
  by the orchestrator;
- `generate_with_context(...)` must treat
  `context.open_inference.root_span` as the parent span for the module-owned
  OpenInference LLM span `oi.llm.diagnostic_response`;
- `generate_with_context(...)` must call the shared `ModelClient`
  asynchronously;
- this module must store the model client behind `Arc<dyn ModelClient>`;
- the current version must not require callers to pass raw model settings per
  request;
- provider selection remains outside this module and belongs to upstream runtime
  wiring.

## 6) Constructor Rules

`LlmStructuredGeneration::new(settings, model_client)` must:
- validate that `settings.max_output_tokens > 0`;
- retain `settings.max_output_tokens` for reuse;
- retain the supplied model client for reuse.

Constructor failure caused by invalid settings must surface through this
module's typed error boundary.

## 7) Input Rules

`generate(prompt_context)` must use:
- `prompt_context.prompt` as the model input text.

Input validation rules:
- `prompt_context.prompt` must be non-empty after trimming;
- if `prompt_context.prompt` is empty after trimming, the module must fail before
  calling the model client;
- `incident_evidence_chunks` and `theory_chunks` are traceability fields and
  must not be serialized into a second prompt by this module;
- this module must not modify, summarize, trim, or re-render
  `prompt_context.prompt` before passing it to the model client.

## 8) Model Call Rules

This module must call the shared `ModelClient` trait defined by:
- `Specification/runtime/api_clients/model/model_client.md`

The current call must use:
- exactly one model message:
  - `role = ModelMessageRole::User`
  - `content = prompt_context.prompt.clone()`
- `response_mode = ModelResponseMode::JsonObject`
- `temperature = 0.0`
- `max_output_tokens = Some(settings.max_output_tokens)`

Rules:
- the module must not add an extra hidden system message in the current version;
- the module must not accept per-request temperature overrides;
- the module must not accept per-request output-token overrides;
- the module must use the configured `max_output_tokens` value on every request;
- the module must rely on the supplied prompt text to describe the domain JSON
  contract;
- the OpenInference input/output payload and LLM metadata contract for
  `generate_with_context(...)` is owned by
  `Specification/runtime/observability/open_inference_spans.md`;
- the module must not construct provider-specific request objects.

## 9) Stop-Reason Rules

The module must inspect the model finish reason before accepting the response as
successful output.

Processing order:
1. map `ModelGenerationResponse.prompt_tokens`, `completion_tokens`, and
   `total_tokens` into `ModelTokenUsage`;
2. inspect `ModelGenerationResponse.finish_reason`;
3. only when the finish reason is acceptable, parse
   `ModelGenerationResponse.content` as JSON.

Rules:
- the module must read the reason from
  `ModelGenerationResponse.finish_reason`;
- `finish_reason = Some(ModelFinishReason::Stop)` is acceptable;
- `finish_reason = Some(ModelFinishReason::Length)` must be treated as token
  limit truncation;
- when `finish_reason = Some(ModelFinishReason::Length)`, the module must fail
  with `InvalidModelOutput` and must not attempt to salvage partial JSON;
- for `finish_reason = Some(ModelFinishReason::Length)`, the module must
  preserve token-usage metadata from the same `ModelGenerationResponse` but must
  not parse `ModelGenerationResponse.content`;
- for any unacceptable non-`Stop` finish reason, the module must preserve
  token-usage metadata from the same `ModelGenerationResponse` but must not parse
  `ModelGenerationResponse.content`;
- if `finish_reason` is present and is anything other than `Stop`, the current
  version must treat the response as `InvalidModelOutput`;
- if a provider path omits `finish_reason`, the module may accept the response
  only if the content is fully parseable valid JSON and the parsed top-level
  value is a JSON object.

## 10) Output Parsing Rules

The module must treat `ModelGenerationResponse.content` as strict JSON.

Rules:
- the response content must parse with `serde_json`;
- the parsed top-level value must be a JSON object;
- JSON arrays, strings, numbers, booleans, and null are invalid top-level
  outputs for this module;
- Markdown code fences, prose wrappers, and multiple JSON values are invalid
  outputs;
- incomplete or truncated JSON must fail with `InvalidModelOutput`;
- the module must not attempt JSON repair;
- the module must not retry with a corrective prompt in the current version;
- the module must not validate required diagnostic-response fields in the
  current version;
- the module must not apply business-rule normalization to the parsed JSON.

Successful output mapping rules:
- `ModelGenerationResponse.content` parsed as JSON maps to
  `LlmStructuredGenerationOutput.response_json`;
- `ModelGenerationResponse.prompt_tokens` maps to
  `LlmStructuredGenerationOutput.token_usage.prompt_tokens`;
- `ModelGenerationResponse.completion_tokens` maps to
  `LlmStructuredGenerationOutput.token_usage.completion_tokens`;
- `ModelGenerationResponse.total_tokens` maps to
  `LlmStructuredGenerationOutput.token_usage.total_tokens`;
- token-usage values must be preserved exactly as returned by `ModelClient`
  without recomputation by this module.

## 11) Error Boundary

This module must define a module-owned direct error type equivalent in ownership
to:

```rust
#[derive(Debug, thiserror::Error)]
pub enum LlmStructuredGenerationError {
    #[error("invalid config: {0}")]
    InvalidConfig(&'static str),
    #[error("invalid input: {0}")]
    InvalidInput(&'static str),
    #[error(transparent)]
    Model(#[from] ModelClientError),
    #[error("invalid model output: {reason}")]
    InvalidModelOutput {
        reason: &'static str,
        token_usage: ModelTokenUsage,
        finish_reason: Option<ModelFinishReason>,
    },
}
```

Variant rules:
- `InvalidConfig`
  - covers invalid constructor settings such as `max_output_tokens = 0`;
- `InvalidInput`
  - covers invalid module input such as an empty rendered prompt;
- `Model(ModelClientError)`
  - wraps failures returned by the shared model client;
- `InvalidModelOutput`
  - covers JSON parse failures, invalid top-level JSON shape, empty model
    content, or unusable stop reasons returned by the model;
  - must preserve any available token-usage metadata from the model response;
  - must preserve the parsed `finish_reason` value when it was available from
    the model response.

Rules:
- this module must not flatten model-client failures into string-only results;
- this module must wrap model-client failure through one typed dependency
  variant;
- `InvalidModelOutput.token_usage` must use the same `ModelTokenUsage` shape as
  successful output;
- `InvalidModelOutput.finish_reason` must reflect
  `ModelGenerationResponse.finish_reason` when available;
- this module must not expose raw serde or model-client transport error types
  through its public boundary.

## 12) Behavioral Invariants

Rules:
- identical input prompt text, identical settings, and identical model response
  must produce identical `LlmStructuredGenerationOutput` output;
- the current version must always request JSON-object output from the model
  client;
- the module must be side-effect-free except for the model-client call;
- the module must not read files, query databases, or access Qdrant;
- the module must not log raw prompt text or raw model output unless a future
  observability contract explicitly permits it.

## 13) Out Of Scope

The following are explicitly out of scope for this module:
- prompt asset loading;
- prompt-context construction;
- response-schema ownership;
- diagnostic-response business validation;
- response normalization into trusted final user-facing types;
- model-output repair;
- provider selection;
- request orchestration across pipeline modules.
