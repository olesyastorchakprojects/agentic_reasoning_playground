## 1) Purpose / Scope

This document defines the runtime leaf-module contract for
`information_adequacy_analyzer`.

This module exists to:
- assess whether a structured runtime input contains enough information for the
  next diagnostic step to proceed safely;
- provide one shared adequacy classification for both initial-request analysis
  and continuation-observation analysis;
- identify the highest-priority missing-information topics when the input is
  weak or blocking;
- produce a deterministic list of canonical follow-up questions without free-
  form wording generation.

This module does not:
- normalize raw user input text;
- call a model;
- reinterpret raw continuation text;
- extract observations;
- update hypotheses;
- retrieve evidence;
- decide orchestration policy after classification.

This document is the source of truth for:
- the `information_adequacy_analyzer` leaf-module boundary;
- the shared adequacy output types produced by this module;
- the deterministic public interface for initial and observation analysis;
- the module-owned topic-selection rules;
- the canonical question-literal mapping rules;
- the module-owned error boundary.

Shared runtime types are defined by:
- `Specification/runtime/runtime.md`

The structured query input is defined by:
- `Specification/runtime/request_pipeline/query_structuring.md`

The structured observation input is defined by:
- `Specification/runtime/request_pipeline/observation_extraction.md`

Unit-test requirements for this module are defined by:
- `Specification/runtime/unit_tests.md`

The generated Rust module file for the current version is:
- `src/request_pipeline/information_adequacy_analyzer.rs`

## 2) Shared Dependencies

This module depends on:
- `Specification/runtime/runtime.md`
- `Specification/runtime/request_pipeline/query_structuring.md`
- `Specification/runtime/request_pipeline/observation_extraction.md`

Required shared runtime input types:
- `StructuredUserQuery`
- `StructuredUserQueryTerm`
- `StructuredUserQuerySupportLevel`
- `StructuredUserQueryConfidence`
- `ObservationBoundaryResolverOutput`
- `ObservationBoundaryResolution`
- `ObservationExtractionOutput`
- `ExtractedObservation`
- `ObservationPolarity`
- `Confidence`

Required shared runtime output types:
- `AdequacyStatus`
- `MissingInformationTopic`
- `AdequacyAssessment`

## 3) Generated Rust Artifact

The generated Rust crate must include:

- `src/request_pipeline/information_adequacy_analyzer.rs`

Parent module exposure:
- `src/request_pipeline/mod.rs` must expose `information_adequacy_analyzer`.

## 4) Shared Output Types

The shared output types used by this module are defined in `src/shared_types/mod.rs`
by:
- `Specification/runtime/runtime.md`

Shared-type rules:
- `AdequacyStatus` is the shared classification output returned by this module;
- `AdequacyStatus::Blocking` means the current structured input is not
  diagnostically sufficient for safe downstream progression;
- `AdequacyStatus::WeakButRunnable` means the current structured input contains
  a usable signal but remains diagnostically thin;
- `AdequacyStatus::Sufficient` means the current structured input is adequate
  for normal downstream progression;
- `MissingInformationTopic` is the shared machine-readable explanation of which
  kind of information is missing;
- `AdequacyAssessment.status` is the only final adequacy classification
  returned by this module;
- `AdequacyAssessment.missing_information_topics` must contain at most 3 items;
- `AdequacyAssessment.missing_information_topics` must not contain duplicates;
- `AdequacyAssessment.missing_information_topics` must be ordered by module-
  owned priority rules;
- `AdequacyAssessment.follow_up_questions` must be a deterministic projection of
  `missing_information_topics`;
- `AdequacyAssessment.follow_up_questions` must contain at most 3 items;
- `AdequacyAssessment.follow_up_questions.len()` must equal
  `AdequacyAssessment.missing_information_topics.len()`;
- `AdequacyAssessment.follow_up_questions` must not contain duplicates;
- `AdequacyAssessment.summary_reason` must be non-empty after trimming;
- when `AdequacyAssessment.status = AdequacyStatus::Sufficient`,
  `missing_information_topics` and `follow_up_questions` must both be empty;
- when `AdequacyAssessment.status = AdequacyStatus::Blocking`,
  `missing_information_topics` and `follow_up_questions` must both be non-empty;
- when `AdequacyAssessment.status = AdequacyStatus::WeakButRunnable`,
  `missing_information_topics` and `follow_up_questions` may be empty in the
  current version when the deterministic weakness rule does not map to a
  canonical missing-information topic.

Shared-type placement rules:
- downstream modules and the orchestrator must import these types from
  `crate::shared_types`;
- this module must not redefine these shared output types locally.

## 5) Public Interface

The generated Rust module must define a public module boundary equivalent in
ownership to:

```rust
pub struct InformationAdequacyAnalyzer;

impl InformationAdequacyAnalyzer {
    pub fn new() -> Self;

    pub fn analyze_initial(
        &self,
        query: &StructuredUserQuery,
    ) -> Result<AdequacyAssessment, InformationAdequacyAnalyzerError>;

    pub fn analyze_supported_observation(
        &self,
        observation: &ObservationExtractionOutput,
    ) -> Result<AdequacyAssessment, InformationAdequacyAnalyzerError>;

    pub fn analyze_unsupported_observation(
        &self,
        boundary_output: &ObservationBoundaryResolverOutput,
    ) -> Result<AdequacyAssessment, InformationAdequacyAnalyzerError>;
}
```

Rules:
- `InformationAdequacyAnalyzer` is stateless in the current version;
- `new()` must construct a stateless analyzer instance and must not fail;
- `analyze_initial(...)` must be fully deterministic for the same
  `StructuredUserQuery` input;
- `analyze_supported_observation(...)` must be fully deterministic for the same
  `ObservationExtractionOutput` input;
- `analyze_unsupported_observation(...)` must be fully deterministic for the
  same `ObservationBoundaryResolverOutput` input;
- this module must not perform network I/O, file I/O, model calls, or database
  access;
- this module must not use randomness;
- this module must not accept per-request configuration overrides in the
  current version.

## 6) Initial-Request Analysis Rules

`analyze_initial(query)` must interpret `StructuredUserQuery` through the
following derived counts:

- `symptom_count` = `query.symptoms.len()`
- `observability_count` = `query.observability_signals.len()`
- `failure_mode_count` = `query.failure_modes.len()`
- `trigger_count` = `query.triggers.len()`
- `scope_count` = `query.affected_subsystems.len() + query.entities.len()`
- `unresolved_count` = `query.unresolved_terms.len()`
- `symptom_signal_count` = `symptom_count + observability_count`
- `diagnostic_anchor_count` = count of non-empty groups among:
  - symptom/evidence
  - failure mode
  - trigger/change
  - scope/component

Support-level helper counts:
- `weak_inference_term_count` = number of entries across
  `query.symptoms`, `query.affected_subsystems`, `query.failure_modes`, and
  `query.system_properties` whose `support_level` is `WeakInference`;
- `explicit_term_count` = number of entries across the same four fields whose
  `support_level` is `Explicit`.

`analyze_initial(...)` must return `AdequacyStatus::Blocking` when at least one
of the following holds:
- `symptom_signal_count == 0`;
- `diagnostic_anchor_count <= 1`;
- `symptom_signal_count > 0`
  and `scope_count == 0`
  and `trigger_count == 0`
  and `failure_mode_count == 0`;
- `unresolved_count >= 2`
  and (`symptom_signal_count == 0` or `diagnostic_anchor_count == 1`).

`analyze_initial(...)` must return `AdequacyStatus::WeakButRunnable` when none
of the blocking rules hold and at least one of the following holds:
- `symptom_signal_count == 1`;
- `scope_count == 0`;
- `trigger_count == 0 and failure_mode_count == 0`;
- `weak_inference_term_count > explicit_term_count`.

`analyze_initial(...)` must return `AdequacyStatus::Sufficient` when neither
the blocking rules nor the weak rules hold.

## 7) Supported-Observation Analysis Rules

`analyze_supported_observation(observation)` must interpret
`ObservationExtractionOutput` through the following derived counts:

- `observation_count` = `observation.observations.len()`
- `present_count` = number of extracted observations whose polarity is
  `ObservationPolarity::Present`
- `absent_count` = number of extracted observations whose polarity is
  `ObservationPolarity::Absent`
- `corrected_count` = number of extracted observations whose polarity is
  `ObservationPolarity::Corrected`
- `question_count` = `observation.missing_context_questions.len()`
- `high_conf_count` = number of extracted observations whose confidence is
  `Confidence::High`
- `medium_or_higher_count` = number of extracted observations whose confidence
  is `Confidence::Medium` or `Confidence::High`

`analyze_supported_observation(...)` must return `AdequacyStatus::Blocking` when at least
one of the following holds:
- `observation_count == 0`;
- `observation.needs_more_context == true`;
- `observation_count == 1` and `observation.confidence == Confidence::Low`;
- `corrected_count > 0`
  and `present_count == 0`
  and `absent_count == 0`
  and `observation_count == corrected_count`
  and `observation.confidence == Confidence::Low`.

`analyze_supported_observation(...)` must return `AdequacyStatus::WeakButRunnable` when
none of the blocking rules hold and at least one of the following holds:
- `observation_count == 1`;
- `question_count > 0`;
- `medium_or_higher_count == 0`;
- `corrected_count == observation_count`
  and `observation.confidence != Confidence::High`.

`analyze_supported_observation(...)` must return `AdequacyStatus::Sufficient` when
neither the blocking rules nor the weak rules hold.

## 8) Unsupported-Observation Analysis Rules

`analyze_unsupported_observation(boundary_output)` must accept only:
- `boundary_output.resolution = ObservationBoundaryResolution::Unsupported`

`analyze_unsupported_observation(...)` must return:
- `AdequacyStatus::Blocking`

`analyze_unsupported_observation(...)` must not read or require
`ObservationExtractionOutput`.

`analyze_unsupported_observation(...)` must fail through the typed error
boundary when:
- `boundary_output.resolution = ObservationBoundaryResolution::Supported(...)`

Unsupported-observation topic priority order:
1. `ObservedResult`
2. `ExecutionContext`
3. `CheckOutcome`

`analyze_unsupported_observation(...)` must select:
- `ObservedResult`
- `ExecutionContext`
- `CheckOutcome`

Unsupported-observation pruning rules:
- the selected topics must preserve the priority order listed above;
- the selected topics must be limited to the first 2 topics in the current
  version.

## 9) Missing-Topic Selection Rules

### 9.1 General Rules

General topic-selection rules:
- the module must choose topics before constructing `follow_up_questions`;
- topics must be selected in priority order;
- selection must stop after 3 unique topics;
- topics must not be reordered after selection;
- this module must not emit observation-specific topics from
  `analyze_initial(...)`;
- this module must not emit initial-request-specific topics from
  `analyze_observation(...)`.

### 9.2 Initial-Request Topic Rules

Initial-request topic priority order:
1. `SymptomDescription`
2. `AffectedComponent`
3. `TriggerOrRecentChange`
4. `FailureMechanismHint`
5. `ExpectedVsActual`
6. `TermClarification`

`analyze_initial(...)` must select:
- `SymptomDescription` when `symptom_signal_count == 0`;
- `SymptomDescription` when `symptom_signal_count == 1`;
- `AffectedComponent` when `scope_count == 0`;
- `TriggerOrRecentChange` when `trigger_count == 0`;
- `FailureMechanismHint` when `failure_mode_count == 0`
  and `diagnostic_anchor_count < 2`;
- `ExpectedVsActual` when `symptom_signal_count > 0`
  and `diagnostic_anchor_count < 2`;
- `TermClarification` when `unresolved_count >= 2`.

Initial-request pruning rules:
- if `symptom_signal_count == 0`, `SymptomDescription` must be the first
  selected topic;
- `FailureMechanismHint` must not be selected when 3 higher-priority topics
  were already selected;
- `ExpectedVsActual` must not be selected when 3 higher-priority topics were
  already selected;
- `TermClarification` must not be selected when 3 higher-priority topics were
  already selected.

### 9.3 Supported-Observation Topic Rules

Supported-observation topic priority order:
1. `ObservedResult`
2. `CheckOutcome`
3. `ExecutionContext`
4. `ScopeOrBlastRadius`
5. `CorrectionTarget`

`analyze_supported_observation(...)` must select:
- `ObservedResult` when `observation_count == 0`;
- `ObservedResult` when `observation_count == 1`
  and `observation.confidence == Confidence::Low`;
- `CheckOutcome` when `observation.needs_more_context == true`
  and `question_count > 0`;
- `ExecutionContext` when `observation.needs_more_context == true`;
- `ScopeOrBlastRadius` when `observation_count > 0`
  and `question_count > 0`
  and `present_count + absent_count > 0`;
- `CorrectionTarget` when `corrected_count > 0`
  and `corrected_count == observation_count`.

Supported-observation pruning rules:
- if `observation_count == 0`, `ObservedResult` must be the first selected
  topic;
- when `observation.needs_more_context == true`, at least one topic must be
  selected;
- `CorrectionTarget` must not be selected unless at least one extracted
  observation has polarity `ObservationPolarity::Corrected`.

## 10) Canonical Follow-Up Question Rules

`follow_up_questions` must be built only from canonical string literals mapped
from `missing_information_topics`.

Canonical mapping:

- `MissingInformationTopic::SymptomDescription` =>
  `"What exactly are you observing: errors, timeouts, retries, stale data, leader changes, or another visible failure?"`
- `MissingInformationTopic::AffectedComponent` =>
  `"Which component or subsystem seems involved: for example the database, broker, lock service, scheduler, API gateway, or another part of the system?"`
- `MissingInformationTopic::TriggerOrRecentChange` =>
  `"What changed right before this started: for example a deploy, restart, failover, scaling event, config change, or traffic spike?"`
- `MissingInformationTopic::FailureMechanismHint` =>
  `"What failure pattern does this look closest to: for example timeout handling, replication lag, leader-election instability, lock contention, or resource exhaustion?"`
- `MissingInformationTopic::ExpectedVsActual` =>
  `"What did you expect to happen, and what happened instead?"`
- `MissingInformationTopic::ObservedResult` =>
  `"What was the exact observed result: for example the error message, timeout behavior, empty result, retry loop, or recovery signal?"`
- `MissingInformationTopic::ExecutionContext` =>
  `"Where and when did you observe this: for example which node, component, request path, environment, or time window?"`
- `MissingInformationTopic::CheckOutcome` =>
  `"What was the result of the check you ran: for example what did the command, log, metric, or status output show?"`
- `MissingInformationTopic::ScopeOrBlastRadius` =>
  `"How wide is the impact: is this limited to one node, shard, or request path, or does it affect the whole system?"`
- `MissingInformationTopic::CorrectionTarget` =>
  `"Which earlier assumption are you correcting, and what is the corrected fact now?"`
- `MissingInformationTopic::TermClarification` =>
  `"Some terms are still ambiguous. Can you restate the issue using the exact observed behavior and concrete component names?"`

Construction rules:
- the module must not paraphrase, shorten, expand, localize, or otherwise
  rewrite the canonical question text;
- the module must not generate free-form follow-up question wording;
- the order of `follow_up_questions` must exactly match the order of
  `missing_information_topics`;
- the module must construct `follow_up_questions` exclusively by mapping each
  selected topic to its canonical question literal;
- if `missing_information_topics` is empty, `follow_up_questions` must be
  empty.

## 11) Summary-Reason Rules

`AdequacyAssessment.summary_reason` must be deterministic and must use one of
the following exact string literals:

Selection rule:
- when multiple summary-reason predicates match, the module must select the
  first matching literal in the order listed below for that analysis path.

For `analyze_initial(...)`:
- `Blocking` + `symptom_signal_count == 0` =>
  `"The request does not describe any concrete symptom or observable behavior."`
- `Blocking` + `diagnostic_anchor_count <= 1` =>
  `"The request contains too little anchored diagnostic context to proceed safely."`
- `Blocking` + isolated symptoms without scope/trigger/failure mode =>
  `"The request names a symptom but does not anchor it to component, trigger, or failure pattern context."`
- `Blocking` + unresolved-term rule =>
  `"The request remains too ambiguous because key terms are unresolved."`
- `WeakButRunnable` =>
  `"The request contains a usable signal but is still diagnostically thin."`
- `Sufficient` =>
  `"The request contains enough diagnostic context to continue."`

For `analyze_supported_observation(...)`:
- `Blocking` + `observation_count == 0` =>
  `"The observation does not contain a concrete new diagnostic fact."`
- `Blocking` + `observation.needs_more_context == true` =>
  `"The observation requires more context before diagnostic update can proceed safely."`
- `Blocking` + single low-confidence observation =>
  `"The observation is too weak and low-confidence for a safe diagnostic update."`
- `Blocking` + correction-only low-confidence rule =>
  `"The observation only corrects a prior assumption and is still too weak for a safe diagnostic update."`
- `WeakButRunnable` =>
  `"The observation contains a usable update signal but still lacks diagnostic strength."`
- `Sufficient` =>
  `"The observation contains enough concrete diagnostic information to continue."`

For `analyze_unsupported_observation(...)`:
- `Blocking` =>
  `"The latest user message is not yet a supported standalone diagnostic observation."`

## 12) Error Boundary

The generated Rust module must define a typed error boundary equivalent in
ownership to:

```rust
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, serde::Serialize, serde::Deserialize)]
pub enum InformationAdequacyAnalyzerError {
    #[error("structured user query is invalid: {0}")]
    InvalidStructuredUserQuery(String),

    #[error("observation extraction output is invalid: {0}")]
    InvalidObservationExtractionOutput(String),
}
```

Rules:
- `analyze_initial(...)` must return `InvalidStructuredUserQuery(...)` only
  when the incoming `StructuredUserQuery` is structurally invalid for the
  current analyzer contract;
- `analyze_supported_observation(...)` must return
  `InvalidObservationExtractionOutput(...)` only when the incoming
  `ObservationExtractionOutput` is structurally invalid for the current
  analyzer contract;
- `analyze_unsupported_observation(...)` must return
  `InvalidObservationExtractionOutput(...)` when the incoming
  `ObservationBoundaryResolverOutput` is structurally invalid for the current
  analyzer contract, including the case where `resolution` is
  `Supported(...)`;
- semantic weakness must be reported through `AdequacyAssessment`, not through
  the typed error boundary;
- the current version must not use panic for invalid analyzer input.

## 13) Unit Tests

Generated unit-test requirements for this module are owned by:
- `Specification/runtime/unit_tests.md`

Rules:
- crate-level generated unit tests for this module must follow the dedicated
  `information_adequacy_analyzer` section in
  `Specification/runtime/unit_tests.md`;
- this module specification defines the runtime contract and deterministic
  rules that those generated unit tests must verify;
- this document does not duplicate the crate-level unit-test case list.
