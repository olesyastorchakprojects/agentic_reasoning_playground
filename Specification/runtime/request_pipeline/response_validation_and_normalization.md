## 1) Purpose / Scope

This document defines the runtime leaf-module contract for
`response_validation_and_normalization`.

This module exists to:
- accept an untrusted model-generated JSON object through the shared generation
  output type;
- validate that the JSON matches the current diagnostic-response contract;
- enforce module-owned business rules that cannot be represented reliably by
  type shape alone;
- normalize the accepted JSON into the trusted final response type.

This module does not:
- call the model;
- repair invalid model output;
- load prompt assets;
- assemble prompt context;
- inspect retrieval evidence, cards, or original user input;
- choose provider-specific validation behavior.

The current version validates the diagnostic response shape represented by the
prompt contract, but the module input remains only the shared generation output
type.

## 2) Shared Dependencies

This module depends on:
- `Specification/runtime/runtime.md`
- `Specification/runtime/observability/open_inference_spans.md`

Required shared runtime input type:
- `LlmStructuredGenerationOutput`

Required shared runtime output types:
- `ResponseValidationAndNormalizationOutput`
- `DiagnosticResponse`
- `DiagnosticResultInterpretation`

## 3) Generated Rust Artifact

The generated Rust crate must include:

- `src/request_pipeline/response_validation_and_normalization.rs`

Parent module exposure:
- `src/request_pipeline/mod.rs` must expose `response_validation_and_normalization`.

## 4) Shared Output Types

The generated Rust runtime must define shared types equivalent in ownership to:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ResponseValidationAndNormalizationOutput {
    pub response: DiagnosticResponse,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiagnosticResponse {
    pub problem_understanding: String,
    pub similar_practical_context: String,
    pub active_hypotheses: Vec<String>,
    pub first_check: String,
    pub result_interpretation: DiagnosticResultInterpretation,
    pub competing_interpretation: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiagnosticResultInterpretation {
    pub supports_primary_if: String,
    pub supports_competing_if: String,
    pub inconclusive_if: Option<String>,
}
```

Type ownership rules:
- these are trusted cross-module response types and must be defined in
  `src/shared_types.rs`;
- the module must deserialize the untrusted JSON into these types only after
  shape validation succeeds;
- `ResponseValidationAndNormalizationOutput` must contain the trusted response
  only, not raw model JSON;
- token-usage metadata remains owned by the upstream model-generation output and
  must not be merged into the trusted response type.

## 5) Public Interface

The generated Rust module must define a public module boundary equivalent in
ownership to:

```rust
#[derive(Debug)]
pub struct ResponseValidationAndNormalization;

impl ResponseValidationAndNormalization {
    pub fn new() -> Self;

    pub fn validate_and_normalize(
        &self,
        input: &LlmStructuredGenerationOutput,
    ) -> Result<ResponseValidationAndNormalizationOutput, ResponseValidationAndNormalizationError>;

    pub fn validate_and_normalize_with_context(
        &self,
        input: &LlmStructuredGenerationOutput,
        context: &Context,
    ) -> Result<ResponseValidationAndNormalizationOutput, ResponseValidationAndNormalizationError>;
}
```

Rules:
- the current version has no module settings;
- `new()` must not read files or environment variables;
- `validate_and_normalize(...)` must delegate to
  `validate_and_normalize_with_context(input, &Context::noop())`;
- `validate_and_normalize_with_context(...)` is the context-aware request-time
  entrypoint used by the orchestrator;
- `validate_and_normalize_with_context(...)` must treat
  `context.open_inference.root_span` as the parent span for the module-owned
  OpenInference guardrail span `oi.guardrail.response_validation`;
- `validate_and_normalize_with_context(...)` must be deterministic and
  side-effect-free.

## 6) Input Rules

`validate_and_normalize(input)` must use:
- `input.response_json` as the untrusted response JSON.

Rules:
- the module must not inspect `input.token_usage` for validation decisions;
- the module must not mutate `input.response_json`;
- the module must not accept any alternate raw JSON input in the current version;
- the OpenInference input/output payload contract for
  `validate_and_normalize_with_context(...)` is owned by
  `Specification/runtime/observability/open_inference_spans.md`.

## 7) Shape Validation Rules

The accepted top-level JSON object must contain exactly these fields:
- `problem_understanding`
- `similar_practical_context`
- `active_hypotheses`
- `first_check`
- `result_interpretation`
- `competing_interpretation`

Rules:
- all required fields must be present;
- unknown top-level fields must fail validation;
- `problem_understanding` must be a string;
- `similar_practical_context` must be a string;
- `active_hypotheses` must be an array of strings;
- `first_check` must be a string;
- `result_interpretation` must be an object;
- `competing_interpretation` must be either a string or null.

The nested `result_interpretation` object must contain exactly these fields:
- `supports_primary_if`
- `supports_competing_if`
- `inconclusive_if`

Nested rules:
- all nested required fields must be present;
- unknown nested fields must fail validation;
- `supports_primary_if` must be a string;
- `supports_competing_if` must be a string;
- `inconclusive_if` must be either a string or null.

Implementation rules:
- generated code must use structured JSON deserialization and validation such as
  `serde` / `serde_json`;
- generated code must not use ad hoc string scanning to validate JSON fields;
- generated code must reject unknown fields rather than silently ignoring them.

## 8) Business Rules

The module must enforce these MVP business rules after shape validation:

- all string fields must be non-empty after trimming;
- string values must be normalized by trimming leading and trailing whitespace;
- `active_hypotheses` items that become empty after trimming must be dropped
  before hypothesis-count validation;
- `active_hypotheses.len()` must be `2` or `3`;
- `first_check` must be one compact next check, not an empty value;
- `competing_interpretation = Some(value)` must contain a non-empty trimmed
  string;
- `inconclusive_if = Some(value)` must contain a non-empty trimmed string.

Prohibited final-diagnosis language:
- `problem_understanding`, `similar_practical_context`, `first_check`,
  `supports_primary_if`, `supports_competing_if`, `inconclusive_if`, and
  `competing_interpretation` must not contain any of these case-insensitive
  phrases:
  - `confirms the root cause`
  - `proves the diagnosis`
  - `definitive root cause`

Rules:
- when a nullable string field is present and contains only whitespace,
  validation must fail;
- when a required string field contains only whitespace, validation must fail;
- when the remaining trimmed active hypotheses violate the `2..=3` rule after
  dropping whitespace-only items, validation must fail;
- the module must not rewrite wording beyond leading/trailing whitespace
  trimming in the current version;
- the module must not synthesize missing hypotheses or missing interpretations.

## 9) Normalization Rules

Successful normalization must produce:
- `ResponseValidationAndNormalizationOutput.response`

Field mapping rules:
- `response.problem_understanding` <- trimmed
  `response_json.problem_understanding`;
- `response.similar_practical_context` <- trimmed
  `response_json.similar_practical_context`;
- `response.active_hypotheses` <- each trimmed item from
  `response_json.active_hypotheses` in original order, excluding items that
  become empty after trimming;
- `response.first_check` <- trimmed `response_json.first_check`;
- `response.result_interpretation.supports_primary_if` <- trimmed nested value;
- `response.result_interpretation.supports_competing_if` <- trimmed nested value;
- `response.result_interpretation.inconclusive_if` <- `None` or trimmed nested
  value;
- `response.competing_interpretation` <- `None` or trimmed top-level value.

Rules:
- output must preserve the model-returned ordering of `active_hypotheses`;
- output must not sort, deduplicate, summarize, or rephrase hypotheses;
- output must not include raw `serde_json::Value`;
- output must not include model token usage in the current version.

## 10) Error Boundary

This module must define a module-owned direct error type equivalent in ownership
to:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ResponseValidationAndNormalizationError {
    #[error("invalid response shape: {0}")]
    InvalidResponseShape(&'static str),
    #[error("business rule violation: {0}")]
    BusinessRuleViolation(&'static str),
}
```

Variant rules:
- `InvalidResponseShape`
  - covers missing required fields, unknown fields, wrong field types, invalid
    nested shape, or a top-level value that is not an object;
- `BusinessRuleViolation`
  - covers empty required strings, invalid `active_hypotheses` length, empty
    hypothesis strings, non-empty nullable fields that normalize to empty, or
    prohibited final-diagnosis language.

Rules:
- `ResponseValidationAndNormalizationError` must derive `Debug` and
  `thiserror::Error`;
- every error variant must define an explicit `#[error("...")]` message;
- this module must not expose raw serde errors through its public boundary;
- this module must not flatten shape failures and business-rule failures into
  one catch-all string-only result;
- error reason strings must be stable enough for unit tests.

## 11) Behavioral Invariants

Rules:
- identical `LlmStructuredGenerationOutput.response_json` must produce identical
  `ResponseValidationAndNormalizationOutput` or identical validation failure;
- validation must not depend on model token usage;
- validation must not depend on provider finish reason;
- the module must be side-effect-free;
- the module must not read files, query databases, call Qdrant, or call a model.

## 12) Out Of Scope

The following are explicitly out of scope for this module:
- model-output repair;
- schema loading from prompt assets;
- evidence-aware validation;
- localization or style rewriting;
- final response rendering for a UI or CLI;
- observability event emission beyond any future crate-level logging contract.
