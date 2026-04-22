use std::path::{Path, PathBuf};
use std::time::Duration;

use backon::{ExponentialBuilder, Retryable};
use tokenizers::Tokenizer;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, thiserror::Error)]
pub enum TokenizerError {
    #[error("failed to load tokenizer from {path}: {reason}")]
    Load { path: String, reason: String },
    #[error("failed to download tokenizer from {url}: {reason}")]
    Download { url: String, reason: String },
    #[error("tokenizer JSON is invalid: {reason}")]
    InvalidJson { reason: String },
    #[error("tokenizer JSON is missing required field 'model'")]
    MissingModelField,
    #[error("failed to write tokenizer cache to {path}: {reason}")]
    CacheWrite { path: String, reason: String },
}

const DOWNLOAD_TIMEOUT_SECS: u64 = 10;
const DOWNLOAD_MAX_ATTEMPTS: usize = 3;

/// Returns the root directory for cached tokenizer artifacts.
/// Cache layout: `{tokenizer_cache_root()}/{source}/tokenizer.json`.
pub(crate) fn tokenizer_cache_root() -> PathBuf {
    std::env::temp_dir().join("distributed_diagnostics_tokenizers")
}

/// Wraps a Hugging Face tokenizer artifact. Returns raw stripped tokens; callers
/// are responsible for any domain-specific normalization (case, length, filtering).
#[derive(Debug)]
pub struct HfTokenizer {
    inner: Tokenizer,
}

impl HfTokenizer {
    /// Load a tokenizer by HuggingFace repo ID.
    ///
    /// Uses a local cache at `{temp_dir}/distributed_diagnostics_tokenizers/{source}/tokenizer.json`.
    /// If cached, loads from disk. Otherwise downloads from HuggingFace with up to
    /// 3 attempts (exponential backoff with jitter), validates, and caches.
    pub async fn load(source: &str) -> Result<Self, TokenizerError> {
        let url = hf_tokenizer_url(source);
        let inner = load_or_download_from_url(source, &tokenizer_cache_root(), &url).await?;
        Ok(Self { inner })
    }

    /// Tokenize `text` and return token strings with subword markers stripped
    /// (GPT-2 `Ġ`, SentencePiece `▁`, WordPiece `##`).
    pub fn tokenize(&self, text: &str) -> Vec<String> {
        let encoding = match self.inner.encode(text, false) {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };

        encoding
            .get_tokens()
            .iter()
            .map(|raw| strip_tokenizer_markers(raw).to_string())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// URL and cache path helpers
// ---------------------------------------------------------------------------

fn hf_tokenizer_url(repo_id: &str) -> String {
    format!("https://huggingface.co/{repo_id}/resolve/main/tokenizer.json")
}

fn cache_path(cache_root: &Path, source: &str) -> PathBuf {
    cache_root.join(source).join("tokenizer.json")
}

// ---------------------------------------------------------------------------
// Download and validation
// ---------------------------------------------------------------------------

fn validate_tokenizer_json(bytes: &[u8]) -> Result<(), TokenizerError> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| TokenizerError::InvalidJson {
            reason: e.to_string(),
        })?;
    if value.get("model").is_none() {
        return Err(TokenizerError::MissingModelField);
    }
    Ok(())
}

fn download_backoff() -> ExponentialBuilder {
    ExponentialBuilder::default()
        .with_factor(2.0)
        .with_min_delay(Duration::from_millis(100))
        .with_max_delay(Duration::from_secs(30))
        .with_max_times(DOWNLOAD_MAX_ATTEMPTS - 1)
        .with_jitter()
}

async fn fetch_bytes(url: &str, client: &reqwest::Client) -> Result<Vec<u8>, TokenizerError> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| TokenizerError::Download {
            url: url.to_string(),
            reason: e.to_string(),
        })?;

    if !response.status().is_success() {
        return Err(TokenizerError::Download {
            url: url.to_string(),
            reason: format!("HTTP {}", response.status()),
        });
    }

    response
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| TokenizerError::Download {
            url: url.to_string(),
            reason: e.to_string(),
        })
}

async fn load_or_download_from_url(
    source: &str,
    cache_root: &Path,
    url: &str,
) -> Result<Tokenizer, TokenizerError> {
    let cache = cache_path(cache_root, source);

    if cache.exists() {
        return Tokenizer::from_file(&cache).map_err(|e| TokenizerError::Load {
            path: cache.display().to_string(),
            reason: e.to_string(),
        });
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
        .build()
        .map_err(|e| TokenizerError::Download {
            url: url.to_string(),
            reason: format!("failed to build HTTP client: {e}"),
        })?;

    let url_owned = url.to_string();
    let bytes = {
        let client = client.clone();
        let url = url_owned.clone();
        (move || {
            let client = client.clone();
            let url = url.clone();
            async move { fetch_bytes(&url, &client).await }
        })
        .retry(download_backoff())
        .await?
    };

    validate_tokenizer_json(&bytes)?;

    if let Some(parent) = cache.parent() {
        std::fs::create_dir_all(parent).map_err(|e| TokenizerError::CacheWrite {
            path: cache.display().to_string(),
            reason: e.to_string(),
        })?;
    }

    std::fs::write(&cache, &bytes).map_err(|e| TokenizerError::CacheWrite {
        path: cache.display().to_string(),
        reason: e.to_string(),
    })?;

    Tokenizer::from_bytes(&bytes).map_err(|e| TokenizerError::Load {
        path: url_owned,
        reason: e.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Token normalization helpers
// ---------------------------------------------------------------------------

/// Strip leading/trailing marker characters emitted by common tokenizer variants
/// (GPT-2 Ġ prefix, WordPiece ## prefix, SentencePiece ▁ prefix).
fn strip_tokenizer_markers(token: &str) -> &str {
    let t = token
        .strip_prefix('Ġ')
        .or_else(|| token.strip_prefix('▁'))
        .unwrap_or(token);

    t.strip_prefix("##").unwrap_or(t)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{
        populate_tokenizer_cache, MockHttpServer, MockResponse, TempArtifactDir,
    };

    // ── strip_tokenizer_markers ──────────────────────────────────────────────

    #[test]
    fn strip_gpt2_prefix() {
        assert_eq!(strip_tokenizer_markers("Ġhello"), "hello");
    }

    #[test]
    fn strip_sentencepiece_prefix() {
        assert_eq!(strip_tokenizer_markers("▁world"), "world");
    }

    #[test]
    fn strip_wordpiece_prefix() {
        assert_eq!(strip_tokenizer_markers("##ing"), "ing");
    }

    #[test]
    fn plain_token_unchanged() {
        assert_eq!(strip_tokenizer_markers("foo"), "foo");
    }

    // ── HfTokenizer::load ────────────────────────────────────────────────────

    #[tokio::test]
    async fn load_tokenizes_when_cache_exists() {
        populate_tokenizer_cache("test/tok");
        let tokenizer = HfTokenizer::load("test/tok").await.unwrap();
        let tokens = tokenizer.tokenize("service down");
        assert_eq!(tokens, vec!["service".to_string(), "down".to_string()]);
    }

    // ── validate_tokenizer_json ──────────────────────────────────────────────

    #[test]
    fn validate_accepts_json_with_model_field() {
        let bytes = br#"{"model": {"type": "WordLevel"}, "version": "1.0"}"#;
        assert!(validate_tokenizer_json(bytes).is_ok());
    }

    #[test]
    fn validate_rejects_json_without_model_field() {
        let bytes = br#"{"version": "1.0"}"#;
        let err = validate_tokenizer_json(bytes).unwrap_err();
        assert!(matches!(err, TokenizerError::MissingModelField));
    }

    #[test]
    fn validate_rejects_non_json_bytes() {
        let err = validate_tokenizer_json(b"not json at all").unwrap_err();
        assert!(matches!(err, TokenizerError::InvalidJson { .. }));
    }

    // ── cache_path ───────────────────────────────────────────────────────────

    #[test]
    fn cache_path_uses_source_as_nested_subdir() {
        let base = Path::new("/tmp/tokenizers");
        let path = cache_path(base, "Qwen/Qwen3-Embedding-0.6B");
        assert_eq!(
            path,
            PathBuf::from("/tmp/tokenizers/Qwen/Qwen3-Embedding-0.6B/tokenizer.json")
        );
    }

    // ── load_or_download_from_url ────────────────────────────────────────────

    #[tokio::test]
    async fn load_reads_from_cache_when_file_exists() {
        let dir = TempArtifactDir::new();
        let source = "test/model";

        let cache = cache_path(dir.path(), source);
        std::fs::create_dir_all(cache.parent().unwrap()).unwrap();

        let artifact_path = dir.write_basic_tokenizer("tokenizer_src.json");
        std::fs::copy(&artifact_path, &cache).unwrap();

        let result =
            load_or_download_from_url(source, dir.path(), "http://must-not-be-called").await;
        assert!(result.is_ok(), "expected Ok, got {result:?}");
    }

    #[tokio::test]
    async fn load_downloads_validates_and_caches_when_no_cache() {
        let dir = TempArtifactDir::new();
        let source = "test/download-model";

        let artifact_bytes = {
            let tmp = TempArtifactDir::new();
            let p = tmp.write_basic_tokenizer("tok.json");
            std::fs::read(p).unwrap()
        };

        let server = MockHttpServer::new(vec![MockResponse::ok(artifact_bytes)]).await;
        let url = format!("{}/tokenizer.json", server.base_url());

        let result = load_or_download_from_url(source, dir.path(), &url).await;
        assert!(result.is_ok(), "expected Ok, got {result:?}");
        assert!(
            cache_path(dir.path(), source).exists(),
            "cache file should be written"
        );
    }

    #[tokio::test]
    async fn load_returns_error_on_http_failure() {
        let dir = TempArtifactDir::new();
        let source = "test/fail-model";

        let server = MockHttpServer::new(vec![MockResponse::status(
            500,
            b"Internal Server Error".to_vec(),
        )])
        .await;
        let url = format!("{}/tokenizer.json", server.base_url());

        let result = load_or_download_from_url(source, dir.path(), &url).await;
        assert!(
            matches!(result, Err(TokenizerError::Download { .. })),
            "expected Download error, got {result:?}"
        );
    }
}
