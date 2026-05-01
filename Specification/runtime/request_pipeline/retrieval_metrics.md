## 1) Purpose / Scope

This document defines the retrieval-quality metric helper contract for the
runtime crate.

It defines:

- the metric inputs;
- the metric formulas;
- the rank and ordering rules;
- the required shared output metric bundle shape at the semantic level.

This document does not define:

- CLI behavior;
- orchestration ownership;
- observability attribute names;
- dataset-level reporting aggregates such as mean reciprocal rank across many
  requests;
- mapping from concrete module outputs into ranked id lists.

The generated Rust helper artifact for the current version is:

- `src/request_pipeline/retrieval_metrics.rs`

Module-placement rule:

- the retrieval metrics helper must live in a dedicated internal Rust module of
  the generated runtime crate;
- the helper must not be inlined into `candidate_card_retrieval`,
  `incident_evidence_retrieval`, `theory_evidence_retrieval`, or orchestrator
  module bodies;
- unit tests for the helper must live in that helper module under its own
  `#[cfg(test)]` test module.

## 2) Role Of The Retrieval Metrics Helper

The retrieval metrics helper is a runtime utility owned by the runtime crate.

Its purpose is:

- to compute request-local retrieval-quality metrics from one typed golden
  retrieval target set, one ordered ranked id list emitted by the current
  retrieval stage, and one effective top-k cutoff;
- to provide one source of truth for retrieval metric formulas shared across
  candidate-card retrieval, incident-evidence retrieval, and theory-evidence
  retrieval;
- to keep retrieval-quality metric computation consistent across retrieval
  boundaries.

The helper computes per-request metrics only.
It does not compute cross-request aggregates.
The helper may reject invalid metric inputs.

The helper must expose one internal callable entrypoint semantically equivalent
to:

```rust
pub(crate) fn compute_retrieval_metrics(
    golden_targets: &GoldenRetrievalTargetsById,
    actual_ranked_ids: &[String],
    k: usize,
) -> Result<RetrievalEvaluationMetrics, RetrievalMetricsError>;
```

Interface rules:

- the helper entrypoint is internal to the crate and must not become a separate
  public API surface;
- the helper is id-agnostic:
  - the same helper contract applies to candidate-card retrieval ids and chunk
    retrieval ids;
- concrete retrieval modules own the mapping from their outputs into
  `actual_ranked_ids`;
- concrete retrieval modules also own the normalization from shared typed golden
  retrieval targets into the helper's internal id-based target shape;
- `k` is the stage-owned effective top-k cutoff supplied by the calling module;
- for the current contract, calling retrieval modules must pass their configured
  stage `top_k` as `k`.
- if `k = 0`, the helper must return `RetrievalMetricsError::InvalidK`.

## 3) Shared Types And Inputs

The helper consumes:

- one `GoldenRetrievalTargetsById`;
- one ordered list of actual ids representing the emitted ranked output of the
  current stage;
- one positive integer `k`.

The helper uses the following shared runtime types:

- `GoldenCardRetrievalTargets`
- `GoldenChunkRetrievalTargets`
- `RetrievalEvaluationMetrics`
- `CandidateCardRetrievalMetrics`
- `IncidentEvidenceBranchRetrievalMetrics`
- `IncidentEvidenceRetrievalMetrics`
- `TheoryEvidenceRetrievalMetrics`

These shared types are defined by:

- `Specification/runtime/runtime.md`

The helper must define one internal normalized target type equivalent in
ownership to:

```rust
pub(crate) struct GoldenRetrievalTargetsById {
    pub strict_positive_ids: Vec<String>,
    pub soft_positive_ids: Vec<String>,
    pub graded_relevance: Vec<GoldenRetrievalRelevanceById>,
}

pub(crate) struct GoldenRetrievalRelevanceById {
    pub id: String,
    pub score: f32,
}
```

Normalization rules:

- `GoldenCardRetrievalTargets` must be normalized into
  `GoldenRetrievalTargetsById` by mapping:
  - `strict_card_ids -> strict_positive_ids`
  - `soft_card_ids -> soft_positive_ids`
  - `GoldenCardRelevance.card_id -> GoldenRetrievalRelevanceById.id`
- `GoldenChunkRetrievalTargets` must be normalized into
  `GoldenRetrievalTargetsById` by mapping:
  - `strict_chunk_ids -> strict_positive_ids`
  - `soft_chunk_ids -> soft_positive_ids`
  - `GoldenChunkRelevance.chunk_id -> GoldenRetrievalRelevanceById.id`
- the helper itself must operate only on the normalized id-based shape;
- `GoldenRetrievalTargetsById` and `GoldenRetrievalRelevanceById` are internal
  helper-owned normalization types and must not be promoted to shared runtime
  types in `src/shared_types.rs`.

Input rules:

- `actual_ranked_ids` must follow the exact emitted rank order of the current
  stage output;
- ids in `actual_ranked_ids` are opaque strings to the helper;
- `k` is the effective top-k cutoff of the current stage;
- soft relevance comes from `GoldenRetrievalTargetsById.soft_positive_ids`;
- strict relevance comes from `GoldenRetrievalTargetsById.strict_positive_ids`;
- graded relevance comes from `GoldenRetrievalTargetsById.graded_relevance`;
- for the current contract, `strict_positive_ids` and `soft_positive_ids` are
  required non-empty sets;
- if `strict_positive_ids` or `soft_positive_ids` is empty, the helper must
  reject the input as invalid rather than synthesizing metric values.
- this helper contract intentionally remains stricter than the current JSON
  schema for golden cases;
- calling retrieval modules must guard against empty target sets before calling
  the helper and must set `metrics = None` when the matching golden target set
  is empty.

Golden-target validity defaults:

- each `strict_positive_ids` list must be interpreted as a set of unique ids;
- each `soft_positive_ids` list must be interpreted as a set of unique ids;
- each `graded_relevance` list must contain at most one entry per id;
- duplicate ids in `strict_positive_ids` are invalid input and must be rejected;
- duplicate ids in `soft_positive_ids` are invalid input and must be rejected;
- duplicate `graded_relevance.id` entries are invalid input and must be
  rejected;
- `StrictRel` must be a subset of `SoftRel`; if not, the helper must reject the
  input as invalid;
- graded relevance scores must be exactly one of:
  - `0.0`
  - `0.5`
  - `1.0`
- every id in `StrictRel` must appear in `graded_relevance` with score exactly
  `1.0`;
- every id in `SoftRel - StrictRel` must appear in `graded_relevance` with score
  exactly `0.5`;
- additional ids may appear in `graded_relevance` with score `0.0`;
- every id with score `1.0` must belong to `StrictRel`;
- every id with score `0.5` must belong to `SoftRel - StrictRel`;
- every id with score `0.0` must not belong to `SoftRel`;
- leading and trailing whitespace in ids must be trimmed before validation; if
  trimming yields an empty string, the helper must reject the input as invalid.

Normalization and validation order rules:

- calling modules must normalize typed golden targets into
  `GoldenRetrievalTargetsById` before calling the helper entrypoint;
- trimming, duplicate detection, subset validation, and graded-relevance
  consistency checks must be applied to the normalized id-based shape;
- the helper must reject invalid normalized targets rather than silently
  repairing them.

## 4) Effective Top-k Set And Order

Let:

- `ActualTopK` be the first `k` ids in the current stage output rank order.

Rank rules:

- ranks are `1`-based;
- the first item in `ActualTopK` has rank `1`;
- the second item in `ActualTopK` has rank `2`, and so on.

Duplicate-handling rules:

- let `ActualTopK_dedup` be the ordered prefix of unique ids obtained by
  scanning `ActualTopK` from left to right and keeping only the first
  occurrence of each id;
- recall, reciprocal rank, relevant counts, DCG, IDCG, and nDCG must be
  computed against `ActualTopK_dedup`, not against the raw duplicated list;
- later duplicate occurrences must not increase recall, reciprocal rank,
  relevant counts, or DCG.

## 5) Recall@k

### 5.1) Soft Recall@k

Let:

- `Rel_soft` be the set of all ids from
  `GoldenRetrievalTargetsById.soft_positive_ids`;
- `TopK_set` be the set of ids in `ActualTopK_dedup`.

Then:

`recall_soft = |Rel_soft ∩ TopK_set| / |Rel_soft|`

### 5.2) Strict Recall@k

Let:

- `Rel_strict` be the set of all ids from
  `GoldenRetrievalTargetsById.strict_positive_ids`;
- `TopK_set` be the set of ids in `ActualTopK_dedup`.

Then:

`recall_strict = |Rel_strict ∩ TopK_set| / |Rel_strict|`

Recall rules:

- recall is request-local;
- recall depends only on which relevant ids appear in the top-k prefix, not on
  their internal order.

## 6) Reciprocal Rank@k

This document defines per-request reciprocal rank.
It does not define mean reciprocal rank across many requests.

### 6.1) Soft Reciprocal Rank@k

Let:

- `rank_soft_first` be the `1`-based rank of the first id in `ActualTopK_dedup`
  whose value belongs to `Rel_soft`.

Then:

- if such an id exists:
  - `rr_soft = 1 / rank_soft_first`
- otherwise:
  - `rr_soft = 0`

### 6.2) Strict Reciprocal Rank@k

Let:

- `rank_strict_first` be the `1`-based rank of the first id in
  `ActualTopK_dedup` whose value belongs to `Rel_strict`.

Then:

- if such an id exists:
  - `rr_strict = 1 / rank_strict_first`
- otherwise:
  - `rr_strict = 0`

Derived rank outputs:

- `first_relevant_rank_soft` is `Some(rank_soft_first)` when a soft-relevant id
  exists in `ActualTopK_dedup`, otherwise `None`;
- `first_relevant_rank_strict` is `Some(rank_strict_first)` when a
  strict-relevant id exists in `ActualTopK_dedup`, otherwise `None`.

## 7) Relevant Count@k

### 7.1) Soft Relevant Count@k

`num_relevant_soft = |Rel_soft ∩ TopK_set|`

### 7.2) Strict Relevant Count@k

`num_relevant_strict = |Rel_strict ∩ TopK_set|`

Count rules:

- counts are computed over ids in `ActualTopK_dedup`;
- duplicate ids must not increase the count.

## 8) nDCG@k

The graded ranking metric for the current version is nDCG@k.

### 8.1) Graded Relevance Lookup

Let:

- `grade(id)` be the graded relevance score looked up from
  `GoldenRetrievalTargetsById.graded_relevance`;
- if an id is not present in `graded_relevance`, its graded relevance is `0.0`.

### 8.2) DCG@k

Let:

- `rel_i` be the graded relevance of the id at rank `i` in `ActualTopK_dedup`.

Then:

`dcg_at_k = sum_{i=1..|ActualTopK_dedup|} rel_i / log2(i + 1)`

### 8.3) IDCG@k

Let:

- `IdealTopK` contain up to `k` unique ids with the highest graded relevance
  scores from `GoldenRetrievalTargetsById.graded_relevance`, considering only
  ids whose grade is strictly greater than `0.0`, sorted by:
  - descending `score`
  - then ascending lexical `id` as the deterministic tie-break rule.

Let:

- `ideal_rel_i` be the graded relevance at rank `i` in `IdealTopK`.

Then:

`idcg_at_k = sum_{i=1..|IdealTopK|} ideal_rel_i / log2(i + 1)`

### 8.4) nDCG@k

Then:

- if `idcg_at_k > 0`:
  - `ndcg = dcg_at_k / idcg_at_k`
- otherwise:
  - `ndcg = 0`

## 9) Output Semantics

The helper returns one per-request metric bundle in the shared
`RetrievalEvaluationMetrics` type defined by:

- `Specification/runtime/runtime.md`

The shared runtime must define retrieval metric types equivalent in ownership
to:

```rust
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
```

Output rules:

- `evaluated_k` is the effective stage-owned top-k value supplied to the
  helper;
- metric values must be deterministic for the same inputs;
- this helper must not produce cross-request mean values;
- this helper must not rename per-request reciprocal rank into MRR.

Shared-wrapper rules:

- `RetrievalEvaluationMetrics` is the reusable shared per-call metric bundle;
- `CandidateCardRetrievalMetrics`, `IncidentEvidenceBranchRetrievalMetrics`,
  `IncidentEvidenceRetrievalMetrics`, and `TheoryEvidenceRetrievalMetrics`
  are the shared module-facing attachment wrappers used by the corresponding
  retrieval module outputs;
- the helper entrypoint itself returns only `RetrievalEvaluationMetrics`;
- construction of module-specific shared wrapper structs belongs to the calling
  retrieval modules, not to the helper.

## 10) Error Model

The helper must define one internal error type for retrieval-metric computation
failures.

The generated Rust helper must define an internal error type equivalent in
ownership to:

```rust
pub(crate) enum RetrievalMetricsError {
    InvalidGoldenTargets { reason: String },
    InvalidK { reason: String },
    InconsistentGradedRelevance { reason: String },
    UnexpectedComputationState { reason: String },
}
```

Required failure categories:

- invalid golden retrieval target shape for metric computation;
- invalid effective top-k metric input;
- inconsistent graded relevance state;
- unexpected internal metric computation state.

Error-model rules:

- helper failures are internal utility failures;
- helper failures must not become an independent public pipeline error domain;
- `InvalidK` must be returned when `k = 0`;
- candidate-card retrieval wraps helper failures into
  `CandidateCardRetrievalError`;
- incident-evidence retrieval wraps helper failures into
  `IncidentEvidenceRetrievalError`;
- theory-evidence retrieval wraps helper failures into
  `TheoryEvidenceRetrievalError`.

## 11) Module Attachment Rules

The retrieval helper is reused by three retrieval boundaries.

Attachment rules:

- `candidate_card_retrieval` must compute retrieval metrics against
  `GoldenCandidateCardSection.retrieval_relevant_cards` and attach:
  - `metrics: Option<CandidateCardRetrievalMetrics>`
- if `GoldenCandidateCardSection.retrieval_relevant_cards.strict_card_ids` or
  `.soft_card_ids` is empty, `candidate_card_retrieval` must not call the
  helper and must attach `metrics = None`
- `incident_evidence_retrieval` must compute retrieval metrics twice:
  - once for `primary_chunks` against
    `GoldenIncidentEvidenceTargets.primary_card_evidence_query.relevance_judgments`
  - once for `alternative_chunks` against
    `GoldenIncidentEvidenceTargets.alternative_cards_evidence_query.relevance_judgments`
  - and attach:
    `metrics: Option<IncidentEvidenceRetrievalMetrics>`
- if either incident-evidence branch has an empty `strict_chunk_ids` or
  `soft_chunk_ids` target set, that branch-local helper call must be skipped
  and `incident_evidence_retrieval` must attach `metrics = None` for the whole
  module output in the current version;
- `theory_evidence_retrieval` must compute retrieval metrics against
  `GoldenTheoryEvidenceTargets.mechanism_explanation` and attach:
  - `metrics: Option<TheoryEvidenceRetrievalMetrics>`
- if `GoldenTheoryEvidenceTargets.mechanism_explanation.strict_chunk_ids` or
  `.soft_chunk_ids` is empty, `theory_evidence_retrieval` must not call the
  helper and must attach `metrics = None`
- when matching golden targets are absent from the current execution context,
  the corresponding module output `metrics` field must be `None`.

The helper itself does not own:

- extraction of ranked ids from concrete module outputs;
- OpenInference emission;
- observability event shapes.

Those responsibilities belong to the retrieval modules that call the helper.

## 12) Implementation Guidance

The metrics helper should be implemented through small private helper methods
for individual metric computations.

Recommended decomposition:

- one private helper for recall-style metrics;
- one private helper for reciprocal-rank-style metrics;
- one private helper for relevant-count metrics;
- one private helper for DCG/IDCG/nDCG computation;
- one private helper for validation and normalization of
  `GoldenRetrievalTargetsById`.

Implementation guidance rules:

- the helper may return an error when metric inputs violate the contract defined
  in this document;
- helper failure is an internal utility failure, not an independent public
  pipeline error domain;
- the public helper entrypoint may assemble the final metric bundle from these
  private helper methods;
- private helper methods should be deterministic and side-effect free;
- private helper methods should be testable in isolation through module-local
  unit tests.
