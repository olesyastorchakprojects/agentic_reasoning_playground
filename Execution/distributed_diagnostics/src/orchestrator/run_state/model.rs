use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, Display, EnumString};
use thiserror::Error;
use uuid::Uuid;

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::str::FromStr;

    use chrono::Utc;
    use uuid::Uuid;

    use crate::orchestrator::run_state::model::{
        FinishedStepRecord, PendingStepRecord, RunId, RunIteration, RunIterationId, RunState,
        RunStatus, StepError, StepKind, StepRecord, StepRecordId, StepResultEnvelope,
    };
    use crate::request_pipeline::input_normalization::InputNormalizationError;
    use crate::shared_types::UserRequest;

    // ─── Helpers ──────────────────────────────────────────────────────────────

    fn user_req() -> UserRequest {
        UserRequest {
            query: "test query".to_string(),
        }
    }

    fn run_id() -> RunId {
        RunId(Uuid::new_v4())
    }

    fn step_record_id() -> StepRecordId {
        StepRecordId(Uuid::new_v4())
    }

    fn iteration_id() -> RunIterationId {
        RunIterationId(Uuid::new_v4())
    }

    fn pending(kind: StepKind) -> StepRecord {
        StepRecord::Pending(PendingStepRecord {
            record_id: step_record_id(),
            step: kind,
            started_at: Utc::now(),
        })
    }

    fn finished_ok(kind: StepKind, result: StepResultEnvelope) -> StepRecord {
        let now = Utc::now();
        StepRecord::Finished(FinishedStepRecord {
            record_id: step_record_id(),
            step: kind,
            started_at: now,
            finished_at: now,
            result: Ok(result),
        })
    }

    fn finished_err(kind: StepKind, error: StepError) -> StepRecord {
        let now = Utc::now();
        StepRecord::Finished(FinishedStepRecord {
            record_id: step_record_id(),
            step: kind,
            started_at: now,
            finished_at: now,
            result: Err(error),
        })
    }

    fn user_input_result() -> StepResultEnvelope {
        StepResultEnvelope::UserInputReceived(user_req())
    }

    // ─── Module structure: types importable from documented paths ─────────────

    #[test]
    fn public_types_importable_from_model_path() {
        let _: RunId = RunId(Uuid::new_v4());
        let _: StepRecordId = StepRecordId(Uuid::new_v4());
        let _: RunIterationId = RunIterationId(Uuid::new_v4());
        let _: RunStatus = RunStatus::Active;
        let _: StepKind = StepKind::UserInputReceived;
    }

    // ─── Id newtypes: Copy ────────────────────────────────────────────────────

    #[test]
    fn run_id_is_copy() {
        let id = run_id();
        let id2 = id;
        assert_eq!(id, id2);
    }

    #[test]
    fn step_record_id_is_copy() {
        let id = step_record_id();
        let id2 = id;
        assert_eq!(id, id2);
    }

    #[test]
    fn run_iteration_id_is_copy() {
        let id = iteration_id();
        let id2 = id;
        assert_eq!(id, id2);
    }

    // ─── Id newtypes: Hash ────────────────────────────────────────────────────

    #[test]
    fn run_id_is_hashable() {
        let id = run_id();
        let mut set = HashSet::new();
        set.insert(id);
        assert!(set.contains(&id));
    }

    #[test]
    fn step_record_id_is_hashable() {
        let id = step_record_id();
        let mut set = HashSet::new();
        set.insert(id);
        assert!(set.contains(&id));
    }

    #[test]
    fn run_iteration_id_is_hashable() {
        let id = iteration_id();
        let mut set = HashSet::new();
        set.insert(id);
        assert!(set.contains(&id));
    }

    // ─── Id newtypes: Serialize / Deserialize ─────────────────────────────────

    #[test]
    fn run_id_serializes_and_deserializes() {
        let id = run_id();
        let json = serde_json::to_string(&id).unwrap();
        let back: RunId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn step_record_id_serializes_and_deserializes() {
        let id = step_record_id();
        let json = serde_json::to_string(&id).unwrap();
        let back: StepRecordId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn run_iteration_id_serializes_and_deserializes() {
        let id = iteration_id();
        let json = serde_json::to_string(&id).unwrap();
        let back: RunIterationId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    // ─── RunStatus: serialize / deserialize ───────────────────────────────────

    #[test]
    fn run_status_all_variants_round_trip() {
        for status in [
            RunStatus::Active,
            RunStatus::WaitingForUser,
            RunStatus::Error,
            RunStatus::Archived,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let back: RunStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, back, "round-trip failed for {status:?}");
        }
    }

    // ─── RunStatus: strum display and parsing ─────────────────────────────────

    #[test]
    fn run_status_display_and_from_str() {
        let cases = [
            (RunStatus::Active, "Active"),
            (RunStatus::WaitingForUser, "WaitingForUser"),
            (RunStatus::Error, "Error"),
            (RunStatus::Archived, "Archived"),
        ];
        for (status, expected) in cases {
            assert_eq!(status.to_string(), expected);
            assert_eq!(RunStatus::from_str(expected).unwrap(), status);
        }
    }

    // ─── StepKind: serialize / deserialize ────────────────────────────────────

    #[test]
    fn step_kind_all_variants_round_trip() {
        let kinds = [
            StepKind::UserInputReceived,
            StepKind::InputNormalization,
            StepKind::QueryStructuring,
            StepKind::CandidateCardRetrieval,
            StepKind::CardHydration,
            StepKind::IncidentEvidenceRetrieval,
            StepKind::TheoryEvidenceRetrieval,
            StepKind::PromptContextAssembly,
            StepKind::LlmStructuredGeneration,
            StepKind::ResponseValidationAndNormalization,
        ];
        for kind in kinds {
            let json = serde_json::to_string(&kind).unwrap();
            let back: StepKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, back, "round-trip failed for {kind:?}");
        }
    }

    // ─── StepKind: strum display and parsing ──────────────────────────────────

    #[test]
    fn step_kind_display_and_from_str() {
        let cases = [
            (StepKind::UserInputReceived, "UserInputReceived"),
            (StepKind::InputNormalization, "InputNormalization"),
            (StepKind::QueryStructuring, "QueryStructuring"),
            (StepKind::CandidateCardRetrieval, "CandidateCardRetrieval"),
            (StepKind::CardHydration, "CardHydration"),
            (
                StepKind::IncidentEvidenceRetrieval,
                "IncidentEvidenceRetrieval",
            ),
            (StepKind::TheoryEvidenceRetrieval, "TheoryEvidenceRetrieval"),
            (StepKind::PromptContextAssembly, "PromptContextAssembly"),
            (StepKind::LlmStructuredGeneration, "LlmStructuredGeneration"),
            (
                StepKind::ResponseValidationAndNormalization,
                "ResponseValidationAndNormalization",
            ),
        ];
        for (kind, expected) in cases {
            assert_eq!(kind.to_string(), expected);
            assert_eq!(StepKind::from_str(expected).unwrap(), kind);
        }
    }

    // ─── StepRecord::Pending: serde round-trip ────────────────────────────────

    #[test]
    fn pending_step_record_round_trip_preserves_all_fields() {
        let record_id = step_record_id();
        let started_at = Utc::now();
        let record = StepRecord::Pending(PendingStepRecord {
            record_id,
            step: StepKind::InputNormalization,
            started_at,
        });
        let json = serde_json::to_string(&record).unwrap();
        let back: StepRecord = serde_json::from_str(&json).unwrap();
        match back {
            StepRecord::Pending(p) => {
                assert_eq!(p.record_id, record_id);
                assert_eq!(p.step, StepKind::InputNormalization);
                assert_eq!(p.started_at, started_at);
            }
            _ => panic!("expected Pending after round-trip"),
        }
    }

    // ─── StepRecord::Finished with Ok: serde round-trip ──────────────────────

    #[test]
    fn finished_step_record_ok_round_trip_preserves_payload() {
        let record_id = step_record_id();
        let req = user_req();
        let now = Utc::now();
        let record = StepRecord::Finished(FinishedStepRecord {
            record_id,
            step: StepKind::UserInputReceived,
            started_at: now,
            finished_at: now,
            result: Ok(StepResultEnvelope::UserInputReceived(req.clone())),
        });
        let json = serde_json::to_string(&record).unwrap();
        let back: StepRecord = serde_json::from_str(&json).unwrap();
        match back {
            StepRecord::Finished(f) => {
                assert_eq!(f.record_id, record_id);
                assert_eq!(f.step, StepKind::UserInputReceived);
                match f.result {
                    Ok(StepResultEnvelope::UserInputReceived(r)) => assert_eq!(r, req),
                    other => panic!("unexpected result: {other:?}"),
                }
            }
            _ => panic!("expected Finished after round-trip"),
        }
    }

    // ─── StepRecord::Finished with Err: serde round-trip ─────────────────────

    #[test]
    fn finished_step_record_err_round_trip_preserves_error_variant() {
        let record_id = step_record_id();
        let now = Utc::now();
        let error = StepError::InputNormalization(InputNormalizationError::EmptyQuery);
        let record = StepRecord::Finished(FinishedStepRecord {
            record_id,
            step: StepKind::InputNormalization,
            started_at: now,
            finished_at: now,
            result: Err(error),
        });
        let json = serde_json::to_string(&record).unwrap();
        let back: StepRecord = serde_json::from_str(&json).unwrap();
        match back {
            StepRecord::Finished(f) => {
                assert_eq!(f.record_id, record_id);
                assert!(matches!(
                    f.result,
                    Err(StepError::InputNormalization(
                        InputNormalizationError::EmptyQuery
                    ))
                ));
            }
            _ => panic!("expected Finished after round-trip"),
        }
    }

    // ─── RunState: full struct serde round-trip ────────────────────────────────

    #[test]
    fn run_state_round_trip() {
        let rid = run_id();
        let now = Utc::now();
        let state = RunState {
            run_id: rid,
            status: RunStatus::Active,
            created_at: now,
            updated_at: now,
            revision: 3,
            iterations: vec![RunIteration {
                iteration_id: iteration_id(),
                step_records: vec![finished_ok(
                    StepKind::UserInputReceived,
                    user_input_result(),
                )],
            }],
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: RunState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.run_id, rid);
        assert_eq!(back.status, RunStatus::Active);
        assert_eq!(back.revision, 3);
        assert_eq!(back.iterations.len(), 1);
    }
}

use crate::request_pipeline::candidate_card_retrieval::CandidateCardRetrievalError;
use crate::request_pipeline::card_hydration::CardHydrationError;
use crate::request_pipeline::incident_evidence_retrieval::IncidentEvidenceRetrievalError;
use crate::request_pipeline::input_normalization::InputNormalizationError;
use crate::request_pipeline::llm_structured_generation::LlmStructuredGenerationError;
use crate::request_pipeline::prompt_context_assembly::PromptContextAssemblyError;
use crate::request_pipeline::query_structuring::QueryStructuringError;
use crate::request_pipeline::response_validation_and_normalization::ResponseValidationAndNormalizationError;
use crate::request_pipeline::theory_evidence_retrieval::TheoryEvidenceRetrievalError;
use crate::shared_types::{
    CandidateCardRetrievalOutput, CardHydrationOutput, IncidentEvidenceRetrievalOutput,
    LlmStructuredGenerationOutput, NormalizedUserRequest, PromptContextAssemblyOutput,
    QueryStructuringOutput, ResponseValidationAndNormalizationOutput,
    TheoryEvidenceRetrievalOutput, UserRequest,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StepRecordId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunIterationId(pub Uuid);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumString, Display, AsRefStr,
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
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumString, Display, AsRefStr,
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
