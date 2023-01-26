use super::errors::ApiError;
use crate::model::registrar::{self, Registrar, RegistrarAggregate, RegistrarServices};
use crate::model::update::UpdateLocationsServices;
use crate::model::zone::{LocationServices, LocationZone, LocationZoneAggregate};
use crate::model::{UpdateLocations, UpdateLocationsSaga};
use crate::server::queries::{TracingQuery, WeatherQuery, WeatherViewProjection};
use crate::services::noaa::{NoaaWeatherApi, NoaaWeatherServices};
use axum::extract::FromRef;
use cqrs_es::Query;
use postgres_es::PostgresViewRepository;
use sqlx::PgPool;
use std::fmt;
use std::sync::Arc;
use url::Url;
use std::str::FromStr;

pub const WEATHER_QUERY_VIEW: &str = "weather_query";
pub const VIEW_PAYLOAD: &str = "payload";

#[tracing::instrument(level = "trace")]
pub async fn initialize_app_state(db_pool: PgPool) -> Result<AppState, ApiError> {
    let user_agent = axum::http::HeaderValue::from_str("(here.com, contact@example.com)")
        .expect("invalid user_agent");
    let base_url = Url::from_str("https://api.weather.gov")?;
    let noaa_api = NoaaWeatherApi::new(base_url, user_agent)?;
    let noaa = NoaaWeatherServices::NOAA(noaa_api);

    // -- UpdateLocationsSaga aggregate --
    let update_locations_tracing_query = TracingQuery::<UpdateLocations>::default();
    let update_locations_queries: Vec<Box<dyn Query<UpdateLocations>>> =
        vec![Box::new(update_locations_tracing_query)];
    let update_locations_services = UpdateLocationsServices::new(noaa.clone());
    let update_locations_agg = Arc::new(postgres_es::postgres_cqrs(
        db_pool.clone(),
        update_locations_queries,
        update_locations_services,
    ));

    // -- Registrar aggregate --
    let registrar_tracing_query = TracingQuery::<Registrar>::default();
    let registrar_queries: Vec<Box<dyn Query<Registrar>>> = vec![Box::new(registrar_tracing_query)];
    let registrar_services = RegistrarServices::Saga(registrar::StartUpdateLocationsServices::new(
        update_locations_agg.clone(),
    ));
    let registrar_agg = Arc::new(postgres_es::postgres_cqrs(
        db_pool.clone(),
        registrar_queries,
        registrar_services,
    ));

    // -- LocationZone aggregate --
    let location_zone_tracing_query = TracingQuery::<LocationZone>::default();
    let weather_view = Arc::new(PostgresViewRepository::new(
        WEATHER_QUERY_VIEW,
        db_pool.clone(),
    ));
    let mut weather_query = WeatherQuery::new(weather_view.clone());
    weather_query.use_error_handler(Box::new(
        |err| tracing::error!(error=?err, "services query failed"),
    ));

    let location_queries: Vec<Box<dyn Query<LocationZone>>> = vec![
        Box::new(location_zone_tracing_query),
        Box::new(weather_query),
    ];
    let location_services = LocationServices::new(noaa.clone());
    let location_agg = Arc::new(postgres_es::postgres_cqrs(
        db_pool.clone(),
        location_queries,
        location_services,
    ));

    // -- assemble app state --
    Ok(AppState {
        registrar_agg,
        update_locations_agg,
        location_agg,
        weather_view,
        db_pool,
    })
}

#[derive(Clone)]
pub struct AppState {
    pub registrar_agg: RegistrarAggregate,
    pub update_locations_agg: UpdateLocationsSaga,
    pub location_agg: LocationZoneAggregate,
    pub weather_view: WeatherViewProjection,
    pub db_pool: PgPool,
}

impl fmt::Debug for AppState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiState").finish()
    }
}

impl FromRef<AppState> for RegistrarAggregate {
    fn from_ref(app: &AppState) -> Self {
        app.registrar_agg.clone()
    }
}

impl FromRef<AppState> for UpdateLocationsSaga {
    fn from_ref(app: &AppState) -> Self {
        app.update_locations_agg.clone()
    }
}

impl FromRef<AppState> for LocationZoneAggregate {
    fn from_ref(app: &AppState) -> Self {
        app.location_agg.clone()
    }
}

impl FromRef<AppState> for WeatherViewProjection {
    fn from_ref(app: &AppState) -> Self {
        app.weather_view.clone()
    }
}

impl FromRef<AppState> for PgPool {
    fn from_ref(app: &AppState) -> Self {
        app.db_pool.clone()
    }
}
