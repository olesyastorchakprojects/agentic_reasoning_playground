## 1) Purpose / Scope

This document defines the runtime leaf-module contract for
`incident_evidence_retrieval`.

This module exists to:
- accept the shared `CandidateCardRetrievalOutput`;
- accept the shared `NormalizedUserRequest`;
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
- PostgreSQL card hydration;
- prompt-time semantic bucketing such as `evidence_for_match`, `first_check_hint`, or `alternative_context`;
- chunk-ingest contracts or how `chunk_tags` are produced during ingest;
- prompt assembly or response generation.

Shared request and response types are defined by:
- `Specification/runtime/runtime.md`

Practice-chunk collection behavior is defined by:
- `Specification/runtime/api_clients/qdrant/practice_chunks_collection.md`

The generated Rust module file for the current version is:
- `src/request_pipeline/incident_evidence_retrieval.rs`

## 2) Required Shared Types

This module must use the shared runtime types:
- `NormalizedUserRequest`
- `CandidateCardRetrievalOutput`
- `IncidentEvidenceChunk`
- `IncidentEvidenceRetrievalOutput`

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
- `primary_chunks` and `alternative_chunks` encode retrieval-branch separation only and must not be interpreted as prompt-time semantic roles.

Import rule for the generated Rust module:
- shared input and output types used by this module, including `NormalizedUserRequest`, `CandidateCardRetrievalOutput`, `IncidentEvidenceChunk`, and `IncidentEvidenceRetrievalOutput`, must be imported from `crate::shared_types`;
- `PracticeChunksCollection` must be imported through `crate::api_clients::qdrant::...`;
- `NormalizedUserQuery` must be imported through `crate::api_clients::qdrant::...`.

Shared-type placement rule:
- before code generation for this module, `CandidateCard`, `CandidateCardRetrievalOutput`, `IncidentEvidenceChunk`, and `IncidentEvidenceRetrievalOutput` must exist in `src/shared_types.rs`.

## 3) Settings Dependency

This module must receive the typed settings slice:
- `CollectionRetrievalSettings`

`CollectionRetrievalSettings` is defined at the crate-level runtime boundary in:
- `Specification/runtime/runtime.md`

Rules:
- this module must receive the practice-chunks-specific `CollectionRetrievalSettings` slice through its constructor;
- this module must not read raw TOML or raw environment variables directly;
- `top_k` is the retrieval request limit passed through to both collection search calls;
- `score_threshold` is the retrieval score threshold passed through to both collection search calls;
- `top_k = 0` is invalid and the constructor must fail fast in the current version;
- the current version uses one shared `CollectionRetrievalSettings` value for both the primary search and the alternative search.

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
        settings: CollectionRetrievalSettings,
    ) -> Result<Self, IncidentEvidenceRetrievalError>;

    pub async fn retrieve(
        &self,
        request: &NormalizedUserRequest,
        candidates: &CandidateCardRetrievalOutput,
    ) -> Result<IncidentEvidenceRetrievalOutput, IncidentEvidenceRetrievalError>;
}
```

For the current version, the implementation-owned fields must contain exactly:
- `collection: Arc<dyn PracticeChunksCollection>`
- `settings: CollectionRetrievalSettings`

Rules:
- `new(...)` must retain the injected dependency and typed settings for reuse;
- `new(...)` must fail fast when `settings.top_k = 0`;
- `new(...)` constructor validation failures must be returned through `IncidentEvidenceRetrievalError` rather than through panic;
- `retrieve(...)` must be the single public request-time entrypoint of this module;
- `retrieve(...)` must not mutate the input `NormalizedUserRequest`;
- `retrieve(...)` must not mutate the input `CandidateCardRetrievalOutput`;
- `retrieve(...)` must not re-rank, deduplicate, or semantically reinterpret returned chunks.

Debug rules:
- `IncidentEvidenceRetrieval` must not derive `Debug` in the current version because `Arc<dyn PracticeChunksCollection>` does not implement `Debug`;
- the generated Rust module must provide a manual `impl Debug for IncidentEvidenceRetrieval`;
- the manual `Debug` output must omit internal trait-object state and may print only stable structural fields such as `settings`.

## 6) Hardcoded Tag Policy

For the current version, this module owns two hardcoded chunk-tag sets.

Primary-search tag set:
- `chunk_role:symptom`
- `chunk_role:impact`
- `chunk_role:timeline`
- `chunk_role:symptom_change`
- `chunk_role:investigation`
- `chunk_role:diagnostic_step`
- `chunk_role:hypothesis_update`
- `chunk_role:recovery`

Alternative-search tag set:
- `chunk_role:failure_mode`
- `chunk_role:root_cause`
- `chunk_role:contributing_factor`
- `chunk_role:uncertainty`
- `chunk_role:lesson`

Tag-policy rules:
- these tag sets must be hardcoded inside the module in the current version;
- these tag sets must not be loaded from runtime config in the current version;
- the primary-search call must use exactly the primary-search tag set;
- the alternative-search call must use exactly the alternative-search tag set.

## 7) Retrieval Behavior

This module performs at most two collection calls per request.

Primary-search rules:
- if `candidates.primary` is `Some(...)`, the module must issue one collection search;
- the primary search request must use exactly one requested card id:
  - `candidates.primary.case_id`
- the primary search request must use:
  - `user_query = NormalizedUserQuery(request.query.clone())`
  - `filter.case_ids = vec![candidates.primary.case_id.clone()]`
  - `filter.chunk_tags = primary-search tag set`
  - `limit = settings.top_k`
  - `score_threshold = settings.score_threshold`

Alternative-search rules:
- if `candidates.alternatives` is not empty, the module must issue one collection search;
- all alternative `case_id` values must be searched in one collection call in the current version;
- the alternative search request must use requested card ids in the same order as `candidates.alternatives[*].case_id`;
- the alternative search request must use:
  - `user_query = NormalizedUserQuery(request.query.clone())`
  - `filter.case_ids = candidates.alternatives[*].case_id`
  - `filter.chunk_tags = alternative-search tag set`
  - `limit = settings.top_k`
  - `score_threshold = settings.score_threshold`

General retrieval rules:
- if `candidates.primary` is `None`, the primary search must not be called;
- if `candidates.alternatives.is_empty()`, the alternative search must not be called;
- if both input branches are absent, the module must return a successful empty output without issuing any collection search;
- each executed search call returns at most `settings.top_k` chunks because the collection request limit must be set to `settings.top_k`;
- the module must not perform additional truncation after receiving successful collection results;
- the module must build collection-level `NormalizedUserQuery` from the unchanged `NormalizedUserRequest.query` string;
- the module must not paraphrase, tokenize, rewrite, or otherwise mutate `NormalizedUserRequest.query` before constructing `NormalizedUserQuery`;
- the module must not validate cross-branch uniqueness of candidate `case_id` values;
- the module must trust the supplied `CandidateCardRetrievalOutput`, even if the same `case_id` appears in both `primary` and `alternatives`;
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

Empty-input success rule:
- if the incoming candidates are:

```rust
CandidateCardRetrievalOutput {
    primary: None,
    alternatives: vec![],
}
```

then the module must return:

```rust
IncidentEvidenceRetrievalOutput {
    primary_chunks: vec![],
    alternative_chunks: vec![],
}
```

without issuing any collection search.

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
- `retrieve(...)` must return `IncidentEvidenceRetrievalError`;
- collection failures from either the primary search or the alternative search must be wrapped as `IncidentEvidenceRetrievalError::Collection`;
- if one search succeeds and the other fails, the whole module call must fail;
- partial success is forbidden in the current version.

## 10) Tests

Detailed unit-test requirements for this module must be defined in:
- `Specification/runtime/unit_tests.md`
