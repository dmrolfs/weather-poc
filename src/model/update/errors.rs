use crate::errors::WeatherError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum UpdateLocationsError {
    #[error("rejected command: {0}")]
    RejectedCommand(String),

    #[error("{0}")]
    Weather(#[from] WeatherError),
}
