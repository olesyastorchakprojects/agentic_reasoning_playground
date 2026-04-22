use std::sync::Mutex;

use crate::api_clients::postgres::incident_card_store::IncidentCardStoreError;
use crate::shared_types::IncidentCard;

#[derive(Debug)]
pub struct MockPostgresIncidentCardStore {
    responses: Mutex<Vec<Result<Vec<IncidentCard>, IncidentCardStoreError>>>,
    pub captured_case_ids: Mutex<Vec<Vec<String>>>,
}

impl MockPostgresIncidentCardStore {
    pub fn new(responses: Vec<Result<Vec<IncidentCard>, IncidentCardStoreError>>) -> Self {
        Self {
            responses: Mutex::new(responses),
            captured_case_ids: Mutex::new(vec![]),
        }
    }

    pub async fn get_cards_by_case_ids(
        &self,
        case_ids: &[String],
    ) -> Result<Vec<IncidentCard>, IncidentCardStoreError> {
        self.captured_case_ids
            .lock()
            .unwrap()
            .push(case_ids.to_vec());
        self.responses.lock().unwrap().remove(0)
    }
}
