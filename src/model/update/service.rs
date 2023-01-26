use crate::model::{UpdateLocations, WeatherAlert};
use crate::services::noaa::{AlertApi, NoaaWeatherError, NoaaWeatherServices};
use async_trait::async_trait;
use pretty_snowflake::Id;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct UpdateLocationsServices(NoaaWeatherServices);

impl UpdateLocationsServices {
    pub fn new(noaa: NoaaWeatherServices) -> Self { Self(noaa) }
}

#[async_trait]
impl AlertApi for UpdateLocationsServices {
    async fn active_alerts(&self) -> Result<Vec<WeatherAlert>, NoaaWeatherError> {
        self.0.active_alerts().await
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
