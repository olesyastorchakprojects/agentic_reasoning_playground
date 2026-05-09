use std::path::Path;

use thiserror::Error;

use crate::shared_types::{GoldenQuestion, UserRequest};

#[derive(Debug)]
pub struct GoldenEvalCase {
    pub initial_request: UserRequest,
    pub observation_requests: Vec<UserRequest>,
}

#[derive(Debug, Error)]
pub enum GoldenEvalInputError {
    #[error("failed to read golden cases file '{path}': {message}")]
    GoldenCasesRead { path: String, message: String },

    #[error("failed to read golden cases schema '{path}': {message}")]
    GoldenCasesSchemaRead { path: String, message: String },

    #[error("golden cases file is not valid JSON: {message}")]
    InvalidJson { message: String },

    #[error("golden cases schema validation failed: {message}")]
    SchemaValidation { message: String },

    #[error("golden cases typed parsing failed: {message}")]
    TypedParse { message: String },
}

pub fn load_golden_eval_requests(
    golden_cases_file: &Path,
    golden_cases_schema: &Path,
) -> Result<Vec<GoldenEvalCase>, GoldenEvalInputError> {
    let schema_text = std::fs::read_to_string(golden_cases_schema).map_err(|e| {
        GoldenEvalInputError::GoldenCasesSchemaRead {
            path: golden_cases_schema.to_string_lossy().into_owned(),
            message: e.to_string(),
        }
    })?;

    let schema_value =
        serde_json::from_str::<serde_json::Value>(&schema_text).map_err(|e| {
            GoldenEvalInputError::GoldenCasesSchemaRead {
                path: golden_cases_schema.to_string_lossy().into_owned(),
                message: e.to_string(),
            }
        })?;

    let cases_text = std::fs::read_to_string(golden_cases_file).map_err(|e| {
        GoldenEvalInputError::GoldenCasesRead {
            path: golden_cases_file.to_string_lossy().into_owned(),
            message: e.to_string(),
        }
    })?;

    let cases_value =
        serde_json::from_str::<serde_json::Value>(&cases_text).map_err(|e| {
            GoldenEvalInputError::InvalidJson {
                message: e.to_string(),
            }
        })?;

    let validator = jsonschema::validator_for(&schema_value).map_err(|e| {
        GoldenEvalInputError::GoldenCasesSchemaRead {
            path: golden_cases_schema.to_string_lossy().into_owned(),
            message: format!("invalid schema: {e}"),
        }
    })?;

    validator.validate(&cases_value).map_err(|e| {
        GoldenEvalInputError::SchemaValidation {
            message: e.to_string(),
        }
    })?;

    let golden_questions =
        serde_json::from_value::<Vec<GoldenQuestion>>(cases_value).map_err(|e| {
            GoldenEvalInputError::TypedParse {
                message: e.to_string(),
            }
        })?;

    let cases = golden_questions
        .into_iter()
        .map(|q| {
            let initial_request = UserRequest {
                query: q.query.raw.clone(),
                golden_question: Some(q.clone()),
            };
            let observation_requests = q
                .query
                .observations
                .iter()
                .map(|obs| UserRequest {
                    query: obs.raw.clone(),
                    golden_question: Some(q.clone()),
                })
                .collect();
            GoldenEvalCase { initial_request, observation_requests }
        })
        .collect();

    Ok(cases)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    const VALID_SCHEMA_STR: &str = r#"{
        "type": "array",
        "items": {
            "type": "object",
            "required": [
                "case_id",
                "query",
                "expected_query_structuring",
                "expected_candidate_cards",
                "expected_incident_evidence",
                "expected_theory_evidence"
            ],
            "properties": {
                "case_id": { "type": "string" },
                "query": { "type": "object" },
                "expected_query_structuring": { "type": "object" },
                "expected_candidate_cards": { "type": "object" },
                "expected_incident_evidence": { "type": "object" },
                "expected_theory_evidence": { "type": "object" }
            }
        }
    }"#;

    const STRICT_CASE_ID_SCHEMA_STR: &str = r#"{
        "type": "array",
        "items": {
            "type": "object",
            "required": ["case_id"],
            "properties": {
                "case_id": { "type": "string", "minLength": 1 }
            }
        }
    }"#;

    const PERMISSIVE_SCHEMA_STR: &str = r#"{"type": "array"}"#;

    const VALID_MINIMAL_CASE_STR: &str = r#"[{
        "case_id": "test-001",
        "query": { "raw": "what should I check first?", "observations": [] },
        "expected_query_structuring": {
            "symptoms": {
                "strict_vocabulary_terms": [],
                "soft_vocabulary_terms": [],
                "graded_relevance": []
            },
            "affected_subsystems": {
                "strict_vocabulary_terms": [],
                "soft_vocabulary_terms": [],
                "graded_relevance": []
            },
            "failure_modes": {
                "strict_vocabulary_terms": [],
                "soft_vocabulary_terms": [],
                "graded_relevance": []
            },
            "system_properties": {
                "strict_vocabulary_terms": [],
                "soft_vocabulary_terms": [],
                "graded_relevance": []
            }
        },
        "expected_candidate_cards": {
            "retrieval_relevant_cards": {
                "strict_card_ids": [],
                "soft_card_ids": [],
                "graded_relevance": []
            }
        },
        "expected_incident_evidence": {
            "primary_card_evidence_query": {
                "retrieval_call_id": "primary",
                "relevance_judgments": {
                    "strict_chunk_ids": [],
                    "soft_chunk_ids": [],
                    "graded_relevance": []
                }
            },
            "alternative_cards_evidence_query": {
                "retrieval_call_id": "alternatives",
                "relevance_judgments": {
                    "strict_chunk_ids": [],
                    "soft_chunk_ids": [],
                    "graded_relevance": []
                }
            }
        },
        "expected_theory_evidence": {
            "mechanism_explanation": {
                "strict_chunk_ids": [],
                "soft_chunk_ids": [],
                "graded_relevance": []
            }
        }
    }]"#;

    fn write_temp_file(dir: &TempDir, name: &str, content: &str) -> std::path::PathBuf {
        let path = dir.path().join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn missing_cases_file_returns_golden_cases_read_error() {
        let dir = TempDir::new().unwrap();
        let schema_path = write_temp_file(&dir, "schema.json", VALID_SCHEMA_STR);
        let cases_path = dir.path().join("nonexistent_cases.json");

        let err = load_golden_eval_requests(&cases_path, &schema_path).unwrap_err();
        match &err {
            GoldenEvalInputError::GoldenCasesRead { path, .. } => {
                assert_eq!(path, cases_path.to_str().unwrap());
            }
            other => panic!("expected GoldenCasesRead, got: {other}"),
        }
    }

    #[test]
    fn missing_schema_file_returns_golden_cases_schema_read_error() {
        let dir = TempDir::new().unwrap();
        let schema_path = dir.path().join("nonexistent_schema.json");
        let cases_path = dir.path().join("cases.json");

        let err = load_golden_eval_requests(&cases_path, &schema_path).unwrap_err();
        match &err {
            GoldenEvalInputError::GoldenCasesSchemaRead { path, .. } => {
                assert_eq!(path, schema_path.to_str().unwrap());
            }
            other => panic!("expected GoldenCasesSchemaRead, got: {other}"),
        }
    }

    #[test]
    fn malformed_cases_json_returns_invalid_json_error() {
        let dir = TempDir::new().unwrap();
        let schema_path = write_temp_file(&dir, "schema.json", VALID_SCHEMA_STR);
        let cases_path = write_temp_file(&dir, "cases.json", "not valid json!!!");

        let err = load_golden_eval_requests(&cases_path, &schema_path).unwrap_err();
        assert!(
            matches!(err, GoldenEvalInputError::InvalidJson { .. }),
            "expected InvalidJson, got: {err}"
        );
    }

    #[test]
    fn schema_validation_failure_returns_schema_validation_error() {
        let dir = TempDir::new().unwrap();
        let schema_path = write_temp_file(&dir, "schema.json", STRICT_CASE_ID_SCHEMA_STR);
        // case_id is a number; schema requires a non-empty string
        let cases_path = write_temp_file(&dir, "cases.json", r#"[{"case_id": 42}]"#);

        let err = load_golden_eval_requests(&cases_path, &schema_path).unwrap_err();
        assert!(
            matches!(err, GoldenEvalInputError::SchemaValidation { .. }),
            "expected SchemaValidation, got: {err}"
        );
    }

    #[test]
    fn typed_parse_failure_after_valid_schema_returns_typed_parse_error() {
        let dir = TempDir::new().unwrap();
        // Permissive schema accepts any array element — schema validation passes.
        let schema_path = write_temp_file(&dir, "schema.json", PERMISSIVE_SCHEMA_STR);
        // Missing required GoldenQuestion fields — serde deserialization fails.
        let cases_path = write_temp_file(&dir, "cases.json", r#"[{"wrong_field": "value"}]"#);

        let err = load_golden_eval_requests(&cases_path, &schema_path).unwrap_err();
        assert!(
            matches!(err, GoldenEvalInputError::TypedParse { .. }),
            "expected TypedParse, got: {err}"
        );
    }

    const VALID_CASE_WITH_OBSERVATIONS_STR: &str = r#"[{
        "case_id": "test-obs-001",
        "query": {
            "raw": "what should I check first?",
            "observations": [
                {"observation_id": "obs_1", "raw": "first observation text"},
                {"observation_id": "obs_2", "raw": "second observation text"}
            ]
        },
        "expected_query_structuring": {
            "symptoms": { "strict_vocabulary_terms": [], "soft_vocabulary_terms": [], "graded_relevance": [] },
            "affected_subsystems": { "strict_vocabulary_terms": [], "soft_vocabulary_terms": [], "graded_relevance": [] },
            "failure_modes": { "strict_vocabulary_terms": [], "soft_vocabulary_terms": [], "graded_relevance": [] },
            "system_properties": { "strict_vocabulary_terms": [], "soft_vocabulary_terms": [], "graded_relevance": [] }
        },
        "expected_candidate_cards": {
            "retrieval_relevant_cards": { "strict_card_ids": [], "soft_card_ids": [], "graded_relevance": [] }
        },
        "expected_incident_evidence": {
            "primary_card_evidence_query": { "retrieval_call_id": "primary", "relevance_judgments": { "strict_chunk_ids": [], "soft_chunk_ids": [], "graded_relevance": [] } },
            "alternative_cards_evidence_query": { "retrieval_call_id": "alternatives", "relevance_judgments": { "strict_chunk_ids": [], "soft_chunk_ids": [], "graded_relevance": [] } }
        },
        "expected_theory_evidence": {
            "mechanism_explanation": { "strict_chunk_ids": [], "soft_chunk_ids": [], "graded_relevance": [] }
        }
    }]"#;

    #[test]
    fn successful_load_returns_one_case_per_golden_case() {
        let dir = TempDir::new().unwrap();
        let schema_path = write_temp_file(&dir, "schema.json", VALID_SCHEMA_STR);
        let cases_path = write_temp_file(&dir, "cases.json", VALID_MINIMAL_CASE_STR);

        let cases = load_golden_eval_requests(&cases_path, &schema_path).unwrap();
        assert_eq!(cases.len(), 1);
    }

    #[test]
    fn successful_load_initial_request_query_matches_query_raw() {
        let dir = TempDir::new().unwrap();
        let schema_path = write_temp_file(&dir, "schema.json", VALID_SCHEMA_STR);
        let cases_path = write_temp_file(&dir, "cases.json", VALID_MINIMAL_CASE_STR);

        let cases = load_golden_eval_requests(&cases_path, &schema_path).unwrap();
        assert_eq!(cases[0].initial_request.query, "what should I check first?");
    }

    #[test]
    fn successful_load_golden_question_is_some_and_preserves_structure() {
        let dir = TempDir::new().unwrap();
        let schema_path = write_temp_file(&dir, "schema.json", VALID_SCHEMA_STR);
        let cases_path = write_temp_file(&dir, "cases.json", VALID_MINIMAL_CASE_STR);

        let cases = load_golden_eval_requests(&cases_path, &schema_path).unwrap();
        let gq = cases[0]
            .initial_request
            .golden_question
            .as_ref()
            .expect("golden_question must be Some in batch mode");

        assert_eq!(gq.case_id, "test-001");
        assert_eq!(gq.query.raw, "what should I check first?");
        assert!(gq.expected_query_structuring.symptoms.strict_vocabulary_terms.is_empty());
        assert!(gq.expected_query_structuring.affected_subsystems.graded_relevance.is_empty());
        assert!(gq.expected_candidate_cards.retrieval_relevant_cards.strict_card_ids.is_empty());
        assert_eq!(
            gq.expected_incident_evidence.primary_card_evidence_query.retrieval_call_id,
            "primary"
        );
        assert_eq!(
            gq.expected_incident_evidence
                .alternative_cards_evidence_query
                .retrieval_call_id,
            "alternatives"
        );
        assert!(gq.expected_theory_evidence.mechanism_explanation.strict_chunk_ids.is_empty());
    }

    #[test]
    fn case_without_observations_has_empty_observation_requests() {
        let dir = TempDir::new().unwrap();
        let schema_path = write_temp_file(&dir, "schema.json", VALID_SCHEMA_STR);
        let cases_path = write_temp_file(&dir, "cases.json", VALID_MINIMAL_CASE_STR);

        let cases = load_golden_eval_requests(&cases_path, &schema_path).unwrap();
        assert!(cases[0].observation_requests.is_empty());
    }

    #[test]
    fn observations_produce_one_observation_request_each() {
        let dir = TempDir::new().unwrap();
        let schema_path = write_temp_file(&dir, "schema.json", VALID_SCHEMA_STR);
        let cases_path =
            write_temp_file(&dir, "cases.json", VALID_CASE_WITH_OBSERVATIONS_STR);

        let cases = load_golden_eval_requests(&cases_path, &schema_path).unwrap();
        assert_eq!(cases[0].observation_requests.len(), 2);
    }

    #[test]
    fn observation_request_query_matches_observation_raw() {
        let dir = TempDir::new().unwrap();
        let schema_path = write_temp_file(&dir, "schema.json", VALID_SCHEMA_STR);
        let cases_path =
            write_temp_file(&dir, "cases.json", VALID_CASE_WITH_OBSERVATIONS_STR);

        let cases = load_golden_eval_requests(&cases_path, &schema_path).unwrap();
        assert_eq!(cases[0].observation_requests[0].query, "first observation text");
        assert_eq!(cases[0].observation_requests[1].query, "second observation text");
    }

    #[test]
    fn observation_requests_carry_golden_question() {
        let dir = TempDir::new().unwrap();
        let schema_path = write_temp_file(&dir, "schema.json", VALID_SCHEMA_STR);
        let cases_path =
            write_temp_file(&dir, "cases.json", VALID_CASE_WITH_OBSERVATIONS_STR);

        let cases = load_golden_eval_requests(&cases_path, &schema_path).unwrap();
        for obs_req in &cases[0].observation_requests {
            let gq = obs_req
                .golden_question
                .as_ref()
                .expect("observation_request must carry golden_question");
            assert_eq!(gq.case_id, "test-obs-001");
        }
    }
}
