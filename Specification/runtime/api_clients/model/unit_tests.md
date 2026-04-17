## 1) Purpose / Scope

This document defines the mandatory generated unit-test contract for the runtime model-client slice.

This document is the single source of truth for:
- required module-level unit-test cases for runtime model-client modules;
- required request-shape assertions for provider-specific clients;
- required response-mapping and validation tests.

This document must be read together with:
- `Specification/runtime/unit_tests_common.md`

## 2) Covered Modules

The current runtime model-client test scope covers:

- `model.openai_client`
- `model.ollama_client`

## 3) Required Unit Tests By Module

### 3.1) `model.openai_client`

Generated unit tests for `OpenAiModelClient` must include all of the following cases:

- constructor fails when `base_url` has invalid runtime shape according to the spec;
- constructor fails when `api_key` is empty;
- constructor fails when `model_name` is empty;
- constructor fails when `timeout_sec == 0`;
- constructor fails when `retry_policy.max_attempts == 0`;
- constructor applies `timeout_sec` to the outbound HTTP client configuration;
- empty `messages` fails with the exact `ModelClientError::InvalidRequest` variant before any HTTP call is sent;
- empty message content fails with the exact `ModelClientError::InvalidRequest` variant before any HTTP call is sent;
- invalid `temperature` fails with the exact `ModelClientError::InvalidRequest` variant before any HTTP call is sent;
- invalid `max_output_tokens` fails with the exact `ModelClientError::InvalidRequest` variant before any HTTP call is sent;
- outbound request body contains exactly the configured `model_name`;
- outbound request body preserves message order;
- outbound request body preserves message content exactly;
- when `max_output_tokens = None`, outbound request omits `max_tokens`;
- `ModelResponseMode::Text` omits `response_format`;
- `ModelResponseMode::JsonObject` sends the exact `response_format = { \"type\": \"json_object\" }` shape;
- when response `choices` contains more than one element, the implementation uses the first choice as the canonical answer;
- response `message.role` is ignored during response validation and mapping;
- successful response maps `message.content` into `ModelGenerationResponse.content`;
- successful response maps OpenAI usage fields into token counts;
- successful response maps known finish reasons correctly;
- unknown finish reason maps to `ModelFinishReason::Unknown(...)`;
- HTTP or network transport failure returns the exact `ModelClientError::Transport` variant;
- non-2xx HTTP response returns the exact `ModelClientError::UnexpectedStatus` variant;
- response without `choices` returns the exact `ModelClientError::InvalidResponse` variant;
- response without `message.content` returns the exact `ModelClientError::InvalidResponse` variant;
- response with empty assistant content returns the exact `ModelClientError::InvalidResponse` variant.

### 3.2) `model.ollama_client`

Generated unit tests for `OllamaModelClient` must include all of the following cases:

- constructor fails when `base_url` has invalid runtime shape according to the spec;
- constructor fails when `model_name` is empty;
- constructor fails when `timeout_sec == 0`;
- constructor fails when `retry_policy.max_attempts == 0`;
- constructor applies `timeout_sec` to the outbound HTTP client configuration;
- empty `messages` fails with the exact `ModelClientError::InvalidRequest` variant before any HTTP call is sent;
- empty message content fails with the exact `ModelClientError::InvalidRequest` variant before any HTTP call is sent;
- invalid `temperature` fails with the exact `ModelClientError::InvalidRequest` variant before any HTTP call is sent;
- invalid `max_output_tokens` fails with the exact `ModelClientError::InvalidRequest` variant before any HTTP call is sent;
- outbound request body contains exactly the configured `model_name`;
- outbound request body preserves message order;
- outbound request body preserves message content exactly;
- outbound request body always sends `stream = false`;
- `temperature` is encoded under `options.temperature`;
- when `max_output_tokens = Some(value)`, it is encoded under `options.num_predict`;
- `ModelResponseMode::Text` omits `format`;
- `ModelResponseMode::JsonObject` sends `format = \"json\"`;
- response `message.role` is ignored during response validation and mapping;
- successful response maps `message.content` into `ModelGenerationResponse.content`;
- successful response maps `prompt_eval_count` and `eval_count` into token counts;
- successful response maps known finish reasons correctly;
- unknown finish reason maps to `ModelFinishReason::Unknown(...)`;
- HTTP or network transport failure returns the exact `ModelClientError::Transport` variant;
- non-2xx HTTP response returns the exact `ModelClientError::UnexpectedStatus` variant;
- response without `message` returns the exact `ModelClientError::InvalidResponse` variant;
- response without `message.content` returns the exact `ModelClientError::InvalidResponse` variant;
- response with empty assistant content returns the exact `ModelClientError::InvalidResponse` variant.
