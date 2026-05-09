use std::sync::Arc;

use serde::Deserialize;

use crate::api_clients::model::{
    ModelClient, ModelClientError, ModelFinishReason, ModelGenerationRequest, ModelMessage,
    ModelMessageRole, ModelResponseMode,
};
use crate::config::QueryStructuringSettings;
use crate::request_pipeline::query_structuring_metrics::compute_query_structuring_metrics;
use crate::shared_types::{
    Context, ModelTokenUsage, NormalizedUserRequest, QueryStructuringControlledVocabulary,
    QueryStructuringMetrics, QueryStructuringOutput, QueryStructuringVocabularyFieldMetricSet,
    StructuredUserQuery,
};
use tracing::{info_span, field, Instrument};

const QUERY_PLACEHOLDER: &str = "{{normalized_query}}";
const VOCAB_PLACEHOLDER: &str = "{{controlled_vocabulary_json}}";

// ── Module-private asset types ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct QueryStructuringPromptAsset {
    version: String,
    system_prompt: String,
    user_template: String,
    response_schema: serde_json::Value,
}

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, thiserror::Error)]
pub enum QueryStructuringError {
    #[error("invalid config: {0}")]
    InvalidConfig(String),

    #[error("failed to read asset at {path}: {message}")]
    AssetRead { path: String, message: String },

    #[error("failed to parse asset at {path}: {message}")]
    AssetParse { path: String, message: String },

    #[error("invalid prompt asset: {0}")]
    InvalidPromptAsset(String),

    #[error("invalid controlled vocabulary: {0}")]
    InvalidControlledVocabulary(String),

    #[error("model client error: {0}")]
    Model(#[from] ModelClientError),

    #[error("metrics computation failed: {message}")]
    MetricsComputation { message: String },

    #[error("invalid model output: {reason}")]
    InvalidModelOutput {
        reason: String,
        token_usage: ModelTokenUsage,
        finish_reason: Option<ModelFinishReason>,
    },
}

// ── Public struct ─────────────────────────────────────────────────────────────

pub struct QueryStructuring {
    model_client: Arc<dyn ModelClient>,
    controlled_vocabulary: QueryStructuringControlledVocabulary,
    prompt_asset: QueryStructuringPromptAsset,
    max_output_tokens: u32,
    prompt_asset_path: String,
    controlled_vocabulary_path: String,
}

impl std::fmt::Debug for QueryStructuring {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueryStructuring")
            .field("max_output_tokens", &self.max_output_tokens)
            .finish_non_exhaustive()
    }
}

impl QueryStructuring {
    pub fn new(
        settings: QueryStructuringSettings,
        model_client: Arc<dyn ModelClient>,
    ) -> Result<Self, QueryStructuringError> {
        if settings.controlled_vocabulary_path.trim().is_empty() {
            return Err(QueryStructuringError::InvalidConfig(
                "controlled_vocabulary_path must not be empty".to_string(),
            ));
        }
        if settings.prompt_asset_path.trim().is_empty() {
            return Err(QueryStructuringError::InvalidConfig(
                "prompt_asset_path must not be empty".to_string(),
            ));
        }
        if settings.max_output_tokens == 0 {
            return Err(QueryStructuringError::InvalidConfig(
                "max_output_tokens must be greater than zero".to_string(),
            ));
        }

        let vocab_json =
            std::fs::read_to_string(&settings.controlled_vocabulary_path).map_err(|e| {
                QueryStructuringError::AssetRead {
                    path: settings.controlled_vocabulary_path.clone(),
                    message: e.to_string(),
                }
            })?;
        let controlled_vocabulary: QueryStructuringControlledVocabulary =
            serde_json::from_str(&vocab_json).map_err(|e| QueryStructuringError::AssetParse {
                path: settings.controlled_vocabulary_path.clone(),
                message: e.to_string(),
            })?;
        validate_controlled_vocabulary(&controlled_vocabulary)?;

        let prompt_json = std::fs::read_to_string(&settings.prompt_asset_path).map_err(|e| {
            QueryStructuringError::AssetRead {
                path: settings.prompt_asset_path.clone(),
                message: e.to_string(),
            }
        })?;
        let prompt_asset: QueryStructuringPromptAsset = serde_json::from_str(&prompt_json)
            .map_err(|e| QueryStructuringError::AssetParse {
                path: settings.prompt_asset_path.clone(),
                message: e.to_string(),
            })?;
        validate_prompt_asset(&prompt_asset)?;

        Ok(Self {
            model_client,
            controlled_vocabulary,
            prompt_asset,
            max_output_tokens: settings.max_output_tokens,
            prompt_asset_path: settings.prompt_asset_path,
            controlled_vocabulary_path: settings.controlled_vocabulary_path,
        })
    }

    pub async fn structure(
        &self,
        request: &NormalizedUserRequest,
    ) -> Result<QueryStructuringOutput, QueryStructuringError> {
        self.structure_with_context(request, &Context::noop()).await
    }

    pub async fn structure_with_context(
        &self,
        request: &NormalizedUserRequest,
        context: &Context,
    ) -> Result<QueryStructuringOutput, QueryStructuringError> {
        let prompt_name = std::path::Path::new(&self.prompt_asset_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        let vocab_name = std::path::Path::new(&self.controlled_vocabulary_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        let oi_span = crate::observability::oi_llm_query_structuring_span(
            &context.open_inference.root_span,
        );
        let oi_input_json = serde_json::json!({
            "normalized_query": request.query,
            "input_token_count": request.input_token_count,
            "system_prompt": self.prompt_asset.system_prompt,
            "controlled_vocabulary": "..."
        })
        .to_string();
        oi_span.record("input.value", oi_input_json.as_str());
        oi_span.record("input.mime_type", "application/json");
        oi_span.record("llm.model_name", "unknown");
        oi_span.record("llm.provider", "unknown");
        oi_span.record(
            "llm.invocation_parameters",
            r#"{"temperature":0.0,"response_format":"json_schema"}"#,
        );

        let span = info_span!(
            "request_pipeline.query_structuring",
            module.name = "query_structuring",
            query.normalized = %request.query,
            query.input_token_count = request.input_token_count,
            asset.prompt.name = %prompt_name,
            asset.prompt.version = %self.prompt_asset.version,
            asset.prompt.template_placeholders_valid = true,
            asset.vocabulary.name = %vocab_name,
            model.provider = field::Empty,
            model.name = field::Empty,
            model.response_mode = field::Empty,
            model.temperature = field::Empty,
            model.max_output_tokens = field::Empty,
            model.finish_reason = field::Empty,
            model.prompt_tokens = field::Empty,
            model.completion_tokens = field::Empty,
            model.total_tokens = field::Empty,
            structured.intent_present = field::Empty,
            structured.symptoms_count = field::Empty,
            structured.affected_subsystems_count = field::Empty,
            structured.failure_modes_count = field::Empty,
            structured.constraints_count = field::Empty,
            structured.confidence = field::Empty,
            module.outcome = field::Empty,
            status = field::Empty,
            error.type = field::Empty,
            error.message = field::Empty,
        );

        let mut output = self
            .structure_instrumented(request, &oi_span)
            .instrument(span)
            .await?;

        if let Some(ref golden_question) = context.golden_question {
            let metrics = compute_query_structuring_metrics(
                &output.structured_query,
                &golden_question.expected_query_structuring,
                &self.controlled_vocabulary,
                &request.query,
            )
            .map_err(|e| QueryStructuringError::MetricsComputation {
                message: e.to_string(),
            })?;
            // Non-fatal: emit OI span with flat attrs and events; swallows observability errors
            oi_span.in_scope(|| emit_metrics_oi_span(&oi_span, &metrics));
            output.metrics = Some(metrics);
        }

        Ok(output)
    }

    async fn structure_instrumented(
        &self,
        request: &NormalizedUserRequest,
        oi_span: &tracing::Span,
    ) -> Result<QueryStructuringOutput, QueryStructuringError> {
        let vocab_json = serde_json::to_string(&self.controlled_vocabulary)
            .expect("QueryStructuringControlledVocabulary serialization must not fail");

        let user_message = substitute_template(
            &self.prompt_asset.user_template,
            &request.query,
            &vocab_json,
        );

        let llm_span = info_span!(
            "llm.call.query_structuring",
            llm.task = "query_structuring",
            model.provider = field::Empty,
            model.name = field::Empty,
            model.response_mode = field::Empty,
            model.temperature = field::Empty,
            model.max_output_tokens = field::Empty,
            model.finish_reason = field::Empty,
            model.prompt_tokens = field::Empty,
            model.completion_tokens = field::Empty,
            model.total_tokens = field::Empty,
            status = field::Empty,
            error.type = field::Empty,
            error.message = field::Empty,
        );

        let model_request = ModelGenerationRequest {
            messages: vec![
                ModelMessage {
                    role: ModelMessageRole::System,
                    content: self.prompt_asset.system_prompt.clone(),
                },
                ModelMessage {
                    role: ModelMessageRole::User,
                    content: user_message,
                },
            ],
            temperature: 0.0,
            max_output_tokens: Some(self.max_output_tokens),
            response_mode: ModelResponseMode::JsonSchema(self.prompt_asset.response_schema.clone()),
        };

        let response = {
            async {
                match self.model_client.generate(&model_request).await {
            Ok(r) => {
                oi_span.record("llm.model_name", "unknown");
                oi_span.record("llm.provider", "unknown");
                tracing::Span::current().record("model.provider", "unknown");
                tracing::Span::current().record("model.name", "unknown");
                        tracing::Span::current().record("model.response_mode", "JsonSchema");
                        tracing::Span::current().record("model.temperature", 0.0);
                        tracing::Span::current().record("model.max_output_tokens", self.max_output_tokens as i64);
                        if let Some(ref fr) = r.finish_reason {
                            tracing::Span::current().record("model.finish_reason", format!("{:?}", fr));
                        }
                        if let Some(pt) = r.prompt_tokens {
                            oi_span.record("llm.token_count.prompt", pt as i64);
                            tracing::Span::current().record("model.prompt_tokens", pt as i64);
                        }
                        if let Some(ct) = r.completion_tokens {
                            oi_span.record("llm.token_count.completion", ct as i64);
                            tracing::Span::current().record("model.completion_tokens", ct as i64);
                        }
                        if let Some(tt) = r.total_tokens {
                            oi_span.record("llm.token_count.total", tt as i64);
                            tracing::Span::current().record("model.total_tokens", tt as i64);
                        }
                        oi_span.record("status", "ok");
                        tracing::Span::current().record("status", "ok");
                        Ok(r)
                    }
                    Err(e) => {
                        crate::observability::record_error(
                            oi_span,
                            "QueryStructuring.Model",
                            &format!("Model client error: {}", e),
                        );
                        tracing::Span::current().record("status", "error");
                        tracing::Span::current().record("error.type", "QueryStructuring.Model");
                        tracing::Span::current()
                            .record("error.message", format!("Model client error: {}", e));
                        Err(e)
                    }
                }
            }
            .instrument(llm_span)
            .await
        };

        let response = match response {
            Ok(r) => r,
            Err(e) => {
                crate::observability::record_error(
                    oi_span,
                    "QueryStructuring.Model",
                    &format!("Model client error: {}", e),
                );
                tracing::Span::current().record("module.outcome", "failure");
                tracing::Span::current().record("status", "error");
                tracing::Span::current().record("error.type", "QueryStructuring.Model");
                tracing::Span::current()
                    .record("error.message", format!("Model client error: {}", e));
                return Err(QueryStructuringError::Model(e));
            }
        };

        let token_usage = ModelTokenUsage {
            prompt_tokens: response.prompt_tokens,
            completion_tokens: response.completion_tokens,
            total_tokens: response.total_tokens,
        };
        let finish_reason = response.finish_reason.clone();
        let content = response.content.clone();
        oi_span.record("llm.raw_response", content.as_str());

        tracing::Span::current().record("model.provider", "unknown");
        tracing::Span::current().record("model.name", "unknown");
        tracing::Span::current().record("model.response_mode", "JsonSchema");
        tracing::Span::current().record("model.temperature", 0.0);
        tracing::Span::current().record("model.max_output_tokens", self.max_output_tokens as i64);
        if let Some(ref fr) = finish_reason {
            tracing::Span::current().record("model.finish_reason", format!("{:?}", fr));
        }
        if let Some(pt) = response.prompt_tokens {
            tracing::Span::current().record("model.prompt_tokens", pt as i64);
        }
        if let Some(ct) = response.completion_tokens {
            tracing::Span::current().record("model.completion_tokens", ct as i64);
        }
        if let Some(tt) = response.total_tokens {
            tracing::Span::current().record("model.total_tokens", tt as i64);
        }

        let is_acceptable_finish = matches!(&finish_reason, Some(ModelFinishReason::Stop) | None);
        if !is_acceptable_finish {
            let reason = if matches!(finish_reason, Some(ModelFinishReason::Length)) {
                "model output truncated: finish_reason was length"
            } else {
                "model returned unusable finish reason"
            };
            tracing::Span::current().record("module.outcome", "failure");
            tracing::Span::current().record("status", "error");
            tracing::Span::current().record("error.type", "QueryStructuring.InvalidModelOutput");
            tracing::Span::current().record("error.message", reason);
            crate::observability::record_error(oi_span, "QueryStructuring.InvalidModelOutput", reason);
            return Err(QueryStructuringError::InvalidModelOutput {
                reason: reason.to_string(),
                token_usage,
                finish_reason,
            });
        }

        let parse_token_usage = token_usage.clone();
        let parse_finish_reason = finish_reason.clone();
        let structured_query: StructuredUserQuery =
            serde_json::from_str(&content).map_err(|e| {
                crate::observability::record_error(
                    oi_span,
                    "QueryStructuring.InvalidModelOutput",
                    &format!("Failed to parse model output: {}", e),
                );
                tracing::Span::current().record("module.outcome", "failure");
                tracing::Span::current().record("status", "error");
                tracing::Span::current().record("error.type", "QueryStructuring.InvalidModelOutput");
                tracing::Span::current()
                    .record("error.message", format!("Failed to parse model output: {}", e));
                QueryStructuringError::InvalidModelOutput {
                    reason: "failed to parse model output as StructuredUserQuery".to_string(),
                    token_usage: parse_token_usage,
                    finish_reason: parse_finish_reason,
                }
            })?;

        if structured_query.failure_modes.len() > 1 {
            crate::observability::record_error(
                oi_span,
                "QueryStructuring.InvalidModelOutput",
                "failure_modes must contain at most one item",
            );
            tracing::Span::current().record("module.outcome", "failure");
            tracing::Span::current().record("status", "error");
            tracing::Span::current().record("error.type", "QueryStructuring.InvalidModelOutput");
            tracing::Span::current()
                .record("error.message", "failure_modes must contain at most one item");
            return Err(QueryStructuringError::InvalidModelOutput {
                reason: "failure_modes must contain at most one item".to_string(),
                token_usage,
                finish_reason,
            });
        }

        let structured_query_json = serde_json::to_string(&structured_query)
            .unwrap_or_else(|_| "{}".to_string());
        let intent_present = !structured_query.intent.is_empty();
        let symptoms_count = structured_query.symptoms.len();
        let affected_subsystems_count = structured_query.affected_subsystems.len();
        let failure_modes_count = structured_query.failure_modes.len();
        let constraints_count = structured_query.constraints.len();
        let confidence_str = format!("{:?}", structured_query.confidence);

        tracing::event!(
            tracing::Level::INFO,
            event.name = "structured_query_payload",
            structured_query.json = %structured_query_json
        );
        oi_span.record("output.value", structured_query_json.as_str());
        oi_span.record("output.mime_type", "application/json");
        oi_span.record("status", "ok");
        tracing::Span::current().record("structured.intent_present", intent_present);
        tracing::Span::current().record("structured.symptoms_count", symptoms_count as i64);
        tracing::Span::current().record("structured.affected_subsystems_count", affected_subsystems_count as i64);
        tracing::Span::current().record("structured.failure_modes_count", failure_modes_count as i64);
        tracing::Span::current().record("structured.constraints_count", constraints_count as i64);
        tracing::Span::current().record("structured.confidence", &confidence_str);
        tracing::Span::current().record("module.outcome", "success");
        tracing::Span::current().record("status", "ok");

        Ok(QueryStructuringOutput {
            structured_query,
            token_usage,
            metrics: None,
        })
    }
}

// ── Validation helpers ────────────────────────────────────────────────────────

fn validate_controlled_vocabulary(
    vocab: &QueryStructuringControlledVocabulary,
) -> Result<(), QueryStructuringError> {
    if vocab.canonical_symptoms.is_empty() {
        return Err(QueryStructuringError::InvalidControlledVocabulary(
            "canonical_symptoms must not be empty".to_string(),
        ));
    }
    if vocab.affected_components.is_empty() {
        return Err(QueryStructuringError::InvalidControlledVocabulary(
            "affected_components must not be empty".to_string(),
        ));
    }
    if vocab.failure_mode_candidates.is_empty() {
        return Err(QueryStructuringError::InvalidControlledVocabulary(
            "failure_mode_candidates must not be empty".to_string(),
        ));
    }
    if vocab.violated_properties.is_empty() {
        return Err(QueryStructuringError::InvalidControlledVocabulary(
            "violated_properties must not be empty".to_string(),
        ));
    }
    Ok(())
}

fn validate_prompt_asset(asset: &QueryStructuringPromptAsset) -> Result<(), QueryStructuringError> {
    if asset.version.trim().is_empty() {
        return Err(QueryStructuringError::InvalidPromptAsset(
            "prompt asset version must not be empty".to_string(),
        ));
    }
    if asset.system_prompt.trim().is_empty() {
        return Err(QueryStructuringError::InvalidPromptAsset(
            "prompt asset system_prompt must not be empty".to_string(),
        ));
    }
    if asset.user_template.trim().is_empty() {
        return Err(QueryStructuringError::InvalidPromptAsset(
            "prompt asset user_template must not be empty".to_string(),
        ));
    }
    if !asset.response_schema.is_object() {
        return Err(QueryStructuringError::InvalidPromptAsset(
            "prompt asset response_schema must be a JSON object".to_string(),
        ));
    }
    validate_user_template_placeholders(&asset.user_template)
}

fn validate_user_template_placeholders(template: &str) -> Result<(), QueryStructuringError> {
    if template.matches(QUERY_PLACEHOLDER).count() != 1 {
        return Err(QueryStructuringError::InvalidPromptAsset(
            "user_template must contain {{normalized_query}} exactly once".to_string(),
        ));
    }
    if template.matches(VOCAB_PLACEHOLDER).count() != 1 {
        return Err(QueryStructuringError::InvalidPromptAsset(
            "user_template must contain {{controlled_vocabulary_json}} exactly once".to_string(),
        ));
    }
    let stripped = template
        .replace(QUERY_PLACEHOLDER, "")
        .replace(VOCAB_PLACEHOLDER, "");
    if stripped.contains("{{") {
        return Err(QueryStructuringError::InvalidPromptAsset(
            "user_template contains unrecognized placeholder construct".to_string(),
        ));
    }
    Ok(())
}

// ── Prompt assembly ───────────────────────────────────────────────────────────

fn substitute_template(template: &str, query: &str, vocab_json: &str) -> String {
    // Both positions are guaranteed to exist exactly once by constructor validation.
    // Substitution is positional over the original template — inserted text is never
    // rescanned for placeholder patterns.
    let query_pos = template.find(QUERY_PLACEHOLDER).unwrap();
    let vocab_pos = template.find(VOCAB_PLACEHOLDER).unwrap();

    if query_pos < vocab_pos {
        let before = &template[..query_pos];
        let between = &template[query_pos + QUERY_PLACEHOLDER.len()..vocab_pos];
        let after = &template[vocab_pos + VOCAB_PLACEHOLDER.len()..];
        format!("{}{}{}{}{}", before, query, between, vocab_json, after)
    } else {
        let before = &template[..vocab_pos];
        let between = &template[vocab_pos + VOCAB_PLACEHOLDER.len()..query_pos];
        let after = &template[query_pos + QUERY_PLACEHOLDER.len()..];
        format!("{}{}{}{}{}", before, vocab_json, between, query, after)
    }
}

// ── OpenInference metrics span emission ───────────────────────────────────────

fn emit_metrics_oi_span(oi_parent: &tracing::Span, metrics: &QueryStructuringMetrics) {
    use opentelemetry::{Key, Value};
    use tracing_opentelemetry::OpenTelemetrySpanExt;

    let span = crate::observability::oi_chain_query_structuring_metrics_span(oi_parent);

    span.record("input.value", r#"{"golden_backed":true,"source":"structured_query"}"#);
    span.record("input.mime_type", "application/json");

    let agg = &metrics.aggregates;
    span.set_attribute(Key::from("qs.core.global.aggregate.macro_precision_soft"), Value::F64(agg.macro_precision_soft as f64));
    span.set_attribute(Key::from("qs.core.global.aggregate.macro_recall_strict"), Value::F64(agg.macro_recall_strict as f64));
    span.set_attribute(Key::from("qs.core.global.aggregate.macro_recall_soft"), Value::F64(agg.macro_recall_soft as f64));
    span.set_attribute(Key::from("qs.core.global.aggregate.overall_grounded_strict_recall"), Value::F64(agg.overall_grounded_strict_recall as f64));
    span.set_attribute(Key::from("qs.core.global.aggregate.all_fields_core_success_rate"), Value::F64(agg.all_fields_core_success_rate as f64));

    record_vocab_field_flat_attrs(&span, "symptoms", &metrics.vocab_fields.symptoms);
    record_vocab_field_flat_attrs(&span, "affected_subsystems", &metrics.vocab_fields.affected_subsystems);
    record_vocab_field_flat_attrs(&span, "failure_modes", &metrics.vocab_fields.failure_modes);
    record_vocab_field_flat_attrs(&span, "system_properties", &metrics.vocab_fields.system_properties);

    let nv = &metrics.non_vocab_fields;
    span.set_attribute(Key::from("qs.diag.non_vocab.entities.count.value"), Value::I64(nv.entities_count as i64));
    span.set_attribute(Key::from("qs.diag.non_vocab.constraints.count.value"), Value::I64(nv.constraints_count as i64));
    span.set_attribute(Key::from("qs.diag.non_vocab.triggers.count.value"), Value::I64(nv.triggers_count as i64));
    span.set_attribute(Key::from("qs.diag.non_vocab.observability_signals.count.value"), Value::I64(nv.observability_signals_count as i64));
    span.set_attribute(Key::from("qs.diag.non_vocab.unresolved_terms.count.value"), Value::I64(nv.unresolved_terms_count as i64));
    span.set_attribute(Key::from("qs.diag.non_vocab.intent.presence.present"), Value::Bool(nv.intent_present));
    span.set_attribute(Key::from("qs.diag.non_vocab.scenario.presence.present"), Value::Bool(nv.scenario_present));

    let core_json = build_core_event_payload(metrics).to_string();
    let symptoms_json = build_vocab_field_event_payload("symptoms", &metrics.vocab_fields.symptoms).to_string();
    let affected_json = build_vocab_field_event_payload("affected_subsystems", &metrics.vocab_fields.affected_subsystems).to_string();
    let failure_json = build_vocab_field_event_payload("failure_modes", &metrics.vocab_fields.failure_modes).to_string();
    let system_json = build_vocab_field_event_payload("system_properties", &metrics.vocab_fields.system_properties).to_string();
    let non_vocab_json = build_non_vocab_event_payload(nv).to_string();

    let _guard = span.enter();
    tracing::event!(tracing::Level::INFO, event.name = "query_structuring_metrics.core", payload = %core_json);
    tracing::event!(tracing::Level::INFO, event.name = "query_structuring_metrics.vocab.symptoms", payload = %symptoms_json);
    tracing::event!(tracing::Level::INFO, event.name = "query_structuring_metrics.vocab.affected_subsystems", payload = %affected_json);
    tracing::event!(tracing::Level::INFO, event.name = "query_structuring_metrics.vocab.failure_modes", payload = %failure_json);
    tracing::event!(tracing::Level::INFO, event.name = "query_structuring_metrics.vocab.system_properties", payload = %system_json);
    tracing::event!(tracing::Level::INFO, event.name = "query_structuring_metrics.non_vocab", payload = %non_vocab_json);
    drop(_guard);

    let output_json = serde_json::to_string(metrics).unwrap_or_default();
    span.record("output.value", output_json.as_str());
    span.record("output.mime_type", "application/json");
    span.record("status", "ok");
}

fn record_vocab_field_flat_attrs(
    span: &tracing::Span,
    field: &str,
    m: &QueryStructuringVocabularyFieldMetricSet,
) {
    use opentelemetry::{Key, Value};
    use tracing_opentelemetry::OpenTelemetrySpanExt;

    let c = format!("qs.core.vocab.{field}");
    let d = format!("qs.diag.vocab.{field}");

    span.set_attribute(Key::from(format!("{c}.selection.precision_soft")), Value::F64(m.precision_soft as f64));
    span.set_attribute(Key::from(format!("{c}.selection.recall_strict")), Value::F64(m.recall_strict as f64));
    span.set_attribute(Key::from(format!("{c}.selection.recall_soft")), Value::F64(m.recall_soft as f64));
    span.set_attribute(Key::from(format!("{c}.grounding.grounded_strict_recall")), Value::F64(m.grounded_strict_recall as f64));
    span.set_attribute(Key::from(format!("{c}.success.field_core_success")), Value::Bool(m.field_core_success));
    span.set_attribute(Key::from(format!("{c}.success.field_grounded_success")), Value::Bool(m.field_grounded_success));

    span.set_attribute(Key::from(format!("{d}.contract.invalid_vocab_count")), Value::I64(m.invalid_vocab_count as i64));
    span.set_attribute(Key::from(format!("{d}.contract.duplicate_term_count")), Value::I64(m.duplicate_term_count as i64));
    span.set_attribute(Key::from(format!("{d}.selection.num_false_positive")), Value::I64(m.num_false_positive as i64));
    span.set_attribute(Key::from(format!("{d}.selection.num_false_negative_strict")), Value::I64(m.num_false_negative_strict as i64));
    span.set_attribute(Key::from(format!("{d}.selection.num_predicted_terms")), Value::I64(m.num_predicted_terms as i64));
    span.set_attribute(Key::from(format!("{d}.graded.graded_coverage")), Value::F64(m.graded_coverage as f64));
    span.set_attribute(Key::from(format!("{d}.graded.average_selected_score")), Value::F64(m.average_selected_score as f64));
    span.set_attribute(Key::from(format!("{d}.graded.zero_score_selection_count")), Value::I64(m.zero_score_selection_count as i64));
    span.set_attribute(Key::from(format!("{d}.grounding.unsupported_selected_term_rate")), Value::F64(m.unsupported_selected_term_rate as f64));
    span.set_attribute(Key::from(format!("{d}.grounding.missing_evidence_span_count")), Value::I64(m.missing_evidence_span_count as i64));
    span.set_attribute(Key::from(format!("{d}.grounding.invalid_evidence_span_count")), Value::I64(m.invalid_evidence_span_count as i64));
    span.set_attribute(Key::from(format!("{d}.grounding.evidence_span_near_substring_rate")), Value::F64(m.evidence_span_near_substring_rate as f64));
    span.set_attribute(Key::from(format!("{d}.support.weak_inference_rate")), Value::F64(m.weak_inference_rate as f64));
    span.set_attribute(Key::from(format!("{d}.support.strict_terms_weak_inference_rate")), Value::F64(m.strict_terms_weak_inference_rate as f64));
    span.set_attribute(Key::from(format!("{d}.support.weak_false_positive_rate")), Value::F64(m.weak_false_positive_rate as f64));
    span.set_attribute(Key::from(format!("{d}.success.empty_when_gold_exists")), Value::Bool(m.empty_when_gold_exists));
}

fn vocab_core_section(m: &QueryStructuringVocabularyFieldMetricSet) -> serde_json::Value {
    serde_json::json!({
        "selection": {
            "precision_soft": m.precision_soft,
            "recall_strict": m.recall_strict,
            "recall_soft": m.recall_soft,
        },
        "grounding": { "grounded_strict_recall": m.grounded_strict_recall },
        "success": {
            "field_core_success": m.field_core_success,
            "field_grounded_success": m.field_grounded_success,
        },
    })
}

fn build_core_event_payload(metrics: &QueryStructuringMetrics) -> serde_json::Value {
    let vf = &metrics.vocab_fields;
    let agg = &metrics.aggregates;
    serde_json::json!({
        "global": {
            "aggregate": {
                "macro_precision_soft": agg.macro_precision_soft,
                "macro_recall_strict": agg.macro_recall_strict,
                "macro_recall_soft": agg.macro_recall_soft,
                "overall_grounded_strict_recall": agg.overall_grounded_strict_recall,
                "all_fields_core_success_rate": agg.all_fields_core_success_rate,
            }
        },
        "vocab": {
            "symptoms": vocab_core_section(&vf.symptoms),
            "affected_subsystems": vocab_core_section(&vf.affected_subsystems),
            "failure_modes": vocab_core_section(&vf.failure_modes),
            "system_properties": vocab_core_section(&vf.system_properties),
        },
    })
}

fn build_vocab_field_event_payload(
    field: &str,
    m: &QueryStructuringVocabularyFieldMetricSet,
) -> serde_json::Value {
    serde_json::json!({
        "field": field,
        "core": vocab_core_section(m),
        "diag": {
            "contract": {
                "invalid_vocab_count": m.invalid_vocab_count,
                "duplicate_term_count": m.duplicate_term_count,
            },
            "selection": {
                "num_false_positive": m.num_false_positive,
                "num_false_negative_strict": m.num_false_negative_strict,
                "num_predicted_terms": m.num_predicted_terms,
            },
            "graded": {
                "graded_coverage": m.graded_coverage,
                "average_selected_score": m.average_selected_score,
                "zero_score_selection_count": m.zero_score_selection_count,
            },
            "grounding": {
                "unsupported_selected_term_rate": m.unsupported_selected_term_rate,
                "missing_evidence_span_count": m.missing_evidence_span_count,
                "invalid_evidence_span_count": m.invalid_evidence_span_count,
                "evidence_span_near_substring_rate": m.evidence_span_near_substring_rate,
            },
            "support": {
                "weak_inference_rate": m.weak_inference_rate,
                "strict_terms_weak_inference_rate": m.strict_terms_weak_inference_rate,
                "weak_false_positive_rate": m.weak_false_positive_rate,
            },
            "success": { "empty_when_gold_exists": m.empty_when_gold_exists },
        },
    })
}

fn build_non_vocab_event_payload(
    nv: &crate::shared_types::QueryStructuringNonVocabularyFieldMetrics,
) -> serde_json::Value {
    serde_json::json!({
        "diag": {
            "entities": { "count": { "value": nv.entities_count } },
            "constraints": { "count": { "value": nv.constraints_count } },
            "triggers": { "count": { "value": nv.triggers_count } },
            "observability_signals": { "count": { "value": nv.observability_signals_count } },
            "unresolved_terms": { "count": { "value": nv.unresolved_terms_count } },
            "intent": { "presence": { "present": nv.intent_present } },
            "scenario": { "presence": { "present": nv.scenario_present } },
        }
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;

    use super::{QueryStructuring, QueryStructuringError};
    use crate::api_clients::model::{
        ModelClient, ModelClientError, ModelFinishReason, ModelGenerationRequest,
        ModelGenerationResponse, ModelMessageRole, ModelResponseMode,
    };
    use crate::config::QueryStructuringSettings;
    use crate::request_pipeline::query_structuring_metrics::compute_query_structuring_metrics;
    use crate::shared_types::{
        Context, GoldenCandidateCardSection, GoldenCardRetrievalTargets,
        GoldenChunkRetrievalCallTargets, GoldenChunkRetrievalTargets,
        GoldenIncidentEvidenceTargets, GoldenQueryStructuringTargets, GoldenQuestion,
        GoldenQuestionQuery, GoldenTermRelevance, GoldenTheoryEvidenceTargets,
        GoldenVocabularyFieldTargets, NormalizedUserRequest, OpenInferenceContext,
        QueryStructuringControlledVocabulary, StructuredUserQueryConfidence,
    };
    use crate::test_utils::TempArtifactDir;

    // ── Mock clients ──────────────────────────────────────────────────────────

    struct MockModelClient {
        response: ModelGenerationResponse,
        captured: Arc<Mutex<Option<ModelGenerationRequest>>>,
    }

    impl MockModelClient {
        fn new_with_capture(
            response: ModelGenerationResponse,
        ) -> (Arc<Self>, Arc<Mutex<Option<ModelGenerationRequest>>>) {
            let captured = Arc::new(Mutex::new(None));
            let client = Arc::new(Self {
                response,
                captured: Arc::clone(&captured),
            });
            (client, captured)
        }

        fn new(response: ModelGenerationResponse) -> Arc<Self> {
            Arc::new(Self {
                response,
                captured: Arc::new(Mutex::new(None)),
            })
        }
    }

    #[async_trait]
    impl ModelClient for MockModelClient {
        async fn generate(
            &self,
            request: &ModelGenerationRequest,
        ) -> Result<ModelGenerationResponse, ModelClientError> {
            *self.captured.lock().unwrap() = Some(request.clone());
            Ok(self.response.clone())
        }
    }

    struct NoopModelClient;

    #[async_trait]
    impl ModelClient for NoopModelClient {
        async fn generate(
            &self,
            _: &ModelGenerationRequest,
        ) -> Result<ModelGenerationResponse, ModelClientError> {
            panic!("model client must not be called in constructor tests")
        }
    }

    fn noop_client() -> Arc<dyn ModelClient> {
        Arc::new(NoopModelClient)
    }

    // ── Fixtures ──────────────────────────────────────────────────────────────

    const VOCAB_JSON: &str = r#"{
        "version": "v1",
        "canonical_symptoms": ["high_latency"],
        "affected_components": ["api_gateway"],
        "failure_mode_candidates": ["overload"],
        "violated_properties": ["availability"]
    }"#;

    const PROMPT_JSON: &str = r#"{
        "version": "v1",
        "system_prompt": "You are a helpful assistant.",
        "user_template": "Query: {{normalized_query}}\nVocabulary: {{controlled_vocabulary_json}}",
        "response_schema": {"type": "object"}
    }"#;

    fn valid_model_output() -> String {
        serde_json::json!({
            "intent": "diagnose failure",
            "scenario": "Service is down.",
            "symptoms": [],
            "affected_subsystems": [],
            "failure_modes": [],
            "system_properties": [],
            "entities": [],
            "constraints": [],
            "triggers": [],
            "observability_signals": [],
            "unresolved_terms": [],
            "rejected_nearby_terms": [],
            "confidence": "medium"
        })
        .to_string()
    }

    fn valid_response() -> ModelGenerationResponse {
        ModelGenerationResponse {
            content: valid_model_output(),
            finish_reason: Some(ModelFinishReason::Stop),
            prompt_tokens: Some(100),
            completion_tokens: Some(50),
            total_tokens: Some(150),
        }
    }

    fn write_vocab(dir: &TempArtifactDir) -> String {
        dir.write_json("vocab.json", VOCAB_JSON)
            .to_str()
            .unwrap()
            .to_string()
    }

    fn write_prompt(dir: &TempArtifactDir) -> String {
        dir.write_json("prompt.json", PROMPT_JSON)
            .to_str()
            .unwrap()
            .to_string()
    }

    fn make_settings(vocab_path: &str, prompt_path: &str) -> QueryStructuringSettings {
        QueryStructuringSettings {
            controlled_vocabulary_path: vocab_path.to_string(),
            prompt_asset_path: prompt_path.to_string(),
            max_output_tokens: 256,
        }
    }

    fn make_qs(
        dir: &TempArtifactDir,
        client: Arc<dyn ModelClient>,
    ) -> Result<QueryStructuring, QueryStructuringError> {
        QueryStructuring::new(make_settings(&write_vocab(dir), &write_prompt(dir)), client)
    }

    fn make_request(query: &str) -> NormalizedUserRequest {
        NormalizedUserRequest {
            query: query.to_string(),
            input_token_count: 3,
        }
    }

    // ── Constructor tests ─────────────────────────────────────────────────────

    #[test]
    fn new_fails_when_vocabulary_path_is_empty() {
        let dir = TempArtifactDir::new();
        let err = QueryStructuring::new(make_settings("", &write_prompt(&dir)), noop_client())
            .unwrap_err();
        assert!(matches!(err, QueryStructuringError::InvalidConfig(_)));
    }

    #[test]
    fn new_fails_when_prompt_path_is_empty() {
        let dir = TempArtifactDir::new();
        let err = QueryStructuring::new(make_settings(&write_vocab(&dir), ""), noop_client())
            .unwrap_err();
        assert!(matches!(err, QueryStructuringError::InvalidConfig(_)));
    }

    #[test]
    fn new_fails_when_max_output_tokens_is_zero() {
        let dir = TempArtifactDir::new();
        let err = QueryStructuring::new(
            QueryStructuringSettings {
                controlled_vocabulary_path: write_vocab(&dir),
                prompt_asset_path: write_prompt(&dir),
                max_output_tokens: 0,
            },
            noop_client(),
        )
        .unwrap_err();
        assert!(matches!(err, QueryStructuringError::InvalidConfig(_)));
    }

    #[test]
    fn new_fails_when_vocabulary_file_not_found() {
        let dir = TempArtifactDir::new();
        let err = QueryStructuring::new(
            make_settings("/nonexistent/vocab.json", &write_prompt(&dir)),
            noop_client(),
        )
        .unwrap_err();
        assert!(matches!(err, QueryStructuringError::AssetRead { .. }));
    }

    #[test]
    fn new_fails_when_prompt_file_not_found() {
        let dir = TempArtifactDir::new();
        let err = QueryStructuring::new(
            make_settings(&write_vocab(&dir), "/nonexistent/prompt.json"),
            noop_client(),
        )
        .unwrap_err();
        assert!(matches!(err, QueryStructuringError::AssetRead { .. }));
    }

    #[test]
    fn new_fails_when_vocabulary_json_invalid() {
        let dir = TempArtifactDir::new();
        let bad_vocab = dir
            .write_json("bad_vocab.json", "not json")
            .to_str()
            .unwrap()
            .to_string();
        let err = QueryStructuring::new(
            make_settings(&bad_vocab, &write_prompt(&dir)),
            noop_client(),
        )
        .unwrap_err();
        assert!(matches!(err, QueryStructuringError::AssetParse { .. }));
    }

    #[test]
    fn new_fails_when_prompt_json_invalid() {
        let dir = TempArtifactDir::new();
        let bad_prompt = dir
            .write_json("bad_prompt.json", "not json")
            .to_str()
            .unwrap()
            .to_string();
        let err = QueryStructuring::new(
            make_settings(&write_vocab(&dir), &bad_prompt),
            noop_client(),
        )
        .unwrap_err();
        assert!(matches!(err, QueryStructuringError::AssetParse { .. }));
    }

    #[test]
    fn new_fails_when_user_template_missing_query_placeholder() {
        let dir = TempArtifactDir::new();
        let p = dir
            .write_json(
                "p.json",
                r#"{"version":"v1","system_prompt":"sys","user_template":"Vocab: {{controlled_vocabulary_json}}","response_schema":{}}"#,
            )
            .to_str()
            .unwrap()
            .to_string();
        let err = QueryStructuring::new(make_settings(&write_vocab(&dir), &p), noop_client())
            .unwrap_err();
        assert!(matches!(err, QueryStructuringError::InvalidPromptAsset(_)));
    }

    #[test]
    fn new_fails_when_user_template_missing_vocabulary_placeholder() {
        let dir = TempArtifactDir::new();
        let p = dir
            .write_json(
                "p.json",
                r#"{"version":"v1","system_prompt":"sys","user_template":"Query: {{normalized_query}}","response_schema":{}}"#,
            )
            .to_str()
            .unwrap()
            .to_string();
        let err = QueryStructuring::new(make_settings(&write_vocab(&dir), &p), noop_client())
            .unwrap_err();
        assert!(matches!(err, QueryStructuringError::InvalidPromptAsset(_)));
    }

    #[test]
    fn new_succeeds_with_valid_assets() {
        let dir = TempArtifactDir::new();
        assert!(make_qs(&dir, noop_client()).is_ok());
    }

    // ── structure() — request shape ───────────────────────────────────────────

    #[tokio::test]
    async fn structure_builds_exactly_two_messages_system_then_user() {
        let dir = TempArtifactDir::new();
        let (client, captured) = MockModelClient::new_with_capture(valid_response());
        make_qs(&dir, client)
            .unwrap()
            .structure(&make_request("q"))
            .await
            .unwrap();

        let req = captured.lock().unwrap().take().unwrap();
        assert_eq!(req.messages.len(), 2);
        assert!(matches!(req.messages[0].role, ModelMessageRole::System));
        assert!(matches!(req.messages[1].role, ModelMessageRole::User));
    }

    #[tokio::test]
    async fn structure_user_message_contains_compact_vocabulary_json() {
        let dir = TempArtifactDir::new();
        let (client, captured) = MockModelClient::new_with_capture(valid_response());
        make_qs(&dir, client)
            .unwrap()
            .structure(&make_request("the query"))
            .await
            .unwrap();

        let req = captured.lock().unwrap().take().unwrap();
        let user_msg = &req.messages[1].content;

        let expected_vocab = r#"{"canonical_symptoms":["high_latency"],"affected_components":["api_gateway"],"failure_mode_candidates":["overload"],"violated_properties":["availability"]}"#;
        assert!(
            user_msg.contains(expected_vocab),
            "user message did not contain expected compact vocabulary JSON"
        );
        assert!(
            user_msg.contains("the query"),
            "user message did not contain query text"
        );
    }

    #[tokio::test]
    async fn structure_sends_json_schema_response_mode() {
        let dir = TempArtifactDir::new();
        let (client, captured) = MockModelClient::new_with_capture(valid_response());
        make_qs(&dir, client)
            .unwrap()
            .structure(&make_request("q"))
            .await
            .unwrap();

        let req = captured.lock().unwrap().take().unwrap();
        assert!(matches!(req.response_mode, ModelResponseMode::JsonSchema(_)));
    }

    #[tokio::test]
    async fn structure_sends_temperature_zero() {
        let dir = TempArtifactDir::new();
        let (client, captured) = MockModelClient::new_with_capture(valid_response());
        make_qs(&dir, client)
            .unwrap()
            .structure(&make_request("q"))
            .await
            .unwrap();

        let req = captured.lock().unwrap().take().unwrap();
        assert_eq!(req.temperature, 0.0_f32);
    }

    #[tokio::test]
    async fn structure_sends_configured_max_output_tokens() {
        let dir = TempArtifactDir::new();
        let (client, captured) = MockModelClient::new_with_capture(valid_response());
        make_qs(&dir, client)
            .unwrap()
            .structure(&make_request("q"))
            .await
            .unwrap();

        let req = captured.lock().unwrap().take().unwrap();
        assert_eq!(req.max_output_tokens, Some(256));
    }

    // ── structure() — success path ────────────────────────────────────────────

    #[tokio::test]
    async fn structure_succeeds_with_finish_reason_stop() {
        let dir = TempArtifactDir::new();
        let client = MockModelClient::new(valid_response());
        assert!(make_qs(&dir, client)
            .unwrap()
            .structure(&make_request("q"))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn structure_preserves_token_usage_on_success() {
        let dir = TempArtifactDir::new();
        let client = MockModelClient::new(valid_response());
        let out = make_qs(&dir, client)
            .unwrap()
            .structure(&make_request("q"))
            .await
            .unwrap();

        assert_eq!(out.token_usage.prompt_tokens, Some(100));
        assert_eq!(out.token_usage.completion_tokens, Some(50));
        assert_eq!(out.token_usage.total_tokens, Some(150));
    }

    #[tokio::test]
    async fn structure_maps_to_exact_query_structuring_output_shape() {
        let dir = TempArtifactDir::new();
        let content = serde_json::json!({
            "intent": "diagnose lock contention",
            "scenario": "Two workers hold the same lock.",
            "symptoms": [{"term": "high_latency", "evidence_span": "slow ops", "support_level": "explicit"}],
            "affected_subsystems": [{"term": "api_gateway", "evidence_span": "api slow", "support_level": "strong_paraphrase"}],
            "failure_modes": [{"term": "overload", "evidence_span": "cpu high", "support_level": "weak_inference"}],
            "system_properties": [],
            "entities": ["worker_a"],
            "constraints": ["under load"],
            "triggers": ["high traffic"],
            "observability_signals": ["cpu spike"],
            "unresolved_terms": ["unknown_term"],
            "rejected_nearby_terms": [{"term": "split_brain", "reason": "not in query"}],
            "confidence": "high"
        })
        .to_string();
        let client = MockModelClient::new(ModelGenerationResponse {
            content,
            prompt_tokens: Some(300),
            completion_tokens: Some(80),
            total_tokens: Some(380),
            ..valid_response()
        });
        let out = make_qs(&dir, client)
            .unwrap()
            .structure(&make_request("lock contention"))
            .await
            .unwrap();

        let q = &out.structured_query;
        assert_eq!(q.intent, "diagnose lock contention");
        assert_eq!(q.scenario, "Two workers hold the same lock.");
        assert_eq!(q.symptoms.len(), 1);
        assert_eq!(q.symptoms[0].term, "high_latency");
        assert_eq!(q.failure_modes.len(), 1);
        assert_eq!(q.failure_modes[0].term, "overload");
        assert_eq!(q.entities, ["worker_a"]);
        assert_eq!(q.constraints, ["under load"]);
        assert_eq!(q.unresolved_terms, ["unknown_term"]);
        assert_eq!(q.rejected_nearby_terms.len(), 1);
        assert_eq!(q.rejected_nearby_terms[0].term, "split_brain");
        assert!(matches!(q.confidence, StructuredUserQueryConfidence::High));
        assert_eq!(out.token_usage.prompt_tokens, Some(300));
        assert_eq!(out.token_usage.completion_tokens, Some(80));
        assert_eq!(out.token_usage.total_tokens, Some(380));
    }

    // ── structure() — failure paths ───────────────────────────────────────────

    #[tokio::test]
    async fn structure_fails_on_malformed_json() {
        let dir = TempArtifactDir::new();
        let client = MockModelClient::new(ModelGenerationResponse {
            content: "not json at all".to_string(),
            ..valid_response()
        });
        let err = make_qs(&dir, client)
            .unwrap()
            .structure(&make_request("q"))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            QueryStructuringError::InvalidModelOutput { .. }
        ));
    }

    #[tokio::test]
    async fn structure_fails_on_missing_required_field() {
        let dir = TempArtifactDir::new();
        // "confidence" field is absent
        let content = r#"{"intent":"x","scenario":"y","symptoms":[],"affected_subsystems":[],"failure_modes":[],"system_properties":[],"entities":[],"constraints":[],"triggers":[],"observability_signals":[],"unresolved_terms":[],"rejected_nearby_terms":[]}"#;
        let client = MockModelClient::new(ModelGenerationResponse {
            content: content.to_string(),
            ..valid_response()
        });
        let err = make_qs(&dir, client)
            .unwrap()
            .structure(&make_request("q"))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            QueryStructuringError::InvalidModelOutput { .. }
        ));
    }

    #[tokio::test]
    async fn structure_fails_on_unknown_support_level() {
        let dir = TempArtifactDir::new();
        let content = r#"{"intent":"x","scenario":"y","symptoms":[{"term":"t","evidence_span":"e","support_level":"no_such_level"}],"affected_subsystems":[],"failure_modes":[],"system_properties":[],"entities":[],"constraints":[],"triggers":[],"observability_signals":[],"unresolved_terms":[],"rejected_nearby_terms":[],"confidence":"medium"}"#;
        let client = MockModelClient::new(ModelGenerationResponse {
            content: content.to_string(),
            ..valid_response()
        });
        let err = make_qs(&dir, client)
            .unwrap()
            .structure(&make_request("q"))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            QueryStructuringError::InvalidModelOutput { .. }
        ));
    }

    #[tokio::test]
    async fn structure_fails_on_unknown_confidence() {
        let dir = TempArtifactDir::new();
        let content = r#"{"intent":"x","scenario":"y","symptoms":[],"affected_subsystems":[],"failure_modes":[],"system_properties":[],"entities":[],"constraints":[],"triggers":[],"observability_signals":[],"unresolved_terms":[],"rejected_nearby_terms":[],"confidence":"very_high"}"#;
        let client = MockModelClient::new(ModelGenerationResponse {
            content: content.to_string(),
            ..valid_response()
        });
        let err = make_qs(&dir, client)
            .unwrap()
            .structure(&make_request("q"))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            QueryStructuringError::InvalidModelOutput { .. }
        ));
    }

    #[tokio::test]
    async fn structure_fails_when_failure_modes_exceed_one() {
        let dir = TempArtifactDir::new();
        let content = r#"{"intent":"x","scenario":"y","symptoms":[],"affected_subsystems":[],"failure_modes":[{"term":"a","evidence_span":"e1","support_level":"explicit"},{"term":"b","evidence_span":"e2","support_level":"explicit"}],"system_properties":[],"entities":[],"constraints":[],"triggers":[],"observability_signals":[],"unresolved_terms":[],"rejected_nearby_terms":[],"confidence":"medium"}"#;
        let client = MockModelClient::new(ModelGenerationResponse {
            content: content.to_string(),
            ..valid_response()
        });
        let err = make_qs(&dir, client)
            .unwrap()
            .structure(&make_request("q"))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            QueryStructuringError::InvalidModelOutput { reason, .. }
            if reason == "failure_modes must contain at most one item"
        ));
    }

    #[tokio::test]
    async fn structure_fails_with_finish_reason_length() {
        let dir = TempArtifactDir::new();
        let client = MockModelClient::new(ModelGenerationResponse {
            content: r#"{"intent": "truncated"#.to_string(),
            finish_reason: Some(ModelFinishReason::Length),
            ..valid_response()
        });
        let err = make_qs(&dir, client)
            .unwrap()
            .structure(&make_request("q"))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            QueryStructuringError::InvalidModelOutput { reason, .. }
            if reason == "model output truncated: finish_reason was length"
        ));
    }

    #[tokio::test]
    async fn structure_fails_with_non_stop_finish_reason() {
        let dir = TempArtifactDir::new();
        let client = MockModelClient::new(ModelGenerationResponse {
            finish_reason: Some(ModelFinishReason::ContentFilter),
            ..valid_response()
        });
        let err = make_qs(&dir, client)
            .unwrap()
            .structure(&make_request("q"))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            QueryStructuringError::InvalidModelOutput { .. }
        ));
    }

    // ── structure() — InvalidModelOutput metadata preservation ───────────────

    #[tokio::test]
    async fn invalid_model_output_preserves_token_usage() {
        let dir = TempArtifactDir::new();
        let client = MockModelClient::new(ModelGenerationResponse {
            content: "bad json".to_string(),
            finish_reason: Some(ModelFinishReason::Stop),
            prompt_tokens: Some(200),
            completion_tokens: Some(30),
            total_tokens: Some(230),
        });
        let err = make_qs(&dir, client)
            .unwrap()
            .structure(&make_request("q"))
            .await
            .unwrap_err();
        let QueryStructuringError::InvalidModelOutput { token_usage, .. } = err else {
            panic!("expected InvalidModelOutput");
        };
        assert_eq!(token_usage.prompt_tokens, Some(200));
        assert_eq!(token_usage.completion_tokens, Some(30));
        assert_eq!(token_usage.total_tokens, Some(230));
    }

    #[tokio::test]
    async fn invalid_model_output_preserves_finish_reason() {
        let dir = TempArtifactDir::new();
        let client = MockModelClient::new(ModelGenerationResponse {
            content: "bad json".to_string(),
            finish_reason: Some(ModelFinishReason::Stop),
            ..valid_response()
        });
        let err = make_qs(&dir, client)
            .unwrap()
            .structure(&make_request("q"))
            .await
            .unwrap_err();
        let QueryStructuringError::InvalidModelOutput { finish_reason, .. } = err else {
            panic!("expected InvalidModelOutput");
        };
        assert!(matches!(finish_reason, Some(ModelFinishReason::Stop)));
    }

    // ── Constructor — InvalidControlledVocabulary ─────────────────────────────

    #[test]
    fn new_fails_when_vocabulary_arrays_are_empty() {
        let dir = TempArtifactDir::new();
        let bad_vocab = dir
            .write_json(
                "empty_vocab.json",
                r#"{"version":"v1","canonical_symptoms":[],"affected_components":["x"],"failure_mode_candidates":["x"],"violated_properties":["x"]}"#,
            )
            .to_str()
            .unwrap()
            .to_string();
        let err = QueryStructuring::new(
            make_settings(&bad_vocab, &write_prompt(&dir)),
            noop_client(),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            QueryStructuringError::InvalidControlledVocabulary(_)
        ));
    }

    // ── Constructor — InvalidPromptAsset for empty fields ─────────────────────

    #[test]
    fn new_fails_when_prompt_version_is_empty() {
        let dir = TempArtifactDir::new();
        let p = dir
            .write_json(
                "p.json",
                r#"{"version":"","system_prompt":"sys","user_template":"{{normalized_query}} {{controlled_vocabulary_json}}","response_schema":{}}"#,
            )
            .to_str()
            .unwrap()
            .to_string();
        let err = QueryStructuring::new(make_settings(&write_vocab(&dir), &p), noop_client())
            .unwrap_err();
        assert!(matches!(err, QueryStructuringError::InvalidPromptAsset(_)));
    }

    #[test]
    fn new_fails_when_user_template_has_unknown_placeholder() {
        let dir = TempArtifactDir::new();
        let p = dir
            .write_json(
                "p.json",
                r#"{"version":"v1","system_prompt":"sys","user_template":"{{normalized_query}} {{controlled_vocabulary_json}} {{unknown}}","response_schema":{}}"#,
            )
            .to_str()
            .unwrap()
            .to_string();
        let err = QueryStructuring::new(make_settings(&write_vocab(&dir), &p), noop_client())
            .unwrap_err();
        assert!(matches!(err, QueryStructuringError::InvalidPromptAsset(_)));
    }

    // ── structure() — finish_reason absent ───────────────────────────────────

    #[tokio::test]
    async fn structure_succeeds_when_finish_reason_is_absent() {
        let dir = TempArtifactDir::new();
        let client = MockModelClient::new(ModelGenerationResponse {
            finish_reason: None,
            ..valid_response()
        });
        assert!(make_qs(&dir, client)
            .unwrap()
            .structure(&make_request("q"))
            .await
            .is_ok());
    }

    // ── Helpers for golden-eval context tests ─────────────────────────────────

    fn empty_vocab_field() -> GoldenVocabularyFieldTargets {
        GoldenVocabularyFieldTargets {
            strict_vocabulary_terms: vec![],
            soft_vocabulary_terms: vec![],
            graded_relevance: vec![],
        }
    }

    fn make_golden_question(query: &str) -> GoldenQuestion {
        GoldenQuestion {
            case_id: "test-q001".to_string(),
            query: GoldenQuestionQuery { raw: query.to_string(), observations: vec![] },
            expected_query_structuring: GoldenQueryStructuringTargets {
                symptoms: empty_vocab_field(),
                affected_subsystems: empty_vocab_field(),
                failure_modes: empty_vocab_field(),
                system_properties: empty_vocab_field(),
            },
            expected_candidate_cards: GoldenCandidateCardSection {
                retrieval_relevant_cards: GoldenCardRetrievalTargets {
                    strict_card_ids: vec![],
                    soft_card_ids: vec![],
                    graded_relevance: vec![],
                },
            },
            expected_incident_evidence: GoldenIncidentEvidenceTargets {
                primary_card_evidence_query: GoldenChunkRetrievalCallTargets {
                    retrieval_call_id: "primary".to_string(),
                    relevance_judgments: GoldenChunkRetrievalTargets {
                        strict_chunk_ids: vec![],
                        soft_chunk_ids: vec![],
                        graded_relevance: vec![],
                    },
                },
                alternative_cards_evidence_query: GoldenChunkRetrievalCallTargets {
                    retrieval_call_id: "alternatives".to_string(),
                    relevance_judgments: GoldenChunkRetrievalTargets {
                        strict_chunk_ids: vec![],
                        soft_chunk_ids: vec![],
                        graded_relevance: vec![],
                    },
                },
            },
            expected_theory_evidence: GoldenTheoryEvidenceTargets {
                mechanism_explanation: GoldenChunkRetrievalTargets {
                    strict_chunk_ids: vec![],
                    soft_chunk_ids: vec![],
                    graded_relevance: vec![],
                },
            },
        }
    }

    fn golden_context(query: &str) -> Context {
        Context::new(
            OpenInferenceContext { root_span: tracing::Span::none() },
            Some(make_golden_question(query)),
        )
    }

    // Matches the vocabulary written by write_vocab() from VOCAB_JSON
    fn test_vocab() -> QueryStructuringControlledVocabulary {
        QueryStructuringControlledVocabulary {
            canonical_symptoms: vec!["high_latency".to_string()],
            affected_components: vec!["api_gateway".to_string()],
            failure_mode_candidates: vec!["overload".to_string()],
            violated_properties: vec!["availability".to_string()],
        }
    }

    // ── structure_with_context — metrics attachment ───────────────────────────

    #[tokio::test]
    async fn structure_with_context_attaches_metrics_when_golden_question_present() {
        let dir = TempArtifactDir::new();
        let client = MockModelClient::new(valid_response());
        let qs = make_qs(&dir, client).unwrap();
        let request = make_request("high latency issue");
        let context = golden_context(&request.query);

        let output = qs.structure_with_context(&request, &context).await.unwrap();
        assert!(
            output.metrics.is_some(),
            "metrics must be Some when golden_question is present"
        );

        // Verify the attached value equals what the helper produces for the same inputs
        let golden_question = context.golden_question.as_ref().unwrap();
        let expected = compute_query_structuring_metrics(
            &output.structured_query,
            &golden_question.expected_query_structuring,
            &test_vocab(),
            &request.query,
        )
        .unwrap();
        assert_eq!(output.metrics.unwrap(), expected);
    }

    #[tokio::test]
    async fn structure_with_context_metrics_is_none_when_no_golden_question() {
        let dir = TempArtifactDir::new();
        let client = MockModelClient::new(valid_response());
        let output = make_qs(&dir, client)
            .unwrap()
            .structure_with_context(&make_request("q"), &Context::noop())
            .await
            .unwrap();
        assert!(output.metrics.is_none());
    }

    #[tokio::test]
    async fn structure_with_context_returns_metrics_computation_error_on_helper_failure() {
        let dir = TempArtifactDir::new();
        let client = MockModelClient::new(valid_response());
        let qs = make_qs(&dir, client).unwrap();
        let request = make_request("q");

        // "not_in_vocab" is not in canonical_symptoms → InconsistentVocabularyMapping
        // from the helper, which must be surfaced as MetricsComputation
        let bad_field = GoldenVocabularyFieldTargets {
            strict_vocabulary_terms: vec!["not_in_vocab".to_string()],
            soft_vocabulary_terms: vec!["not_in_vocab".to_string()],
            graded_relevance: vec![GoldenTermRelevance {
                term: "not_in_vocab".to_string(),
                score: 1.0,
            }],
        };
        let mut golden = make_golden_question(&request.query);
        golden.expected_query_structuring.symptoms = bad_field;

        let context = Context::new(
            OpenInferenceContext { root_span: tracing::Span::none() },
            Some(golden),
        );

        let err = qs.structure_with_context(&request, &context).await.unwrap_err();
        assert!(
            matches!(err, QueryStructuringError::MetricsComputation { .. }),
            "expected MetricsComputation, got: {err}"
        );
    }

    // ── substitute_template — vocab placeholder precedes query ────────────────

    #[tokio::test]
    async fn structure_correct_when_vocab_placeholder_precedes_query_in_template() {
        let dir = TempArtifactDir::new();
        let p = dir
            .write_json(
                "p_reversed.json",
                r#"{"version":"v1","system_prompt":"sys","user_template":"Vocab: {{controlled_vocabulary_json}} Query: {{normalized_query}}","response_schema":{"type":"object"}}"#,
            )
            .to_str()
            .unwrap()
            .to_string();
        let (client, captured) = MockModelClient::new_with_capture(valid_response());
        QueryStructuring::new(make_settings(&write_vocab(&dir), &p), client)
            .unwrap()
            .structure(&make_request("hello world"))
            .await
            .unwrap();

        let req = captured.lock().unwrap().take().unwrap();
        let user_msg = &req.messages[1].content;
        let vocab_pos = user_msg.find("canonical_symptoms").unwrap();
        let query_pos = user_msg.find("hello world").unwrap();
        assert!(
            vocab_pos < query_pos,
            "vocab JSON should appear before query text when template has vocab-first order"
        );
    }
}
