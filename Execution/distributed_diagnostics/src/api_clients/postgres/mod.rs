pub mod incident_card_store;

use thiserror::Error;

pub use crate::shared_types::{
    DiscriminatingCheck, ExpectedObservation, IncidentCard, IncidentPhase,
};
pub use incident_card_store::{
    IncidentCardStoreError, PostgresIncidentCardStore, PostgresIncidentCardStoreConfig,
};

#[derive(Debug, Error)]
pub enum PostgresApiClientError {
    #[error("incident card store: {0}")]
    IncidentCardStore(#[from] IncidentCardStoreError),
}
