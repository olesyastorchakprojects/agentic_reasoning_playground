# Runtime Config Contract

This document defines the contract for:
- `Execution/distributed_diagnostics/runtime.toml`

## Format

`runtime.toml` is a TOML runtime config file.

It is the source of truth for:
- runtime-owned retrieval behavior;
- request-level input normalization settings;
- active model transport selection;
- model transport runtime settings.
- observability enablement and export timing settings.

Invalid `runtime.toml`:
- `runtime.toml` is invalid if TOML does not parse, if a required section or field is missing, if a value type does not match the contract, or if a value violates the constraints of this contract;
- this is a startup error;
- runtime initialization must fail before request processing begins.

## Expected Structure

`[runtime]`
- `config_version`: version string for the current runtime config shape

`[retrieval.cards]`
- `top_k`: retrieval cut-off for cards retrieval
- `score_threshold`: minimum accepted retrieval score for cards retrieval

`[retrieval.cards.embedding_retry]`
- `max_attempts`
- `backoff`

`[retrieval.cards.qdrant_retry]`
- `max_attempts`
- `backoff`

`[retrieval.practice]`
- `top_k`
- `score_threshold`

`[retrieval.practice.embedding_retry]`
- `max_attempts`
- `backoff`

`[retrieval.practice.qdrant_retry]`
- `max_attempts`
- `backoff`

`[retrieval.theory]`
- `top_k`
- `score_threshold`

`[retrieval.theory.embedding_retry]`
- `max_attempts`
- `backoff`

`[retrieval.theory.qdrant_retry]`
- `max_attempts`
- `backoff`

`[input_normalization]`
- `max_input_tokens`
- `tokenizer_source`

`[model]`
- `transport_kind`: active model transport; allowed values:
  - `ollama`
  - `together`

`[model.ollama]`
- `model_name`
- `timeout_sec`

`[model.ollama.retry]`
- `max_attempts`
- `backoff`

`[model.together]`
- `model_name`
- `timeout_sec`

`[model.together.retry]`
- `max_attempts`
- `backoff`

`[observability]`
- `tracing_enabled`
- `metrics_enabled`
- `trace_batch_scheduled_delay_ms`
- `metrics_export_interval_ms`

## Semantics Of Config Values

`runtime.config_version`
- identifies the runtime config contract revision;
- it is config-shape versioning, not corpus versioning.

`retrieval.<collection>.top_k`
- number of candidate hits requested for the given collection;
- this is runtime behavior, not ingest compatibility.

`retrieval.<collection>.score_threshold`
- minimum accepted retrieval score for the given collection;
- this is runtime behavior, not ingest compatibility.

`input_normalization.max_input_tokens`
- maximum accepted token count for normalized user input;
- this is an operational runtime ceiling, not the tokenizer or model's theoretical maximum context length.

`input_normalization.tokenizer_source`
- Hugging Face tokenizer repository identifier used for runtime token counting;
- the tokenizer utility resolves this identifier according to `Specification/runtime/utils/tokenizer.md`.

`retrieval.<collection>.embedding_retry.backoff`
`retrieval.<collection>.qdrant_retry.backoff`
`model.<transport>.retry.backoff`
- `exponential` means retry delay must grow exponentially.

`model.transport_kind`
- selects the active model transport variant in resolved runtime settings;
- `ollama` must resolve into `ModelTransportSettings::Ollama(...)`;
- `together` must resolve into `ModelTransportSettings::Together(...)`.

`model.ollama`
- runtime-owned settings for the Ollama transport;
- the endpoint URL itself is not stored in `runtime.toml`; it is loaded from environment.

`model.together`
- runtime-owned settings for the Together transport;
- the endpoint URL and API key are not stored in `runtime.toml`; they are loaded from environment.

`observability`
- runtime-owned observability enablement flags and exporter timing settings;
- exporter endpoint URLs themselves are not stored in `runtime.toml`; they are loaded from environment.

## Ownership Rules

`runtime.toml` owns:
- runtime retrieval knobs such as `top_k` and `score_threshold`;
- request-level input normalization knobs such as `max_input_tokens` and `tokenizer_source`;
- retry settings;
- active model transport selection;
- model transport runtime settings.
- observability enablement and exporter timing settings.

`runtime.toml` must not redefine:
- Qdrant collection names;
- vector names;
- sparse strategy selection;
- sparse tokenizer settings;
- sparse preprocessing settings;
- embedding model compatibility settings.

Those fields belong to:
- `Execution/distributed_diagnostics/ingest.toml`
