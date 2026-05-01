## 1) Purpose / Scope

This document defines the runtime leaf-module contract for `query_structuring`.

This module exists to:
- accept the shared `NormalizedUserRequest`;
- load a prebuilt controlled-vocabulary JSON asset;
- load a prompt JSON asset;
- assemble the model prompt from the normalized query and controlled vocabulary;
- call the shared `ModelClient`;
- parse strict JSON returned by the model;
- return the shared `QueryStructuringOutput`.

This document is the source of truth for:
- the `query_structuring` leaf-module boundary;
- the module public interface;
- module-owned prompt assembly behavior;
- module-owned vocabulary-asset loading behavior;
- model-call behavior at this module boundary;
- the module-owned error boundary;
- the MVP output shape for incident-oriented user queries.

This document does not define:
- how the controlled vocabulary is produced from incident cards;
- PostgreSQL reads for incident-card terms;
- query-mode classification;
- conceptual or mixed-mode query structuring;
- downstream retrieval behavior;
- response-generation behavior after structuring.

Shared request and response types are defined by:
- `Specification/runtime/runtime.md`

Shared model-client behavior is defined by:
- `Specification/runtime/api_clients/model/model_client.md`

OpenInference span behavior for the context-aware execution path is defined by:
- `Specification/runtime/observability/open_inference_spans.md`

The generated Rust module file for the current version is:
- `src/request_pipeline/query_structuring.rs`

## 2) MVP Scope Limitation

The current module version supports only incident-oriented user queries.

For the MVP:
- the module assumes the incoming user query is an incident-investigation style request;
- the module must not implement separate conceptual-query or mixed-query structuring modes;
- prompt selection is fixed to one incident-oriented prompt asset;
- future query-mode classification belongs to a separate post-MVP layer.

## 3) Required Shared Types

This module must use the shared runtime types:
- `NormalizedUserRequest`
- `StructuredUserQuery`
- `QueryStructuringOutput`
- `QueryStructuringControlledVocabulary`
- `QueryStructuringMetrics`
- `Context`

These shared types are defined in:
- `Specification/runtime/runtime.md`

The current generated Rust runtime must define shared types equivalent in
ownership to:

```rust
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

Shared-type rules:
- `StructuredUserQuery` is the typed structured interpretation of one normalized user query;
- it is the domain payload produced inside `query_structuring`;
- it must not contain raw prompt text, raw model wire payloads, file paths, or module-private parsing metadata;
- it must not contain model token-usage metadata;
- `QueryStructuringOutput` is the shared cross-module output of `query_structuring`;
- `QueryStructuringOutput.structured_query` contains the domain result;
- `QueryStructuringOutput.token_usage` contains model-call token usage mapped from `ModelGenerationResponse`;
- `QueryStructuringOutput.metrics` contains request-local query-structuring
  metrics in the shared `QueryStructuringMetrics` shape when such metrics were
  computed for the current execution;
- `QueryStructuringOutput.metrics = None` is allowed when no matching golden
  query-structuring targets are available in the current execution context;
- `QueryStructuringControlledVocabulary` is the shared typed controlled-
  vocabulary asset shape used by this module and by the dedicated query-
  structuring metrics helper;
- `QueryStructuringMetrics` is the shared request-local metric bundle attached to
  `QueryStructuringOutput`;
- `Context` is the execution-time companion that carries request-local
  observability state and optional golden-eval metadata for context-aware
  execution;
- `ModelTokenUsage.prompt_tokens`, `completion_tokens`, and `total_tokens` remain optional because model providers may omit them;
- `StructuredUserQueryTerm.term` is the selected normalized term string returned by the model;
- `StructuredUserQueryTerm.evidence_span` is the short query-grounded evidence string returned by the model;
- `StructuredUserQuerySupportLevel` must be parsed from the model JSON string values:
  - `"explicit"`
  - `"strong_paraphrase"`
  - `"weak_inference"`
- `StructuredUserQueryConfidence` must be parsed from the model JSON string values:
  - `"low"`
  - `"medium"`
  - `"high"`

Import rule for the generated Rust module:
- shared output and metadata types used by this module, including
  `StructuredUserQuery`, `QueryStructuringOutput`,
  `QueryStructuringControlledVocabulary`, `QueryStructuringMetrics`,
  `ModelTokenUsage`, and `Context`, must be imported from
  `crate::shared_types`;
- `ModelFinishReason` must be imported through the canonical crate path `crate::api_clients::model::ModelFinishReason`.

## 4) Settings Dependency

This module must receive the typed settings slice:
- `QueryStructuringSettings`

`QueryStructuringSettings` is defined at the crate-level runtime boundary in:
- `Specification/runtime/runtime.md`

For the current version, `QueryStructuringSettings` contains exactly:

```rust
pub struct QueryStructuringSettings {
    pub controlled_vocabulary_path: String,
    pub prompt_asset_path: String,
    pub max_output_tokens: u32,
}
```

Rules:
- this module must receive `QueryStructuringSettings` through its constructor;
- this module must not read raw TOML or raw environment variables directly;
- this module must not redefine config-loading rules that belong to the `config` subsystem.

## 5) Module-Owned Asset Types

The module must load two module-owned JSON assets:
- a controlled vocabulary asset;
- a prompt asset.

The generated Rust module must use the shared controlled-vocabulary asset type
and define one module-owned prompt asset type equivalent in ownership to:

```rust
pub struct QueryStructuringPromptAsset {
    pub version: String,
    pub system_prompt: String,
    pub user_template: String,
}
```

Ownership rules:
- `QueryStructuringControlledVocabulary` is a shared runtime type defined in
  `crate::shared_types`;
- `QueryStructuringPromptAsset` remains module-private in the current version.

Vocabulary naming note:
- the structured output field `affected_subsystems` intentionally maps to the
  shared controlled-vocabulary field `affected_components`;
- this name mismatch is deliberate because the structured query uses a user-
  facing subsystem-oriented label while the shared vocabulary asset preserves
  the existing component-oriented source taxonomy.

## 6) Public Interface

The generated Rust module must define a public module boundary equivalent in
ownership to:

```rust
pub struct QueryStructuring {
    // implementation-owned fields
}

impl QueryStructuring {
    pub fn new(
        settings: QueryStructuringSettings,
        model_client: std::sync::Arc<dyn ModelClient>,
    ) -> Result<Self, QueryStructuringError>;

    pub async fn structure(
        &self,
        request: &NormalizedUserRequest,
    ) -> Result<QueryStructuringOutput, QueryStructuringError>;

    pub async fn structure_with_context(
        &self,
        request: &NormalizedUserRequest,
        context: &Context,
    ) -> Result<QueryStructuringOutput, QueryStructuringError>;
}
```

For the current version, the implementation-owned fields must contain exactly:
- `model_client: Arc<dyn ModelClient>`
- `controlled_vocabulary: QueryStructuringControlledVocabulary`
- `prompt_asset: QueryStructuringPromptAsset`
- `max_output_tokens: u32`

Rules:
- `new(...)` must load and validate both JSON assets from disk once and retain them for reuse;
- `structure(...)` must delegate to
  `structure_with_context(request, &Context::noop())`;
- `structure_with_context(...)` is the context-aware execution path used by the
  orchestrator;
- `structure_with_context(...)` must treat `context.open_inference.root_span` as
  the parent span for the module-owned OpenInference LLM span
  `oi.llm.query_structuring`;
- `structure_with_context(...)` must not reread asset files from disk on each
  request;
- `structure_with_context(...)` must call the shared `ModelClient`
  asynchronously;
- this module must store the model client behind `Arc<dyn ModelClient>`;
- the current version must not require callers to pass raw prompt strings or raw vocabulary JSON per request.

## 7) Constructor Rules

`QueryStructuring::new(settings, model_client)` must:
- validate that `controlled_vocabulary_path` is non-empty after trimming;
- validate that `prompt_asset_path` is non-empty after trimming;
- validate that `max_output_tokens > 0`;
- read the controlled vocabulary JSON file from disk at `controlled_vocabulary_path`;
- parse that JSON into `QueryStructuringControlledVocabulary`;
- read the prompt JSON file from disk at `prompt_asset_path`;
- parse that JSON into `QueryStructuringPromptAsset`;
- validate the loaded asset contents;
- retain both parsed assets and the supplied model client for reuse.

Constructor validation rules:
- `QueryStructuringControlledVocabulary` arrays must all be present;
- controlled-vocabulary arrays must not be empty in the current version;
- each controlled-vocabulary term string must be non-empty after trimming;
- duplicate controlled-vocabulary terms within the same field array are invalid
  and must be rejected after trimming;
- controlled-vocabulary validation must preserve the original left-to-right
  order of distinct accepted terms and must not sort the asset;
- the prompt asset must contain non-empty `version`, `system_prompt`, and `user_template`;
- `user_template` must contain exactly these placeholders:
  - `{{normalized_query}}`
  - `{{controlled_vocabulary_json}}`
- each required placeholder must appear exactly once;
- any additional `{{...}}` placeholder-like construct in `user_template` is an `InvalidPromptAsset` error;
- constructor failure caused by file reading or JSON parsing must surface through this module's typed error boundary;
- the current version must not defer asset loading until the first request.

## 8) Controlled Vocabulary Asset Rules

The current controlled vocabulary asset is a prebuilt JSON file supplied from
outside this module.

Rules:
- this module must treat the controlled vocabulary asset as read-only runtime input;
- this module must load the controlled vocabulary asset by reading the JSON file from the local filesystem path stored in `QueryStructuringSettings.controlled_vocabulary_path`;
- this module must not query PostgreSQL to rebuild or refresh the vocabulary;
- this module must not mutate, deduplicate, sort, or regenerate the asset contents at request time;
- this module may trust that the asset was pre-cleaned before runtime, but it must still validate presence, JSON shape, trimmed non-empty strings, and per-field uniqueness at construction time;
- equality against controlled-vocabulary terms is exact string equality after the
  constructor-time trimming validation described above.

The current controlled vocabulary JSON shape is:

```json
{
  "canonical_symptoms": ["string"],
  "affected_components": ["string"],
  "failure_mode_candidates": ["string"],
  "violated_properties": ["string"]
}
```

## 9) Prompt Asset Rules

The prompt must be stored as a JSON asset rather than hardcoded as one Rust
string constant.

The current prompt JSON shape is:

```json
{
  "version": "v2",
  "system_prompt": "string",
  "user_template": "string"
}
```

Rules:
- `system_prompt` is sent as the system message content exactly as loaded;
- `user_template` is the template used to build the single user message;
- the current version must not support prompt selection among multiple versions at runtime;
- the current version uses exactly one prompt asset loaded from `prompt_asset_path`;
- the prompt asset must be loaded by reading the JSON file from the local filesystem path stored in `QueryStructuringSettings.prompt_asset_path`;
- future prompt versioning may exist in the asset content, but this module must not switch behavior based on `version` in the current version beyond exposing it for diagnostics if needed internally.

## 10) Prompt Assembly Rules

`structure_with_context(request, context)` must assemble the model request in
the following order:

1. serialize the loaded `QueryStructuringControlledVocabulary` into compact JSON;
2. substitute `request.query` into the single placeholder position reserved for `{{normalized_query}}`;
3. substitute the compact controlled-vocabulary JSON into the single placeholder position reserved for `{{controlled_vocabulary_json}}`;
4. build exactly two model messages:
   - one `system` message from `prompt_asset.system_prompt`;
   - one `user` message from the substituted `user_template`.

Rules:
- prompt assembly must be deterministic;
- this module must not semantically rewrite `request.query`;
- this module must not reorder controlled-vocabulary categories;
- controlled-vocabulary JSON must be compact rather than pretty-printed in the current version;
- prompt assembly must not inject extra hidden fields or debug commentary into the model input.
- prompt assembly must use isolated placeholder substitution by placeholder position rather than naive repeated global string replacement over already-substituted text;
- user-provided query text must be treated as opaque data during substitution;
- placeholder substitution must not rescan inserted user text for additional placeholder patterns.

## 11) Metrics Attachment Rules

This module must compute request-local query-structuring metrics after
successfully parsing and validating the model output into `StructuredUserQuery`.

Rules:
- `structure_with_context(...)` must derive golden query-structuring targets
  only from `context.golden_question`;
- when `context.golden_question = Some(golden_question)`,
  `structure_with_context(...)` must call the dedicated helper
  `compute_query_structuring_metrics(...)` using:
  - the parsed `StructuredUserQuery`;
  - `golden_question.expected_query_structuring`;
  - the loaded `QueryStructuringControlledVocabulary`;
  - `request.query` as the raw user query string for grounding checks;
- when `context.golden_question = Some(golden_question)`, the returned
  `QueryStructuringOutput.metrics` must be `Some(computed_metrics)`;
- when `context.golden_question = None`, the returned
  `QueryStructuringOutput.metrics` must be `None`;
- the module must not attempt to synthesize golden targets when
  `context.golden_question = None`;
- the OpenInference input/output payload and LLM attribute semantics for
  `structure_with_context(...)` are owned by
  `Specification/runtime/observability/open_inference_spans.md`;
- helper failures from `compute_query_structuring_metrics(...)` must be wrapped
  into `QueryStructuringError` through the module's typed error boundary;
- failure of the helper computation itself is fatal for
  `structure_with_context(...)`;
- failure that occurs only while serializing or recording the already computed
  metric bundle into observability spans or events is non-fatal and must not
  change the returned `QueryStructuringOutput.metrics`.

## 12) Model Call Rules

This module must call the shared `ModelClient` trait defined by:
- `Specification/runtime/api_clients/model/model_client.md`

The current call must use:
- `response_mode = JsonObject`
- `temperature = 0.0`
- `max_output_tokens = settings.max_output_tokens`

Rules:
- the module must not select a different response mode in the current version;
- the module must not accept per-request temperature overrides;
- the module must not accept per-request output-token overrides;
- the module must use the configured `QueryStructuringSettings.max_output_tokens` value on every request;
- provider selection remains outside this module and belongs to upstream runtime wiring.

Stop-reason rules:
- the module must inspect the model finish reason before accepting the response as successful output;
- the module must read this reason from the shared `ModelGenerationResponse.finish_reason` field returned by `ModelClient`;
- for the current Together-compatible provider path, that shared `finish_reason` value originates from the raw provider field `choices[0].finish_reason`;
- for the current live-tested Together path used with `openai/gpt-oss-20b`, a normally completed usable response is expected to come back with `finish_reason = stop`;
- `finish_reason = length` must be treated as a token-limit truncation signal;
- when `finish_reason = length`, the module must fail with `InvalidModelOutput` and must not attempt to salvage the partial JSON;
- incomplete or truncated JSON must fail with `InvalidModelOutput`;
- if `finish_reason` is present and is anything other than `stop`, the current version must treat the response as `InvalidModelOutput`;
- if a future provider path omits `finish_reason`, the module may accept the response only if the content is fully parseable valid JSON and all business rules pass.

Example raw Together-compatible JSON response showing where the reason comes from:

```json
{
  "id": "resp_123",
  "object": "chat.completion",
  "created": 1710000000,
  "model": "openai/gpt-oss-20b",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "{\"intent\":\"diagnose incident cause around distributed locking behavior\",\"scenario\":\"A lock service appears to allow two workers to act as lock owners at the same time during network instability.\",\"symptoms\":[],\"affected_subsystems\":[],\"failure_modes\":[],\"system_properties\":[],\"entities\":[],\"constraints\":[],\"triggers\":[],\"observability_signals\":[],\"unresolved_terms\":[],\"rejected_nearby_terms\":[],\"confidence\":\"medium\"}"
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 1800,
    "completion_tokens": 220,
    "total_tokens": 2020
  }
}
```

Interpretation rules for the current module:
- `choices[0].message.content` becomes `ModelGenerationResponse.content`;
- `choices[0].finish_reason` becomes `ModelGenerationResponse.finish_reason`;
- `usage.prompt_tokens`, `usage.completion_tokens`, and `usage.total_tokens` map into the shared token-count fields returned by `ModelClient`.

Token-usage mapping rules at this module boundary:
- `ModelGenerationResponse.prompt_tokens` maps to `QueryStructuringOutput.token_usage.prompt_tokens`;
- `ModelGenerationResponse.completion_tokens` maps to `QueryStructuringOutput.token_usage.completion_tokens`;
- `ModelGenerationResponse.total_tokens` maps to `QueryStructuringOutput.token_usage.total_tokens`;
- successful output must preserve token-usage values exactly as returned by `ModelClient` without recomputation by this module.

## 13) Output Parsing And Mapping Rules

The module must treat the model output as strict JSON and parse it into
`StructuredUserQuery`, then wrap it into `QueryStructuringOutput`.

Required top-level JSON fields:
- `intent`
- `scenario`
- `symptoms`
- `affected_subsystems`
- `failure_modes`
- `system_properties`
- `entities`
- `constraints`
- `triggers`
- `observability_signals`
- `unresolved_terms`
- `rejected_nearby_terms`
- `confidence`

Field mapping rules:
- each of `symptoms`, `affected_subsystems`, `failure_modes`, and `system_properties`
  must parse as arrays of objects with:
  - `term`
  - `evidence_span`
  - `support_level`
- `entities`, `constraints`, `triggers`, `observability_signals`, and
  `unresolved_terms` must parse as arrays of strings;
- `rejected_nearby_terms` must parse as an array of objects with:
  - `term`
  - `reason`
- unknown extra JSON fields may be ignored in the current version after required
  fields are successfully parsed;
- duplicate rejected-nearby terms are allowed in the raw parsed output in the
  current version and are not normalized by this module.

MVP business rules:
- `failure_modes.len()` must be `<= 1`;
- `support_level = weak_inference` is allowed in raw output in the current
  version and must not be dropped by this module;
- `confidence` must parse into one of:
  - `low`
  - `medium`
  - `high`

Business-rule violation mapping rules:
- if parsed output violates `failure_modes.len() <= 1`, the module must return `QueryStructuringError::InvalidModelOutput`;
- in that case, `reason` must be exactly `"failure_modes must contain at most one item"`;
- that `InvalidModelOutput` value must preserve the token-usage and finish-reason metadata taken from the same model response.

Example model JSON response accepted by the current version:

```json
{
  "intent": "diagnose incident cause around distributed locking behavior",
  "scenario": "A lock service appears to allow two workers to act as lock owners at the same time during network instability.",
  "symptoms": [
    {
      "term": "duplicate_lock_holders",
      "evidence_span": "two workers act as if they both hold the same lock",
      "support_level": "strong_paraphrase"
    },
    {
      "term": "lost_updates",
      "evidence_span": "concurrent writes overwrite each other",
      "support_level": "strong_paraphrase"
    }
  ],
  "affected_subsystems": [
    {
      "term": "lock_service",
      "evidence_span": "lock service appears to allow two workers",
      "support_level": "explicit"
    },
    {
      "term": "key_value_api",
      "evidence_span": "concurrent writes overwrite each other",
      "support_level": "weak_inference"
    }
  ],
  "failure_modes": [
    {
      "term": "lock_ownership_violation",
      "evidence_span": "both hold the same lock",
      "support_level": "strong_paraphrase"
    }
  ],
  "system_properties": [
    {
      "term": "safe mutual exclusion for distributed locks",
      "evidence_span": "both hold the same lock",
      "support_level": "strong_paraphrase"
    }
  ],
  "entities": ["worker_a", "worker_b", "distributed_lock"],
  "constraints": ["symptom appears during network instability"],
  "triggers": ["network instability"],
  "observability_signals": ["conflicting write attempts", "simultaneous lock ownership behavior"],
  "unresolved_terms": [],
  "rejected_nearby_terms": [
    {
      "term": "lwt_split_brain",
      "reason": "not directly supported by the query text"
    }
  ],
  "confidence": "medium"
}
```

## 14) Error Boundary

This module must define a module-owned direct error type equivalent in
ownership to:

```rust
pub enum QueryStructuringError {
    InvalidConfig(String),
    AssetRead {
        path: String,
        message: String,
    },
    AssetParse {
        path: String,
        message: String,
    },
    InvalidPromptAsset(String),
    InvalidControlledVocabulary(String),
    Model(#[from] ModelClientError),
    MetricsComputation {
        message: String,
    },
    InvalidModelOutput {
        reason: String,
        token_usage: ModelTokenUsage,
        finish_reason: Option<ModelFinishReason>,
    },
}
```

Variant rules:
- `InvalidConfig`
  - covers invalid constructor settings such as empty asset paths or `max_output_tokens = 0`;
  - the payload is `String` rather than `&'static str` because the current
    runtime error type is serializable and deserializable;
- `AssetRead`
  - covers file-reading failures while reading prompt and vocabulary JSON files from disk;
- `AssetParse`
  - covers invalid JSON in prompt and vocabulary assets;
- `InvalidPromptAsset`
  - covers syntactically valid JSON that violates the prompt-asset contract;
  - the payload is `String` rather than `&'static str` because the current
    runtime error type is serializable and deserializable;
- `InvalidControlledVocabulary`
  - covers syntactically valid JSON that violates the controlled-vocabulary contract;
  - the payload is `String` rather than `&'static str` because the current
    runtime error type is serializable and deserializable;
- `Model(ModelClientError)`
  - wraps failures returned by the shared model client;
- `MetricsComputation`
  - wraps failures returned by the dedicated query-structuring metrics helper;
  - must preserve enough diagnostic detail to identify the failing metric input
    category and field when available;
- `InvalidModelOutput`
  - covers JSON parse failures, missing required fields, invalid enum values,
    invalid top-level shape, or unusable stop reasons returned by the model;
  - `reason` is `String` rather than `&'static str` because the current runtime
    error type is serializable and deserializable;
  - must preserve any available token-usage metadata from the model response;
  - must preserve the parsed `finish_reason` value when it was available from the model response.

Rules:
- this module must not flatten model or asset failures into string-only results;
- this module must wrap model-client failure through one typed dependency variant;
- this module must wrap query-structuring metrics-helper failures through
  `MetricsComputation` rather than flattening them into `InvalidModelOutput`;
- `InvalidModelOutput.token_usage` must use the same `ModelTokenUsage` shape as successful output;
- `InvalidModelOutput.finish_reason` must reflect the parsed `ModelGenerationResponse.finish_reason` when available;
- this module must not expose raw serde or filesystem error types through its public boundary.

## 14) Behavioral Invariants

The current version of this module must preserve all of the following invariants:

- identical `NormalizedUserRequest.query`, identical assets, and identical model response must produce identical `QueryStructuringOutput` output;
- the module must never read PostgreSQL directly;
- `structure(...)` must always send exactly one system message and one user message;
- the current version must always request JSON-object output from the model client;
- successful output must always parse into the shared typed `StructuredUserQuery` and wrap it into `QueryStructuringOutput`;
- the current version must preserve the raw model-selected field values after successful typed parsing rather than applying additional semantic normalization.

## 15) Testing Ownership

Unit-test ownership for runtime modules is defined by:
- `Specification/runtime/unit_tests.md`

Required unit-test cases for this module are owned by:
- `Specification/runtime/unit_tests.md`

## 16) Non-Goals

For the current version, this module must not:
- generate or refresh the vocabulary from PostgreSQL;
- classify the query into incident vs conceptual vs mixed modes;
- support conceptual-only or mixed-mode structuring;
- dynamically choose among multiple prompt assets;
- rerank or repair model-selected terms after successful parsing;
- own downstream retrieval or answer-generation logic.
