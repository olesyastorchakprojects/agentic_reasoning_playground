## 1) Purpose / Scope

This document defines the minimal crate-level runtime specification for the application skeleton.

The current version exists to define:
- the crate-level module tree;
- the crate-level module boundaries;
- the crate-level error hierarchy;
- the runtime configuration model and loading rules;
- crate-level composition rules for `lib.rs` and `main.rs`;
- how the runtime crate-level specification relates to existing runtime slice specifications.

This document does not define:
- detailed observability spans, metrics, or dashboards;
- orchestration flows;
- domain logic;
- request/response workflows above the current runtime API-client layer;
- detailed behavior of individual API-client modules already specified elsewhere;
- pre-ingest incident-card chunk generation from PostgreSQL.

The current version must be minimal.
It must define only the crate structure, error-model rules, and configuration rules required to generate a clean Rust crate skeleton that can be extended later.

This document is the crate-level source of truth for:
- `src/lib.rs`
- `src/main.rs`
- `src/errors/mod.rs`
- `src/shared_types.rs`
- `src/api_clients/mod.rs`
- `src/config/mod.rs`
- crate-level composition and re-export rules

Detailed child-module behavior and child-module generation remain defined in their dedicated specifications under:
- `Specification/runtime/api_clients/`
- `Specification/runtime/request_pipeline/`
- `Specification/runtime/utils/`

Related external preprocessing specification:
- `Specification/card_to_chunk_converter/incident_card_chunk_conversion.md`

That specification defines the converter that reads canonical incident cards
from PostgreSQL and produces the pre-ingest chunk files later consumed by
hybrid ingest.

## 2) Crate Structure

The generated runtime must be a Rust crate that includes both:
- a library target;
- a binary target.

The current crate-level structure consists of:
- `lib.rs`
- `main.rs`
- `errors`
- `config`
- `observability`
- `api_clients`
- `request_pipeline`
- `utils`

The current required crate-level module tree is:

- crate root
  - `errors`
  - `config`
  - `observability`
  - `api_clients`
    - `embedding_client`
    - `model`
    - `qdrant`
    - `postgres`
  - `request_pipeline`
    - `input_normalization`
    - `query_structuring`
    - `candidate_card_retrieval`
    - `card_hydration`
    - `incident_evidence_retrieval`
  - `utils`
    - `retry`
    - `tokenizer`

Structure rules:
- `lib.rs` is the primary public crate boundary;
- `main.rs` is a thin binary entrypoint;
- `errors` owns only crate-level root error definitions and root error re-exports;
- `config` owns runtime config loading, resolved settings, and configuration errors;
- `observability` owns observability initialization and lifetime management;
- `api_clients` is the parent boundary for runtime external-service clients;
- `request_pipeline` is the parent boundary for request-processing leaf modules above the current API-client layer;
- `utils` owns reusable crate-wide helpers that are intentionally shared across multiple runtime areas;
- child API-client subtrees keep their own dedicated module contracts;
- the generated crate structure must remain extension-friendly for future runtime layers.

Current request-pipeline file-layout rule:
- `request_pipeline::input_normalization` is generated as `src/request_pipeline/input_normalization.rs`;
- `request_pipeline::query_structuring` is generated as `src/request_pipeline/query_structuring.rs`;
- `request_pipeline::candidate_card_retrieval` is generated as `src/request_pipeline/candidate_card_retrieval.rs`;
- `request_pipeline::card_hydration` is generated as `src/request_pipeline/card_hydration.rs`;
- `request_pipeline::incident_evidence_retrieval` is generated as `src/request_pipeline/incident_evidence_retrieval.rs`;
- the current version must not split `query_structuring` into a nested `mod.rs` subtree;
- future refactoring into a directory module is allowed only after the crate-level runtime specification is updated.

## 3) Module Boundary Rules

### `lib.rs`

`lib.rs` must:
- declare the public crate module tree;
- expose the top-level runtime modules required by this specification;
- serve as the main public boundary for library consumers.

`lib.rs` must not:
- contain concrete API-client implementation logic;
- contain application bootstrapping logic beyond what is required to expose the crate boundary;
- duplicate type or error definitions owned by child modules.

### `main.rs`

`main.rs` must:
- provide the binary crate entrypoint;
- remain thin;
- parse CLI arguments that provide config-file paths for startup;
- delegate into library-owned code rather than owning runtime logic itself.

`main.rs` must not:
- contain API-client business logic;
- become the ownership boundary for shared runtime types;
- define a parallel error hierarchy separate from the library crate.
- perform ad hoc config parsing outside the `config` module.

### `errors`

`errors` must:
- define the crate-level root error type;
- re-export parent-module error types where needed by the crate boundary;
- preserve the typed error hierarchy of the crate.

### `config`

`config` must:
- define the resolved runtime `Settings` model;
- load and merge runtime TOML, ingest TOML, and environment variables into typed settings;
- define the config-owned startup error type;
- expose typed settings to the rest of the crate.

`config` must not:
- leave runtime execution logic dependent on raw TOML maps or unchecked string maps after startup;
- duplicate API-client behavior contracts;
- push raw environment lookups into downstream runtime modules after settings construction.

### `api_clients`

`api_clients` must:
- be the parent module for external-service client boundaries;
- define the parent API-client error type;
- expose child API-client subtrees through explicit modules.

`api_clients` must not:
- redefine child-module behavior contracts already specified elsewhere;
- flatten child-module errors into strings;
- own future orchestration or domain-level runtime flows.

### `observability`

`observability` must:
- define the typed observability initialization boundary;
- initialize tracing and metrics exporters from typed settings;
- own observability runtime lifetime management for the current process.

`observability` must not:
- define request-level spans or metrics that are not yet part of the current runtime stage;
- read raw environment variables directly;
- require business modules to construct OTEL providers themselves.

### `utils`

`utils` must:
- contain small reusable helper modules that are intentionally shared across multiple runtime areas;
- contain helpers such as retry and tokenizer utilities when those helpers are not specific to a single API-client subtree.

`utils` must not:
- become a catch-all location for subtree-specific request-preparation logic;
- absorb service-specific logic that belongs to one API-client subtree.

## 4) Error Model

The runtime crate must use the `thiserror` crate for error definitions.

Error hierarchy rule:
- each module defines its own error enum;
- each parent module wraps its direct child-module errors through typed enum variants;
- `RuntimeError` includes only errors from top-level runtime subsystems;
- `ApiClientError` includes only errors from API-client child modules;
- the error hierarchy must mirror the crate module hierarchy.

The current required parent error types are:
- `RuntimeError`
- `ConfigError`
- `ApiClientError`
- `ModelApiClientError`
- `QdrantApiClientError`
- `PostgresApiClientError`

### `RuntimeError`

`RuntimeError` is the single root public error type at the crate boundary.

The generated Rust module must define a crate-level enum equivalent in ownership to:

```rust
pub enum RuntimeError {
    Config(ConfigError),
    ApiClients(ApiClientError),
}
```

Rules:
- `RuntimeError` must include only top-level subsystem errors;
- `RuntimeError` must not directly include leaf API-client errors when an intermediate parent error exists;
- future top-level subsystems may be added as additional `RuntimeError` variants in later iterations.

### `ConfigError`

`ConfigError` is the parent error type for the `config` module boundary.

The generated Rust module must define a config-owned enum equivalent in ownership to:

```rust
pub enum ConfigError {
    Load(String),
    MissingEnvironment { key: String },
    InvalidValue { field: String, reason: String },
}
```

Rules:
- `ConfigError` must be defined in `src/config/mod.rs` or re-exported from there as the parent error of the config subsystem;
- config loading and settings construction must return `ConfigError` before conversion into `RuntimeError`;
- raw `config` crate or dotenv library errors must not leak through the public crate boundary.

### `ApiClientError`

`ApiClientError` is the parent error type for the `api_clients` module boundary.

`ApiClientError` must be defined in `src/api_clients/mod.rs`.
`src/errors/mod.rs` must define only `RuntimeError` and may re-export `ApiClientError`.

The generated Rust module must define a parent enum equivalent in ownership to:

```rust
pub enum ApiClientError {
    Embedding(EmbeddingClientError),
    Model(ModelApiClientError),
    Qdrant(QdrantApiClientError),
    Postgres(PostgresApiClientError),
}
```

Rules:
- `ApiClientError` must include only API-client child-module errors;
- `ApiClientError` must wrap child errors through typed variants;
- `ApiClientError` must not flatten child errors into string-only variants;
- `ApiClientError` must not leak raw third-party error types through its public interface.

### API-Client Subsystem Parent Errors

The generated runtime must define explicit subsystem parent errors for multi-module API-client subtrees.

The current required subsystem parent error types are:
- `ModelApiClientError`
- `QdrantApiClientError`
- `PostgresApiClientError`

The generated Rust modules must define parent enums equivalent in ownership to:

```rust
pub enum ModelApiClientError {
    Client(ModelClientError),
}

pub enum QdrantApiClientError {
    DenseSearch(DenseSearchClientError),
    HybridSearch(HybridSearchClientError),
    CardsCollection(CardsCollectionError),
    PracticeChunksCollection(PracticeChunksCollectionError),
    TheoryChunksCollection(TheoryChunksCollectionError),
}

pub enum PostgresApiClientError {
    IncidentCardStore(IncidentCardStoreError),
}
```

Rules:
- subsystem parent errors must wrap their direct child-module errors through typed variants;
- `ApiClientError` must wrap subsystem parent errors when such an intermediate parent exists;
- `ApiClientError` must not bypass subsystem parents by wrapping those subtree leaf errors directly.

### Leaf Error Ownership

Leaf API-client modules must keep their own public error enums as defined by their dedicated specifications.

Rules:
- leaf modules remain the source of truth for their own failure categories;
- parent error enums wrap child errors rather than replacing them;
- generated code must preserve typed child errors through the parent hierarchy;
- raw third-party, transport, parser, or library errors must not leak through public module interfaces;
- error variants must preserve available diagnostic information owned by the failure point.

### Boundary Rules

Rules:
- crate-level public entrypoints must return `RuntimeError`;
- config-module public entrypoints may return `ConfigError`;
- parent-module public entrypoints may return their parent-module error type;
- leaf-module public entrypoints may continue returning their own leaf error enums where their dedicated specifications require that boundary;
- conversion from child-module errors into parent-module errors must be explicit and typed;
- the generated crate must not create competing parallel root error types.

## 5) Configuration

The runtime must use:
- one runtime TOML config file;
- one ingest TOML config file;
- environment variables loaded from process environment, optionally populated through one `.env` file before config merge.

The current runtime config files are:
- `Execution/distributed_diagnostics/runtime.toml`
- `Execution/distributed_diagnostics/ingest.toml`

The machine-readable schemas for those files are:
- `Execution/schemas/runtime_config.schema.json`
- `Execution/schemas/ingest_config.schema.json`
- `Execution/schemas/env.schema.json`

The human-readable contracts for those files are:
- `Specification/contracts/runtime/runtime_config.md`
- `Specification/contracts/runtime/ingest_config.md`
- `Specification/contracts/runtime/env.md`

Related external ingest-preparation contract:
- `Specification/card_to_chunk_converter/incident_card_chunk_conversion.md`

That contract defines how canonical incident cards are converted into
pre-ingest `chunks.jsonl` files before they are ingested into Qdrant-backed
collections.

### Settings Model

The runtime must define one internal resolved config type named `Settings`.

`Settings` must represent the merged runtime configuration state used by the crate after config loading.

The generated Rust settings model must define top-level types equivalent in ownership to:

```rust
pub struct Settings {
    pub runtime: RuntimeSettings,
    pub retrieval: RetrievalSettings,
    pub input_normalization: InputNormalizationSettings,
    pub query_structuring: QueryStructuringSettings,
    pub model: ModelSettings,
    pub embedding_model: EmbeddingModelSettings,
    pub observability: ObservabilitySettings,
    pub postgres: PostgresSettings,
}

pub struct RuntimeSettings {
    pub config_version: String,
}

pub struct InputNormalizationSettings {
    pub max_input_tokens: usize,
    pub tokenizer_source: String,
}

pub struct QueryStructuringSettings {
    pub controlled_vocabulary_path: String,
    pub prompt_asset_path: String,
    pub max_output_tokens: u32,
}

pub struct EmbeddingModelSettings {
    pub url: String,
    pub name: String,
    pub dimension: usize,
}

pub struct ObservabilitySettings {
    pub tracing_enabled: bool,
    pub metrics_enabled: bool,
    pub tracing_endpoint: String,
    pub metrics_endpoint: String,
    pub trace_batch_scheduled_delay_ms: u64,
    pub metrics_export_interval_ms: u64,
}

pub struct PostgresSettings {
    pub url: String,
}
```

Settings model rules:
- each config field that participates in runtime execution after startup must correspond to one typed field or typed nested field of `Settings`, or to a typed intermediate loading model used only during settings construction;
- resolved runtime execution must use typed settings rather than raw TOML values;
- values loaded from ingest config must be merged into runtime-facing settings where those values are needed by runtime modules;
- the runtime must not keep a parallel top-level `IngestSettings` object once the resolved `Settings` object has been constructed;
- metadata-only config sections that are not used after startup do not need to be preserved inside resolved `Settings`.

### Shared Types

This section defines the types that move between runtime modules.
Only cross-module types belong here.
Types that are private to a single module must be defined in that module specification instead.

For the current request-pipeline stage, the required shared types are:
- `UserRequest`
- `IncidentCard`
- `IncidentPhase`
- `DiscriminatingCheck`
- `ExpectedObservation`
- `NormalizedUserRequest`
- `StructuredUserQuery`
- `QueryStructuringOutput`
- `CandidateCard`
- `CandidateCardRetrievalOutput`
- `CardHydrationOutput`
- `IncidentEvidenceChunk`
- `IncidentEvidenceRetrievalOutput`

The generated Rust runtime must define shared types equivalent in ownership to:

```rust
pub struct UserRequest {
    pub query: String,
}

pub struct IncidentPhase {
    pub phase_name: String,
    pub context: String,
    pub symptoms: Vec<String>,
    pub user_visible_impact: Vec<String>,
    pub observations: Vec<String>,
    pub actions_taken: Vec<String>,
    pub changes_after_actions: Vec<String>,
}

pub struct DiscriminatingCheck {
    pub question: String,
    pub why: String,
}

pub struct ExpectedObservation {
    pub observation: String,
    pub effect: String,
}

pub struct IncidentCard {
    pub case_id: String,
    pub title: String,
    pub source_type: String,
    pub source_name: String,
    pub source_path: String,
    pub vendor_or_project: Option<String>,
    pub system_type: Option<String>,
    pub version_tested: Option<String>,
    pub report_date: Option<String>,
    pub short_summary: String,
    pub canonical_symptoms: Vec<String>,
    pub affected_components: Vec<String>,
    pub failure_mode_candidates: Vec<String>,
    pub observed_phases: Vec<String>,
    pub incident_phases: Vec<IncidentPhase>,
    pub turning_points: Vec<String>,
    pub candidate_explanations: Vec<String>,
    pub diagnostic_patterns: Vec<String>,
    pub discriminating_checks: Vec<DiscriminatingCheck>,
    pub expected_observations: Vec<ExpectedObservation>,
    pub investigation_steps: Vec<String>,
    pub root_cause_summary: Option<String>,
    pub reasoning_summary: Option<String>,
    pub mitigations_or_workarounds: Vec<String>,
    pub prevention_or_design_followups: Vec<String>,
    pub claimed_guarantees: Vec<String>,
    pub violated_properties: Vec<String>,
    pub resolution_status: Option<String>,
    pub fix_versions: Vec<String>,
    pub confidence_notes: Vec<String>,
    pub source_refs: Vec<String>,
}

pub struct NormalizedUserRequest {
    pub query: String,
    pub input_token_count: usize,
}

pub struct StructuredUserQuery {
    pub intent: String,
    pub scenario: String,
    pub symptoms: Vec<StructuredUserQueryTerm>,
    pub affected_subsystems: Vec<StructuredUserQueryTerm>,
    pub failure_modes: Vec<StructuredUserQueryTerm>,
    pub system_properties: Vec<StructuredUserQueryTerm>,
    pub entities: Vec<String>,
    pub constraints: Vec<String>,
    pub triggers: Vec<String>,
    pub observability_signals: Vec<String>,
    pub unresolved_terms: Vec<String>,
    pub rejected_nearby_terms: Vec<RejectedNearbyTerm>,
    pub confidence: StructuredUserQueryConfidence,
}

pub struct QueryStructuringOutput {
    pub structured_query: StructuredUserQuery,
    pub token_usage: ModelTokenUsage,
}

pub struct CandidateCard {
    pub case_id: String,
    pub score: f32,
}

pub struct CandidateCardRetrievalOutput {
    pub primary: Option<CandidateCard>,
    pub alternatives: Vec<CandidateCard>,
}

pub struct CardHydrationOutput {
    pub primary: Option<IncidentCard>,
    pub alternatives: Vec<IncidentCard>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IncidentEvidenceChunk {
    pub chunk_id: String,
    pub case_id: String,
    pub score: f32,
    pub chunk_tags: Vec<String>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IncidentEvidenceRetrievalOutput {
    pub primary_chunks: Vec<IncidentEvidenceChunk>,
    pub alternative_chunks: Vec<IncidentEvidenceChunk>,
}

pub struct ModelTokenUsage {
    pub prompt_tokens: Option<usize>,
    pub completion_tokens: Option<usize>,
    pub total_tokens: Option<usize>,
}

pub struct StructuredUserQueryTerm {
    pub term: String,
    pub evidence_span: String,
    pub support_level: StructuredUserQuerySupportLevel,
}

pub struct RejectedNearbyTerm {
    pub term: String,
    pub reason: String,
}

pub enum StructuredUserQuerySupportLevel {
    Explicit,
    StrongParaphrase,
    WeakInference,
}

pub enum StructuredUserQueryConfidence {
    Low,
    Medium,
    High,
}
```

Shared type rules:

1. `UserRequest`
   - `UserRequest` is the raw request received by the runtime from the caller;
   - `UserRequest` must contain exactly one field:
     - `query: String`
   - `query` is the raw user-provided request text before normalization;
   - `UserRequest` must not contain normalized fields, token counts, config values, derived values, or module-private processing metadata.

2. `IncidentCard`
   - `IncidentCard` is the canonical full-card runtime representation shared across PostgreSQL storage, hydration, and downstream request-pipeline modules;
   - `IncidentCard` must remain structurally aligned with the canonical card contract in `Specification/contracts/storage/incident_card.md`;
   - `IncidentCard.case_id` is the stable unique identity of the card;
   - `IncidentCard` must not be owned privately by one API-client module once it is used across module boundaries.

3. `IncidentPhase`
   - `IncidentPhase` is a shared nested component type used by `IncidentCard`;
   - it must be defined in `src/shared_types.rs` together with `IncidentCard`.

4. `DiscriminatingCheck`
   - `DiscriminatingCheck` is a shared nested component type used by `IncidentCard`;
   - it must be defined in `src/shared_types.rs` together with `IncidentCard`.

5. `ExpectedObservation`
   - `ExpectedObservation` is a shared nested component type used by `IncidentCard`;
   - it must be defined in `src/shared_types.rs` together with `IncidentCard`.

6. `NormalizedUserRequest`
   - `NormalizedUserRequest` is the normalized request produced by the input-normalization boundary;
   - `NormalizedUserRequest` must contain exactly two fields:
     - `query: String`
     - `input_token_count: usize`
   - `query` is the normalized form of `UserRequest.query`;
   - `input_token_count` is the token count computed for `NormalizedUserRequest.query` using the tokenizer defined by the input-normalization contract;
   - `NormalizedUserRequest` must not contain raw input copies, config values, or module-private processing metadata.

7. `StructuredUserQuery`
   - `StructuredUserQuery` is the shared structured interpretation produced by the `query_structuring` boundary;
   - it must contain only cross-module data needed by downstream runtime modules;
   - it must not contain raw prompt text, raw model responses, file paths, or module-private parsing metadata;
   - vocabulary-backed term selections must be represented through `StructuredUserQueryTerm`;
   - rejected nearby candidates must be represented through `RejectedNearbyTerm`;
   - confidence must be represented through `StructuredUserQueryConfidence` rather than raw string values.

8. `QueryStructuringOutput`
   - `QueryStructuringOutput` is the shared runtime output of the `query_structuring` boundary;
   - it wraps the semantic result plus execution metadata from the model call;
   - `structured_query` must contain the parsed domain interpretation;
   - `token_usage` must contain model token-usage metadata and must not be merged into `StructuredUserQuery`.

9. `ModelTokenUsage`
   - `ModelTokenUsage` is shared execution metadata for one model call;
   - `prompt_tokens`, `completion_tokens`, and `total_tokens` must remain `Option<usize>` because providers may omit some or all usage fields;
   - this type is metadata-only and must not be treated as part of the semantic query structure.

10. `CandidateCard`
   - `CandidateCard` is the shared runtime representation of one candidate incident card selected by the `candidate_card_retrieval` boundary;
   - `CandidateCard` must contain exactly two fields:
     - `case_id: String`
     - `score: f32`
   - `case_id` is the canonical incident-card identifier;
   - `score` is the retrieval score associated with that card;
   - `score` must preserve the original retrieval `f32` value without rounding, bucketing, normalization, or rescaling;
   - `CandidateCard` must not contain collection-layer request types, Qdrant payloads, hydration data, or module-private ranking metadata.

11. `CandidateCardRetrievalOutput`
   - `CandidateCardRetrievalOutput` is the shared runtime output of the `candidate_card_retrieval` boundary;
   - it must contain exactly two fields:
     - `primary: Option<CandidateCard>`
     - `alternatives: Vec<CandidateCard>`
   - `primary` is the highest-ranked candidate when at least one candidate exists;
   - `primary` must be `None` when retrieval returns zero candidates;
   - `alternatives` contains the remaining selected candidates after excluding `primary`, in retrieval order.

12. `CardHydrationOutput`
   - `CardHydrationOutput` is the shared runtime output of the `card_hydration` boundary;
   - it must contain exactly two fields:
     - `primary: Option<IncidentCard>`
     - `alternatives: Vec<IncidentCard>`
   - `primary` contains the hydrated full-card form of the primary candidate when one exists;
   - `primary` must be `None` when there is no primary candidate to hydrate;
   - `alternatives` contains the hydrated full-card forms of the alternative candidates in preserved retrieval order.

13. `IncidentEvidenceChunk`
   - `IncidentEvidenceChunk` is the shared runtime representation of one retrieved practice chunk selected by the `incident_evidence_retrieval` boundary;
   - it must contain exactly five fields:
     - `chunk_id: String`
     - `case_id: String`
     - `score: f32`
     - `chunk_tags: Vec<String>`
     - `text: String`
   - `case_id` is the domain identity of the source incident card linked to the chunk;
   - `score` must preserve the original retrieval `f32` value without rounding, normalization, or bucketing;
   - `chunk_tags` must preserve the raw collection-returned tag list;
   - the generated Rust type must derive:
     - `Debug`
     - `Clone`
     - `PartialEq`

14. `IncidentEvidenceRetrievalOutput`
   - `IncidentEvidenceRetrievalOutput` is the shared runtime output of the `incident_evidence_retrieval` boundary;
   - it must contain exactly two fields:
     - `primary_chunks: Vec<IncidentEvidenceChunk>`
     - `alternative_chunks: Vec<IncidentEvidenceChunk>`
   - `primary_chunks` contains only chunks returned by the primary evidence-retrieval call;
   - `alternative_chunks` contains only chunks returned by the alternative evidence-retrieval call;
   - `IncidentEvidenceRetrievalOutput` must preserve the separation between primary and alternative retrieval paths exactly;
   - the generated Rust type must derive:
     - `Debug`
     - `Clone`
     - `PartialEq`

Shared type export rule:
- all shared types defined in this section, including `StructuredUserQuery`, `QueryStructuringOutput`, and `ModelTokenUsage`, must be defined in `src/shared_types.rs`;
- all shared types defined in this section, including `CandidateCard` and `CandidateCardRetrievalOutput`, must be defined in `src/shared_types.rs`;
- all shared types defined in this section, including `IncidentCard`, `IncidentPhase`, `DiscriminatingCheck`, and `ExpectedObservation`, must be defined in `src/shared_types.rs`;
- all shared types defined in this section, including `CardHydrationOutput`, must be defined in `src/shared_types.rs`;
- all shared types defined in this section, including `IncidentEvidenceChunk` and `IncidentEvidenceRetrievalOutput`, must be defined in `src/shared_types.rs`;
- `lib.rs` must expose the shared-types module as `pub mod shared_types;`;
- runtime leaf modules must import these shared types through `crate::shared_types::...` rather than through ad hoc re-exports.

### Retrieval Settings

The generated Rust settings model must define retrieval settings equivalent in ownership to:

```rust
pub struct RetrievalSettings {
    pub qdrant_url: String,
    pub cards: CollectionRetrievalSettings,
    pub practice: CollectionRetrievalSettings,
    pub theory: CollectionRetrievalSettings,
}

pub struct CollectionRetrievalSettings {
    pub top_k: usize,
    pub score_threshold: f32,
    pub max_alternatives: usize,
    pub embedding_retry: RetryPolicyConfig,
    pub qdrant_retry: RetryPolicyConfig,
    pub collection: CollectionSettings,
}
```

Rules:
- `RetrievalSettings` is the single typed retrieval settings section used by retrieval-facing runtime code;
- each collection section must contain runtime-owned retrieval knobs plus one typed resolved collection description;
- retrieval code must be able to access collection name selection, top-k, threshold, alternative-card limit, and retry settings from one `CollectionRetrievalSettings` value without separately reading raw ingest config.

### Input Normalization Settings

The generated Rust settings model must define input-normalization settings equivalent in ownership to:

```rust
pub struct InputNormalizationSettings {
    pub max_input_tokens: usize,
    pub tokenizer_source: String,
}
```

Rules:
- `InputNormalizationSettings` is the single typed settings slice used by request-level input normalization code;
- input-normalization runtime modules must receive this typed settings slice rather than reading raw TOML values;
- tokenizer loading behavior itself is defined by `Specification/runtime/utils/tokenizer.md` rather than by the crate-level settings model.

### Query Structuring Settings

The generated Rust settings model must define query-structuring settings equivalent in ownership to:

```rust
pub struct QueryStructuringSettings {
    pub controlled_vocabulary_path: String,
    pub prompt_asset_path: String,
    pub max_output_tokens: u32,
}
```

Rules:
- `QueryStructuringSettings` is the single typed settings slice used by request-level query structuring code;
- query-structuring runtime modules must receive this typed settings slice rather than reading raw TOML values;
- the controlled vocabulary and prompt asset are runtime-owned file inputs selected by config, not by direct module constants.

### Collection Variants

The generated Rust settings model must define collection variants equivalent in ownership to:

```rust
pub enum CollectionSettings {
    Dense(DenseCollectionSettings),
    Hybrid(HybridCollectionSettings),
}

pub struct DenseCollectionSettings {
    pub name: String,
    pub vector_name: String,
    pub corpus_version: String,
}

pub struct HybridCollectionSettings {
    pub dense_vector_name: String,
    pub sparse_vector_name: String,
    pub corpus_version: String,
    pub sparse: SparseSettings,
}
```

Variant-selection rules:
- `qdrant.collections.<collection>.kind = "dense"` must construct `CollectionSettings::Dense(...)`;
- `qdrant.collections.<collection>.kind = "hybrid"` must construct `CollectionSettings::Hybrid(...)`;
- if the collection kind value is unsupported, startup must fail before runtime request processing begins.

### Sparse Settings

The generated Rust settings model must define sparse settings equivalent in ownership to:

```rust
pub struct SparseSettings {
    pub tokenizer: TokenizerSettings,
    pub preprocessing: SparsePreprocessingSettings,
    pub strategy: SparseStrategySettings,
}

pub struct TokenizerSettings {
    pub library: String,
    pub source: String,
}

pub struct SparsePreprocessingSettings {
    pub kind: String,
    pub lowercase: bool,
    pub min_token_length: usize,
}

pub enum SparseStrategySettings {
    BagOfWords(BagOfWordsSettings),
    Bm25Like(Bm25LikeSettings),
}

pub struct BagOfWordsSettings {
    pub name: String,
    pub query: String,
    pub sparse_vocabulary_path: String,
}

pub struct Bm25LikeSettings {
    pub name: String,
    pub query: String,
    pub sparse_vocabulary_path: String,
    pub bm25_term_stats_path: String,
    pub k1: f32,
    pub b: f32,
    pub idf_smoothing: f32,
}
```

Variant-selection rules:
- `hybrid.sparse.strategy.kind = "bag_of_words"` must construct `SparseStrategySettings::BagOfWords(...)`;
- `hybrid.sparse.strategy.kind = "bm25_like"` must construct `SparseStrategySettings::Bm25Like(...)`;
- both strategy subtrees may exist in raw TOML at the same time;
- only the subtree matching `hybrid.sparse.strategy.kind` becomes part of the resolved `Settings`;
- unsupported sparse strategy kinds must cause startup failure before runtime request processing begins.

### Model Settings

The generated Rust settings model must define model settings equivalent in ownership to:

```rust
pub struct ModelSettings {
    pub transport: ModelTransportSettings,
}

pub enum ModelTransportSettings {
    Ollama(OllamaModelSettings),
    Together(TogetherModelSettings),
}

pub struct OllamaModelSettings {
    pub url: String,
    pub model_name: String,
    pub timeout_sec: u64,
    pub retry: RetryPolicyConfig,
}

pub struct TogetherModelSettings {
    pub url: String,
    pub api_key: String,
    pub model_name: String,
    pub timeout_sec: u64,
    pub retry: RetryPolicyConfig,
}
```

Variant-selection rules:
- `model.transport_kind = "ollama"` must construct `ModelTransportSettings::Ollama(...)`;
- `model.transport_kind = "together"` must construct `ModelTransportSettings::Together(...)`;
- unsupported transport kinds must cause startup failure before runtime request processing begins.

### Field Mapping Rules

Required top-level field mappings:
- `Settings.runtime.config_version` <- `runtime.toml [runtime].config_version`
- `Settings.input_normalization.max_input_tokens` <- `runtime.toml [input_normalization].max_input_tokens`
- `Settings.input_normalization.tokenizer_source` <- `runtime.toml [input_normalization].tokenizer_source`
- `Settings.query_structuring.controlled_vocabulary_path` <- `runtime.toml [query_structuring].controlled_vocabulary_path`
- `Settings.query_structuring.prompt_asset_path` <- `runtime.toml [query_structuring].prompt_asset_path`
- `Settings.query_structuring.max_output_tokens` <- `runtime.toml [query_structuring].max_output_tokens`
- `Settings.embedding_model.url` <- environment variable `OLLAMA_URL`
- `Settings.embedding_model.name` <- `ingest.toml [embedding.model].name`
- `Settings.embedding_model.dimension` <- `ingest.toml [embedding.model].dimension`
- `Settings.observability.tracing_enabled` <- `runtime.toml [observability].tracing_enabled`
- `Settings.observability.metrics_enabled` <- `runtime.toml [observability].metrics_enabled`
- `Settings.observability.tracing_endpoint` <- environment variable `TRACING_ENDPOINT`
- `Settings.observability.metrics_endpoint` <- environment variable `METRICS_ENDPOINT`
- `Settings.observability.trace_batch_scheduled_delay_ms` <- `runtime.toml [observability].trace_batch_scheduled_delay_ms`
- `Settings.observability.metrics_export_interval_ms` <- `runtime.toml [observability].metrics_export_interval_ms`
- `Settings.postgres.url` <- environment variable `POSTGRES_URL`
- `Settings.retrieval.qdrant_url` <- environment variable `QDRANT_URL`

Required retrieval field mappings for each of `cards`, `practice`, and `theory`:
- `top_k` <- `runtime.toml [retrieval.<collection>].top_k`
- `score_threshold` <- `runtime.toml [retrieval.<collection>].score_threshold`
- `max_alternatives` <- `runtime.toml [retrieval.<collection>].max_alternatives`
- `embedding_retry` <- `runtime.toml [retrieval.<collection>.embedding_retry]`
- `qdrant_retry` <- `runtime.toml [retrieval.<collection>.qdrant_retry]`
- `collection` <- resolved from `ingest.toml [qdrant.collections.<collection>]`

Required dense collection field mappings:
- `name` <- `ingest.toml [qdrant.collections.<collection>.dense].name`
- `vector_name` <- `ingest.toml [qdrant.collections.<collection>.dense].vector_name`
- `corpus_version` <- `ingest.toml [qdrant.collections.<collection>].corpus_version`

Required hybrid collection field mappings:
- `dense_vector_name` <- `ingest.toml [qdrant.collections.<collection>.hybrid].dense_vector_name`
- `sparse_vector_name` <- `ingest.toml [qdrant.collections.<collection>.hybrid].sparse_vector_name`
- `corpus_version` <- `ingest.toml [qdrant.collections.<collection>].corpus_version`
- `sparse` <- resolved from `ingest.toml [qdrant.collections.<collection>.hybrid.sparse]`

Required sparse tokenizer field mappings:
- `library` <- `ingest.toml [qdrant.collections.<collection>.hybrid.sparse.tokenizer].library`
- `source` <- `ingest.toml [qdrant.collections.<collection>.hybrid.sparse.tokenizer].source`

Required sparse preprocessing field mappings:
- `kind` <- `ingest.toml [qdrant.collections.<collection>.hybrid.sparse.preprocessing].kind`
- `lowercase` <- `ingest.toml [qdrant.collections.<collection>.hybrid.sparse.preprocessing].lowercase`
- `min_token_length` <- `ingest.toml [qdrant.collections.<collection>.hybrid.sparse.preprocessing].min_token_length`

Required bag-of-words field mappings:
- `name` <- `ingest.toml [qdrant.collections.<collection>.hybrid.sparse.bag_of_words].name`
- `query` <- `ingest.toml [qdrant.collections.<collection>.hybrid.sparse.bag_of_words].query`
- `sparse_vocabulary_path` <- `ingest.toml [qdrant.collections.<collection>.hybrid.sparse.bag_of_words].sparse_vocabulary_path`

Required BM25-like field mappings:
- `name` <- `ingest.toml [qdrant.collections.<collection>.hybrid.sparse.bm25_like].name`
- `query` <- `ingest.toml [qdrant.collections.<collection>.hybrid.sparse.bm25_like].query`
- `sparse_vocabulary_path` <- `ingest.toml [qdrant.collections.<collection>.hybrid.sparse.bm25_like].sparse_vocabulary_path`
- `bm25_term_stats_path` <- `ingest.toml [qdrant.collections.<collection>.hybrid.sparse.bm25_like].bm25_term_stats_path`
- `k1` <- `ingest.toml [qdrant.collections.<collection>.hybrid.sparse.bm25_like].k1`
- `b` <- `ingest.toml [qdrant.collections.<collection>.hybrid.sparse.bm25_like].b`
- `idf_smoothing` <- `ingest.toml [qdrant.collections.<collection>.hybrid.sparse.bm25_like].idf_smoothing`

Required model field mappings:
- `ModelTransportSettings::Ollama.url` <- environment variable `OLLAMA_URL`
- `ModelTransportSettings::Ollama.model_name` <- `runtime.toml [model.ollama].model_name`
- `ModelTransportSettings::Ollama.timeout_sec` <- `runtime.toml [model.ollama].timeout_sec`
- `ModelTransportSettings::Ollama.retry` <- `runtime.toml [model.ollama.retry]`
- `ModelTransportSettings::Together.url` <- environment variable `OPENAI_COMPATIBLE_URL`
- `ModelTransportSettings::Together.api_key` <- environment variable `TOGETHER_API_KEY`
- `ModelTransportSettings::Together.model_name` <- `runtime.toml [model.together].model_name`
- `ModelTransportSettings::Together.timeout_sec` <- `runtime.toml [model.together].timeout_sec`
- `ModelTransportSettings::Together.retry` <- `runtime.toml [model.together.retry]`

### Config Loading Rules

Rules:
- config loading must use the Rust `config` crate;
- if a `.env` file is used, it must be loaded into process environment with `dotenvy` before `config::Environment` is applied;
- `.env` loading must happen exactly once on the startup path;
- service endpoint URLs must come from environment variables rather than TOML files in the current runtime contract;
- runtime modules must not perform ad hoc direct environment lookups after `Settings` has been constructed;
- startup must fail if a required environment variable is missing;
- startup must fail if runtime TOML, ingest TOML, or environment values cannot be merged into typed `Settings`;
- retry settings must deserialize into typed retry-policy structs rather than remaining unchecked strings in execution logic;
- raw TOML strategy discriminator strings such as `kind` and `transport_kind` must not remain the runtime branching mechanism after settings construction;
- startup must not allow a resolved `Settings` object whose enum variants disagree with the TOML discriminator values that selected them.

Contract/schema rules:
- `runtime_config.md`, `ingest_config.md`, and `env.md` are the human-readable source-of-truth contracts for their corresponding configuration sources;
- `runtime_config.schema.json`, `ingest_config.schema.json`, and `env.schema.json` are the machine-readable contract artifacts for those same configuration sources;
- the environment contract may contain keys that are not yet consumed by the current runtime startup path, as long as the keys remain part of the repository-owned `.env` contract;
- those schema files do not require a separate mandatory JSON-schema validation step inside the Rust runtime startup path;
- it is sufficient for runtime startup to enforce the same guarantees through config loading, typed deserialization, and typed validation into resolved `Settings`.

### Settings Propagation Rules

Rules:
- `Settings` must be constructed exactly once inside the `config` module;
- after `Settings` has been constructed, runtime modules must not parse raw TOML values, raw config maps, or raw environment values for their own operation;
- future bootstrap code may depend on the whole `Settings` value in order to wire the runtime;
- non-bootstrap runtime modules must receive only the typed settings slices they require;
- leaf API clients must not depend on the crate-level `Settings` type;
- future parent runtime modules may convert crate-level typed settings slices into module-owned config types before constructing leaf API clients;
- the current minimal crate-skeleton stage does not require production runtime wiring that constructs leaf API clients from top-level `Settings`;
- propagation of configuration into concrete module interfaces must be defined by the corresponding child module specifications rather than by ad hoc crate-level wiring logic.

### Observability Initialization Contract

The current runtime stage requires observability initialization only.

The current stage does not require:
- request-level tracing spans;
- request-level metrics;
- Phoenix/OpenInference semantic telemetry;
- Grafana dashboard generation.

The generated runtime must define a typed observability initialization boundary equivalent in ownership to:

```rust
pub struct ObservabilityRuntime {
    // implementation-owned guards and providers
}

impl ObservabilityRuntime {
    pub fn initialize(
        settings: &ObservabilitySettings,
    ) -> Result<Self, RuntimeError>;
}
```

Initialization rules:
- observability initialization occurs exactly once at process startup;
- initialization must consume only `Settings.observability`;
- business modules must not initialize OTEL providers directly;
- when `tracing_enabled = true`, startup must initialize OTLP tracing export using `tracing_endpoint`;
- when `metrics_enabled = true`, startup must initialize OTLP metrics export using `metrics_endpoint`;
- when both are disabled, initialization must still succeed with a concrete no-op-safe runtime object;
- initialization failure is a startup error;
- provider lifetime must be kept alive for the full process lifetime;
- detailed implementation rules are defined by `Specification/runtime/observability/implementation.md`.

### CLI Contract

The runtime crate must provide a CLI entrypoint for startup-time config loading.

Required CLI arguments:
- `--config`: path to the runtime TOML file
- `--ingest-config`: path to the ingest TOML file

CLI rules:
- `--config` is required;
- `--ingest-config` is required;
- invalid CLI arguments are a startup error;
- CLI startup must delegate config loading to the library-owned `config` module rather than parsing TOML or environment values inside `main.rs`;
- startup may load the repository-owned `.env` file through `dotenvy` before environment-based config merge begins;
- `.env` loading must happen exactly once on the startup path;
- if a required environment variable is absent after `.env` loading and environment merge, startup must fail before any later runtime initialization begins;
- the CLI must construct resolved `Settings` from the CLI-supplied config paths before any later runtime wiring is attempted;
- the current minimal crate-skeleton stage does not require the CLI to construct or execute higher-level runtime flows after settings have been loaded successfully.

CLI path rules:
- `--config` supplies the source path for the runtime config contract defined by `Specification/contracts/runtime/runtime_config.md`;
- `--ingest-config` supplies the source path for the ingest config contract defined by `Specification/contracts/runtime/ingest_config.md`;
- the environment contract defined by `Specification/contracts/runtime/env.md` is sourced from the standard repository `.env` file and the resulting process environment.

## 6) Composition Rules

The crate-level runtime specification composes existing runtime slice specifications.

Composition rules:
- this document must not redefine detailed behavior already owned by dedicated child specifications;
- child API-client module behavior remains defined by the corresponding files under `Specification/runtime/api_clients/`;
- crate-level generation must wire child modules together structurally without overriding their dedicated contracts;
- parent modules must import child types and child errors rather than regenerating duplicate local definitions;
- re-exports may be used where helpful, but they must not hide the declared module hierarchy.

Compatibility rule:
- this crate-level specification must remain compatible with the child generation-order specifications that already define generated child-module artifacts.

The current child artifact ownership is delegated to:
- `Specification/runtime/api_clients/model/generation_order.md`
- `Specification/runtime/api_clients/qdrant/generation_order.md`

This document must not be interpreted as a replacement for those child generation-order specifications.

## 7) General Implementation Guidance

These rules apply to the current minimal runtime crate skeleton.

Implementation rules:
- the implementation must be modular;
- config loading and settings construction must be centralized under the `config` module;
- each parent module must own its own public interface and parent error type;
- cross-module interaction must happen through explicit typed interfaces;
- parent modules must wrap child concerns rather than absorb child-owned implementation logic;
- the generated crate must preserve the declared Rust module layout instead of collapsing logic into one large file;
- behavior already owned by dedicated child specifications must be implemented in the corresponding child modules rather than redefined at crate level;
- reusable retry and tokenizer helpers may live under `src/utils/`;
- Qdrant sparse query preparation must live under the Qdrant subtree rather than under `src/utils/`.

Boundary and ownership rules:
- module boundaries must be preserved in both code structure and error ownership;
- runtime modules must consume typed settings rather than parsing raw config values themselves;
- parent modules must not reach into private child implementation details instead of using declared interfaces;
- child-owned types and child-owned errors must be imported and wrapped rather than redefined locally;
- the crate-level structure must stay easy to extend with future runtime layers without rewriting the existing ownership model.
