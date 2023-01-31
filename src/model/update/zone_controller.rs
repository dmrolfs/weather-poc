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

        let mut handles = vec![];

        for event in events {
            if let E::Started(_, zones) = &event.payload {
                let inner_ref = self.inner.clone();

                let update_zones: HashSet<_> = zones.iter().collect();

                let observation_tasks: Vec<_> = zones
                    .iter()
                    .cloned()
                    .map(|z| {
                        let inner = inner_ref.clone();
                        let update_saga_id = update_saga_id.to_string();
                        let metadata = metadata.clone();
                        task::spawn(async move {
                            inner
                                .do_update_zone_observation(update_saga_id.as_str(), &z, metadata)
                                .await
                        })
                    })
                    .collect();

                let forecast_tasks: Vec<_> = zones
                    .iter()
                    .cloned()
                    .map(|z| {
                        let inner = inner_ref.clone();
                        let update_saga_id = update_saga_id.to_string();
                        let metadata = metadata.clone();
                        task::spawn(async move {
                            inner
                                .do_update_zone_forecast(update_saga_id.as_str(), &z, metadata)
                                .await
                        })
                    })
                    .collect();

                let mut alert_tasks = vec![];
                let alerts = inner_ref.do_get_alerts().await;
                for alert in alerts {
                    for affected_zone in alert.affected_zones.clone() {
                        if update_zones.contains(&affected_zone) {
                            let inner = inner_ref.clone();
                            let update_saga_id = update_saga_id.to_string();
                            let metadata = metadata.clone();
                            let alert = alert.clone();
                            let update_task = task::spawn(async move {
                                inner
                                    .do_update_zone_alert(
                                        update_saga_id.as_str(),
                                        affected_zone,
                                        alert,
                                        metadata,
                                    )
                                    .await
                            });
                            alert_tasks.push(update_task);
                        }
                    }
                }

                handles.extend(observation_tasks);
                handles.extend(forecast_tasks);
                handles.extend(alert_tasks);
            }
        }

        futures::future::join_all(handles).await;
    }
}

impl UpdateLocationZoneControllerRef {
    #[tracing::instrument(level="trace", skip())]
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

    #[tracing::instrument(level="trace", skip())]
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

    // async fn do_update_alerts(&self, update_saga_id: &str, metadata: HashMap<String, String>,) -> Vec<impl Future<Output = ()>> {
    //     let alerts = match self.noaa.active_alerts().await {
    //         Ok(alerts) => alerts,
    //         Err(error) => {
    //             tracing::error!(?error, "failed to pull weather alerts from NOAA.");
    //             vec![]
    //         },
    //     };
    //
    //     let mut tasks = vec![];
    //
    //     for alert in alerts {
    //         for affected_zone in alert.affected_zones.clone() {
    //             if self.zones.contains(&affected_zone) {
    //                 let task = self.do_update_zone_alert(update_saga_id, affected_zone, alert.clone(), metadata.clone());
    //                 tasks.push(task);
    //             }
    //         }
    //     }
    //
    //     tasks
    //
    //     // alerts
    //     //     .into_iter()
    //     //     .flat_map(|alert| {
    //     //         let alert_0 = alert.clone();
    //     //         alert.affected_zones
    //     //             .into_iter()
    //     //             .filter(|affected_zone| self.zones.contains(affected_zone))
    //     //             .map(|affected_zone| {
    //     //                 self.do_update_zone_alert(update_saga_id, affected_zone, alert_0.clone(), metadata.clone())
    //     //             })
    //     //             .collect::<Vec<_>>()
    //     //     })
    //     //     .collect()
    // }

    #[tracing::instrument(level="trace", skip())]
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

    #[tracing::instrument(level="trace", skip())]
    async fn do_send_command(
        &self, update_saga_id: &str, command: queries::CommandEnvelope<LocationZone>,
    ) {
        let zone = LocationZoneCode::new(command.target_id());
        let metadata = command.metadata().clone();
        let send_outcome = self.location_tx.send(command.clone()).await;
        tracing::debug!(?send_outcome, ?command, "sending command to location aggregate channel");
        if send_outcome.is_err() {
            let command = queries::CommandEnvelope::new_with_metadata(
                update_saga_id,
                UpdateLocationsCommand::NoteLocationUpdateFailure(zone.clone()),
                metadata,
            );

            let note_outcome = self.update_tx.send(command.clone()).await;
            tracing::warn!(?note_outcome, ?command, "sending failure note command to update saga channel");
            if let Err(error) = note_outcome {
                tracing::warn!(
                    ?error,
                    "failed to update saga on zone command failure: {zone}"
                );
            }
        }
    }
}
