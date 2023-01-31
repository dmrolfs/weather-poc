use super::UpdateLocations;
use crate::model::update::{UpdateLocationsCommand, UpdateLocationsEvent as E};
use crate::model::zone::LocationZoneCommand;
use crate::model::{LocationZone, LocationZoneCode, WeatherAlert};
use crate::queries;
use crate::services::noaa::{AlertApi, NoaaWeatherServices};
use async_trait::async_trait;
use cqrs_es::Query;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use tokio::{sync::mpsc, task};

pub struct UpdateLocationZoneController {
    inner: Arc<UpdateLocationZoneControllerRef>,
}

impl UpdateLocationZoneController {
    pub fn new(
        noaa: NoaaWeatherServices,
        location_tx: mpsc::Sender<queries::CommandEnvelope<LocationZone>>,
        update_tx: mpsc::Sender<queries::CommandEnvelope<UpdateLocations>>,
    ) -> Self {
        Self {
            inner: Arc::new(UpdateLocationZoneControllerRef { noaa, location_tx, update_tx }),
        }
    }
}

#[derive(Debug)]
struct UpdateLocationZoneControllerRef {
    pub noaa: NoaaWeatherServices,
    pub location_tx: mpsc::Sender<queries::CommandEnvelope<LocationZone>>,
    pub update_tx: mpsc::Sender<queries::CommandEnvelope<UpdateLocations>>,
}

impl fmt::Debug for UpdateLocationZoneController {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpdateLocationZoneController").finish()
    }
}

#[async_trait]
impl Query<UpdateLocations> for UpdateLocationZoneController {
    async fn dispatch(
        &self, update_saga_id: &str, events: &[cqrs_es::EventEnvelope<UpdateLocations>],
    ) {
        let metadata =
            maplit::hashmap! { "correlation".to_string() => update_saga_id.to_string(), };

        for event in events {
            if let E::Started(_, zones) = &event.payload {
                let saga_id = update_saga_id.to_string();
                let zones = zones.clone();
                let metadata = metadata.clone();

                self.inner.clone().do_spawn_update_observations(
                    saga_id.as_str(),
                    zones.as_slice(),
                    &metadata,
                );
                self.inner.clone().do_spawn_update_forecasts(
                    saga_id.as_str(),
                    zones.as_slice(),
                    &metadata,
                );

                let inner_ref = self.inner.clone();
                tokio::spawn(async move {
                    inner_ref.do_spawn_update_alerts(saga_id.as_str(), zones.as_slice(), &metadata);
                });
            }
        }
    }
}

impl UpdateLocationZoneControllerRef {
    #[tracing::instrument(level = "trace", skip())]
    fn do_spawn_update_observations(
        self: Arc<Self>, update_saga_id: &str, zones: &[LocationZoneCode],
        metadata: &HashMap<String, String>,
    ) {
        for z in zones.iter().cloned() {
            let self_ref = self.clone();
            let saga_id = update_saga_id.to_string();
            let metadata = metadata.clone();
            task::spawn(async move {
                tracing::debug!("spawning observation update on {z} zone..");
                self_ref.do_update_zone_observation(&saga_id, &z, metadata).await;
            });
        }
    }

    #[tracing::instrument(level = "trace", skip())]
    fn do_spawn_update_forecasts(
        self: Arc<Self>, update_saga_id: &str, zones: &[LocationZoneCode],
        metadata: &HashMap<String, String>,
    ) {
        for z in zones.iter().cloned() {
            let self_ref = self.clone();
            let saga_id = update_saga_id.to_string();
            let metadata = metadata.clone();
            task::spawn(async move {
                tracing::debug!("spawning forecast update on {z} zone..");
                self_ref.do_update_zone_forecast(&saga_id, &z, metadata).await;
            });
        }
    }

    #[tracing::instrument(level = "trace", skip())]
    async fn do_spawn_update_alerts(
        self: Arc<Self>, update_saga_id: &str, zones: &[LocationZoneCode],
        metadata: &HashMap<String, String>,
    ) {
        let update_zones: HashSet<_> = zones.iter().collect();

        for alert in self.do_get_alerts().await {
            let update_affected = alert.affected_zones.iter().filter(|z| update_zones.contains(z));

            for affected in update_affected.cloned() {
                let self_ref = self.clone();
                let saga_id = update_saga_id.to_string();
                let alert = alert.clone();
                let metadata = metadata.clone();
                task::spawn(async move {
                    tracing::debug!(?alert, "spawning alert update on {affected} zone..");
                    self_ref.do_update_zone_alert(&saga_id, affected, alert, metadata).await;
                });
            }
        }
    }

    #[tracing::instrument(level = "trace", skip())]
    async fn do_update_zone_observation(
        &self, update_saga_id: &str, zone: &LocationZoneCode, metadata: HashMap<String, String>,
    ) {
        let command = queries::CommandEnvelope::new_with_metadata(
            zone.to_string(),
            LocationZoneCommand::Observe,
            metadata,
        );

        self.do_send_command(update_saga_id, command).await;
    }

    #[tracing::instrument(level = "trace", skip())]
    async fn do_update_zone_forecast(
        &self, update_saga_id: &str, zone: &LocationZoneCode, metadata: HashMap<String, String>,
    ) {
        let command = queries::CommandEnvelope::new_with_metadata(
            zone.to_string(),
            LocationZoneCommand::Forecast,
            metadata,
        );

        self.do_send_command(update_saga_id, command).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn do_get_alerts(&self) -> Vec<WeatherAlert> {
        match self.noaa.active_alerts().await {
            Ok(alerts) => alerts,
            Err(error) => {
                tracing::error!(?error, "failed to pull weather alerts from NOAA.");
                vec![]
            },
        }
    }

    #[tracing::instrument(level = "trace", skip())]
    async fn do_update_zone_alert(
        &self, update_saga_id: &str, zone: LocationZoneCode, alert: WeatherAlert,
        metadata: HashMap<String, String>,
    ) {
        let command = queries::CommandEnvelope::new_with_metadata(
            zone,
            LocationZoneCommand::NoteAlert(Some(alert)),
            metadata,
        );

        self.do_send_command(update_saga_id, command).await
    }

    #[tracing::instrument(level = "trace", skip())]
    async fn do_send_command(
        &self, update_saga_id: &str, command: queries::CommandEnvelope<LocationZone>,
    ) {
        let zone = LocationZoneCode::new(command.target_id());
        let metadata = command.metadata().clone();
        let send_outcome = self.location_tx.send(command.clone()).await;
        tracing::debug!(
            ?send_outcome,
            ?command,
            "sending command to location aggregate channel"
        );
        if send_outcome.is_err() {
            let command = queries::CommandEnvelope::new_with_metadata(
                update_saga_id,
                UpdateLocationsCommand::NoteLocationUpdateFailure(zone.clone()),
                metadata,
            );

            let note_outcome = self.update_tx.send(command.clone()).await;
            tracing::error!(
                ?note_outcome,
                ?command,
                "sending failure note command to update saga channel"
            );
            if let Err(error) = note_outcome {
                tracing::error!(
                    ?error,
                    "failed to update saga on zone command failure: {zone}"
                );
            }
        }
    }
}
