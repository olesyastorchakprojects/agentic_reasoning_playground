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
- detailed observability spans, OpenInference spans, metrics, or dashboards;
- domain logic;
- detailed behavior of individual API-client modules already specified elsewhere;
- pre-ingest incident-card chunk generation from PostgreSQL;
- detailed orchestration-loop internals already owned by
  `Specification/runtime/orchestrator/`.

The current version must remain focused.
It must define only the crate structure, startup/CLI entry behavior,
error-model rules, and configuration rules required to generate a clean Rust
runtime skeleton that can be extended later.

This document is the crate-level source of truth for:
- `src/lib.rs`
- `src/main.rs`
- `src/golden_eval_input.rs`
- `src/startup.rs`
- `src/errors/mod.rs`
- `src/shared_types.rs`
- `src/api_clients/mod.rs`
- `src/config/mod.rs`
- crate-level composition and re-export rules

Detailed child-module behavior and child-module generation remain defined in their dedicated specifications under:
- `Specification/runtime/api_clients/`
- `Specification/runtime/request_pipeline/`
- `Specification/runtime/orchestrator/`
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
- `golden_eval_input`
- `startup`
- `errors`
- `config`
- `observability`
- `api_clients`
- `request_pipeline`
- `orchestrator`
- `utils`

The current required crate-level module tree is:

- crate root
  - `golden_eval_input`
  - `startup`
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
    - `query_structuring_metrics`
    - `retrieval_metrics`
    - `candidate_card_retrieval`
    - `card_hydration`
    - `incident_evidence_retrieval`
    - `theory_evidence_retrieval`
    - `prompt_context_assembly`
    - `llm_structured_generation`
    - `response_validation_and_normalization`
  - `orchestrator`
    - `orchestrator`
    - `transition_policy`
    - `step_executor`
    - `run_state`
      - `model`
      - `view`
      - `apply`
    - `run_repository`
    - `errors`
  - `utils`
    - `retry`
    - `tokenizer`

Structure rules:
- `lib.rs` is the primary public crate boundary;
- `main.rs` is a thin binary entrypoint;
- `golden_eval_input` owns startup-time golden batch input validation, typed
  parsing, and conversion into runtime request inputs;
- `startup` owns typed runtime wiring from resolved `Settings` into the
  dependency graph required to construct the orchestrator and its leaf modules;
- `errors` owns only crate-level root error definitions and root error re-exports;
- `config` owns runtime config loading, resolved settings, and configuration errors;
- `observability` owns observability initialization and lifetime management;
- `api_clients` is the parent boundary for runtime external-service clients;
- `request_pipeline` is the parent boundary for request-processing leaf modules above the current API-client layer;
- `orchestrator` is the parent boundary for durable run state, run-state views,
  run-state mutation, transition policy, step execution, repository access, and
  orchestration-owned errors;
- `utils` owns reusable crate-wide helpers that are intentionally shared across multiple runtime areas;
- child API-client subtrees keep their own dedicated module contracts;
- the generated crate structure must remain extension-friendly for future runtime layers.

Current request-pipeline file-layout rule:
- `request_pipeline::input_normalization` is generated as `src/request_pipeline/input_normalization.rs`;
- `request_pipeline::query_structuring` is generated as `src/request_pipeline/query_structuring.rs`;
- `request_pipeline::query_structuring_metrics` is generated as `src/request_pipeline/query_structuring_metrics.rs`;
- `request_pipeline::retrieval_metrics` is generated as `src/request_pipeline/retrieval_metrics.rs`;
- `request_pipeline::candidate_card_retrieval` is generated as `src/request_pipeline/candidate_card_retrieval.rs`;
- `request_pipeline::card_hydration` is generated as `src/request_pipeline/card_hydration.rs`;
- `request_pipeline::incident_evidence_retrieval` is generated as `src/request_pipeline/incident_evidence_retrieval.rs`;
- `request_pipeline::theory_evidence_retrieval` is generated as `src/request_pipeline/theory_evidence_retrieval.rs`;
- `request_pipeline::prompt_context_assembly` is generated as `src/request_pipeline/prompt_context_assembly.rs`;
- `request_pipeline::llm_structured_generation` is generated as `src/request_pipeline/llm_structured_generation.rs`;
- `request_pipeline::response_validation_and_normalization` is generated as `src/request_pipeline/response_validation_and_normalization.rs`;
- the current version must not split `query_structuring` into a nested `mod.rs` subtree;
- future refactoring into a directory module is allowed only after the crate-level runtime specification is updated.

Current orchestrator file-layout rule:
- `orchestrator::orchestrator` is generated as `src/orchestrator/orchestrator.rs`;
- `orchestrator::run_state::model` is generated as `src/orchestrator/run_state/model.rs`;
- `orchestrator::run_state::view` is generated as `src/orchestrator/run_state/view.rs`;
- `orchestrator::run_state::apply` is generated as `src/orchestrator/run_state/apply.rs`;
- other orchestrator modules and parent module files are owned by
  `Specification/runtime/generated_artifacts.md`.

Current golden-eval-input file-layout rule:
- `golden_eval_input` is generated as `src/golden_eval_input.rs`;
- `golden_eval_input` is a dedicated crate-level module and must not be folded
  into `lib.rs`, `main.rs`, `config`, or `orchestrator`.

## 3) Module Boundary Rules

### `lib.rs`

`lib.rs` must:
- declare the public crate module tree;
- expose the top-level runtime modules required by this specification;
- serve as the main public boundary for library consumers.

`lib.rs` must not:
- contain concrete API-client implementation logic;
- contain application bootstrapping logic beyond what is required to expose the crate boundary;
- contain inline golden-file schema validation or golden batch parsing logic;
- duplicate type or error definitions owned by child modules.

### `main.rs`

`main.rs` must:
- provide the binary crate entrypoint;
- remain thin;
- parse CLI arguments that provide config-file paths for startup;
- select interactive versus golden-backed batch-eval startup mode from typed
  CLI arguments;
- delegate into library-owned code rather than owning runtime logic itself.

`main.rs` must not:
- contain API-client business logic;
- become the ownership boundary for shared runtime types;
- define a parallel error hierarchy separate from the library crate.
- perform ad hoc config parsing outside the `config` module.
- own schema validation, golden-file parsing, or orchestration-specific
  `UserRequest` construction inline.

### `golden_eval_input`

`golden_eval_input` must:
- define the startup-time boundary for golden-backed batch input loading;
- validate the supplied golden-cases JSON file against the supplied JSON
  schema;
- parse validated golden JSON into typed runtime structures;
- convert validated parsed golden-case items directly into `UserRequest`
  values for batch runtime execution;
- expose a typed entrypoint that accepts the supplied golden-cases file path
  and golden-cases schema path and returns `Vec<UserRequest>`;
- define a typed parent error boundary `GoldenEvalInputError` for file
  loading, schema validation, JSON parsing, and typed parsing failures.

The generated Rust module must define an entrypoint equivalent in ownership to:

```rust
pub fn load_golden_eval_requests(
    golden_cases_file: &std::path::Path,
    golden_cases_schema: &std::path::Path,
) -> Result<Vec<UserRequest>, GoldenEvalInputError>;
```

Interface rules:
- `golden_cases_file` is the path supplied through `--golden-cases-file`;
- `golden_cases_schema` is the path supplied through `--golden-cases-schema`;
- on success, the returned vector must contain exactly one `UserRequest` per
  validated golden-case item from the file;
- on failure, the function must return `GoldenEvalInputError` rather than
  panicking, exiting the process, or returning untyped string errors.

`golden_eval_input` must not:
- own orchestration-loop logic;
- create or mutate `RunState`;
- assemble execution-time `Context`;
- absorb generic config loading responsibilities owned by `config`.

### `startup`

`startup` must:
- define the typed runtime-wiring boundary that builds the orchestrator from
  resolved `Settings`;
- construct configured API clients, request-pipeline modules, transition policy,
  `StepExecutor`, `RunRepository`, and the root `Orchestrator`;
- expose a typed startup entrypoint for orchestration wiring;
- define a typed parent error boundary `StartupError` for dependency-wiring and
  leaf-module-construction failures that occur after config loading and before
  runtime request execution begins.

`startup` must not:
- parse CLI arguments;
- load or merge config files directly;
- validate or parse golden input files;
- own interactive-loop or batch-loop control flow.

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
- `GoldenEvalInputError`
- `ObservabilityError`
- `ApiClientError`
- `ModelApiClientError`
- `QdrantApiClientError`
- `PostgresApiClientError`
- `StartupError`

### `RuntimeError`

`RuntimeError` is the single root public error type at the crate boundary.

The generated Rust module must define a crate-level enum equivalent in ownership to:

```rust
pub enum RuntimeError {
    Config(ConfigError),
    GoldenEvalInput(GoldenEvalInputError),
    Observability(ObservabilityError),
    ApiClients(ApiClientError),
    Startup(StartupError),
}
```

Rules:
- `RuntimeError` must include only top-level subsystem errors;
- `RuntimeError` must not directly include leaf API-client errors when an intermediate parent error exists;
- future top-level subsystems may be added as additional `RuntimeError` variants in later iterations.

### `GoldenEvalInputError`

`GoldenEvalInputError` is the parent error type for the `golden_eval_input`
module boundary.

The generated Rust module must define a golden-input-owned enum equivalent in
ownership to:

```rust
pub enum GoldenEvalInputError {
    GoldenCasesRead { path: String, message: String },
    GoldenCasesSchemaRead { path: String, message: String },
    InvalidJson { message: String },
    SchemaValidation { message: String },
    TypedParse { message: String },
}
```

Rules:
- `GoldenEvalInputError` must be defined in `src/golden_eval_input.rs`;
- golden batch input loading must return `GoldenEvalInputError` before
  conversion into `RuntimeError`;
- raw filesystem, JSON, or schema-validation library errors must not leak
  through the public crate boundary.

### `ObservabilityError`

`ObservabilityError` is the parent error type for the `observability` module
boundary.

The generated Rust module must define an observability-owned enum equivalent in
ownership to:

```rust
pub enum ObservabilityError {
    Initialization { message: String },
}
```

Rules:
- `ObservabilityError` must be defined in `src/observability/mod.rs`;
- startup-time observability initialization must return `ObservabilityError`
  before conversion into `RuntimeError`;
- raw exporter and OTEL initialization errors must not leak through the public
  crate boundary.

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

### `StartupError`

`StartupError` is the parent error type for the `startup` module boundary.

The generated Rust module must define a startup-owned enum that wraps typed
construction failures from the runtime wiring phase.

Rules:
- `StartupError` must be defined in `src/startup.rs`;
- `StartupError` must wrap typed module-construction failures through explicit
  variants rather than flattening them into strings;
- startup wiring must return `StartupError` before conversion into
  `RuntimeError`;
- `StartupError` must not absorb CLI parsing, config loading, observability
  initialization, or golden-file validation failures that belong to other
  top-level boundaries.

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
    pub prompt_context: PromptContextSettings,
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
- `GoldenQuestion`
- `GoldenQuestionQuery`
- `GoldenQueryStructuringTargets`
- `GoldenVocabularyFieldTargets`
- `GoldenTermRelevance`
- `GoldenCandidateCardSection`
- `GoldenCardRetrievalTargets`
- `GoldenCardRelevance`
- `GoldenIncidentEvidenceTargets`
- `GoldenChunkRetrievalCallTargets`
- `GoldenTheoryEvidenceTargets`
- `GoldenChunkRetrievalTargets`
- `GoldenChunkRelevance`
- `RetrievalEvaluationMetrics`
- `CandidateCardRetrievalMetrics`
- `IncidentEvidenceBranchRetrievalMetrics`
- `IncidentEvidenceRetrievalMetrics`
- `TheoryEvidenceRetrievalMetrics`
- `OpenInferenceContext`
- `Context`
- `IncidentCard`
- `IncidentPhase`
- `DiscriminatingCheck`
- `ExpectedObservation`
- `NormalizedUserRequest`
- `StructuredUserQuery`
- `QueryStructuringOutput`
- `QueryStructuringControlledVocabulary`
- `QueryStructuringMetrics`
- `ModelTokenUsage`
- `CandidateCard`
- `CandidateCardRetrievalOutput`
- `CardHydrationOutput`
- `IncidentEvidenceChunk`
- `IncidentEvidenceRetrievalOutput`
- `TheoryEvidenceChunk`
- `TheoryEvidenceRetrievalOutput`
- `IncidentChunkTag`
- `PromptEvidenceRole`
- `PromptIncidentEvidenceChunk`
- `PromptTheoryEvidenceChunk`
- `PromptContextAssemblyOutput`

The generated Rust runtime must define shared types equivalent in ownership to:

```rust
pub struct UserRequest {
    pub query: String,
    pub golden_question: Option<GoldenQuestion>,
}

pub struct GoldenQuestion {
    pub case_id: String,
    pub query: GoldenQuestionQuery,
    pub expected_query_structuring: GoldenQueryStructuringTargets,
    pub expected_candidate_cards: GoldenCandidateCardSection,
    pub expected_incident_evidence: GoldenIncidentEvidenceTargets,
    pub expected_theory_evidence: GoldenTheoryEvidenceTargets,
}

pub struct GoldenQuestionQuery {
    pub raw: String,
}

pub struct GoldenQueryStructuringTargets {
    pub symptoms: GoldenVocabularyFieldTargets,
    pub affected_subsystems: GoldenVocabularyFieldTargets,
    pub failure_modes: GoldenVocabularyFieldTargets,
    pub system_properties: GoldenVocabularyFieldTargets,
}

pub struct GoldenVocabularyFieldTargets {
    pub strict_vocabulary_terms: Vec<String>,
    pub soft_vocabulary_terms: Vec<String>,
    pub graded_relevance: Vec<GoldenTermRelevance>,
}

pub struct GoldenTermRelevance {
    pub term: String,
    pub score: f32,
}

pub struct GoldenCandidateCardSection {
    pub retrieval_relevant_cards: GoldenCardRetrievalTargets,
}

pub struct GoldenCardRetrievalTargets {
    pub strict_card_ids: Vec<String>,
    pub soft_card_ids: Vec<String>,
    pub graded_relevance: Vec<GoldenCardRelevance>,
}

pub struct GoldenCardRelevance {
    pub card_id: String,
    pub score: f32,
}

pub struct GoldenIncidentEvidenceTargets {
    pub primary_card_evidence_query: GoldenChunkRetrievalCallTargets,
    pub alternative_cards_evidence_query: GoldenChunkRetrievalCallTargets,
}

pub struct GoldenChunkRetrievalCallTargets {
    pub retrieval_call_id: String,
    pub relevance_judgments: GoldenChunkRetrievalTargets,
}

pub struct GoldenTheoryEvidenceTargets {
    pub mechanism_explanation: GoldenChunkRetrievalTargets,
}

pub struct GoldenChunkRetrievalTargets {
    pub strict_chunk_ids: Vec<String>,
    pub soft_chunk_ids: Vec<String>,
    pub graded_relevance: Vec<GoldenChunkRelevance>,
}

pub struct GoldenChunkRelevance {
    pub chunk_id: String,
    pub score: f32,
}

pub struct RetrievalEvaluationMetrics {
    pub evaluated_k: u32,
    pub recall_soft: f32,
    pub recall_strict: f32,
    pub rr_soft: f32,
    pub rr_strict: f32,
    pub ndcg: f32,
    pub first_relevant_rank_soft: Option<u32>,
    pub first_relevant_rank_strict: Option<u32>,
    pub num_relevant_soft: u32,
    pub num_relevant_strict: u32,
}

pub struct CandidateCardRetrievalMetrics {
    pub retrieval_relevant_cards: RetrievalEvaluationMetrics,
}

pub struct IncidentEvidenceBranchRetrievalMetrics {
    pub relevance_judgments: RetrievalEvaluationMetrics,
}

pub struct IncidentEvidenceRetrievalMetrics {
    pub primary_card_evidence_query: IncidentEvidenceBranchRetrievalMetrics,
    pub alternative_cards_evidence_query: IncidentEvidenceBranchRetrievalMetrics,
}

pub struct TheoryEvidenceRetrievalMetrics {
    pub mechanism_explanation: RetrievalEvaluationMetrics,
}

pub struct OpenInferenceContext {
    pub root_span: tracing::Span,
}

pub struct Context {
    pub open_inference: OpenInferenceContext,
    pub golden_question: Option<GoldenQuestion>,
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
    pub metrics: Option<QueryStructuringMetrics>,
}

pub struct QueryStructuringControlledVocabulary {
    pub canonical_symptoms: Vec<String>,
    pub affected_components: Vec<String>,
    pub failure_mode_candidates: Vec<String>,
    pub violated_properties: Vec<String>,
}

pub struct QueryStructuringMetrics {
    pub top_level: QueryStructuringTopLevelMetrics,
    pub vocab_fields: QueryStructuringVocabularyFieldMetrics,
    pub non_vocab_fields: QueryStructuringNonVocabularyFieldMetrics,
    pub aggregates: QueryStructuringAggregateMetrics,
}

pub struct QueryStructuringTopLevelMetrics {
    pub macro_precision_soft: f32,
    pub macro_recall_strict: f32,
    pub macro_recall_soft: f32,
    pub overall_grounded_strict_recall: f32,
    pub all_fields_core_success_rate: f32,
}

pub struct QueryStructuringVocabularyFieldMetrics {
    pub symptoms: QueryStructuringVocabularyFieldMetricSet,
    pub affected_subsystems: QueryStructuringVocabularyFieldMetricSet,
    pub failure_modes: QueryStructuringVocabularyFieldMetricSet,
    pub system_properties: QueryStructuringVocabularyFieldMetricSet,
}

pub struct QueryStructuringVocabularyFieldMetricSet {
    pub invalid_vocab_count: u32,
    pub duplicate_term_count: u32,
    pub precision_soft: f32,
    pub recall_strict: f32,
    pub recall_soft: f32,
    pub num_false_positive: u32,
    pub num_false_negative_strict: u32,
    pub num_predicted_terms: u32,
    pub graded_coverage: f32,
    pub average_selected_score: f32,
    pub zero_score_selection_count: u32,
    pub grounded_strict_recall: f32,
    pub unsupported_selected_term_rate: f32,
    pub missing_evidence_span_count: u32,
    pub invalid_evidence_span_count: u32,
    pub evidence_span_near_substring_rate: f32,
    pub weak_inference_rate: f32,
    pub strict_terms_weak_inference_rate: f32,
    pub weak_false_positive_rate: f32,
    pub field_core_success: bool,
    pub field_grounded_success: bool,
    pub empty_when_gold_exists: bool,
}

pub struct QueryStructuringNonVocabularyFieldMetrics {
    pub entities_count: u32,
    pub constraints_count: u32,
    pub triggers_count: u32,
    pub observability_signals_count: u32,
    pub unresolved_terms_count: u32,
    pub intent_present: bool,
    pub scenario_present: bool,
}

pub struct QueryStructuringAggregateMetrics {
    pub macro_precision_soft: f32,
    pub macro_recall_strict: f32,
    pub macro_recall_soft: f32,
    pub overall_grounded_strict_recall: f32,
    pub all_fields_core_success_rate: f32,
}

pub struct CandidateCard {
    pub case_id: String,
    pub score: f32,
}

pub struct CandidateCardRetrievalOutput {
    pub primary: Option<CandidateCard>,
    pub alternatives: Vec<CandidateCard>,
    pub metrics: Option<CandidateCardRetrievalMetrics>,
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
    pub metrics: Option<IncidentEvidenceRetrievalMetrics>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TheoryEvidenceChunk {
    pub chunk_id: String,
    pub score: f32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TheoryEvidenceRetrievalOutput {
    pub chunks: Vec<TheoryEvidenceChunk>,
    pub metrics: Option<TheoryEvidenceRetrievalMetrics>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    strum_macros::EnumString,
    strum_macros::Display,
    strum_macros::AsRefStr,
)]
pub enum IncidentChunkTag {
    #[strum(serialize = "chunk_role:symptom")]
    Symptom,
    #[strum(serialize = "chunk_role:impact")]
    Impact,
    #[strum(serialize = "chunk_role:timeline")]
    Timeline,
    #[strum(serialize = "chunk_role:symptom_change")]
    SymptomChange,
    #[strum(serialize = "chunk_role:investigation")]
    Investigation,
    #[strum(serialize = "chunk_role:diagnostic_step")]
    DiagnosticStep,
    #[strum(serialize = "chunk_role:hypothesis_update")]
    HypothesisUpdate,
    #[strum(serialize = "chunk_role:recovery")]
    Recovery,
    #[strum(serialize = "chunk_role:failure_mode")]
    FailureMode,
    #[strum(serialize = "chunk_role:root_cause")]
    RootCause,
    #[strum(serialize = "chunk_role:contributing_factor")]
    ContributingFactor,
    #[strum(serialize = "chunk_role:uncertainty")]
    Uncertainty,
    #[strum(serialize = "chunk_role:lesson")]
    Lesson,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PromptEvidenceRole {
    EvidenceForMatch,
    FirstCheckHint,
    SupportingExplanation,
    AlternativeContext,
    MechanismExplanation,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PromptIncidentEvidenceChunk {
    pub role: PromptEvidenceRole,
    pub chunk_id: String,
    pub case_id: String,
    pub score: f32,
    pub chunk_tags: Vec<IncidentChunkTag>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PromptTheoryEvidenceChunk {
    pub role: PromptEvidenceRole,
    pub chunk_id: String,
    pub score: f32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PromptContextAssemblyOutput {
    pub prompt: String,
    pub incident_evidence_chunks: Vec<PromptIncidentEvidenceChunk>,
    pub theory_chunks: Vec<PromptTheoryEvidenceChunk>,
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
   - `UserRequest` must contain:
     - `query: String`
     - `golden_question: Option<GoldenQuestion>`
   - `query` is the raw user-provided request text before normalization;
   - `golden_question` is optional eval companion input carried only for
     golden-backed batch execution;
   - in interactive mode, `golden_question` must be `None`;
   - in batch-eval mode, `golden_question` must be `Some(...)` for each
     request created from one validated golden case;
   - `UserRequest` must not contain normalized fields, token counts, config
     values, or derived values unrelated to one request's runtime execution.

2. `GoldenQuestion`
   - `GoldenQuestion` is the shared typed representation of one golden-case
     item from the runtime-eval dataset consumed by golden-backed batch mode;
   - it must preserve the case-level structure needed by:
     - query structuring metrics;
     - candidate-card retrieval metrics;
     - incident-evidence retrieval metrics;
     - theory-evidence retrieval metrics;
   - `GoldenQuestion` is shared because it crosses the runtime entry boundary,
     `UserRequest`, orchestration, and context-aware request-pipeline modules.

3. `GoldenQuestionQuery`
   - `GoldenQuestionQuery` is the shared nested query section of
     `GoldenQuestion`;
   - `raw` is the canonical input query text for that golden case.

4. `GoldenQueryStructuringTargets`
   - `GoldenQueryStructuringTargets` is the shared grouped target set for the
     four vocabulary-backed query-structuring fields;
   - each field must use `GoldenVocabularyFieldTargets`.

5. `GoldenVocabularyFieldTargets`
   - `GoldenVocabularyFieldTargets` is the shared target structure for one
     vocabulary-backed query-structuring field;
   - it must preserve:
     - `strict_vocabulary_terms`
     - `soft_vocabulary_terms`
     - `graded_relevance`

6. `GoldenTermRelevance`
   - `GoldenTermRelevance` is one graded vocabulary relevance item;
   - `score` must preserve the explicit runtime-eval score as `f32`.

7. `GoldenCandidateCardSection`
   - `GoldenCandidateCardSection` is the shared top-level target section for
     candidate-card retrieval;
   - it must preserve the `retrieval_relevant_cards` nested structure.

8. `GoldenCardRetrievalTargets`
   - `GoldenCardRetrievalTargets` is the shared target structure for candidate
     card retrieval;
   - it must preserve:
     - `strict_card_ids`
     - `soft_card_ids`
     - `graded_relevance`

9. `GoldenCardRelevance`
   - `GoldenCardRelevance` is one graded card-relevance item;
   - `score` must preserve the explicit runtime-eval score as `f32`.

10. `GoldenIncidentEvidenceTargets`
   - `GoldenIncidentEvidenceTargets` is the shared grouped target set for the
     incident-evidence retrieval boundary;
   - it must preserve both retrieval calls separately:
     - `primary_card_evidence_query`
     - `alternative_cards_evidence_query`

11. `GoldenChunkRetrievalCallTargets`
   - `GoldenChunkRetrievalCallTargets` is the shared target structure for one
     named retrieval call;
   - it must preserve:
     - `retrieval_call_id`
     - `relevance_judgments`

12. `GoldenTheoryEvidenceTargets`
   - `GoldenTheoryEvidenceTargets` is the shared grouped target set for theory
     evidence retrieval;
   - it must preserve the `mechanism_explanation` target structure.

13. `GoldenChunkRetrievalTargets`
   - `GoldenChunkRetrievalTargets` is the shared target structure for one
     chunk-based retrieval output;
   - it must preserve:
     - `strict_chunk_ids`
     - `soft_chunk_ids`
     - `graded_relevance`

14. `GoldenChunkRelevance`
   - `GoldenChunkRelevance` is one graded chunk-relevance item;
   - `score` must preserve the explicit runtime-eval score as `f32`.

15. `OpenInferenceContext`
   - `OpenInferenceContext` is the execution-time observability companion used
     by context-aware runtime modules;
   - it is shared because it crosses orchestration and leaf-module boundaries.
   - detailed OpenInference span semantics, hierarchy, and payload contracts are
     owned by
     `Specification/runtime/observability/open_inference_spans.md`.

16. `Context`
   - `Context` is the execution-time companion object passed to context-aware
     request-pipeline modules;
   - it must contain:
     - `open_inference: OpenInferenceContext`
     - `golden_question: Option<GoldenQuestion>`
   - in interactive mode, `golden_question` must be `None`;
   - in batch-eval mode, `golden_question` must contain the per-request
     validated golden case companion.
   - `open_inference.root_span` is normally the iteration-scoped
     `oi.chain.diagnostic_iteration` span provided by the orchestrator and
     defined in
     `Specification/runtime/observability/open_inference_spans.md`.

17. `IncidentCard`
   - `IncidentCard` is the canonical full-card runtime representation shared across PostgreSQL storage, hydration, and downstream request-pipeline modules;
   - `IncidentCard` must remain structurally aligned with the canonical card contract in `Specification/contracts/storage/incident_card.md`;
   - `IncidentCard.case_id` is the stable unique identity of the card;
   - `IncidentCard` must not be owned privately by one API-client module once it is used across module boundaries.

18. `IncidentPhase`
   - `IncidentPhase` is a shared nested component type used by `IncidentCard`;
   - it must be defined in `src/shared_types.rs` together with `IncidentCard`.

19. `DiscriminatingCheck`
   - `DiscriminatingCheck` is a shared nested component type used by `IncidentCard`;
   - it must be defined in `src/shared_types.rs` together with `IncidentCard`.

20. `ExpectedObservation`
   - `ExpectedObservation` is a shared nested component type used by `IncidentCard`;
   - it must be defined in `src/shared_types.rs` together with `IncidentCard`.

21. `NormalizedUserRequest`
   - `NormalizedUserRequest` is the normalized request produced by the input-normalization boundary;
   - `NormalizedUserRequest` must contain exactly two fields:
     - `query: String`
     - `input_token_count: usize`
   - `query` is the normalized form of `UserRequest.query`;
   - `input_token_count` is the token count computed for `NormalizedUserRequest.query` using the tokenizer defined by the input-normalization contract;
   - `NormalizedUserRequest` must not contain raw input copies, config values, or module-private processing metadata.

22. `StructuredUserQuery`
   - `StructuredUserQuery` is the shared structured interpretation produced by the `query_structuring` boundary;
   - it must contain only cross-module data needed by downstream runtime modules;
   - it must not contain raw prompt text, raw model responses, file paths, or module-private parsing metadata;
   - vocabulary-backed term selections must be represented through `StructuredUserQueryTerm`;
   - rejected nearby candidates must be represented through `RejectedNearbyTerm`;
   - confidence must be represented through `StructuredUserQueryConfidence` rather than raw string values.

23. `QueryStructuringOutput`
   - `QueryStructuringOutput` is the shared runtime output of the `query_structuring` boundary;
   - it wraps the semantic result plus execution metadata from the model call;
   - `structured_query` must contain the parsed domain interpretation;
   - `token_usage` must contain model token-usage metadata and must not be merged into `StructuredUserQuery`;
   - `metrics` must contain request-local query-structuring metrics in the
     shared `QueryStructuringMetrics` shape when such metrics were computed for
     the current execution;
   - `metrics = None` is allowed for executions that do not carry matching
     golden query-structuring targets.

24. `QueryStructuringControlledVocabulary`
   - `QueryStructuringControlledVocabulary` is the shared typed controlled-
     vocabulary asset shape used by `query_structuring` and the query-
     structuring metrics helper;
   - it must preserve:
     - `canonical_symptoms`
     - `affected_components`
     - `failure_mode_candidates`
     - `violated_properties`
   - it is shared because the loaded and validated vocabulary asset crosses the
     `query_structuring` module boundary into a dedicated internal metrics
     helper module.

25. `QueryStructuringMetrics`
   - `QueryStructuringMetrics` is the shared request-local metric bundle for one
     query-structuring output;
   - it must contain:
     - `top_level`
     - `vocab_fields`
     - `non_vocab_fields`
     - `aggregates`

26. `ModelTokenUsage`
   - `ModelTokenUsage` is shared execution metadata for one model call;
   - `prompt_tokens`, `completion_tokens`, and `total_tokens` must remain `Option<usize>` because providers may omit some or all usage fields;
   - this type is metadata-only and must not be treated as part of the semantic query structure.

27. `CandidateCard`
   - `CandidateCard` is the shared runtime representation of one candidate incident card selected by the `candidate_card_retrieval` boundary;
   - `CandidateCard` must contain exactly two fields:
     - `case_id: String`
     - `score: f32`
   - `case_id` is the canonical incident-card identifier;
   - `score` is the retrieval score associated with that card;
   - `score` must preserve the original retrieval `f32` value without rounding, bucketing, normalization, or rescaling;
   - `CandidateCard` must not contain collection-layer request types, Qdrant payloads, hydration data, or module-private ranking metadata.

28. `CandidateCardRetrievalOutput`
   - `CandidateCardRetrievalOutput` is the shared runtime output of the `candidate_card_retrieval` boundary;
   - it must contain exactly two fields:
     - `primary: Option<CandidateCard>`
     - `alternatives: Vec<CandidateCard>`
   - `primary` is the highest-ranked candidate when at least one candidate exists;
   - `primary` must be `None` when retrieval returns zero candidates;
   - `alternatives` contains the remaining selected candidates after excluding `primary`, in retrieval order.

29. `CardHydrationOutput`
   - `CardHydrationOutput` is the shared runtime output of the `card_hydration` boundary;
   - it must contain exactly two fields:
     - `primary: Option<IncidentCard>`
     - `alternatives: Vec<IncidentCard>`
   - `primary` contains the hydrated full-card form of the primary candidate when one exists;
   - `primary` must be `None` when there is no primary candidate to hydrate;
   - `alternatives` contains the hydrated full-card forms of the alternative candidates in preserved retrieval order.

30. `IncidentEvidenceChunk`
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

29. `IncidentEvidenceRetrievalOutput`
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

30. `TheoryEvidenceChunk`
   - `TheoryEvidenceChunk` is the shared runtime representation of one retrieved theory chunk selected by the `theory_evidence_retrieval` boundary;
   - it must contain exactly three fields:
     - `chunk_id: String`
     - `score: f32`
     - `text: String`
   - `score` must preserve the original retrieval `f32` value without rounding, normalization, bucketing, or rescaling;
   - `text` must preserve the raw collection-returned theory chunk text;
   - the generated Rust type must derive:
     - `Debug`
     - `Clone`
     - `PartialEq`

31. `TheoryEvidenceRetrievalOutput`
   - `TheoryEvidenceRetrievalOutput` is the shared runtime output of the `theory_evidence_retrieval` boundary;
   - it must contain exactly one field:
     - `chunks: Vec<TheoryEvidenceChunk>`
   - `chunks` contains only chunks returned by the theory evidence retrieval call;
   - `chunks` must preserve collection-returned order exactly;
   - the generated Rust type must derive:
     - `Debug`
     - `Clone`
     - `PartialEq`

32. `IncidentChunkTag`
   - `IncidentChunkTag` is the shared typed representation of the finite canonical incident chunk tag vocabulary;
   - it must be defined in `src/shared_types.rs`, not in `src/config/mod.rs`;
   - it must use `strum_macros::EnumString`, `strum_macros::Display`, and `strum_macros::AsRefStr`;
   - each variant must serialize and parse only its full canonical tag string such as `chunk_role:symptom`;
   - short aliases such as `symptom` must not parse successfully;
   - unknown raw tag strings must not parse successfully;
   - the generated Rust type must derive:
     - `Debug`
     - `Clone`
     - `Copy`
     - `PartialEq`
     - `Eq`
     - `Hash`

33. `PromptEvidenceRole`
   - `PromptEvidenceRole` is the shared prompt-facing role assigned to a selected evidence chunk by the `prompt_context_assembly` boundary;
   - `PromptEvidenceRole` must not derive `serde::Serialize` in the current version;
   - prompt JSON serialization of this enum is owned by `prompt_context_assembly` through module-private DTO mapping;
   - it must contain exactly these variants:
     - `EvidenceForMatch`
     - `FirstCheckHint`
     - `SupportingExplanation`
     - `AlternativeContext`
     - `MechanismExplanation`
   - the generated Rust type must derive:
     - `Debug`
     - `Clone`
     - `Copy`
     - `PartialEq`
     - `Eq`
     - `Hash`

34. `PromptIncidentEvidenceChunk`
   - `PromptIncidentEvidenceChunk` is the shared prompt-facing representation of one selected incident evidence chunk;
   - it must preserve selected chunk ids, case ids, scores, tags, and text from `IncidentEvidenceChunk`;
   - `role` must contain the role assigned by `prompt_context_assembly`;
   - `chunk_tags` must contain typed `IncidentChunkTag` values parsed from recognized source `IncidentEvidenceChunk.chunk_tags`;
   - the generated Rust type must derive:
     - `Debug`
     - `Clone`
     - `PartialEq`

35. `PromptTheoryEvidenceChunk`
   - `PromptTheoryEvidenceChunk` is the shared prompt-facing representation of one selected theory chunk;
   - it must preserve selected chunk ids, scores, and text from `TheoryEvidenceChunk`;
   - `role` must be `PromptEvidenceRole::MechanismExplanation` in the current version;
   - the generated Rust type must derive:
     - `Debug`
     - `Clone`
     - `PartialEq`

36. `PromptContextAssemblyOutput`
   - `PromptContextAssemblyOutput` is the shared runtime output of the `prompt_context_assembly` boundary;
   - it must contain exactly three fields:
     - `prompt: String`
     - `incident_evidence_chunks: Vec<PromptIncidentEvidenceChunk>`
     - `theory_chunks: Vec<PromptTheoryEvidenceChunk>`
   - `prompt` is the filled prompt string intended as input to the next model-generation module;
   - `incident_evidence_chunks` contains the selected incident chunks separately from the prompt for history and traceability;
   - `theory_chunks` contains the selected theory chunks separately from the prompt for history and traceability;
   - the generated Rust type must derive:
     - `Debug`
     - `Clone`
     - `PartialEq`

Shared type export rule:
- all shared types defined in this section, including `StructuredUserQuery`,
  `QueryStructuringOutput`, `QueryStructuringControlledVocabulary`,
  `QueryStructuringMetrics`, and `ModelTokenUsage`, must be defined in
  `src/shared_types.rs`;
- all shared types defined in this section, including `GoldenQuestion`,
  `GoldenQuestionQuery`, `GoldenQueryStructuringTargets`,
  `GoldenVocabularyFieldTargets`, `GoldenTermRelevance`,
  `GoldenCandidateCardSection`, `GoldenCardRetrievalTargets`,
  `GoldenCardRelevance`, `GoldenIncidentEvidenceTargets`,
  `GoldenChunkRetrievalCallTargets`, `GoldenTheoryEvidenceTargets`,
  `GoldenChunkRetrievalTargets`, `GoldenChunkRelevance`,
  `OpenInferenceContext`, and `Context`, must be defined in
  `src/shared_types.rs`;
- all shared types defined in this section, including `CandidateCard` and `CandidateCardRetrievalOutput`, must be defined in `src/shared_types.rs`;
- all shared types defined in this section, including `IncidentCard`, `IncidentPhase`, `DiscriminatingCheck`, and `ExpectedObservation`, must be defined in `src/shared_types.rs`;
- all shared types defined in this section, including `CardHydrationOutput`, must be defined in `src/shared_types.rs`;
- all shared types defined in this section, including `IncidentEvidenceChunk` and `IncidentEvidenceRetrievalOutput`, must be defined in `src/shared_types.rs`;
- all shared types defined in this section, including `TheoryEvidenceChunk` and `TheoryEvidenceRetrievalOutput`, must be defined in `src/shared_types.rs`;
- all shared types defined in this section, including `IncidentChunkTag`, `PromptEvidenceRole`, `PromptIncidentEvidenceChunk`, `PromptTheoryEvidenceChunk`, and `PromptContextAssemblyOutput`, must be defined in `src/shared_types.rs`;
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

### Prompt Context Settings

The generated Rust settings model must define prompt-context settings equivalent
in ownership to:

```rust
pub struct PromptContextSettings {
    pub prompt_asset_path: String,
    pub chunk_packing: ChunkPackingSettings,
}

pub struct ChunkPackingSettings {
    pub evidence_for_match: ChunkRolePackingSettings,
    pub first_check_hint: ChunkRolePackingSettings,
    pub supporting_explanation: ChunkRolePackingSettings,
    pub alternative_context: ChunkRolePackingSettings,
    pub mechanism_explanation: ChunkRolePackingSettings,
}

pub struct ChunkRolePackingSettings {
    pub source: ChunkPackingSource,
    pub limit: usize,
    pub per_case_limit: Option<usize>,
    pub fallback_to_any_chunk: bool,
    pub tag_priority: Vec<IncidentChunkTag>,
}

pub enum ChunkPackingSource {
    PrimaryIncident,
    AlternativeIncident,
    Theory,
}
```

Rules:
- `PromptContextSettings` is the single typed settings slice used by `prompt_context_assembly`;
- prompt-context runtime modules must receive this typed settings slice rather than reading raw TOML values;
- `prompt_asset_path` is the runtime-owned JSON prompt asset path selected by config;
- prompt-context chunk limits and tag priorities are owned by runtime config;
- prompt-context chunk selection mechanics are owned by `prompt_context_assembly`;
- `ChunkRolePackingSettings.tag_priority` must use the shared `IncidentChunkTag` type from `src/shared_types.rs`;
- raw config must use full canonical tag strings such as `chunk_role:symptom`;
- short tag aliases such as `symptom` are invalid;
- raw `ChunkPackingSource` config strings must be:
  - `primary_incident`
  - `alternative_incident`
  - `theory`
- unsupported raw `ChunkPackingSource` config strings must cause config loading to fail before runtime request processing begins;
- config loading must parse configured tags into `IncidentChunkTag`;
- unknown configured tags must cause config loading to fail before runtime request processing begins;
- duplicate tags inside one role `tag_priority` list must cause config loading to fail before runtime request processing begins;
- `Cargo.toml` must include `strum` with derive support enabled and must include `strum_macros` explicitly.
- `evidence_for_match.limit` must be greater than or equal to `1`;
- `first_check_hint.limit` must be greater than or equal to `1`;
- `supporting_explanation.limit` defaults to `1` in the runtime config contract;
- `supporting_explanation.limit` may be `0` only when explicitly disabled in config;
- `alternative_context.limit` may be `0`;
- `mechanism_explanation.limit` may be `0`;
- `per_case_limit` is valid only for `alternative_context` in the current version;
- `alternative_context.per_case_limit` must be greater than zero when `alternative_context.limit > 0`;
- `alternative_context.per_case_limit` may be absent or any non-negative value when `alternative_context.limit = 0`;
- `supporting_explanation.source` must be `PrimaryIncident` in the current version;
- `mechanism_explanation.tag_priority` must be empty in the current version because theory chunks do not expose tags.

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
- `Settings.prompt_context.prompt_asset_path` <- `runtime.toml [prompt_context].prompt_asset_path`
- `Settings.prompt_context.chunk_packing` <- `runtime.toml [prompt_context.chunk_packing]`
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

Required prompt-context field mappings:
- `PromptContextSettings.chunk_packing.evidence_for_match.source` <- `runtime.toml [prompt_context.chunk_packing.evidence_for_match].source`
- `PromptContextSettings.chunk_packing.evidence_for_match.limit` <- `runtime.toml [prompt_context.chunk_packing.evidence_for_match].limit`
- `PromptContextSettings.chunk_packing.evidence_for_match.fallback_to_any_chunk` <- `runtime.toml [prompt_context.chunk_packing.evidence_for_match].fallback_to_any_chunk`
- `PromptContextSettings.chunk_packing.evidence_for_match.tag_priority` <- `runtime.toml [prompt_context.chunk_packing.evidence_for_match].tag_priority`
- `PromptContextSettings.chunk_packing.first_check_hint.source` <- `runtime.toml [prompt_context.chunk_packing.first_check_hint].source`
- `PromptContextSettings.chunk_packing.first_check_hint.limit` <- `runtime.toml [prompt_context.chunk_packing.first_check_hint].limit`
- `PromptContextSettings.chunk_packing.first_check_hint.fallback_to_any_chunk` <- `runtime.toml [prompt_context.chunk_packing.first_check_hint].fallback_to_any_chunk`
- `PromptContextSettings.chunk_packing.first_check_hint.tag_priority` <- `runtime.toml [prompt_context.chunk_packing.first_check_hint].tag_priority`
- `PromptContextSettings.chunk_packing.supporting_explanation.source` <- `runtime.toml [prompt_context.chunk_packing.supporting_explanation].source`
- `PromptContextSettings.chunk_packing.supporting_explanation.limit` <- `runtime.toml [prompt_context.chunk_packing.supporting_explanation].limit`
- `PromptContextSettings.chunk_packing.supporting_explanation.fallback_to_any_chunk` <- `runtime.toml [prompt_context.chunk_packing.supporting_explanation].fallback_to_any_chunk`
- `PromptContextSettings.chunk_packing.supporting_explanation.tag_priority` <- `runtime.toml [prompt_context.chunk_packing.supporting_explanation].tag_priority`
- `PromptContextSettings.chunk_packing.alternative_context.source` <- `runtime.toml [prompt_context.chunk_packing.alternative_context].source`
- `PromptContextSettings.chunk_packing.alternative_context.limit` <- `runtime.toml [prompt_context.chunk_packing.alternative_context].limit`
- `PromptContextSettings.chunk_packing.alternative_context.per_case_limit` <- `runtime.toml [prompt_context.chunk_packing.alternative_context].per_case_limit`
- `PromptContextSettings.chunk_packing.alternative_context.fallback_to_any_chunk` <- `runtime.toml [prompt_context.chunk_packing.alternative_context].fallback_to_any_chunk`
- `PromptContextSettings.chunk_packing.alternative_context.tag_priority` <- `runtime.toml [prompt_context.chunk_packing.alternative_context].tag_priority`
- `PromptContextSettings.chunk_packing.mechanism_explanation.source` <- `runtime.toml [prompt_context.chunk_packing.mechanism_explanation].source`
- `PromptContextSettings.chunk_packing.mechanism_explanation.limit` <- `runtime.toml [prompt_context.chunk_packing.mechanism_explanation].limit`
- `PromptContextSettings.chunk_packing.mechanism_explanation.fallback_to_any_chunk` <- `runtime.toml [prompt_context.chunk_packing.mechanism_explanation].fallback_to_any_chunk`
- `PromptContextSettings.chunk_packing.mechanism_explanation.tag_priority` <- `runtime.toml [prompt_context.chunk_packing.mechanism_explanation].tag_priority`

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
- the current runtime stage requires typed startup wiring that constructs the
  orchestrator dependency graph from top-level `Settings` through the dedicated
  `startup` module;
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

Optional paired CLI arguments:
- `--golden-cases-file`: path to one golden-cases JSON file
- `--golden-cases-schema`: path to the JSON schema used to validate that file

CLI rules:
- `--config` is required;
- `--ingest-config` is required;
- `--golden-cases-file` and `--golden-cases-schema` must be supplied together
  or omitted together;
- when both optional golden arguments are absent, the runtime must start in
  interactive mode;
- when both optional golden arguments are present, the runtime must start in
  golden-backed batch-eval mode;
- when exactly one optional golden argument is supplied, startup must fail
  before config loading or runtime wiring begins;
- invalid CLI arguments are a startup error;
- CLI startup must delegate config loading to the library-owned `config` module rather than parsing TOML or environment values inside `main.rs`;
- startup may load the repository-owned `.env` file through `dotenvy` before environment-based config merge begins;
- `.env` loading must happen exactly once on the startup path;
- if a required environment variable is absent after `.env` loading and environment merge, startup must fail before any later runtime initialization begins;
- the CLI must construct resolved `Settings` from the CLI-supplied config paths before any later runtime wiring is attempted;
- the runtime entry layer owns golden-file startup validation and typed parsing
  in golden-backed batch-eval mode;
- the runtime entry layer must delegate golden-file validation and parsing to
  the dedicated `golden_eval_input` module rather than implementing that logic
  inline in `main.rs` or `lib.rs`;
- in golden-backed batch-eval mode, the runtime entry layer must construct one
  `UserRequest` per validated golden-case item, using the values returned by
  `golden_eval_input`;
- in interactive mode, the runtime entry layer must construct
  `UserRequest { query, golden_question: None }`;
- in golden-backed batch-eval mode, the runtime entry layer must invoke the
  orchestrator once per returned `UserRequest`, in source-file order;
- each golden-backed batch item must create a separate run rather than being
  merged into one multi-question run;
- the current runtime stage may reuse one resolved `Settings` object, one
  observability runtime, and one built orchestrator instance across the whole
  batch;
- a per-request orchestrator failure in golden-backed batch mode is scoped to
  that run outcome and must not retroactively invalidate already completed
  earlier runs;
- the current runtime stage requires the CLI entry layer to execute either the
  interactive loop or golden-backed batch-eval run creation after settings have
  been loaded successfully.

CLI path rules:
- `--config` supplies the source path for the runtime config contract defined by `Specification/contracts/runtime/runtime_config.md`;
- `--ingest-config` supplies the source path for the ingest config contract defined by `Specification/contracts/runtime/ingest_config.md`;
- `--golden-cases-file` supplies the source path for one runtime-eval
  golden-cases JSON file;
- `--golden-cases-schema` supplies the source path for the JSON schema used to
  validate that golden-cases file;
- the environment contract defined by `Specification/contracts/runtime/env.md` is sourced from the standard repository `.env` file and the resulting process environment.

Golden-backed batch-eval startup rules:
- the runtime entry layer must validate the golden JSON file against the
  supplied schema before constructing any batch request;
- the validation and typed parsing steps must be implemented inside the
  dedicated `golden_eval_input` module;
- if the golden schema path does not exist, startup must fail before any run is
  created;
- if the golden file path does not exist, startup must fail before any run is
  created;
- if the golden JSON cannot be parsed as JSON, startup must fail before any run
  is created;
- if schema validation fails, startup must fail before any run is created;
- if typed parsing into the runtime-owned golden-case model fails after schema
  validation, startup must fail before any run is created;
- no partial batch execution is allowed after any startup-time golden input
  validation or parsing failure.

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
