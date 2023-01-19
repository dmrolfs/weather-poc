mod app;
mod happy;
mod model;

pub use app::AppLocationServices;
pub use happy::HappyPathLocationServices;

use crate::model::{LocationZoneCode, WeatherFrame};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, USER_AGENT};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use url::Url;
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, ToSchema, Serialize, Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct ZoneForecast(String);

#[async_trait]
pub trait WeatherApi: Sync + Send {
    async fn zone_observations(
        &self, zone: &LocationZoneCode,
    ) -> Result<WeatherFrame, WeatherApiError>;

    async fn zone_forecast(&self, zone: &LocationZoneCode)
        -> Result<ZoneForecast, WeatherApiError>;
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
    ) -> Result<WeatherFrame, WeatherApiError> {
        match self {
            Self::App(svc) => svc.zone_observations(zone).await,
            Self::HappyPath(svc) => svc.zone_observations(zone).await,
        }
    }

    async fn zone_forecast(
        &self, zone: &LocationZoneCode,
    ) -> Result<ZoneForecast, WeatherApiError> {
        match self {
            Self::App(svc) => svc.zone_forecast(zone).await,
            Self::HappyPath(svc) => svc.zone_forecast(zone).await,
        }
    }
}

#[derive(Debug, Error)]
pub struct WeatherApiError;
