use super::service::WeatherApiError;
use thiserror::Error;

#[derive(Debug, Error)]
pub struct LocationZoneError;
