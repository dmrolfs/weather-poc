pub use errors::RegistrarError;
pub use protocol::{RegistrarCommand, RegistrarEvent};
pub use service::{FullRegistrarServices, HappyPathServices, RegistrarServices};

use crate::model::LocationZoneCode;
use async_trait::async_trait;
use cqrs_es::Aggregate;
use once_cell::sync::Lazy;
use postgres_es::PostgresCqrs;
use pretty_snowflake::{Id, Label, Labeling};
use serde::{Deserialize, Serialize};
use service::RegistrarApi;
use std::collections::HashSet;
use std::sync::Arc;

pub type RegistrarAggregate = Arc<PostgresCqrs<Registrar>>;

pub const AGGREGATE_TYPE: &str = "registrar";
pub const REGISTRAR_SNOWFLAKE_ID: i64 = 1;
pub const REGISTRAR_PRETTY_ID: &str = "<singleton>";

static REGISTRAR_SINGLETON_ID: Lazy<Id<Registrar>> = Lazy::new(|| {
    Id::direct(
        <Registrar as Label>::labeler().label(),
        REGISTRAR_SNOWFLAKE_ID,
        REGISTRAR_PRETTY_ID,
    )
});

#[inline]
pub fn singleton_id() -> Id<Registrar> {
    REGISTRAR_SINGLETON_ID.clone()
}

#[derive(Debug, Default, Clone, Label, PartialEq, Serialize, Deserialize)]
pub struct Registrar {
    location_codes: HashSet<LocationZoneCode>,
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
                maplit::hashmap! { "correlation".to_string() => saga_id.pretty().to_string(), };
            let command = UpdateLocationsCommand::UpdateLocations(saga_id.clone(), zone_ids);
            self.update
                .execute_with_metadata(saga_id.pretty(), command, metadata)
                .await?;
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
