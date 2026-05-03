use crate::orchestrator::run_state::model::{RunStatus, StepError, StepKind, StepResultEnvelope};
use crate::orchestrator::run_state::view::{IterationView, RunStateView};
use crate::shared_types::ResponseValidationAndNormalizationOutput;

use super::{PolicyError, PolicyTransition, TransitionPolicy};

const CANONICAL_STEPS: &[StepKind] = &[
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

#[derive(Debug, Default)]
pub struct LinearPipelineTransitionPolicy;

impl LinearPipelineTransitionPolicy {
    pub fn new() -> Self {
        Self
    }
}

impl TransitionPolicy for LinearPipelineTransitionPolicy {
    fn next_transition(
        &self,
        state: RunStateView<'_>,
    ) -> Result<PolicyTransition, PolicyError> {
        // Priority 1: archived run
        if state.status() == RunStatus::Archived {
            return Err(PolicyError::RunArchived);
        }

        // Priority 2: no current iteration
        let iteration = state
            .last_iteration()
            .ok_or(PolicyError::NoCurrentIteration)?;

        // Priority 3: pending step
        if iteration.pending_step().is_some() {
            return Err(PolicyError::PendingStepPresent);
        }

        // Priority 4: user input check
        check_user_input(iteration)?;

        // Priority 5: validate successful-step history
        validate_successful_history(iteration)?;

        // Priority 6: terminal error
        if let Some(error) = find_terminal_error(iteration) {
            return Ok(PolicyTransition::FinishWithError {
                error: error.clone(),
            });
        }

        // Priority 7: terminal result
        if let Some(result) = find_terminal_result(iteration)? {
            return Ok(PolicyTransition::FinishWithResult {
                result: result.clone(),
            });
        }

        // Priority 8: next step
        Ok(PolicyTransition::ExecuteStep {
            step: next_step(iteration),
        })
    }
}

// ─── Section 12: User Input Check ────────────────────────────────────────────

fn check_user_input(iteration: IterationView<'_>) -> Result<(), PolicyError> {
    let view = iteration
        .finished_step(StepKind::UserInputReceived)
        .ok_or(PolicyError::MissingUserInput)?;

    match view.result() {
        Err(_) => Err(PolicyError::MissingUserInput),
        Ok(StepResultEnvelope::UserInputReceived(_)) => Ok(()),
        Ok(_) => Err(PolicyError::UnexpectedStepResult {
            step: StepKind::UserInputReceived,
        }),
    }
}

// ─── Section 13: Successful-Step Validation ──────────────────────────────────

fn validate_successful_history(iteration: IterationView<'_>) -> Result<(), PolicyError> {
    let mut successful = [false; 9];

    for view in iteration.finished_steps() {
        let kind = view.kind();
        let Some(pos) = canonical_position(kind) else {
            continue; // UserInputReceived — not part of executable validation
        };
        let Ok(envelope) = view.result() else {
            continue; // failed records do not participate
        };

        if !result_variant_matches(kind, envelope) {
            return Err(PolicyError::UnexpectedStepResult { step: kind });
        }
        if successful[pos] {
            return Err(PolicyError::DuplicateSuccessfulStep { step: kind });
        }
        successful[pos] = true;
    }

    // Successful executable steps must form a prefix of the canonical order.
    let mut gap_found = false;
    for (pos, &is_successful) in successful.iter().enumerate() {
        if is_successful {
            if gap_found {
                return Err(PolicyError::StepOutOfOrder {
                    step: CANONICAL_STEPS[pos],
                });
            }
        } else {
            gap_found = true;
        }
    }

    Ok(())
}

fn canonical_position(kind: StepKind) -> Option<usize> {
    CANONICAL_STEPS.iter().position(|&s| s == kind)
}

fn result_variant_matches(kind: StepKind, envelope: &StepResultEnvelope) -> bool {
    matches!(
        (kind, envelope),
        (StepKind::InputNormalization, StepResultEnvelope::InputNormalization(_))
            | (StepKind::QueryStructuring, StepResultEnvelope::QueryStructuring(_))
            | (
                StepKind::CandidateCardRetrieval,
                StepResultEnvelope::CandidateCardRetrieval(_)
            )
            | (StepKind::CardHydration, StepResultEnvelope::CardHydration(_))
            | (
                StepKind::IncidentEvidenceRetrieval,
                StepResultEnvelope::IncidentEvidenceRetrieval(_)
            )
            | (
                StepKind::TheoryEvidenceRetrieval,
                StepResultEnvelope::TheoryEvidenceRetrieval(_)
            )
            | (
                StepKind::PromptContextAssembly,
                StepResultEnvelope::PromptContextAssembly(_)
            )
            | (
                StepKind::LlmStructuredGeneration,
                StepResultEnvelope::LlmStructuredGeneration(_)
            )
            | (
                StepKind::ResponseValidationAndNormalization,
                StepResultEnvelope::ResponseValidationAndNormalization(_)
            )
    )
}

// ─── Section 14: Terminal Error ───────────────────────────────────────────────

fn find_terminal_error<'a>(iteration: IterationView<'a>) -> Option<&'a StepError> {
    iteration.finished_steps().find_map(|view| {
        if view.kind() == StepKind::UserInputReceived {
            return None;
        }
        match view.result() {
            Err(error) => Some(error),
            Ok(_) => None,
        }
    })
}

// ─── Section 15: Terminal Result ──────────────────────────────────────────────

fn find_terminal_result<'a>(
    iteration: IterationView<'a>,
) -> Result<Option<&'a ResponseValidationAndNormalizationOutput>, PolicyError> {
    let Some(view) = iteration.finished_step(StepKind::ResponseValidationAndNormalization) else {
        return Ok(None);
    };
    match view.result() {
        Ok(StepResultEnvelope::ResponseValidationAndNormalization(result)) => Ok(Some(result)),
        Ok(_) => Err(PolicyError::UnexpectedStepResult {
            step: StepKind::ResponseValidationAndNormalization,
        }),
        Err(_) => Ok(None), // already surfaced by find_terminal_error at priority 6
    }
}

// ─── Section 16: Next Step ────────────────────────────────────────────────────

fn next_step(iteration: IterationView<'_>) -> StepKind {
    for &step in CANONICAL_STEPS {
        let has_success = iteration
            .finished_step(step)
            .map_or(false, |v| v.result().is_ok());
        if !has_success {
            return step;
        }
    }
    unreachable!("all canonical steps succeeded but FinishWithResult was not returned")
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::run_state::model::{
        FinishedStepRecord, PendingStepRecord, RunId, RunIteration, RunIterationId, RunState,
        RunStatus, StepError, StepKind, StepRecord, StepRecordId, StepResultEnvelope,
    };
    use crate::orchestrator::run_state::view::RunStateView;
    use crate::shared_types::{
        CandidateCardRetrievalOutput, CardHydrationOutput, DiagnosticResponse,
        DiagnosticResultInterpretation, IncidentEvidenceRetrievalOutput,
        LlmStructuredGenerationOutput, ModelTokenUsage, NormalizedUserRequest,
        PromptContextAssemblyOutput, QueryStructuringOutput,
        ResponseValidationAndNormalizationOutput, StructuredUserQuery,
        StructuredUserQueryConfidence, TheoryEvidenceRetrievalOutput, UserRequest,
    };
    use chrono::Utc;
    use uuid::Uuid;

    // ─── Fixtures ─────────────────────────────────────────────────────────────

    fn new_run_id() -> RunId {
        RunId(Uuid::new_v4())
    }

    fn new_iteration_id() -> RunIterationId {
        RunIterationId(Uuid::new_v4())
    }

    fn new_record_id() -> StepRecordId {
        StepRecordId(Uuid::new_v4())
    }

    fn finished_ok(kind: StepKind, result: StepResultEnvelope) -> StepRecord {
        let now = Utc::now();
        StepRecord::Finished(FinishedStepRecord {
            record_id: new_record_id(),
            step: kind,
            started_at: now,
            finished_at: now,
            result: Ok(result),
        })
    }

    fn finished_err(kind: StepKind, error: StepError) -> StepRecord {
        let now = Utc::now();
        StepRecord::Finished(FinishedStepRecord {
            record_id: new_record_id(),
            step: kind,
            started_at: now,
            finished_at: now,
            result: Err(error),
        })
    }

    fn pending(kind: StepKind) -> StepRecord {
        StepRecord::Pending(PendingStepRecord {
            record_id: new_record_id(),
            step: kind,
            started_at: Utc::now(),
        })
    }

    fn active_run(records: Vec<StepRecord>) -> RunState {
        let now = Utc::now();
        RunState {
            run_id: new_run_id(),
            status: RunStatus::Active,
            created_at: now,
            updated_at: now,
            revision: 0,
            iterations: vec![RunIteration {
                iteration_id: new_iteration_id(),
                config_snapshot: None,
                step_records: records,
            }],
        }
    }

    fn empty_run() -> RunState {
        let now = Utc::now();
        RunState {
            run_id: new_run_id(),
            status: RunStatus::Active,
            created_at: now,
            updated_at: now,
            revision: 0,
            iterations: vec![],
        }
    }

    fn archived_run_with_iteration() -> RunState {
        let now = Utc::now();
        RunState {
            run_id: new_run_id(),
            status: RunStatus::Archived,
            created_at: now,
            updated_at: now,
            revision: 0,
            iterations: vec![RunIteration {
                iteration_id: new_iteration_id(),
                config_snapshot: None,
                step_records: vec![user_input_ok()],
            }],
        }
    }

    fn archived_run_empty() -> RunState {
        let now = Utc::now();
        RunState {
            run_id: new_run_id(),
            status: RunStatus::Archived,
            created_at: now,
            updated_at: now,
            revision: 0,
            iterations: vec![],
        }
    }

    // ─── Canonical step envelopes ─────────────────────────────────────────────

    fn user_input_ok() -> StepRecord {
        finished_ok(
            StepKind::UserInputReceived,
            StepResultEnvelope::UserInputReceived(UserRequest {
                query: "service down".to_string(),
                golden_question: None,
            }),
        )
    }

    fn input_normalization_ok() -> StepRecord {
        finished_ok(
            StepKind::InputNormalization,
            StepResultEnvelope::InputNormalization(NormalizedUserRequest {
                query: "service down".to_string(),
                input_token_count: 2,
            }),
        )
    }

    fn query_structuring_ok() -> StepRecord {
        finished_ok(
            StepKind::QueryStructuring,
            StepResultEnvelope::QueryStructuring(QueryStructuringOutput {
                structured_query: StructuredUserQuery {
                    intent: "diagnose".to_string(),
                    scenario: "down".to_string(),
                    symptoms: vec![],
                    affected_subsystems: vec![],
                    failure_modes: vec![],
                    system_properties: vec![],
                    entities: vec![],
                    constraints: vec![],
                    triggers: vec![],
                    observability_signals: vec![],
                    unresolved_terms: vec![],
                    rejected_nearby_terms: vec![],
                    confidence: StructuredUserQueryConfidence::Medium,
                },
                token_usage: ModelTokenUsage {
                    prompt_tokens: None,
                    completion_tokens: None,
                    total_tokens: None,
                },
                metrics: Some(crate::shared_types::QueryStructuringMetrics::default()),
            }),
        )
    }

    fn candidate_card_retrieval_ok() -> StepRecord {
        finished_ok(
            StepKind::CandidateCardRetrieval,
            StepResultEnvelope::CandidateCardRetrieval(CandidateCardRetrievalOutput {
                primary: None,
                alternatives: vec![],
            metrics: None,
            }),
        )
    }

    fn card_hydration_ok() -> StepRecord {
        finished_ok(
            StepKind::CardHydration,
            StepResultEnvelope::CardHydration(CardHydrationOutput {
                primary: None,
                alternatives: vec![],
            }),
        )
    }

    fn incident_evidence_retrieval_ok() -> StepRecord {
        finished_ok(
            StepKind::IncidentEvidenceRetrieval,
            StepResultEnvelope::IncidentEvidenceRetrieval(IncidentEvidenceRetrievalOutput {
                primary_chunks: vec![],
                alternative_chunks: vec![],
            metrics: None,
            }),
        )
    }

    fn theory_evidence_retrieval_ok() -> StepRecord {
        finished_ok(
            StepKind::TheoryEvidenceRetrieval,
            StepResultEnvelope::TheoryEvidenceRetrieval(TheoryEvidenceRetrievalOutput {
                chunks: vec![],
            metrics: None,
            }),
        )
    }

    fn prompt_context_assembly_ok() -> StepRecord {
        finished_ok(
            StepKind::PromptContextAssembly,
            StepResultEnvelope::PromptContextAssembly(PromptContextAssemblyOutput {
                prompt: "prompt".to_string(),
                response_schema: serde_json::Value::Object(serde_json::Map::new()),
                evidence_topology: Default::default(),
                incident_evidence_chunks: vec![],
                theory_chunks: vec![],
            }),
        )
    }

    fn llm_structured_generation_ok() -> StepRecord {
        finished_ok(
            StepKind::LlmStructuredGeneration,
            StepResultEnvelope::LlmStructuredGeneration(LlmStructuredGenerationOutput {
                response_json: serde_json::json!({}),
                token_usage: ModelTokenUsage {
                    prompt_tokens: None,
                    completion_tokens: None,
                    total_tokens: None,
                },
            }),
        )
    }

    fn minimal_final_result() -> ResponseValidationAndNormalizationOutput {
        ResponseValidationAndNormalizationOutput {
            response: DiagnosticResponse {
                problem_understanding: "service down".to_string(),
                similar_practical_context: "similar incident".to_string(),
                active_hypotheses: vec![
                    crate::shared_types::ActiveHypothesis {
                        hypothesis: "overload".to_string(),
                        source: crate::shared_types::HypothesisSource::PrimaryIncident,
                        confidence: crate::shared_types::HypothesisConfidence::Medium,
                    },
                    crate::shared_types::ActiveHypothesis {
                        hypothesis: "network fault".to_string(),
                        source: crate::shared_types::HypothesisSource::AlternativeContext,
                        confidence: crate::shared_types::HypothesisConfidence::Low,
                    },
                ],
                first_check: "check logs".to_string(),
                result_interpretation: DiagnosticResultInterpretation {
                    supports_primary_if: "logs show errors".to_string(),
                    supports_competing_if: "no errors".to_string(),
                    inconclusive_if: None,
                },
                alternative_context_assessment: crate::shared_types::AlternativeContextAssessment {
                    used_as_hypothesis: true,
                    reason: "alternative case shows a different failure mechanism".to_string(),
                },
            },
        }
    }

    fn response_validation_ok() -> StepRecord {
        finished_ok(
            StepKind::ResponseValidationAndNormalization,
            StepResultEnvelope::ResponseValidationAndNormalization(minimal_final_result()),
        )
    }

    fn all_canonical_steps_ok() -> Vec<StepRecord> {
        vec![
            user_input_ok(),
            input_normalization_ok(),
            query_structuring_ok(),
            candidate_card_retrieval_ok(),
            card_hydration_ok(),
            incident_evidence_retrieval_ok(),
            theory_evidence_retrieval_ok(),
            prompt_context_assembly_ok(),
            llm_structured_generation_ok(),
            response_validation_ok(),
        ]
    }

    fn policy() -> LinearPipelineTransitionPolicy {
        LinearPipelineTransitionPolicy::new()
    }

    fn step_ok(kind: StepKind) -> StepRecord {
        match kind {
            StepKind::UserInputReceived => user_input_ok(),
            StepKind::InputNormalization => input_normalization_ok(),
            StepKind::QueryStructuring => query_structuring_ok(),
            StepKind::CandidateCardRetrieval => candidate_card_retrieval_ok(),
            StepKind::CardHydration => card_hydration_ok(),
            StepKind::IncidentEvidenceRetrieval => incident_evidence_retrieval_ok(),
            StepKind::TheoryEvidenceRetrieval => theory_evidence_retrieval_ok(),
            StepKind::PromptContextAssembly => prompt_context_assembly_ok(),
            StepKind::LlmStructuredGeneration => llm_structured_generation_ok(),
            StepKind::ResponseValidationAndNormalization => response_validation_ok(),
        }
    }

    // ─── Constructor ──────────────────────────────────────────────────────────

    #[test]
    fn new_constructs_stateless_policy() {
        let p = LinearPipelineTransitionPolicy::new();
        let _ = format!("{p:?}");
    }

    #[test]
    fn default_constructs_stateless_policy() {
        let p = LinearPipelineTransitionPolicy::default();
        let _ = format!("{p:?}");
    }

    // ─── Section 10: RunArchived ──────────────────────────────────────────────

    #[test]
    fn returns_run_archived_for_archived_run_with_iteration() {
        let state = archived_run_with_iteration();
        let view = RunStateView::new(&state);
        let err = policy().next_transition(view).unwrap_err();
        assert_eq!(err, PolicyError::RunArchived);
    }

    #[test]
    fn run_archived_fires_before_no_current_iteration() {
        let state = archived_run_empty();
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap_err(),
            PolicyError::RunArchived
        );
    }

    // ─── Section 10: NoCurrentIteration ──────────────────────────────────────

    #[test]
    fn returns_no_current_iteration_for_run_with_no_iterations() {
        let state = empty_run();
        let view = RunStateView::new(&state);
        let err = policy().next_transition(view).unwrap_err();
        assert_eq!(err, PolicyError::NoCurrentIteration);
    }

    // ─── Section 11: PendingStepPresent ──────────────────────────────────────

    #[test]
    fn returns_pending_step_present_when_iteration_has_pending_step() {
        let state = active_run(vec![user_input_ok(), pending(StepKind::InputNormalization)]);
        let view = RunStateView::new(&state);
        let err = policy().next_transition(view).unwrap_err();
        assert_eq!(err, PolicyError::PendingStepPresent);
    }

    #[test]
    fn pending_step_fires_before_missing_user_input() {
        // No UserInputReceived AND a pending step → PendingStepPresent wins.
        let state = active_run(vec![pending(StepKind::InputNormalization)]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap_err(),
            PolicyError::PendingStepPresent
        );
    }

    // ─── Section 12: User Input Check ────────────────────────────────────────

    #[test]
    fn returns_missing_user_input_when_user_input_record_absent() {
        let state = active_run(vec![]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap_err(),
            PolicyError::MissingUserInput
        );
    }

    #[test]
    fn returns_missing_user_input_when_user_input_record_is_err() {
        let state = active_run(vec![finished_err(
            StepKind::UserInputReceived,
            StepError::MissingRequiredInput {
                message: "no input".to_string(),
            },
        )]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap_err(),
            PolicyError::MissingUserInput
        );
    }

    #[test]
    fn returns_unexpected_step_result_when_user_input_has_wrong_ok_variant() {
        let state = active_run(vec![finished_ok(
            StepKind::UserInputReceived,
            StepResultEnvelope::InputNormalization(NormalizedUserRequest {
                query: "q".to_string(),
                input_token_count: 1,
            }),
        )]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap_err(),
            PolicyError::UnexpectedStepResult {
                step: StepKind::UserInputReceived
            }
        );
    }

    #[test]
    fn missing_user_input_fires_before_duplicate_step_validation() {
        // No UserInputReceived + duplicate InputNormalization → MissingUserInput wins.
        let state = active_run(vec![input_normalization_ok(), input_normalization_ok()]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap_err(),
            PolicyError::MissingUserInput
        );
    }

    // ─── Section 13: Successful-Step Validation ───────────────────────────────

    #[test]
    fn returns_duplicate_successful_step_when_same_step_recorded_twice() {
        let state = active_run(vec![
            user_input_ok(),
            input_normalization_ok(),
            input_normalization_ok(),
        ]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap_err(),
            PolicyError::DuplicateSuccessfulStep {
                step: StepKind::InputNormalization
            }
        );
    }

    #[test]
    fn returns_step_out_of_order_when_step_succeeds_without_predecessor() {
        // QueryStructuring (pos 1) present but InputNormalization (pos 0) absent.
        let state = active_run(vec![user_input_ok(), query_structuring_ok()]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap_err(),
            PolicyError::StepOutOfOrder {
                step: StepKind::QueryStructuring
            }
        );
    }

    #[test]
    fn returns_step_out_of_order_for_first_violating_canonical_position() {
        // InputNormalization (pos 0) OK, QueryStructuring (pos 1) absent,
        // CandidateCardRetrieval (pos 2) present → pos 2 is first violation.
        let state = active_run(vec![
            user_input_ok(),
            input_normalization_ok(),
            candidate_card_retrieval_ok(),
        ]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap_err(),
            PolicyError::StepOutOfOrder {
                step: StepKind::CandidateCardRetrieval
            }
        );
    }

    #[test]
    fn returns_unexpected_step_result_when_successful_step_has_wrong_envelope_variant() {
        // InputNormalization step record stores a QueryStructuring envelope.
        let state = active_run(vec![
            user_input_ok(),
            finished_ok(
                StepKind::InputNormalization,
                StepResultEnvelope::QueryStructuring(QueryStructuringOutput {
                    structured_query: StructuredUserQuery {
                        intent: "x".to_string(),
                        scenario: "y".to_string(),
                        symptoms: vec![],
                        affected_subsystems: vec![],
                        failure_modes: vec![],
                        system_properties: vec![],
                        entities: vec![],
                        constraints: vec![],
                        triggers: vec![],
                        observability_signals: vec![],
                        unresolved_terms: vec![],
                        rejected_nearby_terms: vec![],
                        confidence: StructuredUserQueryConfidence::Low,
                    },
                    token_usage: ModelTokenUsage {
                        prompt_tokens: None,
                        completion_tokens: None,
                        total_tokens: None,
                    },
                    metrics: Some(crate::shared_types::QueryStructuringMetrics::default()),
                }),
            ),
        ]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap_err(),
            PolicyError::UnexpectedStepResult {
                step: StepKind::InputNormalization
            }
        );
    }

    #[test]
    fn validation_fires_before_terminal_error() {
        // QueryStructuring out of order AND a step error → StepOutOfOrder wins.
        let state = active_run(vec![
            user_input_ok(),
            query_structuring_ok(), // InputNormalization absent → out of order
            finished_err(
                StepKind::CardHydration,
                StepError::MissingRequiredInput {
                    message: "no card".to_string(),
                },
            ),
        ]);
        let view = RunStateView::new(&state);
        assert!(matches!(
            policy().next_transition(view).unwrap_err(),
            PolicyError::StepOutOfOrder { .. }
        ));
    }

    // ─── Section 14: FinishWithError ─────────────────────────────────────────

    #[test]
    fn returns_finish_with_error_when_executable_step_has_err_result() {
        let err_payload = StepError::MissingRequiredInput {
            message: "retrieval failed".to_string(),
        };
        let state = active_run(vec![
            user_input_ok(),
            input_normalization_ok(),
            query_structuring_ok(),
            finished_err(StepKind::CandidateCardRetrieval, err_payload.clone()),
        ]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::FinishWithError { error: err_payload }
        );
    }

    #[test]
    fn user_input_received_err_does_not_produce_finish_with_error() {
        // UserInputReceived Err is caught by section 12 as MissingUserInput.
        let state = active_run(vec![finished_err(
            StepKind::UserInputReceived,
            StepError::MissingRequiredInput {
                message: "no input".to_string(),
            },
        )]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap_err(),
            PolicyError::MissingUserInput
        );
    }

    #[test]
    fn finish_with_error_fires_before_finish_with_result() {
        // All 9 canonical steps succeed in order (validation passes, FinishWithResult
        // would fire), but CardHydration also has an additional failure record.
        // Failed records don't affect the prefix validation — only successful records
        // do. The error scan at priority 6 finds the failure and wins over
        // FinishWithResult at priority 7.
        let err_payload = StepError::MissingRequiredInput {
            message: "hydration failed".to_string(),
        };
        let mut records = all_canonical_steps_ok();
        records.push(finished_err(StepKind::CardHydration, err_payload.clone()));
        let state = active_run(records);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::FinishWithError { error: err_payload }
        );
    }

    // ─── Section 15: FinishWithResult ────────────────────────────────────────

    #[test]
    fn returns_finish_with_result_when_all_canonical_steps_complete() {
        let state = active_run(all_canonical_steps_ok());
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::FinishWithResult {
                result: minimal_final_result()
            }
        );
    }

    #[test]
    fn returns_unexpected_step_result_when_final_step_has_wrong_ok_variant() {
        let mut records = all_canonical_steps_ok();
        records[9] = finished_ok(
            StepKind::ResponseValidationAndNormalization,
            StepResultEnvelope::LlmStructuredGeneration(LlmStructuredGenerationOutput {
                response_json: serde_json::json!({}),
                token_usage: ModelTokenUsage {
                    prompt_tokens: None,
                    completion_tokens: None,
                    total_tokens: None,
                },
            }),
        );
        let state = active_run(records);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap_err(),
            PolicyError::UnexpectedStepResult {
                step: StepKind::ResponseValidationAndNormalization
            }
        );
    }

    // ─── Section 16: ExecuteStep ──────────────────────────────────────────────

    #[test]
    fn returns_execute_step_with_first_canonical_step_when_no_steps_done() {
        let state = active_run(vec![user_input_ok()]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::ExecuteStep {
                step: StepKind::InputNormalization
            }
        );
    }

    #[test]
    fn returns_execute_step_for_each_canonical_position_in_order() {
        // For each prefix of N completed canonical steps, the policy must return
        // the (N+1)-th canonical step.
        for n in 0..CANONICAL_STEPS.len() - 1 {
            let mut records = vec![user_input_ok()];
            for &kind in &CANONICAL_STEPS[..n] {
                records.push(step_ok(kind));
            }
            let state = active_run(records);
            let view = RunStateView::new(&state);
            let expected = CANONICAL_STEPS[n];
            assert_eq!(
                policy().next_transition(view).unwrap(),
                PolicyTransition::ExecuteStep { step: expected },
                "after {n} completed steps, expected ExecuteStep {{ step: {expected:?} }}"
            );
        }
    }
}
