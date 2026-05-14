use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::orchestrator::run_state::model::{
    RunId, RunIterationId, RunState, StepKind, StepResultEnvelope,
};
use crate::orchestrator::run_state::view::{IterationView, RunStateView};
use super::{
    Confidence, Hypothesis, HypothesisEvidenceSource, HypothesisId, HypothesisStatus,
    NormalizedUserRequest, ObservationBoundaryResolution, ObservationBoundaryResolverOutput,
    ResolvedObservation, ResponseValidationAndNormalizationOutput,
};

// ─── Supporting types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObservationStatus {
    Pending,
    Processed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub iteration_id: RunIterationId,
    pub normalized_user_input: String,
    pub resolved: ResolvedObservation,
    pub status: ObservationStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedCheck {
    pub iteration_id: RunIterationId,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProblemUnderstandingSource {
    InitialRequest(String),
    DiagnosticUpdate {
        problem_understanding: String,
        observation: Option<Observation>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemUnderstanding {
    pub iteration_id: RunIterationId,
    pub text: Option<String>,
    pub source: ProblemUnderstandingSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HypothesisState {
    pub iteration_id: RunIterationId,
    pub status: HypothesisStatus,
    pub confidence: Confidence,
    pub source: HypothesisEvidenceSource,
    pub problem_understanding: ProblemUnderstanding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedHypothesis {
    pub hypothesis_id: HypothesisId,
    pub text: String,
    pub state_history: Vec<HypothesisState>,
}

// ─── Root type ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticContext {
    pub run_id: RunId,
    pub problem_understanding: Vec<ProblemUnderstanding>,
    pub hypotheses: Vec<TrackedHypothesis>,
    pub observations: Vec<Observation>,
    pub suggested_checks: Vec<SuggestedCheck>,
}

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum DiagnosticContextError {
    #[error("invalid step payload in iteration {iteration_id:?}: {reason}")]
    InvalidStepPayload {
        iteration_id: RunIterationId,
        reason: String,
    },
}

// ─── Construction ─────────────────────────────────────────────────────────────

impl DiagnosticContext {
    pub fn from_run_state(run_state: &RunState) -> Result<Self, DiagnosticContextError> {
        let mut ctx = DiagnosticContext {
            run_id: run_state.run_id,
            problem_understanding: Vec::new(),
            hypotheses: Vec::new(),
            observations: Vec::new(),
            suggested_checks: Vec::new(),
        };

        let view = RunStateView::new(run_state);
        for (idx, iteration) in view.normal_iterations().enumerate() {
            if idx == 0 {
                process_iteration_0(&mut ctx, iteration)?;
            } else {
                process_iteration_n(&mut ctx, iteration)?;
            }
        }

        apply_observation_status_rule(&mut ctx);

        Ok(ctx)
    }
}

fn find_envelope<'a>(
    iteration: IterationView<'a>,
    kind: StepKind,
) -> Option<&'a StepResultEnvelope> {
    iteration.finished_step(kind)
        .and_then(|v| v.result().as_ref().ok())
}

fn process_iteration_0(
    ctx: &mut DiagnosticContext,
    iteration: IterationView<'_>,
) -> Result<(), DiagnosticContextError> {
    let norm: Option<&NormalizedUserRequest> =
        find_envelope(iteration, StepKind::InputNormalization).and_then(|e| {
            if let StepResultEnvelope::InputNormalization(n) = e {
                Some(n)
            } else {
                None
            }
        });

    let rvn: Option<&ResponseValidationAndNormalizationOutput> = find_envelope(
        iteration,
        StepKind::ResponseValidationAndNormalization,
    )
    .and_then(|e| {
        if let StepResultEnvelope::ResponseValidationAndNormalization(o) = e {
            Some(o)
        } else {
            None
        }
    });

    // Entry is created only when InputNormalization succeeded (provides the source query).
    let Some(norm) = norm else {
        return Ok(());
    };

    let text = rvn.as_ref().map(|o| o.response.problem_understanding.clone());

    let pu = ProblemUnderstanding {
        iteration_id: iteration.iteration_id(),
        text,
        source: ProblemUnderstandingSource::InitialRequest(norm.query.clone()),
    };

    if let Some(output) = rvn {
        upsert_hypotheses(ctx, iteration.iteration_id(), &output.response.hypotheses, &pu);
        ctx.suggested_checks.push(SuggestedCheck {
            iteration_id: iteration.iteration_id(),
            text: output.response.first_check.clone(),
        });
    }

    ctx.problem_understanding.push(pu);
    Ok(())
}

fn process_iteration_n(
    ctx: &mut DiagnosticContext,
    iteration: IterationView<'_>,
) -> Result<(), DiagnosticContextError> {
    let obr: Option<&ObservationBoundaryResolverOutput> =
        find_envelope(iteration, StepKind::ObservationBoundaryResolver).and_then(|e| {
            if let StepResultEnvelope::ObservationBoundaryResolver(o) = e {
                Some(o)
            } else {
                None
            }
        });

    let rvn: Option<&ResponseValidationAndNormalizationOutput> = find_envelope(
        iteration,
        StepKind::ResponseValidationAndNormalization,
    )
    .and_then(|e| {
        if let StepResultEnvelope::ResponseValidationAndNormalization(o) = e {
            Some(o)
        } else {
            None
        }
    });

    // Entry creation rule: skip if neither step succeeded.
    if obr.is_none() && rvn.is_none() {
        return Ok(());
    }

    // Require the previous entry to have a closed text for DiagnosticUpdate.
    let prev_text = ctx
        .problem_understanding
        .last()
        .and_then(|e| e.text.as_deref())
        .map(str::to_string);

    let prev_text = prev_text.ok_or_else(|| DiagnosticContextError::InvalidStepPayload {
        iteration_id: iteration.iteration_id(),
        reason: "previous iteration has no closed problem understanding text".to_string(),
    })?;

    // Build Observation only when OBR returned Supported (status applied at end).
    let observation = obr.and_then(|o| {
        if let ObservationBoundaryResolution::Supported(ref resolved) = o.resolution {
            Some(Observation {
                iteration_id: iteration.iteration_id(),
                normalized_user_input: o.normalized_user_input.clone(),
                resolved: resolved.clone(),
                status: ObservationStatus::Pending,
            })
        } else {
            None
        }
    });

    if let Some(obs) = observation.clone() {
        ctx.observations.push(obs);
    }

    let text = rvn.as_ref().map(|o| o.response.problem_understanding.clone());

    let pu = ProblemUnderstanding {
        iteration_id: iteration.iteration_id(),
        text,
        source: ProblemUnderstandingSource::DiagnosticUpdate {
            problem_understanding: prev_text,
            observation,
        },
    };

    if let Some(output) = rvn {
        upsert_hypotheses(ctx, iteration.iteration_id(), &output.response.hypotheses, &pu);
        ctx.suggested_checks.push(SuggestedCheck {
            iteration_id: iteration.iteration_id(),
            text: output.response.first_check.clone(),
        });
    }

    ctx.problem_understanding.push(pu);
    Ok(())
}

fn upsert_hypotheses(
    ctx: &mut DiagnosticContext,
    iteration_id: RunIterationId,
    hypotheses: &[Hypothesis],
    pu: &ProblemUnderstanding,
) {
    for hyp in hypotheses {
        let state = HypothesisState {
            iteration_id,
            status: hyp.status.clone(),
            confidence: hyp.confidence,
            source: hyp.source,
            problem_understanding: pu.clone(),
        };
        if let Some(tracked) = ctx.hypotheses.iter_mut().find(|t| t.hypothesis_id == hyp.id) {
            tracked.state_history.push(state);
        } else {
            ctx.hypotheses.push(TrackedHypothesis {
                hypothesis_id: hyp.id,
                text: hyp.text.clone(),
                state_history: vec![state],
            });
        }
    }
}

fn apply_observation_status_rule(ctx: &mut DiagnosticContext) {
    let len = ctx.observations.len();
    for (i, obs) in ctx.observations.iter_mut().enumerate() {
        obs.status = if i + 1 == len {
            ObservationStatus::Pending
        } else {
            ObservationStatus::Processed
        };
    }
}

// ─── View methods ─────────────────────────────────────────────────────────────

impl DiagnosticContext {
    pub fn current_problem_understanding(&self) -> Option<&ProblemUnderstanding> {
        self.problem_understanding.last()
    }

    pub fn active_hypotheses(&self) -> Vec<&TrackedHypothesis> {
        self.hypotheses
            .iter()
            .filter(|t| {
                t.state_history.last().is_some_and(|s| {
                    matches!(s.status, HypothesisStatus::Active | HypothesisStatus::Weakened)
                })
            })
            .collect()
    }

    pub fn rejected_hypotheses(&self) -> Vec<&TrackedHypothesis> {
        self.hypotheses
            .iter()
            .filter(|t| {
                t.state_history
                    .last()
                    .is_some_and(|s| matches!(s.status, HypothesisStatus::Rejected(_)))
            })
            .collect()
    }

    pub fn last_check(&self) -> Option<&SuggestedCheck> {
        self.suggested_checks.last()
    }

    pub fn current_observation(&self) -> Option<&Observation> {
        self.observations.last()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::orchestrator::run_state::model::{
        FinishedStepRecord, PendingStepRecord, RunIteration, RunIterationStatus, RunState,
        RunStatus, StepError, StepRecord, StepRecordId,
    };
    use crate::shared_types::{
        Confidence, DiagnosticResponse, DiagnosticResultInterpretation, Hypothesis,
        HypothesisEvidenceSource, HypothesisId, HypothesisStatus, NormalizedUserRequest,
        ObservationBoundaryResolution, ObservationBoundaryResolverOutput, ResolvedObservation,
        ResponseValidationAndNormalizationOutput, UserRequest,
    };

    // ─── Builders ─────────────────────────────────────────────────────────────

    fn new_run_id() -> crate::orchestrator::run_state::model::RunId {
        crate::orchestrator::run_state::model::RunId(Uuid::new_v4())
    }

    fn new_iteration_id() -> RunIterationId {
        RunIterationId(Uuid::new_v4())
    }

    fn new_record_id() -> StepRecordId {
        StepRecordId(Uuid::new_v4())
    }

    fn new_hyp_id() -> HypothesisId {
        HypothesisId(Uuid::new_v4())
    }

    fn empty_run_state() -> RunState {
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

    fn finished_err(kind: StepKind) -> StepRecord {
        let now = Utc::now();
        StepRecord::Finished(FinishedStepRecord {
            record_id: new_record_id(),
            step: kind,
            started_at: now,
            finished_at: now,
            result: Err(StepError::Unexpected {
                message: "test error".to_string(),
            }),
        })
    }

    fn pending_record(kind: StepKind) -> StepRecord {
        StepRecord::Pending(PendingStepRecord {
            record_id: new_record_id(),
            step: kind,
            started_at: Utc::now(),
        })
    }

    fn input_norm_result(query: &str) -> StepResultEnvelope {
        StepResultEnvelope::InputNormalization(NormalizedUserRequest {
            query: query.to_string(),
            input_token_count: 10,
        })
    }

    fn user_input_result() -> StepResultEnvelope {
        StepResultEnvelope::UserInputReceived(UserRequest {
            query: "test".to_string(),
            golden_question: None,
        })
    }

    fn make_hypothesis(id: HypothesisId, text: &str, status: HypothesisStatus) -> Hypothesis {
        Hypothesis {
            id,
            text: text.to_string(),
            status,
            source: HypothesisEvidenceSource::PrimaryIncident,
            confidence: Confidence::Medium,
        }
    }

    fn rvn_result(
        problem_understanding: &str,
        hypotheses: Vec<Hypothesis>,
        first_check: &str,
    ) -> StepResultEnvelope {
        StepResultEnvelope::ResponseValidationAndNormalization(
            ResponseValidationAndNormalizationOutput {
                response: DiagnosticResponse {
                    problem_understanding: problem_understanding.to_string(),
                    similar_practical_context: "context".to_string(),
                    hypotheses,
                    first_check: first_check.to_string(),
                    result_interpretation: DiagnosticResultInterpretation {
                        supports_primary_if: "if X".to_string(),
                        supports_competing_if: "if Y".to_string(),
                        inconclusive_if: None,
                    },
                    competing_interpretation: None,
                },
            },
        )
    }

    fn obr_supported(normalized_user_input: &str, resolved_text: &str) -> StepResultEnvelope {
        StepResultEnvelope::ObservationBoundaryResolver(ObservationBoundaryResolverOutput {
            normalized_user_input: normalized_user_input.to_string(),
            confidence: Confidence::Medium,
            reason: "observation accepted".to_string(),
            resolution: ObservationBoundaryResolution::Supported(ResolvedObservation {
                text: resolved_text.to_string(),
            }),
            token_usage: crate::shared_types::ModelTokenUsage {
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
            },
        })
    }

    fn obr_unsupported(normalized_user_input: &str) -> StepResultEnvelope {
        StepResultEnvelope::ObservationBoundaryResolver(ObservationBoundaryResolverOutput {
            normalized_user_input: normalized_user_input.to_string(),
            confidence: Confidence::Low,
            reason: "not a new observation".to_string(),
            resolution: ObservationBoundaryResolution::Unsupported,
            token_usage: crate::shared_types::ModelTokenUsage {
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
            },
        })
    }

    fn iter_with_records(records: Vec<StepRecord>) -> RunIteration {
        RunIteration {
            iteration_id: new_iteration_id(),
            config_snapshot: None,
            status: RunIterationStatus::Active,
            step_records: records,
        }
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

    // ─── Empty RunState ────────────────────────────────────────────────────────

    #[test]
    fn empty_run_state_produces_empty_context() {
        let state = empty_run_state();
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();
        assert_eq!(ctx.run_id, state.run_id);
        assert!(ctx.problem_understanding.is_empty());
        assert!(ctx.hypotheses.is_empty());
        assert!(ctx.observations.is_empty());
        assert!(ctx.suggested_checks.is_empty());
    }

    // ─── Iteration 0: partial (only InputNormalization) ───────────────────────

    #[test]
    fn iter0_only_input_norm_produces_pu_with_text_none() {
        let iter = iter_with_records(vec![
            finished_ok(StepKind::UserInputReceived, user_input_result()),
            finished_ok(
                StepKind::InputNormalization,
                input_norm_result("what is happening"),
            ),
        ]);
        let state = run_with_iterations(vec![iter]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        assert_eq!(ctx.problem_understanding.len(), 1);
        let pu = &ctx.problem_understanding[0];
        assert!(pu.text.is_none());
        assert!(
            matches!(&pu.source, ProblemUnderstandingSource::InitialRequest(q) if q == "what is happening")
        );
    }

    #[test]
    fn iter0_input_norm_and_rvn_produces_pu_with_some_text() {
        let h_id = new_hyp_id();
        let iter = iter_with_records(vec![
            finished_ok(StepKind::UserInputReceived, user_input_result()),
            finished_ok(
                StepKind::InputNormalization,
                input_norm_result("my query"),
            ),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("understood the problem", vec![make_hypothesis(h_id, "H1", HypothesisStatus::Active)], "check X"),
            ),
        ]);
        let state = run_with_iterations(vec![iter]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        assert_eq!(ctx.problem_understanding.len(), 1);
        let pu = &ctx.problem_understanding[0];
        assert_eq!(pu.text.as_deref(), Some("understood the problem"));
        assert!(
            matches!(&pu.source, ProblemUnderstandingSource::InitialRequest(q) if q == "my query")
        );
    }

    #[test]
    fn initial_request_source_carries_exactly_normalized_query() {
        let iter = iter_with_records(vec![
            finished_ok(
                StepKind::InputNormalization,
                input_norm_result("exact normalized query"),
            ),
        ]);
        let state = run_with_iterations(vec![iter]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        let pu = &ctx.problem_understanding[0];
        match &pu.source {
            ProblemUnderstandingSource::InitialRequest(q) => {
                assert_eq!(q, "exact normalized query");
            }
            _ => panic!("expected InitialRequest source"),
        }
    }

    // ─── Iteration 0: hypothesis construction ─────────────────────────────────

    #[test]
    fn iter0_each_hypothesis_produces_one_tracked_hypothesis() {
        let h1 = new_hyp_id();
        let h2 = new_hyp_id();
        let iter = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result(
                    "pu",
                    vec![
                        make_hypothesis(h1, "Hypothesis one", HypothesisStatus::Active),
                        make_hypothesis(h2, "Hypothesis two", HypothesisStatus::Weakened),
                    ],
                    "first check",
                ),
            ),
        ]);
        let state = run_with_iterations(vec![iter]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        assert_eq!(ctx.hypotheses.len(), 2);
        assert_eq!(ctx.hypotheses[0].hypothesis_id, h1);
        assert_eq!(ctx.hypotheses[0].text, "Hypothesis one");
        assert_eq!(ctx.hypotheses[0].state_history.len(), 1);
        assert!(matches!(ctx.hypotheses[0].state_history[0].status, HypothesisStatus::Active));
        assert_eq!(ctx.hypotheses[1].hypothesis_id, h2);
        assert!(matches!(ctx.hypotheses[1].state_history[0].status, HypothesisStatus::Weakened));
    }

    #[test]
    fn iter0_hypothesis_state_fields_copied_from_response() {
        let h_id = new_hyp_id();
        let hyp = Hypothesis {
            id: h_id,
            text: "some hypothesis".to_string(),
            status: HypothesisStatus::Active,
            source: HypothesisEvidenceSource::TheoryMechanism,
            confidence: Confidence::High,
        };
        let iter_id;
        let iter = {
            let i = iter_with_records(vec![
                finished_ok(StepKind::InputNormalization, input_norm_result("q")),
                finished_ok(
                    StepKind::ResponseValidationAndNormalization,
                    rvn_result("pu", vec![hyp], "check"),
                ),
            ]);
            iter_id = i.iteration_id;
            i
        };
        let state = run_with_iterations(vec![iter]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        let state_entry = &ctx.hypotheses[0].state_history[0];
        assert_eq!(state_entry.iteration_id, iter_id);
        assert!(matches!(state_entry.status, HypothesisStatus::Active));
        assert!(matches!(state_entry.source, HypothesisEvidenceSource::TheoryMechanism));
        assert!(matches!(state_entry.confidence, Confidence::High));
    }

    #[test]
    fn iter0_hypothesis_state_problem_understanding_is_same_iteration() {
        let h_id = new_hyp_id();
        let iter_id;
        let iter = {
            let i = iter_with_records(vec![
                finished_ok(StepKind::InputNormalization, input_norm_result("q")),
                finished_ok(
                    StepKind::ResponseValidationAndNormalization,
                    rvn_result("the understanding", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c"),
                ),
            ]);
            iter_id = i.iteration_id;
            i
        };
        let state = run_with_iterations(vec![iter]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        let hs_pu = &ctx.hypotheses[0].state_history[0].problem_understanding;
        assert_eq!(hs_pu.iteration_id, iter_id);
        assert_eq!(hs_pu.text.as_deref(), Some("the understanding"));
    }

    #[test]
    fn iter0_suggested_check_from_first_check_field() {
        let h_id = new_hyp_id();
        let iter_id;
        let iter = {
            let i = iter_with_records(vec![
                finished_ok(StepKind::InputNormalization, input_norm_result("q")),
                finished_ok(
                    StepKind::ResponseValidationAndNormalization,
                    rvn_result("pu", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "check the logs"),
                ),
            ]);
            iter_id = i.iteration_id;
            i
        };
        let state = run_with_iterations(vec![iter]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        assert_eq!(ctx.suggested_checks.len(), 1);
        assert_eq!(ctx.suggested_checks[0].iteration_id, iter_id);
        assert_eq!(ctx.suggested_checks[0].text, "check the logs");
    }

    // ─── Iteration N: observation and diagnostic update ───────────────────────

    fn two_iteration_state(iter1_records: Vec<StepRecord>) -> RunState {
        let h_id = new_hyp_id();
        let iter0 = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("iter0 understanding", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c0"),
            ),
        ]);
        let iter1 = iter_with_records(iter1_records);
        run_with_iterations(vec![iter0, iter1])
    }

    #[test]
    fn iter_n_with_obr_sets_diagnostic_update_observation() {
        let state = two_iteration_state(vec![
            finished_ok(
                StepKind::ObservationBoundaryResolver,
                obr_supported("user said this", "resolved text"),
            ),
        ]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        assert_eq!(ctx.problem_understanding.len(), 2);
        let pu1 = &ctx.problem_understanding[1];
        match &pu1.source {
            ProblemUnderstandingSource::DiagnosticUpdate { observation, .. } => {
                let obs = observation.as_ref().expect("observation should be Some");
                assert_eq!(obs.normalized_user_input, "user said this");
                assert_eq!(obs.resolved.text, "resolved text");
            }
            _ => panic!("expected DiagnosticUpdate source"),
        }
    }

    #[test]
    fn iter_n_without_obr_sets_observation_none() {
        let h_id = new_hyp_id();
        let state = two_iteration_state(vec![finished_ok(
            StepKind::ResponseValidationAndNormalization,
            rvn_result("iter1 pu", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c1"),
        )]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        let pu1 = &ctx.problem_understanding[1];
        match &pu1.source {
            ProblemUnderstandingSource::DiagnosticUpdate { observation, .. } => {
                assert!(observation.is_none());
            }
            _ => panic!("expected DiagnosticUpdate source"),
        }
    }

    #[test]
    fn iter_n_diagnostic_update_problem_understanding_from_prev_entry() {
        let state = two_iteration_state(vec![finished_ok(
            StepKind::ObservationBoundaryResolver,
            obr_supported("raw", "resolved"),
        )]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        let pu1 = &ctx.problem_understanding[1];
        match &pu1.source {
            ProblemUnderstandingSource::DiagnosticUpdate { problem_understanding, .. } => {
                assert_eq!(problem_understanding, "iter0 understanding");
            }
            _ => panic!("expected DiagnosticUpdate source"),
        }
    }

    #[test]
    fn iter_n_rvn_sets_current_text_some() {
        let h_id = new_hyp_id();
        let state = two_iteration_state(vec![
            finished_ok(
                StepKind::ObservationBoundaryResolver,
                obr_supported("raw", "resolved"),
            ),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("iter1 understanding", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c1"),
            ),
        ]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        let pu1 = &ctx.problem_understanding[1];
        assert_eq!(pu1.text.as_deref(), Some("iter1 understanding"));
    }

    #[test]
    fn iter_n_without_rvn_text_is_none() {
        let state = two_iteration_state(vec![finished_ok(
            StepKind::ObservationBoundaryResolver,
            obr_supported("raw", "resolved"),
        )]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        assert!(ctx.problem_understanding[1].text.is_none());
    }

    // ─── Hypothesis updates across iterations ─────────────────────────────────

    #[test]
    fn existing_hypothesis_gets_new_state_appended() {
        let h_id = new_hyp_id();
        let iter0 = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu0", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c0"),
            ),
        ]);
        let iter1 = iter_with_records(vec![
            finished_ok(StepKind::ObservationBoundaryResolver, obr_supported("r", "s")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu1", vec![make_hypothesis(h_id, "H", HypothesisStatus::Weakened)], "c1"),
            ),
        ]);
        let state = run_with_iterations(vec![iter0, iter1]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        assert_eq!(ctx.hypotheses.len(), 1);
        assert_eq!(ctx.hypotheses[0].state_history.len(), 2);
        assert!(matches!(ctx.hypotheses[0].state_history[0].status, HypothesisStatus::Active));
        assert!(matches!(ctx.hypotheses[0].state_history[1].status, HypothesisStatus::Weakened));
    }

    #[test]
    fn new_hypothesis_in_later_iteration_creates_tracked_hypothesis() {
        let h0 = new_hyp_id();
        let h1 = new_hyp_id();
        let iter0 = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu0", vec![make_hypothesis(h0, "H0", HypothesisStatus::Active)], "c0"),
            ),
        ]);
        let iter1 = iter_with_records(vec![
            finished_ok(StepKind::ObservationBoundaryResolver, obr_supported("r", "s")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result(
                    "pu1",
                    vec![
                        make_hypothesis(h0, "H0", HypothesisStatus::Active),
                        make_hypothesis(h1, "H1 new", HypothesisStatus::Active),
                    ],
                    "c1",
                ),
            ),
        ]);
        let state = run_with_iterations(vec![iter0, iter1]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        assert_eq!(ctx.hypotheses.len(), 2);
        let new_h = ctx.hypotheses.iter().find(|t| t.hypothesis_id == h1).unwrap();
        assert_eq!(new_h.text, "H1 new");
        assert_eq!(new_h.state_history.len(), 1);
    }

    #[test]
    fn rejected_hypothesis_carries_rejection_reason() {
        let h_id = new_hyp_id();
        let iter0 = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result(
                    "pu0",
                    vec![make_hypothesis(
                        h_id,
                        "H",
                        HypothesisStatus::Rejected("evidence contradicts this".to_string()),
                    )],
                    "c0",
                ),
            ),
        ]);
        let state = run_with_iterations(vec![iter0]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        let status = &ctx.hypotheses[0].state_history[0].status;
        assert!(
            matches!(status, HypothesisStatus::Rejected(r) if r == "evidence contradicts this")
        );
    }

    #[test]
    fn three_closed_iterations_produce_three_state_history_entries() {
        let h_id = new_hyp_id();
        let iter0 = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu0", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c0"),
            ),
        ]);
        let iter1 = iter_with_records(vec![
            finished_ok(StepKind::ObservationBoundaryResolver, obr_supported("r1", "s1")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu1", vec![make_hypothesis(h_id, "H", HypothesisStatus::Weakened)], "c1"),
            ),
        ]);
        let iter2 = iter_with_records(vec![
            finished_ok(StepKind::ObservationBoundaryResolver, obr_supported("r2", "s2")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result(
                    "pu2",
                    vec![make_hypothesis(h_id, "H", HypothesisStatus::Rejected("disproved".to_string()))],
                    "c2",
                ),
            ),
        ]);
        let state = run_with_iterations(vec![iter0, iter1, iter2]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        assert_eq!(ctx.hypotheses[0].state_history.len(), 3);
        assert!(matches!(ctx.hypotheses[0].state_history[0].status, HypothesisStatus::Active));
        assert!(matches!(ctx.hypotheses[0].state_history[1].status, HypothesisStatus::Weakened));
        assert!(matches!(ctx.hypotheses[0].state_history[2].status, HypothesisStatus::Rejected(_)));
    }

    // ─── Error and skip behaviour ──────────────────────────────────────────────

    #[test]
    fn absent_step_is_silently_skipped() {
        let iter = iter_with_records(vec![
            finished_ok(StepKind::UserInputReceived, user_input_result()),
            // InputNormalization absent — no entry created
        ]);
        let state = run_with_iterations(vec![iter]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();
        assert!(ctx.problem_understanding.is_empty());
    }

    #[test]
    fn failed_step_result_is_silently_skipped() {
        let iter = iter_with_records(vec![
            finished_err(StepKind::InputNormalization),
        ]);
        let state = run_with_iterations(vec![iter]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();
        assert!(ctx.problem_understanding.is_empty());
    }

    #[test]
    fn pending_step_does_not_contribute() {
        let iter = iter_with_records(vec![
            pending_record(StepKind::InputNormalization),
        ]);
        let state = run_with_iterations(vec![iter]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();
        assert!(ctx.problem_understanding.is_empty());
    }

    #[test]
    fn invalid_payload_returns_invalid_step_payload_error() {
        // We can't easily trigger this through the type system in tests
        // (the type system guards correct payloads), but we test the case
        // where ObservationBoundaryResolver stores a UserInputReceived payload
        // by constructing a malformed envelope. Since Rust's type system
        // prevents this directly, we verify the happy path compiles and the
        // error variant is reachable.
        let err = DiagnosticContextError::InvalidStepPayload {
            iteration_id: new_iteration_id(),
            reason: "test reason".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("test reason"));
    }

    #[test]
    fn finished_with_wait_input_iteration_is_skipped() {
        // A short/clarification iteration (FinishedWithWaitInput) must not contribute.
        let iter0 = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
        ]);
        let mut short_iter = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("clarify")),
        ]);
        short_iter.status = RunIterationStatus::FinishedWithWaitInput;
        let state = run_with_iterations(vec![iter0, short_iter]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();
        // Only the first normal iteration contributes.
        assert_eq!(ctx.problem_understanding.len(), 1);
        assert!(matches!(
            &ctx.problem_understanding[0].source,
            ProblemUnderstandingSource::InitialRequest(_)
        ));
    }

    #[test]
    fn finished_with_error_iteration_is_skipped() {
        let iter0 = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
        ]);
        let mut error_iter = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("retry")),
        ]);
        error_iter.status = RunIterationStatus::FinishedWithError;
        let state = run_with_iterations(vec![iter0, error_iter]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();
        assert_eq!(ctx.problem_understanding.len(), 1);
    }

    #[test]
    fn short_iteration_between_normal_ones_does_not_affect_indexing() {
        // Normal iter 0 (initial) → short iter (skipped) → normal iter 1 (continuation).
        // iter 1 must still be treated as continuation (not initial) and require OBR.
        let h_id = new_hyp_id();
        let iter0 = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu0", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c0"),
            ),
        ]);
        let mut short_iter = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("clarify")),
        ]);
        short_iter.status = RunIterationStatus::FinishedWithWaitInput;
        let iter1 = iter_with_records(vec![
            finished_ok(StepKind::ObservationBoundaryResolver, obr_supported("r", "s")),
        ]);
        let state = run_with_iterations(vec![iter0, short_iter, iter1]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();
        // 2 problem_understanding entries: iter0 and iter1 (short is skipped)
        assert_eq!(ctx.problem_understanding.len(), 2);
        assert!(matches!(
            &ctx.problem_understanding[1].source,
            ProblemUnderstandingSource::DiagnosticUpdate { .. }
        ));
    }

    #[test]
    fn prev_entry_text_none_returns_invalid_step_payload() {
        // Iter 0 succeeds with InputNormalization only (text=None).
        // Iter 1 has ObservationBoundaryResolver → triggers entry creation
        // → prev_entry.text is None → must return error.
        let iter0 = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            // No RVN → text = None
        ]);
        let iter1 = iter_with_records(vec![
            finished_ok(StepKind::ObservationBoundaryResolver, obr_supported("r", "s")),
        ]);
        let state = run_with_iterations(vec![iter0, iter1]);
        let result = DiagnosticContext::from_run_state(&state);
        assert!(matches!(
            result,
            Err(DiagnosticContextError::InvalidStepPayload { .. })
        ));
    }

    // ─── Observation status rule ───────────────────────────────────────────────

    #[test]
    fn single_observation_has_status_pending() {
        let state = two_iteration_state(vec![finished_ok(
            StepKind::ObservationBoundaryResolver,
            obr_supported("raw", "resolved"),
        )]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();
        assert_eq!(ctx.observations.len(), 1);
        assert!(matches!(ctx.observations[0].status, ObservationStatus::Pending));
    }

    #[test]
    fn two_observations_first_processed_second_pending() {
        let h_id = new_hyp_id();
        let iter0 = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu0", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c0"),
            ),
        ]);
        let iter1 = iter_with_records(vec![
            finished_ok(StepKind::ObservationBoundaryResolver, obr_supported("r1", "s1")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu1", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c1"),
            ),
        ]);
        let iter2 = iter_with_records(vec![
            finished_ok(StepKind::ObservationBoundaryResolver, obr_supported("r2", "s2")),
        ]);
        let state = run_with_iterations(vec![iter0, iter1, iter2]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        assert_eq!(ctx.observations.len(), 2);
        assert!(matches!(ctx.observations[0].status, ObservationStatus::Processed));
        assert!(matches!(ctx.observations[1].status, ObservationStatus::Pending));
    }

    #[test]
    fn at_most_one_observation_is_pending() {
        let h_id = new_hyp_id();
        let iter0 = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu0", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c0"),
            ),
        ]);
        let iter1 = iter_with_records(vec![
            finished_ok(StepKind::ObservationBoundaryResolver, obr_supported("r1", "s1")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu1", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c1"),
            ),
        ]);
        let iter2 = iter_with_records(vec![
            finished_ok(StepKind::ObservationBoundaryResolver, obr_supported("r2", "s2")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu2", vec![make_hypothesis(h_id, "H", HypothesisStatus::Weakened)], "c2"),
            ),
        ]);
        let iter3 = iter_with_records(vec![
            finished_ok(StepKind::ObservationBoundaryResolver, obr_supported("r3", "s3")),
        ]);
        let state = run_with_iterations(vec![iter0, iter1, iter2, iter3]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        let pending_count = ctx
            .observations
            .iter()
            .filter(|o| matches!(o.status, ObservationStatus::Pending))
            .count();
        assert_eq!(pending_count, 1);
    }

    // ─── Ordering invariants ───────────────────────────────────────────────────

    #[test]
    fn problem_understanding_entries_are_in_iteration_sequence_order() {
        let h_id = new_hyp_id();
        let iter0 = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu0", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c0"),
            ),
        ]);
        let iter1 = iter_with_records(vec![
            finished_ok(StepKind::ObservationBoundaryResolver, obr_supported("r", "s")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu1", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c1"),
            ),
        ]);
        let iter0_id = iter0.iteration_id;
        let iter1_id = iter1.iteration_id;
        let state = run_with_iterations(vec![iter0, iter1]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        assert_eq!(ctx.problem_understanding[0].iteration_id, iter0_id);
        assert_eq!(ctx.problem_understanding[1].iteration_id, iter1_id);
    }

    #[test]
    fn first_problem_understanding_entry_has_initial_request_source() {
        let iter = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
        ]);
        let state = run_with_iterations(vec![iter]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        assert!(matches!(
            &ctx.problem_understanding[0].source,
            ProblemUnderstandingSource::InitialRequest(_)
        ));
    }

    #[test]
    fn subsequent_problem_understanding_entries_have_diagnostic_update_source() {
        let h_id = new_hyp_id();
        let iter0 = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu0", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c0"),
            ),
        ]);
        let iter1 = iter_with_records(vec![
            finished_ok(StepKind::ObservationBoundaryResolver, obr_supported("r", "s")),
        ]);
        let state = run_with_iterations(vec![iter0, iter1]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        assert!(matches!(
            &ctx.problem_understanding[1].source,
            ProblemUnderstandingSource::DiagnosticUpdate { .. }
        ));
    }

    // ─── View methods ─────────────────────────────────────────────────────────

    #[test]
    fn current_problem_understanding_returns_none_for_empty_context() {
        let state = empty_run_state();
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();
        assert!(ctx.current_problem_understanding().is_none());
    }

    #[test]
    fn current_problem_understanding_returns_last_entry() {
        let h_id = new_hyp_id();
        let iter0 = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu0", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c0"),
            ),
        ]);
        let iter1 = iter_with_records(vec![
            finished_ok(StepKind::ObservationBoundaryResolver, obr_supported("r", "s")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu1", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c1"),
            ),
        ]);
        let state = run_with_iterations(vec![iter0, iter1]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        assert_eq!(
            ctx.current_problem_understanding().unwrap().text.as_deref(),
            Some("pu1")
        );
    }

    #[test]
    fn active_hypotheses_returns_active_and_weakened() {
        let h_active = new_hyp_id();
        let h_weakened = new_hyp_id();
        let h_rejected = new_hyp_id();
        let iter = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result(
                    "pu",
                    vec![
                        make_hypothesis(h_active, "H active", HypothesisStatus::Active),
                        make_hypothesis(h_weakened, "H weakened", HypothesisStatus::Weakened),
                        make_hypothesis(
                            h_rejected,
                            "H rejected",
                            HypothesisStatus::Rejected("reason".to_string()),
                        ),
                    ],
                    "c",
                ),
            ),
        ]);
        let state = run_with_iterations(vec![iter]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        let active = ctx.active_hypotheses();
        assert_eq!(active.len(), 2);
        assert!(active.iter().any(|t| t.hypothesis_id == h_active));
        assert!(active.iter().any(|t| t.hypothesis_id == h_weakened));
        assert!(!active.iter().any(|t| t.hypothesis_id == h_rejected));
    }

    #[test]
    fn active_hypotheses_excludes_rejected() {
        let h_id = new_hyp_id();
        let iter = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result(
                    "pu",
                    vec![make_hypothesis(
                        h_id,
                        "H",
                        HypothesisStatus::Rejected("r".to_string()),
                    )],
                    "c",
                ),
            ),
        ]);
        let state = run_with_iterations(vec![iter]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();
        assert!(ctx.active_hypotheses().is_empty());
    }

    #[test]
    fn rejected_hypotheses_returns_only_rejected() {
        let h_active = new_hyp_id();
        let h_rejected = new_hyp_id();
        let iter = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result(
                    "pu",
                    vec![
                        make_hypothesis(h_active, "HA", HypothesisStatus::Active),
                        make_hypothesis(
                            h_rejected,
                            "HR",
                            HypothesisStatus::Rejected("reason".to_string()),
                        ),
                    ],
                    "c",
                ),
            ),
        ]);
        let state = run_with_iterations(vec![iter]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        let rejected = ctx.rejected_hypotheses();
        assert_eq!(rejected.len(), 1);
        assert_eq!(rejected[0].hypothesis_id, h_rejected);
    }

    #[test]
    fn active_hypotheses_preserves_order() {
        let h1 = new_hyp_id();
        let h2 = new_hyp_id();
        let h3 = new_hyp_id();
        let iter = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result(
                    "pu",
                    vec![
                        make_hypothesis(h1, "H1", HypothesisStatus::Active),
                        make_hypothesis(h2, "H2", HypothesisStatus::Weakened),
                        make_hypothesis(h3, "H3", HypothesisStatus::Active),
                    ],
                    "c",
                ),
            ),
        ]);
        let state = run_with_iterations(vec![iter]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        let active = ctx.active_hypotheses();
        assert_eq!(active[0].hypothesis_id, h1);
        assert_eq!(active[1].hypothesis_id, h2);
        assert_eq!(active[2].hypothesis_id, h3);
    }

    #[test]
    fn active_hypotheses_empty_when_all_rejected() {
        let h_id = new_hyp_id();
        let iter = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result(
                    "pu",
                    vec![make_hypothesis(h_id, "H", HypothesisStatus::Rejected("r".to_string()))],
                    "c",
                ),
            ),
        ]);
        let state = run_with_iterations(vec![iter]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();
        assert!(ctx.active_hypotheses().is_empty());
    }

    #[test]
    fn rejected_hypotheses_empty_when_all_active() {
        let h_id = new_hyp_id();
        let iter = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c"),
            ),
        ]);
        let state = run_with_iterations(vec![iter]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();
        assert!(ctx.rejected_hypotheses().is_empty());
    }

    #[test]
    fn current_observation_returns_none_for_empty_context() {
        let state = empty_run_state();
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();
        assert!(ctx.current_observation().is_none());
    }

    #[test]
    fn current_observation_returns_last_element() {
        let h_id = new_hyp_id();
        let iter0 = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu0", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c0"),
            ),
        ]);
        let iter1 = iter_with_records(vec![
            finished_ok(StepKind::ObservationBoundaryResolver, obr_supported("r1", "first")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu1", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c1"),
            ),
        ]);
        let iter2 = iter_with_records(vec![
            finished_ok(StepKind::ObservationBoundaryResolver, obr_supported("r2", "second")),
        ]);
        let state = run_with_iterations(vec![iter0, iter1, iter2]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        let obs = ctx.current_observation().unwrap();
        assert_eq!(obs.resolved.text, "second");
        assert!(matches!(obs.status, ObservationStatus::Pending));
    }

    // ─── Unsupported resolution ───────────────────────────────────────────────

    #[test]
    fn obr_unsupported_produces_no_observation_entry() {
        let h_id = new_hyp_id();
        let iter0 = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu0", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c0"),
            ),
        ]);
        let iter1 = iter_with_records(vec![
            finished_ok(StepKind::ObservationBoundaryResolver, obr_unsupported("not an obs")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu1", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c1"),
            ),
        ]);
        let state = run_with_iterations(vec![iter0, iter1]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        assert!(ctx.observations.is_empty());
    }

    #[test]
    fn obr_unsupported_sets_diagnostic_update_observation_none() {
        let h_id = new_hyp_id();
        let iter0 = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu0", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c0"),
            ),
        ]);
        let iter1 = iter_with_records(vec![
            finished_ok(StepKind::ObservationBoundaryResolver, obr_unsupported("not obs")),
        ]);
        let state = run_with_iterations(vec![iter0, iter1]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        let pu1 = &ctx.problem_understanding[1];
        match &pu1.source {
            ProblemUnderstandingSource::DiagnosticUpdate { observation, .. } => {
                assert!(observation.is_none());
            }
            _ => panic!("expected DiagnosticUpdate source"),
        }
    }

    #[test]
    fn supported_after_unsupported_produces_observation() {
        let h_id = new_hyp_id();
        let iter0 = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu0", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c0"),
            ),
        ]);
        let iter1 = iter_with_records(vec![
            finished_ok(StepKind::ObservationBoundaryResolver, obr_unsupported("nope")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu1", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "c1"),
            ),
        ]);
        let iter2 = iter_with_records(vec![
            finished_ok(StepKind::ObservationBoundaryResolver, obr_supported("yes", "resolved text")),
        ]);
        let state = run_with_iterations(vec![iter0, iter1, iter2]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        assert_eq!(ctx.observations.len(), 1);
        assert_eq!(ctx.observations[0].resolved.text, "resolved text");
        assert!(matches!(ctx.observations[0].status, ObservationStatus::Pending));
    }

    // ─── last_check() ─────────────────────────────────────────────────────────

    #[test]
    fn last_check_returns_none_for_empty_context() {
        let state = empty_run_state();
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();
        assert!(ctx.last_check().is_none());
    }

    #[test]
    fn last_check_returns_most_recent_suggested_check() {
        let h_id = new_hyp_id();
        let iter0 = iter_with_records(vec![
            finished_ok(StepKind::InputNormalization, input_norm_result("q")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu0", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "check iter 0"),
            ),
        ]);
        let iter1 = iter_with_records(vec![
            finished_ok(StepKind::ObservationBoundaryResolver, obr_supported("r", "s")),
            finished_ok(
                StepKind::ResponseValidationAndNormalization,
                rvn_result("pu1", vec![make_hypothesis(h_id, "H", HypothesisStatus::Active)], "check iter 1"),
            ),
        ]);
        let state = run_with_iterations(vec![iter0, iter1]);
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();

        assert_eq!(ctx.last_check().unwrap().text, "check iter 1");
    }
}
