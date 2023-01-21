use super::state::{AppState, WEATHER_QUERY_VIEW};
use crate::model::{registrar, RegistrarAggregate};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::{routing, Router};
use itertools::Itertools;
use sql_query_builder as sql;
use strum_macros::EnumVariantNames;
use utoipa::{OpenApi, ToSchema};

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
(status = 200, description = "Initiate weather update"),
(status = "5XX", description = "server error", body = WeatherError),
    ),
)]
#[axum::debug_handler]
#[tracing::instrument(level = "trace", skip(loc_registrar))]
async fn update_weather(State(loc_registrar): State<RegistrarAggregate>) -> impl IntoResponse {
    let aggregate_id = registrar::generate_id();
}