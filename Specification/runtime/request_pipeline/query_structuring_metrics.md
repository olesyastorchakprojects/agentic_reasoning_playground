## 1) Purpose / Scope

This document defines the query-structuring quality metric helper contract for
the runtime crate.

The generated Rust helper artifact for the current version is:

- `src/request_pipeline/query_structuring_metrics.rs`

It defines:

- the metric inputs;
- the field-to-vocabulary mapping rules;
- the per-field metric formulas;
- the cross-field aggregate formulas;
- the required output metric bundle shape at the semantic level.

This document does not define:

- CLI behavior;
- orchestration ownership;
- observability attribute names;
- alternative Rust module placements beyond the current generated helper
  artifact path;
- cross-request report aggregates across many runs.

Module-placement rule:

- the query-structuring metrics helper must live in a dedicated internal Rust
  module of the generated runtime crate;
- the helper must not be inlined into `query_structuring`, `orchestrator`, or
  observability module bodies;
- unit tests for the helper must live in that helper module under its own
  `#[cfg(test)]` test module.

## 2) Role Of The Query-Structuring Metrics Helper

The query-structuring metrics helper is a runtime utility owned by the runtime
crate.

Its purpose is:

- to compute request-local query-structuring quality metrics from one actual
  `StructuredUserQuery`, one matching golden query-structuring target set, and
  one parsed controlled-vocabulary asset;
- to provide one source of truth for query-structuring metric formulas;
- to keep field-level and aggregate metric computation consistent across
  runtime, persistence, and later evaluation layers.

The helper computes per-request metrics only.
It does not compute cross-request batch means or pass rates across many runs.
The helper may reject invalid metric inputs.

The helper must expose one internal callable entrypoint semantically equivalent
to:

```rust
pub(crate) fn compute_query_structuring_metrics(
    structured_query: &StructuredUserQuery,
    golden_targets: &GoldenQueryStructuringTargets,
    controlled_vocabulary: &QueryStructuringControlledVocabulary,
    raw_user_query: &str,
) -> Result<QueryStructuringMetrics, QueryStructuringMetricsError>;
```

Interface rules:

- the helper entrypoint is internal to the crate and must not become a separate
  public API surface;
- `raw_user_query` is the request-local query string used for evidence-span
  grounding checks;
- callers must pass the same request-local query text whose structured form
  produced `structured_query`;
- the helper must not reread any assets from disk;
- the helper must return the full `QueryStructuringMetrics` bundle on success.

## 3) Inputs

The helper consumes:

- one `StructuredUserQuery`;
- one `GoldenQueryStructuringTargets`;
- one parsed `QueryStructuringControlledVocabulary`;
- one request-local raw user query string.

Input rules:

- `StructuredUserQuery` is the actual request-local output produced by the
  query-structuring stage;
- `GoldenQueryStructuringTargets` is the matching request-local golden target
  set extracted from the current `GoldenQuestion`;
- `QueryStructuringControlledVocabulary` is the already-loaded and already-
  validated controlled-vocabulary asset used by the query-structuring stage;
- `QueryStructuringControlledVocabulary` is the shared runtime type defined in
  `Specification/runtime/runtime.md` and owned in `src/shared_types.rs`;
- `raw_user_query` is the request-local query text used for evidence-span
  grounding checks;
- the helper must not read vocabulary files from disk directly;
- the helper must not accept raw JSON strings in place of typed inputs.

Golden-target validity defaults:

- each `strict_vocabulary_terms` list must be interpreted as a set of unique
  term strings;
- each `soft_vocabulary_terms` list must be interpreted as a set of unique term
  strings;
- each `graded_relevance` list must contain at most one entry per term string;
- duplicate strings within `strict_vocabulary_terms` are invalid input and must
  be rejected;
- duplicate strings within `soft_vocabulary_terms` are invalid input and must
  be rejected;
- duplicate `graded_relevance.term` entries are invalid input and must be
  rejected;
- `StrictGold` must be a subset of `SoftGold`; if not, the helper must reject
  the input as invalid;
- any term appearing in `strict_vocabulary_terms`, `soft_vocabulary_terms`, or
  `graded_relevance` must belong to the corresponding field-specific controlled
  vocabulary set; otherwise the helper must reject the input as invalid;
- graded relevance scores must be exactly one of:
  - `0.0`
  - `0.5`
  - `1.0`
- every term in `StrictGold` must appear in `graded_relevance` with score
  exactly `1.0`; otherwise the helper must reject the input as invalid;
- every term in `SoftGold` must appear in `graded_relevance`; if absent, the
  helper must treat that as invalid input rather than synthesizing an implicit
  score;
- every term in `SoftGold - StrictGold` must appear in `graded_relevance` with
  score exactly `0.5`; otherwise the helper must reject the input as invalid;
- additional controlled-vocabulary terms may appear in `graded_relevance` with
  score `0.0`;
- every term appearing in `graded_relevance` with score `1.0` must belong to
  `StrictGold`; otherwise the helper must reject the input as invalid;
- every term appearing in `graded_relevance` with score `0.5` must belong to
  `SoftGold - StrictGold`; otherwise the helper must reject the input as
  invalid;
- every term appearing in `graded_relevance` with score `0.0` must not belong
  to `SoftGold`; otherwise the helper must reject the input as invalid;
- leading/trailing whitespace in golden term strings must be trimmed before
  validation; if trimming yields an empty string, the helper must reject the
  input as invalid.

Vocabulary field mapping rules:

- the `affected_subsystems` -> `affected_components` mapping is intentional:
  `affected_subsystems` is the structured output field name, while
  `affected_components` is the controlled-vocabulary asset field name for the
  same semantic category;

- `StructuredUserQuery.symptoms` uses
  `QueryStructuringControlledVocabulary.canonical_symptoms`;
- `StructuredUserQuery.affected_subsystems` uses
  `QueryStructuringControlledVocabulary.affected_components`;
- `StructuredUserQuery.failure_modes` uses
  `QueryStructuringControlledVocabulary.failure_mode_candidates`;
- `StructuredUserQuery.system_properties` uses
  `QueryStructuringControlledVocabulary.violated_properties`.

Golden target mapping rules:

- `StructuredUserQuery.symptoms` is evaluated against
  `GoldenQueryStructuringTargets.symptoms`;
- `StructuredUserQuery.affected_subsystems` is evaluated against
  `GoldenQueryStructuringTargets.affected_subsystems`;
- `StructuredUserQuery.failure_modes` is evaluated against
  `GoldenQueryStructuringTargets.failure_modes`;
- `StructuredUserQuery.system_properties` is evaluated against
  `GoldenQueryStructuringTargets.system_properties`.

Rich metrics apply only to the four vocabulary-backed fields above.

The current contract does not apply the same rich semantic metric surface to:

- `intent`
- `scenario`
- `entities`
- `constraints`
- `triggers`
- `observability_signals`
- `unresolved_terms`
- `rejected_nearby_terms`

## 4) Predicted Sets And Deduplication

For each vocabulary-backed field:

- let `PredictedRaw` be the emitted ordered list of selected
  `StructuredUserQueryTerm.term` values in the current field;
- let `PredictedDedup` be the ordered left-to-right unique list obtained by
  keeping only the first occurrence of each term string;
- let `PredictedSet` be the set of unique terms in `PredictedDedup`.

Deduplication rules:

- duplicate later occurrences must not increase recall, precision, graded
  coverage, grounded recall, or support-level counts;
- `duplicate_term_count` counts how many emitted terms were discarded by this
  left-to-right deduplication process;
- evidence/support diagnostics for a duplicated term must be evaluated only on
  its first occurrence.

## 5) Contract And Vocabulary Metrics

For each of these vocabulary-backed fields:

- `symptoms`
- `affected_subsystems`
- `failure_modes`
- `system_properties`

the helper must compute:

- `invalid_vocab_count`
- `duplicate_term_count`

Definitions:

- `invalid_vocab_count = |PredictedSet - AllowedVocabularySet|`;
- `duplicate_term_count = |PredictedRaw| - |PredictedDedup|`.

Where:

- `AllowedVocabularySet` is the field-specific controlled vocabulary set from
  `QueryStructuringControlledVocabulary`.

Rules:

- `invalid_vocab_count` is a per-field diagnostic count, not a rate;
- invalid vocabulary terms must still participate in `PredictedSet` when
  computing false positives and unsupported selections;
- `duplicate_term_count` is a per-field output-discipline metric and must not be
  folded into semantic correctness metrics.

## 6) Set-Based Term Selection Metrics

For each of these vocabulary-backed fields:

- `symptoms`
- `affected_subsystems`
- `failure_modes`
- `system_properties`

let:

- `StrictGold` be the set of terms from
  `GoldenVocabularyFieldTargets.strict_vocabulary_terms`;
- `SoftGold` be the set of terms from
  `GoldenVocabularyFieldTargets.soft_vocabulary_terms`.

The helper must compute:

- `precision_soft`
- `recall_strict`
- `recall_soft`
- `num_false_positive`
- `num_false_negative_strict`
- `num_predicted_terms`

Definitions:

- `num_predicted_terms = |PredictedSet|`;
- `num_false_positive = |PredictedSet - SoftGold|`;
- `num_false_negative_strict = |StrictGold - PredictedSet|`;
- `precision_soft = |PredictedSet ∩ SoftGold| / |PredictedSet|`
- `recall_strict = |PredictedSet ∩ StrictGold| / |StrictGold|`
- `recall_soft = |PredictedSet ∩ SoftGold| / |SoftGold|`

Empty-set rules:

- if `PredictedSet` is empty and `SoftGold` is empty, `precision_soft = 1.0`;
- if `PredictedSet` is empty and `SoftGold` is non-empty, `precision_soft = 0.0`;
- if `PredictedSet` is non-empty and `SoftGold` is empty, `precision_soft = 0.0`;
- if `StrictGold` is empty, `recall_strict = 1.0`;
- if `SoftGold` is empty, `recall_soft = 1.0`.

Set-metric rules:

- the helper must treat query structuring as set selection, not ranking;
- the helper must not define MRR, reciprocal rank, nDCG@k, or recall@k for
  query structuring;
- `StrictGold` is allowed to be empty for a field;
- when a term is not in `SoftGold`, it is a false positive even if it is in the
  controlled vocabulary.

## 7) Graded Relevance Metrics

For each of these vocabulary-backed fields:

- `symptoms`
- `affected_subsystems`
- `failure_modes`
- `system_properties`

let:

- `grade(term)` be the score looked up from
  `GoldenVocabularyFieldTargets.graded_relevance`;
- if a term is not present in `graded_relevance`, then `grade(term) = 0.0`.

The helper must compute:

- `graded_coverage`
- `average_selected_score`
- `zero_score_selection_count`

Definitions:

- `positive_grade_sum = sum(score for score > 0.0 in graded_relevance)`;
- `selected_positive_grade_sum = sum(grade(term) for term in PredictedSet when grade(term) > 0.0)`;
- `selected_grade_sum = sum(grade(term) for term in PredictedSet)`;
- `graded_coverage = selected_positive_grade_sum / positive_grade_sum`
- `average_selected_score = selected_grade_sum / |PredictedSet|`
- `zero_score_selection_count = |{ term in PredictedSet : grade(term) = 0.0 }|`

Empty/zero rules:

- if `positive_grade_sum = 0.0`, then `graded_coverage = 1.0` when
  `PredictedSet` is empty, otherwise `0.0`;
- if `PredictedSet` is empty, then `average_selected_score = 0.0`.

Graded-relevance rules:

- the current contract restricts graded relevance scores to:
  - `0.0`
  - `0.5`
  - `1.0`
- `graded_coverage` is a set-based graded coverage metric, not a ranking metric;
- `zero_score_selection_count` counts known non-relevant selected terms and is
  not reduced by soft-match credit.

## 8) Evidence-Span And Grounding Metrics

For each of these vocabulary-backed fields:

- `symptoms`
- `affected_subsystems`
- `failure_modes`
- `system_properties`

the helper must compute:

- `grounded_strict_recall`
- `unsupported_selected_term_rate`
- `missing_evidence_span_count`
- `invalid_evidence_span_count`
- `evidence_span_near_substring_rate`

Evidence normalization rules:

- `normalized_query` is the raw user query lowercased with repeated whitespace
  collapsed to single spaces;
- `normalized_evidence_span` is the emitted `evidence_span` lowercased with
  repeated whitespace collapsed to single spaces;
- punctuation-only differences at token boundaries must not invalidate a
  substring match.

Deterministic normalization defaults:

- before substring comparison, the helper must trim leading and trailing
  whitespace from both strings;
- the helper must lowercase using Rust's standard Unicode-aware lowercase
  transformation;
- the helper must collapse every maximal run of Unicode whitespace to a single
  ASCII space;
- the helper must strip ASCII punctuation characters from the start and end of
  each whitespace-delimited token in both strings;
- the helper must not strip or rewrite punctuation characters occurring inside
  a token after boundary trimming;
- the helper must not rewrite alphanumeric characters;
- the helper must not apply stemming, lemmatization, synonym expansion, accent
  folding, transliteration, or token reordering.

Boundary examples that must hold:

- `"timeout"` matches `"timeout,"`;
- `"raft election"` matches `"raft election:"`;
- `"node-1"` does not normalize to `"node 1"`;
- `"read-only"` does not normalize to `"read only"`.

Near-substring rule:

- `evidence_span_near_substring_rate` uses `normalized_evidence_span` and
  `normalized_query`;
- a selected term counts as near-substring-grounded when its normalized
  evidence span is non-empty and appears as a contiguous substring of the
  normalized query.

Valid strict grounding rule:

- a strict term is validly grounded when:
  - the term is in `StrictGold`;
  - its normalized evidence span is non-empty;
  - its normalized evidence span is a near-substring of the normalized query;
  - its `support_level` is not `WeakInference`.

Definitions:

- `grounded_strict_recall = grounded_strict_hits / |StrictGold|`
- `unsupported_selected_term_rate = unsupported_selected_terms / |PredictedSet|`
- `missing_evidence_span_count` is the number of selected terms whose
  normalized evidence span is empty;
- `invalid_evidence_span_count` is the number of selected terms whose
  normalized evidence span is non-empty but not a near-substring of the
  normalized query;
- `evidence_span_near_substring_rate = near_substring_selected_terms / |PredictedSet|`

Where:

- `grounded_strict_hits` is the number of strict-gold predicted terms satisfying
  the valid strict grounding rule;
- `unsupported_selected_terms` counts selected terms whose evidence span is
  empty, not a near-substring, or marked `WeakInference`;
- `missing_evidence_span_count` does not include terms whose evidence span is
  non-empty but invalid as a query grounding span;
- `invalid_evidence_span_count` does not include terms whose evidence span is
  empty;
- `near_substring_selected_terms` counts selected terms whose evidence span
  satisfies the near-substring rule.

Empty rules:

- if `StrictGold` is empty, `grounded_strict_recall = 1.0`;
- if `PredictedSet` is empty, `unsupported_selected_term_rate = 0.0`;
- if `PredictedSet` is empty, `evidence_span_near_substring_rate = 0.0`.
- `evidence_span_near_substring_rate = 0.0` in the empty-prediction case is
  intentional rather than vacuous-success semantics because no evidence spans
  were provided.

## 9) Support-Level Metrics

For each of these vocabulary-backed fields:

- `symptoms`
- `affected_subsystems`
- `failure_modes`
- `system_properties`

the helper must compute:

- `weak_inference_rate`
- `strict_terms_weak_inference_rate`
- `weak_false_positive_rate`

Definitions:

- `weak_inference_rate = weak_selected_terms / |PredictedSet|`
- `strict_terms_weak_inference_rate = weak_strict_selected_terms / strict_selected_terms`
- `weak_false_positive_rate = weak_false_positive_terms / false_positive_terms`

Where:

- `weak_selected_terms` counts selected terms whose support level is
  `WeakInference`;
- `strict_selected_terms` counts selected terms whose term belongs to
  `StrictGold`;
- `weak_strict_selected_terms` counts strict-selected terms whose support level
  is `WeakInference`;
- `false_positive_terms` counts selected terms whose term is not in `SoftGold`;
- `weak_false_positive_terms` counts false-positive selected terms whose support
  level is `WeakInference`.

Empty rules:

- if `PredictedSet` is empty, `weak_inference_rate = 0.0`;
- if `strict_selected_terms = 0`, `strict_terms_weak_inference_rate = 0.0`;
- if `false_positive_terms = 0`, `weak_false_positive_rate = 0.0`.

## 10) Field-Level Success Metrics

For each of these vocabulary-backed fields:

- `symptoms`
- `affected_subsystems`
- `failure_modes`
- `system_properties`

the helper must compute:

- `field_core_success`
- `field_grounded_success`
- `empty_when_gold_exists`

Definitions:

- `field_core_success = (recall_strict = 1.0) AND (invalid_vocab_count = 0)`
- `field_grounded_success = (grounded_strict_recall = 1.0) AND (unsupported_selected_term_rate = 0.0)`
- `empty_when_gold_exists = PredictedSet is empty AND StrictGold is non-empty`

Rules:

- these are request-local booleans, not cross-request rates;
- `field_core_success` is the minimal semantic pass/fail gate for a
  vocabulary-backed field;
- `field_grounded_success` is the trust-oriented grounding gate for a
  vocabulary-backed field;
- `field_grounded_success` is intentionally a strict all-selected-terms gate:
  it requires strict-core grounding success and also rejects unsupported soft
  or false-positive selections in the same field;
- `field_grounded_success` therefore expresses full field grounding cleanliness,
  not merely strict-core grounding success.

## 11) Non-Vocabulary Field Metrics

For the current contract the helper must compute only light extraction counts
for non-vocabulary fields.

The helper must compute:

- `entities_count`
- `constraints_count`
- `triggers_count`
- `observability_signals_count`
- `unresolved_terms_count`
- `intent_present`
- `scenario_present`

Definitions:

- each `*_count` is the length of the corresponding array field in
  `StructuredUserQuery`;
- `intent_present` is `true` when `intent.trim()` is non-empty;
- `scenario_present` is `true` when `scenario.trim()` is non-empty.

Rules:

- these fields do not currently receive strict/soft/graded vocabulary metrics;
- these fields do not currently receive evidence-span or support-level metrics
  because the current output contract does not attach those annotations to them.

## 12) Cross-Field Aggregates

The helper must compute cross-field aggregates over exactly these four
vocabulary-backed fields:

- `symptoms`
- `affected_subsystems`
- `failure_modes`
- `system_properties`

The helper must compute:

- `macro_precision_soft`
- `macro_recall_strict`
- `macro_recall_soft`
- `overall_grounded_strict_recall`
- `all_fields_core_success_rate`

Definitions:

- `macro_precision_soft` is the arithmetic mean of the four per-field
  `precision_soft` values;
- `macro_recall_strict` is the arithmetic mean of the four per-field
  `recall_strict` values;
- `macro_recall_soft` is the arithmetic mean of the four per-field
  `recall_soft` values;
- `overall_grounded_strict_recall = sum(grounded_strict_hits across fields) / sum(|StrictGold| across fields)`
- `all_fields_core_success_rate = passed_core_fields / 4.0`

Where:

- `passed_core_fields` is the number of vocabulary-backed fields whose
  `field_core_success` is `true`.

Aggregate rules:

- `all_fields_core_success_rate` is request-local and therefore can take values:
  - `0.0`
  - `0.25`
  - `0.5`
  - `0.75`
  - `1.0`
- if every field has empty `StrictGold`, then
  `overall_grounded_strict_recall = 1.0`;
- otherwise `overall_grounded_strict_recall` must use the documented global
  numerator and denominator formula;
- the helper must not compute cross-request pass rates or batch means.

## 13) Output Semantics

The helper returns one per-request metric bundle in the shared
`QueryStructuringMetrics` type defined by:

- `Specification/runtime/runtime.md`

Shared-shape rules:

- this document defines the semantic contract for the shared metric types used
  by query structuring;
- `runtime.md` remains the source of truth for the exact shared Rust type
  declarations and ownership in `src/shared_types.rs`;
- the helper output must populate every field of the shared
  `QueryStructuringMetrics` shape;
- the shared `QueryStructuringMetrics` shape must contain:
  - `top_level`
  - `vocab_fields`
  - `non_vocab_fields`
  - `aggregates`
- the shared `QueryStructuringVocabularyFieldMetrics` shape must contain one
  `QueryStructuringVocabularyFieldMetricSet` for each vocabulary-backed field:
  - `symptoms`
  - `affected_subsystems`
  - `failure_modes`
  - `system_properties`
- each shared `QueryStructuringVocabularyFieldMetricSet` must carry the
  vocabulary, set-based, graded, grounding, support-level, and field-success
  outputs defined in Sections 5 through 10;
- the shared `QueryStructuringNonVocabularyFieldMetrics` shape must carry the
  light count/presence outputs defined in Section 11;
- the shared `QueryStructuringAggregateMetrics` shape must carry the request-
  local cross-field aggregate outputs defined in Section 12;
- the shared `QueryStructuringTopLevelMetrics` shape is a convenience duplicate
  of the selected request-local aggregate outputs.

Top-level rules:

- `top_level` is a convenience duplicate of the selected request-local aggregate
  metrics used for quick consumption by later observability and reporting
  layers;
- each field in `top_level` must equal the corresponding field in `aggregates`;
- the helper must not rename or reinterpret these per-request values as batch
  metrics.

## 14) Error Model

The helper must define one internal error type for query-structuring metric
computation failures.

Required failure categories:

- invalid golden query-structuring target shape for metric computation;
- inconsistent field-to-vocabulary mapping state;
- inconsistent graded-relevance state;
- invalid raw user-query input for grounding normalization;
- unexpected internal metric computation state.

Error-model rules:

- helper failures are internal utility failures;
- helper failures must not become an independent public pipeline error domain;
- query structuring wraps helper failures into `QueryStructuringError`;
- helper failures must preserve enough diagnostic information to identify the
  failing field and failure category;
- an empty or all-whitespace `raw_user_query` is invalid helper input and must
  be rejected rather than normalized into a degenerate grounding surface;
- violation of any golden-target validity default from Section 3 must be
  reported as invalid input rather than silently repaired;
- violation of the documented graded-score domain must be reported as invalid
  input rather than rounded or clamped;
- the helper must not silently deduplicate or repair invalid golden targets.

## 15) Implementation Guidance

The helper should be implemented through small private helper methods for
individual metric families.

Recommended decomposition:

- one private helper for vocabulary contract counts;
- one private helper for set-based metrics;
- one private helper for graded metrics;
- one private helper for grounding metrics;
- one private helper for support-level metrics;
- one private helper for request-local cross-field aggregates.

Implementation guidance rules:

- metric computation must be deterministic for the same typed inputs;
- vocabulary-backed metrics should be computed field-by-field and then folded
  into request-local aggregates;
- non-vocabulary field metrics should remain lightweight in the current version;
- helper-local private utilities may normalize strings for evidence-span
  comparison as long as the normalization remains deterministic and follows the
  documented near-substring rule.
