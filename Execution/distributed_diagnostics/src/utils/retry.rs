use std::time::Duration;

use backon::ExponentialBuilder;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetryBackoffKind {
    Exponential,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicyConfig {
    pub max_attempts: u32,
    pub backoff: RetryBackoffKind,
}

/// Returns a configured `ExponentialBuilder` for use with `.retry()` from `backon`.
/// Callers determine which errors are retryable via `.when(predicate)`.
pub fn build_backoff(policy: &RetryPolicyConfig) -> ExponentialBuilder {
    let max_retries = policy.max_attempts.saturating_sub(1) as usize;

    ExponentialBuilder::default()
        .with_factor(2.0)
        .with_min_delay(Duration::from_millis(100))
        .with_max_delay(Duration::from_secs(30))
        .with_max_times(max_retries)
        .with_jitter()
}
