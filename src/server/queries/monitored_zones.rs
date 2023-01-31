use crate::model::registrar::RegistrarEvent;
use crate::model::{LocationZoneCode, Registrar};
use cqrs_es::persist::GenericQuery;
use cqrs_es::{EventEnvelope, View};
use postgres_es::PostgresViewRepository;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use utoipa::ToSchema;

pub type MonitoredZonesRepository = PostgresViewRepository<MonitoredZonesView, Registrar>;
pub type MonitoredZonesViewProjection = Arc<MonitoredZonesRepository>;

pub type MonitoredZonesQuery =
    GenericQuery<MonitoredZonesRepository, MonitoredZonesView, Registrar>;

#[derive(Debug, Default, Clone, PartialEq, ToSchema, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitoredZonesView {
    pub zones: HashSet<LocationZoneCode>,
}

impl View<Registrar> for MonitoredZonesView {
    fn update(&mut self, event: &EventEnvelope<Registrar>) {
        use RegistrarEvent as Evt;

        match &event.payload {
            Evt::ForecastZoneAdded(zone) => {
                self.zones.insert(zone.clone());
            },
            Evt::ForecastZoneForgotten(zone) => {
                self.zones.remove(zone);
            },
            Evt::AllForecastZonesForgotten => {
                self.zones.clear();
            },
        }
    }
}
