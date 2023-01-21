mod app;
mod happy;

pub use app::AppLocationServices;
pub use happy::HappyPathLocationServices;

use crate::errors::WeatherError;
use crate::model::{LocationZoneCode, WeatherFrame, ZoneForecast};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, USER_AGENT};
use thiserror::Error;
use url::Url;

#[async_trait]
pub trait WeatherApi: Sync + Send {
    async fn zone_observations(
        &self, zone: &LocationZoneCode,
    ) -> Result<WeatherFrame, LocationServiceError>;

    async fn zone_forecast(
        &self, zone: &LocationZoneCode,
    ) -> Result<ZoneForecast, LocationServiceError>;
}

#[derive(Debug, Clone)]
pub enum LocationServices {
    App(AppLocationServices),
    HappyPath(HappyPathLocationServices),
}

#[async_trait]
impl WeatherApi for LocationServices {
    async fn zone_observations(
        &self, zone: &LocationZoneCode,
    ) -> Result<WeatherFrame, LocationServiceError> {
        match self {
            Self::App(svc) => svc.zone_observations(zone).await,
            Self::HappyPath(svc) => svc.zone_observations(zone).await,
        }
    }

    async fn zone_forecast(
        &self, zone: &LocationZoneCode,
    ) -> Result<ZoneForecast, LocationServiceError> {
        match self {
            Self::App(svc) => svc.zone_forecast(zone).await,
            Self::HappyPath(svc) => svc.zone_forecast(zone).await,
        }
    }
}

#[derive(Debug, Error)]
pub enum LocationServiceError {
    #[error("supplied Weather API url is not a base url to query: {0}")]
    NotABaseUrl(Url),

    #[error("Weather API call failed: {0}")]
    HttpRequest(#[from] reqwest::Error),

    #[error("error occurred in HTTP middleware calling Weather API: {0}")]
    HttpMiddleware(#[from] reqwest_middleware::Error),

    #[error("failed to parse Weather API GeoJson response: {0}")]
    GeoJson(#[from] geojson::Error),

    #[error("{0}")]
    Weather(#[from] WeatherError),
    // #[error("failed to parse Weather API JSON response: {0}")]
    // Json(#[from] serde_json::Error),
}
