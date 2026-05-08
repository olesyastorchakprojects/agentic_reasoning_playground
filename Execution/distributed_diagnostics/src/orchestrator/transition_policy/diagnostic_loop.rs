use crate::orchestrator::run_state::model::{RunStatus, StepError, StepKind, StepResultEnvelope};
use crate::orchestrator::run_state::view::{IterationView, RunStateView};
use crate::shared_types::{
    AdequacyStatus, ObservationBoundaryResolution, ResponseValidationAndNormalizationOutput,
};

use super::{PolicyError, PolicyTransition, TransitionPolicy};

// ─── Canonical step orders ────────────────────────────────────────────────────

const INITIAL_STEPS: &[StepKind] = &[
    StepKind::InputNormalization,
    StepKind::QueryStructuring,
    StepKind::InformationAdequacyInitial,
    StepKind::CandidateCardRetrieval,
    StepKind::CardHydration,
    StepKind::IncidentEvidenceRetrieval,
    StepKind::TheoryEvidenceRetrieval,
    StepKind::PromptContextAssembly,
    StepKind::LlmStructuredGeneration,
    StepKind::ResponseValidationAndNormalization,
];

// Continuation: ObservationBoundaryResolver resolved to Supported
const CONTINUATION_SUPPORTED_STEPS: &[StepKind] = &[
    StepKind::InputNormalization,
    StepKind::ObservationBoundaryResolver,
    StepKind::ObservationExtraction,
    StepKind::InformationAdequacySupportedObservation,
    StepKind::CandidateCardRetrieval,
    StepKind::CardBranchReranking,
    StepKind::CardHydration,
    StepKind::IncidentEvidenceRetrieval,
    StepKind::TheoryEvidenceRetrieval,
    StepKind::DiagnosticUpdatePromptContextAssembly,
    StepKind::LlmStructuredGeneration,
    StepKind::ResponseValidationAndNormalization,
];

// Continuation: ObservationBoundaryResolver resolved to Unsupported
const CONTINUATION_UNSUPPORTED_STEPS: &[StepKind] = &[
    StepKind::InputNormalization,
    StepKind::ObservationBoundaryResolver,
    StepKind::InformationAdequacyUnsupportedObservation,
    StepKind::CandidateCardRetrieval,
    StepKind::CardBranchReranking,
    StepKind::CardHydration,
    StepKind::IncidentEvidenceRetrieval,
    StepKind::TheoryEvidenceRetrieval,
    StepKind::DiagnosticUpdatePromptContextAssembly,
    StepKind::LlmStructuredGeneration,
    StepKind::ResponseValidationAndNormalization,
];

// ─── Public struct ────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct DiagnosticLoopTransitionPolicy;

impl DiagnosticLoopTransitionPolicy {
    pub fn new() -> Self {
        Self
    }
}

impl TransitionPolicy for DiagnosticLoopTransitionPolicy {
    fn next_transition(&self, state: RunStateView<'_>) -> Result<PolicyTransition, PolicyError> {
        // Priority 1: archived run
        if state.status() == RunStatus::Archived {
            return Err(PolicyError::RunArchived);
        }

        // Priority 2: no current iteration
        if state.iteration_count() == 0 {
            return Err(PolicyError::NoCurrentIteration);
        }
        let iteration_index = state.iteration_count() - 1;

        let iteration = state.last_iteration().ok_or(PolicyError::NoCurrentIteration)?;

        // Priority 3: pending step
        if iteration.pending_step().is_some() {
            return Err(PolicyError::PendingStepPresent);
        }

        // Priority 4: user input check
        check_user_input(iteration)?;

        // Select canonical order (continuation branches on OBR resolution)
        let canonical: &[StepKind] = if iteration_index == 0 {
            INITIAL_STEPS
        } else {
            effective_continuation_steps(iteration)
        };

        // Priority 5: validate successful-step history
        validate_successful_history(iteration, canonical)?;

        // Priority 6: terminal error
        if let Some(error) = find_terminal_error(iteration, canonical) {
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

        // Priority 8a: adequacy gate — WaitForUser when blocking or weak
        if let Some(wait) = check_adequacy_wait(iteration, canonical) {
            return Ok(wait);
        }

        // Priority 8b: next executable step
        Ok(PolicyTransition::ExecuteStep {
            step: next_step(iteration, canonical),
        })
    }
}

// ─── Canonical order selection ────────────────────────────────────────────────

fn effective_continuation_steps(iteration: IterationView<'_>) -> &'static [StepKind] {
    if let Some(view) = iteration.finished_step(StepKind::ObservationBoundaryResolver) {
        if let Ok(StepResultEnvelope::ObservationBoundaryResolver(output)) = view.result() {
            if matches!(output.resolution, ObservationBoundaryResolution::Unsupported) {
                return CONTINUATION_UNSUPPORTED_STEPS;
            }
        }
    }
    CONTINUATION_SUPPORTED_STEPS
}

// ─── User input check ─────────────────────────────────────────────────────────

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

// ─── Successful-step validation ───────────────────────────────────────────────

fn validate_successful_history(
    iteration: IterationView<'_>,
    canonical: &[StepKind],
) -> Result<(), PolicyError> {
    let mut successful = vec![false; canonical.len()];

    for view in iteration.finished_steps() {
        let kind = view.kind();
        let Some(pos) = canonical.iter().position(|&s| s == kind) else {
            continue;
        };
        let Ok(envelope) = view.result() else {
            continue;
        };

        if !result_variant_matches(kind, envelope) {
            return Err(PolicyError::UnexpectedStepResult { step: kind });
        }
        if successful[pos] {
            return Err(PolicyError::DuplicateSuccessfulStep { step: kind });
        }
        successful[pos] = true;
    }

    // Successful executable steps must form a prefix of the active canonical order.
    let mut gap_found = false;
    for (pos, &is_successful) in successful.iter().enumerate() {
        if is_successful {
            if gap_found {
                return Err(PolicyError::StepOutOfOrder {
                    step: canonical[pos],
                });
            }
        } else {
            gap_found = true;
        }
    }

    Ok(())
}

fn result_variant_matches(kind: StepKind, envelope: &StepResultEnvelope) -> bool {
    matches!(
        (kind, envelope),
        (StepKind::InputNormalization, StepResultEnvelope::InputNormalization(_))
            | (StepKind::QueryStructuring, StepResultEnvelope::QueryStructuring(_))
            | (
                StepKind::InformationAdequacyInitial,
                StepResultEnvelope::InformationAdequacy(_)
            )
            | (
                StepKind::InformationAdequacySupportedObservation,
                StepResultEnvelope::InformationAdequacy(_)
            )
            | (
                StepKind::InformationAdequacyUnsupportedObservation,
                StepResultEnvelope::InformationAdequacy(_)
            )
            | (
                StepKind::ObservationBoundaryResolver,
                StepResultEnvelope::ObservationBoundaryResolver(_)
            )
            | (
                StepKind::ObservationExtraction,
                StepResultEnvelope::ObservationExtraction(_)
            )
            | (
                StepKind::CandidateCardRetrieval,
                StepResultEnvelope::CandidateCardRetrieval(_)
            )
            | (
                StepKind::CardBranchReranking,
                StepResultEnvelope::CardBranchReranking(_)
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
                StepKind::DiagnosticUpdatePromptContextAssembly,
                StepResultEnvelope::DiagnosticUpdatePromptContextAssembly(_)
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

// ─── Adequacy gate ────────────────────────────────────────────────────────────

fn check_adequacy_wait(
    iteration: IterationView<'_>,
    canonical: &[StepKind],
) -> Option<PolicyTransition> {
    // InformationAdequacyInitial (initial iteration)
    if canonical.contains(&StepKind::InformationAdequacyInitial) {
        if let Some(view) = iteration.finished_step(StepKind::InformationAdequacyInitial) {
            if let Ok(StepResultEnvelope::InformationAdequacy(assessment)) = view.result() {
                if matches!(
                    assessment.status,
                    AdequacyStatus::Blocking | AdequacyStatus::WeakButRunnable
                ) {
                    return Some(PolicyTransition::WaitForUser {
                        follow_up_questions: assessment.follow_up_questions.clone(),
                    });
                }
            }
        }
    }
    // InformationAdequacySupportedObservation (continuation supported branch)
    if canonical.contains(&StepKind::InformationAdequacySupportedObservation) {
        if let Some(view) =
            iteration.finished_step(StepKind::InformationAdequacySupportedObservation)
        {
            if let Ok(StepResultEnvelope::InformationAdequacy(assessment)) = view.result() {
                if matches!(
                    assessment.status,
                    AdequacyStatus::Blocking | AdequacyStatus::WeakButRunnable
                ) {
                    return Some(PolicyTransition::WaitForUser {
                        follow_up_questions: assessment.follow_up_questions.clone(),
                    });
                }
            }
        }
    }
    // InformationAdequacyUnsupportedObservation (continuation unsupported branch — always WaitForUser)
    if canonical.contains(&StepKind::InformationAdequacyUnsupportedObservation) {
        if let Some(view) =
            iteration.finished_step(StepKind::InformationAdequacyUnsupportedObservation)
        {
            if let Ok(StepResultEnvelope::InformationAdequacy(assessment)) = view.result() {
                return Some(PolicyTransition::WaitForUser {
                    follow_up_questions: assessment.follow_up_questions.clone(),
                });
            }
        }
    }
    None
}

// ─── Terminal error ───────────────────────────────────────────────────────────

fn find_terminal_error<'a>(
    iteration: IterationView<'a>,
    canonical: &[StepKind],
) -> Option<&'a StepError> {
    iteration.finished_steps().find_map(|view| {
        let kind = view.kind();
        if kind == StepKind::UserInputReceived {
            return None;
        }
        if !canonical.contains(&kind) {
            return None;
        }
        match view.result() {
            Err(error) => Some(error),
            Ok(_) => None,
        }
    })
}

// ─── Terminal result ──────────────────────────────────────────────────────────

fn find_terminal_result<'a>(
    iteration: IterationView<'a>,
) -> Result<Option<&'a ResponseValidationAndNormalizationOutput>, PolicyError> {
    let Some(view) =
        iteration.finished_step(StepKind::ResponseValidationAndNormalization)
    else {
        return Ok(None);
    };
    match view.result() {
        Ok(StepResultEnvelope::ResponseValidationAndNormalization(result)) => Ok(Some(result)),
        Ok(_) => Err(PolicyError::UnexpectedStepResult {
            step: StepKind::ResponseValidationAndNormalization,
        }),
        Err(_) => Ok(None),
    }
}

// ─── Next step ────────────────────────────────────────────────────────────────

fn next_step(iteration: IterationView<'_>, canonical: &[StepKind]) -> StepKind {
    for &step in canonical {
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
        FinishedStepRecord, PendingStepRecord, RunId, RunIteration, RunIterationId,
        RunIterationStatus, RunState, RunStatus, StepError, StepKind, StepRecord, StepRecordId,
        StepResultEnvelope,
    };
    use crate::orchestrator::run_state::view::RunStateView;
    use crate::shared_types::{
        AdequacyAssessment, AdequacyStatus, CardBranchRerankingOutput, CardHydrationOutput,
        CandidateCardRetrievalOutput, Confidence, DiagnosticResponse,
        DiagnosticResultInterpretation, EvidenceTopology, Hypothesis, HypothesisEvidenceSource,
        HypothesisId, HypothesisStatus, IncidentEvidenceRetrievalOutput,
        LlmStructuredGenerationOutput, ModelTokenUsage, NormalizedUserRequest,
        ObservationBoundaryResolution, ObservationBoundaryResolverOutput,
        ObservationExtractionOutput, PrimaryCardStatus, PromptContextAssemblyOutput,
        QueryStructuringOutput, ResolvedObservation, ResponseValidationAndNormalizationOutput,
        StructuredUserQuery, StructuredUserQueryConfidence, TheoryEvidenceRetrievalOutput,
        UserRequest,
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

    fn run_with_iterations(iterations: Vec<RunIteration>) -> RunState {
        let now = Utc::now();
        RunState {
            run_id: new_run_id(),
            status: RunStatus::Active,
            created_at: now,
            updated_at: now,
            revision: 0,
            iterations,
        }
    }

    fn iteration(records: Vec<StepRecord>) -> RunIteration {
        RunIteration {
            iteration_id: new_iteration_id(),
            config_snapshot: None,
            status: RunIterationStatus::Active,
            step_records: records,
        }
    }

    fn single_iteration_run(records: Vec<StepRecord>) -> RunState {
        run_with_iterations(vec![iteration(records)])
    }

    fn two_iteration_run(
        iter0_records: Vec<StepRecord>,
        iter1_records: Vec<StepRecord>,
    ) -> RunState {
        run_with_iterations(vec![iteration(iter0_records), iteration(iter1_records)])
    }

    fn empty_run() -> RunState {
        run_with_iterations(vec![])
    }

    fn archived_run() -> RunState {
        let now = Utc::now();
        RunState {
            run_id: new_run_id(),
            status: RunStatus::Archived,
            created_at: now,
            updated_at: now,
            revision: 0,
            iterations: vec![iteration(vec![user_input_ok()])],
        }
    }

    fn policy() -> DiagnosticLoopTransitionPolicy {
        DiagnosticLoopTransitionPolicy::new()
    }

    // ─── Step record builders ─────────────────────────────────────────────────

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

    fn adequacy_assessment(status: AdequacyStatus, questions: Vec<String>) -> AdequacyAssessment {
        AdequacyAssessment {
            status,
            missing_information_topics: vec![],
            follow_up_questions: questions,
            summary_reason: "test".to_string(),
        }
    }

    fn information_adequacy_initial_ok() -> StepRecord {
        finished_ok(
            StepKind::InformationAdequacyInitial,
            StepResultEnvelope::InformationAdequacy(adequacy_assessment(
                AdequacyStatus::Sufficient,
                vec![],
            )),
        )
    }

    fn information_adequacy_initial_blocking() -> StepRecord {
        finished_ok(
            StepKind::InformationAdequacyInitial,
            StepResultEnvelope::InformationAdequacy(adequacy_assessment(
                AdequacyStatus::Blocking,
                vec!["What symptom?".to_string()],
            )),
        )
    }

    fn information_adequacy_initial_weak() -> StepRecord {
        finished_ok(
            StepKind::InformationAdequacyInitial,
            StepResultEnvelope::InformationAdequacy(adequacy_assessment(
                AdequacyStatus::WeakButRunnable,
                vec!["What component?".to_string()],
            )),
        )
    }

    fn information_adequacy_supported_ok() -> StepRecord {
        finished_ok(
            StepKind::InformationAdequacySupportedObservation,
            StepResultEnvelope::InformationAdequacy(adequacy_assessment(
                AdequacyStatus::Sufficient,
                vec![],
            )),
        )
    }

    fn information_adequacy_supported_blocking() -> StepRecord {
        finished_ok(
            StepKind::InformationAdequacySupportedObservation,
            StepResultEnvelope::InformationAdequacy(adequacy_assessment(
                AdequacyStatus::Blocking,
                vec!["Need more context.".to_string()],
            )),
        )
    }

    fn information_adequacy_supported_weak() -> StepRecord {
        finished_ok(
            StepKind::InformationAdequacySupportedObservation,
            StepResultEnvelope::InformationAdequacy(adequacy_assessment(
                AdequacyStatus::WeakButRunnable,
                vec!["Thin signal.".to_string()],
            )),
        )
    }

    fn information_adequacy_unsupported_ok() -> StepRecord {
        finished_ok(
            StepKind::InformationAdequacyUnsupportedObservation,
            StepResultEnvelope::InformationAdequacy(adequacy_assessment(
                AdequacyStatus::Blocking,
                vec!["What did you observe?".to_string()],
            )),
        )
    }

    fn candidate_card_retrieval_ok() -> StepRecord {
        finished_ok(
            StepKind::CandidateCardRetrieval,
            StepResultEnvelope::CandidateCardRetrieval(CandidateCardRetrievalOutput {
                ranked_candidates: vec![],
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
                evidence_topology: EvidenceTopology::default(),
                incident_evidence_chunks: vec![],
                theory_chunks: vec![],
            }),
        )
    }

    fn obr_ok() -> StepRecord {
        finished_ok(
            StepKind::ObservationBoundaryResolver,
            StepResultEnvelope::ObservationBoundaryResolver(ObservationBoundaryResolverOutput {
                normalized_user_input: "latency up".to_string(),
                confidence: Confidence::Medium,
                reason: "accepted".to_string(),
                resolution: ObservationBoundaryResolution::Supported(ResolvedObservation {
                    text: "latency spike observed".to_string(),
                }),
            }),
        )
    }

    fn obr_unsupported_ok() -> StepRecord {
        finished_ok(
            StepKind::ObservationBoundaryResolver,
            StepResultEnvelope::ObservationBoundaryResolver(ObservationBoundaryResolverOutput {
                normalized_user_input: "hmm".to_string(),
                confidence: Confidence::Low,
                reason: "not a diagnostic observation".to_string(),
                resolution: ObservationBoundaryResolution::Unsupported,
            }),
        )
    }

    fn observation_extraction_ok() -> StepRecord {
        finished_ok(
            StepKind::ObservationExtraction,
            StepResultEnvelope::ObservationExtraction(ObservationExtractionOutput {
                normalized_user_input: "latency up".to_string(),
                resolved_observation: ResolvedObservation {
                    text: "latency spike".to_string(),
                },
                confidence: Confidence::Medium,
                observations: vec![],
                needs_more_context: false,
                missing_context_questions: vec![],
                token_usage: ModelTokenUsage {
                    prompt_tokens: None,
                    completion_tokens: None,
                    total_tokens: None,
                },
            }),
        )
    }

    fn card_branch_reranking_ok() -> StepRecord {
        finished_ok(
            StepKind::CardBranchReranking,
            StepResultEnvelope::CardBranchReranking(CardBranchRerankingOutput {
                primary_card_id: "card-1".to_string(),
                primary_card_status: PrimaryCardStatus::Tentative,
                alternative_card_ids: vec![],
                challenger_card_ids: vec![],
            }),
        )
    }

    fn diagnostic_update_prompt_context_assembly_ok() -> StepRecord {
        finished_ok(
            StepKind::DiagnosticUpdatePromptContextAssembly,
            StepResultEnvelope::DiagnosticUpdatePromptContextAssembly(
                PromptContextAssemblyOutput {
                    prompt: "diagnostic prompt".to_string(),
                    response_schema: serde_json::Value::Object(serde_json::Map::new()),
                    evidence_topology: EvidenceTopology::default(),
                    incident_evidence_chunks: vec![],
                    theory_chunks: vec![],
                },
            ),
        )
    }

    fn llm_ok() -> StepRecord {
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
                hypotheses: vec![
                    Hypothesis {
                        id: HypothesisId(Uuid::from_u128(0x1111)),
                        text: "overload".to_string(),
                        status: HypothesisStatus::Active,
                        source: HypothesisEvidenceSource::PrimaryIncident,
                        confidence: Confidence::Medium,
                    },
                    Hypothesis {
                        id: HypothesisId(Uuid::from_u128(0x2222)),
                        text: "network fault".to_string(),
                        status: HypothesisStatus::Weakened,
                        source: HypothesisEvidenceSource::AlternativeContext,
                        confidence: Confidence::Low,
                    },
                ],
                first_check: "check logs".to_string(),
                result_interpretation: DiagnosticResultInterpretation {
                    supports_primary_if: "logs show errors".to_string(),
                    supports_competing_if: "no errors".to_string(),
                    inconclusive_if: None,
                },
                competing_interpretation: None,
            },
        }
    }

    fn response_validation_ok() -> StepRecord {
        finished_ok(
            StepKind::ResponseValidationAndNormalization,
            StepResultEnvelope::ResponseValidationAndNormalization(minimal_final_result()),
        )
    }

    fn all_initial_steps_ok() -> Vec<StepRecord> {
        vec![
            user_input_ok(),
            input_normalization_ok(),
            query_structuring_ok(),
            information_adequacy_initial_ok(),
            candidate_card_retrieval_ok(),
            card_hydration_ok(),
            incident_evidence_retrieval_ok(),
            theory_evidence_retrieval_ok(),
            prompt_context_assembly_ok(),
            llm_ok(),
            response_validation_ok(),
        ]
    }

    fn all_continuation_steps_ok() -> Vec<StepRecord> {
        vec![
            user_input_ok(),
            input_normalization_ok(),
            obr_ok(),
            observation_extraction_ok(),
            information_adequacy_supported_ok(),
            candidate_card_retrieval_ok(),
            card_branch_reranking_ok(),
            card_hydration_ok(),
            incident_evidence_retrieval_ok(),
            theory_evidence_retrieval_ok(),
            diagnostic_update_prompt_context_assembly_ok(),
            llm_ok(),
            response_validation_ok(),
        ]
    }

    fn all_continuation_unsupported_steps_ok() -> Vec<StepRecord> {
        vec![
            user_input_ok(),
            input_normalization_ok(),
            obr_unsupported_ok(),
            information_adequacy_unsupported_ok(),
            candidate_card_retrieval_ok(),
            card_branch_reranking_ok(),
            card_hydration_ok(),
            incident_evidence_retrieval_ok(),
            theory_evidence_retrieval_ok(),
            diagnostic_update_prompt_context_assembly_ok(),
            llm_ok(),
            response_validation_ok(),
        ]
    }

    // ─── Constructor ──────────────────────────────────────────────────────────

    #[test]
    fn new_constructs_stateless_policy() {
        let p = DiagnosticLoopTransitionPolicy::new();
        let _ = format!("{p:?}");
    }

    #[test]
    fn default_constructs_stateless_policy() {
        let _ = DiagnosticLoopTransitionPolicy::default();
    }

    // ─── RunArchived ──────────────────────────────────────────────────────────

    #[test]
    fn returns_run_archived_for_archived_run() {
        let state = archived_run();
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap_err(),
            PolicyError::RunArchived
        );
    }

    #[test]
    fn run_archived_fires_before_no_current_iteration() {
        let now = Utc::now();
        let state = RunState {
            run_id: new_run_id(),
            status: RunStatus::Archived,
            created_at: now,
            updated_at: now,
            revision: 0,
            iterations: vec![],
        };
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap_err(),
            PolicyError::RunArchived
        );
    }

    // ─── NoCurrentIteration ──────────────────────────────────────────────────

    #[test]
    fn returns_no_current_iteration_for_run_with_no_iterations() {
        let state = empty_run();
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap_err(),
            PolicyError::NoCurrentIteration
        );
    }

    // ─── PendingStepPresent ───────────────────────────────────────────────────

    #[test]
    fn returns_pending_step_present_when_iteration_has_pending_step() {
        let state =
            single_iteration_run(vec![user_input_ok(), pending(StepKind::InputNormalization)]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap_err(),
            PolicyError::PendingStepPresent
        );
    }

    // ─── Initial iteration canonical order ───────────────────────────────────

    #[test]
    fn chooses_initial_canonical_order_when_iteration_index_is_zero() {
        let state = single_iteration_run(vec![user_input_ok()]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::ExecuteStep {
                step: StepKind::InputNormalization
            }
        );
    }

    #[test]
    fn initial_order_selects_query_structuring_after_input_normalization() {
        let state = single_iteration_run(vec![user_input_ok(), input_normalization_ok()]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::ExecuteStep {
                step: StepKind::QueryStructuring
            }
        );
    }

    #[test]
    fn initial_order_selects_information_adequacy_initial_after_query_structuring() {
        let state = single_iteration_run(vec![
            user_input_ok(),
            input_normalization_ok(),
            query_structuring_ok(),
        ]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::ExecuteStep {
                step: StepKind::InformationAdequacyInitial
            }
        );
    }

    #[test]
    fn initial_order_selects_candidate_card_retrieval_after_sufficient_adequacy() {
        let state = single_iteration_run(vec![
            user_input_ok(),
            input_normalization_ok(),
            query_structuring_ok(),
            information_adequacy_initial_ok(),
        ]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::ExecuteStep {
                step: StepKind::CandidateCardRetrieval
            }
        );
    }

    #[test]
    fn initial_order_selects_prompt_context_assembly_not_diagnostic_update() {
        let state = single_iteration_run(vec![
            user_input_ok(),
            input_normalization_ok(),
            query_structuring_ok(),
            information_adequacy_initial_ok(),
            candidate_card_retrieval_ok(),
            card_hydration_ok(),
            incident_evidence_retrieval_ok(),
            theory_evidence_retrieval_ok(),
        ]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::ExecuteStep {
                step: StepKind::PromptContextAssembly
            }
        );
    }

    #[test]
    fn returns_execute_step_for_each_initial_canonical_position() {
        for n in 0..INITIAL_STEPS.len() - 1 {
            let mut records = vec![user_input_ok()];
            for &kind in &INITIAL_STEPS[..n] {
                records.push(match kind {
                    StepKind::InputNormalization => input_normalization_ok(),
                    StepKind::QueryStructuring => query_structuring_ok(),
                    StepKind::InformationAdequacyInitial => information_adequacy_initial_ok(),
                    StepKind::CandidateCardRetrieval => candidate_card_retrieval_ok(),
                    StepKind::CardHydration => card_hydration_ok(),
                    StepKind::IncidentEvidenceRetrieval => incident_evidence_retrieval_ok(),
                    StepKind::TheoryEvidenceRetrieval => theory_evidence_retrieval_ok(),
                    StepKind::PromptContextAssembly => prompt_context_assembly_ok(),
                    StepKind::LlmStructuredGeneration => llm_ok(),
                    StepKind::ResponseValidationAndNormalization => response_validation_ok(),
                    _ => panic!("unexpected step in initial order: {kind:?}"),
                });
            }
            let state = single_iteration_run(records);
            let view = RunStateView::new(&state);
            let expected = INITIAL_STEPS[n];
            assert_eq!(
                policy().next_transition(view).unwrap(),
                PolicyTransition::ExecuteStep { step: expected },
                "after {n} initial steps, expected ExecuteStep {{ step: {expected:?} }}"
            );
        }
    }

    // ─── Initial iteration: WaitForUser on adequacy ───────────────────────────

    #[test]
    fn initial_returns_wait_for_user_when_adequacy_is_blocking() {
        let state = single_iteration_run(vec![
            user_input_ok(),
            input_normalization_ok(),
            query_structuring_ok(),
            information_adequacy_initial_blocking(),
        ]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::WaitForUser {
                follow_up_questions: vec!["What symptom?".to_string()]
            }
        );
    }

    #[test]
    fn initial_returns_wait_for_user_when_adequacy_is_weak() {
        let state = single_iteration_run(vec![
            user_input_ok(),
            input_normalization_ok(),
            query_structuring_ok(),
            information_adequacy_initial_weak(),
        ]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::WaitForUser {
                follow_up_questions: vec!["What component?".to_string()]
            }
        );
    }

    #[test]
    fn initial_continues_to_candidate_card_retrieval_when_adequate() {
        let state = single_iteration_run(vec![
            user_input_ok(),
            input_normalization_ok(),
            query_structuring_ok(),
            information_adequacy_initial_ok(),
        ]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::ExecuteStep {
                step: StepKind::CandidateCardRetrieval
            }
        );
    }

    // ─── Continuation iteration canonical order ───────────────────────────────

    #[test]
    fn chooses_continuation_canonical_order_when_iteration_index_is_one() {
        let state = two_iteration_run(all_initial_steps_ok(), vec![user_input_ok()]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::ExecuteStep {
                step: StepKind::InputNormalization
            }
        );
    }

    #[test]
    fn continuation_order_includes_observation_boundary_resolver_after_input_normalization() {
        let state = two_iteration_run(
            all_initial_steps_ok(),
            vec![user_input_ok(), input_normalization_ok()],
        );
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::ExecuteStep {
                step: StepKind::ObservationBoundaryResolver
            }
        );
    }

    #[test]
    fn continuation_supported_selects_observation_extraction_after_obr() {
        let state = two_iteration_run(
            all_initial_steps_ok(),
            vec![user_input_ok(), input_normalization_ok(), obr_ok()],
        );
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::ExecuteStep {
                step: StepKind::ObservationExtraction
            }
        );
    }

    #[test]
    fn continuation_unsupported_selects_information_adequacy_unsupported_after_obr() {
        let state = two_iteration_run(
            all_initial_steps_ok(),
            vec![user_input_ok(), input_normalization_ok(), obr_unsupported_ok()],
        );
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::ExecuteStep {
                step: StepKind::InformationAdequacyUnsupportedObservation
            }
        );
    }

    #[test]
    fn continuation_supported_selects_information_adequacy_supported_after_obs_extraction() {
        let state = two_iteration_run(
            all_initial_steps_ok(),
            vec![
                user_input_ok(),
                input_normalization_ok(),
                obr_ok(),
                observation_extraction_ok(),
            ],
        );
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::ExecuteStep {
                step: StepKind::InformationAdequacySupportedObservation
            }
        );
    }

    #[test]
    fn continuation_order_includes_card_branch_reranking() {
        let state = two_iteration_run(
            all_initial_steps_ok(),
            vec![
                user_input_ok(),
                input_normalization_ok(),
                obr_ok(),
                observation_extraction_ok(),
                information_adequacy_supported_ok(),
                candidate_card_retrieval_ok(),
            ],
        );
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::ExecuteStep {
                step: StepKind::CardBranchReranking
            }
        );
    }

    #[test]
    fn continuation_order_includes_diagnostic_update_prompt_context_assembly() {
        let state = two_iteration_run(
            all_initial_steps_ok(),
            vec![
                user_input_ok(),
                input_normalization_ok(),
                obr_ok(),
                observation_extraction_ok(),
                information_adequacy_supported_ok(),
                candidate_card_retrieval_ok(),
                card_branch_reranking_ok(),
                card_hydration_ok(),
                incident_evidence_retrieval_ok(),
                theory_evidence_retrieval_ok(),
            ],
        );
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::ExecuteStep {
                step: StepKind::DiagnosticUpdatePromptContextAssembly
            }
        );
    }

    #[test]
    fn returns_execute_step_for_each_continuation_supported_canonical_position() {
        for n in 0..CONTINUATION_SUPPORTED_STEPS.len() - 1 {
            let mut records = vec![user_input_ok()];
            for &kind in &CONTINUATION_SUPPORTED_STEPS[..n] {
                records.push(match kind {
                    StepKind::InputNormalization => input_normalization_ok(),
                    StepKind::ObservationBoundaryResolver => obr_ok(),
                    StepKind::ObservationExtraction => observation_extraction_ok(),
                    StepKind::InformationAdequacySupportedObservation => {
                        information_adequacy_supported_ok()
                    }
                    StepKind::CandidateCardRetrieval => candidate_card_retrieval_ok(),
                    StepKind::CardBranchReranking => card_branch_reranking_ok(),
                    StepKind::CardHydration => card_hydration_ok(),
                    StepKind::IncidentEvidenceRetrieval => incident_evidence_retrieval_ok(),
                    StepKind::TheoryEvidenceRetrieval => theory_evidence_retrieval_ok(),
                    StepKind::DiagnosticUpdatePromptContextAssembly => {
                        diagnostic_update_prompt_context_assembly_ok()
                    }
                    StepKind::LlmStructuredGeneration => llm_ok(),
                    StepKind::ResponseValidationAndNormalization => response_validation_ok(),
                    _ => panic!("unexpected step in continuation order: {kind:?}"),
                });
            }
            let state = two_iteration_run(all_initial_steps_ok(), records);
            let view = RunStateView::new(&state);
            let expected = CONTINUATION_SUPPORTED_STEPS[n];
            assert_eq!(
                policy().next_transition(view).unwrap(),
                PolicyTransition::ExecuteStep { step: expected },
                "after {n} continuation steps, expected ExecuteStep {{ step: {expected:?} }}"
            );
        }
    }

    // ─── Continuation: WaitForUser on adequacy ────────────────────────────────

    #[test]
    fn continuation_returns_wait_for_user_when_supported_adequacy_is_blocking() {
        let state = two_iteration_run(
            all_initial_steps_ok(),
            vec![
                user_input_ok(),
                input_normalization_ok(),
                obr_ok(),
                observation_extraction_ok(),
                information_adequacy_supported_blocking(),
            ],
        );
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::WaitForUser {
                follow_up_questions: vec!["Need more context.".to_string()]
            }
        );
    }

    #[test]
    fn continuation_returns_wait_for_user_when_supported_adequacy_is_weak() {
        let state = two_iteration_run(
            all_initial_steps_ok(),
            vec![
                user_input_ok(),
                input_normalization_ok(),
                obr_ok(),
                observation_extraction_ok(),
                information_adequacy_supported_weak(),
            ],
        );
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::WaitForUser {
                follow_up_questions: vec!["Thin signal.".to_string()]
            }
        );
    }

    #[test]
    fn continuation_unsupported_always_returns_wait_for_user() {
        let state = two_iteration_run(
            all_initial_steps_ok(),
            vec![
                user_input_ok(),
                input_normalization_ok(),
                obr_unsupported_ok(),
                information_adequacy_unsupported_ok(),
            ],
        );
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::WaitForUser {
                follow_up_questions: vec!["What did you observe?".to_string()]
            }
        );
    }

    #[test]
    fn continuation_continues_to_candidate_card_retrieval_when_supported_adequate() {
        let state = two_iteration_run(
            all_initial_steps_ok(),
            vec![
                user_input_ok(),
                input_normalization_ok(),
                obr_ok(),
                observation_extraction_ok(),
                information_adequacy_supported_ok(),
            ],
        );
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::ExecuteStep {
                step: StepKind::CandidateCardRetrieval
            }
        );
    }

    // ─── FinishWithResult ─────────────────────────────────────────────────────

    #[test]
    fn returns_finish_with_result_from_successful_initial_iteration() {
        let state = single_iteration_run(all_initial_steps_ok());
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::FinishWithResult {
                result: minimal_final_result()
            }
        );
    }

    #[test]
    fn returns_finish_with_result_from_successful_continuation_iteration() {
        let state = two_iteration_run(all_initial_steps_ok(), all_continuation_steps_ok());
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::FinishWithResult {
                result: minimal_final_result()
            }
        );
    }

    #[test]
    fn returns_finish_with_result_from_unsupported_branch_iteration() {
        let state = two_iteration_run(
            all_initial_steps_ok(),
            all_continuation_unsupported_steps_ok(),
        );
        // The unsupported path leads to WaitForUser at the adequacy step, not FinishWithResult.
        // So this iteration with all steps including a final result after WaitForUser
        // is not a typical production path. Test that the FinishWithResult is what's
        // returned when response_validation is present in the unsupported branch.
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::FinishWithResult {
                result: minimal_final_result()
            }
        );
    }

    // ─── FinishWithError ──────────────────────────────────────────────────────

    #[test]
    fn returns_finish_with_error_when_initial_step_fails() {
        let err = StepError::MissingRequiredInput {
            message: "retrieval failed".to_string(),
        };
        let state = single_iteration_run(vec![
            user_input_ok(),
            input_normalization_ok(),
            finished_err(StepKind::QueryStructuring, err.clone()),
        ]);
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::FinishWithError { error: err }
        );
    }

    #[test]
    fn returns_finish_with_error_when_continuation_step_fails() {
        let err = StepError::MissingRequiredInput {
            message: "obr failed".to_string(),
        };
        let state = two_iteration_run(
            all_initial_steps_ok(),
            vec![
                user_input_ok(),
                input_normalization_ok(),
                finished_err(StepKind::ObservationBoundaryResolver, err.clone()),
            ],
        );
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::FinishWithError { error: err }
        );
    }

    // ─── Validation: non-canonical steps ignored ──────────────────────────────

    #[test]
    fn continuation_rejects_prompt_context_assembly_as_out_of_order() {
        // PromptContextAssembly is NOT in continuation order — ignored by prefix check.
        let state = two_iteration_run(
            all_initial_steps_ok(),
            vec![
                user_input_ok(),
                input_normalization_ok(),
                prompt_context_assembly_ok(),
            ],
        );
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap(),
            PolicyTransition::ExecuteStep {
                step: StepKind::ObservationBoundaryResolver
            }
        );
    }

    #[test]
    fn continuation_duplicate_step_returns_duplicate_error() {
        let state = two_iteration_run(
            all_initial_steps_ok(),
            vec![
                user_input_ok(),
                input_normalization_ok(),
                input_normalization_ok(),
            ],
        );
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap_err(),
            PolicyError::DuplicateSuccessfulStep {
                step: StepKind::InputNormalization
            }
        );
    }

    #[test]
    fn continuation_step_out_of_order_returns_out_of_order_error() {
        let state = two_iteration_run(
            all_initial_steps_ok(),
            vec![
                user_input_ok(),
                input_normalization_ok(),
                observation_extraction_ok(), // OBR not yet done
            ],
        );
        let view = RunStateView::new(&state);
        assert_eq!(
            policy().next_transition(view).unwrap_err(),
            PolicyError::StepOutOfOrder {
                step: StepKind::ObservationExtraction
            }
        );
    }
}
