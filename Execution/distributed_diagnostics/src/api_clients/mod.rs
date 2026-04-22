pub mod embedding_client;
pub mod model;
pub mod postgres;
pub mod qdrant;

use thiserror::Error;

pub use embedding_client::EmbeddingClientError;
pub use model::{ModelApiClientError, ModelClientError};
pub use postgres::{IncidentCardStoreError, PostgresApiClientError};
pub use qdrant::{
    CardsCollectionError, DenseSearchClientError, HybridSearchClientError,
    PracticeChunksCollectionError, QdrantApiClientError, TheoryChunksCollectionError,
};

#[derive(Debug, Error)]
pub enum ApiClientError {
    #[error("embedding client: {0}")]
    Embedding(#[from] EmbeddingClientError),
    #[error("model api clients: {0}")]
    Model(#[from] ModelApiClientError),
    #[error("qdrant api clients: {0}")]
    Qdrant(#[from] QdrantApiClientError),
    #[error("postgres api clients: {0}")]
    Postgres(#[from] PostgresApiClientError),
}
