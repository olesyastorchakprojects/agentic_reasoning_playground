use thiserror::Error;

pub use crate::api_clients::ApiClientError;
pub use crate::config::ConfigError;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("config: {0}")]
    Config(#[from] ConfigError),
    #[error("api clients: {0}")]
    ApiClients(#[from] ApiClientError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_clients::EmbeddingClientError;

    #[test]
    fn config_error_converts_to_runtime_error_config_variant() {
        let config_err = ConfigError::Load("test load failure".into());
        let runtime_err = RuntimeError::from(config_err);
        assert!(
            matches!(runtime_err, RuntimeError::Config(_)),
            "expected RuntimeError::Config, got: {runtime_err}"
        );
    }

    #[test]
    fn api_client_error_converts_to_runtime_error_api_clients_variant() {
        let leaf = EmbeddingClientError::InvalidRequest("bad input".into());
        let api_err = ApiClientError::Embedding(leaf);
        let runtime_err = RuntimeError::from(api_err);
        assert!(
            matches!(runtime_err, RuntimeError::ApiClients(_)),
            "expected RuntimeError::ApiClients, got: {runtime_err}"
        );
    }

    #[test]
    fn runtime_error_preserves_typed_child_config_error() {
        let config_err = ConfigError::MissingEnvironment {
            key: "TEST_VAR".into(),
        };
        let runtime_err = RuntimeError::Config(config_err);
        if let RuntimeError::Config(inner) = runtime_err {
            assert!(matches!(inner, ConfigError::MissingEnvironment { .. }));
        } else {
            panic!("expected Config variant");
        }
    }

    #[test]
    fn runtime_error_preserves_typed_child_api_client_error() {
        let leaf = EmbeddingClientError::Transport("connection refused".into());
        let api_err = ApiClientError::Embedding(leaf);
        let runtime_err = RuntimeError::ApiClients(api_err);
        if let RuntimeError::ApiClients(inner) = runtime_err {
            assert!(matches!(inner, ApiClientError::Embedding(_)));
        } else {
            panic!("expected ApiClients variant");
        }
    }
}
