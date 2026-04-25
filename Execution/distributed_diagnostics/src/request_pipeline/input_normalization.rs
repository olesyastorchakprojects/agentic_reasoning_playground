use crate::config::InputNormalizationSettings;
use crate::shared_types::{NormalizedUserRequest, UserRequest};
use crate::utils::tokenizer::{HfTokenizer, TokenizerError};
use tracing::{info_span, field};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, thiserror::Error)]
pub enum InputNormalizationError {
    #[error("query is empty after normalization")]
    EmptyQuery,
    #[error("input too long: {token_count} tokens exceeds limit of {max_input_tokens}")]
    InputTooLong {
        token_count: usize,
        max_input_tokens: usize,
    },
    #[error(transparent)]
    Tokenizer(#[from] TokenizerError),
}

#[derive(Debug)]
pub struct InputNormalization {
    tokenizer: HfTokenizer,
    max_input_tokens: usize,
}

impl InputNormalization {
    pub async fn new(
        settings: InputNormalizationSettings,
    ) -> Result<Self, InputNormalizationError> {
        let tokenizer = HfTokenizer::load(&settings.tokenizer_source).await?;
        Ok(Self {
            tokenizer,
            max_input_tokens: settings.max_input_tokens,
        })
    }

    pub fn normalize(
        &self,
        request: UserRequest,
    ) -> Result<NormalizedUserRequest, InputNormalizationError> {
        let raw_query = request.query.clone();
        let raw_chars = raw_query.len();

        let span = info_span!(
            "request_pipeline.input_normalization",
            module.name = "input_normalization",
            input.raw_query = %raw_query,
            input.raw_chars = raw_chars,
            input.normalized_query = field::Empty,
            input.normalized_chars = field::Empty,
            input.normalized_token_count = field::Empty,
            input.max_tokens = self.max_input_tokens,
            input.within_limit = field::Empty,
            normalization.trimmed = field::Empty,
            normalization.collapsed_whitespace = field::Empty,
            normalization.changed = field::Empty,
            module.outcome = field::Empty,
            status = field::Empty,
            error.type = field::Empty,
            error.message = field::Empty,
        );

        let _enter = span.enter();

        tracing::info!(raw_query = %raw_query, "input_normalization.input_received");

        // Detect if input had leading/trailing whitespace
        let trimmed = raw_query != raw_query.trim();

        // Normalize: trim and collapse whitespace
        let query: String = raw_query
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        // Any difference in internal whitespace canonicalization counts as collapsed whitespace.
        let collapsed_whitespace = raw_query.trim() != query && raw_query.split_whitespace().count() > 1;

        let changed = raw_query != query;

        span.record("normalization.trimmed", trimmed);
        span.record("normalization.collapsed_whitespace", collapsed_whitespace);
        span.record("normalization.changed", changed);

        tracing::info!(normalized_query = %query, "input_normalization.normalized");

        if query.is_empty() {
            span.record("module.outcome", "failure");
            span.record("status", "error");
            span.record("error.type", "InputNormalization.EmptyQuery");
            span.record("error.message", "Query is empty after normalization");
            return Err(InputNormalizationError::EmptyQuery);
        }

        let tokens = match self.tokenizer.tokenize(&query) {
            Ok(tokens) => tokens,
            Err(error) => {
                span.record("module.outcome", "failure");
                span.record("status", "error");
                span.record("error.type", "InputNormalization.Tokenizer");
                span.record("error.message", error.to_string());
                return Err(InputNormalizationError::Tokenizer(error));
            }
        };
        let input_token_count = tokens.len();

        tracing::info!(token_count = input_token_count, "input_normalization.token_counted");

        if input_token_count == 0 {
            span.record("module.outcome", "failure");
            span.record("status", "error");
            span.record("error.type", "InputNormalization.EmptyQuery");
            span.record("error.message", "Tokenizer produced zero tokens");
            return Err(InputNormalizationError::EmptyQuery);
        }

        let within_limit = input_token_count <= self.max_input_tokens;

        if !within_limit {
            span.record("input.normalized_query", &query);
            span.record("input.normalized_chars", query.len());
            span.record("input.normalized_token_count", input_token_count);
            span.record("input.within_limit", false);
            span.record("module.outcome", "failure");
            span.record("status", "error");
            span.record("error.type", "InputNormalization.InputTooLong");
            span.record(
                "error.message",
                format!(
                    "Input too long: {} tokens exceeds limit of {}",
                    input_token_count, self.max_input_tokens
                ),
            );
            return Err(InputNormalizationError::InputTooLong {
                token_count: input_token_count,
                max_input_tokens: self.max_input_tokens,
            });
        }

        // Success path
        span.record("input.normalized_query", &query);
        span.record("input.normalized_chars", query.len());
        span.record("input.normalized_token_count", input_token_count);
        span.record("input.within_limit", true);
        span.record("module.outcome", "success");
        span.record("status", "ok");

        Ok(NormalizedUserRequest {
            query,
            input_token_count,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::populate_tokenizer_cache;
    use crate::utils::tokenizer::tokenizer_cache_root;
    use tokenizers::models::wordlevel::WordLevel;
    use tokenizers::pre_tokenizers::whitespace::Whitespace;
    use tokenizers::Tokenizer;

    const TEST_SOURCE: &str = "test/input-norm";

    fn make_normalization(max_input_tokens: usize) -> InputNormalization {
        populate_tokenizer_cache(TEST_SOURCE);
        let tokenizer = {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(HfTokenizer::load(TEST_SOURCE)).unwrap()
        };
        InputNormalization {
            tokenizer,
            max_input_tokens,
        }
    }

    fn req(query: &str) -> UserRequest {
        UserRequest {
            query: query.to_string(),
        }
    }

    fn populate_zero_token_cache(source: &str) {
        let cache = tokenizer_cache_root().join(source).join("tokenizer.json");
        std::fs::create_dir_all(cache.parent().unwrap()).expect("create tokenizer cache dir");

        // No unknown token is configured, so an out-of-vocabulary term causes encode() to fail.
        let model = WordLevel::builder()
            .vocab([("service".to_string(), 1u32)].into_iter().collect())
            .build()
            .expect("build wordlevel tokenizer");
        let mut tokenizer = Tokenizer::new(model);
        tokenizer.with_pre_tokenizer(Some(Whitespace));
        tokenizer
            .save(&cache, false)
            .expect("save tokenizer to cache");
    }

    // ── constructor ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn constructor_succeeds_when_tokenizer_loads() {
        populate_tokenizer_cache(TEST_SOURCE);
        let settings = InputNormalizationSettings {
            max_input_tokens: 100,
            tokenizer_source: TEST_SOURCE.to_string(),
        };
        let result = InputNormalization::new(settings).await;
        assert!(result.is_ok(), "expected Ok, got {result:?}");
    }

    // ── whitespace normalization ─────────────────────────────────────────────

    #[test]
    fn leading_and_trailing_whitespace_are_trimmed() {
        let norm = make_normalization(100);
        let result = norm.normalize(req("  service down  ")).unwrap();
        assert_eq!(result.query, "service down");
    }

    #[test]
    fn newlines_are_flattened() {
        let norm = make_normalization(100);
        let result = norm.normalize(req("service\ndown")).unwrap();
        assert_eq!(result.query, "service down");
    }

    #[test]
    fn tabs_and_mixed_whitespace_are_canonicalized() {
        let norm = make_normalization(100);
        let result = norm.normalize(req("service\t \t down")).unwrap();
        assert_eq!(result.query, "service down");
    }

    // ── empty query errors ───────────────────────────────────────────────────

    #[test]
    fn empty_query_after_normalization_fails_with_empty_query() {
        let norm = make_normalization(100);
        let result = norm.normalize(req("   "));
        assert!(
            matches!(result, Err(InputNormalizationError::EmptyQuery)),
            "expected EmptyQuery, got {result:?}"
        );
    }

    #[test]
    fn tokenizer_failure_is_reported_as_tokenizer_error() {
        const ZERO_TOKEN_SOURCE: &str = "test/input-norm-zero-token";
        populate_zero_token_cache(ZERO_TOKEN_SOURCE);
        let tokenizer = {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(HfTokenizer::load(ZERO_TOKEN_SOURCE)).unwrap()
        };
        let norm = InputNormalization {
            tokenizer,
            max_input_tokens: 100,
        };
        let result = norm.normalize(req("unknownterm"));
        assert!(
            matches!(result, Err(InputNormalizationError::Tokenizer(_))),
            "expected Tokenizer error, got {result:?}"
        );
    }

    // ── token ceiling ────────────────────────────────────────────────────────

    #[test]
    fn query_at_exactly_max_tokens_succeeds() {
        // "service down" → 2 tokens with the mock tokenizer
        let norm = make_normalization(2);
        let result = norm.normalize(req("service down"));
        assert!(result.is_ok(), "expected Ok, got {result:?}");
    }

    #[test]
    fn query_exceeding_max_tokens_fails_with_input_too_long() {
        // "service down" → 2 tokens; limit is 1
        let norm = make_normalization(1);
        let result = norm.normalize(req("service down"));
        assert!(
            matches!(
                result,
                Err(InputNormalizationError::InputTooLong {
                    token_count: 2,
                    max_input_tokens: 1
                })
            ),
            "expected InputTooLong, got {result:?}"
        );
    }

    // ── successful output fields ─────────────────────────────────────────────

    #[test]
    fn successful_normalization_returns_canonical_query() {
        let norm = make_normalization(100);
        let result = norm.normalize(req("  service   down  ")).unwrap();
        assert_eq!(result.query, "service down");
    }

    #[test]
    fn successful_normalization_returns_correct_token_count() {
        let norm = make_normalization(100);
        // "service down" → 2 tokens
        let result = norm.normalize(req("service down")).unwrap();
        assert_eq!(result.input_token_count, 2);
    }
}
