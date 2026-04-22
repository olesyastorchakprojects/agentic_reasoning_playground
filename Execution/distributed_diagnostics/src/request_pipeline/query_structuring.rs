use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::api_clients::model::{
    ModelClient, ModelClientError, ModelFinishReason, ModelGenerationRequest, ModelMessage,
    ModelMessageRole, ModelResponseMode,
};
use crate::config::QueryStructuringSettings;
use crate::shared_types::{
    ModelTokenUsage, NormalizedUserRequest, QueryStructuringOutput, StructuredUserQuery,
};

const QUERY_PLACEHOLDER: &str = "{{normalized_query}}";
const VOCAB_PLACEHOLDER: &str = "{{controlled_vocabulary_json}}";

// ── Module-private asset types ────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct QueryStructuringControlledVocabulary {
    canonical_symptoms: Vec<String>,
    affected_components: Vec<String>,
    failure_mode_candidates: Vec<String>,
    violated_properties: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct QueryStructuringPromptAsset {
    version: String,
    system_prompt: String,
    user_template: String,
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
        })
    }

    pub async fn structure(
        &self,
        request: &NormalizedUserRequest,
    ) -> Result<QueryStructuringOutput, QueryStructuringError> {
        let vocab_json = serde_json::to_string(&self.controlled_vocabulary)
            .expect("QueryStructuringControlledVocabulary serialization must not fail");

        let user_message = substitute_template(
            &self.prompt_asset.user_template,
            &request.query,
            &vocab_json,
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
            response_mode: ModelResponseMode::JsonObject,
        };

        let response = self.model_client.generate(&model_request).await?;

        let token_usage = ModelTokenUsage {
            prompt_tokens: response.prompt_tokens,
            completion_tokens: response.completion_tokens,
            total_tokens: response.total_tokens,
        };
        let finish_reason = response.finish_reason;
        let content = response.content;

        let is_acceptable_finish = matches!(&finish_reason, Some(ModelFinishReason::Stop) | None);
        if !is_acceptable_finish {
            let reason = if matches!(finish_reason, Some(ModelFinishReason::Length)) {
                "model output truncated: finish_reason was length"
            } else {
                "model returned unusable finish reason"
            };
            return Err(QueryStructuringError::InvalidModelOutput {
                reason: reason.to_string(),
                token_usage,
                finish_reason,
            });
        }

        let parse_token_usage = token_usage.clone();
        let parse_finish_reason = finish_reason.clone();
        let structured_query: StructuredUserQuery =
            serde_json::from_str(&content).map_err(|_| {
                QueryStructuringError::InvalidModelOutput {
                    reason: "failed to parse model output as StructuredUserQuery".to_string(),
                    token_usage: parse_token_usage,
                    finish_reason: parse_finish_reason,
                }
            })?;

        if structured_query.failure_modes.len() > 1 {
            return Err(QueryStructuringError::InvalidModelOutput {
                reason: "failure_modes must contain at most one item".to_string(),
                token_usage,
                finish_reason,
            });
        }

        Ok(QueryStructuringOutput {
            structured_query,
            token_usage,
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
    use crate::shared_types::{NormalizedUserRequest, StructuredUserQueryConfidence};
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
        "canonical_symptoms": ["high_latency"],
        "affected_components": ["api_gateway"],
        "failure_mode_candidates": ["overload"],
        "violated_properties": ["availability"]
    }"#;

    const PROMPT_JSON: &str = r#"{
        "version": "v1",
        "system_prompt": "You are a helpful assistant.",
        "user_template": "Query: {{normalized_query}}\nVocabulary: {{controlled_vocabulary_json}}"
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
                r#"{"version":"v1","system_prompt":"sys","user_template":"Vocab: {{controlled_vocabulary_json}}"}"#,
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
                r#"{"version":"v1","system_prompt":"sys","user_template":"Query: {{normalized_query}}"}"#,
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
    async fn structure_sends_json_object_response_mode() {
        let dir = TempArtifactDir::new();
        let (client, captured) = MockModelClient::new_with_capture(valid_response());
        make_qs(&dir, client)
            .unwrap()
            .structure(&make_request("q"))
            .await
            .unwrap();

        let req = captured.lock().unwrap().take().unwrap();
        assert!(matches!(req.response_mode, ModelResponseMode::JsonObject));
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
                r#"{"canonical_symptoms":[],"affected_components":["x"],"failure_mode_candidates":["x"],"violated_properties":["x"]}"#,
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
                r#"{"version":"","system_prompt":"sys","user_template":"{{normalized_query}} {{controlled_vocabulary_json}}"}"#,
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
                r#"{"version":"v1","system_prompt":"sys","user_template":"{{normalized_query}} {{controlled_vocabulary_json}} {{unknown}}"}"#,
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

    // ── substitute_template — vocab placeholder precedes query ────────────────

    #[tokio::test]
    async fn structure_correct_when_vocab_placeholder_precedes_query_in_template() {
        let dir = TempArtifactDir::new();
        let p = dir
            .write_json(
                "p_reversed.json",
                r#"{"version":"v1","system_prompt":"sys","user_template":"Vocab: {{controlled_vocabulary_json}} Query: {{normalized_query}}"}"#,
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
