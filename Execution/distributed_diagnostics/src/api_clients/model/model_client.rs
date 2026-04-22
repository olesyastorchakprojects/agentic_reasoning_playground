use async_trait::async_trait;

use super::shared_types::{ModelGenerationRequest, ModelGenerationResponse};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, thiserror::Error)]
pub enum ModelClientError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("transport failure: {0}")]
    Transport(String),
    #[error("unexpected HTTP status: {0}")]
    UnexpectedStatus(u16),
    #[error("invalid response: {0}")]
    InvalidResponse(String),
}

#[async_trait]
pub trait ModelClient: Send + Sync {
    async fn generate(
        &self,
        request: &ModelGenerationRequest,
    ) -> Result<ModelGenerationResponse, ModelClientError>;
}

/// Shared request validation used by all concrete model clients.
pub fn validate_request(req: &ModelGenerationRequest) -> Result<(), ModelClientError> {
    if req.messages.is_empty() {
        return Err(ModelClientError::InvalidRequest(
            "messages must not be empty".to_string(),
        ));
    }
    for msg in &req.messages {
        if msg.content.trim().is_empty() {
            return Err(ModelClientError::InvalidRequest(
                "message content must not be empty".to_string(),
            ));
        }
    }
    if !req.temperature.is_finite() || req.temperature < 0.0 {
        return Err(ModelClientError::InvalidRequest(
            "temperature must be finite and non-negative".to_string(),
        ));
    }
    if let Some(max_tokens) = req.max_output_tokens {
        if max_tokens == 0 {
            return Err(ModelClientError::InvalidRequest(
                "max_output_tokens must be > 0".to_string(),
            ));
        }
    }
    Ok(())
}
