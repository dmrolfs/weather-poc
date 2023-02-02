mod errors;
mod protocol;
mod queries;
mod saga;
mod service;
mod zone_controller;

pub use errors::UpdateLocationsError;
pub use protocol::{location_event_to_command, UpdateLocationsCommand, UpdateLocationsEvent};
pub use queries::{
    UpdateLocationsQuery, UpdateLocationsView, UpdateLocationsViewProjection,
    UPDATE_LOCATIONS_QUERY_VIEW,
};
pub use saga::{generate_id, UpdateLocations, UpdateLocationsSaga, UpdateLocationsState};
pub use service::UpdateLocationsServices;
pub use zone_controller::UpdateLocationZoneController;

use crate::model;
use crate::model::{CommandRelay, EventSubscriber, LocationZone, TracingQuery};
use crate::services::noaa::NoaaWeatherServices;
use cqrs_es::Query;
use postgres_es::PostgresViewRepository;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::mpsc;

pub async fn make_update_locations_saga<C>(
    location_tx: mpsc::Sender<model::CommandEnvelope<LocationZone>>,
    (update_tx, update_rx): (
        mpsc::Sender<model::CommandEnvelope<UpdateLocations>>,
        mpsc::Receiver<model::CommandEnvelope<UpdateLocations>>,
    ),
    location_subscriber: &EventSubscriber<LocationZone, UpdateLocations, C>,
    noaa: NoaaWeatherServices, db_pool: PgPool,
) -> (UpdateLocationsSaga, UpdateLocationsViewProjection)
where
    C: FnMut(model::EventEnvelope<LocationZone>) -> Vec<UpdateLocationsCommand>
        + Send
        + Sync
        + 'static,
{
    let update_locations_view = Arc::new(PostgresViewRepository::new(
        UPDATE_LOCATIONS_QUERY_VIEW,
        db_pool.clone(),
    ));
    let mut update_locations_query = UpdateLocationsQuery::new(update_locations_view.clone());
    update_locations_query.use_error_handler(Box::new(|error| {
        tracing::error!(?error, "update locations query failed")
    }));

    let update_locations_queries: Vec<Box<dyn Query<UpdateLocations>>> = vec![
        Box::<TracingQuery<UpdateLocations>>::default(),
        Box::new(update_locations_query),
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

    (agg, update_locations_view)
}
