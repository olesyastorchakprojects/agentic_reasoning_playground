## 1) Purpose / Scope

This document defines the runtime leaf-module contract for
`observation_extraction`.

This module exists to:
- accept the output of `observation_boundary_resolver`;
- require that the boundary result is `Supported(...)` before extraction proceeds;
- extract one or more atomic diagnostic observations from the resolved standalone observation text;
- assess whether the resolved observation is sufficiently specific for downstream diagnostic update;
- return a strict typed extraction result containing extracted observations plus adequacy-assessment signals;
- generate a minimal set of follow-up questions when more context is required.

This module does not:
- decide whether the original continuation input is supported or unsupported;
- resolve ambiguous references against `DiagnosticContext`;
- update hypotheses;
- decide what the extracted observations mean diagnostically;
- retrieve evidence;
- assemble the final diagnostic-update prompt;
- choose the active model provider or construct provider-specific clients.

This document is the source of truth for:
- the `observation_extraction` leaf-module boundary;
- the module public interface;
- the shared runtime output types produced by this module;
- the prompt asset contract consumed by this module;
- the model JSON contract returned by this module;
- the module-owned business rules for adequacy assessment and follow-up questions;
- the module-owned error boundary.

The input observation boundary behavior is defined by:
- `Specification/runtime/request_pipeline/observation_boundary_resolver.md`

Shared runtime and settings types are defined by:
- `Specification/runtime/runtime.md`

OpenInference span behavior for the context-aware execution path is defined by:
- `Specification/runtime/observability/open_inference_spans.md`

The generated Rust module file for the current version is:
- `src/request_pipeline/observation_extraction.rs`

## 2) Shared Dependencies

This module depends on:
- `Specification/runtime/api_clients/model/model_client.md`
- `Specification/runtime/runtime.md`
- `Specification/runtime/request_pipeline/observation_boundary_resolver.md`
- `Specification/runtime/observability/open_inference_spans.md`

Required shared runtime input types:
- `ObservationBoundaryResolverOutput`
- `ObservationBoundaryResolution`
- `ResolvedObservation`
- `Context`

Required shared runtime output types:
- `ObservationExtractionOutput`
- `ExtractedObservation`
- `ObservationPolarity`
- `Confidence`
- `ModelTokenUsage`

Required model-client types:
- `ModelClient`
- `ModelClientError`
- `ModelGenerationRequest`
- `ModelGenerationResponse`
- `ModelMessage`
- `ModelMessageRole`
- `ModelResponseMode`
- `ModelFinishReason`

## 3) Generated Rust Artifact

The generated Rust crate must include:

- `src/request_pipeline/observation_extraction.rs`

Parent module exposure:
- `src/request_pipeline/mod.rs` must expose `observation_extraction`.

## 4) Shared Output And Settings Types

The shared output types used by this module are defined in `src/shared_types/mod.rs`
by:
- `Specification/runtime/runtime.md`

Shared-type rules:
- `ObservationExtractionOutput` is the trusted cross-module output of `observation_extraction`;
- `normalized_user_input` must preserve `ObservationBoundaryResolverOutput.normalized_user_input` exactly;
- `resolved_observation` must preserve the exact `ResolvedObservation` returned by `ObservationBoundaryResolverOutput.resolution`;
- `ObservationExtractionOutput` must include an overall module-level `confidence: Confidence` field representing confidence in the adequacy assessment and extraction result as a whole;
- `observations` contains zero or more extracted atomic observations;
- each `ExtractedObservation` must include `confidence: Confidence` representing confidence in that extracted atomic observation;
- `needs_more_context` is the module-owned adequacy signal for whether the resolved observation is sufficiently specific for downstream diagnostic update;
- `missing_context_questions` contains the minimal set of follow-up questions produced by this module when more context is required;
- `token_usage` must contain the shared `ModelTokenUsage` metadata for this model call;
- `token_usage` is transport-level execution metadata and must not be part of the model-returned JSON contract;
- `confidence` fields must use the shared `Confidence` enum defined by `Specification/runtime/runtime.md`;
- `ObservationPolarity::Present` means the extracted fact happened or is true;
- `ObservationPolarity::Absent` means the extracted fact did not happen or was not observed;
- `ObservationPolarity::Corrected` means the user corrected an earlier fact.

Shared-type placement rules:
- downstream modules and the orchestrator must import them from `crate::shared_types`;
- this module must not redefine these output types locally.

The generated Rust settings model must define a module settings slice equivalent
in ownership to:

```rust
pub struct ObservationExtractionSettings {
    pub prompt_asset_path: String,
    pub max_output_tokens: u32,
}
```

Settings rules:
- `ObservationExtractionSettings` is the single typed constructor settings slice used by this module;
- this module must not receive provider selection or model-name fields in its constructor settings;
- `prompt_asset_path` must be a non-empty absolute path to the JSON prompt asset;
- `max_output_tokens` must be greater than zero.

Runtime-settings integration:
- startup/wiring code must read `Settings.observation_extraction` from crate-level config;
- startup/wiring code must use `provider` and `model` directly from `Settings.observation_extraction` to choose and construct the correct `Arc<dyn ModelClient>`;
- when `provider = "ollama"`, wiring must use URL, timeout, and retry from `[model.ollama]`, and must pass `Settings.observation_extraction.model` as the `model_name` of a module-specific `OllamaModelClient`;
- when `provider = "together"`, wiring must use URL, API key, timeout, and retry from `[model.together]`, and must pass `Settings.observation_extraction.model` as the `model_name` of a module-specific `TogetherModelClient`;
- if the selected `provider` does not match the active `Settings.model.transport` variant, startup must fail before runtime request processing begins;
- after client selection, wiring code may derive `ObservationExtractionSettings` using only `prompt_asset_path` and `max_output_tokens`;
- `provider` and `model` belong to the runtime-owned `Settings.observation_extraction` slice and must not be read from `ObservationExtractionSettings`.

## 5) Public Interface

The generated Rust module must define a public module boundary equivalent in
ownership to:

```rust
pub struct ObservationExtraction {
    // implementation-owned fields
}

impl ObservationExtraction {
    pub fn new(
        settings: ObservationExtractionSettings,
        model_client: std::sync::Arc<dyn ModelClient>,
    ) -> Result<Self, ObservationExtractionError>;

    pub async fn extract(
        &self,
        input: &ObservationBoundaryResolverOutput,
    ) -> Result<ObservationExtractionOutput, ObservationExtractionError>;

    pub async fn extract_with_context(
        &self,
        input: &ObservationBoundaryResolverOutput,
        context: &Context,
    ) -> Result<ObservationExtractionOutput, ObservationExtractionError>;
}
```

For the current version, implementation-owned fields must contain exactly:
- `model_client: Arc<dyn ModelClient>`
- `prompt_asset: ObservationExtractionPromptAsset`
- `max_output_tokens: u32`

Rules:
- `new(...)` must validate constructor settings once;
- `extract(...)` must delegate to `extract_with_context(..., &Context::noop())`;
- `extract_with_context(...)` is the context-aware request-time entrypoint used by the orchestrator;
- `extract_with_context(...)` must treat `context.open_inference.root_span` as the parent span for the module-owned OpenInference LLM span `oi.llm.observation_extraction`;
- `src/observability/mod.rs` must define a helper factory function named `oi_llm_observation_extraction_span(parent: &tracing::Span) -> tracing::Span` for this module-owned span;
- this module must store the model client behind `Arc<dyn ModelClient>`;
- provider selection remains outside this module and belongs to upstream runtime wiring.

## 6) Prompt Asset Rules

The prompt must be stored as a JSON asset rather than hardcoded as one Rust
string constant.

The current prompt JSON shape is:

```json
{
  "version": "string",
  "system_prompt": "string",
  "user_template": "string",
  "response_schema": {}
}
```

The generated runtime prompt asset type must be equivalent in ownership to:

```rust
struct ObservationExtractionPromptAsset {
    pub version: String,
    pub system_prompt: String,
    pub user_template: String,
    pub response_schema: serde_json::Value,
}
```

Rules:
- `ObservationExtractionPromptAsset` is module-private;
- the prompt asset must be loaded from `ObservationExtractionSettings.prompt_asset_path`;
- the schema path must be derived from `prompt_asset_path` by replacing `.manual_test.json` with `.schema.json`, or `.json` with `.schema.json` otherwise;
- constructor failure caused by unreadable files, invalid JSON, or schema-validation failure must surface through the module's typed error boundary;
- `system_prompt` must be non-empty;
- `user_template` must be non-empty;
- `response_schema` must be present and must be a JSON object;
- `user_template` must contain exactly this placeholder:
  - `{{user_message}}`
- the required placeholder must appear exactly once;
- any additional `{{...}}` placeholder-like construct in `user_template` is invalid.

## 7) Input Rules

`extract_with_context(input, context)` must accept:
- `ObservationBoundaryResolverOutput`

Rules:
- this module must inspect `input.resolution` before constructing the model request;
- when `input.resolution = ObservationBoundaryResolution::Unsupported`, the module must fail through its typed error boundary and must not call the model;
- when `input.resolution = ObservationBoundaryResolution::Supported(resolved)`, extraction must operate over the standalone resolved observation text rather than over the original raw user message alone;
- `normalized_user_input` in successful output must equal `input.normalized_user_input` exactly;
- `resolved_observation` in successful output must equal the `ResolvedObservation` carried by the `Supported(...)` boundary result exactly.

Extraction-source rule:
- the primary textual source for prompt assembly in this module is the resolved standalone observation text from `ObservationBoundaryResolution::Supported(ResolvedObservation)`;
- the original normalized input is preserved for traceability, but extraction semantics must follow the resolved standalone observation.

## 8) Prompt Context Rules

`extract_with_context(...)` must build the model user message by substituting the resolved standalone observation text into `{{user_message}}`.

Rules:
- prompt assembly must be deterministic;
- prompt assembly must not semantically rewrite the resolved standalone observation text before substitution;
- this module must not inject `DiagnosticContext`, hypotheses, retrieved evidence, or prior observations into the prompt in the current version;
- the module must not include unsupported-input explanations or orchestration metadata in the prompt.

## 9) Model Call Rules

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
- after a successful model call, the module must construct `ModelTokenUsage` from `ModelGenerationResponse`:
  - `prompt_tokens` <- `ModelGenerationResponse.prompt_tokens`
  - `completion_tokens` <- `ModelGenerationResponse.completion_tokens`
  - `total_tokens` <- `ModelGenerationResponse.total_tokens`
- when the model provider omits any of these token counts, the corresponding `ModelTokenUsage` fields must remain `None`;
- token-usage metadata must be preserved even when later JSON parsing or business-rule validation fails, so that invalid-model-output errors can surface the recorded token usage;
- if `finish_reason` is present and is anything other than `stop`, the current version must treat the response as invalid model output;
- `finish_reason = length` must be treated as truncation and must fail without salvage.

## 10) Model JSON Contract

The model must return exactly one JSON object with this top-level shape:

```json
{
  "confidence": "low | medium | high",
  "observations": [
    {
      "statement": "string",
      "confidence": "low | medium | high",
      "condition": "string | null",
      "polarity": "present | absent | corrected",
      "time_relation": "string | null",
      "source_span": "string"
    }
  ],
  "needs_more_context": true,
  "missing_context_questions": ["string"]
}
```

Contract rules:
- `observations` must be an array;
- `confidence` must be present at the top level;
- `needs_more_context` must be a boolean;
- `missing_context_questions` must be an array of strings;
- top-level `confidence` must be one of:
  - `low`
  - `medium`
  - `high`
- each observation object must contain exactly:
  - `statement`
  - `confidence`
  - `condition`
  - `polarity`
  - `time_relation`
  - `source_span`
- `statement` must be a string;
- `confidence` must be one of:
  - `low`
  - `medium`
  - `high`
- `condition` must be either a string or `null`;
- `polarity` must be one of:
  - `present`
  - `absent`
  - `corrected`
- `time_relation` must be either a string or `null`;
- `source_span` must be a string.

## 11) Output Parsing And Normalization Rules

The module must treat the model output as strict JSON and parse it into
`ObservationExtractionOutput`.

Field mapping rules:
- `normalized_user_input` <- `input.normalized_user_input`
- `resolved_observation` <- the `ResolvedObservation` carried by `input.resolution`
- `confidence` <- parsed top-level shared `Confidence`
- `observations` <- parsed and normalized `ExtractedObservation` values in original order
- `needs_more_context` <- parsed boolean
- `missing_context_questions` <- parsed string array in original order
- `token_usage` <- model-call token usage returned by `ModelGenerationResponse`

Token-usage mapping rules:
- `token_usage.prompt_tokens` <- `ModelGenerationResponse.prompt_tokens`
- `token_usage.completion_tokens` <- `ModelGenerationResponse.completion_tokens`
- `token_usage.total_tokens` <- `ModelGenerationResponse.total_tokens`
- `token_usage` must be attached to every successful `ObservationExtractionOutput`;
- `token_usage` must not be parsed from the model JSON object returned under `response_mode = JsonSchema(...)`.

Observation mapping rules:
- `statement` must be trimmed;
- top-level `confidence = "low"` maps to `Confidence::Low`;
- top-level `confidence = "medium"` maps to `Confidence::Medium`;
- top-level `confidence = "high"` maps to `Confidence::High`;
- observation-level `confidence = "low"` maps to `Confidence::Low`;
- observation-level `confidence = "medium"` maps to `Confidence::Medium`;
- observation-level `confidence = "high"` maps to `Confidence::High`;
- `condition = Some(value)` must be trimmed; whitespace-only strings are invalid;
- `polarity = "present"` maps to `ObservationPolarity::Present`;
- `polarity = "absent"` maps to `ObservationPolarity::Absent`;
- `polarity = "corrected"` maps to `ObservationPolarity::Corrected`;
- `time_relation = Some(value)` must be trimmed; whitespace-only strings are invalid;
- `source_span` must be trimmed before exact-substring verification and preserved in its trimmed form after shape validation.

## 12) Business Rules

The module must enforce these business rules after shape validation:

- all required string fields must be non-empty after trimming;
- nullable string fields that are present as strings must be non-empty after trimming;
- the top-level `confidence` field must contain a valid shared `Confidence` value;
- `source_span` must be a non-empty exact substring of the resolved standalone observation text used in the prompt; substring verification must be performed as `resolved_observation_text.contains(source_span.as_str())` using the trimmed `source_span` value, and validation must fail when that check is false;
- each extracted observation must contain a valid shared `Confidence` value;
- when `needs_more_context = false`, `missing_context_questions` must be empty;
- when `needs_more_context = false`, `observations` must contain at least one item;
- when `needs_more_context = true`, `missing_context_questions` must contain between `1` and `2` items inclusive;
- when `needs_more_context = true`, the returned follow-up questions must be compact disambiguating questions rather than broad exploratory prompts;
- when `needs_more_context = true`, `observations` may be empty or non-empty in the current version;
- `time_relation` must be used only when explicit ordering is stated in the resolved standalone observation text;
- `condition` should preserve explicit limiting conditions when stated, but the module must not fail only because a valid observation keeps the condition inside `statement` instead of duplicating it in `condition`.

Adequacy-semantics rules:
- `needs_more_context = true` means the message is too vague, ambiguous, or referential to produce a useful standalone diagnostic observation without clarification;
- `needs_more_context = false` means the message already contains at least one usable standalone diagnostic fact, even if broader incident investigation would still benefit from later additional data;
- this module must not set `needs_more_context = true` solely because the extracted observation does not fully distinguish between all remaining hypotheses.

## 13) Error Boundary

This module must define a module-owned direct error type equivalent in ownership
to:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ObservationExtractionError {
    #[error("invalid settings: {0}")]
    InvalidSettings(String),

    #[error("invalid prompt asset: {0}")]
    InvalidPromptAsset(String),

    #[error("unsupported boundary input cannot be extracted")]
    UnsupportedBoundaryInput,

    #[error("model client error: {0}")]
    ModelClient(#[from] ModelClientError),

    #[error("invalid model output: {reason}")]
    InvalidModelOutput {
        reason: String,
        finish_reason: Option<ModelFinishReason>,
        token_usage: ModelTokenUsage,
    },
}
```

Rules:
- `UnsupportedBoundaryInput` must be returned manually when `input.resolution = Unsupported`;
- `UnsupportedBoundaryInput` must not use `#[from]`;
- invalid JSON shape, invalid business-rule output, non-substring `source_span`, and truncation must surface as `InvalidModelOutput`;
- the module must not silently salvage or repair invalid model output in the current version.

## 14) Testing Ownership

Unit-test ownership for runtime modules is defined by:
- `Specification/runtime/unit_tests.md`
