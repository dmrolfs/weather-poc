use super::state::AppState;
use crate::model::registrar::{MonitoredZonesView, MonitoredZonesViewProjection, RegistrarCommand};
use crate::model::update::{
    UpdateLocationsEvent, UpdateLocationsState, UpdateLocationsView, UpdateLocationsViewProjection,
};
use crate::model::zone::WeatherViewProjection;
use crate::model::{registrar, LocationZoneCode, RegistrarAggregate};
use crate::server::errors::ApiError;
use crate::server::result::OptionalResult;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{routing, Json, Router};
use cqrs_es::persist::ViewRepository;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};

#[derive(OpenApi)]
#[openapi(
    paths(
        update_weather,
        serve_update_state,
        serve_location_weather,
        serve_all_zones,
        delete_all_zones,
        add_forecast_zone,
        remove_forecast_zone,
    ),
    components(
        schemas(
            LocationZoneCode, UpdateLocationsView, MonitoredZonesView,
            UpdateLocationsEvent, UpdateLocationsState,
            crate::errors::WeatherError, ApiError,
        )
    ),
    tags((name= "weather", description = "Weather API"))
)]
pub struct WeatherApiDoc;

pub fn api() -> Router<AppState> {
    Router::new()
        .route("/", routing::post(update_weather))
        .route("/updates/:update_id", routing::get(serve_update_state))
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

    reg.execute(&aggregate_id.id, RegistrarCommand::UpdateWeather)
        .await
        .map_err::<ApiError, _>(|err| err.into())
        .map(move |()| (StatusCode::OK, aggregate_id.id.to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, IntoParams, ToSchema, Serialize, Deserialize)]
#[into_params(names("update_process_id"))]
#[repr(transparent)]
#[serde(transparent)]
struct UpdateProcessId(String);

impl std::fmt::Display for UpdateProcessId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl UpdateProcessId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl AsRef<str> for UpdateProcessId {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

#[utoipa::path(
    get,
    path = "/updates",
    context_path = "/api/v1/weather",
    tag = "weather",
    params(UpdateProcessId),
    responses(
        (status = 200, description = "report on update weather process", body = UpdateLocationsView),
        (status = 404, description = "no update process for identifier"),
    ),
)]
#[axum::debug_handler]
async fn serve_update_state(
    Path(update_id): Path<UpdateProcessId>, State(view_repo): State<UpdateLocationsViewProjection>,
) -> impl IntoResponse {
    view_repo
        .load(update_id.as_ref())
        .await
        .map_err::<ApiError, _>(|error| error.into())
        .map(|v| OptionalResult(v.map(Json)))
}

#[utoipa::path(
    get,
    path = "/zones",
    context_path = "/api/v1/weather",
    tag = "weather",
    responses(
        (status = 200, description = "list all zones to monitor", body = [MonitoredZonesView])
    ),
)]
#[tracing::instrument(level = "trace", skip(view_repo))]
async fn serve_all_zones(
    State(view_repo): State<MonitoredZonesViewProjection>,
) -> impl IntoResponse {
    let registrar_id = registrar::singleton_id();
    let view = view_repo
        .load(&registrar_id.id)
        .await
        .map_err::<ApiError, _>(|error| error.into())
        .map(|v| OptionalResult(v.map(Json)));

    tracing::debug!("view for registrar monitored zones: {view:?}");
    view
}

#[utoipa::path(
    delete,
    path = "/zones",
    context_path = "/api/v1/weather",
    tag = "weather",
    responses(
        (status = 200, description = "delete all zones"),
    ),
)]
#[tracing::instrument(level = "trace", skip(reg))]
async fn delete_all_zones(State(reg): State<RegistrarAggregate>) -> impl IntoResponse {
    let aggregate_id = registrar::singleton_id();
    reg.execute(&aggregate_id.id, RegistrarCommand::ClearZoneMonitoring)
        .await
        .map_err::<ApiError, _>(|err| err.into())
}

#[utoipa::path(
    post,
    path = "/zones",
    context_path = "/api/v1/weather",
    tag = "weather",
    params(LocationZoneCode),
    responses(
        (status = 200, description = "zone added to monitor"),
    )
)]
#[tracing::instrument(level = "trace", skip(reg))]
async fn add_forecast_zone(
    Path(zone_code): Path<LocationZoneCode>, State(reg): State<RegistrarAggregate>,
) -> impl IntoResponse {
    let aggregate_id = registrar::singleton_id();
    reg.execute(
        &aggregate_id.id,
        RegistrarCommand::MonitorForecastZone(zone_code),
    )
    .await
    .map_err::<ApiError, _>(|err| err.into())
}

#[utoipa::path(
    delete,
    path = "/zones",
    context_path = "/api/v1/weather",
    tag = "weather",
    params(LocationZoneCode),
    responses(
        (status = 200, description = "zone removed from monitor"),
    )
)]
#[tracing::instrument(level = "trace", skip(reg))]
async fn remove_forecast_zone(
    Path(zone_code): Path<LocationZoneCode>, State(reg): State<RegistrarAggregate>,
) -> impl IntoResponse {
    let aggregate_id = registrar::singleton_id();
    reg.execute(
        &aggregate_id.id,
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
