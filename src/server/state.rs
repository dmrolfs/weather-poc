use super::errors::ApiError;
use crate::model::registrar::{self, Registrar, RegistrarAggregate, RegistrarServices};
use crate::model::update::{
    UpdateLocationZoneController, UpdateLocationsCommand, UpdateLocationsServices,
};
use crate::model::zone::{LocationServices, LocationZone, LocationZoneAggregate};
use crate::model::{UpdateLocations, UpdateLocationsSaga};
use crate::queries::{self, CommandEnvelope, CommandRelay, EventBroadcastQuery, EventSubscriber};
use crate::server::queries::{
    MonitoredZonesQuery, MonitoredZonesViewProjection, TracingQuery, WeatherQuery,
    WeatherViewProjection,
};
use crate::services::noaa::{NoaaWeatherApi, NoaaWeatherServices};
use axum::extract::FromRef;
use cqrs_es::Query;
use postgres_es::PostgresViewRepository;
use sqlx::PgPool;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use url::Url;

pub const WEATHER_QUERY_VIEW: &str = "weather_query";
pub const MONITORED_ZONES_QUERY_VIEW: &str = "monitored_zones_query";
pub const VIEW_PAYLOAD: &str = "payload";

#[derive(Clone)]
pub struct AppState {
    pub registrar_agg: RegistrarAggregate,
    pub update_locations_agg: UpdateLocationsSaga,
    pub location_agg: LocationZoneAggregate,
    pub weather_view: WeatherViewProjection,
    pub monitored_zones_view: MonitoredZonesViewProjection,
    pub db_pool: PgPool,
    pub location_relay_handler: Arc<JoinHandle<()>>,
    pub location_subscriber_handler: Arc<JoinHandle<()>>,
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

impl FromRef<AppState> for MonitoredZonesViewProjection {
    fn from_ref(app: &AppState) -> Self {
        app.monitored_zones_view.clone()
    }
}

impl FromRef<AppState> for PgPool {
    fn from_ref(app: &AppState) -> Self {
        app.db_pool.clone()
    }
}

#[tracing::instrument(level = "debug")]
pub async fn initialize_app_state(db_pool: PgPool) -> Result<AppState, ApiError> {
    let user_agent = axum::http::HeaderValue::from_str("(here.com, contact@example.com)")
        .expect("invalid user_agent");
    let base_url = Url::from_str("https://api.weather.gov")?;
    let noaa_api = NoaaWeatherApi::new(base_url, user_agent)?;
    let noaa = NoaaWeatherServices::Noaa(noaa_api);

    let (location_tx, location_rx) = mpsc::channel(num_cpus::get());
    let (update_tx, update_rx) = mpsc::channel(num_cpus::get());

    let location_broadcast_query: EventBroadcastQuery<LocationZone> =
        EventBroadcastQuery::new(num_cpus::get());
    let location_subscriber = location_broadcast_query.subscribe(
        update_tx.clone(),
        crate::model::update::location_event_to_command,
    );

    let update_locations_agg = make_update_locations_saga(
        location_tx,
        (update_tx, update_rx),
        &location_subscriber,
        noaa.clone(),
        db_pool.clone(),
    )
    .await;

    let (location_agg, weather_view) =
        make_location_zone_aggregate_view(location_broadcast_query, noaa, db_pool.clone());

    let (registrar_agg, registrar_view) = make_registrar_aggregate(
        db_pool.clone(),
        location_agg.clone(),
        update_locations_agg.clone(),
    );

    let location_relay = CommandRelay::new(location_agg.clone(), location_rx);
    let location_relay_handler = Arc::new(location_relay.run());
    let location_subscriber_handler = Arc::new(location_subscriber.run());

    Ok(AppState {
        registrar_agg,
        update_locations_agg,
        location_agg,
        weather_view,
        monitored_zones_view: registrar_view,
        db_pool,
        location_relay_handler,
        location_subscriber_handler,
    })
}

async fn make_update_locations_saga<C>(
    location_tx: mpsc::Sender<CommandEnvelope<LocationZone>>,
    (update_tx, update_rx): (
        mpsc::Sender<CommandEnvelope<UpdateLocations>>,
        mpsc::Receiver<CommandEnvelope<UpdateLocations>>,
    ),
    location_subscriber: &EventSubscriber<LocationZone, UpdateLocations, C>,
    noaa: NoaaWeatherServices, db_pool: PgPool,
) -> UpdateLocationsSaga
where
    C: FnMut(queries::EventEnvelope<LocationZone>) -> Vec<UpdateLocationsCommand>
        + Send
        + Sync
        + 'static,
{
    let update_locations_queries: Vec<Box<dyn Query<UpdateLocations>>> = vec![
        Box::<TracingQuery<UpdateLocations>>::default(),
        // Box::new(TracingQuery::<UpdateLocations>::default()),
        Box::new(UpdateLocationZoneController::new(
            noaa.clone(),
            location_tx,
            update_tx,
        )),
    ];
    let mut update_locations_services = UpdateLocationsServices::for_noaa(noaa);
    update_locations_services
        .with_subscriber_tx(location_subscriber.subscriber_admin_tx())
        .await;
    let agg = Arc::new(postgres_es::postgres_cqrs(
        db_pool,
        update_locations_queries,
        update_locations_services,
    ));

    let relay = CommandRelay::new(agg.clone(), update_rx);
    relay.run();
    agg
}

fn make_location_zone_aggregate_view(
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

fn make_registrar_aggregate(
    db_pool: PgPool, location_agg: LocationZoneAggregate, update_saga: UpdateLocationsSaga,
) -> (RegistrarAggregate, MonitoredZonesViewProjection) {
    let monitored_zones_view = Arc::new(PostgresViewRepository::new(
        MONITORED_ZONES_QUERY_VIEW,
        db_pool.clone(),
    ));
    let mut monitored_zones_query = MonitoredZonesQuery::new(monitored_zones_view.clone());
    monitored_zones_query.use_error_handler(Box::new(|error| {
        tracing::error!(?error, "monitored zones query failed")
    }));

    let agg = Arc::new(postgres_es::postgres_cqrs(
        db_pool,
        vec![
            Box::<TracingQuery<Registrar>>::default(),
            Box::new(monitored_zones_query),
        ],
        // vec![Box::new(TracingQuery::<Registrar>::default())],
        RegistrarServices::Full(registrar::FullRegistrarServices::new(
            location_agg,
            update_saga,
        )),
    ));

    (agg, monitored_zones_view)
}
