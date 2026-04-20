## 1) Purpose / Scope

This document defines the runtime leaf-module contract for `candidate_card_retrieval`.

This module exists to:
- accept the shared `NormalizedUserRequest`;
- depend on the collection-level `CardsCollection` trait through dependency injection;
- convert the normalized query text into a collection-level `CardSearchRequest`;
- call the selected cards collection implementation;
- map ordered retrieval hits into the shared `CandidateCard` type;
- partition the ordered candidate list into `primary` and `alternatives`;
- return the shared `CandidateCardRetrievalOutput`.

This document is the source of truth for:
- the `candidate_card_retrieval` leaf-module boundary;
- the module public interface;
- the module-owned partitioning behavior from ordered hits into `primary` and `alternatives`;
- the module-owned request construction for collection search;
- the module-owned error boundary.

This document does not define:
- dense or hybrid query construction;
- embedding generation;
- sparse query preparation;
- raw Qdrant transport behavior;
- cross-collection ranking;
- card hydration from PostgreSQL;
- downstream evidence retrieval or response generation.

Shared request and response types are defined by:
- `Specification/runtime/runtime.md`

Collection-level card-search behavior is defined by:
- `Specification/runtime/api_clients/qdrant/cards_collection.md`

Collection-level shared Qdrant types are defined by:
- `Specification/runtime/api_clients/qdrant/shared_types.md`

The generated Rust module file for the current version is:
- `src/request_pipeline/candidate_card_retrieval.rs`

## 2) Required Shared Types

This module must use the shared runtime types:
- `NormalizedUserRequest`
- `CandidateCard`
- `CandidateCardRetrievalOutput`

These shared types are defined in:
- `Specification/runtime/runtime.md`

The current generated Rust runtime must define shared types equivalent in
ownership to:

```rust
pub struct CandidateCard {
    pub case_id: String,
    pub score: f32,
}

pub struct CandidateCardRetrievalOutput {
    pub primary: Option<CandidateCard>,
    pub alternatives: Vec<CandidateCard>,
}
```

Shared-type rules:
- `CandidateCard` is the shared downstream-facing representation of one candidate incident card returned by retrieval;
- `CandidateCard` must contain only the fields needed across request-pipeline module boundaries in the current version;
- `CandidateCard` must not expose collection-layer request types, transport payloads, vector data, collection names, or module-private ranking metadata;
- `CandidateCard.case_id` is the canonical incident-card identifier selected from retrieval output;
- `CandidateCard.score` is the retrieval score returned by the collection layer for that card;
- `CandidateCardRetrievalOutput` is the shared cross-module output of `candidate_card_retrieval`;
- `CandidateCardRetrievalOutput.primary` contains the highest-ranked candidate card when retrieval returns at least one hit;
- `CandidateCardRetrievalOutput.primary` must be `None` when retrieval returns zero hits;
- `CandidateCardRetrievalOutput.alternatives` contains the remaining selected candidates in retrieval order after excluding the `primary` hit.

Import rule for the generated Rust module:
- shared output types used by this module, including `CandidateCard` and `CandidateCardRetrievalOutput`, must be imported from `crate::shared_types`;
- collection-layer card-search types and trait dependencies must be imported through `crate::api_clients::qdrant::...`;
- `NormalizedUserQuery` must be imported through `crate::api_clients::qdrant::...`.

## 3) Settings Dependency

This module must receive the typed settings slice:
- `CollectionRetrievalSettings`

`CollectionRetrievalSettings` is defined at the crate-level runtime boundary in:
- `Specification/runtime/runtime.md`

For the current version, `CollectionRetrievalSettings` contains exactly:

```rust
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
- this module must receive the cards-specific `CollectionRetrievalSettings` slice through its constructor;
- this module must not read raw TOML or raw environment variables directly;
- this module must not redefine config-loading rules that belong to the `config` subsystem;
- `top_k` is the retrieval request limit passed through to the collection-layer `CardSearchRequest`;
- `score_threshold` is the retrieval score threshold passed through to the collection-layer `CardSearchRequest`;
- `max_alternatives` defines the maximum number of returned `alternatives`;
- the module-owned collection search limit must be `top_k`;
- `max_alternatives` may be zero, in which case the module still requests enough hits for the `primary` candidate and must always return `alternatives = vec![]`;
- the current MVP output must contain at most three selected cards total across `primary` and `alternatives`;
- in the current version this module must be constructed from the cards retrieval settings slice, not from practice or theory retrieval settings.

## 4) Collection Dependency

This module must depend on the collection-level trait:
- `CardsCollection`

`CardsCollection` and its associated public types are defined in:
- `Specification/runtime/api_clients/qdrant/cards_collection.md`

The generated Rust module must store the dependency equivalent in ownership to:

```rust
std::sync::Arc<dyn CardsCollection>
```

Dependency rules:
- the module must accept a collection implementation through dependency injection;
- the module must depend on the trait boundary rather than on a concrete dense or hybrid collection type;
- the module must remain agnostic to whether the supplied implementation is dense or hybrid;
- the module must not own collection-construction logic;
- the module must not select between collection implementations at request time.

## 5) Public Interface

The generated Rust module must define a public module boundary equivalent in
ownership to:

```rust
pub struct CandidateCardRetrieval {
    // implementation-owned fields
}

impl CandidateCardRetrieval {
    pub fn new(
        settings: CollectionRetrievalSettings,
        cards_collection: std::sync::Arc<dyn CardsCollection>,
    ) -> Result<Self, CandidateCardRetrievalError>;

    pub async fn retrieve(
        &self,
        request: &NormalizedUserRequest,
    ) -> Result<CandidateCardRetrievalOutput, CandidateCardRetrievalError>;
}
```

For the current version, the implementation-owned fields must contain exactly:
- `cards_collection: Arc<dyn CardsCollection>`
- `top_k: usize`
- `max_alternatives: usize`
- `score_threshold: f32`

Rules:
- `new(...)` must validate constructor-owned settings and retain dependencies for reuse;
- `retrieve(...)` must be the single public request-time entrypoint of this module;
- `retrieve(...)` must not mutate the input request;
- `retrieve(...)` must not perform card hydration or any PostgreSQL reads;
- `retrieve(...)` must not expose collection-layer result types in its public return value.

Debug rules:
- `CandidateCardRetrieval` must not derive `Debug` in the current version because `Arc<dyn CardsCollection>` does not implement `Debug`;
- the generated Rust module must provide a manual `impl Debug for CandidateCardRetrieval`;
- the manual `Debug` output must omit internal trait-object state and may print only stable structural fields such as `top_k`, `max_alternatives`, and `score_threshold`.

## 6) Request Construction

When building the collection-level request, this module must construct a
`CardSearchRequest` equivalent in ownership to:

```rust
CardSearchRequest {
    user_query: NormalizedUserQuery(request.query.clone()),
    limit: top_k,
    score_threshold,
}
```

Request-construction rules:
- `CardSearchRequest.user_query` must be built from the unchanged `NormalizedUserRequest.query` string;
- the module must not paraphrase, tokenize, rewrite, or otherwise mutate `NormalizedUserRequest.query` before constructing `NormalizedUserQuery`;
- `CardSearchRequest.limit` must equal `self.top_k`;
- `CardSearchRequest.score_threshold` must equal `self.score_threshold`;
- `CardSearchRequest` must be the only collection-layer input created by this module in the current version.

## 7) Retrieval and Partitioning Rules

The module must call:
- `CardsCollection::search(&CardSearchRequest)`

The returned `CardSearchResult.hits` must be interpreted in the original order
provided by the collection layer.

Partitioning rules:
- if `hits` is empty, the module must return:

```rust
CandidateCardRetrievalOutput {
    primary: None,
    alternatives: vec![],
}
```

- if `hits` is non-empty, the first hit in order must become `primary`;
- `alternatives` must be built only from `hits[1..]`;
- all subsequent hits in order must become `alternatives`;
- `alternatives` must contain at most `max_alternatives` items;
- the total number of selected cards across `primary` and `alternatives` must not exceed three;
- the module must not reorder hits by score or by any secondary rule;
- the module must not deduplicate hits because the collection layer is already the source of ordered retrieval candidates;
- `primary`, when present, must never be duplicated inside `alternatives`.

Mapping rules:
- each `CardSearchHit` must map to one `CandidateCard`;
- `CandidateCard.case_id` <- `CardSearchHit.case_id`
- `CandidateCard.score` <- `CardSearchHit.score`
- `CandidateCard.score` must preserve the original `f32` retrieval score value without rounding, bucketing, normalization, or rescaling.

The current module behavior is intentionally simple:
- top-1 hit becomes `primary`;
- the remaining selected hits become `alternatives`;
- the module may request more than three hits from retrieval via `top_k`, but it must return at most three selected cards total;
- no additional score-band classification or thresholding beyond the collection request is performed inside this module.

## 8) Constructor Validation Rules

`new(...)` must fail with `CandidateCardRetrievalError::InvalidConfiguration`
when:
- `top_k == 0`;
- `top_k < 1 + max_alternatives`;
- `score_threshold` is negative;
- `score_threshold` is `NaN`;
- `score_threshold` is `+inf` or `-inf`;
- `max_alternatives > 2`.

Constructor validation rules:
- `max_alternatives = 0` is valid;
- constructor validation must happen once at module creation time rather than at each request when possible.

## 9) Error Boundary

The generated Rust module must define a public error enum equivalent in
ownership to:

```rust
#[derive(Debug, thiserror::Error)]
pub enum CandidateCardRetrievalError {
    #[error("invalid configuration: {0}")]
    InvalidConfiguration(&'static str),
    #[error("cards collection error: {0}")]
    Collection(CardsCollectionError),
}
```

Error rules:
- this module must return `CandidateCardRetrievalError` from both `new(...)` and `retrieve(...)`;
- collection search failures must be wrapped as `CandidateCardRetrievalError::Collection`;
- this module must not leak raw transport, embedding, or parser errors directly through its public boundary;
- zero-hit retrieval is not an error and must return a successful empty-output shape;
- the module must not create string-only catch-all runtime failures for collection errors when a typed child error exists.
