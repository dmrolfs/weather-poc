use super::state::AppState;
use crate::model::registrar::MONITORED_ZONES_QUERY_VIEW;
use crate::model::zone::WEATHER_QUERY_VIEW;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{routing, Json, Router};
use itertools::Itertools;
use serde::Serialize;
use serde_json::json;
use sql_query_builder as sql;
use std::collections::HashMap;
use strum_macros::{Display, EnumString, EnumVariantNames};
use utoipa::{OpenApi, ToSchema};

#[derive(OpenApi)]
#[openapi(
    paths(serve_health, serve_deep_health),
    components(
        schemas(HealthStatus, HealthStatusReport)
    ),
    tags(
        (name= "health", description = "Weather API")
    )
)]
pub struct HealthApiDoc;

pub fn api() -> Router<AppState> {
    Router::new()
        .route("/", routing::get(serve_health))
        .route("/deep", routing::get(serve_deep_health))
}

#[derive(
    Debug,
    Copy,
    Clone,
    PartialEq,
    Eq,
    Hash,
    Display,
    EnumString,
    EnumVariantNames,
    ToSchema,
    Serialize,
)]
#[strum(serialize_all = "camelCase", ascii_case_insensitive)]
pub enum HealthStatus {
    Up,
    NotReady,
    Error,
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq, ToSchema, Serialize)]
pub struct HealthStatusReport {
    status: HealthStatus,
}

impl From<HealthStatus> for HealthStatusReport {
    fn from(status: HealthStatus) -> Self {
        Self { status }
    }
}

impl From<HealthStatus> for StatusCode {
    fn from(health: HealthStatus) -> Self {
        match health {
            HealthStatus::Up => Self::OK,
            HealthStatus::Error => Self::INTERNAL_SERVER_ERROR,
            HealthStatus::Down | HealthStatus::NotReady => Self::SERVICE_UNAVAILABLE,
        }
    }
}

#[utoipa::path(
    get,
    path = "/",
    context_path = "/api/v1/health",
    tag = "health",
    responses(
        (status = 200, description = "system up"),
        (status = 5XX, description = "system down"),
    )
)]
#[axum::debug_handler]
#[tracing::instrument(level = "trace", skip(app))]
async fn serve_health(State(app): State<AppState>) -> impl IntoResponse {
    let (system_health, _) = check_health(app).await;
    let status_code: StatusCode = system_health.into();
    status_code
}

#[utoipa::path(
    get,
    path = "/deep",
    context_path = "/api/v1/health",
    tag = "health",
    responses(
        (status = 200, description = "system up"),
        (status = 5XX, description = "system down"),
    )
)]
#[axum::debug_handler]
#[tracing::instrument(level = "trace", skip(app))]
async fn serve_deep_health(State(app): State<AppState>) -> impl IntoResponse {
    let (system_health, _health_report) = check_health(app).await;
    serde_json::to_value::<HealthStatusReport>(system_health.into())
        .map(|resp| (system_health.into(), Json(resp)))
        .unwrap_or_else(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": error.to_string() })),
            )
        })
}

#[tracing::instrument(level = "trace", skip(state))]
async fn check_health(state: AppState) -> (HealthStatus, HashMap<HealthStatus, Vec<&'static str>>) {
    let weather_view_select_sql =
        sql::Select::new().select("version").from(WEATHER_QUERY_VIEW).to_string();
    let weather_view_status: Result<(), anyhow::Error> = sqlx::query(&weather_view_select_sql)
        .fetch_optional(&state.db_pool)
        .await
        .map_err(|err| err.into())
        .map(|_| ());

    let monitored_zones_view_select_sql = sql::Select::new()
        .select("version")
        .from(MONITORED_ZONES_QUERY_VIEW)
        .to_string();
    let monitored_zones_view_status: Result<(), anyhow::Error> =
        sqlx::query(&monitored_zones_view_select_sql)
            .fetch_optional(&state.db_pool)
            .await
            .map_err(|err| err.into())
            .map(|_| ());

    let model_select_sql = sql::Select::new().select("event_version").from("events").to_string();
    let model_status: Result<(), anyhow::Error> = sqlx::query(&model_select_sql)
        .fetch_optional(&state.db_pool)
        .await
        .map_err(|err| err.into())
        .map(|_| ());

    let service_statuses = vec![
        ("model", model_status),
        ("weather_view", weather_view_status),
        ("monitored_zones_view", monitored_zones_view_status),
    ];

    let service_by_status = service_statuses
        .into_iter()
        .map(|(service, status)| {
            let health = match status {
                Ok(()) => HealthStatus::Up,
                Err(error) => {
                    tracing::error!("{service} is down with error: {error:?}");
                    HealthStatus::Error
                },
            };
            (service, health)
        })
        .into_group_map_by(|(_, health)| *health);

    let health_report: HashMap<_, _> = service_by_status
        .into_iter()
        .map(|(status, service_status)| {
            let services: Vec<_> = service_status.into_iter().map(|s| s.0).collect();
            (status, services)
        })
        .collect();

    let all_services_are_up =
        health_report.iter().all(|(health, _services)| *health == HealthStatus::Up);
    let system_health = if all_services_are_up { HealthStatus::Up } else { HealthStatus::Down };

    (system_health, health_report)
}
