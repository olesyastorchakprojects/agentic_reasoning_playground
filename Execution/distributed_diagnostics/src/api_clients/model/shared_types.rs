pub use crate::utils::retry::{RetryBackoffKind, RetryPolicyConfig};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelMessageRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMessage {
    pub role: ModelMessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelResponseMode {
    Text,
    JsonObject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelGenerationRequest {
    pub messages: Vec<ModelMessage>,
    pub temperature: f32,
    pub max_output_tokens: Option<u32>,
    pub response_mode: ModelResponseMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModelFinishReason {
    Stop,
    Length,
    ContentFilter,
    ToolCalls,
    Unknown(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelGenerationResponse {
    pub content: String,
    pub finish_reason: Option<ModelFinishReason>,
    pub prompt_tokens: Option<usize>,
    pub completion_tokens: Option<usize>,
    pub total_tokens: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct TogetherModelClientConfig {
    pub base_url: url::Url,
    pub api_key: String,
    pub model_name: String,
    pub timeout_sec: u64,
}

#[derive(Debug, Clone)]
pub struct OllamaModelClientConfig {
    pub base_url: url::Url,
    pub model_name: String,
    pub timeout_sec: u64,
}
