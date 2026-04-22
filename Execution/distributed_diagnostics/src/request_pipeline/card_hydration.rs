use std::collections::HashMap;
use std::sync::Arc;

use crate::api_clients::postgres::incident_card_store::IncidentCardStoreError;
use crate::shared_types::{CandidateCardRetrievalOutput, CardHydrationOutput, IncidentCard};

#[cfg(not(test))]
use crate::api_clients::postgres::incident_card_store::PostgresIncidentCardStore;
#[cfg(test)]
use crate::test_utils::postgres_store::MockPostgresIncidentCardStore as PostgresIncidentCardStore;

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, thiserror::Error)]
pub enum CardHydrationError {
    #[error("missing hydrated card for case_id: {case_id}")]
    MissingCard { case_id: String },
    #[error("incident card store error: {0}")]
    Store(IncidentCardStoreError),
}

// ─── Public struct ────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct CardHydration {
    incident_card_store: Arc<PostgresIncidentCardStore>,
}

impl CardHydration {
    pub fn new(incident_card_store: Arc<PostgresIncidentCardStore>) -> Self {
        Self {
            incident_card_store,
        }
    }

    pub async fn hydrate(
        &self,
        candidates: &CandidateCardRetrievalOutput,
    ) -> Result<CardHydrationOutput, CardHydrationError> {
        if candidates.primary.is_none() && candidates.alternatives.is_empty() {
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

        let cards = self
            .incident_card_store
            .get_cards_by_case_ids(&case_ids)
            .await
            .map_err(CardHydrationError::Store)?;

        let lookup: HashMap<String, IncidentCard> =
            cards.into_iter().map(|c| (c.case_id.clone(), c)).collect();

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
            .transpose()?;

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
            .collect::<Result<Vec<IncidentCard>, _>>()?;

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
        };

        let err = sut.hydrate(&candidates).await.unwrap_err();
        assert!(matches!(err, CardHydrationError::Store(_)));
    }
}
