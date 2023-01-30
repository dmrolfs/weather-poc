use crate::model::{LocationZoneCode, WeatherAlert};
use crate::queries::SubscribeCommand;
use crate::services::noaa::{AlertApi, NoaaWeatherError, NoaaWeatherServices};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

#[derive(Debug, Clone)]
pub struct UpdateLocationsServices {
    location_subscriber_tx: Arc<RwLock<Option<mpsc::Sender<SubscribeCommand>>>>,
    noaa: NoaaWeatherServices,
}

impl UpdateLocationsServices {
    pub fn new(
        location_subscriber_tx: mpsc::Sender<SubscribeCommand>, noaa: NoaaWeatherServices,
    ) -> Self {
        Self {
            location_subscriber_tx: Arc::new(RwLock::new(Some(location_subscriber_tx))),
            noaa,
        }
    }

    pub fn for_noaa(noaa: NoaaWeatherServices) -> Self {
        Self {
            location_subscriber_tx: Arc::new(RwLock::new(None)),
            noaa,
        }
    }

    pub async fn with_subscriber_tx(&mut self, subscriber_tx: mpsc::Sender<SubscribeCommand>) {
        *self.location_subscriber_tx.write().await = Some(subscriber_tx);
    }

    pub async fn add_subscriber(
        &self, subscriber_id: impl Into<String>, zones: &[LocationZoneCode],
    ) {
        let subscriber_tx = self.location_subscriber_tx.read().await;
        if let Some(ref tx) = *subscriber_tx {
            let subscriber_id = subscriber_id.into();
            let publisher_ids = zones.iter().map(|z| z.to_string()).collect();
            let outcome = tx
                .send(SubscribeCommand::Add {
                    subscriber_id: subscriber_id.clone(),
                    publisher_ids,
                })
                .await;
            if let Err(error) = outcome {
                tracing::error!(
                    ?error,
                    "location broadcast subscription failed for update saga: {subscriber_id}"
                );
            }
        }
    }

    pub async fn remove_subscriber(&self, subscriber_id: impl Into<String>) {
        let subscriber_tx = self.location_subscriber_tx.read().await;
        if let Some(ref tx) = *subscriber_tx {
            let subscriber_id = subscriber_id.into();
            let outcome = tx
                .send(SubscribeCommand::Remove { subscriber_id: subscriber_id.clone() })
                .await;
            if let Err(error) = outcome {
                tracing::error!(
                    ?error,
                    "location broadcast subscirption failed for update saga: {subscriber_id}"
                );
            }
        }
    }
}

#[async_trait]
impl AlertApi for UpdateLocationsServices {
    async fn active_alerts(&self) -> Result<Vec<WeatherAlert>, NoaaWeatherError> {
        self.noaa.active_alerts().await
    }
}

// impl UpdateLocationsServices {
//     #[tracing::instrument(level="debug", skip())]
//     pub async fn update_zone_weather(&self, zone: &LocationZoneIdentifier) -> Result<(), WeatherError>{
//         self.zone_cqrs.execute_with_metadata(zone.code.as_str(), LocationZoneCommand::Observe, self.meta_data.clone()).await?;
//         Ok(())
//     }
//
//     #[tracing::instrument(level="debug", skip())]
//     pub async fn update_zone_forecast(&self, zone: &LocationZoneIdentifier) -> Result<(), WeatherError> {
//         self.zone_cqrs.execute_with_metadata(zone.code.as_str(), LocationZoneCommand::Forecast, self.meta_data.clone()).await?;
//         Ok(())
//     }
//
//     #[tracing::instrument(level="debug", skip())]
//     pub async fn mark_alert_status(&self, zone: &LocationZoneIdentifier, active_alert: bool) -> Result<(), WeatherError> {
//         self.zone_cqrs.execute_with_metadata(zone.code.as_str(), LocationZoneCommand::NoteAlertStatus(active_alert), self.meta_data.clone()).await?;
//         Ok(())
//     }
// }
