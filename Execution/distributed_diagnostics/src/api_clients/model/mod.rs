pub mod model_client;
pub mod ollama_client;
pub mod openai_client;
pub mod shared_types;

use thiserror::Error;

pub use model_client::{ModelClient, ModelClientError};
pub use ollama_client::OllamaModelClient;
pub use openai_client::OpenAiModelClient;
pub use shared_types::{
    ModelFinishReason, ModelGenerationRequest, ModelGenerationResponse, ModelMessage,
    ModelMessageRole, ModelResponseMode, OllamaModelClientConfig, OpenAiModelClientConfig,
    RetryBackoffKind, RetryPolicyConfig,
};

#[derive(Debug, Error)]
pub enum ModelApiClientError {
    #[error("model client: {0}")]
    Client(#[from] ModelClientError),
}
