use tokenizers::Tokenizer;

#[derive(Debug, thiserror::Error)]
pub enum TokenizerError {
    #[error("failed to load tokenizer from {path}: {reason}")]
    Load { path: String, reason: String },
}

/// Wraps a Hugging Face tokenizer artifact with the sparse text-space
/// normalization rules from `sparse_text_space.md`.
#[derive(Debug)]
pub struct SparseTokenizer {
    inner: Tokenizer,
    lowercase: bool,
    min_token_length: usize,
}

impl SparseTokenizer {
    /// Load a tokenizer from a JSON artifact file (HuggingFace `tokenizers` format).
    pub fn from_file(
        path: &str,
        lowercase: bool,
        min_token_length: usize,
    ) -> Result<Self, TokenizerError> {
        let inner = Tokenizer::from_file(path).map_err(|e| TokenizerError::Load {
            path: path.to_string(),
            reason: e.to_string(),
        })?;
        Ok(Self { inner, lowercase, min_token_length })
    }

    /// Tokenize `text` and return canonical normalized token strings in
    /// left-to-right order (as required by `sparse_text_space.md`).
    pub fn tokenize(&self, text: &str) -> Vec<String> {
        let encoding = match self.inner.encode(text, false) {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };

        encoding
            .get_tokens()
            .iter()
            .filter_map(|raw| self.normalize(raw))
            .collect()
    }

    fn normalize(&self, raw: &str) -> Option<String> {
        let stripped = strip_tokenizer_markers(raw);

        let token = if self.lowercase {
            stripped.to_lowercase()
        } else {
            stripped.to_string()
        };

        if token.len() < self.min_token_length {
            return None;
        }

        if !token.chars().any(|c| c.is_alphanumeric()) {
            return None;
        }

        if is_unknown_placeholder(&token) {
            return None;
        }

        Some(token)
    }
}

/// Strip leading/trailing marker characters emitted by common tokenizer variants
/// (GPT-2 Ġ prefix, WordPiece ## prefix, SentencePiece ▁ prefix).
fn strip_tokenizer_markers(token: &str) -> &str {
    let t = token
        .strip_prefix('Ġ')
        .or_else(|| token.strip_prefix('▁'))
        .unwrap_or(token);

    t.strip_prefix("##").unwrap_or(t)
}

fn is_unknown_placeholder(token: &str) -> bool {
    matches!(token, "[unk]" | "unk")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::TempArtifactDir;

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

    #[test]
    fn unknown_placeholder_is_filtered() {
        assert!(is_unknown_placeholder("[unk]"));
        assert!(is_unknown_placeholder("unk"));
        assert!(!is_unknown_placeholder("service"));
    }

    #[test]
    fn from_file_loads_hf_tokenizer_artifact() {
        let dir = TempArtifactDir::new();
        let tokenizer_path = dir.write_basic_tokenizer("tokenizer.json");

        let tokenizer =
            SparseTokenizer::from_file(tokenizer_path.to_str().unwrap(), true, 2).unwrap();

        let tokens = tokenizer.tokenize("service down");
        assert_eq!(tokens, vec!["service".to_string(), "down".to_string()]);
    }

    #[test]
    fn tokenize_applies_lowercase_and_filters_short_and_non_alnum_tokens() {
        let dir = TempArtifactDir::new();
        let tokenizer_path = dir.write_basic_tokenizer("tokenizer.json");

        let tokenizer =
            SparseTokenizer::from_file(tokenizer_path.to_str().unwrap(), true, 2).unwrap();

        let tokens = tokenizer.tokenize("service ! a down");
        assert_eq!(tokens, vec!["service".to_string(), "down".to_string()]);
    }

    #[test]
    fn from_file_invalid_path_returns_error() {
        let err = SparseTokenizer::from_file("/nonexistent/tokenizer.json", true, 2).unwrap_err();
        match err {
            TokenizerError::Load { path, .. } => assert!(path.contains("tokenizer.json")),
        }
    }
}
