use super::service::LocationServiceError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LocationZoneError {
    #[error("rejected command: {0}")]
    RejectedCommand(String),

    #[error("{0}")]
    LocationService(#[from] LocationServiceError),
}
