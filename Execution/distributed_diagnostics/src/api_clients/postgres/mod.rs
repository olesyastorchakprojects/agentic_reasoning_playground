pub mod incident_card_store;

use thiserror::Error;

pub use incident_card_store::{IncidentCardStoreError, PostgresIncidentCardStore, PostgresIncidentCardStoreConfig};
pub use crate::shared_types::{DiscriminatingCheck, ExpectedObservation, IncidentCard, IncidentPhase};

#[derive(Debug, Error)]
pub enum PostgresApiClientError {
    #[error("incident card store: {0}")]
    IncidentCardStore(#[from] IncidentCardStoreError),
}
