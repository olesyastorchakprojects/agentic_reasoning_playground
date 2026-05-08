use serde::{Deserialize, Serialize};

use crate::shared_types::{
    CandidateCardRetrievalOutput, CardBranchRerankingOutput, CardSelectionContext,
    PrimaryCardStatus,
};

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum CardBranchRerankingError {
    #[error("card selection context history must not be empty")]
    EmptyCardSelectionHistory,

    #[error("fresh candidate retrieval output must contain a primary card")]
    MissingFreshPrimary,

    #[error("duplicate card id '{card_id}' in fresh candidate list")]
    DuplicateFreshCandidate { card_id: String },

    #[error("fresh primary card does not match ranked_candidates[0]")]
    FreshPrimaryMismatch,

    #[error("fresh candidate window is too shallow for reranking policy")]
    InsufficientFreshCandidateWindow,
}

// ─── Constants ────────────────────────────────────────────────────────────────

const TENTATIVE_RETENTION_WINDOW: usize = 2;
const STICKY_RETENTION_WINDOW: usize = 4;
const ALTERNATIVE_ELIGIBILITY_WINDOW: usize = 5;
const MAX_ALTERNATIVES: usize = 2;
const MIN_FRESH_WINDOW: usize = 5;

// ─── Public struct ────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct CardBranchReranking {}

impl CardBranchReranking {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn rerank(
        &self,
        fresh_candidates: &CandidateCardRetrievalOutput,
        card_selection_context: &CardSelectionContext,
    ) -> Result<CardBranchRerankingOutput, CardBranchRerankingError> {
        // 1. EmptyCardSelectionHistory
        if card_selection_context.history.is_empty() {
            return Err(CardBranchRerankingError::EmptyCardSelectionHistory);
        }

        // 2. MissingFreshPrimary
        let fresh_primary = fresh_candidates
            .primary
            .as_ref()
            .ok_or(CardBranchRerankingError::MissingFreshPrimary)?;

        // 3. FreshPrimaryMismatch (only when ranked_candidates[0] exists)
        if let Some(first) = fresh_candidates.ranked_candidates.first() {
            if first.case_id != fresh_primary.case_id {
                return Err(CardBranchRerankingError::FreshPrimaryMismatch);
            }
        }

        // 4. DuplicateFreshCandidate
        let mut seen = std::collections::HashSet::new();
        for card in &fresh_candidates.ranked_candidates {
            if !seen.insert(&card.case_id) {
                return Err(CardBranchRerankingError::DuplicateFreshCandidate {
                    card_id: card.case_id.clone(),
                });
            }
        }

        // 5. InsufficientFreshCandidateWindow
        if fresh_candidates.ranked_candidates.len() < MIN_FRESH_WINDOW {
            return Err(CardBranchRerankingError::InsufficientFreshCandidateWindow);
        }

        let fresh_ids: Vec<&str> = fresh_candidates
            .ranked_candidates
            .iter()
            .map(|c| c.case_id.as_str())
            .collect();

        let last = card_selection_context.history.last().unwrap();
        let previous_primary = &last.primary_card_id;
        let previous_status = last.primary_card_status;

        // ── Primary selection ─────────────────────────────────────────────────
        let previous_fresh_rank = fresh_ids
            .iter()
            .position(|id| *id == previous_primary.as_str())
            .map(|i| i + 1); // 1-based

        let retention_window = match previous_status {
            PrimaryCardStatus::Tentative => TENTATIVE_RETENTION_WINDOW,
            PrimaryCardStatus::Sticky => STICKY_RETENTION_WINDOW,
        };

        let (new_primary_id, new_primary_status) =
            match previous_fresh_rank {
                Some(rank) if rank <= retention_window => {
                    (previous_primary.clone(), PrimaryCardStatus::Sticky)
                }
                _ => (fresh_ids[0].to_string(), PrimaryCardStatus::Tentative),
            };

        // ── Alternative selection ─────────────────────────────────────────────

        // Step 1: preserve historical alternatives within top-5 window
        let mut alternatives: Vec<String> = last
            .alternative_card_ids
            .iter()
            .filter(|alt_id: &&String| {
                let rank = fresh_ids
                    .iter()
                    .position(|id| *id == alt_id.as_str())
                    .map(|i| i + 1);
                matches!(rank, Some(r) if r <= ALTERNATIVE_ELIGIBILITY_WINDOW)
                    && **alt_id != new_primary_id
            })
            .cloned()
            .collect();

        // Step 2: fill remaining slots from fresh list
        if alternatives.len() < MAX_ALTERNATIVES {
            for id in &fresh_ids {
                if alternatives.len() >= MAX_ALTERNATIVES {
                    break;
                }
                if *id != new_primary_id && !alternatives.iter().any(|a: &String| a.as_str() == *id) {
                    alternatives.push(id.to_string());
                }
            }
        }

        // Step 3: trim
        alternatives.truncate(MAX_ALTERNATIVES);

        Ok(CardBranchRerankingOutput {
            primary_card_id: new_primary_id,
            primary_card_status: new_primary_status,
            alternative_card_ids: alternatives,
            challenger_card_ids: vec![],
        })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_types::{
        CandidateCard, CandidateCardRetrievalOutput, CardBranchRerankingOutput,
        CardSelectionContext, CardSelectionSnapshot, PrimaryCardStatus,
    };
    use crate::orchestrator::run_state::model::RunIterationId;
    use uuid::Uuid;

    fn reranker() -> CardBranchReranking {
        CardBranchReranking::new()
    }

    fn iter_id() -> RunIterationId {
        RunIterationId(Uuid::new_v4())
    }

    fn card(id: &str) -> CandidateCard {
        CandidateCard { case_id: id.to_string(), score: 0.9 }
    }

    fn fresh(cards: Vec<CandidateCard>) -> CandidateCardRetrievalOutput {
        let primary = cards.first().cloned();
        let alternatives = cards.iter().skip(1).cloned().collect();
        CandidateCardRetrievalOutput {
            ranked_candidates: cards,
            primary,
            alternatives,
            metrics: None,
        }
    }

    fn fresh_no_primary(cards: Vec<CandidateCard>) -> CandidateCardRetrievalOutput {
        CandidateCardRetrievalOutput {
            ranked_candidates: cards,
            primary: None,
            alternatives: vec![],
            metrics: None,
        }
    }

    fn history_one(
        primary_id: &str,
        status: PrimaryCardStatus,
        alternatives: Vec<&str>,
    ) -> CardSelectionContext {
        CardSelectionContext {
            history: vec![CardSelectionSnapshot {
                iteration_id: iter_id(),
                primary_card_id: primary_id.to_string(),
                primary_card_status: status,
                alternative_card_ids: alternatives.into_iter().map(|s| s.to_string()).collect(),
                challenger_card_ids: vec![],
            }],
        }
    }

    // five fresh candidates anchored on p1
    fn five_fresh(primary: &str, rest: &[&str]) -> CandidateCardRetrievalOutput {
        let mut all = vec![card(primary)];
        all.extend(rest.iter().map(|s| card(s)));
        fresh(all)
    }

    // ─── Constructor ─────────────────────────────────────────────────────────

    #[test]
    fn new_constructs_stateless_reranker() {
        let _ = CardBranchReranking::new();
    }

    // ─── Validation errors ────────────────────────────────────────────────────

    #[test]
    fn empty_history_fails() {
        let ctx = CardSelectionContext { history: vec![] };
        let f = five_fresh("p1", &["a", "b", "c", "d"]);
        let err = reranker().rerank(&f, &ctx).unwrap_err();
        assert!(matches!(err, CardBranchRerankingError::EmptyCardSelectionHistory));
    }

    #[test]
    fn missing_primary_fails() {
        let ctx = history_one("p1", PrimaryCardStatus::Tentative, vec![]);
        let f = fresh_no_primary(vec![card("p1"), card("a"), card("b"), card("c"), card("d")]);
        let err = reranker().rerank(&f, &ctx).unwrap_err();
        assert!(matches!(err, CardBranchRerankingError::MissingFreshPrimary));
    }

    #[test]
    fn duplicate_ids_in_ranked_candidates_fail() {
        let ctx = history_one("p1", PrimaryCardStatus::Tentative, vec![]);
        let f = CandidateCardRetrievalOutput {
            ranked_candidates: vec![card("p1"), card("a"), card("a"), card("c"), card("d")],
            primary: Some(card("p1")),
            alternatives: vec![],
            metrics: None,
        };
        let err = reranker().rerank(&f, &ctx).unwrap_err();
        assert!(matches!(err, CardBranchRerankingError::DuplicateFreshCandidate { card_id } if card_id == "a"));
    }

    #[test]
    fn primary_mismatch_with_ranked_candidates_fails() {
        let ctx = history_one("p1", PrimaryCardStatus::Tentative, vec![]);
        let f = CandidateCardRetrievalOutput {
            ranked_candidates: vec![card("x"), card("a"), card("b"), card("c"), card("d")],
            primary: Some(card("p1")),
            alternatives: vec![],
            metrics: None,
        };
        let err = reranker().rerank(&f, &ctx).unwrap_err();
        assert!(matches!(err, CardBranchRerankingError::FreshPrimaryMismatch));
    }

    #[test]
    fn fewer_than_five_ranked_candidates_fail() {
        let ctx = history_one("p1", PrimaryCardStatus::Tentative, vec![]);
        let f = fresh(vec![card("p1"), card("a"), card("b"), card("c")]);
        let err = reranker().rerank(&f, &ctx).unwrap_err();
        assert!(matches!(err, CardBranchRerankingError::InsufficientFreshCandidateWindow));
    }

    // ─── Primary selection — Tentative ────────────────────────────────────────

    #[test]
    fn tentative_primary_within_top2_becomes_sticky() {
        // previous primary "p1" is at rank 1 → preserved, status → Sticky
        let ctx = history_one("p1", PrimaryCardStatus::Tentative, vec![]);
        let f = five_fresh("p1", &["a", "b", "c", "d"]);
        let out = reranker().rerank(&f, &ctx).unwrap();
        assert_eq!(out.primary_card_id, "p1");
        assert_eq!(out.primary_card_status, PrimaryCardStatus::Sticky);
    }

    #[test]
    fn tentative_primary_at_rank2_becomes_sticky() {
        // previous primary "p1" is at rank 2 → preserved
        let ctx = history_one("p1", PrimaryCardStatus::Tentative, vec![]);
        let f = five_fresh("x", &["p1", "b", "c", "d"]);
        // But wait - "x" is ranked 1, "p1" is ranked 2, and primary = Some("x")
        // This is a valid fresh result where "x" is the new top candidate.
        // Previous primary "p1" is at fresh rank 2, within tentative window (top 2).
        // But the primary field must correspond to ranked_candidates[0] = "x".
        // The module should treat the *previous* primary "p1" at rank 2 → preserve it, status Sticky.
        let out = reranker().rerank(&f, &ctx).unwrap();
        assert_eq!(out.primary_card_id, "p1");
        assert_eq!(out.primary_card_status, PrimaryCardStatus::Sticky);
    }

    #[test]
    fn tentative_primary_below_top2_is_replaced() {
        // previous primary "p1" at rank 3 → replace with fresh rank-1
        let ctx = history_one("p1", PrimaryCardStatus::Tentative, vec![]);
        let f = five_fresh("new1", &["new2", "p1", "c", "d"]);
        let out = reranker().rerank(&f, &ctx).unwrap();
        assert_eq!(out.primary_card_id, "new1");
        assert_eq!(out.primary_card_status, PrimaryCardStatus::Tentative);
    }

    #[test]
    fn tentative_primary_absent_from_fresh_is_replaced() {
        let ctx = history_one("gone", PrimaryCardStatus::Tentative, vec![]);
        let f = five_fresh("new1", &["a", "b", "c", "d"]);
        let out = reranker().rerank(&f, &ctx).unwrap();
        assert_eq!(out.primary_card_id, "new1");
        assert_eq!(out.primary_card_status, PrimaryCardStatus::Tentative);
    }

    // ─── Primary selection — Sticky ───────────────────────────────────────────

    #[test]
    fn sticky_primary_within_top4_stays_sticky() {
        let ctx = history_one("p1", PrimaryCardStatus::Sticky, vec![]);
        // p1 at rank 4
        let f = five_fresh("x", &["y", "z", "p1", "d"]);
        let out = reranker().rerank(&f, &ctx).unwrap();
        assert_eq!(out.primary_card_id, "p1");
        assert_eq!(out.primary_card_status, PrimaryCardStatus::Sticky);
    }

    #[test]
    fn sticky_primary_below_top4_is_replaced() {
        let ctx = history_one("p1", PrimaryCardStatus::Sticky, vec![]);
        // p1 at rank 5 → replaced
        let f = five_fresh("x", &["y", "z", "w", "p1"]);
        let out = reranker().rerank(&f, &ctx).unwrap();
        assert_eq!(out.primary_card_id, "x");
        assert_eq!(out.primary_card_status, PrimaryCardStatus::Tentative);
    }

    #[test]
    fn sticky_primary_absent_from_fresh_is_replaced() {
        let ctx = history_one("gone", PrimaryCardStatus::Sticky, vec![]);
        let f = five_fresh("new1", &["a", "b", "c", "d"]);
        let out = reranker().rerank(&f, &ctx).unwrap();
        assert_eq!(out.primary_card_id, "new1");
        assert_eq!(out.primary_card_status, PrimaryCardStatus::Tentative);
    }

    // ─── Alternative selection ────────────────────────────────────────────────

    #[test]
    fn historical_alternatives_within_top5_are_preserved_in_order() {
        // Previous alts: ["a2", "a1"]. Both within top 5 fresh ranks.
        let ctx = history_one("p1", PrimaryCardStatus::Sticky, vec!["a2", "a1"]);
        // fresh: p1=1, a1=2, a2=3, c=4, d=5
        let f = five_fresh("p1", &["a1", "a2", "c", "d"]);
        let out = reranker().rerank(&f, &ctx).unwrap();
        // preserved in historical order: ["a2", "a1"]
        assert_eq!(out.alternative_card_ids, vec!["a2", "a1"]);
    }

    #[test]
    fn historical_alternatives_outside_top5_are_dropped() {
        // Previous alt "old" is not in fresh top 5
        let ctx = history_one("p1", PrimaryCardStatus::Sticky, vec!["old"]);
        let f = five_fresh("p1", &["a", "b", "c", "d"]);
        // "old" absent → drop. Fill from fresh: a, b
        let out = reranker().rerank(&f, &ctx).unwrap();
        assert!(!out.alternative_card_ids.contains(&"old".to_string()));
        assert!(out.alternative_card_ids.len() <= 2);
    }

    #[test]
    fn alternatives_filled_from_fresh_when_fewer_than_two_preserved() {
        // No historical alternatives preserved
        let ctx = history_one("p1", PrimaryCardStatus::Tentative, vec![]);
        let f = five_fresh("p1", &["a", "b", "c", "d"]);
        let out = reranker().rerank(&f, &ctx).unwrap();
        assert_eq!(out.alternative_card_ids, vec!["a", "b"]);
    }

    #[test]
    fn primary_never_appears_in_alternatives() {
        let ctx = history_one("p1", PrimaryCardStatus::Sticky, vec!["p1", "a"]);
        let f = five_fresh("p1", &["a", "b", "c", "d"]);
        let out = reranker().rerank(&f, &ctx).unwrap();
        assert!(!out.alternative_card_ids.contains(&out.primary_card_id));
    }

    #[test]
    fn challenger_card_ids_always_empty() {
        let ctx = history_one("p1", PrimaryCardStatus::Tentative, vec![]);
        let f = five_fresh("p1", &["a", "b", "c", "d"]);
        let out = reranker().rerank(&f, &ctx).unwrap();
        assert!(out.challenger_card_ids.is_empty());
    }

    #[test]
    fn alternatives_never_exceed_two() {
        let ctx = history_one("p1", PrimaryCardStatus::Tentative, vec!["a", "b"]);
        let f = five_fresh("p1", &["a", "b", "c", "d"]);
        let out = reranker().rerank(&f, &ctx).unwrap();
        assert!(out.alternative_card_ids.len() <= 2);
    }
}
