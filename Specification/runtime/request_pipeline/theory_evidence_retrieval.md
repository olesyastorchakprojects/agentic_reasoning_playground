## 1) Purpose / Scope

This document defines the runtime leaf-module contract for
`theory_evidence_retrieval`.

This module exists to:
- accept the shared `RetrievalQueryInput`;
- receive the theory retrieval policy through typed retrieval settings;
- depend on the Qdrant-backed `TheoryChunksCollection` client through dependency injection;
- issue one theory-chunk search against the theory corpus for the normalized query;
- return the shared `TheoryEvidenceRetrievalOutput`;
- preserve collection-returned hit order in `chunks`.

This document is the source of truth for:
- the `theory_evidence_retrieval` leaf-module boundary;
- the module public interface;
- the module-owned theory-chunk request construction behavior;
- the module-owned mapping from `TheoryChunkSearchHit` into shared runtime output;
- the module-owned error boundary.

This document does not define:
- query normalization;
- semantic card retrieval from Qdrant;
- PostgreSQL card hydration;
- practice or incident evidence retrieval;
- dense or hybrid query construction;
- embedding generation;
- sparse query preparation;
- raw Qdrant transport behavior;
- cross-collection ranking;
- prompt assembly or response generation.

Shared request and response types are defined by:
- `Specification/runtime/runtime.md`

Theory-chunk collection behavior is defined by:
- `Specification/runtime/api_clients/qdrant/theory_chunks_collection.md`

OpenInference span behavior for the context-aware execution path is defined by:
- `Specification/runtime/observability/open_inference_spans.md`

The generated Rust module file for the current version is:
- `src/request_pipeline/theory_evidence_retrieval.rs`

## 2) Required Shared Types

This module must use the shared runtime types:
- `RetrievalQueryInput`
- `TheoryEvidenceChunk`
- `TheoryEvidenceRetrievalOutput`
- `TheoryEvidenceRetrievalMetrics`

These shared types are defined in:
- `Specification/runtime/runtime.md`

The current generated Rust runtime must define shared types equivalent in
ownership to:

```rust
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
```

Shared-type rules:
- `TheoryEvidenceChunk` is the shared runtime representation of one retrieved theory chunk selected by `theory_evidence_retrieval`;
- `TheoryEvidenceChunk.score` must preserve the raw `f32` value returned by the collection without rounding, normalization, bucketing, or rescaling;
- `TheoryEvidenceChunk.text` must preserve the raw collection-returned text;
- `TheoryEvidenceRetrievalOutput.chunks` contains only hits returned by the theory search call;
- `TheoryEvidenceRetrievalOutput.chunks` must preserve collection-returned hit order exactly;
- `TheoryEvidenceRetrievalOutput` must not expose collection-layer request types, transport payloads, vector data, collection names, or module-private retrieval metadata;
- `TheoryEvidenceRetrievalOutput.metrics` contains request-local retrieval
  metrics in the shared `TheoryEvidenceRetrievalMetrics` shape when such
  metrics were computed for the current execution;
- `TheoryEvidenceRetrievalOutput.metrics = None` is allowed when no matching
  golden theory-evidence retrieval targets are available in the current
  execution context.

Import rule for the generated Rust module:
- shared input and output types used by this module, including `RetrievalQueryInput`, `TheoryEvidenceChunk`, and `TheoryEvidenceRetrievalOutput`, must be imported from `crate::shared_types`;
- `TheoryEvidenceRetrievalMetrics` must be imported from `crate::shared_types`
  when request-local retrieval metrics are attached to the module output;
- retrieval metrics helper behavior is defined by
  `Specification/runtime/request_pipeline/retrieval_metrics.md`;
- `CollectionRetrievalSettings` must be imported from `crate::config`;
- `TheoryChunksCollection` must be imported through `crate::api_clients::qdrant::...`;
- `TheoryChunksCollectionError` must be imported through `crate::api_clients::qdrant::...`;
- `TheoryChunkSearchRequest` must be imported through `crate::api_clients::qdrant::...`;
- `NormalizedUserQuery` must be imported through `crate::api_clients::qdrant::...`.

Shared-type placement rule:
- before code generation for this module, `RetrievalQueryInput`, `TheoryEvidenceChunk`, and `TheoryEvidenceRetrievalOutput` must exist in `src/shared_types/mod.rs`.

## 3) Settings Dependency

This module must receive the typed settings slice:
- `CollectionRetrievalSettings`

`CollectionRetrievalSettings` is defined at the crate-level runtime boundary in:
- `Specification/runtime/runtime.md`

Rules:
- this module must receive the theory-specific `CollectionRetrievalSettings` slice through its constructor;
- this module must not read raw TOML or raw environment variables directly;
- this module must not redefine config-loading rules that belong to the `config` subsystem;
- `top_k` is the retrieval request limit passed through to the theory collection search call;
- `score_threshold` is the retrieval score threshold passed through to the theory collection search call;
- `top_k = 0` is invalid and the constructor must fail fast in the current version;
- negative `score_threshold` is invalid and the constructor must fail fast in the current version;
- `NaN`, `+inf`, and `-inf` `score_threshold` values are invalid and the constructor must fail fast in the current version;
- the current version uses one `CollectionRetrievalSettings` value for the single theory search call;
- `max_alternatives` is not used by this module in the current version and must not affect theory retrieval behavior.

Retrieval-policy rules:
- the retrieval policy for this module is the theory-specific settings slice plus the selected injected collection implementation;
- the module must remain agnostic to whether the injected collection implementation is dense or hybrid;
- the module must not select between dense and hybrid collection implementations at request time;
- the module must not apply additional score thresholds, truncation rules, or ranking policies beyond the values passed into the collection-level request.

## 4) Collection Dependency

This module must depend on the Qdrant collection trait:
- `TheoryChunksCollection`

`TheoryChunksCollection` is defined in:
- `Specification/runtime/api_clients/qdrant/theory_chunks_collection.md`

The generated Rust module must store the dependency equivalent in ownership to:

```rust
std::sync::Arc<dyn TheoryChunksCollection + Send + Sync>
```

Dependency rules:
- the module must accept `TheoryChunksCollection` through dependency injection;
- the stored trait object must include `Send + Sync` bounds because `retrieve(...)` is async and awaits collection search through the injected dependency;
- the module must depend on the trait boundary rather than on a concrete dense or hybrid collection type;
- the module must not construct Qdrant clients itself;
- the module must not own collection-construction logic;
- the module must reuse the injected collection for all theory evidence retrieval requests.

## 5) Public Interface

The generated Rust module must define a public module boundary equivalent in
ownership to:

```rust
pub struct TheoryEvidenceRetrieval {
    // implementation-owned fields
}

impl TheoryEvidenceRetrieval {
    pub fn new(
        collection: std::sync::Arc<dyn TheoryChunksCollection + Send + Sync>,
        settings: CollectionRetrievalSettings,
    ) -> Result<Self, TheoryEvidenceRetrievalError>;

    pub async fn retrieve(
        &self,
        request: &RetrievalQueryInput,
    ) -> Result<TheoryEvidenceRetrievalOutput, TheoryEvidenceRetrievalError>;

    pub async fn retrieve_with_context(
        &self,
        request: &RetrievalQueryInput,
        context: &Context,
    ) -> Result<TheoryEvidenceRetrievalOutput, TheoryEvidenceRetrievalError>;
}
```

For the current version, the implementation-owned fields must contain exactly:
- `collection: Arc<dyn TheoryChunksCollection + Send + Sync>`
- `settings: CollectionRetrievalSettings`

Rules:
- `new(...)` must retain the injected dependency and typed settings for reuse;
- `new(...)` must fail fast when `settings.top_k = 0`;
- `new(...)` must fail fast when `settings.score_threshold` is negative, `NaN`, `+inf`, or `-inf`;
- `new(...)` constructor validation failures must be returned through `TheoryEvidenceRetrievalError` rather than through panic;
- `retrieve(...)` must delegate to
  `retrieve_with_context(request, &Context::noop())`;
- `retrieve_with_context(...)` is the context-aware request-time entrypoint used
  by the orchestrator;
- `retrieve_with_context(...)` must treat `context.open_inference.root_span` as
  the parent span for the module-owned OpenInference retriever span
  `oi.retriever.theory_evidence`;
- when `TheoryEvidenceRetrievalOutput.metrics = Some(...)`,
  `retrieve_with_context(...)` must also emit the companion OpenInference
  metrics span `oi.chain.theory_evidence_retrieval_metrics` as defined by
  `Specification/runtime/observability/open_inference_spans.md`;
- `retrieve(...)` must not mutate the input `RetrievalQueryInput`;
- `retrieve(...)` must not expose collection-layer result types in its public return value;
- when `context.golden_question = Some(...)`, `retrieve_with_context(...)` must
  compute retrieval metrics against
  `golden_question.expected_theory_evidence.mechanism_explanation` and set
  `TheoryEvidenceRetrievalOutput.metrics = Some(...)`;
- if `golden_question.expected_theory_evidence.mechanism_explanation`
  contains an empty `strict_chunk_ids` or `soft_chunk_ids` list,
  `retrieve_with_context(...)` must not call `compute_retrieval_metrics(...)`
  and must return `TheoryEvidenceRetrievalOutput.metrics = None`;
- when `context.golden_question = None`, `TheoryEvidenceRetrievalOutput.metrics`
  must be `None`.
- helper failures from `compute_retrieval_metrics(...)` must be wrapped into
  `TheoryEvidenceRetrievalError` and must fail the request-time call rather
  than being silently ignored.

Debug rules:
- `TheoryEvidenceRetrieval` must not derive `Debug` in the current version because `Arc<dyn TheoryChunksCollection + Send + Sync>` does not implement `Debug`;
- the generated Rust module must provide a manual `impl Debug for TheoryEvidenceRetrieval`;
- the manual `Debug` output must omit internal trait-object state and may print only stable structural fields such as `settings`.

## 6) Request Construction

When building the collection-level request, this module must construct a
`TheoryChunkSearchRequest` equivalent in ownership to:

```rust
TheoryChunkSearchRequest {
    user_query: NormalizedUserQuery(request.query_text.clone()),
    limit: settings.top_k,
    score_threshold: settings.score_threshold,
}
```

Request-construction rules:
- `TheoryChunkSearchRequest.user_query` must be built from the unchanged `RetrievalQueryInput.query_text` string;
- the module must not paraphrase, tokenize, rewrite, trim, or otherwise mutate `RetrievalQueryInput.query_text` before constructing `NormalizedUserQuery`;
- `TheoryChunkSearchRequest.limit` must equal `settings.top_k`;
- `TheoryChunkSearchRequest.score_threshold` must equal `settings.score_threshold`;
- `TheoryChunkSearchRequest` must be the only collection-layer input created by this module in the current version.

## 7) Retrieval Behavior

This module performs exactly one collection call per successful request-time
`retrieve(...)` invocation.

Retrieval rules:
- the module must call `TheoryChunksCollection::search(&TheoryChunkSearchRequest)`;
- the collection call must use the request constructed from the normalized query and theory retrieval settings;
- a successful empty collection result is not an error;
- the OpenInference input/output payload contract for
  `retrieve_with_context(...)` is owned by
  `Specification/runtime/observability/open_inference_spans.md`;
- if the collection returns zero hits, the module must return:

```rust
TheoryEvidenceRetrievalOutput {
    chunks: vec![],
    metrics: None,
}
```

- each executed search call returns at most `settings.top_k` chunks because the collection request limit must be set to `settings.top_k`;
- the module must not perform additional truncation after receiving successful collection results;
- the module must not re-rank, deduplicate, merge, or semantically reinterpret returned chunks;
- the module must not inspect or depend on candidate cards, hydrated cards, practice chunks, incident evidence, or prompt context.
- request-local retrieval metrics, when computed, must use:
  - `actual_ranked_ids = chunks[*].chunk_id`
- for the current contract, the module must call
  `compute_retrieval_metrics(...)` with:
  - normalized golden targets derived from
    `golden_question.expected_theory_evidence.mechanism_explanation`
  - `actual_ranked_ids`
  - `k = settings.top_k`
- after helper success, the module must attach:

```rust
TheoryEvidenceRetrievalMetrics {
    mechanism_explanation: computed_metrics,
}
```

Independence rules:
- this module must not accept `CandidateCardRetrievalOutput`;
- this module must not accept `CardHydrationOutput`;
- this module must not accept `IncidentEvidenceRetrievalOutput`;
- this module must not use incident `case_id` values;
- theory evidence retrieval must be able to return successful theory evidence even when no incident card candidates exist elsewhere in the pipeline.

## 8) Output Mapping And Ordering Rules

This module maps collection hits into shared runtime output.

Mapping rules:
- `TheoryEvidenceChunk.chunk_id` <- `TheoryChunkSearchHit.chunk_id`
- `TheoryEvidenceChunk.score` <- `TheoryChunkSearchHit.score`
- `TheoryEvidenceChunk.text` <- `TheoryChunkSearchHit.text`

Ordering rules:
- `TheoryEvidenceRetrievalOutput.chunks` must preserve the exact hit order returned by the theory search call;
- the module must not reorder hits by score or by any secondary rule;
- the module must not deduplicate returned chunks;
- the module must not attach module-invented metadata to returned chunks.

## 9) Error Boundary

The generated Rust module must define a public error enum equivalent in
ownership to:

```rust
#[derive(Debug, thiserror::Error)]
pub enum TheoryEvidenceRetrievalError {
    #[error("invalid settings: {0}")]
    InvalidSettings(&'static str),
    #[error("theory chunks collection error: {0}")]
    Collection(TheoryChunksCollectionError),
}
```

Error rules:
- constructor validation failures such as `top_k = 0` or invalid `score_threshold` must return `TheoryEvidenceRetrievalError::InvalidSettings`;
- `retrieve(...)` must return `TheoryEvidenceRetrievalError`;
- collection failures must be wrapped as `TheoryEvidenceRetrievalError::Collection`;
- the `Collection` variant must not use `#[from]`;
- collection errors must be explicitly wrapped with `.map_err(TheoryEvidenceRetrievalError::Collection)`;
- collection errors must implement `std::fmt::Display` as required by `thiserror::Error`; this is guaranteed by `Specification/runtime/api_clients/qdrant/theory_chunks_collection.md` section `10) Error Model`;
- empty collection results are not errors;
- partial success behavior is not applicable in the current version because the module performs only one collection call.

## 10) Tests

Detailed unit-test requirements for this module must be defined in:
- `Specification/runtime/unit_tests.md`
