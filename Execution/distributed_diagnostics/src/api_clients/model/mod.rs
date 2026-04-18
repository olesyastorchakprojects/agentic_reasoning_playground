pub mod model_client;
pub mod ollama_client;
pub mod shared_types;
pub mod together_client;

use thiserror::Error;

pub use model_client::{ModelClient, ModelClientError};
pub use ollama_client::OllamaModelClient;
pub use shared_types::{
    ModelFinishReason, ModelGenerationRequest, ModelGenerationResponse, ModelMessage,
    ModelMessageRole, ModelResponseMode, OllamaModelClientConfig, RetryBackoffKind,
    RetryPolicyConfig, TogetherModelClientConfig,
};
pub use together_client::TogetherModelClient;

#[derive(Debug, Error)]
pub enum ModelApiClientError {
    #[error("model client: {0}")]
    Client(#[from] ModelClientError),
}
