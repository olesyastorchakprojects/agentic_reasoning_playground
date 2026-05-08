use serde::Deserialize;
use tracing::{field, info_span};
use uuid::Uuid;

use crate::shared_types::{
    Confidence, Context, DiagnosticResponse, DiagnosticResultInterpretation, Hypothesis,
    HypothesisEvidenceSource, HypothesisId, HypothesisStatus, LlmStructuredGenerationOutput,
    ResponseValidationAndNormalizationOutput,
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
struct RawHypothesis {
    id: String,
    text: String,
    status: String,
    rejection_reason: Option<String>,
    source: String,
    confidence: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDiagnosticResponse {
    problem_understanding: String,
    similar_practical_context: String,
    hypotheses: Vec<RawHypothesis>,
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
            "validation.hypotheses.count" = field::Empty,
            "validation.hypotheses.valid_count_range" = field::Empty,
            "validation.inconclusive_if.present" = field::Empty,
            "validation.prohibited_final_diagnosis_language_found" = field::Empty,
            "normalization.trimmed_fields_count" = field::Empty,
            "normalization.success" = field::Empty,
        );

        let _guard = span.enter();

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
                span.record(
                    "validation.result_interpretation.present",
                    meta.result_interpretation_present,
                );
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

        let hyp_count = filtered_hypotheses_count(&raw);
        let hyp_valid = hyp_count >= 2 && hyp_count <= 3;
        span.record("validation.hypotheses.count", hyp_count);
        span.record("validation.hypotheses.valid_count_range", hyp_valid);
        span.record(
            "validation.inconclusive_if.present",
            raw.result_interpretation.inconclusive_if.is_some(),
        );

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
        let response = normalize(raw).map_err(|e| {
            let err_msg = match &e {
                ResponseValidationAndNormalizationError::BusinessRuleViolation(msg) => msg.clone(),
                _ => format!("{:?}", e),
            };
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
// Required key presence check
// ---------------------------------------------------------------------------

const REQUIRED_TOP_LEVEL_KEYS: &[&str] = &[
    "problem_understanding",
    "similar_practical_context",
    "hypotheses",
    "first_check",
    "result_interpretation",
    "competing_interpretation",
];

const REQUIRED_NESTED_KEYS: &[&str] = &[
    "supports_primary_if",
    "supports_competing_if",
    "inconclusive_if",
];

const VALID_HYPOTHESIS_STATUSES: &[&str] = &["active", "weakened", "rejected"];
const VALID_HYPOTHESIS_SOURCES: &[&str] =
    &["primary_incident", "alternative_context", "theory_mechanism"];
const VALID_HYPOTHESIS_CONFIDENCES: &[&str] = &["low", "medium", "high"];

fn check_required_keys_present(
    json: &serde_json::Value,
) -> Result<(), ResponseValidationAndNormalizationError> {
    let obj = json
        .as_object()
        .ok_or(ResponseValidationAndNormalizationError::InvalidResponseShape(
            "response JSON does not match expected shape".to_string(),
        ))?;

    for key in REQUIRED_TOP_LEVEL_KEYS {
        if !obj.contains_key(*key) {
            return Err(ResponseValidationAndNormalizationError::InvalidResponseShape(
                "response JSON does not match expected shape".to_string(),
            ));
        }
    }

    if let Some(ri) = obj.get("result_interpretation").and_then(|v| v.as_object()) {
        for key in REQUIRED_NESTED_KEYS {
            if !ri.contains_key(*key) {
                return Err(ResponseValidationAndNormalizationError::InvalidResponseShape(
                    "response JSON does not match expected shape".to_string(),
                ));
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

    let hyp_len = filtered_hypotheses_count(raw);
    if hyp_len < 2 || hyp_len > 3 {
        return Err(BusinessRuleViolation(
            "hypotheses must contain between 2 and 3 items".to_string(),
        ));
    }

    for hyp in &raw.hypotheses {
        if hyp.text.trim().is_empty() {
            return Err(BusinessRuleViolation(
                "hypothesis text must be non-empty after trimming".to_string(),
            ));
        }
        Uuid::parse_str(&hyp.id).map_err(|_| {
            BusinessRuleViolation(format!("hypothesis id is not a valid UUID: '{}'", hyp.id))
        })?;
        if !VALID_HYPOTHESIS_STATUSES.contains(&hyp.status.as_str()) {
            return Err(BusinessRuleViolation(format!(
                "invalid hypothesis status: '{}', must be one of: active, weakened, rejected",
                hyp.status
            )));
        }
        if !VALID_HYPOTHESIS_SOURCES.contains(&hyp.source.as_str()) {
            return Err(BusinessRuleViolation(format!(
                "invalid hypothesis source: '{}', must be one of: primary_incident, alternative_context, theory_mechanism",
                hyp.source
            )));
        }
        if !VALID_HYPOTHESIS_CONFIDENCES.contains(&hyp.confidence.as_str()) {
            return Err(BusinessRuleViolation(format!(
                "invalid hypothesis confidence: '{}', must be one of: low, medium, high",
                hyp.confidence
            )));
        }
        if hyp.status == "rejected" {
            match &hyp.rejection_reason {
                None => {
                    return Err(BusinessRuleViolation(
                        "rejection_reason must be non-empty when status is rejected".to_string(),
                    ))
                }
                Some(r) if r.trim().is_empty() => {
                    return Err(BusinessRuleViolation(
                        "rejection_reason must be non-empty when status is rejected".to_string(),
                    ))
                }
                _ => {}
            }
        } else if hyp.rejection_reason.is_some() {
            return Err(BusinessRuleViolation(
                "rejection_reason must be null when status is not rejected".to_string(),
            ));
        }
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
    if raw.result_interpretation.supports_primary_if.trim().is_empty() {
        return Err(BusinessRuleViolation(
            "supports_primary_if must be non-empty after trimming".to_string(),
        ));
    }
    if raw.result_interpretation.supports_competing_if.trim().is_empty() {
        return Err(BusinessRuleViolation(
            "supports_competing_if must be non-empty after trimming".to_string(),
        ));
    }
    if let Some(s) = &raw.competing_interpretation {
        if s.trim().is_empty() {
            return Err(BusinessRuleViolation(
                "competing_interpretation must be non-empty after trimming when present".to_string(),
            ));
        }
        if contains_prohibited_phrase(s) {
            return Err(BusinessRuleViolation(
                "response contains prohibited final-diagnosis language".to_string(),
            ));
        }
    }

    let string_fields: &[&str] = &[
        &raw.problem_understanding,
        &raw.similar_practical_context,
        &raw.first_check,
        &raw.result_interpretation.supports_primary_if,
        &raw.result_interpretation.supports_competing_if,
    ];
    for value in string_fields {
        if contains_prohibited_phrase(value) {
            return Err(BusinessRuleViolation(
                "response contains prohibited final-diagnosis language".to_string(),
            ));
        }
    }
    for hyp in &raw.hypotheses {
        if contains_prohibited_phrase(&hyp.text) {
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

    let unknown_fields: Vec<String> = obj
        .keys()
        .filter(|k| !REQUIRED_TOP_LEVEL_KEYS.contains(&k.as_str()))
        .cloned()
        .collect();
    let unknown_fields_count = unknown_fields.len();

    let result_interpretation_present = obj
        .get("result_interpretation")
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
    let fields = [
        &raw.problem_understanding,
        &raw.similar_practical_context,
        &raw.first_check,
        &raw.result_interpretation.supports_primary_if,
        &raw.result_interpretation.supports_competing_if,
    ];
    for f in &fields {
        if contains_prohibited_phrase(f) {
            return true;
        }
    }
    for hyp in &raw.hypotheses {
        if contains_prohibited_phrase(&hyp.text) {
            return true;
        }
    }
    if let Some(s) = &raw.result_interpretation.inconclusive_if {
        if contains_prohibited_phrase(s) {
            return true;
        }
    }
    if let Some(s) = &raw.competing_interpretation {
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
    if raw.result_interpretation.supports_primary_if.as_str()
        != raw.result_interpretation.supports_primary_if.trim()
    {
        count += 1;
    }
    if raw.result_interpretation.supports_competing_if.as_str()
        != raw.result_interpretation.supports_competing_if.trim()
    {
        count += 1;
    }
    for h in &raw.hypotheses {
        if h.text.as_str() != h.text.trim() {
            count += 1;
        }
    }
    if let Some(s) = &raw.result_interpretation.inconclusive_if {
        if s.as_str() != s.trim() {
            count += 1;
        }
    }
    if let Some(s) = &raw.competing_interpretation {
        if s.as_str() != s.trim() {
            count += 1;
        }
    }
    count
}

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

fn parse_hypothesis_source(s: &str) -> HypothesisEvidenceSource {
    match s {
        "alternative_context" => HypothesisEvidenceSource::AlternativeContext,
        "theory_mechanism" => HypothesisEvidenceSource::TheoryMechanism,
        _ => HypothesisEvidenceSource::PrimaryIncident,
    }
}

fn parse_hypothesis_confidence(s: &str) -> Confidence {
    match s {
        "low" => Confidence::Low,
        "high" => Confidence::High,
        _ => Confidence::Medium,
    }
}

fn parse_hypothesis_status(
    status: &str,
    rejection_reason: Option<String>,
) -> HypothesisStatus {
    match status {
        "weakened" => HypothesisStatus::Weakened,
        "rejected" => HypothesisStatus::Rejected(
            rejection_reason.unwrap_or_default().trim().to_string(),
        ),
        _ => HypothesisStatus::Active,
    }
}

fn normalize_nullable(s: Option<String>) -> Option<String> {
    s.and_then(|v| {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    })
}

fn normalize(
    raw: RawDiagnosticResponse,
) -> Result<DiagnosticResponse, ResponseValidationAndNormalizationError> {
    use ResponseValidationAndNormalizationError::BusinessRuleViolation;

    let mut hypotheses = Vec::with_capacity(raw.hypotheses.len());
    for h in raw.hypotheses {
        if h.text.trim().is_empty() {
            continue;
        }
        let uuid = Uuid::parse_str(&h.id).map_err(|_| {
            BusinessRuleViolation(format!("hypothesis id is not a valid UUID: '{}'", h.id))
        })?;
        hypotheses.push(Hypothesis {
            id: HypothesisId(uuid),
            text: h.text.trim().to_string(),
            status: parse_hypothesis_status(&h.status, h.rejection_reason),
            source: parse_hypothesis_source(&h.source),
            confidence: parse_hypothesis_confidence(&h.confidence),
        });
    }

    Ok(DiagnosticResponse {
        problem_understanding: raw.problem_understanding.trim().to_string(),
        similar_practical_context: raw.similar_practical_context.trim().to_string(),
        hypotheses,
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
    })
}

fn filtered_hypotheses_count(raw: &RawDiagnosticResponse) -> usize {
    raw.hypotheses.iter().filter(|h| !h.text.trim().is_empty()).count()
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

    fn new_uuid() -> String {
        Uuid::new_v4().to_string()
    }

    fn valid_json() -> serde_json::Value {
        json!({
            "problem_understanding": "The service is experiencing high latency under load",
            "similar_practical_context": "This resembles a GC pause pattern seen in heap-constrained JVMs",
            "hypotheses": [
                {
                    "id": new_uuid(),
                    "text": "Heap exhaustion causing full GC pauses",
                    "status": "active",
                    "rejection_reason": null,
                    "source": "primary_incident",
                    "confidence": "medium"
                },
                {
                    "id": new_uuid(),
                    "text": "Lock contention on shared connection pool",
                    "status": "weakened",
                    "rejection_reason": null,
                    "source": "alternative_context",
                    "confidence": "low"
                }
            ],
            "first_check": "Examine GC pause duration histogram in the last 30 minutes",
            "result_interpretation": {
                "supports_primary_if": "GC pause duration exceeds 200ms",
                "supports_competing_if": "Lock wait time exceeds 100ms with normal GC",
                "inconclusive_if": "Both metrics are within their normal baselines"
            },
            "competing_interpretation": null
        })
    }

    // -----------------------------------------------------------------------
    // Constructor
    // -----------------------------------------------------------------------

    #[test]
    fn new_returns_unit_struct() {
        let _ = make();
    }

    // -----------------------------------------------------------------------
    // Shape validation — missing required top-level fields
    // -----------------------------------------------------------------------

    #[test]
    fn missing_problem_understanding_fails_shape() {
        let mut json = valid_json();
        json.as_object_mut().unwrap().remove("problem_understanding");
        let result = make().validate_and_normalize(&make_input(json));
        assert!(matches!(
            result,
            Err(ResponseValidationAndNormalizationError::InvalidResponseShape(_))
        ));
    }

    #[test]
    fn missing_hypotheses_fails_shape() {
        let mut json = valid_json();
        json.as_object_mut().unwrap().remove("hypotheses");
        let result = make().validate_and_normalize(&make_input(json));
        assert!(matches!(
            result,
            Err(ResponseValidationAndNormalizationError::InvalidResponseShape(_))
        ));
    }

    #[test]
    fn unknown_top_level_field_fails_shape() {
        let mut json = valid_json();
        json.as_object_mut()
            .unwrap()
            .insert("extra_field".to_string(), json!("unexpected"));
        let result = make().validate_and_normalize(&make_input(json));
        assert!(matches!(
            result,
            Err(ResponseValidationAndNormalizationError::InvalidResponseShape(_))
        ));
    }

    #[test]
    fn non_object_top_level_fails_shape() {
        let result = make().validate_and_normalize(&make_input(json!("not an object")));
        assert!(matches!(
            result,
            Err(ResponseValidationAndNormalizationError::InvalidResponseShape(_))
        ));
    }

    // -----------------------------------------------------------------------
    // Hypothesis validation
    // -----------------------------------------------------------------------

    #[test]
    fn invalid_hypothesis_id_fails_business_rule() {
        let mut json = valid_json();
        json["hypotheses"][0]["id"] = json!("not-a-uuid");
        let result = make().validate_and_normalize(&make_input(json));
        assert!(matches!(
            result,
            Err(ResponseValidationAndNormalizationError::BusinessRuleViolation(_))
        ));
    }

    #[test]
    fn invalid_hypothesis_status_fails_business_rule() {
        let mut json = valid_json();
        json["hypotheses"][0]["status"] = json!("confirmed");
        let result = make().validate_and_normalize(&make_input(json));
        assert!(matches!(
            result,
            Err(ResponseValidationAndNormalizationError::BusinessRuleViolation(_))
        ));
    }

    #[test]
    fn rejected_status_without_reason_fails_business_rule() {
        let mut json = valid_json();
        json["hypotheses"][0]["status"] = json!("rejected");
        json["hypotheses"][0]["rejection_reason"] = json!(null);
        let result = make().validate_and_normalize(&make_input(json));
        assert!(matches!(
            result,
            Err(ResponseValidationAndNormalizationError::BusinessRuleViolation(_))
        ));
    }

    #[test]
    fn non_rejected_status_with_reason_fails_business_rule() {
        let mut json = valid_json();
        json["hypotheses"][0]["status"] = json!("active");
        json["hypotheses"][0]["rejection_reason"] = json!("some reason");
        let result = make().validate_and_normalize(&make_input(json));
        assert!(matches!(
            result,
            Err(ResponseValidationAndNormalizationError::BusinessRuleViolation(_))
        ));
    }

    #[test]
    fn zero_hypotheses_fails_business_rule() {
        let mut json = valid_json();
        json["hypotheses"] = json!([]);
        let result = make().validate_and_normalize(&make_input(json));
        assert!(matches!(
            result,
            Err(ResponseValidationAndNormalizationError::BusinessRuleViolation(_))
        ));
    }

    #[test]
    fn four_hypotheses_fails_business_rule() {
        let uuid = new_uuid();
        let mut json = valid_json();
        json["hypotheses"] = json!([
            {"id": uuid, "text": "H1", "status": "active", "rejection_reason": null, "source": "primary_incident", "confidence": "low"},
            {"id": Uuid::new_v4().to_string(), "text": "H2", "status": "active", "rejection_reason": null, "source": "primary_incident", "confidence": "low"},
            {"id": Uuid::new_v4().to_string(), "text": "H3", "status": "active", "rejection_reason": null, "source": "primary_incident", "confidence": "low"},
            {"id": Uuid::new_v4().to_string(), "text": "H4", "status": "active", "rejection_reason": null, "source": "primary_incident", "confidence": "low"},
        ]);
        let result = make().validate_and_normalize(&make_input(json));
        assert!(matches!(
            result,
            Err(ResponseValidationAndNormalizationError::BusinessRuleViolation(_))
        ));
    }

    #[test]
    fn two_hypotheses_succeeds() {
        let mut json = valid_json();
        json["hypotheses"] = json!([
            {"id": new_uuid(), "text": "First hypothesis", "status": "active", "rejection_reason": null, "source": "primary_incident", "confidence": "high"},
            {"id": new_uuid(), "text": "Second hypothesis", "status": "active", "rejection_reason": null, "source": "alternative_context", "confidence": "low"},
        ]);
        assert!(make().validate_and_normalize(&make_input(json)).is_ok());
    }

    #[test]
    fn one_hypothesis_fails_business_rule() {
        let mut json = valid_json();
        json["hypotheses"] = json!([
            {"id": new_uuid(), "text": "Single hypothesis", "status": "active", "rejection_reason": null, "source": "primary_incident", "confidence": "high"},
        ]);
        assert!(matches!(
            make().validate_and_normalize(&make_input(json)),
            Err(ResponseValidationAndNormalizationError::BusinessRuleViolation(_))
        ));
    }

    // -----------------------------------------------------------------------
    // Prohibited language
    // -----------------------------------------------------------------------

    #[test]
    fn prohibited_phrase_in_problem_understanding_fails() {
        let mut json = valid_json();
        json["problem_understanding"] =
            json!("This confirms the root cause of the issue");
        let result = make().validate_and_normalize(&make_input(json));
        assert!(matches!(
            result,
            Err(ResponseValidationAndNormalizationError::BusinessRuleViolation(_))
        ));
    }

    // -----------------------------------------------------------------------
    // Successful normalization
    // -----------------------------------------------------------------------

    #[test]
    fn valid_input_produces_normalized_output() {
        let json = valid_json();
        let result = make().validate_and_normalize(&make_input(json));
        assert!(result.is_ok());
        let out = result.unwrap();
        assert_eq!(out.response.hypotheses.len(), 2);
        assert_eq!(
            out.response.problem_understanding,
            "The service is experiencing high latency under load"
        );
    }

    #[test]
    fn whitespace_in_string_fields_is_trimmed() {
        let mut json = valid_json();
        json["problem_understanding"] = json!("  trimmed  ");
        let out = make().validate_and_normalize(&make_input(json)).unwrap();
        assert_eq!(out.response.problem_understanding, "trimmed");
    }

    #[test]
    fn rejected_hypothesis_carries_reason() {
        let mut json = valid_json();
        json["hypotheses"][0]["status"] = json!("rejected");
        json["hypotheses"][0]["rejection_reason"] = json!("Evidence contradicts this path");
        let out = make().validate_and_normalize(&make_input(json)).unwrap();
        let h = &out.response.hypotheses[0];
        assert!(matches!(&h.status, HypothesisStatus::Rejected(r) if r == "Evidence contradicts this path"));
    }

    #[test]
    fn competing_interpretation_present_is_preserved() {
        let mut json = valid_json();
        json["competing_interpretation"] = json!("Could also be a network partition");
        let out = make().validate_and_normalize(&make_input(json)).unwrap();
        assert_eq!(
            out.response.competing_interpretation,
            Some("Could also be a network partition".to_string())
        );
    }

    #[test]
    fn null_competing_interpretation_maps_to_none() {
        let out = make().validate_and_normalize(&make_input(valid_json())).unwrap();
        assert!(out.response.competing_interpretation.is_none());
    }

    #[test]
    fn hypothesis_ids_are_parsed_as_uuid() {
        let fixed_id = "550e8400-e29b-41d4-a716-446655440000";
        let mut json = valid_json();
        json["hypotheses"][0]["id"] = json!(fixed_id);
        let out = make().validate_and_normalize(&make_input(json)).unwrap();
        assert_eq!(
            out.response.hypotheses[0].id.0,
            Uuid::parse_str(fixed_id).unwrap()
        );
    }

    #[test]
    fn validate_and_normalize_delegates_to_with_context() {
        let json = valid_json();
        let m = make();
        let r1 = m.validate_and_normalize(&make_input(json.clone())).unwrap();
        let r2 = m
            .validate_and_normalize_with_context(&make_input(json), &Context::noop())
            .unwrap();
        assert_eq!(r1, r2);
    }
}
