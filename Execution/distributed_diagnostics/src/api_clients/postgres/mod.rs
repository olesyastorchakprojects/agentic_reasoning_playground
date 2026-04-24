pub mod incident_card_store;
pub mod run_state_store;

use thiserror::Error;

pub use crate::shared_types::{
    DiscriminatingCheck, ExpectedObservation, IncidentCard, IncidentPhase,
};
pub use incident_card_store::{
    IncidentCardStoreError, PostgresIncidentCardStore, PostgresIncidentCardStoreConfig,
};
pub use run_state_store::{
    PostgresRunStateStore, PostgresRunStateStoreConfig, PostgresRunStateStoreTx,
    RunStateStoreError,
};

#[derive(Debug, Error)]
pub enum PostgresApiClientError {
    #[error("incident card store: {0}")]
    IncidentCardStore(#[from] IncidentCardStoreError),
    #[error("run state store: {0}")]
    RunStateStore(#[from] RunStateStoreError),
}
