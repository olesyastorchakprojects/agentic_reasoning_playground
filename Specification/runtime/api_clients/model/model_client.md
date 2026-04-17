## 1) Purpose / Scope

This document defines the shared runtime model-client interface.

This module:
- exposes the domain-facing trait used by higher runtime layers;
- accepts already-assembled chat messages;
- delegates HTTP execution to a concrete provider implementation;
- returns a provider-neutral `ModelGenerationResponse`.

This module does not:
- build prompts from cards or chunks;
- parse model output into business-domain JSON structures;
- select the active provider implementation;
- expose raw provider wire objects at module boundary.

## 2) Shared Dependencies

This module depends on:
- `Specification/runtime/api_clients/model/shared_types.md`
- `Specification/runtime/api_clients/client_common.md`

Retry-config rule:
- `RetryPolicyConfig` is a typed retry configuration value reused by concrete model clients;
- its Rust shape is defined in `Specification/runtime/api_clients/model/shared_types.md`;
- its behavior contract is defined by `Specification/runtime/api_clients/client_common.md`;
- this model-client slice must reuse that shared retry config type rather than defining a duplicate model-specific retry struct.

## 3) Public Trait

The generated Rust module must define:

```rust
#[async_trait::async_trait]
pub trait ModelClient {
    async fn generate(
        &self,
        request: &ModelGenerationRequest,
    ) -> Result<ModelGenerationResponse, ModelClientError>;
}
```

Trait rules:
- callers pass fully assembled `messages`;
- callers do not pass raw HTTP payloads;
- callers do not pass provider-specific model settings per request;
- all concrete implementations must expose the same public `generate(...)` shape.

## 4) Error Model

The generated Rust module must define:

```rust
pub enum ModelClientError {
    InvalidRequest(&'static str),
    Transport(String),
    UnexpectedStatus(u16),
    InvalidResponse(&'static str),
}
```

Rules:
- `InvalidRequest` covers invalid request shape and invalid constructor config;
- `Transport` covers HTTP transport failures and retry exhaustion;
- `UnexpectedStatus` covers non-2xx provider responses;
- `InvalidResponse` covers invalid provider JSON shape and missing required response fields;
- concrete implementations must map provider-specific failures into these variants;
- concrete implementations must not flatten all failures into a single string-only error.

## 5) Shared Request Validation Rules

Concrete implementations must validate incoming request objects immediately on entry to `generate()`.

Rules:
- `messages` must not be empty;
- every message content must be non-empty after trimming;
- `temperature` must be finite and non-negative;
- when `max_output_tokens = Some(value)`, `value` must be greater than zero;
- invalid request shape must fail before any outbound HTTP call;
- implementations must use message contents exactly as provided;
- implementations must not semantically rewrite, trim, or reorder messages before sending the provider request.

## 6) Shared Response Mapping Rules

Rules:
- concrete implementations must map provider output into `ModelGenerationResponse`;
- `content` must be taken from the provider's assistant output text exactly as returned;
- if the provider returns token usage, the implementation must map it into `prompt_tokens`, `completion_tokens`, and `total_tokens`;
- if the provider omits token usage, the corresponding fields must remain `None`;
- if the provider returns a finish reason, the implementation must map it into `ModelFinishReason`;
- provider-specific unknown finish reasons must map to `ModelFinishReason::Unknown(...)`;
- extra response fields that are not used by the mapping must be ignored.

## 7) Retry Rules

Retry behavior must follow:
- `Specification/runtime/api_clients/client_common.md`

Rules:
- retry is implementation-owned;
- retry settings are passed to the constructor and held for the full client lifetime;
- request validation must happen before retry-capable HTTP execution begins.
