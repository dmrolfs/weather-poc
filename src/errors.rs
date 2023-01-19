use crate::{model, server};
use anyhow::anyhow;
use cqrs_es::{persist::PersistenceError, AggregateError};
use sqlx::Error;
use thiserror::Error;
use utoipa::ToSchema;

#[derive(Debug, ToSchema, Error)]
#[non_exhaustive]
pub enum WeatherError {
    // #[error("{0}")]
    // Api(#[from] server::ApiError),
    #[error("Encountered a technical failure: {source}")]
    Unexpected { source: anyhow::Error },
}
