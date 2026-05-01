use std::collections::HashMap;
use std::sync::Arc;

use crate::api_clients::postgres::incident_card_store::{
    IncidentCardStore, IncidentCardStoreError,
};
use crate::shared_types::{CandidateCardRetrievalOutput, CardHydrationOutput, IncidentCard};
use tracing::{info_span, field, Instrument};

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, thiserror::Error)]
pub enum CardHydrationError {
    #[error("missing hydrated card for case_id: {case_id}")]
    MissingCard { case_id: String },
    #[error("incident card store error: {0}")]
    Store(IncidentCardStoreError),
}

// ─── Public struct ────────────────────────────────────────────────────────────

pub struct CardHydration {
    incident_card_store: Arc<dyn IncidentCardStore>,
}

impl std::fmt::Debug for CardHydration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CardHydration").finish_non_exhaustive()
    }
}

impl CardHydration {
    pub fn new(incident_card_store: Arc<dyn IncidentCardStore>) -> Self {
        Self {
            incident_card_store,
        }
    }

    pub async fn hydrate(
        &self,
        candidates: &CandidateCardRetrievalOutput,
    ) -> Result<CardHydrationOutput, CardHydrationError> {
        let primary_present = candidates.primary.is_some();
        let primary_case_id = candidates
            .primary
            .as_ref()
            .map(|p| p.case_id.as_str())
            .unwrap_or("");
        let alternatives_count = candidates.alternatives.len();

        let span = info_span!(
            "request_pipeline.card_hydration",
            module.name = "card_hydration",
            hydration.input.primary_present = primary_present,
            hydration.input.primary_case_id = primary_case_id,
            hydration.input.alternatives_count = alternatives_count,
            hydration.requested_case_ids_count = field::Empty,
            hydration.postgres_call_executed = field::Empty,
            hydration.cards_returned_count = field::Empty,
            hydration.primary_hydrated = field::Empty,
            hydration.alternatives_hydrated_count = field::Empty,
            hydration.order_reconstructed = field::Empty,
            hydration.partition_preserved = field::Empty,
            module.outcome = field::Empty,
            status = field::Empty,
            error.type = field::Empty,
            error.message = field::Empty,
        );

        self.hydrate_instrumented(candidates).instrument(span).await
    }

    async fn hydrate_instrumented(
        &self,
        candidates: &CandidateCardRetrievalOutput,
    ) -> Result<CardHydrationOutput, CardHydrationError> {
        let alternative_case_ids: Vec<&str> = candidates
            .alternatives
            .iter()
            .map(|a| a.case_id.as_str())
            .collect();
        tracing::event!(
            tracing::Level::INFO,
            event.name = "hydration_input_alternative_case_ids",
            hydration.input.alternative_case_ids = %serde_json::to_string(&alternative_case_ids)
                .unwrap_or_else(|_| "[]".to_string())
        );

        if candidates.primary.is_none() && candidates.alternatives.is_empty() {
            tracing::Span::current().record("hydration.postgres_call_executed", false);
            tracing::Span::current().record("hydration.requested_case_ids_count", 0);
            tracing::Span::current().record("hydration.cards_returned_count", 0);
            tracing::event!(
                tracing::Level::INFO,
                event.name = "hydration_requested_case_ids",
                hydration.requested_case_ids = "[]"
            );
            tracing::event!(
                tracing::Level::INFO,
                event.name = "hydration_returned_case_ids",
                hydration.returned_case_ids = "[]"
            );
            tracing::event!(
                tracing::Level::INFO,
                event.name = "hydration_missing_case_ids",
                hydration.missing_case_ids = "[]"
            );
            tracing::Span::current().record("hydration.primary_hydrated", false);
            tracing::Span::current().record("hydration.alternatives_hydrated_count", 0);
            tracing::Span::current().record("hydration.order_reconstructed", true);
            tracing::Span::current().record("hydration.partition_preserved", true);
            tracing::Span::current().record("module.outcome", "success");
            tracing::Span::current().record("status", "ok");

            return Ok(CardHydrationOutput {
                primary: None,
                alternatives: vec![],
            });
        }

        let mut case_ids: Vec<String> = Vec::new();
        if let Some(primary) = &candidates.primary {
            case_ids.push(primary.case_id.clone());
        }
        for alt in &candidates.alternatives {
            case_ids.push(alt.case_id.clone());
        }

        tracing::Span::current().record("hydration.requested_case_ids_count", case_ids.len());
        let case_ids_str: Vec<&str> = case_ids.iter().map(|s| s.as_str()).collect();
        tracing::event!(
            tracing::Level::INFO,
            event.name = "hydration_requested_case_ids",
            hydration.requested_case_ids = %serde_json::to_string(&case_ids_str)
                .unwrap_or_else(|_| "[]".to_string())
        );
        tracing::Span::current().record("hydration.postgres_call_executed", true);

        let postgres_span = info_span!(
            "postgres.incident_cards.get_by_case_ids",
            db.system = "postgresql",
            db.operation = "get_incident_cards_by_case_ids",
            db.requested_case_ids_count = case_ids.len(),
            db.returned_rows_count = field::Empty,
            status = field::Empty,
            error.type = field::Empty,
            error.message = field::Empty,
        );

        let cards = {
            async {
                match self.incident_card_store.get_cards_by_case_ids(&case_ids).await {
                    Ok(c) => {
                        tracing::Span::current().record("db.returned_rows_count", c.len());
                        tracing::Span::current().record("status", "ok");
                        Ok(c)
                    }
                    Err(e) => {
                        tracing::Span::current().record("status", "error");
                        tracing::Span::current()
                            .record("error.type", "CardHydration.Store");
                        tracing::Span::current()
                            .record("error.message", format!("Store query failed: {}", e));
                        Err(e)
                    }
                }
            }
            .instrument(postgres_span)
            .await
        };

        let cards = match cards {
            Ok(c) => c,
            Err(e) => {
                tracing::Span::current().record("module.outcome", "failure");
                tracing::Span::current().record("status", "error");
                tracing::Span::current().record("error.type", "CardHydration.Store");
                tracing::Span::current()
                    .record("error.message", format!("Store query failed: {}", e));
                return Err(CardHydrationError::Store(e));
            }
        };

        let returned_case_ids: Vec<&str> = cards.iter().map(|c| c.case_id.as_str()).collect();
        tracing::Span::current().record("hydration.cards_returned_count", cards.len());
        tracing::event!(
            tracing::Level::INFO,
            event.name = "hydration_returned_case_ids",
            hydration.returned_case_ids = %serde_json::to_string(&returned_case_ids)
                .unwrap_or_else(|_| "[]".to_string())
        );

        let lookup: HashMap<String, IncidentCard> =
            cards.into_iter().map(|c| (c.case_id.clone(), c)).collect();

        // Check for missing case_ids
        let mut missing_ids: Vec<&str> = Vec::new();
        for id in &case_ids {
            if !lookup.contains_key(id) {
                missing_ids.push(id.as_str());
            }
        }
        tracing::event!(
            tracing::Level::INFO,
            event.name = "hydration_missing_case_ids",
            hydration.missing_case_ids = %serde_json::to_string(&missing_ids)
                .unwrap_or_else(|_| "[]".to_string())
        );

        let primary = candidates
            .primary
            .as_ref()
            .map(|p| {
                lookup
                    .get(&p.case_id)
                    .cloned()
                    .ok_or_else(|| CardHydrationError::MissingCard {
                        case_id: p.case_id.clone(),
                    })
            })
            .transpose()
            .map_err(|e| {
                tracing::Span::current().record("module.outcome", "failure");
                tracing::Span::current().record("status", "error");
                tracing::Span::current().record("error.type", "CardHydration.MissingCard");
                let case_id = match &e {
                    CardHydrationError::MissingCard { case_id } => case_id.clone(),
                    _ => String::new(),
                };
                tracing::Span::current()
                    .record("error.message", format!("Missing primary card: {}", case_id));
                e
            })?;

        let primary_hydrated = primary.is_some();
        tracing::Span::current().record("hydration.primary_hydrated", primary_hydrated);

        let alternatives = candidates
            .alternatives
            .iter()
            .map(|alt| {
                lookup
                    .get(&alt.case_id)
                    .cloned()
                    .ok_or_else(|| CardHydrationError::MissingCard {
                        case_id: alt.case_id.clone(),
                    })
            })
            .collect::<Result<Vec<IncidentCard>, _>>()
            .map_err(|e| {
                tracing::Span::current().record("module.outcome", "failure");
                tracing::Span::current().record("status", "error");
                tracing::Span::current().record("error.type", "CardHydration.MissingCard");
                let case_id = match &e {
                    CardHydrationError::MissingCard { case_id } => case_id.clone(),
                    _ => String::new(),
                };
                tracing::Span::current()
                    .record("error.message", format!("Missing alternative card: {}", case_id));
                e
            })?;

        let alternatives_hydrated_count = alternatives.len();
        tracing::Span::current().record("hydration.alternatives_hydrated_count", alternatives_hydrated_count);

        // Check order preservation: alternatives should be in same order as input
        let order_reconstructed = candidates
            .alternatives
            .iter()
            .zip(alternatives.iter())
            .all(|(input_alt, hydrated_card)| input_alt.case_id == hydrated_card.case_id);
        tracing::Span::current().record("hydration.order_reconstructed", order_reconstructed);

        // Check partition preservation: primary should be separate from alternatives
        let partition_preserved = if alternatives_hydrated_count == 0 {
            // No alternatives, so partition is trivially preserved
            true
        } else if let Some(ref prim) = primary {
            // Check that no alternative is the same as primary
            alternatives.iter().all(|alt| alt.case_id != prim.case_id)
        } else {
            // We have alternatives but no primary, partition is preserved
            true
        };
        tracing::Span::current().record("hydration.partition_preserved", partition_preserved);

        tracing::Span::current().record("module.outcome", "success");
        tracing::Span::current().record("status", "ok");

        Ok(CardHydrationOutput {
            primary,
            alternatives,
        })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_types::{CandidateCard, IncidentCard};
    use crate::test_utils::postgres_store::MockPostgresIncidentCardStore;

    fn candidate(case_id: &str) -> CandidateCard {
        CandidateCard {
            case_id: case_id.to_string(),
            score: 0.9,
        }
    }

    fn card(case_id: &str) -> IncidentCard {
        IncidentCard {
            case_id: case_id.to_string(),
            title: "t".to_string(),
            source_type: "st".to_string(),
            source_name: "sn".to_string(),
            source_path: "sp".to_string(),
            vendor_or_project: None,
            system_type: None,
            version_tested: None,
            report_date: None,
            short_summary: "s".to_string(),
            canonical_symptoms: vec![],
            affected_components: vec![],
            failure_mode_candidates: vec![],
            observed_phases: vec![],
            incident_phases: vec![],
            turning_points: vec![],
            candidate_explanations: vec![],
            diagnostic_patterns: vec![],
            discriminating_checks: vec![],
            expected_observations: vec![],
            investigation_steps: vec![],
            root_cause_summary: None,
            reasoning_summary: None,
            mitigations_or_workarounds: vec![],
            prevention_or_design_followups: vec![],
            claimed_guarantees: vec![],
            violated_properties: vec![],
            resolution_status: None,
            fix_versions: vec![],
            confidence_notes: vec![],
            source_refs: vec![],
        }
    }

    fn ok(cards: Vec<IncidentCard>) -> Result<Vec<IncidentCard>, IncidentCardStoreError> {
        Ok(cards)
    }

    // ─── Empty input ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn empty_candidates_returns_empty_without_store_call() {
        let mock = Arc::new(MockPostgresIncidentCardStore::new(vec![]));
        let sut = CardHydration::new(mock.clone());
        let candidates = CandidateCardRetrievalOutput {
            primary: None,
            alternatives: vec![],
            metrics: None,
        };

        let out = sut.hydrate(&candidates).await.unwrap();

        assert!(out.primary.is_none());
        assert!(out.alternatives.is_empty());
        assert!(mock.captured_case_ids.lock().unwrap().is_empty());
    }

    // ─── Primary only ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn primary_only_hydrated_correctly() {
        let mock = Arc::new(MockPostgresIncidentCardStore::new(vec![ok(vec![card(
            "card-1",
        )])]));
        let sut = CardHydration::new(mock);
        let candidates = CandidateCardRetrievalOutput {
            primary: Some(candidate("card-1")),
            alternatives: vec![],
            metrics: None,
        };

        let out = sut.hydrate(&candidates).await.unwrap();

        assert_eq!(out.primary.unwrap().case_id, "card-1");
        assert!(out.alternatives.is_empty());
    }

    // ─── Alternatives only ────────────────────────────────────────────────────

    #[tokio::test]
    async fn alternatives_only_hydrated_in_order() {
        let mock = Arc::new(MockPostgresIncidentCardStore::new(vec![ok(vec![
            card("card-B"),
            card("card-A"),
        ])]));
        let sut = CardHydration::new(mock);
        let candidates = CandidateCardRetrievalOutput {
            primary: None,
            alternatives: vec![candidate("card-A"), candidate("card-B")],
            metrics: None,
        };

        let out = sut.hydrate(&candidates).await.unwrap();

        assert!(out.primary.is_none());
        let ids: Vec<&str> = out
            .alternatives
            .iter()
            .map(|c| c.case_id.as_str())
            .collect();
        assert_eq!(ids, vec!["card-A", "card-B"]);
    }

    // ─── Both present ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn both_present_one_store_call_partitioned_correctly() {
        let mock = Arc::new(MockPostgresIncidentCardStore::new(vec![ok(vec![
            card("card-2"),
            card("card-1"),
            card("card-3"),
        ])]));
        let sut = CardHydration::new(mock.clone());
        let candidates = CandidateCardRetrievalOutput {
            primary: Some(candidate("card-1")),
            alternatives: vec![candidate("card-2"), candidate("card-3")],
            metrics: None,
        };

        let out = sut.hydrate(&candidates).await.unwrap();

        assert_eq!(mock.captured_case_ids.lock().unwrap().len(), 1);
        assert_eq!(out.primary.unwrap().case_id, "card-1");
        let ids: Vec<&str> = out
            .alternatives
            .iter()
            .map(|c| c.case_id.as_str())
            .collect();
        assert_eq!(ids, vec!["card-2", "card-3"]);
    }

    // ─── case_ids passed to store ─────────────────────────────────────────────

    #[tokio::test]
    async fn case_ids_passed_to_store_in_primary_first_order() {
        let mock = Arc::new(MockPostgresIncidentCardStore::new(vec![ok(vec![
            card("card-1"),
            card("card-2"),
            card("card-3"),
        ])]));
        let sut = CardHydration::new(mock.clone());
        let candidates = CandidateCardRetrievalOutput {
            primary: Some(candidate("card-1")),
            alternatives: vec![candidate("card-2"), candidate("card-3")],
            metrics: None,
        };

        sut.hydrate(&candidates).await.unwrap();

        let captured = mock.captured_case_ids.lock().unwrap();
        assert_eq!(captured[0], vec!["card-1", "card-2", "card-3"]);
    }

    // ─── Missing card ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn missing_primary_card_returns_missing_card_error() {
        let mock = Arc::new(MockPostgresIncidentCardStore::new(vec![ok(vec![])]));
        let sut = CardHydration::new(mock);
        let candidates = CandidateCardRetrievalOutput {
            primary: Some(candidate("card-1")),
            alternatives: vec![],
            metrics: None,
        };

        let err = sut.hydrate(&candidates).await.unwrap_err();
        assert!(matches!(err, CardHydrationError::MissingCard { case_id } if case_id == "card-1"));
    }

    #[tokio::test]
    async fn missing_alternative_card_returns_missing_card_error() {
        let mock = Arc::new(MockPostgresIncidentCardStore::new(vec![ok(vec![card(
            "card-1",
        )])]));
        let sut = CardHydration::new(mock);
        let candidates = CandidateCardRetrievalOutput {
            primary: None,
            alternatives: vec![candidate("card-1"), candidate("card-missing")],
            metrics: None,
        };

        let err = sut.hydrate(&candidates).await.unwrap_err();
        assert!(
            matches!(err, CardHydrationError::MissingCard { case_id } if case_id == "card-missing")
        );
    }

    // ─── Store error ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn store_error_wrapped_as_store_variant() {
        let mock = Arc::new(MockPostgresIncidentCardStore::new(vec![Err(
            IncidentCardStoreError::Query("db error".to_string()),
        )]));
        let sut = CardHydration::new(mock);
        let candidates = CandidateCardRetrievalOutput {
            primary: Some(candidate("card-1")),
            alternatives: vec![],
            metrics: None,
        };

        let err = sut.hydrate(&candidates).await.unwrap_err();
        assert!(matches!(err, CardHydrationError::Store(_)));
    }
}
