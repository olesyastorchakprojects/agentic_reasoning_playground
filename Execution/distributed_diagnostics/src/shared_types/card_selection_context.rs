use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::orchestrator::run_state::model::{
    RunIterationId, RunIterationStatus, RunState, StepKind, StepResultEnvelope,
};
use crate::orchestrator::run_state::view::RunStateView;
use super::PrimaryCardStatus;

// ─── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardSelectionContext {
    pub history: Vec<CardSelectionSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardSelectionSnapshot {
    pub iteration_id: RunIterationId,
    pub primary_card_id: String,
    pub primary_card_status: PrimaryCardStatus,
    pub alternative_card_ids: Vec<String>,
    pub challenger_card_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CardSelectionContextError {
    #[error("missing candidate-card retrieval result for initial iteration {iteration_id:?}")]
    MissingInitialCandidateCardRetrieval { iteration_id: RunIterationId },

    #[error("missing card-branch reranking result for iteration {iteration_id:?}")]
    MissingCardBranchReranking { iteration_id: RunIterationId },

    #[error("initial iteration {iteration_id:?} produced no primary candidate card")]
    MissingInitialPrimaryCard { iteration_id: RunIterationId },

    #[error("duplicate card id '{card_id}' across branches in iteration {iteration_id:?}")]
    DuplicateCardAcrossBranches {
        iteration_id: RunIterationId,
        card_id: String,
    },
}

// ─── Construction ─────────────────────────────────────────────────────────────

impl CardSelectionContext {
    pub fn from_run_state(run_state: &RunState) -> Result<Self, CardSelectionContextError> {
        let mut history = Vec::new();
        let view = RunStateView::new(run_state);

        for (idx, iteration) in view.normal_iterations().enumerate() {
            // Skip active continuation iterations: CardBranchReranking hasn't run yet in them.
            if idx > 0 && iteration.status() == RunIterationStatus::Active {
                continue;
            }
            let iteration_id = iteration.iteration_id();
            let snapshot = if idx == 0 {
                let envelope = find_envelope(iteration, StepKind::CandidateCardRetrieval)
                    .ok_or(CardSelectionContextError::MissingInitialCandidateCardRetrieval {
                        iteration_id,
                    })?;

                let output = match envelope {
                    StepResultEnvelope::CandidateCardRetrieval(o) => o,
                    _ => unreachable!(),
                };

                let primary = output.primary.as_ref().ok_or(
                    CardSelectionContextError::MissingInitialPrimaryCard { iteration_id },
                )?;

                let alternative_card_ids: Vec<String> =
                    output.alternatives.iter().map(|c| c.case_id.clone()).collect();

                check_duplicates(iteration_id, &primary.case_id, &alternative_card_ids, &[])?;

                CardSelectionSnapshot {
                    iteration_id,
                    primary_card_id: primary.case_id.clone(),
                    primary_card_status: PrimaryCardStatus::Tentative,
                    alternative_card_ids,
                    challenger_card_ids: vec![],
                }
            } else {
                let envelope = find_envelope(iteration, StepKind::CardBranchReranking)
                    .ok_or(CardSelectionContextError::MissingCardBranchReranking {
                        iteration_id,
                    })?;

                let output = match envelope {
                    StepResultEnvelope::CardBranchReranking(o) => o,
                    _ => unreachable!(),
                };

                check_duplicates(
                    iteration_id,
                    &output.primary_card_id,
                    &output.alternative_card_ids,
                    &output.challenger_card_ids,
                )?;

                CardSelectionSnapshot {
                    iteration_id,
                    primary_card_id: output.primary_card_id.clone(),
                    primary_card_status: output.primary_card_status,
                    alternative_card_ids: output.alternative_card_ids.clone(),
                    challenger_card_ids: output.challenger_card_ids.clone(),
                }
            };

            history.push(snapshot);
        }

        Ok(CardSelectionContext { history })
    }
}

// ─── Private helpers ──────────────────────────────────────────────────────────

fn find_envelope<'a>(
    iteration: crate::orchestrator::run_state::view::IterationView<'a>,
    kind: StepKind,
) -> Option<&'a StepResultEnvelope> {
    iteration.finished_step(kind)
        .and_then(|v| v.result().as_ref().ok())
}

fn check_duplicates(
    iteration_id: RunIterationId,
    primary: &str,
    alternatives: &[String],
    challengers: &[String],
) -> Result<(), CardSelectionContextError> {
    let mut seen = std::collections::HashSet::new();
    seen.insert(primary.to_string());
    for id in alternatives.iter().chain(challengers.iter()) {
        if !seen.insert(id.clone()) {
            return Err(CardSelectionContextError::DuplicateCardAcrossBranches {
                iteration_id,
                card_id: id.clone(),
            });
        }
    }
    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::run_state::model::{
        FinishedStepRecord, RunIteration, RunIterationStatus, RunState, RunStatus, StepError,
        StepRecord, StepRecordId,
    };
    use crate::shared_types::{CandidateCard, CandidateCardRetrievalOutput, CardBranchRerankingOutput, PrimaryCardStatus};
    use chrono::Utc;
    use uuid::Uuid;

    fn new_id() -> Uuid {
        Uuid::new_v4()
    }

    fn iter_id() -> RunIterationId {
        RunIterationId(new_id())
    }

    fn record_id() -> StepRecordId {
        StepRecordId(new_id())
    }

    fn finished_ok(step: StepKind, result: StepResultEnvelope) -> StepRecord {
        StepRecord::Finished(FinishedStepRecord {
            record_id: record_id(),
            step,
            started_at: Utc::now(),
            finished_at: Utc::now(),
            result: Ok(result),
        })
    }

    fn finished_err(step: StepKind) -> StepRecord {
        StepRecord::Finished(FinishedStepRecord {
            record_id: record_id(),
            step,
            started_at: Utc::now(),
            finished_at: Utc::now(),
            result: Err(StepError::Unexpected {
                message: "test error".to_string(),
            }),
        })
    }

    fn candidate(case_id: &str) -> CandidateCard {
        CandidateCard { case_id: case_id.to_string(), score: 0.9 }
    }

    fn retrieval_result(primary: Option<CandidateCard>, alternatives: Vec<CandidateCard>) -> StepResultEnvelope {
        let ranked_candidates: Vec<CandidateCard> = primary
            .iter()
            .cloned()
            .chain(alternatives.iter().cloned())
            .collect();
        StepResultEnvelope::CandidateCardRetrieval(CandidateCardRetrievalOutput {
            ranked_candidates,
            primary,
            alternatives,
            metrics: None,
        })
    }

    fn reranking_result(primary_card_id: &str, status: PrimaryCardStatus, alternatives: Vec<&str>, challengers: Vec<&str>) -> StepResultEnvelope {
        StepResultEnvelope::CardBranchReranking(CardBranchRerankingOutput {
            primary_card_id: primary_card_id.to_string(),
            primary_card_status: status,
            alternative_card_ids: alternatives.into_iter().map(|s| s.to_string()).collect(),
            challenger_card_ids: challengers.into_iter().map(|s| s.to_string()).collect(),
        })
    }

    fn empty_run_state() -> RunState {
        RunState {
            run_id: crate::orchestrator::run_state::model::RunId(new_id()),
            status: RunStatus::Active,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            revision: 0,
            iterations: vec![],
        }
    }

    fn run_state_with(iterations: Vec<RunIteration>) -> RunState {
        RunState { iterations, ..empty_run_state() }
    }

    fn iteration(id: RunIterationId, records: Vec<StepRecord>) -> RunIteration {
        RunIteration { iteration_id: id, config_snapshot: None, status: RunIterationStatus::Active, step_records: records }
    }

    fn finished_iteration(id: RunIterationId, records: Vec<StepRecord>) -> RunIteration {
        RunIteration { iteration_id: id, config_snapshot: None, status: RunIterationStatus::FinishedWithSuccess, step_records: records }
    }

    // ─── Empty state ──────────────────────────────────────────────────────────

    #[test]
    fn empty_run_state_returns_empty_history() {
        let state = empty_run_state();
        let ctx = CardSelectionContext::from_run_state(&state).unwrap();
        assert!(ctx.history.is_empty());
    }

    // ─── Iteration 0 ──────────────────────────────────────────────────────────

    #[test]
    fn iteration_0_projected_from_candidate_card_retrieval() {
        let iid = iter_id();
        let state = run_state_with(vec![iteration(iid, vec![
            finished_ok(StepKind::CandidateCardRetrieval, retrieval_result(
                Some(candidate("card-1")),
                vec![],
            )),
        ])]);
        let ctx = CardSelectionContext::from_run_state(&state).unwrap();
        assert_eq!(ctx.history.len(), 1);
        assert_eq!(ctx.history[0].primary_card_id, "card-1");
    }

    #[test]
    fn iteration_0_sets_status_tentative() {
        let iid = iter_id();
        let state = run_state_with(vec![iteration(iid, vec![
            finished_ok(StepKind::CandidateCardRetrieval, retrieval_result(
                Some(candidate("card-1")),
                vec![],
            )),
        ])]);
        let ctx = CardSelectionContext::from_run_state(&state).unwrap();
        assert_eq!(ctx.history[0].primary_card_status, PrimaryCardStatus::Tentative);
    }

    #[test]
    fn iteration_0_copies_primary_case_id() {
        let iid = iter_id();
        let state = run_state_with(vec![iteration(iid, vec![
            finished_ok(StepKind::CandidateCardRetrieval, retrieval_result(
                Some(candidate("my-card")),
                vec![],
            )),
        ])]);
        let ctx = CardSelectionContext::from_run_state(&state).unwrap();
        assert_eq!(ctx.history[0].primary_card_id, "my-card");
    }

    #[test]
    fn iteration_0_copies_alternatives_in_order() {
        let iid = iter_id();
        let state = run_state_with(vec![iteration(iid, vec![
            finished_ok(StepKind::CandidateCardRetrieval, retrieval_result(
                Some(candidate("p")),
                vec![candidate("a1"), candidate("a2")],
            )),
        ])]);
        let ctx = CardSelectionContext::from_run_state(&state).unwrap();
        assert_eq!(ctx.history[0].alternative_card_ids, vec!["a1", "a2"]);
    }

    #[test]
    fn iteration_0_sets_challenger_ids_empty() {
        let iid = iter_id();
        let state = run_state_with(vec![iteration(iid, vec![
            finished_ok(StepKind::CandidateCardRetrieval, retrieval_result(
                Some(candidate("p")),
                vec![candidate("a1")],
            )),
        ])]);
        let ctx = CardSelectionContext::from_run_state(&state).unwrap();
        assert!(ctx.history[0].challenger_card_ids.is_empty());
    }

    // ─── Later iterations ─────────────────────────────────────────────────────

    #[test]
    fn later_iterations_projected_from_card_branch_reranking() {
        let iid0 = iter_id();
        let iid1 = iter_id();
        let state = run_state_with(vec![
            iteration(iid0, vec![
                finished_ok(StepKind::CandidateCardRetrieval, retrieval_result(
                    Some(candidate("p")), vec![],
                )),
            ]),
            finished_iteration(iid1, vec![
                finished_ok(StepKind::CardBranchReranking, reranking_result(
                    "p2", PrimaryCardStatus::Tentative, vec!["a1"], vec![],
                )),
            ]),
        ]);
        let ctx = CardSelectionContext::from_run_state(&state).unwrap();
        assert_eq!(ctx.history.len(), 2);
        assert_eq!(ctx.history[1].primary_card_id, "p2");
    }

    #[test]
    fn later_iterations_copy_reranking_fields_exactly() {
        let iid0 = iter_id();
        let iid1 = iter_id();
        let state = run_state_with(vec![
            iteration(iid0, vec![
                finished_ok(StepKind::CandidateCardRetrieval, retrieval_result(
                    Some(candidate("p")), vec![],
                )),
            ]),
            finished_iteration(iid1, vec![
                finished_ok(StepKind::CardBranchReranking, reranking_result(
                    "sticky-p", PrimaryCardStatus::Sticky, vec!["alt-1", "alt-2"], vec!["ch-1"],
                )),
            ]),
        ]);
        let ctx = CardSelectionContext::from_run_state(&state).unwrap();
        let snap = &ctx.history[1];
        assert_eq!(snap.primary_card_id, "sticky-p");
        assert_eq!(snap.primary_card_status, PrimaryCardStatus::Sticky);
        assert_eq!(snap.alternative_card_ids, vec!["alt-1", "alt-2"]);
        assert_eq!(snap.challenger_card_ids, vec!["ch-1"]);
    }

    // ─── Error cases ──────────────────────────────────────────────────────────

    #[test]
    fn missing_candidate_retrieval_in_iteration_0_returns_error() {
        let iid = iter_id();
        let state = run_state_with(vec![iteration(iid, vec![])]);
        let err = CardSelectionContext::from_run_state(&state).unwrap_err();
        assert!(matches!(err, CardSelectionContextError::MissingInitialCandidateCardRetrieval { .. }));
    }

    #[test]
    fn failed_candidate_retrieval_in_iteration_0_returns_error() {
        let iid = iter_id();
        let state = run_state_with(vec![iteration(iid, vec![
            finished_err(StepKind::CandidateCardRetrieval),
        ])]);
        let err = CardSelectionContext::from_run_state(&state).unwrap_err();
        assert!(matches!(err, CardSelectionContextError::MissingInitialCandidateCardRetrieval { .. }));
    }

    #[test]
    fn no_primary_in_iteration_0_returns_missing_primary_error() {
        let iid = iter_id();
        let state = run_state_with(vec![iteration(iid, vec![
            finished_ok(StepKind::CandidateCardRetrieval, retrieval_result(None, vec![])),
        ])]);
        let err = CardSelectionContext::from_run_state(&state).unwrap_err();
        assert!(matches!(err, CardSelectionContextError::MissingInitialPrimaryCard { .. }));
    }

    #[test]
    fn missing_card_branch_reranking_in_later_iteration_returns_error() {
        // A FinishedWithSuccess iteration without CardBranchReranking → error.
        let iid0 = iter_id();
        let iid1 = iter_id();
        let state = run_state_with(vec![
            iteration(iid0, vec![
                finished_ok(StepKind::CandidateCardRetrieval, retrieval_result(
                    Some(candidate("p")), vec![],
                )),
            ]),
            finished_iteration(iid1, vec![]),
        ]);
        let err = CardSelectionContext::from_run_state(&state).unwrap_err();
        assert!(matches!(err, CardSelectionContextError::MissingCardBranchReranking { iteration_id } if iteration_id == iid1));
    }

    #[test]
    fn active_continuation_iteration_without_reranking_is_skipped() {
        // An Active iteration at idx > 0 without CardBranchReranking is skipped, not an error.
        let iid0 = iter_id();
        let state = run_state_with(vec![
            iteration(iid0, vec![
                finished_ok(StepKind::CandidateCardRetrieval, retrieval_result(
                    Some(candidate("p")), vec![],
                )),
            ]),
            iteration(iter_id(), vec![]),
        ]);
        let ctx = CardSelectionContext::from_run_state(&state).unwrap();
        assert_eq!(ctx.history.len(), 1);
        assert_eq!(ctx.history[0].primary_card_id, "p");
    }

    #[test]
    fn duplicate_card_across_branches_returns_error() {
        let iid0 = iter_id();
        let iid1 = iter_id();
        let state = run_state_with(vec![
            iteration(iid0, vec![
                finished_ok(StepKind::CandidateCardRetrieval, retrieval_result(
                    Some(candidate("p")), vec![],
                )),
            ]),
            finished_iteration(iid1, vec![
                finished_ok(StepKind::CardBranchReranking, reranking_result(
                    "p", PrimaryCardStatus::Sticky, vec!["p", "alt-2"], vec![],
                )),
            ]),
        ]);
        let err = CardSelectionContext::from_run_state(&state).unwrap_err();
        assert!(matches!(
            err,
            CardSelectionContextError::DuplicateCardAcrossBranches { card_id, .. }
            if card_id == "p"
        ));
    }

    // ─── Status filtering: skip non-normal iterations ─────────────────────────

    #[test]
    fn finished_with_wait_input_iteration_is_skipped() {
        let iid0 = iter_id();
        let mut short_iter = iteration(iter_id(), vec![
            finished_ok(StepKind::CandidateCardRetrieval, retrieval_result(
                Some(candidate("should-not-appear")), vec![],
            )),
        ]);
        short_iter.status = RunIterationStatus::FinishedWithWaitInput;
        let state = run_state_with(vec![
            iteration(iid0, vec![
                finished_ok(StepKind::CandidateCardRetrieval, retrieval_result(
                    Some(candidate("card-1")), vec![],
                )),
            ]),
            short_iter,
        ]);
        let ctx = CardSelectionContext::from_run_state(&state).unwrap();
        // Only the normal iteration contributes; short iteration is skipped.
        assert_eq!(ctx.history.len(), 1);
        assert_eq!(ctx.history[0].primary_card_id, "card-1");
    }

    #[test]
    fn finished_with_error_iteration_is_skipped() {
        let iid0 = iter_id();
        let mut error_iter = iteration(iter_id(), vec![
            finished_ok(StepKind::CandidateCardRetrieval, retrieval_result(
                Some(candidate("should-not-appear")), vec![],
            )),
        ]);
        error_iter.status = RunIterationStatus::FinishedWithError;
        let state = run_state_with(vec![
            iteration(iid0, vec![
                finished_ok(StepKind::CandidateCardRetrieval, retrieval_result(
                    Some(candidate("card-1")), vec![],
                )),
            ]),
            error_iter,
        ]);
        let ctx = CardSelectionContext::from_run_state(&state).unwrap();
        assert_eq!(ctx.history.len(), 1);
        assert_eq!(ctx.history[0].primary_card_id, "card-1");
    }

    #[test]
    fn short_iteration_between_normal_ones_does_not_cause_error() {
        // After iter 0 (initial), a short iter is skipped, then iter 1 (continuation)
        // is treated as a continuation (requires CardBranchReranking, not CandidateCardRetrieval).
        let iid0 = iter_id();
        let iid1 = iter_id();
        let mut short_iter = iteration(iter_id(), vec![]);
        short_iter.status = RunIterationStatus::FinishedWithWaitInput;
        let state = run_state_with(vec![
            iteration(iid0, vec![
                finished_ok(StepKind::CandidateCardRetrieval, retrieval_result(
                    Some(candidate("p0")), vec![],
                )),
            ]),
            short_iter,
            finished_iteration(iid1, vec![
                finished_ok(StepKind::CardBranchReranking, reranking_result(
                    "p1", PrimaryCardStatus::Sticky, vec![], vec![],
                )),
            ]),
        ]);
        let ctx = CardSelectionContext::from_run_state(&state).unwrap();
        assert_eq!(ctx.history.len(), 2);
        assert_eq!(ctx.history[0].primary_card_id, "p0");
        assert_eq!(ctx.history[1].primary_card_id, "p1");
    }

    // ─── Ordering ─────────────────────────────────────────────────────────────

    #[test]
    fn history_preserves_iteration_order() {
        let iid0 = iter_id();
        let iid1 = iter_id();
        let iid2 = iter_id();
        let state = run_state_with(vec![
            iteration(iid0, vec![
                finished_ok(StepKind::CandidateCardRetrieval, retrieval_result(
                    Some(candidate("p0")), vec![],
                )),
            ]),
            finished_iteration(iid1, vec![
                finished_ok(StepKind::CardBranchReranking, reranking_result(
                    "p1", PrimaryCardStatus::Tentative, vec![], vec![],
                )),
            ]),
            finished_iteration(iid2, vec![
                finished_ok(StepKind::CardBranchReranking, reranking_result(
                    "p2", PrimaryCardStatus::Sticky, vec![], vec![],
                )),
            ]),
        ]);
        let ctx = CardSelectionContext::from_run_state(&state).unwrap();
        assert_eq!(ctx.history[0].primary_card_id, "p0");
        assert_eq!(ctx.history[1].primary_card_id, "p1");
        assert_eq!(ctx.history[2].primary_card_id, "p2");
    }
}
