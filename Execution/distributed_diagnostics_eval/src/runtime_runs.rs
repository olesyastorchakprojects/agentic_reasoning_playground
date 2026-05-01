use distributed_diagnostics::api_clients::postgres::{
    PostgresRunStateStore, PostgresRunStateStoreConfig, RunStateStoreError,
};
use distributed_diagnostics::orchestrator::run_state::model::{RunId, RunState};
use uuid::Uuid;

use crate::config::PostgresSettings;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeRunLoaderError {
    #[error(transparent)]
    Store(#[from] RunStateStoreError),
    #[error("runtime run not found: {0}")]
    RunNotFound(Uuid),
}

#[derive(Debug)]
pub struct PostgresRuntimeRunLoader {
    store: PostgresRunStateStore,
}

impl PostgresRuntimeRunLoader {
    pub async fn new(
        config: &PostgresSettings,
    ) -> Result<Self, RuntimeRunLoaderError> {
        let store = PostgresRunStateStore::new(PostgresRunStateStoreConfig {
            postgres_url: config.url.clone(),
        })
        .await?;
        Ok(Self { store })
    }

    pub async fn load_run_state(
        &self,
        runtime_run_id: Uuid,
    ) -> Result<RunState, RuntimeRunLoaderError> {
        self.store
            .load_run(RunId(runtime_run_id))
            .await?
            .ok_or(RuntimeRunLoaderError::RunNotFound(runtime_run_id))
    }
}

#[cfg(test)]
mod tests {
    use crate::config::PostgresSettings;
    use crate::runtime_runs::{PostgresRuntimeRunLoader, RuntimeRunLoaderError};

    #[tokio::test]
    async fn new_fails_when_postgres_url_is_empty() {
        let err = PostgresRuntimeRunLoader::new(&PostgresSettings {
            url: " ".to_string(),
        })
        .await
        .unwrap_err();
        assert!(matches!(err, RuntimeRunLoaderError::Store(_)));
    }
}
