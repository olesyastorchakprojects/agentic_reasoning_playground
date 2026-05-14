use distributed_diagnostics::orchestrator::run_state::model::{
    FinishedStepRecord, RunId, RunIteration, RunIterationId, RunState, StepError, StepKind,
    StepRecord, StepResultEnvelope,
};
use distributed_diagnostics::shared_types::{
    CandidateCardRetrievalOutput, CardHydrationOutput, GoldenQuestion, IncidentEvidenceRetrievalOutput,
    IterationProfile, LlmStructuredGenerationOutput, ModelTokenUsage, NormalizedUserRequest,
    ObservationBoundaryResolverOutput, ObservationExtractionOutput, PromptContextAssemblyOutput,
    QueryStructuringOutput, ResponseValidationAndNormalizationOutput, RunConfigSnapshot,
    RuntimeLlmStageConfigSnapshot, TheoryEvidenceRetrievalOutput, UserRequest,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotIterationSelector {
    LastCompletedIteration,
    ExactIteration(RunIterationId),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RuntimeLlmStageUsageSummary {
    pub provider: String,
    pub model_name: String,
    pub token_usage: ModelTokenUsage,
    pub input_cost_per_million_tokens: f64,
    pub output_cost_per_million_tokens: f64,
    pub prompt_cost_usd: f64,
    pub completion_cost_usd: f64,
    pub total_cost_usd: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RuntimeUsageTotalsSummary {
    pub token_usage: ModelTokenUsage,
    pub prompt_cost_usd: f64,
    pub completion_cost_usd: f64,
    pub total_cost_usd: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RuntimeTokenUsageSummary {
    pub query_structuring: RuntimeLlmStageUsageSummary,
    pub observation_boundary_resolver: RuntimeLlmStageUsageSummary,
    pub observation_extraction: RuntimeLlmStageUsageSummary,
    pub llm_structured_generation: RuntimeLlmStageUsageSummary,
    pub total: RuntimeUsageTotalsSummary,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiagnosticEvalIterationSnapshot {
    pub runtime_run_id: RunId,
    pub iteration_id: RunIterationId,
    pub iteration_kind: IterationProfile,
    pub run_created_at: chrono::DateTime<chrono::Utc>,
    pub run_updated_at: chrono::DateTime<chrono::Utc>,
    pub user_request: UserRequest,
    pub golden_question: Option<GoldenQuestion>,
    pub normalized_user_request: NormalizedUserRequest,
    pub query_structuring_output: QueryStructuringOutput,
    pub candidate_card_retrieval_output: CandidateCardRetrievalOutput,
    pub card_hydration_output: CardHydrationOutput,
    pub incident_evidence_retrieval_output: IncidentEvidenceRetrievalOutput,
    pub theory_evidence_retrieval_output: TheoryEvidenceRetrievalOutput,
    pub prompt_context_assembly_output: PromptContextAssemblyOutput,
    pub llm_structured_generation_output: LlmStructuredGenerationOutput,
    pub response_validation_and_normalization_output: ResponseValidationAndNormalizationOutput,
    /// Present only for continuation iterations.
    pub observation_boundary_resolver_output: Option<ObservationBoundaryResolverOutput>,
    /// Present only for continuation iterations.
    pub observation_extraction_output: Option<ObservationExtractionOutput>,
    /// Present only for continuation iterations — the snapshot of the directly preceding
    /// completed iteration.
    pub previous_snapshot: Option<Box<DiagnosticEvalIterationSnapshot>>,
    pub config_snapshot: Option<RunConfigSnapshot>,
    pub runtime_token_usage: RuntimeTokenUsageSummary,
}

#[derive(Debug, thiserror::Error)]
pub enum SnapshotBuildError {
    #[error("no completed iteration found in run {run_id:?}")]
    NoCompletedIteration { run_id: RunId },
    #[error("iteration {iteration_id:?} not found in run {run_id:?}")]
    IterationNotFound {
        run_id: RunId,
        iteration_id: RunIterationId,
    },
    #[error("continuation iteration {iteration_id:?} has no previous completed iteration in run {run_id:?}")]
    NoPreviousCompletedIteration {
        run_id: RunId,
        iteration_id: RunIterationId,
    },
    #[error("required step {step} is missing in iteration {iteration_id:?}")]
    MissingRequiredStep {
        iteration_id: RunIterationId,
        step: StepKind,
    },
    #[error("step {step} failed in iteration {iteration_id:?}: {message}")]
    StepFailed {
        iteration_id: RunIterationId,
        step: StepKind,
        message: String,
    },
    #[error("step {step} produced an unexpected payload in iteration {iteration_id:?}")]
    UnexpectedStepPayload {
        iteration_id: RunIterationId,
        step: StepKind,
    },
}

pub fn build_snapshot(
    run_state: &RunState,
    selector: SnapshotIterationSelector,
) -> Result<DiagnosticEvalIterationSnapshot, SnapshotBuildError> {
    let iteration = select_iteration(run_state, selector)?;
    build_snapshot_for_iteration(run_state, iteration)
}

fn build_snapshot_for_iteration(
    run_state: &RunState,
    iteration: &RunIteration,
) -> Result<DiagnosticEvalIterationSnapshot, SnapshotBuildError> {
    let iteration_kind = detect_iteration_kind(iteration);

    let user_request = expect_user_request(iteration)?;
    let normalized_user_request = expect_input_normalization(iteration)?;

    let (
        query_structuring_output,
        prompt_context_assembly_output,
        observation_boundary_resolver_output,
        observation_extraction_output,
        previous_snapshot,
    ) = if iteration_kind == IterationProfile::Continuation {
        let prev_iteration = find_previous_completed_iteration(run_state, iteration)?;
        let prev_snap = build_snapshot_for_iteration(run_state, prev_iteration)?;
        let obr = expect_observation_boundary_resolver(iteration)?;
        let oe = expect_observation_extraction(iteration)?;
        let pca = expect_diagnostic_update_prompt_context_assembly(iteration)?;
        (
            prev_snap.query_structuring_output.clone(),
            pca,
            Some(obr),
            Some(oe),
            Some(Box::new(prev_snap)),
        )
    } else {
        let qs = expect_query_structuring(iteration)?;
        let pca = expect_prompt_context_assembly(iteration)?;
        (qs, pca, None, None, None)
    };

    let candidate_card_retrieval_output = expect_candidate_card_retrieval(iteration)?;
    let card_hydration_output = expect_card_hydration(iteration)?;
    let incident_evidence_retrieval_output = expect_incident_evidence_retrieval(iteration)?;
    let theory_evidence_retrieval_output = expect_theory_evidence_retrieval(iteration)?;
    let llm_structured_generation_output = expect_llm_structured_generation(iteration)?;
    let response_validation_and_normalization_output =
        expect_response_validation_and_normalization(iteration)?;

    let query_structuring_usage = build_stage_usage(
        iteration.config_snapshot.as_ref().map(|s| &s.query_structuring),
        if iteration_kind == IterationProfile::Continuation {
            empty_token_usage()
        } else {
            query_structuring_output.token_usage.clone()
        },
    );
    let observation_boundary_resolver_usage = build_stage_usage(
        iteration
            .config_snapshot
            .as_ref()
            .map(|s| &s.observation_boundary_resolver),
        observation_boundary_resolver_output
            .as_ref()
            .map(|output| output.token_usage.clone())
            .unwrap_or_else(empty_token_usage),
    );
    let observation_extraction_usage = build_stage_usage(
        iteration
            .config_snapshot
            .as_ref()
            .map(|s| &s.observation_extraction),
        observation_extraction_output
            .as_ref()
            .map(|output| output.token_usage.clone())
            .unwrap_or_else(empty_token_usage),
    );
    let llm_structured_generation_usage = build_stage_usage(
        iteration
            .config_snapshot
            .as_ref()
            .map(|s| &s.llm_structured_generation),
        llm_structured_generation_output.token_usage.clone(),
    );
    let total_token_usage = combine_token_usage_many([
        &query_structuring_usage.token_usage,
        &observation_boundary_resolver_usage.token_usage,
        &observation_extraction_usage.token_usage,
        &llm_structured_generation_usage.token_usage,
    ]);
    let runtime_token_usage = RuntimeTokenUsageSummary {
        query_structuring: query_structuring_usage.clone(),
        observation_boundary_resolver: observation_boundary_resolver_usage.clone(),
        observation_extraction: observation_extraction_usage.clone(),
        llm_structured_generation: llm_structured_generation_usage.clone(),
        total: RuntimeUsageTotalsSummary {
            token_usage: total_token_usage,
            prompt_cost_usd: query_structuring_usage.prompt_cost_usd
                + observation_boundary_resolver_usage.prompt_cost_usd
                + observation_extraction_usage.prompt_cost_usd
                + llm_structured_generation_usage.prompt_cost_usd,
            completion_cost_usd: query_structuring_usage.completion_cost_usd
                + observation_boundary_resolver_usage.completion_cost_usd
                + observation_extraction_usage.completion_cost_usd
                + llm_structured_generation_usage.completion_cost_usd,
            total_cost_usd: query_structuring_usage.total_cost_usd
                + observation_boundary_resolver_usage.total_cost_usd
                + observation_extraction_usage.total_cost_usd
                + llm_structured_generation_usage.total_cost_usd,
        },
    };

    Ok(DiagnosticEvalIterationSnapshot {
        runtime_run_id: run_state.run_id,
        iteration_id: iteration.iteration_id,
        iteration_kind,
        run_created_at: run_state.created_at,
        run_updated_at: run_state.updated_at,
        golden_question: user_request.golden_question.clone(),
        user_request,
        normalized_user_request,
        query_structuring_output,
        candidate_card_retrieval_output,
        card_hydration_output,
        incident_evidence_retrieval_output,
        theory_evidence_retrieval_output,
        prompt_context_assembly_output,
        llm_structured_generation_output,
        response_validation_and_normalization_output,
        observation_boundary_resolver_output,
        observation_extraction_output,
        previous_snapshot,
        config_snapshot: iteration.config_snapshot.clone(),
        runtime_token_usage,
    })
}

fn detect_iteration_kind(iteration: &RunIteration) -> IterationProfile {
    let has_obr = iteration.step_records.iter().any(|record| {
        matches!(
            record,
            StepRecord::Finished(FinishedStepRecord {
                step: StepKind::ObservationBoundaryResolver,
                ..
            })
        )
    });
    if has_obr {
        IterationProfile::Continuation
    } else {
        IterationProfile::Initial
    }
}

fn find_previous_completed_iteration<'a>(
    run_state: &'a RunState,
    current: &RunIteration,
) -> Result<&'a RunIteration, SnapshotBuildError> {
    let current_pos = run_state
        .iterations
        .iter()
        .position(|i| i.iteration_id == current.iteration_id)
        .ok_or(SnapshotBuildError::IterationNotFound {
            run_id: run_state.run_id,
            iteration_id: current.iteration_id,
        })?;

    run_state.iterations[..current_pos]
        .iter()
        .rev()
        .find(|i| has_successful_final_output(i))
        .ok_or(SnapshotBuildError::NoPreviousCompletedIteration {
            run_id: run_state.run_id,
            iteration_id: current.iteration_id,
        })
}

fn select_iteration<'a>(
    run_state: &'a RunState,
    selector: SnapshotIterationSelector,
) -> Result<&'a RunIteration, SnapshotBuildError> {
    match selector {
        SnapshotIterationSelector::LastCompletedIteration => run_state
            .iterations
            .iter()
            .rev()
            .find(|iteration| has_successful_final_output(iteration))
            .ok_or(SnapshotBuildError::NoCompletedIteration {
                run_id: run_state.run_id,
            }),
        SnapshotIterationSelector::ExactIteration(iteration_id) => run_state
            .iterations
            .iter()
            .find(|iteration| iteration.iteration_id == iteration_id)
            .ok_or(SnapshotBuildError::IterationNotFound {
                run_id: run_state.run_id,
                iteration_id,
            }),
    }
}

fn has_successful_final_output(iteration: &RunIteration) -> bool {
    iteration.step_records.iter().any(|record| {
        matches!(
            record,
            StepRecord::Finished(FinishedStepRecord {
                step: StepKind::ResponseValidationAndNormalization,
                result: Ok(StepResultEnvelope::ResponseValidationAndNormalization(_)),
                ..
            })
        )
    })
}

fn expect_user_request(
    iteration: &RunIteration,
) -> Result<UserRequest, SnapshotBuildError> {
    match expect_step_payload(iteration, StepKind::UserInputReceived)? {
        StepResultEnvelope::UserInputReceived(output) => Ok(output.clone()),
        _ => Err(SnapshotBuildError::UnexpectedStepPayload {
            iteration_id: iteration.iteration_id,
            step: StepKind::UserInputReceived,
        }),
    }
}

fn expect_input_normalization(
    iteration: &RunIteration,
) -> Result<NormalizedUserRequest, SnapshotBuildError> {
    match expect_step_payload(iteration, StepKind::InputNormalization)? {
        StepResultEnvelope::InputNormalization(output) => Ok(output.clone()),
        _ => Err(SnapshotBuildError::UnexpectedStepPayload {
            iteration_id: iteration.iteration_id,
            step: StepKind::InputNormalization,
        }),
    }
}

fn expect_query_structuring(
    iteration: &RunIteration,
) -> Result<QueryStructuringOutput, SnapshotBuildError> {
    match expect_step_payload(iteration, StepKind::QueryStructuring)? {
        StepResultEnvelope::QueryStructuring(output) => Ok(output.clone()),
        _ => Err(SnapshotBuildError::UnexpectedStepPayload {
            iteration_id: iteration.iteration_id,
            step: StepKind::QueryStructuring,
        }),
    }
}

fn expect_candidate_card_retrieval(
    iteration: &RunIteration,
) -> Result<CandidateCardRetrievalOutput, SnapshotBuildError> {
    match expect_step_payload(iteration, StepKind::CandidateCardRetrieval)? {
        StepResultEnvelope::CandidateCardRetrieval(output) => Ok(output.clone()),
        _ => Err(SnapshotBuildError::UnexpectedStepPayload {
            iteration_id: iteration.iteration_id,
            step: StepKind::CandidateCardRetrieval,
        }),
    }
}

fn expect_card_hydration(
    iteration: &RunIteration,
) -> Result<CardHydrationOutput, SnapshotBuildError> {
    match expect_step_payload(iteration, StepKind::CardHydration)? {
        StepResultEnvelope::CardHydration(output) => Ok(output.clone()),
        _ => Err(SnapshotBuildError::UnexpectedStepPayload {
            iteration_id: iteration.iteration_id,
            step: StepKind::CardHydration,
        }),
    }
}

fn expect_incident_evidence_retrieval(
    iteration: &RunIteration,
) -> Result<IncidentEvidenceRetrievalOutput, SnapshotBuildError> {
    match expect_step_payload(iteration, StepKind::IncidentEvidenceRetrieval)? {
        StepResultEnvelope::IncidentEvidenceRetrieval(output) => Ok(output.clone()),
        _ => Err(SnapshotBuildError::UnexpectedStepPayload {
            iteration_id: iteration.iteration_id,
            step: StepKind::IncidentEvidenceRetrieval,
        }),
    }
}

fn expect_theory_evidence_retrieval(
    iteration: &RunIteration,
) -> Result<TheoryEvidenceRetrievalOutput, SnapshotBuildError> {
    match expect_step_payload(iteration, StepKind::TheoryEvidenceRetrieval)? {
        StepResultEnvelope::TheoryEvidenceRetrieval(output) => Ok(output.clone()),
        _ => Err(SnapshotBuildError::UnexpectedStepPayload {
            iteration_id: iteration.iteration_id,
            step: StepKind::TheoryEvidenceRetrieval,
        }),
    }
}

fn expect_prompt_context_assembly(
    iteration: &RunIteration,
) -> Result<PromptContextAssemblyOutput, SnapshotBuildError> {
    match expect_step_payload(iteration, StepKind::PromptContextAssembly)? {
        StepResultEnvelope::PromptContextAssembly(output) => Ok(output.clone()),
        _ => Err(SnapshotBuildError::UnexpectedStepPayload {
            iteration_id: iteration.iteration_id,
            step: StepKind::PromptContextAssembly,
        }),
    }
}

fn expect_diagnostic_update_prompt_context_assembly(
    iteration: &RunIteration,
) -> Result<PromptContextAssemblyOutput, SnapshotBuildError> {
    match expect_step_payload(iteration, StepKind::DiagnosticUpdatePromptContextAssembly)? {
        StepResultEnvelope::DiagnosticUpdatePromptContextAssembly(output) => Ok(output.clone()),
        _ => Err(SnapshotBuildError::UnexpectedStepPayload {
            iteration_id: iteration.iteration_id,
            step: StepKind::DiagnosticUpdatePromptContextAssembly,
        }),
    }
}

fn expect_llm_structured_generation(
    iteration: &RunIteration,
) -> Result<LlmStructuredGenerationOutput, SnapshotBuildError> {
    match expect_step_payload(iteration, StepKind::LlmStructuredGeneration)? {
        StepResultEnvelope::LlmStructuredGeneration(output) => Ok(output.clone()),
        _ => Err(SnapshotBuildError::UnexpectedStepPayload {
            iteration_id: iteration.iteration_id,
            step: StepKind::LlmStructuredGeneration,
        }),
    }
}

fn expect_response_validation_and_normalization(
    iteration: &RunIteration,
) -> Result<ResponseValidationAndNormalizationOutput, SnapshotBuildError> {
    match expect_step_payload(iteration, StepKind::ResponseValidationAndNormalization)? {
        StepResultEnvelope::ResponseValidationAndNormalization(output) => Ok(output.clone()),
        _ => Err(SnapshotBuildError::UnexpectedStepPayload {
            iteration_id: iteration.iteration_id,
            step: StepKind::ResponseValidationAndNormalization,
        }),
    }
}

fn expect_observation_extraction(
    iteration: &RunIteration,
) -> Result<ObservationExtractionOutput, SnapshotBuildError> {
    match expect_step_payload(iteration, StepKind::ObservationExtraction)? {
        StepResultEnvelope::ObservationExtraction(output) => Ok(output.clone()),
        _ => Err(SnapshotBuildError::UnexpectedStepPayload {
            iteration_id: iteration.iteration_id,
            step: StepKind::ObservationExtraction,
        }),
    }
}

fn expect_observation_boundary_resolver(
    iteration: &RunIteration,
) -> Result<ObservationBoundaryResolverOutput, SnapshotBuildError> {
    match expect_step_payload(iteration, StepKind::ObservationBoundaryResolver)? {
        StepResultEnvelope::ObservationBoundaryResolver(output) => Ok(output.clone()),
        _ => Err(SnapshotBuildError::UnexpectedStepPayload {
            iteration_id: iteration.iteration_id,
            step: StepKind::ObservationBoundaryResolver,
        }),
    }
}

fn expect_step_payload<'a>(
    iteration: &'a RunIteration,
    step: StepKind,
) -> Result<&'a StepResultEnvelope, SnapshotBuildError> {
    let finished = iteration
        .step_records
        .iter()
        .find_map(|record| match record {
            StepRecord::Finished(finished) if finished.step == step => Some(finished),
            _ => None,
        })
        .ok_or(SnapshotBuildError::MissingRequiredStep {
            iteration_id: iteration.iteration_id,
            step,
        })?;

    match &finished.result {
        Ok(payload) => Ok(payload),
        Err(error) => Err(SnapshotBuildError::StepFailed {
            iteration_id: iteration.iteration_id,
            step,
            message: format_step_error(error),
        }),
    }
}

fn format_step_error(error: &StepError) -> String {
    error.to_string()
}

fn combine_token_usage(
    lhs: &ModelTokenUsage,
    rhs: &ModelTokenUsage,
) -> ModelTokenUsage {
    ModelTokenUsage {
        prompt_tokens: sum_optional_usize([lhs.prompt_tokens, rhs.prompt_tokens]),
        completion_tokens: sum_optional_usize([lhs.completion_tokens, rhs.completion_tokens]),
        total_tokens: sum_optional_usize([lhs.total_tokens, rhs.total_tokens]),
    }
}

fn combine_token_usage_many(values: [&ModelTokenUsage; 4]) -> ModelTokenUsage {
    let mut prompt_tokens = 0_usize;
    let mut completion_tokens = 0_usize;
    let mut total_tokens = 0_usize;
    let mut saw_prompt = false;
    let mut saw_completion = false;
    let mut saw_total = false;

    for usage in values {
        if let Some(value) = usage.prompt_tokens {
            prompt_tokens += value;
            saw_prompt = true;
        }
        if let Some(value) = usage.completion_tokens {
            completion_tokens += value;
            saw_completion = true;
        }
        if let Some(value) = usage.total_tokens {
            total_tokens += value;
            saw_total = true;
        }
    }

    ModelTokenUsage {
        prompt_tokens: saw_prompt.then_some(prompt_tokens),
        completion_tokens: saw_completion.then_some(completion_tokens),
        total_tokens: saw_total.then_some(total_tokens),
    }
}

fn build_stage_usage(
    config: Option<&RuntimeLlmStageConfigSnapshot>,
    token_usage: ModelTokenUsage,
) -> RuntimeLlmStageUsageSummary {
    let provider = config
        .map(|cfg| cfg.provider.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let model_name = config
        .map(|cfg| cfg.model_name.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let input_cost_per_million_tokens = config
        .map(|cfg| cfg.input_cost_per_million_tokens)
        .unwrap_or(0.0);
    let output_cost_per_million_tokens = config
        .map(|cfg| cfg.output_cost_per_million_tokens)
        .unwrap_or(0.0);
    let prompt_cost_usd =
        token_usage.prompt_tokens.unwrap_or(0) as f64 * input_cost_per_million_tokens / 1_000_000.0;
    let completion_cost_usd = token_usage.completion_tokens.unwrap_or(0) as f64
        * output_cost_per_million_tokens
        / 1_000_000.0;

    RuntimeLlmStageUsageSummary {
        provider,
        model_name,
        token_usage,
        input_cost_per_million_tokens,
        output_cost_per_million_tokens,
        prompt_cost_usd,
        completion_cost_usd,
        total_cost_usd: prompt_cost_usd + completion_cost_usd,
    }
}

fn empty_token_usage() -> ModelTokenUsage {
    ModelTokenUsage {
        prompt_tokens: None,
        completion_tokens: None,
        total_tokens: None,
    }
}

fn sum_optional_usize(values: [Option<usize>; 2]) -> Option<usize> {
    let mut total = 0_usize;
    let mut saw_any = false;
    for value in values.into_iter().flatten() {
        total += value;
        saw_any = true;
    }
    if saw_any {
        Some(total)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use distributed_diagnostics::orchestrator::run_state::model::{
        FinishedStepRecord, RunId, RunIteration, RunIterationId, RunIterationStatus, RunState, RunStatus, StepError,
        StepKind, StepRecord, StepRecordId, StepResultEnvelope,
    };
    use distributed_diagnostics::shared_types::{
        Confidence, DiagnosticResponse, DiagnosticResultInterpretation, ExtractedObservation,
        Hypothesis, HypothesisEvidenceSource, HypothesisId, HypothesisStatus,
        IncidentEvidenceRetrievalOutput, IterationProfile, LlmStructuredGenerationOutput,
        ModelTokenUsage, NormalizedUserRequest, ObservationExtractionOutput, ObservationPolarity,
        PromptContextAssemblyOutput, QueryStructuringOutput, ResponseValidationAndNormalizationOutput,
        ResolvedObservation, StructuredUserQuery, StructuredUserQueryConfidence, UserRequest,
    };
    use serde_json::json;
    use uuid::Uuid;

    use crate::snapshot::{
        build_snapshot, DiagnosticEvalIterationSnapshot, SnapshotBuildError,
        SnapshotIterationSelector,
    };

    fn step_record(step: StepKind, result: StepResultEnvelope) -> StepRecord {
        let now = Utc::now();
        StepRecord::Finished(FinishedStepRecord {
            record_id: StepRecordId(Uuid::new_v4()),
            step,
            started_at: now,
            finished_at: now,
            result: Ok(result),
        })
    }

    fn failed_step_record(step: StepKind) -> StepRecord {
        let now = Utc::now();
        StepRecord::Finished(FinishedStepRecord {
            record_id: StepRecordId(Uuid::new_v4()),
            step,
            started_at: now,
            finished_at: now,
            result: Err(StepError::Unexpected {
                message: "boom".to_string(),
            }),
        })
    }

    fn minimal_iteration(iteration_id: RunIterationId) -> RunIteration {
        RunIteration {
            iteration_id,
            config_snapshot: None,
            status: RunIterationStatus::Active,
            step_records: vec![
                step_record(
                    StepKind::UserInputReceived,
                    StepResultEnvelope::UserInputReceived(UserRequest {
                        query: "why did the cluster stall?".to_string(),
                        golden_question: None,
                    }),
                ),
                step_record(
                    StepKind::InputNormalization,
                    StepResultEnvelope::InputNormalization(NormalizedUserRequest {
                        query: "why did the cluster stall?".to_string(),
                        input_token_count: 6,
                    }),
                ),
                step_record(
                    StepKind::QueryStructuring,
                    StepResultEnvelope::QueryStructuring(QueryStructuringOutput {
                        structured_query: StructuredUserQuery {
                            intent: "diagnose".to_string(),
                            scenario: "distributed cluster".to_string(),
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
                            prompt_tokens: Some(100),
                            completion_tokens: Some(50),
                            total_tokens: Some(150),
                        },
                        metrics: None,
                    }),
                ),
                step_record(
                    StepKind::CandidateCardRetrieval,
                    StepResultEnvelope::CandidateCardRetrieval(
                        distributed_diagnostics::shared_types::CandidateCardRetrievalOutput {
                            ranked_candidates: vec![],
                            primary: None,
                            alternatives: vec![],
                            metrics: None,
                        },
                    ),
                ),
                step_record(
                    StepKind::CardHydration,
                    StepResultEnvelope::CardHydration(
                        distributed_diagnostics::shared_types::CardHydrationOutput {
                            primary: None,
                            alternatives: vec![],
                        },
                    ),
                ),
                step_record(
                    StepKind::IncidentEvidenceRetrieval,
                    StepResultEnvelope::IncidentEvidenceRetrieval(
                        IncidentEvidenceRetrievalOutput {
                            primary_chunks: vec![],
                            alternative_chunks: vec![],
                            metrics: None,
                        },
                    ),
                ),
                step_record(
                    StepKind::TheoryEvidenceRetrieval,
                    StepResultEnvelope::TheoryEvidenceRetrieval(
                        distributed_diagnostics::shared_types::TheoryEvidenceRetrievalOutput {
                            chunks: vec![],
                            metrics: None,
                        },
                    ),
                ),
                step_record(
                    StepKind::PromptContextAssembly,
                    StepResultEnvelope::PromptContextAssembly(PromptContextAssemblyOutput {
                        prompt: "prompt".to_string(),
                        response_schema: json!({"type": "object"}),
                        evidence_topology: Default::default(),
                        incident_evidence_chunks: vec![],
                        theory_chunks: vec![],
                    }),
                ),
                step_record(
                    StepKind::LlmStructuredGeneration,
                    StepResultEnvelope::LlmStructuredGeneration(LlmStructuredGenerationOutput {
                        response_json: json!({"problem_understanding": "foo"}),
                        token_usage: ModelTokenUsage {
                            prompt_tokens: Some(200),
                            completion_tokens: Some(100),
                            total_tokens: Some(300),
                        },
                    }),
                ),
                step_record(
                    StepKind::ResponseValidationAndNormalization,
                    StepResultEnvelope::ResponseValidationAndNormalization(
                        minimal_response_validation_output(),
                    ),
                ),
            ],
        }
    }

    fn minimal_response_validation_output() -> ResponseValidationAndNormalizationOutput {
        ResponseValidationAndNormalizationOutput {
            response: DiagnosticResponse {
                problem_understanding: "foo".to_string(),
                similar_practical_context: "bar".to_string(),
                hypotheses: vec![
                    Hypothesis {
                        id: HypothesisId(Uuid::from_u128(0xABCD)),
                        text: "leader election instability".to_string(),
                        status: HypothesisStatus::Active,
                        source: HypothesisEvidenceSource::PrimaryIncident,
                        confidence: Confidence::Medium,
                    },
                ],
                first_check: "check raft logs".to_string(),
                result_interpretation: DiagnosticResultInterpretation {
                    supports_primary_if: "if leader churn spikes".to_string(),
                    supports_competing_if: "if logs are clean".to_string(),
                    inconclusive_if: Some("if logs are missing".to_string()),
                },
                competing_interpretation: None,
            },
        }
    }

    fn minimal_continuation_iteration(
        iteration_id: RunIterationId,
        resolved_text: &str,
    ) -> RunIteration {
        RunIteration {
            iteration_id,
            config_snapshot: None,
            status: RunIterationStatus::Active,
            step_records: vec![
                step_record(
                    StepKind::UserInputReceived,
                    StepResultEnvelope::UserInputReceived(UserRequest {
                        query: "I ran the check: memory is stable".to_string(),
                        golden_question: None,
                    }),
                ),
                step_record(
                    StepKind::InputNormalization,
                    StepResultEnvelope::InputNormalization(NormalizedUserRequest {
                        query: "memory is stable".to_string(),
                        input_token_count: 3,
                    }),
                ),
                step_record(
                    StepKind::ObservationBoundaryResolver,
                    StepResultEnvelope::ObservationBoundaryResolver(
                        distributed_diagnostics::shared_types::ObservationBoundaryResolverOutput {
                            normalized_user_input: "memory is stable".to_string(),
                            confidence: Confidence::High,
                            reason: "new observation".to_string(),
                            resolution: distributed_diagnostics::shared_types::ObservationBoundaryResolution::Supported(
                                distributed_diagnostics::shared_types::ResolvedObservation {
                                    text: resolved_text.to_string(),
                                },
                            ),
                            token_usage: ModelTokenUsage {
                                prompt_tokens: None,
                                completion_tokens: None,
                                total_tokens: None,
                            },
                        },
                    ),
                ),
                step_record(
                    StepKind::ObservationExtraction,
                    StepResultEnvelope::ObservationExtraction(ObservationExtractionOutput {
                        normalized_user_input: "memory is stable".to_string(),
                        resolved_observation: ResolvedObservation {
                            text: resolved_text.to_string(),
                        },
                        confidence: Confidence::High,
                        observations: vec![ExtractedObservation {
                            statement: "memory usage is stable".to_string(),
                            confidence: Confidence::High,
                            condition: None,
                            polarity: ObservationPolarity::Present,
                            time_relation: None,
                            source_span: resolved_text.to_string(),
                        }],
                        needs_more_context: false,
                        missing_context_questions: vec![],
                        token_usage: ModelTokenUsage {
                            prompt_tokens: Some(50),
                            completion_tokens: Some(30),
                            total_tokens: Some(80),
                        },
                    }),
                ),
                step_record(
                    StepKind::CandidateCardRetrieval,
                    StepResultEnvelope::CandidateCardRetrieval(
                        distributed_diagnostics::shared_types::CandidateCardRetrievalOutput {
                            ranked_candidates: vec![],
                            primary: None,
                            alternatives: vec![],
                            metrics: None,
                        },
                    ),
                ),
                step_record(
                    StepKind::CardHydration,
                    StepResultEnvelope::CardHydration(
                        distributed_diagnostics::shared_types::CardHydrationOutput {
                            primary: None,
                            alternatives: vec![],
                        },
                    ),
                ),
                step_record(
                    StepKind::IncidentEvidenceRetrieval,
                    StepResultEnvelope::IncidentEvidenceRetrieval(
                        IncidentEvidenceRetrievalOutput {
                            primary_chunks: vec![],
                            alternative_chunks: vec![],
                            metrics: None,
                        },
                    ),
                ),
                step_record(
                    StepKind::TheoryEvidenceRetrieval,
                    StepResultEnvelope::TheoryEvidenceRetrieval(
                        distributed_diagnostics::shared_types::TheoryEvidenceRetrievalOutput {
                            chunks: vec![],
                            metrics: None,
                        },
                    ),
                ),
                step_record(
                    StepKind::DiagnosticUpdatePromptContextAssembly,
                    StepResultEnvelope::DiagnosticUpdatePromptContextAssembly(
                        PromptContextAssemblyOutput {
                            prompt: "update prompt".to_string(),
                            response_schema: json!({"type": "object"}),
                            evidence_topology: Default::default(),
                            incident_evidence_chunks: vec![],
                            theory_chunks: vec![],
                        },
                    ),
                ),
                step_record(
                    StepKind::LlmStructuredGeneration,
                    StepResultEnvelope::LlmStructuredGeneration(LlmStructuredGenerationOutput {
                        response_json: json!({"problem_understanding": "updated"}),
                        token_usage: ModelTokenUsage {
                            prompt_tokens: Some(150),
                            completion_tokens: Some(80),
                            total_tokens: Some(230),
                        },
                    }),
                ),
                step_record(
                    StepKind::ResponseValidationAndNormalization,
                    StepResultEnvelope::ResponseValidationAndNormalization(
                        minimal_response_validation_output(),
                    ),
                ),
            ],
        }
    }

    fn minimal_run_state(iterations: Vec<RunIteration>) -> RunState {
        let now = Utc::now();
        RunState {
            run_id: RunId(Uuid::new_v4()),
            status: RunStatus::Archived,
            created_at: now,
            updated_at: now,
            revision: 1,
            iterations,
        }
    }

    #[test]
    fn builds_snapshot_from_last_completed_iteration() {
        let older = minimal_iteration(RunIterationId(Uuid::new_v4()));
        let newer = minimal_iteration(RunIterationId(Uuid::new_v4()));
        let state = minimal_run_state(vec![older, newer.clone()]);

        let snapshot = build_snapshot(
            &state,
            SnapshotIterationSelector::LastCompletedIteration,
        )
        .expect("snapshot should build");

        assert_snapshot_iteration(&snapshot, newer.iteration_id);
        assert_eq!(snapshot.runtime_token_usage.total.token_usage.prompt_tokens, Some(300));
        assert_eq!(snapshot.runtime_token_usage.total.token_usage.completion_tokens, Some(150));
        assert_eq!(snapshot.runtime_token_usage.total.token_usage.total_tokens, Some(450));
        assert_eq!(snapshot.iteration_kind, IterationProfile::Initial);
        assert!(snapshot.observation_extraction_output.is_none());
        assert!(snapshot.previous_snapshot.is_none());
    }

    #[test]
    fn skips_latest_iteration_when_it_is_not_completed() {
        let completed = minimal_iteration(RunIterationId(Uuid::new_v4()));
        let incomplete = RunIteration {
            iteration_id: RunIterationId(Uuid::new_v4()),
            config_snapshot: None,
            status: RunIterationStatus::Active,
            step_records: vec![step_record(
                StepKind::UserInputReceived,
                StepResultEnvelope::UserInputReceived(UserRequest {
                    query: "partial".to_string(),
                    golden_question: None,
                }),
            )],
        };
        let state = minimal_run_state(vec![completed.clone(), incomplete]);

        let snapshot = build_snapshot(
            &state,
            SnapshotIterationSelector::LastCompletedIteration,
        )
        .expect("snapshot should use previous completed iteration");

        assert_snapshot_iteration(&snapshot, completed.iteration_id);
    }

    #[test]
    fn builds_snapshot_for_exact_frozen_iteration() {
        let older = minimal_iteration(RunIterationId(Uuid::new_v4()));
        let newer = minimal_iteration(RunIterationId(Uuid::new_v4()));
        let state = minimal_run_state(vec![older.clone(), newer]);

        let snapshot = build_snapshot(
            &state,
            SnapshotIterationSelector::ExactIteration(older.iteration_id),
        )
        .expect("snapshot should build for exact iteration");

        assert_snapshot_iteration(&snapshot, older.iteration_id);
    }

    #[test]
    fn builds_continuation_snapshot_with_previous_snapshot_and_oe_output() {
        let initial = minimal_iteration(RunIterationId(Uuid::new_v4()));
        let resolved = "memory is stable on all nodes";
        let continuation = minimal_continuation_iteration(RunIterationId(Uuid::new_v4()), resolved);
        let state = minimal_run_state(vec![initial.clone(), continuation.clone()]);

        let snapshot = build_snapshot(
            &state,
            SnapshotIterationSelector::ExactIteration(continuation.iteration_id),
        )
        .expect("continuation snapshot should build");

        assert_eq!(snapshot.iteration_kind, IterationProfile::Continuation);
        assert_eq!(snapshot.iteration_id, continuation.iteration_id);

        let oe = snapshot.observation_extraction_output.as_ref().expect("oe must be present");
        assert_eq!(oe.resolved_observation.text, resolved);
        assert_eq!(oe.observations.len(), 1);

        let prev = snapshot.previous_snapshot.as_ref().expect("previous snapshot must be present");
        assert_eq!(prev.iteration_id, initial.iteration_id);
        assert_eq!(prev.iteration_kind, IterationProfile::Initial);
        assert!(prev.previous_snapshot.is_none());

        // query_structuring_output is inherited from the previous iteration
        assert_eq!(
            snapshot.query_structuring_output.structured_query.intent,
            "diagnose"
        );

        // prompt_context_assembly_output comes from DiagnosticUpdatePromptContextAssembly
        assert_eq!(snapshot.prompt_context_assembly_output.prompt, "update prompt");

        // token usage reflects only the current iteration's LLM call
        assert_eq!(
            snapshot.runtime_token_usage.query_structuring.token_usage.prompt_tokens,
            None
        );
        assert_eq!(
            snapshot
                .runtime_token_usage
                .llm_structured_generation
                .token_usage
                .prompt_tokens,
            Some(150)
        );
    }

    #[test]
    fn continuation_snapshot_fails_when_no_previous_completed_iteration() {
        let resolved = "something happened";
        let continuation = minimal_continuation_iteration(RunIterationId(Uuid::new_v4()), resolved);
        let state = minimal_run_state(vec![continuation.clone()]);

        let err = build_snapshot(
            &state,
            SnapshotIterationSelector::ExactIteration(continuation.iteration_id),
        )
        .expect_err("must fail without a previous iteration");

        assert!(matches!(
            err,
            SnapshotBuildError::NoPreviousCompletedIteration { .. }
        ));
    }

    #[test]
    fn errors_when_required_step_is_missing() {
        let mut iteration = minimal_iteration(RunIterationId(Uuid::new_v4()));
        iteration.step_records.retain(|record| {
            !matches!(
                record,
                StepRecord::Finished(FinishedStepRecord {
                    step: StepKind::PromptContextAssembly,
                    ..
                })
            )
        });
        let state = minimal_run_state(vec![iteration.clone()]);

        let err = build_snapshot(
            &state,
            SnapshotIterationSelector::LastCompletedIteration,
        )
        .expect_err("snapshot must fail");

        assert!(matches!(
            err,
            SnapshotBuildError::MissingRequiredStep {
                iteration_id,
                step: StepKind::PromptContextAssembly,
            } if iteration_id == iteration.iteration_id
        ));
    }

    #[test]
    fn errors_when_required_step_failed() {
        let iteration_id = RunIterationId(Uuid::new_v4());
        let mut iteration = minimal_iteration(iteration_id);
        iteration.step_records.retain(|record| {
            !matches!(
                record,
                StepRecord::Finished(FinishedStepRecord {
                    step: StepKind::TheoryEvidenceRetrieval,
                    ..
                })
            )
        });
        iteration
            .step_records
            .push(failed_step_record(StepKind::TheoryEvidenceRetrieval));
        let state = minimal_run_state(vec![iteration]);

        let err = build_snapshot(
            &state,
            SnapshotIterationSelector::LastCompletedIteration,
        )
        .expect_err("snapshot must fail");

        assert!(matches!(
            err,
            SnapshotBuildError::StepFailed {
                iteration_id: actual_iteration_id,
                step: StepKind::TheoryEvidenceRetrieval,
                ..
            } if actual_iteration_id == iteration_id
        ));
    }

    fn assert_snapshot_iteration(
        snapshot: &DiagnosticEvalIterationSnapshot,
        expected_iteration_id: RunIterationId,
    ) {
        assert_eq!(snapshot.iteration_id, expected_iteration_id);
        assert_eq!(snapshot.user_request.query, "why did the cluster stall?");
        assert_eq!(snapshot.prompt_context_assembly_output.prompt, "prompt");
        assert_eq!(
            snapshot
                .response_validation_and_normalization_output
                .response
                .first_check,
            "check raft logs"
        );
    }
}
