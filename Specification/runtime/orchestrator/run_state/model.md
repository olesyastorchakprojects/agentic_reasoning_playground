## 1) Purpose

This document defines the public persisted model types generated for
`orchestrator::run_state::model`.

## 2) Generated Rust Artifact

The generated Rust crate must include:

- `src/orchestrator/run_state/model.rs`

Parent module exposure:

- `src/orchestrator/run_state/mod.rs` must expose `model`.

All types and fields defined by this spec must be public.

## 3) Imports

The generated module requires:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, Display, EnumString};
use thiserror::Error;
use uuid::Uuid;
```

The generated module also imports these shared runtime types and step error
types:

```rust
use crate::shared_types::{
    AdequacyAssessment,
    CandidateCardRetrievalOutput,
    CardBranchRerankingOutput,
    CardHydrationOutput,
    IncidentEvidenceRetrievalOutput,
    LlmStructuredGenerationOutput,
    NormalizedUserRequest,
    ObservationExtractionOutput,
    ObservationBoundaryResolverOutput,
    PromptContextAssemblyOutput,
    QueryStructuringOutput,
    ResponseValidationAndNormalizationOutput,
    RunConfigSnapshot,
    TheoryEvidenceRetrievalOutput,
    UserRequest,
};
use crate::request_pipeline::candidate_card_retrieval::CandidateCardRetrievalError;
use crate::request_pipeline::card_branch_reranking::CardBranchRerankingError;
use crate::request_pipeline::card_hydration::CardHydrationError;
use crate::request_pipeline::diagnostic_update_prompt_context_assembly::DiagnosticUpdatePromptContextAssemblyError;
use crate::request_pipeline::information_adequacy_analyzer::InformationAdequacyAnalyzerError;
use crate::request_pipeline::incident_evidence_retrieval::IncidentEvidenceRetrievalError;
use crate::request_pipeline::input_normalization::InputNormalizationError;
use crate::request_pipeline::llm_structured_generation::LlmStructuredGenerationError;
use crate::request_pipeline::observation_boundary_resolver::ObservationBoundaryResolverError;
use crate::request_pipeline::observation_extraction::ObservationExtractionError;
use crate::request_pipeline::prompt_context_assembly::PromptContextAssemblyError;
use crate::request_pipeline::query_structuring::QueryStructuringError;
use crate::request_pipeline::response_validation_and_normalization::ResponseValidationAndNormalizationError;
use crate::request_pipeline::theory_evidence_retrieval::TheoryEvidenceRetrievalError;
```

Exact import paths may be adjusted by the generator to match the generated
crate layout.

## 4) Public Types

The generated module must define:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StepRecordId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunIterationId(pub Uuid);

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    EnumString,
    Display,
    AsRefStr,
)]
pub enum RunStatus {
    Active,
    WaitingForUser,
    Error,
    Archived,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    EnumString,
    Display,
    AsRefStr,
)]
pub enum RunIterationStatus {
    Active,
    FinishedWithSuccess,
    FinishedWithError,
    FinishedWithWaitInput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunState {
    pub run_id: RunId,
    pub status: RunStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: u64,
    pub iterations: Vec<RunIteration>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    EnumString,
    Display,
    AsRefStr,
)]
pub enum StepKind {
    UserInputReceived,
    InputNormalization,
    QueryStructuring,
    InformationAdequacyInitial,
    InformationAdequacySupportedObservation,
    InformationAdequacyUnsupportedObservation,
    CandidateCardRetrieval,
    CardBranchReranking,
    CardHydration,
    IncidentEvidenceRetrieval,
    TheoryEvidenceRetrieval,
    PromptContextAssembly,
    DiagnosticUpdatePromptContextAssembly,
    LlmStructuredGeneration,
    ResponseValidationAndNormalization,
    ObservationBoundaryResolver,
    ObservationExtraction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StepResultEnvelope {
    UserInputReceived(UserRequest),
    InputNormalization(NormalizedUserRequest),
    QueryStructuring(QueryStructuringOutput),
    InformationAdequacy(AdequacyAssessment),
    CandidateCardRetrieval(CandidateCardRetrievalOutput),
    CardBranchReranking(CardBranchRerankingOutput),
    CardHydration(CardHydrationOutput),
    IncidentEvidenceRetrieval(IncidentEvidenceRetrievalOutput),
    TheoryEvidenceRetrieval(TheoryEvidenceRetrievalOutput),
    PromptContextAssembly(PromptContextAssemblyOutput),
    DiagnosticUpdatePromptContextAssembly(PromptContextAssemblyOutput),
    LlmStructuredGeneration(LlmStructuredGenerationOutput),
    ResponseValidationAndNormalization(ResponseValidationAndNormalizationOutput),
    ObservationBoundaryResolver(ObservationBoundaryResolverOutput),
    ObservationExtraction(ObservationExtractionOutput),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Error)]
pub enum StepError {
    #[error("missing required input: {message}")]
    MissingRequiredInput { message: String },

    #[error("invalid state: {message}")]
    InvalidState { message: String },

    #[error(transparent)]
    InputNormalization(#[from] InputNormalizationError),

    #[error(transparent)]
    QueryStructuring(#[from] QueryStructuringError),

    #[error(transparent)]
    InformationAdequacy(#[from] InformationAdequacyAnalyzerError),

    #[error(transparent)]
    CandidateCardRetrieval(#[from] CandidateCardRetrievalError),

    #[error(transparent)]
    CardBranchReranking(#[from] CardBranchRerankingError),

    #[error(transparent)]
    CardHydration(#[from] CardHydrationError),

    #[error(transparent)]
    IncidentEvidenceRetrieval(#[from] IncidentEvidenceRetrievalError),

    #[error(transparent)]
    TheoryEvidenceRetrieval(#[from] TheoryEvidenceRetrievalError),

    #[error(transparent)]
    PromptContextAssembly(#[from] PromptContextAssemblyError),

    #[error(transparent)]
    DiagnosticUpdatePromptContextAssembly(#[from] DiagnosticUpdatePromptContextAssemblyError),

    #[error(transparent)]
    LlmStructuredGeneration(#[from] LlmStructuredGenerationError),

    #[error(transparent)]
    ResponseValidationAndNormalization(#[from] ResponseValidationAndNormalizationError),

    #[error(transparent)]
    ObservationBoundaryResolver(#[from] ObservationBoundaryResolverError),

    #[error(transparent)]
    ObservationExtraction(#[from] ObservationExtractionError),

    #[error("external dependency failure: {message}")]
    ExternalDependency { message: String },

    #[error("unexpected step failure: {message}")]
    Unexpected { message: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StepRecord {
    Pending(PendingStepRecord),
    Finished(FinishedStepRecord),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingStepRecord {
    pub record_id: StepRecordId,
    pub step: StepKind,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinishedStepRecord {
    pub record_id: StepRecordId,
    pub step: StepKind,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub result: Result<StepResultEnvelope, StepError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunIteration {
    pub iteration_id: RunIterationId,
    pub config_snapshot: Option<RunConfigSnapshot>,
    pub status: RunIterationStatus,
    pub step_records: Vec<StepRecord>,
}
```

## 5) Constructors

The generated module must define:

```rust
impl RunState {
    pub fn new() -> Self;
}
```

`RunState::new()` must construct a valid empty run header suitable for
`RunRepository::create_run(&state)`.

It must:

- generate `run_id` with `Uuid::new_v4()`;
- set `status = RunStatus::Active`;
- set `created_at = Utc::now()`;
- set `updated_at = created_at`;
- set `revision = 0`;
- set `iterations = Vec::new()`.

## 6) Model Invariants

`RunState` timestamp invariant:

- `updated_at >= created_at`.

`StepRecord` timestamp invariant:

- for `StepRecord::Finished(record)`, `record.finished_at >= record.started_at`.

Pending step invariant:

- at most one `StepRecord::Pending(_)` may exist in a `RunState`.

`RunStatus` semantics:

- `RunStatus::Active` means that the run remains open for future orchestration
  invocations, including retry with the same iteration or continuation with a
  later user input;
- `RunStatus::WaitingForUser` means the latest orchestration invocation paused
  after requesting follow-up input and the next supported continuation path is
  `resume_with_input(...)` with a newly appended iteration;
- a successful user-facing response from one orchestration invocation does not
  by itself move the run into a separate terminal success status;
- `RunStatus::Archived` is the current explicit closed-state marker for a run;
- `RunStatus::Error` means the latest orchestration attempt for the current
  iteration ended in failure, whether through a failed executable step or a
  policy-driven error finish; the run may still later be resumed or
  superseded by a new iteration if higher-level product behavior allows that.

`RunIterationStatus` semantics:

- `RunIterationStatus::Active` means the iteration remains eligible for
  further step execution in the current version;
- `RunIterationStatus::FinishedWithSuccess` means the iteration completed
  through `PolicyTransition::FinishWithResult`;
- `RunIterationStatus::FinishedWithError` means the iteration completed
  through either a failed executable step or
  `PolicyTransition::FinishWithError`;
- `RunIterationStatus::FinishedWithWaitInput` means the iteration completed
  through `PolicyTransition::WaitForUser` and is a short iteration for
  history-view purposes.

Iteration-status invariants:

- at most one `RunIteration` in a `RunState` may have
  `status = RunIterationStatus::Active`;
- if an active iteration exists, it must be the last element of
  `RunState.iterations`;
- any iteration whose `status != RunIterationStatus::Active` must contain no
  pending step record.

`FinishedStepRecord.result` success variant must match `FinishedStepRecord.step`:

| `step` | `result` variant |
| --- | --- |
| `StepKind::UserInputReceived` | `StepResultEnvelope::UserInputReceived(_)` |
| `StepKind::InputNormalization` | `StepResultEnvelope::InputNormalization(_)` |
| `StepKind::QueryStructuring` | `StepResultEnvelope::QueryStructuring(_)` |
| `StepKind::InformationAdequacyInitial` | `StepResultEnvelope::InformationAdequacy(_)` |
| `StepKind::InformationAdequacySupportedObservation` | `StepResultEnvelope::InformationAdequacy(_)` |
| `StepKind::InformationAdequacyUnsupportedObservation` | `StepResultEnvelope::InformationAdequacy(_)` |
| `StepKind::CandidateCardRetrieval` | `StepResultEnvelope::CandidateCardRetrieval(_)` |
| `StepKind::CardBranchReranking` | `StepResultEnvelope::CardBranchReranking(_)` |
| `StepKind::CardHydration` | `StepResultEnvelope::CardHydration(_)` |
| `StepKind::IncidentEvidenceRetrieval` | `StepResultEnvelope::IncidentEvidenceRetrieval(_)` |
| `StepKind::TheoryEvidenceRetrieval` | `StepResultEnvelope::TheoryEvidenceRetrieval(_)` |
| `StepKind::PromptContextAssembly` | `StepResultEnvelope::PromptContextAssembly(_)` |
| `StepKind::DiagnosticUpdatePromptContextAssembly` | `StepResultEnvelope::DiagnosticUpdatePromptContextAssembly(_)` |
| `StepKind::LlmStructuredGeneration` | `StepResultEnvelope::LlmStructuredGeneration(_)` |
| `StepKind::ResponseValidationAndNormalization` | `StepResultEnvelope::ResponseValidationAndNormalization(_)` |
| `StepKind::ObservationBoundaryResolver` | `StepResultEnvelope::ObservationBoundaryResolver(_)` |
| `StepKind::ObservationExtraction` | `StepResultEnvelope::ObservationExtraction(_)` |

`FinishedStepRecord.result` step-specific error variants must match
`FinishedStepRecord.step`:

| `step` | `error` variant |
| --- | --- |
| `StepKind::InputNormalization` | `StepError::InputNormalization(_)` |
| `StepKind::QueryStructuring` | `StepError::QueryStructuring(_)` |
| `StepKind::InformationAdequacyInitial` | `StepError::InformationAdequacy(_)` |
| `StepKind::InformationAdequacySupportedObservation` | `StepError::InformationAdequacy(_)` |
| `StepKind::InformationAdequacyUnsupportedObservation` | `StepError::InformationAdequacy(_)` |
| `StepKind::CandidateCardRetrieval` | `StepError::CandidateCardRetrieval(_)` |
| `StepKind::CardBranchReranking` | `StepError::CardBranchReranking(_)` |
| `StepKind::CardHydration` | `StepError::CardHydration(_)` |
| `StepKind::IncidentEvidenceRetrieval` | `StepError::IncidentEvidenceRetrieval(_)` |
| `StepKind::TheoryEvidenceRetrieval` | `StepError::TheoryEvidenceRetrieval(_)` |
| `StepKind::PromptContextAssembly` | `StepError::PromptContextAssembly(_)` |
| `StepKind::DiagnosticUpdatePromptContextAssembly` | `StepError::DiagnosticUpdatePromptContextAssembly(_)` |
| `StepKind::LlmStructuredGeneration` | `StepError::LlmStructuredGeneration(_)` |
| `StepKind::ResponseValidationAndNormalization` | `StepError::ResponseValidationAndNormalization(_)` |
| `StepKind::ObservationBoundaryResolver` | `StepError::ObservationBoundaryResolver(_)` |
| `StepKind::ObservationExtraction` | `StepError::ObservationExtraction(_)` |

Non-step-specific `StepError` variants may be used for any `StepKind`.

`StepError` text variant invariant:

- `message` fields must not be empty.

## 6) Generation Requirements

The generated Rust derives are part of the type contract shown above.

Unit enums must derive:

- `strum_macros::EnumString`;
- `strum_macros::Display`;
- `strum_macros::AsRefStr`.

`StepResultEnvelope`, `StepError`, and `StepRecord` must not derive strum
traits in the current version because their variants carry payload values.

The generator must ensure all request-pipeline error types carried by
`StepError` derive `Clone`, `PartialEq`, `Serialize`, and `Deserialize`.

The generator may add ordering traits where supported by all fields.

## 7) Ownership Boundaries

- `model.md` owns generated persisted model type definitions.
- `view.md` owns typed read access over this model.
- `apply.md` owns mutation/update operations over this model.
