## 1) Purpose / Scope

This document defines the shared public types for runtime model clients.

It applies to:
- `model_client.md`
- `together_client.md`
- `ollama_client.md`

This document is the source of truth for:
- request and response types at the model-client boundary;
- shared message and response-mode types;
- shared retry-config type shape for model clients;
- shared finish-reason types;
- concrete client config types;
- how crate-level model settings are converted into model-client-owned config types.

## 2) Shared Types

The generated Rust module must define:

```rust
pub enum ModelMessageRole {
    System,
    User,
    Assistant,
}

pub struct ModelMessage {
    pub role: ModelMessageRole,
    pub content: String,
}

pub enum ModelResponseMode {
    Text,
    JsonObject,
}

pub enum RetryBackoffKind {
    Exponential,
}

pub struct RetryPolicyConfig {
    pub max_attempts: u32,
    pub backoff: RetryBackoffKind,
}

pub struct ModelGenerationRequest {
    pub messages: Vec<ModelMessage>,
    pub temperature: f32,
    pub max_output_tokens: Option<u32>,
    pub response_mode: ModelResponseMode,
}

pub enum ModelFinishReason {
    Stop,
    Length,
    ContentFilter,
    ToolCalls,
    Unknown(String),
}

pub struct ModelGenerationResponse {
    pub content: String,
    pub finish_reason: Option<ModelFinishReason>,
    pub prompt_tokens: Option<usize>,
    pub completion_tokens: Option<usize>,
    pub total_tokens: Option<usize>,
}

pub struct TogetherModelClientConfig {
    pub base_url: url::Url,
    pub api_key: String,
    pub model_name: String,
    pub timeout_sec: u64,
}

pub struct OllamaModelClientConfig {
    pub base_url: url::Url,
    pub model_name: String,
    pub timeout_sec: u64,
}
```

## 3) Type Rules

Rules:
- `ModelGenerationRequest.messages` must not be empty;
- every `ModelMessage.content` must be non-empty after trimming;
- `ModelGenerationRequest.temperature` must be finite;
- negative `temperature` is an invalid request;
- `NaN`, `+inf`, and `-inf` temperature values are invalid requests;
- `RetryPolicyConfig.max_attempts` must be greater than zero;
- when `max_output_tokens = Some(value)`, `value` must be greater than zero;
- `ModelGenerationResponse.content` must be non-empty after trimming;
- when all three token counts are present, `total_tokens` must equal `prompt_tokens + completion_tokens`;
- when only some token counts are available from the provider, missing values must remain `None`;
- `TogetherModelClientConfig.base_url` and `OllamaModelClientConfig.base_url` must be valid `http` or `https` URLs;
- config URLs must contain a host;
- config URLs must not contain query parameters;
- config URLs must not contain fragments;
- `api_key` must be non-empty after trimming;
- `model_name` must be non-empty after trimming;
- `timeout_sec` must be greater than zero.

Retry-config compatibility rule:
- the Rust shape of `RetryPolicyConfig` in this file must stay aligned with the retry config shape used by Qdrant runtime API-client specs.

## 4) Ownership Rules

Rules:
- shared types in this file must be imported by concrete model-client modules, not redefined there;
- provider-specific wire request and wire response structs must remain private to concrete client modules;
- this file defines the model-client boundary types only, not raw HTTP payload shapes.

## 5) Settings Propagation Rules

The crate-level runtime settings model must not be used directly as the config type
for leaf model clients.

Leaf model clients must instead receive module-owned config types from this file.

The current crate-level source settings are the active variants of:
- `Settings.model.transport`

The full crate-level settings paths are:
- `Settings.model.transport = ModelTransportSettings::Ollama(OllamaModelSettings)`
- `Settings.model.transport = ModelTransportSettings::Together(TogetherModelSettings)`

Propagation rules:
- when `Settings.model.transport = ModelTransportSettings::Ollama(settings)`, future bootstrap or a parent runtime module may construct:
  - `OllamaModelClientConfig`
  - `RetryPolicyConfig`
- when `Settings.model.transport = ModelTransportSettings::Together(settings)`, future bootstrap or a parent runtime module may construct:
  - `TogetherModelClientConfig`
  - `RetryPolicyConfig`

Required field mappings:
- `OllamaModelClientConfig.base_url` <- `OllamaModelSettings.url`
- `OllamaModelClientConfig.model_name` <- `OllamaModelSettings.model_name`
- `OllamaModelClientConfig.timeout_sec` <- `OllamaModelSettings.timeout_sec`
- `RetryPolicyConfig` for Ollama model clients <- `OllamaModelSettings.retry`
- `TogetherModelClientConfig.base_url` <- `TogetherModelSettings.url`
- `TogetherModelClientConfig.api_key` <- `TogetherModelSettings.api_key`
- `TogetherModelClientConfig.model_name` <- `TogetherModelSettings.model_name`
- `TogetherModelClientConfig.timeout_sec` <- `TogetherModelSettings.timeout_sec`
- `RetryPolicyConfig` for Together model clients <- `TogetherModelSettings.retry`

Rules:
- leaf model clients must not depend on crate-level `Settings`;
- leaf model clients must not read environment variables directly;
- whenever a higher-level runtime module later wires concrete model clients from crate-level settings, conversion from typed settings slices into module-owned config types must happen before concrete model clients are constructed.
