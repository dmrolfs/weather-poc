use super::{LocationServiceError, WeatherApi, ZoneForecast};
use crate::model::{LocationZoneCode, WeatherFrame};
use async_trait::async_trait;

#[derive(Debug, Copy, Clone)]
pub struct HappyPathLocationServices;

#[async_trait]
impl WeatherApi for HappyPathLocationServices {
    async fn zone_observations(
        &self, zone: &LocationZoneCode,
    ) -> Result<WeatherFrame, LocationServiceError> {
        todo!()
    }

    async fn zone_forecast(
        &self, zone: &LocationZoneCode,
    ) -> Result<ZoneForecast, LocationServiceError> {
        todo!()
    }
}
