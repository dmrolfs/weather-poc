use crate::model::{LocationZoneCode, LocationZoneType, WeatherFrame, ZoneForecast};
use crate::services::noaa::{NoaaWeatherError, NoaaWeatherServices, ZoneWeatherApi};
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct LocationServices(NoaaWeatherServices);

impl LocationServices {
    pub fn new(noaa: NoaaWeatherServices) -> Self {
        Self(noaa)
    }
}

#[async_trait]
impl ZoneWeatherApi for LocationServices {
    async fn zone_observation(
        &self, zone_code: &LocationZoneCode,
    ) -> Result<WeatherFrame, NoaaWeatherError> {
        self.0.zone_observation(zone_code).await
    }

    async fn zone_forecast(
        &self, zone_type: LocationZoneType, zone_code: &LocationZoneCode,
    ) -> Result<ZoneForecast, NoaaWeatherError> {
        self.0.zone_forecast(zone_type, zone_code).await
    }
}
