use std::collections::BTreeMap;

use crate::utils::tokenizer::HfTokenizer;

use super::shared_types::{
    Bm25TermStatsArtifact, SparseStrategyConfig, SparseVector, SparseVocabularyArtifact,
};

#[derive(Debug)]
pub struct LoadedSparseArtifacts {
    pub vocabulary: SparseVocabularyArtifact,
    pub bm25_term_stats: Option<Bm25TermStatsArtifact>,
    pub tokenizer: HfTokenizer,
}

pub async fn load_sparse_artifacts(
    strategy: &SparseStrategyConfig,
    collection_name: &str,
) -> Result<LoadedSparseArtifacts, &'static str> {
    let (vocab_path, stats_path) = match strategy {
        SparseStrategyConfig::BagOfWords {
            sparse_vocabulary_path,
        } => (sparse_vocabulary_path.as_str(), None),
        SparseStrategyConfig::Bm25Like {
            sparse_vocabulary_path,
            bm25_term_stats_path,
            ..
        } => (
            sparse_vocabulary_path.as_str(),
            Some(bm25_term_stats_path.as_str()),
        ),
    };

    let vocabulary = SparseVocabularyArtifact::load_from_file(vocab_path)
        .map_err(|_| "failed to load sparse vocabulary")?;

    if vocabulary.tokenizer_library != "tokenizers" {
        return Err("unsupported tokenizer library in sparse vocabulary");
    }

    let tokenizer = HfTokenizer::load(&vocabulary.tokenizer_source)
        .await
        .map_err(|_| "failed to load tokenizer artifact")?;

    let bm25_term_stats = if let Some(path) = stats_path {
        let stats = Bm25TermStatsArtifact::load_from_file(path)
            .map_err(|_| "failed to load bm25 term stats")?;
        validate_artifact_compatibility(&vocabulary, &stats, collection_name)?;
        Some(stats)
    } else {
        None
    };

    Ok(LoadedSparseArtifacts {
        vocabulary,
        bm25_term_stats,
        tokenizer,
    })
}

pub fn build_sparse_vector(
    text: &str,
    tokenizer: &HfTokenizer,
    vocab: &SparseVocabularyArtifact,
    bm25_stats: Option<&Bm25TermStatsArtifact>,
    strategy: &SparseStrategyConfig,
) -> Result<SparseVector, &'static str> {
    let raw_tokens = tokenizer.tokenize(text);
    let tokens = apply_sparse_normalization(raw_tokens, vocab.lowercase, vocab.min_token_length);

    match strategy {
        SparseStrategyConfig::BagOfWords { .. } => {
            let (indices, values) = build_bag_of_words_query(&tokens, &vocab.token_id_by_token)
                .ok_or("no sparse terms after vocabulary lookup")?;
            Ok(SparseVector { indices, values })
        }
        SparseStrategyConfig::Bm25Like {
            k1,
            b,
            idf_smoothing,
            ..
        } => {
            let stats = bm25_stats.ok_or("bm25 term stats must be loaded for Bm25Like")?;
            let (indices, values) = build_bm25_like_query(
                &tokens,
                &vocab.token_id_by_token,
                stats.document_count,
                stats.average_document_length,
                &stats.document_frequency_by_token_id,
                *k1,
                *b,
                *idf_smoothing,
            )
            .ok_or("no sparse terms after vocabulary lookup")?;
            Ok(SparseVector { indices, values })
        }
    }
}

fn validate_artifact_compatibility(
    vocab: &SparseVocabularyArtifact,
    stats: &Bm25TermStatsArtifact,
    collection_name: &str,
) -> Result<(), &'static str> {
    if stats.vocabulary_name != vocab.vocabulary_name {
        return Err("bm25 term stats vocabulary_name does not match sparse vocabulary");
    }
    if stats.collection_name != collection_name {
        return Err("bm25 term stats collection_name does not match configured collection");
    }
    Ok(())
}

/// Apply sparse text space normalization rules to raw tokenizer output.
fn apply_sparse_normalization(
    tokens: Vec<String>,
    lowercase: bool,
    min_token_length: usize,
) -> Vec<String> {
    tokens
        .into_iter()
        .filter_map(|t| {
            let token = if lowercase { t.to_lowercase() } else { t };
            if token.len() < min_token_length {
                return None;
            }
            if !token.chars().any(|c| c.is_alphanumeric()) {
                return None;
            }
            if is_unknown_placeholder(&token) {
                return None;
            }
            Some(token)
        })
        .collect()
}

fn is_unknown_placeholder(token: &str) -> bool {
    matches!(token, "[unk]" | "unk")
}

fn build_bag_of_words_query(
    tokens: &[String],
    token_id_by_token: &BTreeMap<String, u32>,
) -> Option<(Vec<u32>, Vec<f32>)> {
    let mut seen: BTreeMap<u32, ()> = BTreeMap::new();
    for token in tokens {
        if let Some(&id) = token_id_by_token.get(token.as_str()) {
            seen.insert(id, ());
        }
    }

    if seen.is_empty() {
        return None;
    }

    let indices: Vec<u32> = seen.keys().copied().collect();
    let values: Vec<f32> = vec![1.0; indices.len()];
    Some((indices, values))
}

fn build_bm25_like_query(
    tokens: &[String],
    token_id_by_token: &BTreeMap<String, u32>,
    document_count: u64,
    average_document_length: f64,
    document_frequency_by_token_id: &BTreeMap<u32, u64>,
    k1: f32,
    b: f32,
    idf_smoothing: f32,
) -> Option<(Vec<u32>, Vec<f32>)> {
    let mut tf_map: BTreeMap<u32, u32> = BTreeMap::new();
    for token in tokens {
        if let Some(&id) = token_id_by_token.get(token.as_str()) {
            *tf_map.entry(id).or_insert(0) += 1;
        }
    }

    if tf_map.is_empty() {
        return None;
    }

    let doc_len = tokens.len() as f64;
    let n = document_count as f64;
    let k1 = k1 as f64;
    let b = b as f64;
    let idf_s = idf_smoothing as f64;

    let mut indices: Vec<u32> = Vec::with_capacity(tf_map.len());
    let mut values: Vec<f32> = Vec::with_capacity(tf_map.len());

    for (token_id, tf) in &tf_map {
        let df = document_frequency_by_token_id
            .get(token_id)
            .copied()
            .unwrap_or(0) as f64;
        let idf = ((n - df + idf_s) / (df + idf_s) + 1.0).ln();
        let tf_norm = (*tf as f64 * (k1 + 1.0))
            / (*tf as f64 + k1 * (1.0 - b + b * doc_len / average_document_length));

        let weight = (idf * tf_norm) as f32;
        if weight > 0.0 {
            indices.push(*token_id);
            values.push(weight);
        }
    }

    if indices.is_empty() {
        return None;
    }

    Some((indices, values))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{populate_tokenizer_cache, TempArtifactDir};

    fn vocab_json(
        vocabulary_name: &str,
        collection_name: &str,
        tokenizer_source: &str,
        tokenizer_library: &str,
    ) -> String {
        serde_json::json!({
            "vocabulary_name": vocabulary_name,
            "collection_name": collection_name,
            "text_processing": {"lowercase": true, "min_token_length": 2},
            "tokenizer": {
                "library": tokenizer_library,
                "source": tokenizer_source
            },
            "created_at": "2024-01-01T00:00:00Z",
            "tokens": [
                {"token": "service", "token_id": 0},
                {"token": "down", "token_id": 1},
                {"token": "query", "token_id": 2}
            ]
        })
        .to_string()
    }

    fn bm25_stats_json(vocabulary_name: &str, collection_name: &str) -> String {
        serde_json::json!({
            "collection_name": collection_name,
            "vocabulary_name": vocabulary_name,
            "document_count": 100,
            "average_document_length": 10.0,
            "document_frequency_by_token_id": {
                "0": 50,
                "1": 10,
                "2": 20
            }
        })
        .to_string()
    }

    const TEST_SOURCE: &str = "test/model";

    fn make_artifacts_dir() -> TempArtifactDir {
        populate_tokenizer_cache(TEST_SOURCE);
        TempArtifactDir::new()
    }

    async fn load_tokenizer() -> HfTokenizer {
        HfTokenizer::load(TEST_SOURCE).await.unwrap()
    }

    fn vocab_artifact() -> SparseVocabularyArtifact {
        SparseVocabularyArtifact {
            vocabulary_name: "cards__sparse_vocabulary".into(),
            collection_name: "cards".into(),
            token_id_by_token: BTreeMap::from([
                ("service".to_string(), 0u32),
                ("down".to_string(), 1u32),
                ("query".to_string(), 2u32),
            ]),
            lowercase: true,
            min_token_length: 2,
            tokenizer_library: "tokenizers".into(),
            tokenizer_source: TEST_SOURCE.into(),
            tokenizer_revision: None,
        }
    }

    #[tokio::test]
    async fn load_sparse_artifacts_loads_vocab_and_tokenizer_for_bag_of_words() {
        let dir = make_artifacts_dir();
        let vocab_path = dir.write_json(
            "vocab.json",
            &vocab_json(
                "cards__sparse_vocabulary",
                "cards",
                TEST_SOURCE,
                "tokenizers",
            ),
        );

        let loaded = load_sparse_artifacts(
            &SparseStrategyConfig::BagOfWords {
                sparse_vocabulary_path: vocab_path.to_str().unwrap().to_string(),
            },
            "cards",
        )
        .await
        .unwrap();

        assert_eq!(
            loaded.vocabulary.vocabulary_name,
            "cards__sparse_vocabulary"
        );
        assert!(loaded.bm25_term_stats.is_none());
        assert_eq!(
            loaded.tokenizer.tokenize("service down"),
            vec!["service".to_string(), "down".to_string()]
        );
    }

    #[tokio::test]
    async fn load_sparse_artifacts_rejects_unsupported_tokenizer_library() {
        let dir = make_artifacts_dir();
        let vocab_path = dir.write_json(
            "vocab.json",
            &vocab_json("cards__sparse_vocabulary", "cards", TEST_SOURCE, "other"),
        );

        let err = load_sparse_artifacts(
            &SparseStrategyConfig::BagOfWords {
                sparse_vocabulary_path: vocab_path.to_str().unwrap().to_string(),
            },
            "cards",
        )
        .await
        .unwrap_err();

        assert_eq!(err, "unsupported tokenizer library in sparse vocabulary");
    }

    #[tokio::test]
    async fn load_sparse_artifacts_rejects_incompatible_bm25_stats() {
        let dir = make_artifacts_dir();
        let vocab_path = dir.write_json(
            "vocab.json",
            &vocab_json(
                "cards__sparse_vocabulary",
                "cards",
                TEST_SOURCE,
                "tokenizers",
            ),
        );
        let stats_path = dir.write_json(
            "stats.json",
            &bm25_stats_json("wrong__sparse_vocabulary", "cards"),
        );

        let err = load_sparse_artifacts(
            &SparseStrategyConfig::Bm25Like {
                sparse_vocabulary_path: vocab_path.to_str().unwrap().to_string(),
                bm25_term_stats_path: stats_path.to_str().unwrap().to_string(),
                k1: 1.5,
                b: 0.75,
                idf_smoothing: 0.5,
            },
            "cards",
        )
        .await
        .unwrap_err();

        assert_eq!(
            err,
            "bm25 term stats vocabulary_name does not match sparse vocabulary"
        );
    }

    // ── apply_sparse_normalization ───────────────────────────────────────────

    #[test]
    fn normalization_applies_lowercase_and_filters_short_and_non_alnum_tokens() {
        let tokens = vec![
            "Service".to_string(),
            "!".to_string(),
            "a".to_string(),
            "down".to_string(),
        ];
        let result = apply_sparse_normalization(tokens, true, 2);
        assert_eq!(result, vec!["service".to_string(), "down".to_string()]);
    }

    #[test]
    fn normalization_filters_unknown_placeholders() {
        let tokens = vec![
            "[unk]".to_string(),
            "service".to_string(),
            "unk".to_string(),
        ];
        let result = apply_sparse_normalization(tokens, false, 2);
        assert_eq!(result, vec!["service".to_string()]);
    }

    // ── build_sparse_vector ──────────────────────────────────────────────────

    #[tokio::test]
    async fn build_sparse_vector_bag_of_words_deduplicates_and_sorts() {
        let _dir = make_artifacts_dir();
        let tokenizer = load_tokenizer().await;
        let vocab = vocab_artifact();

        let vector = build_sparse_vector(
            "down service down",
            &tokenizer,
            &vocab,
            None,
            &SparseStrategyConfig::BagOfWords {
                sparse_vocabulary_path: "unused".into(),
            },
        )
        .unwrap();

        assert_eq!(vector.indices, vec![0, 1]);
        assert_eq!(vector.values, vec![1.0, 1.0]);
    }

    #[tokio::test]
    async fn build_sparse_vector_rejects_zero_terms_after_vocab_lookup() {
        let _dir = make_artifacts_dir();
        let tokenizer = load_tokenizer().await;
        let vocab = vocab_artifact();

        let err = build_sparse_vector(
            "unknown tokens",
            &tokenizer,
            &vocab,
            None,
            &SparseStrategyConfig::BagOfWords {
                sparse_vocabulary_path: "unused".into(),
            },
        )
        .unwrap_err();

        assert_eq!(err, "no sparse terms after vocabulary lookup");
    }

    #[tokio::test]
    async fn build_sparse_vector_bm25_requires_stats() {
        let _dir = make_artifacts_dir();
        let tokenizer = load_tokenizer().await;
        let vocab = vocab_artifact();

        let err = build_sparse_vector(
            "service down",
            &tokenizer,
            &vocab,
            None,
            &SparseStrategyConfig::Bm25Like {
                sparse_vocabulary_path: "unused".into(),
                bm25_term_stats_path: "unused".into(),
                k1: 1.5,
                b: 0.75,
                idf_smoothing: 0.5,
            },
        )
        .unwrap_err();

        assert_eq!(err, "bm25 term stats must be loaded for Bm25Like");
    }

    #[tokio::test]
    async fn build_sparse_vector_bm25_returns_sorted_aligned_vector() {
        let _dir = make_artifacts_dir();
        let tokenizer = load_tokenizer().await;
        let vocab = vocab_artifact();
        let stats = Bm25TermStatsArtifact {
            collection_name: "cards".into(),
            vocabulary_name: "cards__sparse_vocabulary".into(),
            document_count: 100,
            average_document_length: 10.0,
            document_frequency_by_token_id: BTreeMap::from([(0u32, 50u64), (1u32, 10u64)]),
        };

        let vector = build_sparse_vector(
            "down service",
            &tokenizer,
            &vocab,
            Some(&stats),
            &SparseStrategyConfig::Bm25Like {
                sparse_vocabulary_path: "unused".into(),
                bm25_term_stats_path: "unused".into(),
                k1: 1.5,
                b: 0.75,
                idf_smoothing: 0.5,
            },
        )
        .unwrap();

        assert_eq!(vector.indices, vec![0, 1]);
        assert_eq!(vector.indices.len(), vector.values.len());
        assert!(vector.values.iter().all(|v| *v > 0.0));
    }
}
