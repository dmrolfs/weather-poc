use super::UpdateLocations;
use crate::model::update::UpdateLocationsEvent as E;
use crate::model::zone::LocationZoneCommand;
use crate::model::{LocationZone, LocationZoneAggregate, LocationZoneIdentifier, UpdateLocationsSaga, ZoneUpdateStatus};
use async_trait::async_trait;
use cqrs_es::{EventEnvelope, Query};
use std::collections::HashMap;
use std::fmt;
use futures_util::FutureExt;

type LocationZoneError = <LocationZone as cqrs_es::Aggregate>::Error;

pub struct UpdateLocationZoneController {
    update_saga: UpdateLocationsSaga,
    zone_cqrs: LocationZoneAggregate,
}

impl fmt::Debug for UpdateLocationZoneController {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpdateLocationZoneController").finish()
    }
}

#[async_trait]
impl Query<UpdateLocations> for UpdateLocationZoneController {
    async fn dispatch(&self, update_saga_id: &str, events: &[EventEnvelope<UpdateLocations>]) {
        let metadata =
            maplit::hashmap! { "correlation".to_string() => update_saga_id.to_string(), };

        let mut tasks = vec![];

        for event in events {
            if let E::Started(zones) = &event.payload {
                let event_tasks: Vec<_> = zones
                    .iter()
                    .map(|z| {
                        let task = self.start_zone_update(z, metadata.clone());
                        task.map(|t| (z.clone(), t))
                    })
                    .collect();

                tasks.extend(event_tasks);
            }
        }

        let zone_statuses: Vec<(LocationZoneIdentifier, ZoneUpdateStatus)> = futures::future::join_all(tasks).await;
        let zone_failures: Vec<_> = zone_statuses
            .into_iter()
            .filter(|(_, status)| status == &ZoneUpdateStatus::Failed)
            .collect();

        for (zone, failure) in zone_failures {
            self.update_saga.execute_with_metadata(zone.code.as_str(), )
        }
        zone_failures
            .into_iter()
            .
        //WORK HERE saga send zone update with failure
        todo!()
    }
}

impl UpdateLocationZoneController {
    async fn start_zone_update(
        &self, zone: &LocationZoneIdentifier, metadata: HashMap<String, String>,
    ) -> ZoneUpdateStatus {
        self.0
            .execute_with_metadata(zone.code.as_str(), LocationZoneCommand::Observe, metadata)
            .await
            .map(|()| ZoneUpdateStatus::Started)
            .unwrap_or(ZoneUpdateStatus::Failed)
    }
}
