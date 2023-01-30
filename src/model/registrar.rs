pub use errors::RegistrarError;
pub use protocol::{RegistrarCommand, RegistrarEvent};
pub use service::{HappyPathServices, RegistrarServices, StartUpdateLocationsServices};

use super::Location;
use crate::model::LocationZoneCode;
use async_trait::async_trait;
use cqrs_es::Aggregate;
use postgres_es::PostgresCqrs;
use pretty_snowflake::{Id, Label};
use serde::{Deserialize, Serialize};
use service::RegistrarApi;
use std::collections::HashMap;
use std::sync::Arc;

pub type RegistrarAggregate = Arc<PostgresCqrs<Registrar>>;

pub const AGGREGATE_TYPE: &str = "registrar";

#[inline]
pub fn generate_id() -> Id<Registrar> {
    pretty_snowflake::generator::next_id()
}

#[derive(Debug, Default, Clone, Label, PartialEq, Serialize, Deserialize)]
pub struct Registrar {
    location_codes: HashMap<Location, LocationZoneCode>,
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
            RegistrarCommand::MonitorLocation(loc, zone) => {
                Ok(vec![RegistrarEvent::LocationAdded(loc, zone)])
            },
        }
    }

    #[tracing::instrument(level = "debug")]
    fn apply(&mut self, event: Self::Event) {
        match event {
            RegistrarEvent::LocationAdded(loc, zone) => {
                self.location_codes.insert(loc, zone);
            },
        }
    }
}

mod service {
    use super::RegistrarError;
    use crate::model::update::UpdateLocationsCommand;
    use crate::model::{Location, LocationZoneCode, UpdateLocationsSaga};
    use async_trait::async_trait;
    use std::fmt;

    #[async_trait]
    pub trait RegistrarApi: Sync + Send {
        async fn update_weather(
            &self, zones: &[(&Location, &LocationZoneCode)],
        ) -> Result<(), RegistrarError>;
    }

    #[derive(Debug, Clone)]
    pub enum RegistrarServices {
        Saga(StartUpdateLocationsServices),
        HappyPath(HappyPathServices),
    }

    #[async_trait]
    impl RegistrarApi for RegistrarServices {
        async fn update_weather(
            &self, zones: &[(&Location, &LocationZoneCode)],
        ) -> Result<(), RegistrarError> {
            match self {
                Self::Saga(svc) => svc.update_weather(zones).await,
                Self::HappyPath(svc) => svc.update_weather(zones).await,
            }
        }
    }

    #[derive(Clone)]
    pub struct StartUpdateLocationsServices(UpdateLocationsSaga);

    impl StartUpdateLocationsServices {
        pub fn new(saga: UpdateLocationsSaga) -> Self {
            Self(saga)
        }
    }

    impl fmt::Debug for StartUpdateLocationsServices {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("StartUpdateLocationsService").finish()
        }
    }

    #[async_trait]
    impl RegistrarApi for StartUpdateLocationsServices {
        #[tracing::instrument(level = "debug", skip(self))]
        async fn update_weather(
            &self, zones: &[(&Location, &LocationZoneCode)],
        ) -> Result<(), RegistrarError> {
            // let mut zone_ids = Vec::with_capacity(zones.len());
            // let mut events = Vec::with_capacity(zones.len());
            // for (location, zone_id) in zones {
            //     let location = *(*location);
            //     let zone_id = (*zone_id).clone();
            //     events.push(RegistrarEvent::LocationAdded(location, zone_id.clone()));
            //     zone_ids.push(zone_id);
            // }
            let zone_ids = zones.iter().map(|(_, code)| *code).cloned().collect();

            let saga_id = crate::model::update::generate_id();
            let metadata =
                maplit::hashmap! { "correlation".to_string() => saga_id.pretty().to_string(), };
            let command = UpdateLocationsCommand::UpdateLocations(saga_id.clone(), zone_ids);
            self.0.execute_with_metadata(saga_id.pretty(), command, metadata).await?;
            // Ok(events)
            Ok(())
        }
    }

    #[derive(Debug, Copy, Clone)]
    pub struct HappyPathServices;

    #[async_trait]
    impl RegistrarApi for HappyPathServices {
        async fn update_weather(
            &self, _zones: &[(&Location, &LocationZoneCode)],
        ) -> Result<(), RegistrarError> {
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
    use crate::model::{Location, LocationZoneCode};
    use cqrs_es::DomainEvent;
    use serde::{Deserialize, Serialize};
    use strum_macros::Display;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum RegistrarCommand {
        UpdateWeather,
        MonitorLocation(Location, LocationZoneCode),
    }

    const VERSION: &str = "1.0";

    #[derive(Debug, Display, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[strum(serialize_all = "snake_case")]
    pub enum RegistrarEvent {
        LocationAdded(Location, LocationZoneCode),
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
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum RegistrarError {
        #[error("{0}")]
        UpdateLocations(#[from] cqrs_es::AggregateError<UpdateLocationsError>),
    }
}
