use crate::model::update::saga::UpdateLocationsState;
use crate::model::update::UpdateLocationsEvent;
use crate::model::{AggregateState, UpdateLocations};
use cqrs_es::persist::GenericQuery;
use cqrs_es::{EventEnvelope, View};
use postgres_es::PostgresViewRepository;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const UPDATE_LOCATIONS_QUERY_VIEW: &str = "update_locations_query";

pub type UpdateLocationsViewRepository =
    PostgresViewRepository<UpdateLocationsView, UpdateLocations>;
pub type UpdateLocationsViewProjection = Arc<UpdateLocationsViewRepository>;

pub type UpdateLocationsQuery =
    GenericQuery<UpdateLocationsViewRepository, UpdateLocationsView, UpdateLocations>;

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLocationsView {
    pub state: UpdateLocationsState,
    pub history: Vec<UpdateLocationsEvent>,
}

impl View<UpdateLocations> for UpdateLocationsView {
    fn update(&mut self, event: &EventEnvelope<UpdateLocations>) {
        let evt = event.payload.clone();
        self.history.push(evt.clone());
        if let Some(new_state) = self.state.apply(evt) {
            self.state = new_state;
        }
    }
}
