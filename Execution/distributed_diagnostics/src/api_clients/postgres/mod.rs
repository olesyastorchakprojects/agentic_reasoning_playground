pub mod incident_card_store;

use thiserror::Error;

pub use incident_card_store::{
    DiscriminatingCheck, ExpectedObservation, IncidentCard, IncidentCardStoreError,
    IncidentPhase, PostgresIncidentCardStore,
};

#[derive(Debug, Error)]
pub enum PostgresApiClientError {
    #[error("incident card store: {0}")]
    IncidentCardStore(#[from] IncidentCardStoreError),
}
