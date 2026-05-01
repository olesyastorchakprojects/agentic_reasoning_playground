use serde::Deserialize;
use tracing::{field, info_span};

use crate::shared_types::{
    Context, DiagnosticResponse, DiagnosticResultInterpretation,
    LlmStructuredGenerationOutput, ResponseValidationAndNormalizationOutput,
};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, thiserror::Error)]
pub enum ResponseValidationAndNormalizationError {
    #[error("invalid response shape: {0}")]
    InvalidResponseShape(String),
    #[error("business rule violation: {0}")]
    BusinessRuleViolation(String),
}

// ---------------------------------------------------------------------------
// Private raw deserialization structs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDiagnosticResponse {
    problem_understanding: String,
    similar_practical_context: String,
    active_hypotheses: Vec<String>,
    first_check: String,
    result_interpretation: RawResultInterpretation,
    competing_interpretation: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawResultInterpretation {
    supports_primary_if: String,
    supports_competing_if: String,
    inconclusive_if: Option<String>,
}

// ---------------------------------------------------------------------------
// Public module
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct ResponseValidationAndNormalization;

impl ResponseValidationAndNormalization {
    pub fn new() -> Self {
        Self
    }

    pub fn validate_and_normalize(
        &self,
        input: &LlmStructuredGenerationOutput,
    ) -> Result<ResponseValidationAndNormalizationOutput, ResponseValidationAndNormalizationError>
    {
        self.validate_and_normalize_with_context(input, &Context::noop())
    }

    pub fn validate_and_normalize_with_context(
        &self,
        input: &LlmStructuredGenerationOutput,
        context: &Context,
    ) -> Result<ResponseValidationAndNormalizationOutput, ResponseValidationAndNormalizationError>
    {
        let oi_span = crate::observability::oi_guardrail_response_validation_span(
            &context.open_inference.root_span,
        );
        let oi_input_json =
            serde_json::to_string(&input.response_json).unwrap_or_else(|_| "{}".to_string());
        oi_span.record("input.value", oi_input_json.as_str());
        oi_span.record("input.mime_type", "application/json");

        let span = info_span!(
            "request_pipeline.response_validation_and_normalization",
            "module.name" = "response_validation_and_normalization",
            "module.outcome" = field::Empty,
            "status" = field::Empty,
            "error.type" = field::Empty,
            "error.message" = field::Empty,
            "validation.input.top_level_type" = field::Empty,
            "validation.input.top_level_field_count" = field::Empty,
            "validation.required_fields.present_count" = field::Empty,
            "validation.required_fields.missing_count" = field::Empty,
            "validation.required_fields.missing" = field::Empty,
            "validation.unknown_top_level_fields_count" = field::Empty,
            "validation.unknown_top_level_fields" = field::Empty,
            "validation.result_interpretation.present" = field::Empty,
            "validation.active_hypotheses.count" = field::Empty,
            "validation.active_hypotheses.valid_count_range" = field::Empty,
            "validation.competing_interpretation.present" = field::Empty,
            "validation.inconclusive_if.present" = field::Empty,
            "validation.prohibited_final_diagnosis_language_found" = field::Empty,
            "normalization.trimmed_fields_count" = field::Empty,
            "normalization.success" = field::Empty,
        );

        let _guard = span.enter();

        // Extract metadata early to record validation attempt details
        match extract_validation_metadata(&input.response_json) {
            Ok(meta) => {
                tracing::event!(
                    tracing::Level::INFO,
                    event.name = "validation_input_payload",
                    validation.input.raw_json = %meta.input_raw_json
                );
                span.record("validation.input.top_level_type", meta.input_type.as_str());
                span.record("validation.input.top_level_field_count", meta.input_field_count);
                span.record("validation.required_fields.present_count", meta.required_present_count);
                span.record("validation.required_fields.missing_count", meta.required_missing_count);
                span.record(
                    "validation.required_fields.missing",
                    serde_json::to_string(&meta.required_missing)
                        .unwrap_or_else(|_| "[]".to_string())
                        .as_str(),
                );
                span.record("validation.unknown_top_level_fields_count", meta.unknown_fields_count);
                span.record(
                    "validation.unknown_top_level_fields",
                    serde_json::to_string(&meta.unknown_fields)
                        .unwrap_or_else(|_| "[]".to_string())
                        .as_str(),
                );
                span.record("validation.result_interpretation.present", meta.result_interpretation_present);
            }
            Err(e) => {
                span.record("status", "error");
                span.record("error.type", "ResponseValidation.InvalidResponseShape");
                span.record("error.message", e.as_str());
                span.record("module.outcome", "failure");
                crate::observability::record_error(
                    &oi_span,
                    "ResponseValidation.InvalidResponseShape",
                    e.as_str(),
                );
                return Err(ResponseValidationAndNormalizationError::InvalidResponseShape(e));
            }
        }

        check_required_keys_present(&input.response_json).map_err(|e| {
            span.record("status", "error");
            span.record("error.type", "ResponseValidation.InvalidResponseShape");
            span.record("error.message", "response JSON does not match expected shape");
            span.record("module.outcome", "failure");
            crate::observability::record_error(
                &oi_span,
                "ResponseValidation.InvalidResponseShape",
                "response JSON does not match expected shape",
            );
            e
        })?;

        let raw = serde_json::from_value::<RawDiagnosticResponse>(input.response_json.clone())
            .map_err(|_| {
                let err_msg = "response JSON does not match expected shape";
                span.record("status", "error");
                span.record("error.type", "ResponseValidation.InvalidResponseShape");
                span.record("error.message", err_msg);
                span.record("module.outcome", "failure");
                crate::observability::record_error(
                    &oi_span,
                    "ResponseValidation.InvalidResponseShape",
                    err_msg,
                );
                ResponseValidationAndNormalizationError::InvalidResponseShape(err_msg.to_string())
            })?;

        // Record hypothesis and optional field presence before business rule validation
        let active_hyp_count = filtered_active_hypotheses_count(&raw);
        let active_hyp_valid = active_hyp_count >= 2 && active_hyp_count <= 3;
        span.record("validation.active_hypotheses.count", active_hyp_count);
        span.record("validation.active_hypotheses.valid_count_range", active_hyp_valid);
        span.record("validation.competing_interpretation.present", raw.competing_interpretation.is_some());
        span.record("validation.inconclusive_if.present", raw.result_interpretation.inconclusive_if.is_some());

        apply_business_rules(&raw).map_err(|e| {
            let err_msg = match &e {
                ResponseValidationAndNormalizationError::BusinessRuleViolation(msg) => msg.clone(),
                _ => format!("{:?}", e),
            };
            let prohibited = check_prohibited_phrases_in_raw(&raw);
            span.record("validation.prohibited_final_diagnosis_language_found", prohibited);
            span.record("status", "error");
            span.record("error.type", "ResponseValidation.BusinessRuleViolation");
            span.record("error.message", err_msg.as_str());
            span.record("module.outcome", "failure");
            crate::observability::record_error(
                &oi_span,
                "ResponseValidation.BusinessRuleViolation",
                err_msg.as_str(),
            );
            e
        })?;

        let prohibited = check_prohibited_phrases_in_raw(&raw);
        span.record("validation.prohibited_final_diagnosis_language_found", prohibited);

        let trim_count = count_trimmed_fields(&raw);
        let response = normalize(raw);

        span.record("normalization.trimmed_fields_count", trim_count);
        span.record("normalization.success", true);

        if let Ok(json_str) = serde_json::to_string(&response) {
            tracing::event!(
                tracing::Level::INFO,
                event.name = "final_response_payload",
                final_response.json = %json_str
            );
            oi_span.record("output.value", json_str.as_str());
            oi_span.record("output.mime_type", "application/json");
        }

        oi_span.record("status", "ok");
        span.record("module.outcome", "success");
        span.record("status", "ok");

        Ok(ResponseValidationAndNormalizationOutput { response })
    }
}

// ---------------------------------------------------------------------------
// Required key presence check (serde treats Option<T> as optional by default)
// ---------------------------------------------------------------------------

const REQUIRED_TOP_LEVEL_KEYS: &[&str] = &[
    "problem_understanding",
    "similar_practical_context",
    "active_hypotheses",
    "first_check",
    "result_interpretation",
    "competing_interpretation",
];

const REQUIRED_NESTED_KEYS: &[&str] = &[
    "supports_primary_if",
    "supports_competing_if",
    "inconclusive_if",
];

fn check_required_keys_present(
    json: &serde_json::Value,
) -> Result<(), ResponseValidationAndNormalizationError> {
    let obj = json.as_object().ok_or(
        ResponseValidationAndNormalizationError::InvalidResponseShape(
            "response JSON does not match expected shape".to_string(),
        ),
    )?;

    for key in REQUIRED_TOP_LEVEL_KEYS {
        if !obj.contains_key(*key) {
            return Err(
                ResponseValidationAndNormalizationError::InvalidResponseShape(
                    "response JSON does not match expected shape".to_string(),
                ),
            );
        }
    }

    if let Some(ri) = obj.get("result_interpretation").and_then(|v| v.as_object()) {
        for key in REQUIRED_NESTED_KEYS {
            if !ri.contains_key(*key) {
                return Err(
                    ResponseValidationAndNormalizationError::InvalidResponseShape(
                        "response JSON does not match expected shape".to_string(),
                    ),
                );
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Business rules
// ---------------------------------------------------------------------------

const PROHIBITED_PHRASES: &[&str] = &[
    "confirms the root cause",
    "proves the diagnosis",
    "definitive root cause",
];

fn contains_prohibited_phrase(s: &str) -> bool {
    let lower = s.to_lowercase();
    PROHIBITED_PHRASES.iter().any(|p| lower.contains(p))
}

fn apply_business_rules(
    raw: &RawDiagnosticResponse,
) -> Result<(), ResponseValidationAndNormalizationError> {
    use ResponseValidationAndNormalizationError::BusinessRuleViolation;

    let hyp_len = filtered_active_hypotheses_count(raw);
    if hyp_len < 2 || hyp_len > 3 {
        return Err(BusinessRuleViolation(
            "active_hypotheses must contain 2 or 3 items".to_string(),
        ));
    }

    if raw.problem_understanding.trim().is_empty() {
        return Err(BusinessRuleViolation(
            "problem_understanding must be non-empty after trimming".to_string(),
        ));
    }
    if raw.similar_practical_context.trim().is_empty() {
        return Err(BusinessRuleViolation(
            "similar_practical_context must be non-empty after trimming".to_string(),
        ));
    }
    if raw.first_check.trim().is_empty() {
        return Err(BusinessRuleViolation(
            "first_check must be non-empty after trimming".to_string(),
        ));
    }
    if raw
        .result_interpretation
        .supports_primary_if
        .trim()
        .is_empty()
    {
        return Err(BusinessRuleViolation(
            "supports_primary_if must be non-empty after trimming".to_string(),
        ));
    }
    if raw
        .result_interpretation
        .supports_competing_if
        .trim()
        .is_empty()
    {
        return Err(BusinessRuleViolation(
            "supports_competing_if must be non-empty after trimming".to_string(),
        ));
    }

    let required_fields: &[&str] = &[
        &raw.problem_understanding,
        &raw.similar_practical_context,
        &raw.first_check,
        &raw.result_interpretation.supports_primary_if,
        &raw.result_interpretation.supports_competing_if,
    ];
    for value in required_fields {
        if contains_prohibited_phrase(value) {
            return Err(BusinessRuleViolation(
                "response contains prohibited final-diagnosis language".to_string(),
            ));
        }
    }

    if let Some(s) = &raw.result_interpretation.inconclusive_if {
        if s.trim().is_empty() {
            return Err(BusinessRuleViolation(
                "inconclusive_if must be non-empty after trimming when present".to_string(),
            ));
        }
        if contains_prohibited_phrase(s) {
            return Err(BusinessRuleViolation(
                "response contains prohibited final-diagnosis language".to_string(),
            ));
        }
    }
    if let Some(s) = &raw.competing_interpretation {
        if s.trim().is_empty() {
            return Err(BusinessRuleViolation(
                "competing_interpretation must be non-empty after trimming when present"
                    .to_string(),
            ));
        }
        if contains_prohibited_phrase(s) {
            return Err(BusinessRuleViolation(
                "response contains prohibited final-diagnosis language".to_string(),
            ));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Observability helpers
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct ValidationMetadata {
    input_raw_json: String,
    input_type: String,
    input_field_count: usize,
    required_present_count: usize,
    required_missing_count: usize,
    required_missing: Vec<String>,
    unknown_fields_count: usize,
    unknown_fields: Vec<String>,
    result_interpretation_present: bool,
}

fn extract_validation_metadata(json: &serde_json::Value) -> Result<ValidationMetadata, String> {
    let input_raw_json = serde_json::to_string(json).unwrap_or_default();
    let input_type = match json {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(_) => "boolean".to_string(),
        serde_json::Value::Number(_) => "number".to_string(),
        serde_json::Value::String(_) => "string".to_string(),
        serde_json::Value::Array(_) => "array".to_string(),
        serde_json::Value::Object(_) => "object".to_string(),
    };

    let Some(obj) = json.as_object() else {
        return Ok(ValidationMetadata {
            input_raw_json,
            input_type,
            input_field_count: 0,
            required_present_count: 0,
            required_missing_count: REQUIRED_TOP_LEVEL_KEYS.len(),
            required_missing: REQUIRED_TOP_LEVEL_KEYS.iter().map(|k| (*k).to_string()).collect(),
            unknown_fields_count: 0,
            unknown_fields: vec![],
            result_interpretation_present: false,
        });
    };

    let input_field_count = obj.len();

    let mut required_missing = Vec::new();
    for key in REQUIRED_TOP_LEVEL_KEYS {
        if !obj.contains_key(*key) {
            required_missing.push(key.to_string());
        }
    }
    let required_present_count = REQUIRED_TOP_LEVEL_KEYS.len() - required_missing.len();
    let required_missing_count = required_missing.len();

    let unknown_fields: Vec<String> = obj.keys()
        .filter(|k| !REQUIRED_TOP_LEVEL_KEYS.contains(&k.as_str()))
        .cloned()
        .collect();
    let unknown_fields_count = unknown_fields.len();

    let result_interpretation_present = obj.get("result_interpretation")
        .map(|v| v.is_object())
        .unwrap_or(false);

    Ok(ValidationMetadata {
        input_raw_json,
        input_type,
        input_field_count,
        required_present_count,
        required_missing_count,
        required_missing,
        unknown_fields_count,
        unknown_fields,
        result_interpretation_present,
    })
}

fn check_prohibited_phrases_in_raw(raw: &RawDiagnosticResponse) -> bool {
    let values = [
        &raw.problem_understanding,
        &raw.similar_practical_context,
        &raw.first_check,
        &raw.result_interpretation.supports_primary_if,
        &raw.result_interpretation.supports_competing_if,
    ];

    for value in &values {
        if contains_prohibited_phrase(value) {
            return true;
        }
    }

    if let Some(s) = &raw.competing_interpretation {
        if contains_prohibited_phrase(s) {
            return true;
        }
    }

    if let Some(s) = &raw.result_interpretation.inconclusive_if {
        if contains_prohibited_phrase(s) {
            return true;
        }
    }

    false
}

fn count_trimmed_fields(raw: &RawDiagnosticResponse) -> usize {
    let mut count = 0;

    if raw.problem_understanding.as_str() != raw.problem_understanding.trim() {
        count += 1;
    }
    if raw.similar_practical_context.as_str() != raw.similar_practical_context.trim() {
        count += 1;
    }
    if raw.first_check.as_str() != raw.first_check.trim() {
        count += 1;
    }
    if raw.result_interpretation.supports_primary_if.as_str() != raw.result_interpretation.supports_primary_if.trim() {
        count += 1;
    }
    if raw.result_interpretation.supports_competing_if.as_str() != raw.result_interpretation.supports_competing_if.trim() {
        count += 1;
    }

    for h in &raw.active_hypotheses {
        if h.as_str() != h.trim() {
            count += 1;
        }
    }

    if let Some(s) = &raw.competing_interpretation {
        if s.as_str() != s.trim() {
            count += 1;
        }
    }

    if let Some(s) = &raw.result_interpretation.inconclusive_if {
        if s.as_str() != s.trim() {
            count += 1;
        }
    }

    count
}

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

fn normalize_nullable(s: Option<String>) -> Option<String> {
    s.and_then(|v| {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn normalize(raw: RawDiagnosticResponse) -> DiagnosticResponse {
    DiagnosticResponse {
        problem_understanding: raw.problem_understanding.trim().to_string(),
        similar_practical_context: raw.similar_practical_context.trim().to_string(),
        active_hypotheses: raw
            .active_hypotheses
            .into_iter()
            .map(|h| h.trim().to_string())
            .filter(|h| !h.is_empty())
            .collect(),
        first_check: raw.first_check.trim().to_string(),
        result_interpretation: DiagnosticResultInterpretation {
            supports_primary_if: raw
                .result_interpretation
                .supports_primary_if
                .trim()
                .to_string(),
            supports_competing_if: raw
                .result_interpretation
                .supports_competing_if
                .trim()
                .to_string(),
            inconclusive_if: normalize_nullable(raw.result_interpretation.inconclusive_if),
        },
        competing_interpretation: normalize_nullable(raw.competing_interpretation),
    }
}

fn filtered_active_hypotheses_count(raw: &RawDiagnosticResponse) -> usize {
    raw.active_hypotheses
        .iter()
        .filter(|h| !h.trim().is_empty())
        .count()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_types::ModelTokenUsage;
    use serde_json::json;

    fn make() -> ResponseValidationAndNormalization {
        ResponseValidationAndNormalization::new()
    }

    fn make_input(json: serde_json::Value) -> LlmStructuredGenerationOutput {
        LlmStructuredGenerationOutput {
            response_json: json,
            token_usage: ModelTokenUsage {
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
            },
        }
    }

    fn valid_json() -> serde_json::Value {
        json!({
            "problem_understanding": "The service is experiencing high latency under load",
            "similar_practical_context": "This resembles a GC pause pattern seen in heap-constrained JVMs",
            "active_hypotheses": [
                "Hypothesis A: heap exhaustion causing full GC pauses",
                "Hypothesis B: lock contention on shared connection pool"
            ],
            "first_check": "Examine GC pause duration histogram in the last 30 minutes",
            "result_interpretation": {
                "supports_primary_if": "GC pause duration exceeds 200ms",
                "supports_competing_if": "Lock wait time exceeds 100ms with normal GC",
                "inconclusive_if": "Both metrics are within their normal baselines"
            },
            "competing_interpretation": "Could indicate network saturation upstream"
        })
    }

    // -----------------------------------------------------------------------
    // Constructor
    // -----------------------------------------------------------------------

    #[test]
    fn new_returns_unit_struct() {
        let _ = ResponseValidationAndNormalization::new();
    }

    // -----------------------------------------------------------------------
    // Shape validation — missing required fields
    // -----------------------------------------------------------------------

    #[test]
    fn shape_rejects_missing_problem_understanding() {
        let mut j = valid_json();
        j.as_object_mut().unwrap().remove("problem_understanding");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::InvalidResponseShape(_)
            ),
            "expected InvalidResponseShape, got: {err}"
        );
    }

    #[test]
    fn shape_rejects_missing_active_hypotheses() {
        let mut j = valid_json();
        j.as_object_mut().unwrap().remove("active_hypotheses");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::InvalidResponseShape(_)
            ),
            "expected InvalidResponseShape, got: {err}"
        );
    }

    #[test]
    fn shape_rejects_missing_result_interpretation() {
        let mut j = valid_json();
        j.as_object_mut().unwrap().remove("result_interpretation");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::InvalidResponseShape(_)
            ),
            "expected InvalidResponseShape, got: {err}"
        );
    }

    #[test]
    fn shape_rejects_missing_competing_interpretation_key() {
        let mut j = valid_json();
        j.as_object_mut()
            .unwrap()
            .remove("competing_interpretation");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::InvalidResponseShape(_)
            ),
            "expected InvalidResponseShape for missing competing_interpretation, got: {err}"
        );
    }

    #[test]
    fn shape_rejects_missing_supports_primary_if() {
        let mut j = valid_json();
        j["result_interpretation"]
            .as_object_mut()
            .unwrap()
            .remove("supports_primary_if");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::InvalidResponseShape(_)
            ),
            "expected InvalidResponseShape, got: {err}"
        );
    }

    #[test]
    fn shape_rejects_missing_inconclusive_if_key() {
        let mut j = valid_json();
        j["result_interpretation"]
            .as_object_mut()
            .unwrap()
            .remove("inconclusive_if");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::InvalidResponseShape(_)
            ),
            "expected InvalidResponseShape for missing inconclusive_if key, got: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // Shape validation — unknown fields
    // -----------------------------------------------------------------------

    #[test]
    fn shape_rejects_unknown_top_level_field() {
        let mut j = valid_json();
        j["extra_unknown_field"] = json!("should not be here");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::InvalidResponseShape(_)
            ),
            "expected InvalidResponseShape for unknown top-level field, got: {err}"
        );
    }

    #[test]
    fn shape_rejects_unknown_nested_field_in_result_interpretation() {
        let mut j = valid_json();
        j["result_interpretation"]["unexpected_key"] = json!("oops");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::InvalidResponseShape(_)
            ),
            "expected InvalidResponseShape for unknown nested field, got: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // Shape validation — wrong types
    // -----------------------------------------------------------------------

    #[test]
    fn shape_rejects_number_for_problem_understanding() {
        let mut j = valid_json();
        j["problem_understanding"] = json!(42);
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::InvalidResponseShape(_)
            ),
            "expected InvalidResponseShape for wrong type, got: {err}"
        );
    }

    #[test]
    fn shape_rejects_string_for_active_hypotheses() {
        let mut j = valid_json();
        j["active_hypotheses"] = json!("should be an array");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::InvalidResponseShape(_)
            ),
            "expected InvalidResponseShape for string where array expected, got: {err}"
        );
    }

    #[test]
    fn shape_rejects_non_string_item_in_active_hypotheses() {
        let mut j = valid_json();
        j["active_hypotheses"] = json!([1, 2]);
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::InvalidResponseShape(_)
            ),
            "expected InvalidResponseShape for non-string array items, got: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // Shape validation — nullable fields accepted
    // -----------------------------------------------------------------------

    #[test]
    fn shape_accepts_null_competing_interpretation() {
        let mut j = valid_json();
        j["competing_interpretation"] = json!(null);
        assert!(make().validate_and_normalize(&make_input(j)).is_ok());
    }

    #[test]
    fn shape_accepts_null_inconclusive_if() {
        let mut j = valid_json();
        j["result_interpretation"]["inconclusive_if"] = json!(null);
        assert!(make().validate_and_normalize(&make_input(j)).is_ok());
    }

    // -----------------------------------------------------------------------
    // Business rules — active_hypotheses count
    // -----------------------------------------------------------------------

    #[test]
    fn business_accepts_two_hypotheses() {
        let j = valid_json(); // already has 2
        assert!(make().validate_and_normalize(&make_input(j)).is_ok());
    }

    #[test]
    fn business_accepts_three_hypotheses() {
        let mut j = valid_json();
        j["active_hypotheses"] = json!([
            "Hypothesis A: memory pressure",
            "Hypothesis B: lock contention",
            "Hypothesis C: network saturation"
        ]);
        assert!(make().validate_and_normalize(&make_input(j)).is_ok());
    }

    #[test]
    fn business_rejects_one_hypothesis() {
        let mut j = valid_json();
        j["active_hypotheses"] = json!(["Only one hypothesis"]);
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::BusinessRuleViolation(_)
            ),
            "expected BusinessRuleViolation for one hypothesis, got: {err}"
        );
    }

    #[test]
    fn business_rejects_four_hypotheses() {
        let mut j = valid_json();
        j["active_hypotheses"] = json!(["H1", "H2", "H3", "H4"]);
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::BusinessRuleViolation(_)
            ),
            "expected BusinessRuleViolation for four hypotheses, got: {err}"
        );
    }

    #[test]
    fn business_rejects_zero_hypotheses() {
        let mut j = valid_json();
        j["active_hypotheses"] = json!([]);
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::BusinessRuleViolation(_)
            ),
            "expected BusinessRuleViolation for empty hypotheses, got: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // Business rules — empty / whitespace-only required strings
    // -----------------------------------------------------------------------

    #[test]
    fn business_rejects_empty_problem_understanding() {
        let mut j = valid_json();
        j["problem_understanding"] = json!("");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::BusinessRuleViolation(_)
            ),
            "expected BusinessRuleViolation, got: {err}"
        );
    }

    #[test]
    fn business_rejects_whitespace_only_problem_understanding() {
        let mut j = valid_json();
        j["problem_understanding"] = json!("   \t  ");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::BusinessRuleViolation(_)
            ),
            "expected BusinessRuleViolation for whitespace-only, got: {err}"
        );
    }

    #[test]
    fn business_rejects_empty_similar_practical_context() {
        let mut j = valid_json();
        j["similar_practical_context"] = json!("");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::BusinessRuleViolation(_)
            ),
            "expected BusinessRuleViolation, got: {err}"
        );
    }

    #[test]
    fn business_rejects_empty_first_check() {
        let mut j = valid_json();
        j["first_check"] = json!("");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::BusinessRuleViolation(_)
            ),
            "expected BusinessRuleViolation, got: {err}"
        );
    }

    #[test]
    fn business_rejects_empty_supports_primary_if() {
        let mut j = valid_json();
        j["result_interpretation"]["supports_primary_if"] = json!("");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::BusinessRuleViolation(_)
            ),
            "expected BusinessRuleViolation, got: {err}"
        );
    }

    #[test]
    fn business_rejects_empty_supports_competing_if() {
        let mut j = valid_json();
        j["result_interpretation"]["supports_competing_if"] = json!("");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::BusinessRuleViolation(_)
            ),
            "expected BusinessRuleViolation, got: {err}"
        );
    }

    #[test]
    fn business_rejects_whitespace_only_active_hypothesis() {
        let mut j = valid_json();
        j["active_hypotheses"] = json!(["Valid hypothesis", "   "]);
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::BusinessRuleViolation(_)
            ),
            "expected BusinessRuleViolation for whitespace hypothesis, got: {err}"
        );
    }

    #[test]
    fn business_filters_empty_hypothesis_before_count_check() {
        let mut j = valid_json();
        j["active_hypotheses"] = json!([
            "The lock service may allow multiple holders at once",
            "A lease may expire while a client still believes it holds the lock",
            "The application might be misusing the lock",
            ""
        ]);
        assert!(make().validate_and_normalize(&make_input(j)).is_ok());
    }

    // -----------------------------------------------------------------------
    // Business rules — prohibited phrases
    // -----------------------------------------------------------------------

    #[test]
    fn business_rejects_confirms_root_cause_in_problem_understanding() {
        let mut j = valid_json();
        j["problem_understanding"] = json!("This confirms the root cause of the failure");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::BusinessRuleViolation(_)
            ),
            "expected BusinessRuleViolation for prohibited phrase, got: {err}"
        );
    }

    #[test]
    fn business_rejects_proves_diagnosis_in_first_check() {
        let mut j = valid_json();
        j["first_check"] = json!("Running this test proves the diagnosis");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::BusinessRuleViolation(_)
            ),
            "expected BusinessRuleViolation for prohibited phrase, got: {err}"
        );
    }

    #[test]
    fn business_rejects_definitive_root_cause_in_supports_primary_if() {
        let mut j = valid_json();
        j["result_interpretation"]["supports_primary_if"] =
            json!("This identifies the definitive root cause");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::BusinessRuleViolation(_)
            ),
            "expected BusinessRuleViolation for prohibited phrase, got: {err}"
        );
    }

    #[test]
    fn business_rejects_prohibited_phrase_case_insensitive() {
        let mut j = valid_json();
        j["problem_understanding"] = json!("This CONFIRMS THE ROOT CAUSE clearly");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::BusinessRuleViolation(_)
            ),
            "expected BusinessRuleViolation for case-insensitive phrase, got: {err}"
        );
    }

    #[test]
    fn business_rejects_prohibited_phrase_in_supports_competing_if() {
        let mut j = valid_json();
        j["result_interpretation"]["supports_competing_if"] =
            json!("Confirms the root cause of secondary issue");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::BusinessRuleViolation(_)
            ),
            "expected BusinessRuleViolation, got: {err}"
        );
    }

    #[test]
    fn business_rejects_prohibited_phrase_in_competing_interpretation() {
        let mut j = valid_json();
        j["competing_interpretation"] = json!("This proves the diagnosis conclusively");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::BusinessRuleViolation(_)
            ),
            "expected BusinessRuleViolation, got: {err}"
        );
    }

    #[test]
    fn business_rejects_prohibited_phrase_in_inconclusive_if() {
        let mut j = valid_json();
        j["result_interpretation"]["inconclusive_if"] =
            json!("This is the definitive root cause scenario");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::BusinessRuleViolation(_)
            ),
            "expected BusinessRuleViolation, got: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // Normalization — trimming
    // -----------------------------------------------------------------------

    #[test]
    fn normalization_trims_problem_understanding() {
        let mut j = valid_json();
        j["problem_understanding"] = json!("  trimmed text  ");
        let out = make().validate_and_normalize(&make_input(j)).unwrap();
        assert_eq!(out.response.problem_understanding, "trimmed text");
    }

    #[test]
    fn normalization_trims_first_check() {
        let mut j = valid_json();
        j["first_check"] = json!("\tcheck GC logs\n");
        let out = make().validate_and_normalize(&make_input(j)).unwrap();
        assert_eq!(out.response.first_check, "check GC logs");
    }

    #[test]
    fn normalization_trims_active_hypotheses_items() {
        let mut j = valid_json();
        j["active_hypotheses"] = json!([" Hypothesis A ", "  Hypothesis B  "]);
        let out = make().validate_and_normalize(&make_input(j)).unwrap();
        assert_eq!(
            out.response.active_hypotheses,
            vec!["Hypothesis A", "Hypothesis B"]
        );
    }

    #[test]
    fn normalization_filters_whitespace_only_active_hypotheses_items() {
        let mut j = valid_json();
        j["active_hypotheses"] = json!([" Hypothesis A ", "   ", "  Hypothesis B  "]);
        let out = make().validate_and_normalize(&make_input(j)).unwrap();
        assert_eq!(
            out.response.active_hypotheses,
            vec!["Hypothesis A", "Hypothesis B"]
        );
    }

    #[test]
    fn normalization_preserves_hypothesis_order() {
        let mut j = valid_json();
        j["active_hypotheses"] = json!(["First", "Second", "Third"]);
        let out = make().validate_and_normalize(&make_input(j)).unwrap();
        assert_eq!(
            out.response.active_hypotheses,
            vec!["First", "Second", "Third"]
        );
    }

    // -----------------------------------------------------------------------
    // Normalization — nullable fields
    // -----------------------------------------------------------------------

    #[test]
    fn normalization_null_competing_interpretation_maps_to_none() {
        let mut j = valid_json();
        j["competing_interpretation"] = json!(null);
        let out = make().validate_and_normalize(&make_input(j)).unwrap();
        assert_eq!(out.response.competing_interpretation, None);
    }

    #[test]
    fn business_rejects_whitespace_only_competing_interpretation() {
        let mut j = valid_json();
        j["competing_interpretation"] = json!("   ");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(err, ResponseValidationAndNormalizationError::BusinessRuleViolation(_)),
            "expected BusinessRuleViolation for whitespace-only competing_interpretation, got: {err}"
        );
    }

    #[test]
    fn normalization_non_empty_competing_interpretation_preserved_and_trimmed() {
        let mut j = valid_json();
        j["competing_interpretation"] = json!("  network saturation  ");
        let out = make().validate_and_normalize(&make_input(j)).unwrap();
        assert_eq!(
            out.response.competing_interpretation,
            Some("network saturation".to_string())
        );
    }

    #[test]
    fn normalization_null_inconclusive_if_maps_to_none() {
        let mut j = valid_json();
        j["result_interpretation"]["inconclusive_if"] = json!(null);
        let out = make().validate_and_normalize(&make_input(j)).unwrap();
        assert_eq!(out.response.result_interpretation.inconclusive_if, None);
    }

    #[test]
    fn business_rejects_whitespace_only_inconclusive_if() {
        let mut j = valid_json();
        j["result_interpretation"]["inconclusive_if"] = json!("  \t  ");
        let err = make().validate_and_normalize(&make_input(j)).unwrap_err();
        assert!(
            matches!(
                err,
                ResponseValidationAndNormalizationError::BusinessRuleViolation(_)
            ),
            "expected BusinessRuleViolation for whitespace-only inconclusive_if, got: {err}"
        );
    }

    #[test]
    fn normalization_non_empty_inconclusive_if_preserved_and_trimmed() {
        let mut j = valid_json();
        j["result_interpretation"]["inconclusive_if"] = json!(" metrics within normal range ");
        let out = make().validate_and_normalize(&make_input(j)).unwrap();
        assert_eq!(
            out.response.result_interpretation.inconclusive_if,
            Some("metrics within normal range".to_string())
        );
    }

    // -----------------------------------------------------------------------
    // Behavioral invariants
    // -----------------------------------------------------------------------

    #[test]
    fn token_usage_does_not_affect_validation_result() {
        let j = valid_json();
        let input_no_tokens = LlmStructuredGenerationOutput {
            response_json: j.clone(),
            token_usage: ModelTokenUsage {
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
            },
        };
        let input_with_tokens = LlmStructuredGenerationOutput {
            response_json: j,
            token_usage: ModelTokenUsage {
                prompt_tokens: Some(100),
                completion_tokens: Some(50),
                total_tokens: Some(150),
            },
        };
        let out1 = make().validate_and_normalize(&input_no_tokens).unwrap();
        let out2 = make().validate_and_normalize(&input_with_tokens).unwrap();
        assert_eq!(out1, out2);
    }

    #[test]
    fn identical_input_produces_identical_output() {
        let j = valid_json();
        let input = make_input(j);
        let out1 = make().validate_and_normalize(&input).unwrap();
        let out2 = make().validate_and_normalize(&input).unwrap();
        assert_eq!(out1, out2);
    }

    // -----------------------------------------------------------------------
    // Happy path
    // -----------------------------------------------------------------------

    #[test]
    fn validate_and_normalize_happy_path() {
        let out = make()
            .validate_and_normalize(&make_input(valid_json()))
            .unwrap();
        assert!(!out.response.problem_understanding.is_empty());
        assert!(!out.response.similar_practical_context.is_empty());
        assert_eq!(out.response.active_hypotheses.len(), 2);
        assert!(!out.response.first_check.is_empty());
        assert!(!out
            .response
            .result_interpretation
            .supports_primary_if
            .is_empty());
        assert!(!out
            .response
            .result_interpretation
            .supports_competing_if
            .is_empty());
        assert!(out.response.result_interpretation.inconclusive_if.is_some());
        assert!(out.response.competing_interpretation.is_some());
    }
}
