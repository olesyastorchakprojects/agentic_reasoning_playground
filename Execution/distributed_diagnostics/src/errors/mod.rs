use thiserror::Error;

pub use crate::api_clients::ApiClientError;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("api clients: {0}")]
    ApiClients(#[from] ApiClientError),
}
