## 1) Purpose / Scope

This document defines the runtime leaf-module contract for
`incident_evidence_retrieval`.

This module exists to:
- accept the shared `IncidentEvidenceCardBranchesInput`;
- accept the shared `RetrievalQueryInput`;
- accept the shared `IterationProfile`;
- depend on the Qdrant-backed `PracticeChunksCollection` client through dependency injection;
- issue one practice-chunk search for the selected primary card when a primary card exists;
- issue one practice-chunk search for the selected alternative cards when alternative cards exist;
- return the shared `IncidentEvidenceRetrievalOutput`;
- preserve collection-returned hit order separately for `primary_chunks` and `alternative_chunks`.

This document is the source of truth for:
- the `incident_evidence_retrieval` leaf-module boundary;
- the module public interface;
- the module-owned primary and alternative practice-chunk retrieval behavior;
- the module-owned mapping from `PracticeChunkSearchHit` into shared runtime output;
- the module-owned error boundary.

This document does not define:
- semantic card retrieval from Qdrant;
- how `IncidentEvidenceCardBranchesInput` is produced upstream;
- PostgreSQL card hydration;
- prompt-time semantic bucketing such as `evidence_for_match`, `first_check_hint`, or `alternative_context`;
- chunk-ingest contracts or how `chunk_tags` are produced during ingest;
- prompt assembly or response generation.

Shared request and response types are defined by:
- `Specification/runtime/runtime.md`

Practice-chunk collection behavior is defined by:
- `Specification/runtime/api_clients/qdrant/practice_chunks_collection.md`

OpenInference span behavior for the context-aware execution path is defined by:
- `Specification/runtime/observability/open_inference_spans.md`

The generated Rust module file for the current version is:
- `src/request_pipeline/incident_evidence_retrieval.rs`

## 2) Required Shared Types

This module must use the shared runtime types:
- `RetrievalQueryInput`
- `IncidentEvidenceCardBranchesInput`
- `IterationProfile`
- `IncidentEvidenceChunk`
- `IncidentEvidenceRetrievalOutput`
- `IncidentEvidenceBranchRetrievalMetrics`
- `IncidentEvidenceRetrievalMetrics`

These shared types are defined in:
- `Specification/runtime/runtime.md`

The current generated Rust runtime must define shared types equivalent in
ownership to:

```rust
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
```

Shared-type rules:
- `IncidentEvidenceChunk` is the shared runtime representation of one retrieved practice chunk selected by `incident_evidence_retrieval`;
- `IncidentEvidenceChunk.case_id` is the domain identity of the source incident card linked to the chunk;
- `IncidentEvidenceChunk.case_id` must be mapped from `PracticeChunkSearchHit.case_id` in the current version;
- `IncidentEvidenceChunk.score` must preserve the raw `f32` value returned by the collection without rounding or normalization;
- `IncidentEvidenceChunk.chunk_tags` must preserve the raw collection-returned tags in original order;
- `IncidentEvidenceRetrievalOutput.primary_chunks` contains only hits returned by the primary search call;
- `IncidentEvidenceRetrievalOutput.alternative_chunks` contains only hits returned by the alternatives search call;
- `IncidentEvidenceRetrievalOutput` must preserve the separation between primary and alternative retrieval paths exactly;
- `primary_chunks` and `alternative_chunks` encode retrieval-branch separation only and must not be interpreted as prompt-time semantic roles;
- `IncidentEvidenceRetrievalOutput.metrics` contains request-local retrieval
  metrics in the shared `IncidentEvidenceRetrievalMetrics` shape when such
  metrics were computed for the current execution;
- `IncidentEvidenceRetrievalOutput.metrics = None` is allowed when no matching
  golden incident-evidence retrieval targets are available in the current
  execution context.

Import rule for the generated Rust module:
- shared input and output types used by this module, including `RetrievalQueryInput`, `IncidentEvidenceCardBranchesInput`, `IterationProfile`, `IncidentEvidenceChunk`, and `IncidentEvidenceRetrievalOutput`, must be imported from `crate::shared_types`;
- `IncidentEvidenceBranchRetrievalMetrics` and
  `IncidentEvidenceRetrievalMetrics` must be imported from
  `crate::shared_types` when request-local retrieval metrics are attached to
  the module output;
- retrieval metrics helper behavior is defined by
  `Specification/runtime/request_pipeline/retrieval_metrics.md`;
- `PracticeChunksCollection` must be imported through `crate::api_clients::qdrant::...`;
- `NormalizedUserQuery` must be imported through `crate::api_clients::qdrant::...`.

Shared-type placement rule:
- before code generation for this module, `RetrievalQueryInput`, `IncidentEvidenceCardBranchesInput`, `IterationProfile`, `IncidentEvidenceChunk`, and `IncidentEvidenceRetrievalOutput` must exist in `src/shared_types/mod.rs`.

## 3) Settings Dependency

This module must receive the typed settings slice:
- `IncidentEvidenceRetrievalSettings`

`IncidentEvidenceRetrievalSettings` is defined at the crate-level runtime boundary in:
- `Specification/runtime/runtime.md`

Rules:
- this module must receive the typed `IncidentEvidenceRetrievalSettings` slice through its constructor;
- this module must not read raw TOML or raw environment variables directly;
- `settings.retrieval.top_k` is the retrieval request limit passed through to both collection search calls;
- `settings.retrieval.score_threshold` is the retrieval score threshold passed through to both collection search calls;
- `settings.retrieval.top_k = 0` is invalid and the constructor must fail fast in the current version;
- the current version uses one shared `settings.retrieval` value for both the primary search and the alternative search;
- tag selection is profile-based and must be resolved from:
  - `settings.profiles.initial` when `iteration_profile = IterationProfile::Initial`
  - `settings.profiles.continuation` when `iteration_profile = IterationProfile::Continuation`

## 4) Collection Dependency

This module must depend on the Qdrant collection trait:
- `PracticeChunksCollection`

`PracticeChunksCollection` is defined in:
- `Specification/runtime/api_clients/qdrant/practice_chunks_collection.md`

The generated Rust module must store the dependency equivalent in ownership to:

```rust
std::sync::Arc<dyn PracticeChunksCollection>
```

Dependency rules:
- the module must accept `PracticeChunksCollection` through dependency injection;
- the module must not construct Qdrant clients itself;
- the module must reuse the injected collection for all evidence-retrieval requests.

## 5) Public Interface

The generated Rust module must define a public module boundary equivalent in
ownership to:

```rust
pub struct IncidentEvidenceRetrieval {
    // implementation-owned fields
}

impl IncidentEvidenceRetrieval {
    pub fn new(
        collection: std::sync::Arc<dyn PracticeChunksCollection>,
        settings: IncidentEvidenceRetrievalSettings,
    ) -> Result<Self, IncidentEvidenceRetrievalError>;

    pub async fn retrieve(
        &self,
        request: &RetrievalQueryInput,
        card_branches: &IncidentEvidenceCardBranchesInput,
        iteration_profile: IterationProfile,
    ) -> Result<IncidentEvidenceRetrievalOutput, IncidentEvidenceRetrievalError>;

    pub async fn retrieve_with_context(
        &self,
        request: &RetrievalQueryInput,
        card_branches: &IncidentEvidenceCardBranchesInput,
        iteration_profile: IterationProfile,
        context: &Context,
    ) -> Result<IncidentEvidenceRetrievalOutput, IncidentEvidenceRetrievalError>;
}
```

For the current version, the implementation-owned fields must contain exactly:
- `collection: Arc<dyn PracticeChunksCollection>`
- `settings: IncidentEvidenceRetrievalSettings`

Rules:
- `new(...)` must retain the injected dependency and typed settings for reuse;
- `new(...)` must fail fast when `settings.retrieval.top_k = 0`;
- `new(...)` constructor validation failures must be returned through `IncidentEvidenceRetrievalError` rather than through panic;
- `retrieve(...)` must delegate to
  `retrieve_with_context(request, card_branches, iteration_profile, &Context::noop())`;
- `retrieve_with_context(...)` is the context-aware request-time entrypoint used
  by the orchestrator;
- `retrieve_with_context(...)` must treat `context.open_inference.root_span` as
  the parent span for the module-owned OpenInference retriever spans
  `oi.retriever.incident_evidence.primary` and
  `oi.retriever.incident_evidence.alternatives`;
- when `IncidentEvidenceRetrievalOutput.metrics = Some(...)`,
  `retrieve_with_context(...)` must also emit the companion OpenInference
  metrics span `oi.chain.incident_evidence_retrieval_metrics` as defined by
  `Specification/runtime/observability/open_inference_spans.md`;
- `retrieve(...)` must not mutate the input `RetrievalQueryInput`;
- `retrieve(...)` must not mutate the input `IncidentEvidenceCardBranchesInput`;
- `retrieve(...)` must not re-rank, deduplicate, or semantically reinterpret returned chunks;
- when `context.golden_question = Some(...)`, `retrieve_with_context(...)` must
  compute retrieval metrics twice:
  - once for `primary_chunks` against
    `golden_question.expected_incident_evidence.primary_card_evidence_query.relevance_judgments`
  - once for `alternative_chunks` against
    `golden_question.expected_incident_evidence.alternative_cards_evidence_query.relevance_judgments`
- those two metric bundles must be attached as
  `IncidentEvidenceRetrievalOutput.metrics = Some(...)`;
- if either incident-evidence golden branch contains an empty
  `strict_chunk_ids` or `soft_chunk_ids` list, `retrieve_with_context(...)`
  must not call `compute_retrieval_metrics(...)` for incident evidence in the
  current version and must return `IncidentEvidenceRetrievalOutput.metrics = None`;
- when `context.golden_question = None`,
  `IncidentEvidenceRetrievalOutput.metrics` must be `None`.
- helper failures from `compute_retrieval_metrics(...)` must be wrapped into
  `IncidentEvidenceRetrievalError` and must fail the request-time call rather
  than being silently ignored.

Debug rules:
- `IncidentEvidenceRetrieval` must not derive `Debug` in the current version because `Arc<dyn PracticeChunksCollection>` does not implement `Debug`;
- the generated Rust module must provide a manual `impl Debug for IncidentEvidenceRetrieval`;
- the manual `Debug` output must omit internal trait-object state and may print only stable structural fields such as `settings`.

## 6) Profile-Based Tag Policy

For the current version, this module must select chunk-tag policy from
`IncidentEvidenceRetrievalSettings.profiles`.

Profile-selection rules:
- when `iteration_profile = IterationProfile::Initial`, the module must use:
  - `settings.profiles.initial.primary_tags`
  - `settings.profiles.initial.alternative_tags`
- when `iteration_profile = IterationProfile::Continuation`, the module must use:
  - `settings.profiles.continuation.primary_tags`
  - `settings.profiles.continuation.alternative_tags`
- before issuing any collection search, the module must resolve:
  - `selected_primary_tags`
  - `selected_alternative_tags`
  from exactly one selected profile determined by `iteration_profile`;
- the module must not hardcode tag sets in the current version;
- the module must not read tag configuration from any source other than the injected typed settings.

## 7) Retrieval Behavior

This module performs at most two collection calls per request.

Primary-search rules:
- the module must issue one primary-branch collection search for every valid call;
- the primary search request must use exactly one requested card id:
  - `card_branches.primary_card_id`
- the primary search request must use:
  - `user_query = NormalizedUserQuery(request.query_text.clone())`
  - `filter.case_ids = vec![card_branches.primary_card_id.clone()]`
  - `filter.chunk_tags = selected primary-tags profile`
  - `limit = settings.retrieval.top_k`
  - `score_threshold = settings.retrieval.score_threshold`

Alternative-search rules:
- if `card_branches.alternative_card_ids` is not empty, the module must issue one collection search;
- all alternative `case_id` values must be searched in one collection call in the current version;
- the alternative search request must use requested card ids in the same order as `card_branches.alternative_card_ids[*]`;
- the alternative search request must use:
  - `user_query = NormalizedUserQuery(request.query_text.clone())`
  - `filter.case_ids = card_branches.alternative_card_ids[*]`
  - `filter.chunk_tags = selected alternative-tags profile`
  - `limit = settings.retrieval.top_k`
  - `score_threshold = settings.retrieval.score_threshold`

General retrieval rules:
- if `card_branches.alternative_card_ids.is_empty()`, the alternative search must not be called;
- each executed search call returns at most `settings.retrieval.top_k` chunks because the collection request limit must be set to `settings.retrieval.top_k`;
- the module must not perform additional truncation after receiving successful collection results;
- the module must build collection-level `NormalizedUserQuery` from the unchanged `RetrievalQueryInput.query_text` string;
- request-local retrieval metrics, when computed, must use:
  - `actual_ranked_ids = primary_chunks[*].chunk_id` for the primary branch
  - `actual_ranked_ids = alternative_chunks[*].chunk_id` for the alternatives
    branch
- for the current contract, the module must call
  `compute_retrieval_metrics(...)` twice with:
  - normalized golden targets derived from
    `golden_question.expected_incident_evidence.primary_card_evidence_query.relevance_judgments`
    and `golden_question.expected_incident_evidence.alternative_cards_evidence_query.relevance_judgments`
  - branch-local `actual_ranked_ids`
  - `k = settings.retrieval.top_k`
- after both helper calls succeed, the module must attach:

```rust
IncidentEvidenceRetrievalMetrics {
    primary_card_evidence_query: IncidentEvidenceBranchRetrievalMetrics {
        relevance_judgments: primary_metrics,
    },
    alternative_cards_evidence_query: IncidentEvidenceBranchRetrievalMetrics {
        relevance_judgments: alternative_metrics,
    },
}
```
- the OpenInference input/output payload contracts for the primary and
  alternative retrieval branches are owned by
  `Specification/runtime/observability/open_inference_spans.md`;
- the module must not paraphrase, tokenize, rewrite, or otherwise mutate `RetrievalQueryInput.query_text` before constructing `NormalizedUserQuery`;
- `IncidentEvidenceCardBranchesInput.primary_card_id` must be non-empty for every valid call;
- if `card_branches.primary_card_id.trim().is_empty()`, the module must fail with `IncidentEvidenceRetrievalError::InvalidSettings` in the current version;
- the module must not validate cross-branch uniqueness of card ids in the current version;
- the module must trust the supplied `IncidentEvidenceCardBranchesInput`, even if the same card id appears in both branches;
- the module must not merge the two search calls into one combined collection request in the current version.

## 8) Output Mapping And Ordering Rules

This module maps collection hits into shared runtime output.

Mapping rules:
- `IncidentEvidenceChunk.chunk_id` <- `PracticeChunkSearchHit.chunk_id`
- `IncidentEvidenceChunk.case_id` <- `PracticeChunkSearchHit.case_id`
- `IncidentEvidenceChunk.score` <- `PracticeChunkSearchHit.score`
- `IncidentEvidenceChunk.chunk_tags` <- `PracticeChunkSearchHit.chunk_tags`
- `IncidentEvidenceChunk.text` <- `PracticeChunkSearchHit.text`

Case-id rules:
- successful module output must contain the exact `case_id` value returned by the collection hit;
- the module must not rewrite, drop, or substitute returned `case_id` values.

Ordering rules:
- `IncidentEvidenceRetrievalOutput.primary_chunks` must preserve the exact hit order returned by the primary search call;
- `IncidentEvidenceRetrievalOutput.alternative_chunks` must preserve the exact hit order returned by the alternative search call;
- the module must not deduplicate returned chunks;
- the module must not repartition alternative chunks by `case_id` in the current version.

There is no empty-primary success case in the current version.
If upstream cannot provide a valid non-empty `primary_card_id`, it must not
call this module.

## 9) Error Boundary

The generated Rust module must define a public error enum equivalent in
ownership to:

```rust
#[derive(Debug, thiserror::Error)]
pub enum IncidentEvidenceRetrievalError {
    #[error("invalid settings: {0}")]
    InvalidSettings(&'static str),
    #[error("practice chunks collection error: {0}")]
    Collection(PracticeChunksCollectionError),
}
```

Error rules:
- constructor validation failures such as `top_k = 0` must return `IncidentEvidenceRetrievalError::InvalidSettings`;
- request-time validation failure for empty `primary_card_id` must return `IncidentEvidenceRetrievalError::InvalidSettings`;
- `retrieve(...)` must return `IncidentEvidenceRetrievalError`;
- collection failures from either the primary search or the alternative search must be wrapped as `IncidentEvidenceRetrievalError::Collection`;
- if one search succeeds and the other fails, the whole module call must fail;
- partial success is forbidden in the current version.

## 10) Tests

Detailed unit-test requirements for this module must be defined in:
- `Specification/runtime/unit_tests.md`
