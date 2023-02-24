pub use errors::RegistrarError;
pub use protocol::{RegistrarCommand, RegistrarEvent};
pub use queries::{
    MonitoredZonesQuery, MonitoredZonesView, MonitoredZonesViewProjection,
    MONITORED_ZONES_QUERY_VIEW,
};
pub use service::{FullRegistrarServices, HappyPathServices, RegistrarServices};

use super::{registrar, LocationZoneAggregate, LocationZoneCode, UpdateLocationsSaga};
use crate::model::TracingQuery;
use async_trait::async_trait;
use cqrs_es::Aggregate;
use once_cell::sync::Lazy;
use postgres_es::{PostgresCqrs, PostgresViewRepository};
use serde::{Deserialize, Serialize};
use service::RegistrarApi;
use sqlx::PgPool;
use std::collections::HashSet;
use std::sync::Arc;
use tagid::{Entity, Id, Label};

pub type RegistrarAggregate = Arc<PostgresCqrs<Registrar>>;

pub const AGGREGATE_TYPE: &str = "registrar";

static REGISTRAR_SINGLETON_ID: Lazy<RegistrarId> = Lazy::new(Registrar::next_id);

pub fn make_registrar_aggregate(
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

pub type RegistrarId = Id<Registrar, <<Registrar as Entity>::IdGen as tagid::IdGenerator>::IdType>;

#[inline]
pub fn singleton_id() -> RegistrarId {
    REGISTRAR_SINGLETON_ID.clone()
}

#[derive(Debug, Default, Clone, Label, PartialEq, Serialize, Deserialize)]
pub struct Registrar {
    location_codes: HashSet<LocationZoneCode>,
}

impl tagid::Entity for Registrar {
    type IdGen = tagid::snowflake::pretty::PrettySnowflakeGenerator;
}

#[async_trait]
impl Aggregate for Registrar {
    type Command = RegistrarCommand;
    type Event = RegistrarEvent;
    type Error = RegistrarError;
    type Services = RegistrarServices;

    fn aggregate_type() -> String {
        AGGREGATE_TYPE.to_string()
    }

    #[tracing::instrument(level = "trace", skip())]
    async fn handle(
        &self, command: Self::Command, service: &Self::Services,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            RegistrarCommand::UpdateWeather => {
                let loc_codes: Vec<_> = self.location_codes.iter().collect();
                service.update_weather(&loc_codes).await.map(|_| vec![])
            },
            RegistrarCommand::MonitorForecastZone(zone) if !self.location_codes.contains(&zone) => {
                service.initialize_forecast_zone(&zone).await?;
                Ok(vec![RegistrarEvent::ForecastZoneAdded(zone)])
            },
            RegistrarCommand::MonitorForecastZone(zone) => Err(RegistrarError::RejectedCommand(
                format!("already monitoring location zone code: {zone}"),
            )),
            RegistrarCommand::ClearZoneMonitoring => {
                Ok(vec![RegistrarEvent::AllForecastZonesForgotten])
            },
            RegistrarCommand::ForgetForecastZone(zone) => {
                Ok(vec![RegistrarEvent::ForecastZoneForgotten(zone)])
            },
        }
    }

    #[tracing::instrument(level = "debug")]
    fn apply(&mut self, event: Self::Event) {
        match event {
            RegistrarEvent::ForecastZoneAdded(zone) => {
                self.location_codes.insert(zone);
            },
            RegistrarEvent::ForecastZoneForgotten(zone) => {
                self.location_codes.remove(&zone);
            },
            RegistrarEvent::AllForecastZonesForgotten => {
                self.location_codes.clear();
            },
        }
    }
}

mod service {
    use super::RegistrarError;
    use crate::model::update::UpdateLocationsCommand;
    use crate::model::zone::LocationZoneCommand;
    use crate::model::{LocationZoneAggregate, LocationZoneCode, UpdateLocationsSaga};
    use async_trait::async_trait;
    use std::fmt;

    #[async_trait]
    pub trait RegistrarApi: Sync + Send {
        async fn initialize_forecast_zone(
            &self, zone: &LocationZoneCode,
        ) -> Result<(), RegistrarError>;

        async fn update_weather(&self, zones: &[&LocationZoneCode]) -> Result<(), RegistrarError>;
    }

    #[derive(Debug, Clone)]
    pub enum RegistrarServices {
        Full(FullRegistrarServices),
        HappyPath(HappyPathServices),
    }

    #[async_trait]
    impl RegistrarApi for RegistrarServices {
        async fn initialize_forecast_zone(
            &self, zone: &LocationZoneCode,
        ) -> Result<(), RegistrarError> {
            match self {
                Self::Full(svc) => svc.initialize_forecast_zone(zone).await,
                Self::HappyPath(svc) => svc.initialize_forecast_zone(zone).await,
            }
        }

        async fn update_weather(&self, zones: &[&LocationZoneCode]) -> Result<(), RegistrarError> {
            match self {
                Self::Full(svc) => svc.update_weather(zones).await,
                Self::HappyPath(svc) => svc.update_weather(zones).await,
            }
        }
    }

    #[derive(Clone)]
    pub struct FullRegistrarServices {
        location: LocationZoneAggregate,
        update: UpdateLocationsSaga,
    }

    impl FullRegistrarServices {
        pub fn new(location: LocationZoneAggregate, update: UpdateLocationsSaga) -> Self {
            Self { location, update }
        }
    }

    impl fmt::Debug for FullRegistrarServices {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("StartUpdateLocationsService").finish()
        }
    }

    #[async_trait]
    impl RegistrarApi for FullRegistrarServices {
        async fn initialize_forecast_zone(
            &self, zone: &LocationZoneCode,
        ) -> Result<(), RegistrarError> {
            let aggregate_id = zone.as_ref();
            let command = LocationZoneCommand::WatchZone(zone.clone());
            self.location.execute(aggregate_id, command).await?;
            Ok(())
        }

        #[tracing::instrument(level = "debug", skip(self))]
        async fn update_weather(&self, zones: &[&LocationZoneCode]) -> Result<(), RegistrarError> {
            if zones.is_empty() {
                return Ok(());
            }

            let zone_ids = zones.iter().copied().cloned().collect();

            let saga_id = crate::model::update::generate_id();
            let metadata =
                maplit::hashmap! { "correlation".to_string() => saga_id.id.to_string(), };
            let command = UpdateLocationsCommand::UpdateLocations(saga_id.clone(), zone_ids);
            self.update.execute_with_metadata(&saga_id.id, command, metadata).await?;
            // Ok(events)
            Ok(())
        }
    }

    #[derive(Debug, Copy, Clone)]
    pub struct HappyPathServices;

    #[async_trait]
    impl RegistrarApi for HappyPathServices {
        async fn initialize_forecast_zone(
            &self, _zone: &LocationZoneCode,
        ) -> Result<(), RegistrarError> {
            Ok(())
        }

        async fn update_weather(&self, _zones: &[&LocationZoneCode]) -> Result<(), RegistrarError> {
            // let events = zones
            //     .iter()
            //     .map(|(loc, zone)| RegistrarEvent::LocationAdded(**loc, (*zone).clone()))
            //     .collect();
            // Ok(events)
            Ok(())
        }
    }
}

mod protocol {
    use crate::model::LocationZoneCode;
    use cqrs_es::DomainEvent;
    use serde::{Deserialize, Serialize};
    use strum_macros::Display;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum RegistrarCommand {
        UpdateWeather,
        MonitorForecastZone(LocationZoneCode),
        ClearZoneMonitoring,
        ForgetForecastZone(LocationZoneCode),
    }

    const VERSION: &str = "1.0";

    #[derive(Debug, Display, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[strum(serialize_all = "snake_case")]
    pub enum RegistrarEvent {
        ForecastZoneAdded(LocationZoneCode),
        ForecastZoneForgotten(LocationZoneCode),
        AllForecastZonesForgotten,
    }

    impl DomainEvent for RegistrarEvent {
        fn event_type(&self) -> String {
            self.to_string()
        }

        fn event_version(&self) -> String {
            VERSION.to_string()
        }
    }
}

mod queries {
    use crate::model::{LocationZoneCode, Registrar};
    use cqrs_es::persist::GenericQuery;
    use cqrs_es::{EventEnvelope, View};
    use postgres_es::PostgresViewRepository;
    use serde::{Deserialize, Serialize};
    use std::collections::HashSet;
    use std::sync::Arc;
    use utoipa::ToSchema;

    pub const MONITORED_ZONES_QUERY_VIEW: &str = "monitored_zones_query";
    pub type MonitoredZonesRepository = PostgresViewRepository<MonitoredZonesView, Registrar>;
    pub type MonitoredZonesViewProjection = Arc<MonitoredZonesRepository>;

    pub type MonitoredZonesQuery =
        GenericQuery<MonitoredZonesRepository, MonitoredZonesView, Registrar>;

    #[derive(Debug, Default, Clone, PartialEq, ToSchema, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct MonitoredZonesView {
        pub zones: HashSet<LocationZoneCode>,
    }

    impl View<Registrar> for MonitoredZonesView {
        fn update(&mut self, event: &EventEnvelope<Registrar>) {
            use super::RegistrarEvent as Evt;

            match &event.payload {
                Evt::ForecastZoneAdded(zone) => {
                    self.zones.insert(zone.clone());
                },
                Evt::ForecastZoneForgotten(zone) => {
                    self.zones.remove(zone);
                },
                Evt::AllForecastZonesForgotten => {
                    self.zones.clear();
                },
            }
        }
    }
}

mod errors {
    use crate::model::update::UpdateLocationsError;
    use crate::model::zone::LocationZoneError;
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum RegistrarError {
        #[error("{0}")]
        LocationZone(#[from] cqrs_es::AggregateError<LocationZoneError>),

        #[error("{0}")]
        UpdateForecastZones(#[from] cqrs_es::AggregateError<UpdateLocationsError>),

        #[error("rejected registrar command: {0}")]
        RejectedCommand(String),
    }
}
