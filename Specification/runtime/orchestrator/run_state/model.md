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
    CandidateCardRetrievalOutput,
    CardHydrationOutput,
    IncidentEvidenceRetrievalOutput,
    LlmStructuredGenerationOutput,
    NormalizedUserRequest,
    PromptContextAssemblyOutput,
    QueryStructuringOutput,
    ResponseValidationAndNormalizationOutput,
    TheoryEvidenceRetrievalOutput,
    UserRequest,
};
use crate::request_pipeline::candidate_card_retrieval::CandidateCardRetrievalError;
use crate::request_pipeline::card_hydration::CardHydrationError;
use crate::request_pipeline::incident_evidence_retrieval::IncidentEvidenceRetrievalError;
use crate::request_pipeline::input_normalization::InputNormalizationError;
use crate::request_pipeline::llm_structured_generation::LlmStructuredGenerationError;
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
    CandidateCardRetrieval,
    CardHydration,
    IncidentEvidenceRetrieval,
    TheoryEvidenceRetrieval,
    PromptContextAssembly,
    LlmStructuredGeneration,
    ResponseValidationAndNormalization,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StepResultEnvelope {
    UserInputReceived(UserRequest),
    InputNormalization(NormalizedUserRequest),
    QueryStructuring(QueryStructuringOutput),
    CandidateCardRetrieval(CandidateCardRetrievalOutput),
    CardHydration(CardHydrationOutput),
    IncidentEvidenceRetrieval(IncidentEvidenceRetrievalOutput),
    TheoryEvidenceRetrieval(TheoryEvidenceRetrievalOutput),
    PromptContextAssembly(PromptContextAssemblyOutput),
    LlmStructuredGeneration(LlmStructuredGenerationOutput),
    ResponseValidationAndNormalization(ResponseValidationAndNormalizationOutput),
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
    CandidateCardRetrieval(#[from] CandidateCardRetrievalError),

    #[error(transparent)]
    CardHydration(#[from] CardHydrationError),

    #[error(transparent)]
    IncidentEvidenceRetrieval(#[from] IncidentEvidenceRetrievalError),

    #[error(transparent)]
    TheoryEvidenceRetrieval(#[from] TheoryEvidenceRetrievalError),

    #[error(transparent)]
    PromptContextAssembly(#[from] PromptContextAssemblyError),

    #[error(transparent)]
    LlmStructuredGeneration(#[from] LlmStructuredGenerationError),

    #[error(transparent)]
    ResponseValidationAndNormalization(#[from] ResponseValidationAndNormalizationError),

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
- a successful user-facing response from one orchestration invocation does not
  by itself move the run into a separate terminal success status;
- `RunStatus::Archived` is the current explicit closed-state marker for a run;
- `RunStatus::Error` means the last attempted step recording ended in failure,
  but the run may still later be resumed or superseded by a new iteration if
  higher-level product behavior allows that.

`FinishedStepRecord.result` success variant must match `FinishedStepRecord.step`:

| `step` | `result` variant |
| --- | --- |
| `StepKind::UserInputReceived` | `StepResultEnvelope::UserInputReceived(_)` |
| `StepKind::InputNormalization` | `StepResultEnvelope::InputNormalization(_)` |
| `StepKind::QueryStructuring` | `StepResultEnvelope::QueryStructuring(_)` |
| `StepKind::CandidateCardRetrieval` | `StepResultEnvelope::CandidateCardRetrieval(_)` |
| `StepKind::CardHydration` | `StepResultEnvelope::CardHydration(_)` |
| `StepKind::IncidentEvidenceRetrieval` | `StepResultEnvelope::IncidentEvidenceRetrieval(_)` |
| `StepKind::TheoryEvidenceRetrieval` | `StepResultEnvelope::TheoryEvidenceRetrieval(_)` |
| `StepKind::PromptContextAssembly` | `StepResultEnvelope::PromptContextAssembly(_)` |
| `StepKind::LlmStructuredGeneration` | `StepResultEnvelope::LlmStructuredGeneration(_)` |
| `StepKind::ResponseValidationAndNormalization` | `StepResultEnvelope::ResponseValidationAndNormalization(_)` |

`FinishedStepRecord.result` step-specific error variants must match
`FinishedStepRecord.step`:

| `step` | `error` variant |
| --- | --- |
| `StepKind::InputNormalization` | `StepError::InputNormalization(_)` |
| `StepKind::QueryStructuring` | `StepError::QueryStructuring(_)` |
| `StepKind::CandidateCardRetrieval` | `StepError::CandidateCardRetrieval(_)` |
| `StepKind::CardHydration` | `StepError::CardHydration(_)` |
| `StepKind::IncidentEvidenceRetrieval` | `StepError::IncidentEvidenceRetrieval(_)` |
| `StepKind::TheoryEvidenceRetrieval` | `StepError::TheoryEvidenceRetrieval(_)` |
| `StepKind::PromptContextAssembly` | `StepError::PromptContextAssembly(_)` |
| `StepKind::LlmStructuredGeneration` | `StepError::LlmStructuredGeneration(_)` |
| `StepKind::ResponseValidationAndNormalization` | `StepError::ResponseValidationAndNormalization(_)` |

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
