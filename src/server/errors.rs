use thiserror::Error;
use utoipa::ToSchema;

#[derive(Debug, Error, ToSchema)]
pub enum ApiError {
    #[error("Invalid URL path input: {0}")]
    Path(#[from] axum::extract::rejection::PathRejection),

    #[error("0")]
    IO(#[from] std::io::Error),

    #[error("Invalid JSON payload: {0}")]
    Json(#[from] axum::extract::rejection::JsonRejection),

    #[error("HTTP engine error: {0}")]
    HttpEngine(#[from] hyper::Error),

    #[error("failed database operation: {0} ")]
    Sql(#[from] sqlx::Error),

    #[error("failed joining with thread: {0}")]
    Join(#[from] tokio::task::JoinError),
}
