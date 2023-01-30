use super::errors::ApiError;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::borrow::Cow;

pub type HttpResult = Result<Response, ApiError>;

#[derive(Debug)]
#[repr(transparent)]
pub struct OptionalResult<T>(pub Option<T>);

impl<T: IntoResponse> IntoResponse for OptionalResult<T> {
    fn into_response(self) -> Response {
        self.0
            .map(|result| (StatusCode::OK, result).into_response())
            .unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
    }
}

impl<T: IntoResponse> From<Option<T>> for OptionalResult<T> {
    fn from(result: Option<T>) -> Self {
        Self(result)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let http_error = HttpError::from_error(self.into());
        http_error.into_response()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorReport {
    pub error: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub backtrace: Option<String>,
}

impl From<anyhow::Error> for ErrorReport {
    fn from(error: anyhow::Error) -> Self {
        Self {
            error: error.to_string(),
            error_code: None,
            backtrace: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum HttpError {
    BadRequest { error: ErrorReport },
    NotFound { message: Cow<'static, str> },
    Internal { error: ErrorReport },
}

impl HttpError {
    fn from_error(error: anyhow::Error) -> Self {
        tracing::error!("HTTP handler error: {error}");
        match error.downcast_ref::<ApiError>() {
            Some(ApiError::Path(_)) => Self::BadRequest { error: error.into() },
            Some(
                ApiError::Registrar(_)
                | ApiError::ParseUrl(_)
                | ApiError::Noaa(_)
                | ApiError::IO(_)
                | ApiError::Json(_)
                | ApiError::HttpEngine(_)
                | ApiError::Sql(_)
                | ApiError::Database { .. }
                | ApiError::Join(_),
            ) => Self::Internal { error: error.into() },

            // Some(BankError::BankAccount(BankAccountError::NotFound(account_id))) => {
            //     Self::NotFound {
            //         message: format!("No bank account found for account id: {account_id}").into(),
            //     }
            // },
            // Some(BankError::BankAccount(_)) => Self::BadRequest { error: error.into() },
            // Some(BankError::Api(_)) => Self::Internal { error: error.into() },
            // Some(BankError::Validation(_)) => Self::BadRequest { error: error.into() },
            // Some(BankError::User(_)) => Self::BadRequest { error: error.into() },
            //
            // // consideration in explicit list rt. short circuit is compiler-enforced review of how
            // // respond to new BankError variants
            // Some(BankError::AggregateConflict)
            // | Some(BankError::DatabaseConnection { .. })
            // | Some(BankError::Deserialization { .. })
            // | Some(BankError::Unexpected { .. }) => Self::Internal { error: error.into() },
            // Some(_) => Self::Internal { error: error.into() },
            None => Self::Internal { error: error.into() },
        }
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound { message } => (StatusCode::NOT_FOUND, Json(message)).into_response(),
            Self::BadRequest { error } => (StatusCode::BAD_REQUEST, Json(error)).into_response(),
            Self::Internal { error } => {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response()
            },
        }
    }
}
