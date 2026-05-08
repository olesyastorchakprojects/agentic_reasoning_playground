## 1) Purpose

This document defines the execution-dispatch boundary for
`orchestrator::step_executor`.

`step_executor` receives an explicit `StepKind` selected by orchestration
policy, reads the required finished step outputs from persisted run state,
invokes the matching request-pipeline leaf module, and returns either a
step-typed `StepResultEnvelope` or a `StepError`.

This module does not:

- decide which step should run next;
- mutate `RunState`;
- persist runs or step history;
- create pending-step records;
- define orchestration lifecycle loops.

The current version remains current-iteration-first:

- direct prerequisite step payloads are read from the current iteration;
- older iterations may additionally be read only when a continuation-only step
  requires a history-derived shared projection such as `DiagnosticContext` or
  `CardSelectionContext`.

## 2) Generated Rust Artifact

The generated Rust crate must include:

- `src/orchestrator/step_executor.rs`

Parent module exposure:

- `src/orchestrator/mod.rs` must expose `step_executor`.

All public types and methods defined by this spec must be public.

## 3) Imports

The generated module requires:

```rust
use crate::orchestrator::run_state::model::{
    StepError,
    StepKind,
    StepResultEnvelope,
};
use crate::orchestrator::run_state::view::{FinishedStepView, IterationView, RunStateView};
use crate::request_pipeline::candidate_card_retrieval::CandidateCardRetrieval;
use crate::request_pipeline::card_branch_reranking::CardBranchReranking;
use crate::request_pipeline::card_hydration::CardHydration;
use crate::request_pipeline::diagnostic_update_prompt_context_assembly::DiagnosticUpdatePromptContextAssembly;
use crate::request_pipeline::information_adequacy_analyzer::InformationAdequacyAnalyzer;
use crate::request_pipeline::incident_evidence_retrieval::IncidentEvidenceRetrieval;
use crate::request_pipeline::input_normalization::InputNormalization;
use crate::request_pipeline::llm_structured_generation::LlmStructuredGeneration;
use crate::request_pipeline::observation_boundary_resolver::ObservationBoundaryResolver;
use crate::request_pipeline::observation_extraction::ObservationExtraction;
use crate::request_pipeline::prompt_context_assembly::PromptContextAssembly;
use crate::request_pipeline::query_structuring::QueryStructuring;
use crate::request_pipeline::response_validation_and_normalization::ResponseValidationAndNormalization;
use crate::request_pipeline::theory_evidence_retrieval::TheoryEvidenceRetrieval;
use crate::shared_types::{
    CardSelectionContext,
    Context,
    DiagnosticContext,
    HydratedCardBranchesInput,
    IncidentEvidenceCardBranchesInput,
    IterationProfile,
    ObservationBoundaryResolution,
    RetrievalQueryInput,
    UserRequest,
};
```

Exact import paths may be adjusted by the generator to match the generated
crate layout.

## 4) Public Types

The generated module must define:

```rust
#[derive(Debug)]
pub struct StepExecutor {
    input_normalization: InputNormalization,
    query_structuring: QueryStructuring,
    information_adequacy_analyzer: InformationAdequacyAnalyzer,
    observation_boundary_resolver: ObservationBoundaryResolver,
    observation_extraction: ObservationExtraction,
    candidate_card_retrieval: CandidateCardRetrieval,
    card_branch_reranking: CardBranchReranking,
    card_hydration: CardHydration,
    incident_evidence_retrieval: IncidentEvidenceRetrieval,
    theory_evidence_retrieval: TheoryEvidenceRetrieval,
    prompt_context_assembly: PromptContextAssembly,
    diagnostic_update_prompt_context_assembly: DiagnosticUpdatePromptContextAssembly,
    llm_structured_generation: LlmStructuredGeneration,
    response_validation_and_normalization: ResponseValidationAndNormalization,
}

#[derive(Debug)]
pub struct StepExecutorModules {
    pub input_normalization: InputNormalization,
    pub query_structuring: QueryStructuring,
    pub information_adequacy_analyzer: InformationAdequacyAnalyzer,
    pub observation_boundary_resolver: ObservationBoundaryResolver,
    pub observation_extraction: ObservationExtraction,
    pub candidate_card_retrieval: CandidateCardRetrieval,
    pub card_branch_reranking: CardBranchReranking,
    pub card_hydration: CardHydration,
    pub incident_evidence_retrieval: IncidentEvidenceRetrieval,
    pub theory_evidence_retrieval: TheoryEvidenceRetrieval,
    pub prompt_context_assembly: PromptContextAssembly,
    pub diagnostic_update_prompt_context_assembly: DiagnosticUpdatePromptContextAssembly,
    pub llm_structured_generation: LlmStructuredGeneration,
    pub response_validation_and_normalization: ResponseValidationAndNormalization,
}
```

Ownership rules:

- `StepExecutor` owns all request-pipeline leaf modules by value;
- `StepExecutor` must not require `Arc<...>` around the leaf modules in its
  public interface;
- leaf modules may internally use `Arc` or other shared ownership for their own
  private dependencies.

## 5) Constructor

The generated module must define:

```rust
impl StepExecutor {
    pub fn new(modules: StepExecutorModules) -> Self;
}
```

`new` must move all fields from `StepExecutorModules` into `StepExecutor`
without additional validation.

## 6) Public Execution API

The generated module must define:

```rust
impl StepExecutor {
    pub async fn execute(
        &self,
        step: StepKind,
        state: RunStateView<'_>,
    ) -> Result<StepResultEnvelope, StepError>;

    pub async fn execute_with_context(
        &self,
        step: StepKind,
        state: RunStateView<'_>,
        context: &Context,
    ) -> Result<StepResultEnvelope, StepError>;
}
```

API rules:

- `execute` must not mutate `RunState`;
- `execute` must read only from the current run state supplied via
  `RunStateView`;
- `execute` must delegate to `execute_with_context(step, state, &Context::noop())`;
- `execute_with_context` must not mutate `RunState`;
- `execute_with_context` must use the supplied `RunStateView` as its only
  persisted-state source;
- `execute_with_context` must treat the supplied `Context` as an
  orchestration-owned execution-time companion and must not mutate it;
- `execute` must use the current iteration as the direct prerequisite-step
  source for the requested step;
- `execute` may additionally inspect older iterations only when building
  history-derived shared projections required by continuation-only steps;
- `execute` must return `StepError::MissingRequiredInput` when required
  finished-step inputs are absent from the current iteration;
- `execute` must return `StepError::InvalidState` when the current iteration is
  structurally inconsistent with the requested execution;
- `execute` must map successful leaf-module outputs into the matching
  `StepResultEnvelope` variant;
- `execute` must map leaf-module errors into the matching step-specific
  `StepError` variant using `From` conversions defined by
  `run_state::model::StepError`.

## 7) Error Model

`step_executor` does not define a module-local public error enum.

The public execution error type is:

```rust
crate::orchestrator::run_state::model::StepError
```

The generated implementation must use `StepError` as follows:

- missing current iteration -> `StepError::MissingRequiredInput { ... }`;
- missing prerequisite finished step -> `StepError::MissingRequiredInput { ... }`;
- prerequisite finished step recorded `Err(step_error)` ->
  `StepError::MissingRequiredInput { ... }`;
- `StepKind::UserInputReceived` passed to `execute` ->
  `StepError::InvalidState { ... }`;
- prerequisite finished step contains an unexpected success-envelope variant ->
  `StepError::InvalidState { ... }`;
- leaf-module failures must be converted into the matching step-specific
  `StepError` variant via `From`;
- any structurally inconsistent run-state condition not covered above must be
  reported as `StepError::InvalidState { ... }`.

`step_executor` must not introduce a second public error boundary such as
`StepExecutorError`.

## 8) Current Iteration Selection

`execute` must determine the current execution context as follows:

- call `state.last_iteration()`;
- when it returns `None`, return:

```rust
StepError::MissingRequiredInput {
    message: "step execution requires a current iteration".to_string(),
}
```

- otherwise execute only against that returned `IterationView`.

`execute` must not use older iterations as substitutes for missing direct
prerequisite step outputs in the current iteration.

Older iterations may be read only when the requested step explicitly requires a
history-derived projection defined elsewhere in the runtime specs.

## 9) Step Dispatch Rules

`StepKind::UserInputReceived` is not executable through `StepExecutor`.

When `execute` is called with `StepKind::UserInputReceived`, it must return:

```rust
StepError::InvalidState {
    message: "UserInputReceived is recorded by begin_iteration and is not executable".to_string(),
}
```

All other `StepKind` variants are executable and must dispatch to exactly one
matching request-pipeline leaf module.

## 10) Required Run-State Reads Per Step

For each executable `StepKind`, `execute` must read the required direct
prerequisite finished step results from the current iteration using
`IterationView::finished_step(kind)`.

When a step additionally requires a history-derived shared projection,
`execute` must build that projection from the supplied `RunStateView` using the
projection rules already owned by the relevant shared-type spec. The executor
is not required to call a specific convenience constructor when the projection
rules can be implemented directly from `RunStateView`.

Required lookup paths and execution mapping:

| Requested `StepKind` | Required reads from current iteration | Leaf-module call | Success result |
| --- | --- | --- | --- |
| `InputNormalization` | `finished_step(StepKind::UserInputReceived)` | `input_normalization.normalize(user_request)` | `StepResultEnvelope::InputNormalization(...)` |
| `QueryStructuring` | `finished_step(StepKind::InputNormalization)` | `query_structuring.structure_with_context(&normalized_request, context).await` | `StepResultEnvelope::QueryStructuring(...)` |
| `ObservationBoundaryResolver` | `finished_step(StepKind::InputNormalization)` | `observation_boundary_resolver.resolve_with_context(&normalized_request, &diagnostic_context, context).await` | `StepResultEnvelope::ObservationBoundaryResolver(...)` |
| `ObservationExtraction` | `finished_step(StepKind::ObservationBoundaryResolver)` | `observation_extraction.extract_with_context(&boundary_output, context).await` | `StepResultEnvelope::ObservationExtraction(...)` |
| `InformationAdequacyInitial` | `finished_step(StepKind::QueryStructuring)` | `information_adequacy_analyzer.analyze_initial(&structured_query)` | `StepResultEnvelope::InformationAdequacy(...)` |
| `InformationAdequacySupportedObservation` | `finished_step(StepKind::ObservationExtraction)` | `information_adequacy_analyzer.analyze_supported_observation(&extraction_output)` | `StepResultEnvelope::InformationAdequacy(...)` |
| `InformationAdequacyUnsupportedObservation` | `finished_step(StepKind::ObservationBoundaryResolver)` | `information_adequacy_analyzer.analyze_unsupported_observation(&boundary_output)` | `StepResultEnvelope::InformationAdequacy(...)` |
| `CandidateCardRetrieval` | `finished_step(StepKind::InputNormalization)` | `candidate_card_retrieval.retrieve_with_context(&retrieval_query, context).await` | `StepResultEnvelope::CandidateCardRetrieval(...)` |
| `CardBranchReranking` | `finished_step(StepKind::CandidateCardRetrieval)` | `card_branch_reranking.rerank(&fresh_candidates, &card_selection_context)` | `StepResultEnvelope::CardBranchReranking(...)` |
| `CardHydration` | `finished_step(StepKind::CandidateCardRetrieval)` and optionally `finished_step(StepKind::CardBranchReranking)` | `card_hydration.hydrate(&hydration_candidates).await` | `StepResultEnvelope::CardHydration(...)` |
| `IncidentEvidenceRetrieval` | `finished_step(StepKind::InputNormalization)`, `finished_step(StepKind::CandidateCardRetrieval)`, and optionally `finished_step(StepKind::CardBranchReranking)` | `incident_evidence_retrieval.retrieve_with_context(&retrieval_query, &card_branches, iteration_profile, context).await` | `StepResultEnvelope::IncidentEvidenceRetrieval(...)` |
| `TheoryEvidenceRetrieval` | `finished_step(StepKind::InputNormalization)` | `theory_evidence_retrieval.retrieve_with_context(&retrieval_query, context).await` | `StepResultEnvelope::TheoryEvidenceRetrieval(...)` |
| `PromptContextAssembly` | `finished_step(StepKind::InputNormalization)`, `finished_step(StepKind::QueryStructuring)`, `finished_step(StepKind::CardHydration)`, `finished_step(StepKind::IncidentEvidenceRetrieval)`, and `finished_step(StepKind::TheoryEvidenceRetrieval)` | `prompt_context_assembly.assemble_with_context(&normalized_request, &structured_query, &hydrated_cards, &incident_evidence, &theory_evidence, context)` | `StepResultEnvelope::PromptContextAssembly(...)` |
| `DiagnosticUpdatePromptContextAssembly` | `finished_step(StepKind::ObservationBoundaryResolver)`, `finished_step(StepKind::ObservationExtraction)`, `finished_step(StepKind::CardHydration)`, `finished_step(StepKind::IncidentEvidenceRetrieval)`, and `finished_step(StepKind::TheoryEvidenceRetrieval)` | `diagnostic_update_prompt_context_assembly.assemble_with_context(&problem_understanding, &resolved_observation, &extracted_observations, &cards, &incident_evidence, &theory_evidence, &active_hypotheses, &rejected_hypotheses, last_check, context)` | `StepResultEnvelope::DiagnosticUpdatePromptContextAssembly(...)` |
| `LlmStructuredGeneration` | either `finished_step(StepKind::PromptContextAssembly)` or `finished_step(StepKind::DiagnosticUpdatePromptContextAssembly)` depending on the current iteration profile | `llm_structured_generation.generate_with_context(&prompt_context, context).await` | `StepResultEnvelope::LlmStructuredGeneration(...)` |
| `ResponseValidationAndNormalization` | `finished_step(StepKind::LlmStructuredGeneration)` | `response_validation_and_normalization.validate_and_normalize_with_context(&llm_output, context)` | `StepResultEnvelope::ResponseValidationAndNormalization(...)` |

Source rules for shared adapters:

- `retrieval_query` must be built as:

```rust
RetrievalQueryInput {
    query_text: ...,
}
```

with these rules:

- when the current iteration index is `0`, `retrieval_query.query_text` must be
  `normalized_request.query.clone()`;
- when the current iteration index is greater than `0`,
  `retrieval_query.query_text` must be built from:
  - the latest closed `ProblemUnderstanding.text` from the history-derived
    `DiagnosticContext`; and
  - the trimmed `ResolvedObservation.text` from the successful current-iteration
    `ObservationBoundaryResolverOutput.resolution = Supported(...)`;
- for continuation iterations, `retrieval_query.query_text` must be the
  concatenation of those two strings separated by exactly one ASCII space;
- the previous closed problem-understanding string must appear first, followed
  by the resolved observation string;
- neither input string may be silently omitted in the continuation path;
- when the current iteration index is greater than `0` and no supported
  resolved observation is available, `execute` must return
  `StepError::InvalidState { ... }`.

- `diagnostic_context` must be projected from the supplied `RunStateView`
  using the projection rules owned by
  `Specification/runtime/request_pipeline/diagnostic_context.md`;
- `card_selection_context` must be projected from the supplied `RunStateView`
  using the projection rules owned by
  `Specification/runtime/request_pipeline/card_selection_context.md`;
- `iteration_profile` must be:
  - `IterationProfile::Initial` when the current iteration index is `0`;
  - `IterationProfile::Continuation` when the current iteration index is
    greater than `0`;
- `structured_query` for `InformationAdequacyInitial` must
  be taken from the successful current-iteration
  `StepResultEnvelope::QueryStructuring(...)`;
- `extraction_output` for `InformationAdequacySupportedObservation` must be
  taken from the successful current-iteration
  `StepResultEnvelope::ObservationExtraction(...)`;
- `boundary_output` for `InformationAdequacyUnsupportedObservation` must be
  taken from the successful current-iteration
  `StepResultEnvelope::ObservationBoundaryResolver(...)`;
- `InformationAdequacyUnsupportedObservation` must return
  `StepError::InvalidState { ... }` when `boundary_output.resolution` is not
  `ObservationBoundaryResolution::Unsupported`;
- `card_branches` for `IncidentEvidenceRetrieval` must be built as:
  - from `CandidateCardRetrievalOutput` when `iteration_profile` is `Initial`;
  - from `CardBranchRerankingOutput` when `iteration_profile` is
    `Continuation`;
- `hydration_candidates` for `CardHydration` must be:
  - the original `CandidateCardRetrievalOutput` when no successful
    `CardBranchReranking` result exists in the current iteration;
  - otherwise a compatibility `CandidateCardRetrievalOutput`-shaped adapter
    built from the current iteration's fresh retrieval output plus the current
    iteration's `CardBranchRerankingOutput`, preserving the reranked
    `primary` / `alternatives` branch assignment;
- `cards` for `DiagnosticUpdatePromptContextAssembly` must be built as
  `HydratedCardBranchesInput` from the current iteration's
  `CardHydrationOutput`;
- when building `HydratedCardBranchesInput`, `CardHydrationOutput.primary`
  must be `Some(primary)`; otherwise `execute` must return
  `StepError::InvalidState { ... }`;
- `problem_understanding` for `DiagnosticUpdatePromptContextAssembly` must be
  the latest `ProblemUnderstanding` entry in the history-derived
  `DiagnosticContext` whose `text` is `Some(non-empty string)` after trimming;
- if no such closed problem-understanding entry exists, `execute` must return
  `StepError::InvalidState { ... }`;
- `active_hypotheses`, `rejected_hypotheses`, and `last_check` for
  `DiagnosticUpdatePromptContextAssembly` must be taken from the
  history-derived `DiagnosticContext`;
- `resolved_observation` for `DiagnosticUpdatePromptContextAssembly` must be
  taken from the successful current-iteration
  `ObservationBoundaryResolverOutput.resolution`, and the executor must return
  `StepError::InvalidState { ... }` when that resolution is not
  `Supported(...)`.

## 11) Finished-Step Decoding Rules

For every required lookup, `execute` must:

1. call `iteration.finished_step(required_kind)`;
2. return `StepError::MissingRequiredInput { ... }` when the result is `None`;
3. inspect `FinishedStepView::result()`;
4. require that the stored result is `Ok(...)`;
5. extract the step-specific payload from the matching `StepResultEnvelope`
   variant.

When a required finished step exists but contains `Err(error)`, `execute` must
return:

```rust
StepError::MissingRequiredInput {
    message: format!(
        "required input step {:?} did not complete successfully: {}",
        required_kind,
        error
    ),
}
```

When a required finished step exists but its success envelope variant does not
match the required `StepKind`, `execute` must return:

```rust
StepError::InvalidState {
    message: format!(
        "required input step {:?} stored an unexpected result variant",
        required_kind
    ),
}
```

The generated implementation may define private typed helper functions for
extracting each required step payload.

## 12) User Input Read Rule

For `StepKind::InputNormalization`, `execute` must read:

- `iteration.finished_step(StepKind::UserInputReceived)`.

That lookup must decode:

- `Ok(StepResultEnvelope::UserInputReceived(user_request))`.

The extracted `UserRequest` must be passed by value to:

- `input_normalization.normalize(user_request)`.

Because `FinishedStepView::result()` returns a borrowed value, the implementation
may clone the stored `UserRequest` before calling `normalize`.

## 13) Borrowing And Cloning Rules

`execute` must prefer borrowed reads from `RunStateView`.

Cloning rules:

- cloning extracted shared runtime payloads is allowed when required by the leaf
  module method signature;
- `execute` must not clone the whole `RunState`;
- `execute` must not clone unrelated step payloads;
- `execute` must not materialize an owned iteration snapshot solely for
  convenience.
- `execute_with_context` must prefer passing the borrowed `Context` unchanged to
  context-aware leaf-module calls rather than cloning it.

## 14) Ordering And Pending-Step Rules

`StepExecutor` is a dispatch module, not a policy module.

Therefore:

- `execute` must not decide whether the requested `step` is the best or next
  step to run;
- `execute` must execute any executable `StepKind` as long as the required input
  steps are present in the current iteration;
- `execute` must not read pending-step records as execution inputs.

The current version must not require that prerequisite steps appear
immediately before the requested step in record order.

## 15) Private Helper Allowances

The generated module may define private helpers for:

- loading the current `IterationView`;
- computing the current iteration index;
- loading a required `FinishedStepView` by `StepKind`;
- decoding typed success payloads from `FinishedStepView::result()`;
- building history-derived shared projections such as `DiagnosticContext` and
  `CardSelectionContext`;
- building compatibility adapters such as `RetrievalQueryInput`,
  `IncidentEvidenceCardBranchesInput`, `HydratedCardBranchesInput`, and a
  reranked `CandidateCardRetrievalOutput` hydration input;
- constructing standardized `MissingRequiredInput` messages;
- dispatching the per-step execution match.

Private helpers must not expose new public runtime APIs.

## 16) Unit-Test Ownership

Required unit tests for `step_executor` are owned by:

- `Specification/runtime/unit_tests.md`
- `Specification/runtime/unit_tests_common.md`

This document defines runtime behavior and API contracts only. It must not be
treated as the source of truth for the crate-level required unit-test list.

## 17) Ownership Boundaries

- `step_executor.md` owns the runtime execution-dispatch boundary between
  orchestration and request-pipeline leaf modules.
- `step_executor.md` must not define transition-selection policy.
- `step_executor.md` must not define persistence or repository behavior.
- `step_executor.md` must not redefine leaf-module contracts already owned by
  `Specification/runtime/request_pipeline/...`.
