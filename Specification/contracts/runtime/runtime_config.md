# Runtime Config Contract

This document defines the contract for:
- `Execution/distributed_diagnostics/runtime.toml`

## Format

`runtime.toml` is a TOML runtime config file.

It is the source of truth for:
- runtime-owned retrieval behavior;
- request-level input normalization settings;
- request-level query structuring asset paths;
- request-level prompt context prompt asset path and chunk-packing policy;
- request-level LLM structured-generation model-call limit;
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
- `max_alternatives`: maximum number of non-primary alternative cards returned by candidate-card retrieval

`[retrieval.cards.embedding_retry]`
- `max_attempts`
- `backoff`

`[retrieval.cards.qdrant_retry]`
- `max_attempts`
- `backoff`

`[retrieval.practice]`
- `top_k`
- `score_threshold`
- `max_alternatives`

`[retrieval.practice.embedding_retry]`
- `max_attempts`
- `backoff`

`[retrieval.practice.qdrant_retry]`
- `max_attempts`
- `backoff`

`[retrieval.theory]`
- `top_k`
- `score_threshold`
- `max_alternatives`

`[retrieval.theory.embedding_retry]`
- `max_attempts`
- `backoff`

`[retrieval.theory.qdrant_retry]`
- `max_attempts`
- `backoff`

`[input_normalization]`
- `max_input_tokens`
- `tokenizer_source`

`[query_structuring]`
- `controlled_vocabulary_path`
- `prompt_asset_path`
- `max_output_tokens`

`[prompt_context]`
- `prompt_asset_path`

`[prompt_context.chunk_packing.evidence_for_match]`
- `source`
- `limit`
- `fallback_to_any_chunk`
- `tag_priority`

`[prompt_context.chunk_packing.first_check_hint]`
- `source`
- `limit`
- `fallback_to_any_chunk`
- `tag_priority`

`[prompt_context.chunk_packing.supporting_explanation]`
- `source`
- `limit`
- `fallback_to_any_chunk`
- `tag_priority`

`[prompt_context.chunk_packing.alternative_context]`
- `source`
- `limit`
- `per_case_limit` when `limit > 0`
- `fallback_to_any_chunk`
- `tag_priority`

`[prompt_context.chunk_packing.mechanism_explanation]`
- `source`
- `limit`
- `fallback_to_any_chunk`
- `tag_priority`

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

`retrieval.<collection>.max_alternatives`
- maximum number of non-primary alternative cards retained after top-ranked candidate selection;
- this is runtime behavior;
- it must be greater than or equal to zero.

`input_normalization.max_input_tokens`
- maximum accepted token count for normalized user input;
- this is an operational runtime ceiling, not the tokenizer or model's theoretical maximum context length.

`input_normalization.tokenizer_source`
- Hugging Face tokenizer repository identifier used for runtime token counting;
- the tokenizer utility resolves this identifier according to `Specification/runtime/utils/tokenizer.md`.

`query_structuring.controlled_vocabulary_path`
- path to the prebuilt controlled-vocabulary JSON asset used by the `query_structuring` module;
- this runtime module reads the file contents from disk during module construction;
- the asset is runtime input and is not rebuilt from PostgreSQL by this module.

`query_structuring.prompt_asset_path`
- path to the JSON prompt asset used by the `query_structuring` module;
- the prompt asset must contain the versioned system prompt and user-template content consumed by prompt assembly.

`query_structuring.max_output_tokens`
- output token ceiling used by the `query_structuring` model call;
- this value is runtime-owned and applies uniformly to this module's requests;
- it must be greater than zero.

`prompt_context.prompt_asset_path`
- path to the JSON prompt asset used by the `prompt_context_assembly` module;
- the prompt asset schema must live next to the configured asset as defined by `Specification/runtime/request_pipeline/prompt_context_assembly.md`.

`prompt_context.chunk_packing.<role>.source`
- source pool used for the prompt evidence role;
- allowed values are:
  - `primary_incident`
  - `alternative_incident`
  - `theory`

`prompt_context.chunk_packing.<role>.limit`
- maximum number of chunks selected for the role;
- `evidence_for_match.limit` must be greater than or equal to `1`;
- `first_check_hint.limit` must be greater than or equal to `1`;
- `supporting_explanation.limit` is `1` in the default shipped runtime config;
- `supporting_explanation.limit` may be `0` only when explicitly disabled;
- `alternative_context.limit` may be `0`;
- `mechanism_explanation.limit` may be `0`.

`prompt_context.chunk_packing.<role>.tag_priority`
- ordered list of full canonical incident chunk tags used by role selection;
- values must use full strings such as `chunk_role:symptom`;
- short aliases such as `symptom` are invalid.

`prompt_context.chunk_packing.<role>.fallback_to_any_chunk`
- enables fallback selection from the role's source pool after all configured tag matches.

`prompt_context.chunk_packing.alternative_context.per_case_limit`
- maximum number of alternative chunks selected per hydrated alternative card;
- must be greater than zero when `alternative_context.limit > 0`;
- may be absent or any non-negative integer when `alternative_context.limit = 0` because alternative context selection is disabled.

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
- request-level query structuring asset locations and model-call limit such as `controlled_vocabulary_path`, `prompt_asset_path`, and `max_output_tokens`;
- request-level prompt-context asset location and chunk-packing policy;
- request-level LLM structured-generation model-call limit;
- runtime retrieval knobs such as `top_k`, `score_threshold`, and `max_alternatives`;
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
