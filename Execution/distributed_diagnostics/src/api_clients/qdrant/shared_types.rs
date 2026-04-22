use crate::config::{
    CollectionRetrievalSettings, CollectionSettings, EmbeddingModelSettings, SparseStrategySettings,
};
pub use crate::utils::retry::{RetryBackoffKind, RetryPolicyConfig};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ─── Newtype wrappers ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QdrantCollectionName(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QdrantVectorName(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedUserQuery(pub String);

// ─── Config types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingConfig {
    pub base_url: url::Url,
    pub model_name: String,
    pub embedding_dimension: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QdrantDenseCollectionConfig {
    pub qdrant_base_url: url::Url,
    pub collection_name: QdrantCollectionName,
    pub vector_name: Option<QdrantVectorName>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QdrantHybridCollectionConfig {
    pub qdrant_base_url: url::Url,
    pub collection_name: QdrantCollectionName,
    pub vector_name: QdrantVectorName,
    pub sparse_vector_name: QdrantVectorName,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SparseStrategyConfig {
    BagOfWords {
        sparse_vocabulary_path: String,
    },
    Bm25Like {
        sparse_vocabulary_path: String,
        bm25_term_stats_path: String,
        k1: f32,
        b: f32,
        idf_smoothing: f32,
    },
}

// ─── Artifact types (loaded once, cached for collection lifetime) ────────────

/// On-disk JSON structure for loading; text_processing fields are kept so
/// the tokenizer can apply the same normalization used during vocabulary bootstrap.
#[derive(Debug, Clone, Deserialize)]
struct SparseVocabularyJson {
    pub vocabulary_name: String,
    pub collection_name: String,
    pub text_processing: TextProcessingConfig,
    pub tokenizer: TokenizerConfig,
    pub tokens: Vec<TokenEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TextProcessingConfig {
    pub lowercase: bool,
    pub min_token_length: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TokenizerConfig {
    pub library: String,
    pub source: String,
    pub revision: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TokenEntry {
    pub token: String,
    pub token_id: u32,
}

/// In-memory representation of the sparse vocabulary artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SparseVocabularyArtifact {
    pub vocabulary_name: String,
    pub collection_name: String,
    pub token_id_by_token: BTreeMap<String, u32>,
    pub lowercase: bool,
    pub min_token_length: usize,
    pub tokenizer_library: String,
    pub tokenizer_source: String,
    pub tokenizer_revision: Option<String>,
}

impl SparseVocabularyArtifact {
    pub fn load_from_file(path: &str) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read vocabulary file {path}: {e}"))?;
        let json: SparseVocabularyJson = serde_json::from_str(&content)
            .map_err(|e| format!("invalid vocabulary JSON at {path}: {e}"))?;

        let token_id_by_token: BTreeMap<String, u32> = json
            .tokens
            .into_iter()
            .map(|e| (e.token, e.token_id))
            .collect();

        Ok(Self {
            vocabulary_name: json.vocabulary_name,
            collection_name: json.collection_name,
            token_id_by_token,
            lowercase: json.text_processing.lowercase,
            min_token_length: json.text_processing.min_token_length,
            tokenizer_library: json.tokenizer.library,
            tokenizer_source: json.tokenizer.source,
            tokenizer_revision: json.tokenizer.revision,
        })
    }
}

/// In-memory representation of BM25 corpus statistics.
#[derive(Debug, Clone, PartialEq)]
pub struct Bm25TermStatsArtifact {
    pub collection_name: String,
    pub vocabulary_name: String,
    pub document_count: u64,
    pub average_document_length: f64,
    pub document_frequency_by_token_id: BTreeMap<u32, u64>,
}

impl Bm25TermStatsArtifact {
    pub fn load_from_file(path: &str) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read bm25 term stats file {path}: {e}"))?;
        let json: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| format!("invalid bm25 term stats JSON at {path}: {e}"))?;

        let collection_name = json["collection_name"]
            .as_str()
            .ok_or("missing collection_name")?
            .to_string();
        let vocabulary_name = json["vocabulary_name"]
            .as_str()
            .ok_or("missing vocabulary_name")?
            .to_string();
        let document_count = json["document_count"]
            .as_u64()
            .ok_or("missing or invalid document_count")?;
        if document_count == 0 {
            return Err("document_count must be > 0".into());
        }
        let average_document_length = json["average_document_length"]
            .as_f64()
            .ok_or("missing or invalid average_document_length")?;
        if average_document_length <= 0.0 {
            return Err("average_document_length must be positive".into());
        }

        let df_obj = json["document_frequency_by_token_id"]
            .as_object()
            .ok_or("missing document_frequency_by_token_id")?;

        let mut document_frequency_by_token_id = BTreeMap::new();
        for (k, v) in df_obj {
            let id: u32 = k
                .parse()
                .map_err(|_| format!("non-integer token id key: {k}"))?;
            let freq = v
                .as_u64()
                .ok_or_else(|| format!("invalid df value for id {id}"))?;
            if freq == 0 {
                return Err(format!("document_frequency must be > 0 for token id {id}"));
            }
            if freq > document_count {
                return Err(format!(
                    "df {freq} > document_count {document_count} for token {id}"
                ));
            }
            document_frequency_by_token_id.insert(id, freq);
        }

        Ok(Self {
            collection_name,
            vocabulary_name,
            document_count,
            average_document_length,
            document_frequency_by_token_id,
        })
    }
}

// ─── Vector types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Embedding {
    pub values: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SparseVector {
    pub indices: Vec<u32>,
    pub values: Vec<f32>,
}

// ─── Filter types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QdrantMatchAnyFilter {
    pub field_name: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QdrantFilter {
    pub must_match_any: Vec<QdrantMatchAnyFilter>,
}

// ─── Raw hit types (transport layer) ─────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct RawQdrantPayload {
    pub fields: BTreeMap<String, QdrantPayloadValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum QdrantPayloadValue {
    String(String),
    StringList(Vec<String>),
    Number(f64),
    Bool(bool),
    Null,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawQdrantHit {
    pub score: f32,
    pub payload: RawQdrantPayload,
}

pub(crate) fn embedding_config_from_settings(
    settings: &EmbeddingModelSettings,
) -> Result<EmbeddingConfig, String> {
    let base_url =
        url::Url::parse(&settings.url).map_err(|e| format!("invalid embedding model url: {e}"))?;

    Ok(EmbeddingConfig {
        base_url,
        model_name: settings.name.clone(),
        embedding_dimension: settings.dimension,
    })
}

pub(crate) fn dense_collection_config_from_settings(
    collection_settings: &CollectionRetrievalSettings,
    qdrant_url: &str,
) -> Result<QdrantDenseCollectionConfig, String> {
    let qdrant_base_url =
        url::Url::parse(qdrant_url).map_err(|e| format!("invalid qdrant url: {e}"))?;

    let dense = match &collection_settings.collection {
        CollectionSettings::Dense(settings) => settings,
        CollectionSettings::Hybrid(_) => {
            return Err("expected dense collection settings variant".to_string())
        }
    };

    Ok(QdrantDenseCollectionConfig {
        qdrant_base_url,
        collection_name: QdrantCollectionName(dense.name.clone()),
        vector_name: Some(QdrantVectorName(dense.vector_name.clone())),
    })
}

pub(crate) fn hybrid_collection_config_from_settings(
    collection_settings: &CollectionRetrievalSettings,
    qdrant_url: &str,
) -> Result<(QdrantHybridCollectionConfig, SparseStrategyConfig), String> {
    let qdrant_base_url =
        url::Url::parse(qdrant_url).map_err(|e| format!("invalid qdrant url: {e}"))?;

    let hybrid = match &collection_settings.collection {
        CollectionSettings::Hybrid(settings) => settings,
        CollectionSettings::Dense(_) => {
            return Err("expected hybrid collection settings variant".to_string())
        }
    };

    let (collection_name, sparse) = match &hybrid.sparse.strategy {
        SparseStrategySettings::BagOfWords(settings) => (
            settings.name.clone(),
            SparseStrategyConfig::BagOfWords {
                sparse_vocabulary_path: settings.sparse_vocabulary_path.clone(),
            },
        ),
        SparseStrategySettings::Bm25Like(settings) => (
            settings.name.clone(),
            SparseStrategyConfig::Bm25Like {
                sparse_vocabulary_path: settings.sparse_vocabulary_path.clone(),
                bm25_term_stats_path: settings.bm25_term_stats_path.clone(),
                k1: settings.k1,
                b: settings.b,
                idf_smoothing: settings.idf_smoothing,
            },
        ),
    };

    Ok((
        QdrantHybridCollectionConfig {
            qdrant_base_url,
            collection_name: QdrantCollectionName(collection_name),
            vector_name: QdrantVectorName(hybrid.dense_vector_name.clone()),
            sparse_vector_name: QdrantVectorName(hybrid.sparse_vector_name.clone()),
        },
        sparse,
    ))
}
