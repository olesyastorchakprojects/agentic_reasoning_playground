## 1) Purpose / Scope

`ollama_client` defines the concrete Ollama-compatible runtime model client.

This module:
- implements `ModelClient` for Ollama chat APIs;
- validates request shape;
- constructs the outbound HTTP JSON body;
- executes the provider call with retry behavior;
- maps the provider response into `ModelGenerationResponse`.

This module does not:
- assemble prompts from retrieval results;
- parse returned JSON text into business-domain structs;
- select itself at runtime.

Shared rules:
- `Specification/runtime/api_clients/model/model_client.md`

## 2) Shared Dependencies

This module depends on:
- `Specification/runtime/api_clients/model/shared_types.md`
- `Specification/runtime/api_clients/model/model_client.md`
- `Specification/runtime/api_clients/client_common.md`

## 3) Public Structure

The generated Rust module must define:

```rust
pub struct OllamaModelClient {
    http_client: reqwest::Client,
    config: OllamaModelClientConfig,
    retry_policy: RetryPolicyConfig,
}
```

The generated Rust module must expose:

```rust
impl OllamaModelClient {
    pub fn new(
        config: OllamaModelClientConfig,
        retry_policy: RetryPolicyConfig,
    ) -> Result<Self, ModelClientError>;
}
```

Constructor rules:
- the constructor must create and own a `reqwest::Client`;
- the constructor must validate `config.base_url`, `config.model_name`, and `config.timeout_sec`;
- `config.timeout_sec` must be applied to the constructed `reqwest::Client` as the request timeout for outbound provider calls;
- `retry_policy.max_attempts == 0` is invalid constructor config;
- created HTTP client and validated config must be held for the full lifetime of the client;
- `generate()` must not recreate the HTTP client per request.
- when a future higher-level runtime layer wires this client from crate-level settings, it must construct `OllamaModelClientConfig` and `RetryPolicyConfig` from `Settings.model.transport` before calling this constructor;
- this leaf constructor must not accept crate-level `Settings` directly.

## 4) Trait Implementation

The module must implement:

```rust
#[async_trait::async_trait]
impl ModelClient for OllamaModelClient {
    async fn generate(
        &self,
        request: &ModelGenerationRequest,
    ) -> Result<ModelGenerationResponse, ModelClientError>;
}
```

Implementation rules:
- shared request-validation rules must follow `model_client.md`;
- shared response-mapping rules must follow `model_client.md`;
- invalid request shape must fail before any outbound HTTP call;
- response validation must fail with `ModelClientError::InvalidResponse`.

## 5) External Service Usage

Base Ollama API:
- `http://localhost:11434/`

The current version must use:
- `POST /api/chat`

Required request headers:
- `Content-Type: application/json`

The request JSON body must contain:
- `model`
- `messages`
- `stream = false`

The request JSON body may contain:
- `format`
- `options`

Message mapping rules:
- each `ModelMessage` must map to one Ollama chat message object;
- `ModelMessageRole::System` maps to `"system"`;
- `ModelMessageRole::User` maps to `"user"`;
- `ModelMessageRole::Assistant` maps to `"assistant"`;
- `content` must be serialized exactly as provided.

Temperature and token-limit mapping rules:
- `temperature` must be sent under `options.temperature`;
- when `max_output_tokens = Some(value)`, it must be sent under `options.num_predict`;
- when `max_output_tokens = None`, `options.num_predict` must be omitted.

Response mode mapping rules:
- `ModelResponseMode::Text` must omit `format`;
- `ModelResponseMode::JsonObject` must send:

```json
{
  "format": "json"
}
```
- `ModelResponseMode::JsonSchema(_)` must also send:

```json
{
  "format": "json"
}
```

Rules:
- the current Ollama-compatible path does not transmit the schema object separately;
- `JsonSchema(_)` and `JsonObject` therefore share the same wire-level `format = "json"` mapping in the current version.

## 6) Full HTTP JSON Examples

### 6.1 Text response mode

Outbound request example:

```json
{
  "model": "qwen3:8b",
  "messages": [
    { "role": "system", "content": "You are a diagnostic assistant." },
    { "role": "user", "content": "The service recovered, then metadata latency spiked." }
  ],
  "stream": false,
  "options": {
    "temperature": 0.2,
    "num_predict": 400
  }
}
```

### 6.2 JSON response mode

Outbound request example:

```json
{
  "model": "qwen3:8b",
  "messages": [
    { "role": "system", "content": "Return valid JSON only." },
    { "role": "user", "content": "Summarize the likely incident class." }
  ],
  "stream": false,
  "format": "json",
  "options": {
    "temperature": 0.1,
    "num_predict": 300
  }
}
```

Expected successful response shape:

```json
{
  "message": {
    "role": "assistant",
    "content": "{\"incident_class\":\"recovery_amplification\"}"
  },
  "done_reason": "stop",
  "prompt_eval_count": 120,
  "eval_count": 32
}
```

## 7) Response Validation Rules

Rules:
- the provider response must be a JSON object;
- the response must contain `message`;
- if `message.role` is present, it is transport metadata only and must be ignored by response validation and mapping;
- `message.content` must be a string;
- empty assistant content after trimming is an invalid response;
- if `prompt_eval_count` is present, it must be numeric and non-negative;
- if `eval_count` is present, it must be numeric and non-negative;
- when both `prompt_eval_count` and `eval_count` are present, `total_tokens` must equal their sum;
- extra top-level fields and extra message fields must be ignored.

Finish-reason mapping rules:
- `"stop"` -> `ModelFinishReason::Stop`
- `"length"` -> `ModelFinishReason::Length`
- any other string -> `ModelFinishReason::Unknown(...)`

Token-usage mapping rules:
- `prompt_eval_count` maps to `prompt_tokens`;
- `eval_count` maps to `completion_tokens`;
- when both are present, `total_tokens = Some(prompt_eval_count + eval_count)`;
- when only one is present, `total_tokens` must remain `None`.

## 8) Constraints / Non-Goals

This module must not:
- parse assistant content into domain-specific JSON structs;
- mutate or reorder validated messages;
- expose raw Ollama wire structs at module boundary.
