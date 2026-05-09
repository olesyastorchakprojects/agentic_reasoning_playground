## 1) Purpose

This document defines the logical projection from runtime-owned `RunState` into
the eval-owned `DiagnosticEvalIterationSnapshot`.

The snapshot is the canonical input boundary for judge suites.

The eval engine must not send raw `RunState` directly to suites.

Instead it must:

1. load one canonical `RunState`;
2. select one target iteration;
3. derive one iteration snapshot;
4. build suite-specific payloads from that snapshot.

This indirection exists to:

- keep judge payloads stable even if `RunState` grows;
- isolate eval logic from runtime internal storage layout;
- support future multi-iteration agent loops cleanly.

## 2) Eval Subject

The snapshot subject is:

- one `runtime_run_id`;
- one `iteration_id`;
- one set of finished step outputs for that iteration.

For the current MVP, the default target is the last iteration in the runtime
run.

The snapshot must also classify the selected iteration as:

- `initial`
- `continuation`

For a continuation iteration, the snapshot subject additionally includes the
immediately prior completed iteration context required to judge whether the new
response is a good diagnostic update.

## 3) Source Records

The snapshot must be derived only from:

- `RunState`
- the selected `RunIteration`
- successful `FinishedStepRecord` payloads in that iteration

The projection must use `StepKind` and `StepResultEnvelope` compatibility as
defined by the runtime.

The projection must not infer data from pending steps.

## 4) Required MVP Snapshot Fields

The snapshot must expose at least:

- `runtime_run_id`
- `iteration_id`
- `iteration_kind`
- `run_created_at`
- `run_updated_at`
- `user_request`
- `golden_question`
- `normalized_user_request`
- `query_structuring_output`
- `candidate_card_retrieval_output`
- `card_hydration_output`
- `incident_evidence_retrieval_output`
- `theory_evidence_retrieval_output`
- `prompt_context_assembly_output`
- `llm_structured_generation_output`
- `response_validation_and_normalization_output`
- `runtime_model_usage`

The snapshot may additionally expose convenience summaries derived from those
typed outputs, but these convenience summaries must remain faithful to the
underlying runtime payloads.

For continuation iterations, the snapshot must additionally expose at least:

- `previous_iteration_id`
- `previous_response_validation_and_normalization_output`
- `previous_active_hypotheses`
- `previous_first_check`
- `observation_boundary_resolver_output`
- `observation_extraction_output`
- `card_branch_reranking_output`
- `diagnostic_update_prompt_context_output`

## 5) Field Mapping Rules

### 5.1) `runtime_run_id`

- comes from `RunState.run_id`

### 5.2) `iteration_id`

- comes from the selected `RunIteration.iteration_id`

### 5.2a) `iteration_kind`

- `initial` when the selected iteration is the first completed iteration in the
  runtime run
- `continuation` when the selected iteration is a later completed iteration

### 5.3) `user_request`

- comes from the successful finished record where
  `step = StepKind::UserInputReceived`
- payload type:
  `StepResultEnvelope::UserInputReceived(UserRequest)`

### 5.4) `golden_question`

- comes from `user_request.golden_question`
- may be `None` for non-golden runtime runs

### 5.5) `normalized_user_request`

- comes from the successful finished record where
  `step = StepKind::InputNormalization`

### 5.6) `query_structuring_output`

- comes from the successful finished record where
  `step = StepKind::QueryStructuring`

### 5.7) `candidate_card_retrieval_output`

- comes from the successful finished record where
  `step = StepKind::CandidateCardRetrieval`

### 5.8) `card_hydration_output`

- comes from the successful finished record where
  `step = StepKind::CardHydration`

### 5.9) `incident_evidence_retrieval_output`

- comes from the successful finished record where
  `step = StepKind::IncidentEvidenceRetrieval`

### 5.10) `theory_evidence_retrieval_output`

- comes from the successful finished record where
  `step = StepKind::TheoryEvidenceRetrieval`

### 5.11) `prompt_context_assembly_output`

- comes from the successful finished record where
  `step = StepKind::PromptContextAssembly`

### 5.12) `llm_structured_generation_output`

- comes from the successful finished record where
  `step = StepKind::LlmStructuredGeneration`

### 5.13) `response_validation_and_normalization_output`

- comes from the successful finished record where
  `step = StepKind::ResponseValidationAndNormalization`

### 5.14) `runtime_model_usage`

This is the runtime-owned token and cost accounting bundle for the evaluated
iteration subject.

For the current MVP:

- token counts must be sourced from runtime step payloads that expose
  `ModelTokenUsage`-like values;
- cost may initially be absent from some runtime-owned step outputs until the
  runtime adds explicit cost fields;
- the snapshot contract must still reserve fields for:
  - prompt tokens
  - completion tokens
  - total tokens
  - prompt cost usd
- completion cost usd
- total cost usd

### 5.15) Continuation-only fields

When `iteration_kind = continuation`, the snapshot must additionally project:

- `previous_iteration_id`
  - from the immediately prior completed iteration in the same `RunState`
- `previous_response_validation_and_normalization_output`
  - from the prior iteration successful
    `StepKind::ResponseValidationAndNormalization`
- `previous_active_hypotheses`
  - from the normalized prior response
- `previous_first_check`
  - from the normalized prior response
- `observation_boundary_resolver_output`
  - from the current iteration successful
    `StepKind::ObservationBoundaryResolver`
- `observation_extraction_output`
  - from the current iteration successful `StepKind::ObservationExtraction`
- `card_branch_reranking_output`
  - from the current iteration successful `StepKind::CardBranchReranking`
- `diagnostic_update_prompt_context_output`
  - from the current iteration successful
    `StepKind::DiagnosticUpdatePromptContextAssembly`

## 6) Missing Data Rule

The snapshot builder must fail the evaluated subject if a required MVP field is
missing because a required runtime step did not finish successfully.

Examples:

- missing final validated answer;
- missing query structuring output;
- missing prompt context output for final-answer suites.

For continuation iterations, this failure rule also applies to required
continuation-specific steps such as:

- missing observation extraction output;
- missing diagnostic update prompt context output;
- missing prior completed iteration context.

The eval engine must not silently fabricate partial snapshots for required
suites.

However, suite-specific payload builders may use a narrower subset of snapshot
fields than the full snapshot.

## 7) Suite Payload Construction Rule

Judge suites must build their prompt inputs from the snapshot, not directly
from raw `RunState`.

This means:

- the snapshot is the stable shared boundary;
- suite payloads are narrower derived views over the snapshot;
- adding a new suite must not require redefining runtime-owned state.

## 8) Canonical MVP Payload Bindings

For the current MVP, the suite catalog input-variable names must bind to
snapshot-derived values as follows:

- `raw_user_query`
  - from `snapshot.user_request.query`
- `structured_query`
  - from `snapshot.query_structuring_output.structured_query`
- `evidence_topology`
  - from the prompt-context shape contained in
    `snapshot.prompt_context_assembly_output`
- `incident_evidence_chunks`
  - from `snapshot.incident_evidence_retrieval_output`
- `theory_chunks`
  - from `snapshot.theory_evidence_retrieval_output`
- `matched_incident_card`
  - from the primary hydrated card in `snapshot.card_hydration_output`
- `final_answer`
  - from `snapshot.response_validation_and_normalization_output`
- `active_hypotheses`
  - from the normalized final diagnostic answer payload inside
    `snapshot.response_validation_and_normalization_output`
- `first_check`
  - from the normalized final diagnostic answer payload inside
    `snapshot.response_validation_and_normalization_output`
- `eval_context`
  - from a suite-specific compact JSON context built from the snapshot for
    final-answer suites

`eval_context` exists because some final-answer suites need a compact structured
view that combines:

- user problem framing;
- evidence topology;
- incident and theory evidence context;
- final-answer schema fields.

The worker stage owns building this compact context deterministically from the
snapshot.

For continuation suites, additional bindings may include:

- `previous_response`
  - from the prior iteration normalized response
- `previous_active_hypotheses`
  - from the prior iteration normalized response
- `previous_first_check`
  - from the prior iteration normalized response
- `resolved_observation`
  - from the current continuation observation boundary output
- `observations`
  - from the current continuation observation extraction output
- `update_context`
  - from `diagnostic_update_prompt_context_output`
- `current_response`
  - from the current iteration normalized response

## 9) Forward Compatibility Fields

To support future multi-iteration loops, the snapshot design must reserve room
for optional history fields such as:

- `prior_iterations_count`
- `prior_iteration_ids`
- `prior_iteration_summary`
- `latest_prior_user_observation`
- `previous_iteration_id`
- `previous_response_validation_and_normalization_output`
- `observation_extraction_output`

For the current MVP these fields may be absent or empty.

The current MVP does not require cross-iteration judge suites, but the snapshot
shape must not forbid them.
