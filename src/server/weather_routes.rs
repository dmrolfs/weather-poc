use super::state::AppState;
use crate::model::{registrar, RegistrarAggregate};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{routing, Router, Json};
use cqrs_es::persist::ViewRepository;
use utoipa::OpenApi;
use crate::server::errors::ApiError;
use crate::server::queries::{WeatherView, WeatherViewProjection};
use crate::server::result::OptionalResult;

#[derive(OpenApi)]
#[openapi(
paths(update_weather, serve_location_weather),
components(
schemas(WeatherView)
),
tags(
(name= "weather", description = "Weather API")
)
)]
pub struct WeatherApiDoc;

pub fn api() -> Router<AppState> {
    Router::new()
        .route("/", routing::post(update_weather))
        .route("/:zone", routing::get(serve_location_weather))
}

#[utoipa::path(
    post,
    path = "/",
    context_path = "/api/weather",
    tag = "weather",
    responses(
(status = 200, description = "Initiate services update"),
(status = "5XX", description = "server error", body = WeatherError),
    ),
)]
#[axum::debug_handler]
#[tracing::instrument(level = "trace", skip(loc_registrar))]
async fn update_weather(State(loc_registrar): State<RegistrarAggregate>) -> impl IntoResponse {
    let aggregate_id = registrar::generate_id();
    todo!()
}

#[utoipa::path(
    get,
    path = "/",
    context_path = "/api/weather",
    tag = "weather",
    params(
        ("zone_code" = String, Path, description = "Zone Code"),
    ),
    responses(
    (status = 200, description = "Location Weather Report", body = WeatherView),
    (status = 404, description = "No location zone found"),
    ),
)]
#[axum::debug_handler]
#[tracing::instrument(level="debug", skip(view_repo))]
async fn serve_location_weather(Path(zone_code): Path<String>, State(view_repo): State<WeatherViewProjection>) -> impl IntoResponse {
    let view = view_repo
        .load(zone_code.as_str())
        .await
        .map_err::<ApiError, _>(|err| err.into())
        .map(|v| OptionalResult(v.map(Json)));

    tracing::debug!("view for code[{zone_code}]: {view:?}");
    view
}
