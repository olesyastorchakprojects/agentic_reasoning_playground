## 1) Purpose / Scope

This document defines the crate-level generated unit-test contract for the runtime skeleton.

This document is the single source of truth for:
- required crate-level unit-test artifacts and ownership boundaries;
- crate-level completion rules for generated unit tests;
- how crate-level unit-test generation composes with child runtime test specifications.

This document must be read together with:
- `Specification/runtime/unit_tests_common.md`

Shared runtime-wide unit-test generation rules are owned by
`Specification/runtime/unit_tests_common.md`.
This document defines only the additional crate-level required unit-test cases
and crate-level ownership rules for the current runtime skeleton.

This document does not redefine:
- shared unit-test generation rules, placement rules, helper rules, or environment rules already owned by `Specification/runtime/unit_tests_common.md`;
- child API-client unit-test cases already owned by dedicated child specifications;
- future crate-level integration, smoke, or end-to-end test requirements;
- test requirements for runtime areas that are not yet part of the current skeleton.

## 2) Covered Crate-Level Modules

The current crate-level runtime unit-test scope covers:

- `errors`
- `config`
- `observability`
- `api_clients`
- `utils`
- `main`

This scope is crate-level and compositional.
Detailed required test cases for child API-client subtrees remain defined in their dedicated unit-test specifications.

## 3) Crate-Level Unit-Test Ownership

Crate-level unit-test ownership rules:

- this document owns only crate-level and cross-cutting unit-test requirements;
- detailed required unit tests for child runtime slices must remain owned by the corresponding child specifications;
- crate-level generation must not duplicate child-owned test-case lists;
- when a child runtime slice has its own dedicated unit-test specification, that child specification is the source of truth for the required unit tests of that slice.

The current child-owned unit-test specifications include:

- `Specification/runtime/api_clients/model/unit_tests.md`
- `Specification/runtime/api_clients/qdrant/unit_tests.md`
- `Specification/runtime/api_clients/postgres/unit_tests.md`

Additional child-owned unit-test specifications may be added later without changing the meaning of this document.

## 4) Required Crate-Level Unit Tests

### 4.1) `errors`

Generated unit tests for the crate-level error boundary must include all of the following cases:

- conversion from `ConfigError` into `RuntimeError` produces the exact `RuntimeError::Config(...)` variant;
- conversion from `ApiClientError` into `RuntimeError` produces the exact `RuntimeError::ApiClients(...)` variant;
- the crate-level error boundary preserves typed child errors rather than flattening them into strings.

### 4.2) `api_clients`

Generated unit tests for the crate-level `api_clients` parent boundary must include all of the following cases:

- conversion from `EmbeddingClientError` into `ApiClientError` produces the exact `ApiClientError::Embedding(...)` variant;
- conversion from `ModelApiClientError` into `ApiClientError` produces the exact `ApiClientError::Model(...)` variant;
- conversion from `QdrantApiClientError` into `ApiClientError` produces the exact `ApiClientError::Qdrant(...)` variant;
- conversion from `PostgresApiClientError` into `ApiClientError` produces the exact `ApiClientError::Postgres(...)` variant.

### 4.3) `config`

Generated unit tests for the crate-level `config` boundary must include all of the following cases:

- runtime TOML, ingest TOML, and environment values merge into one executable typed `Settings` object;
- missing required environment variables fail with the exact config-owned error category;
- `OLLAMA_URL` maps into the exact `Settings.embedding_model.url` value;
- `TRACING_ENDPOINT` maps into the exact `Settings.observability.tracing_endpoint` value;
- `METRICS_ENDPOINT` maps into the exact `Settings.observability.metrics_endpoint` value;
- `model.transport_kind = "ollama"` resolves into the exact `ModelTransportSettings::Ollama(...)` variant;
- `model.transport_kind = "together"` resolves into the exact `ModelTransportSettings::Together(...)` variant;
- `qdrant.collections.<collection>.kind = "dense"` resolves into the exact `CollectionSettings::Dense(...)` variant;
- `qdrant.collections.<collection>.kind = "hybrid"` resolves into the exact `CollectionSettings::Hybrid(...)` variant;
- `hybrid.sparse.strategy.kind = "bag_of_words"` resolves into the exact `SparseStrategySettings::BagOfWords(...)` variant;
- `hybrid.sparse.strategy.kind = "bm25_like"` resolves into the exact `SparseStrategySettings::Bm25Like(...)` variant;
- sparse artifact paths from ingest config are preserved in the resolved sparse strategy settings variant;
- unsupported transport kinds fail before runtime request processing begins;
- unsupported collection kinds fail before runtime request processing begins;
- unsupported sparse strategy kinds fail before runtime request processing begins;
- resolved collection retrieval settings preserve `top_k`, `score_threshold`, retry settings, and the typed collection variant in one `CollectionRetrievalSettings` value;
- resolved settings place `corpus_version` on the collection settings boundary rather than as one shared top-level retrieval field.

### 4.4) `utils`

The current crate-level `utils` test scope is limited.

Generated unit tests for crate-level `utils` modules must include all of the following cases:

- retry helpers reject invalid retry-policy inputs when the runtime contract requires constructor validation;
- tokenizer helper test coverage must follow `Specification/runtime/utils/tokenizer.md`;
- sparse-text-space filtering and normalization beyond the tokenizer utility must be tested by the Qdrant runtime specs that depend on that contract.

These crate-level `utils` tests remain helper-focused.
Qdrant-specific sparse query preparation tests remain owned by the Qdrant unit-test specification.

### 4.5) `input_normalization`

Generated unit tests for the `input_normalization` runtime module must include all of the following cases:

- constructor succeeds when the configured tokenizer can be loaded successfully;
- leading and trailing whitespace are trimmed from the query;
- newlines are flattened during normalization;
- tabs and mixed whitespace runs are canonicalized into single ASCII spaces;
- a query that becomes empty after normalization fails with `InputNormalizationError::EmptyQuery`;
- a non-empty query whose normalized form produces zero tokens fails with `InputNormalizationError::EmptyQuery`;
- a query whose token count is exactly equal to `max_input_tokens` succeeds;
- a query whose token count is greater than `max_input_tokens` fails with `InputNormalizationError::InputTooLong`;
- successful normalization returns the expected canonical query string;
- successful normalization returns the correct `input_token_count` for the canonical query.

Tokenizer utility behavior such as download, cache reuse, and tokenizer-load failure handling that is already owned by `Specification/runtime/utils/tokenizer.md` must not be duplicated here.

### 4.6) `observability`

Generated unit tests for the crate-level `observability` boundary must include all of the following cases:

- initialization succeeds in disabled mode when both `tracing_enabled = false` and `metrics_enabled = false`;
- initialization fails as a startup error when tracing is enabled and `tracing_endpoint` is invalid;
- initialization fails as a startup error when metrics are enabled and `metrics_endpoint` is invalid;
- initialization uses `trace_batch_scheduled_delay_ms` when constructing the tracing pipeline;
- initialization uses `metrics_export_interval_ms` when constructing the metrics pipeline.

### 4.7) `main`

Generated unit tests for the crate-level `main` CLI boundary must include all of the following cases:

- CLI argument parsing accepts `--config` and `--ingest-config` and preserves the supplied paths exactly;
- missing required `--config` fails as a startup-time CLI argument error;
- missing required `--ingest-config` fails as a startup-time CLI argument error;
- startup loads `.env` through library-owned config-loading code rather than requiring a dedicated CLI path argument for the env contract;
- startup fails before later runtime initialization when a required environment variable is absent after config loading;
- the CLI delegates config loading to library-owned code rather than parsing TOML content inside `main.rs`.

## 5) Completion Rule

Generation for crate-level runtime unit tests is complete only when all of the following are true:

- required crate-level unit tests from this document exist as executable Rust tests;
- crate-level generated unit tests comply with `Specification/runtime/unit_tests_common.md`;
- child-owned required unit tests remain delegated to their dedicated child specifications without duplication or conflict;
- required crate-level tests are generated in the same generation pass as the corresponding implementation;
- crate-level required tests are not replaced by comments, TODO markers, prose, pseudo-tests, placeholder functions without assertions, or empty test modules.
