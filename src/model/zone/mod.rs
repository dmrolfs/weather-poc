mod errors;
mod location;
mod protocol;
mod queries;
mod service;

pub use errors::LocationZoneError;
pub use location::{LocationZone, LocationZoneAggregate};
pub use protocol::{LocationZoneCommand, LocationZoneEvent};
pub use queries::{WeatherQuery, WeatherView, WeatherViewProjection, WEATHER_QUERY_VIEW};
pub use service::LocationServices;

use crate::model::{EventBroadcastQuery, TracingQuery};
use crate::services::noaa::NoaaWeatherServices;
use cqrs_es::Query;
use postgres_es::PostgresViewRepository;
use sqlx::PgPool;
use std::sync::Arc;

pub fn make_location_zone_aggregate_view(
    location_broadcast_query: EventBroadcastQuery<LocationZone>, noaa: NoaaWeatherServices,
    db_pool: PgPool,
) -> (LocationZoneAggregate, WeatherViewProjection) {
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
        Box::new(location_broadcast_query),
        Box::new(location_zone_tracing_query),
        Box::new(weather_query),
    ];
    let location_services = LocationServices::new(noaa);
    let agg = Arc::new(postgres_es::postgres_cqrs(
        db_pool,
        location_queries,
        location_services,
    ));

    (agg, weather_view)
}
