use super::state::AppState;
use crate::model::registrar::RegistrarCommand;
use crate::model::{registrar, LocationZoneCode, RegistrarAggregate};
use crate::server::errors::ApiError;
use crate::server::queries::{MonitoredZonesViewProjection, WeatherView, WeatherViewProjection};
use crate::server::result::OptionalResult;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{routing, Json, Router};
use cqrs_es::persist::ViewRepository;
use utoipa::OpenApi;

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
        .route(
            "/zones",
            routing::get(serve_all_zones).delete(delete_all_zones),
        )
        .route(
            "/zones/:zone",
            routing::post(add_forecast_zone).delete(remove_forecast_zone),
        )
}

#[utoipa::path(
    post,
    path = "/",
    context_path = "/api/v1/weather",
    tag = "weather",
    responses(
(status = 200, description = "Initiate services update"),
(status = "5XX", description = "server error", body = WeatherError),
    ),
)]
#[axum::debug_handler]
#[tracing::instrument(level = "debug", skip(reg))]
async fn update_weather(State(reg): State<RegistrarAggregate>) -> impl IntoResponse {
    let aggregate_id = registrar::singleton_id();
    reg.execute(aggregate_id.pretty(), RegistrarCommand::UpdateWeather)
        .await
        .map_err::<ApiError, _>(|err| err.into())
}

#[tracing::instrument(level = "trace", skip(view_repo))]
async fn serve_all_zones(
    State(view_repo): State<MonitoredZonesViewProjection>,
) -> impl IntoResponse {
    let registrar_id = registrar::singleton_id();
    let view = view_repo
        .load(registrar_id.pretty())
        .await
        .map_err::<ApiError, _>(|error| error.into())
        .map(|v| OptionalResult(v.map(Json)));

    tracing::debug!("view for registrar monitored zones: {view:?}");
    view
}

#[tracing::instrument(level = "trace", skip(reg))]
async fn delete_all_zones(State(reg): State<RegistrarAggregate>) -> impl IntoResponse {
    let aggregate_id = registrar::singleton_id();
    reg.execute(aggregate_id.pretty(), RegistrarCommand::ClearZoneMonitoring)
        .await
        .map_err::<ApiError, _>(|err| err.into())
}

#[tracing::instrument(level = "trace", skip(reg))]
async fn add_forecast_zone(
    Path(zone_code): Path<LocationZoneCode>, State(reg): State<RegistrarAggregate>,
) -> impl IntoResponse {
    let aggregate_id = registrar::singleton_id();
    reg.execute(
        aggregate_id.pretty(),
        RegistrarCommand::MonitorForecastZone(zone_code),
    )
    .await
    .map_err::<ApiError, _>(|err| err.into())
}

#[tracing::instrument(level = "trace", skip(reg))]
async fn remove_forecast_zone(
    Path(zone_code): Path<LocationZoneCode>, State(reg): State<RegistrarAggregate>,
) -> impl IntoResponse {
    let aggregate_id = registrar::singleton_id();
    reg.execute(
        aggregate_id.pretty(),
        RegistrarCommand::ForgetForecastZone(zone_code),
    )
    .await
    .map_err::<ApiError, _>(|err| err.into())
}

#[utoipa::path(
    get,
    path = "/",
    context_path = "/api/v1/weather",
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
#[tracing::instrument(level = "debug", skip(view_repo))]
async fn serve_location_weather(
    Path(zone_code): Path<LocationZoneCode>, State(view_repo): State<WeatherViewProjection>,
) -> impl IntoResponse {
    let view = view_repo
        .load(zone_code.as_ref())
        .await
        .map_err::<ApiError, _>(|err| err.into())
        .map(|v| OptionalResult(v.map(Json)));

    tracing::debug!("view for code[{zone_code}]: {view:?}");
    view
}
