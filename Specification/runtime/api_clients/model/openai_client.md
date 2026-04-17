## 1) Purpose / Scope

`openai_client` defines the concrete OpenAI-compatible runtime model client.

This module:
- implements `ModelClient` for OpenAI-compatible chat-completions APIs;
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
pub struct OpenAiModelClient {
    http_client: reqwest::Client,
    config: OpenAiModelClientConfig,
    retry_policy: RetryPolicyConfig,
}
```

The generated Rust module must expose:

```rust
impl OpenAiModelClient {
    pub fn new(
        config: OpenAiModelClientConfig,
        retry_policy: RetryPolicyConfig,
    ) -> Result<Self, ModelClientError>;
}
```

Constructor rules:
- the constructor must create and own a `reqwest::Client`;
- the constructor must validate `config.base_url`, `config.api_key`, `config.model_name`, and `config.timeout_sec`;
- `config.timeout_sec` must be applied to the constructed `reqwest::Client` as the request timeout for outbound provider calls;
- `retry_policy.max_attempts == 0` is invalid constructor config;
- created HTTP client and validated config must be held for the full lifetime of the client;
- `generate()` must not recreate the HTTP client per request.

## 4) Trait Implementation

The module must implement:

```rust
#[async_trait::async_trait]
impl ModelClient for OpenAiModelClient {
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

Base OpenAI-compatible API:
- `https://api.openai.com/`

The current version must use:
- `POST /v1/chat/completions`

Required request headers:
- `Authorization: Bearer <api_key>`
- `Content-Type: application/json`

The request JSON body must contain:
- `model`
- `messages`
- `temperature`

The request JSON body may contain:
- `max_tokens`
- `response_format`

Message mapping rules:
- each `ModelMessage` must map to one OpenAI chat message object;
- `ModelMessageRole::System` maps to `"system"`;
- `ModelMessageRole::User` maps to `"user"`;
- `ModelMessageRole::Assistant` maps to `"assistant"`;
- `content` must be serialized exactly as provided.

Response mode mapping rules:
- `ModelResponseMode::Text` must omit `response_format`;
- `ModelResponseMode::JsonObject` must send:

```json
{
  "response_format": {
    "type": "json_object"
  }
}
```

Max-output-token mapping rule:
- when `max_output_tokens = Some(value)`, the request must send `max_tokens = value`;
- when `max_output_tokens = None`, the request must omit `max_tokens`.

## 6) Full HTTP JSON Examples

### 6.1 Text response mode

Outbound request example:

```json
{
  "model": "gpt-4o-mini",
  "messages": [
    { "role": "system", "content": "You are a diagnostic assistant." },
    { "role": "user", "content": "The service recovered, then metadata latency spiked." }
  ],
  "temperature": 0.2,
  "max_tokens": 400
}
```

### 6.2 JSON response mode

Outbound request example:

```json
{
  "model": "gpt-4o-mini",
  "messages": [
    { "role": "system", "content": "Return valid JSON only." },
    { "role": "user", "content": "Summarize the likely incident class." }
  ],
  "temperature": 0.1,
  "max_tokens": 300,
  "response_format": {
    "type": "json_object"
  }
}
```

Expected successful response shape:

```json
{
  "choices": [
    {
      "message": {
        "role": "assistant",
        "content": "{\"incident_class\":\"recovery_amplification\"}"
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 120,
    "completion_tokens": 32,
    "total_tokens": 152
  }
}
```

## 7) Response Validation Rules

Rules:
- the provider response must be a JSON object;
- the response must contain `choices` as a non-empty array;
- `choices` is the OpenAI-compatible list of completion candidates returned for one request;
- the implementation must use `choices[0]` as the canonical assistant answer and ignore any later choices;
- the first choice must contain `message`;
- if `message.role` is present, it is transport metadata only and must be ignored by response validation and mapping;
- the first choice message must contain `content` as a string;
- empty assistant content after trimming is an invalid response;
- if `usage` is present, token counts must be numeric and non-negative;
- if `finish_reason` is present, it must be mapped to `ModelFinishReason`;
- extra top-level fields and extra choice fields must be ignored.

Finish-reason mapping rules:
- `"stop"` -> `ModelFinishReason::Stop`
- `"length"` -> `ModelFinishReason::Length`
- `"content_filter"` -> `ModelFinishReason::ContentFilter`
- `"tool_calls"` -> `ModelFinishReason::ToolCalls`
- any other string -> `ModelFinishReason::Unknown(...)`

## 8) Constraints / Non-Goals

This module must not:
- parse assistant content into domain-specific JSON structs;
- mutate or reorder validated messages;
- expose raw OpenAI wire structs at module boundary.
