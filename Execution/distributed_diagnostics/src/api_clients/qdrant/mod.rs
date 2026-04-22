pub mod cards_collection;
pub mod dense_search_client;
pub mod hybrid_search_client;
pub mod practice_chunks_collection;
pub mod shared_types;
mod sparse_preparation;
pub mod theory_chunks_collection;

use thiserror::Error;

pub use cards_collection::{
    CardSearchHit, CardSearchRequest, CardSearchResult, CardsCollection, CardsCollectionError,
    QdrantCardsCollectionDense, QdrantCardsCollectionHybrid,
};
pub use dense_search_client::{
    DenseSearchClientError, DenseSearchRequest, DenseSearchResponse, QdrantDenseSearchClient,
};
pub use hybrid_search_client::{
    HybridSearchClientError, HybridSearchRequest, HybridSearchResponse, QdrantHybridSearchClient,
};
pub use practice_chunks_collection::{
    PracticeChunkFilter, PracticeChunkSearchHit, PracticeChunkSearchRequest,
    PracticeChunkSearchResult, PracticeChunksCollection, PracticeChunksCollectionError,
    QdrantPracticeChunksCollectionDense, QdrantPracticeChunksCollectionHybrid,
};
pub use shared_types::{
    Bm25TermStatsArtifact, Embedding, EmbeddingConfig, NormalizedUserQuery, QdrantCollectionName,
    QdrantDenseCollectionConfig, QdrantFilter, QdrantHybridCollectionConfig, QdrantMatchAnyFilter,
    QdrantPayloadValue, QdrantVectorName, RawQdrantHit, RawQdrantPayload, RetryBackoffKind,
    RetryPolicyConfig, SparseStrategyConfig, SparseVector, SparseVocabularyArtifact,
};
pub use theory_chunks_collection::{
    QdrantTheoryChunksCollectionDense, QdrantTheoryChunksCollectionHybrid, TheoryChunkSearchHit,
    TheoryChunkSearchRequest, TheoryChunkSearchResult, TheoryChunksCollection,
    TheoryChunksCollectionError,
};

#[derive(Debug, Error)]
pub enum QdrantApiClientError {
    #[error("dense search: {0}")]
    DenseSearch(#[from] DenseSearchClientError),
    #[error("hybrid search: {0}")]
    HybridSearch(#[from] HybridSearchClientError),
    #[error("cards collection: {0}")]
    CardsCollection(#[from] CardsCollectionError),
    #[error("practice chunks collection: {0}")]
    PracticeChunksCollection(#[from] PracticeChunksCollectionError),
    #[error("theory chunks collection: {0}")]
    TheoryChunksCollection(#[from] TheoryChunksCollectionError),
}
