## 1) Purpose / Scope

This document defines the shared embedding-service contract used by runtime API client modules that need to convert a user query into an embedding.

This document defines:
- embedding service request/response shape;
- constructor inputs for embedding-capable modules;
- request execution rules;
- response validation rules.

This document does not define:
- sparse-vector construction;
- collection-specific payload mapping;
- Qdrant request execution.

## 2) Shared Configuration

Embedding-capable runtime modules must use:
- `EmbeddingConfig`
- `RetryPolicyConfig`

Source of truth:
- `Specification/runtime/api_clients/qdrant/shared_types.md`

## 3) Public Structure

The module must expose a concrete public structure:

```rust
pub struct EmbeddingClient {
    http_client: reqwest::Client,
    config: EmbeddingConfig,
    retry_policy: RetryPolicyConfig,
}
```

The module must expose a public constructor:

```rust
impl EmbeddingClient {
    pub fn new(
        config: EmbeddingConfig,
        retry_policy: RetryPolicyConfig,
    ) -> Result<Self, EmbeddingClientError>;
}
```

The module must expose one public async method:

```rust
impl EmbeddingClient {
    pub async fn embed(
        &self,
        query: &NormalizedUserQuery,
    ) -> Result<Embedding, EmbeddingClientError>;
}
```

Constructor rules:
- the constructor must create and own a `reqwest::Client`;
- the constructor must reject invalid configuration through `EmbeddingClientError`;
- `config.base_url` must use `http` or `https`;
- `config.base_url` must contain a host;
- `config.base_url` must not contain query parameters;
- `config.base_url` must not contain a fragment;
- `config.embedding_dimension` must be greater than zero;
- `retry_policy.max_attempts` must be greater than zero;

## 4) External Service Usage

Embedding service request shape:

```json
{
  "model": "embedding-model-name",
  "input": ["user query text"]
}
```

Embedding service response shape:

```json
{
  "embeddings": [
    [0.1, 0.2, 0.3]
  ]
}
```

Embedding request rules:
- the embedding service base URL must be `EmbeddingConfig.base_url`;
- the client must call `POST /api/embed`;
- request `model` must be `EmbeddingConfig.model_name`;
- request `input` must contain exactly one string;
- that input string must be the unchanged `NormalizedUserQuery.0` value supplied by the caller.

## 5) Response Validation Rules

The embedding-capable module must validate:
- HTTP status is `2xx`;
- response body parses as JSON object;
- `embeddings` exists and is an array;
- `embeddings` contains exactly one embedding;
- the returned embedding length equals `EmbeddingConfig.embedding_dimension`.

Validation rules:
- missing `embeddings` is an invalid response;
- zero embeddings is an invalid response;
- more than one embedding is an invalid response;
- wrong embedding length is an invalid response;
- invalid float values in the returned embedding are an invalid response.

## 6) Retry Behavior

Retry behavior must follow:
- `Specification/runtime/api_clients/client_common.md`

## 7) Error Model

The module must define:

```rust
pub enum EmbeddingClientError {
    InvalidRequest(&'static str),
    Transport(String),
    UnexpectedStatus(u16),
    InvalidResponse(&'static str),
}
```

Failure-category rules:
- `InvalidRequest` covers invalid client configuration or invalid outbound request shape;
- `Transport` covers network, timeout, and low-level HTTP execution failures;
- `UnexpectedStatus` covers non-success HTTP status codes returned by the embedding service;
- `InvalidResponse` covers malformed or incomplete embedding-service response bodies.
