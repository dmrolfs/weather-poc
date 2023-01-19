use super::errors::ApiError;
use crate::model::registrar::{Registrar, RegistrarServices};
use crate::model::{registrar, RegistrarAggregate};
use crate::server::queries::{TracingQuery, WeatherQuery, WeatherViewProjection};
use axum::extract::FromRef;
use cqrs_es::Query;
use postgres_es::PostgresViewRepository;
use sqlx::PgPool;
use std::fmt;
use std::sync::Arc;

pub const WEATHER_QUERY_VIEW: &str = "weather_query";
pub const VIEW_PAYLOAD: &str = "payload";

#[tracing::instrument(level = "trace")]
pub async fn initialize_api_state(db_pool: PgPool) -> Result<AppState, ApiError> {
    // -- Registrar aggregate --
    let registrar_tracing_query = TracingQuery::<Registrar>::default();
    let registrar_queries: Vec<Box<dyn Query<Registrar>>> = vec![Box::new(registrar_tracing_query)];
    let registrar_services = RegistrarServices::HappyPath(registrar::HappyPathServices);
    let registrar_agg = Arc::new(postgres_es::postgres_cqrs(
        db_pool.clone(),
        registrar_queries,
        registrar_services,
    ));

    // -- UpdateLocationsSaga aggregate --
    let update_locations_tracing_query = TracingQuery::<UpdateLocations>::default();
    let update_locations_queries: Vec<Box<dyn Query<UpdateLocationsSaga>>> =
        vec![Box::new(update_locations_tracing_query)];
    let update_locations_agg = Arc::new(postgres_es::postgres_cqrs(
        db_pool.clone(),
        update_locations_queries,
        update_locations_services,
    ));

    // -- LocationZone aggregate --
    let location_zone_tracing_query = TracingQuery::<LocationZone>::default();
    let weather_view = Arc::new(PostgresViewRepository::new(
        WEATHER_QUERY_VIEW,
        db_pool.clone(),
    ));
    let mut weather_query = WeatherQuery::new(weather_view.clone());
    weather_query.use_error_handler(Box::new(
        |err| tracing::error!(error=?err, "weather query failed"),
    ));

    let location_queries: Vec<Box<dyn Query<LocationZone>>> = vec![
        Box::new(location_zone_tracing_query),
        Box::new(weather_query),
    ];
    let location_services = registrar::RegistrarServices::HappyPath(registrar::HappyPathServices);
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
    pub location_agg: LocationAggregate,
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

impl FromRef<AppState> for LocationAggregate {
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
