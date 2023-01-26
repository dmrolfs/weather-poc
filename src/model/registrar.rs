pub use errors::RegistrarError;
pub use protocol::{RegistrarCommand, RegistrarEvent};
pub use service::{HappyPathServices, RegistrarServices, StartUpdateLocationsServices};

use super::Location;
use crate::model::LocationZoneIdentifier;
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
    location_codes: HashMap<Location, LocationZoneIdentifier>,
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
            Self::Command::UpdateWeather => {
                let loc_codes: Vec<_> = self.location_codes.iter().collect();
                service.update_weather(&loc_codes).await
            },
            Self::Command::MonitorLocation(loc, zone) => {
                Ok(vec![Self::Event::LocationAdded(loc, zone)])
            },
        }
    }

    fn apply(&mut self, event: Self::Event) {
        match event {
            Self::Event::LocationAdded(loc, zone) => {
                self.location_codes.insert(loc, zone);
            },
        }
    }
}

mod service {
    use std::fmt;
    use super::RegistrarError;
    use crate::model::registrar::RegistrarEvent;
    use crate::model::update::UpdateLocationsCommand;
    use crate::model::{Location, LocationZoneIdentifier, UpdateLocationsSaga};
    use async_trait::async_trait;

    #[async_trait]
    pub trait RegistrarApi: Sync + Send {
        async fn update_weather(
            &self, zones: &[(&Location, &LocationZoneIdentifier)],
        ) -> Result<Vec<RegistrarEvent>, RegistrarError>;
    }

    #[derive(Debug, Clone)]
    pub enum RegistrarServices {
        Saga(StartUpdateLocationsServices),
        HappyPath(HappyPathServices),
    }

    #[async_trait]
    impl RegistrarApi for RegistrarServices {
        async fn update_weather(
            &self, zones: &[(&Location, &LocationZoneIdentifier)],
        ) -> Result<Vec<RegistrarEvent>, RegistrarError> {
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
        async fn update_weather(
            &self, zones: &[(&Location, &LocationZoneIdentifier)],
        ) -> Result<Vec<RegistrarEvent>, RegistrarError> {
            let mut zone_ids = Vec::with_capacity(zones.len());
            let mut events = Vec::with_capacity(zones.len());
            for (location, zone_id) in zones {
                let location = location.clone();
                let zone_id = zone_id.clone();
                events.push(RegistrarEvent::LocationAdded(
                    location.clone(),
                    zone_id.clone(),
                ));
                zone_ids.push(zone_id.clone());
            }

            let saga_id = crate::model::update::generate_id();
            let metadata =
                maplit::hashmap! { "correlation".to_string() => saga_id.pretty().to_string(), };
            let command = UpdateLocationsCommand::UpdateLocations(zone_ids);
            self.0.execute_with_metadata(saga_id.pretty(), command, metadata).await?;
            Ok(events)
        }
    }

    #[derive(Debug, Copy, Clone)]
    pub struct HappyPathServices;

    #[async_trait]
    impl RegistrarApi for HappyPathServices {
        async fn update_weather(
            &self, zones: &[(&Location, &LocationZoneIdentifier)],
        ) -> Result<Vec<RegistrarEvent>, RegistrarError> {
            let events = zones
                .iter()
                .map(|(loc, zone)| RegistrarEvent::LocationAdded(**loc, (*zone).clone()))
                .collect();
            Ok(events)
        }
    }
}

mod protocol {
    use crate::model::{Location, LocationZoneIdentifier};
    use cqrs_es::DomainEvent;
    use serde::{Deserialize, Serialize};
    use strum::Display;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum RegistrarCommand {
        UpdateWeather,
        MonitorLocation(Location, LocationZoneIdentifier),
    }

    const VERSION: &str = "1.0";

    #[derive(Debug, Display, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[strum(serialize_all = "snake_case")]
    pub enum RegistrarEvent {
        LocationAdded(Location, LocationZoneIdentifier),
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
    use thiserror::Error;
    use crate::model::update::UpdateLocationsError;

    #[derive(Debug, Error)]
    pub enum RegistrarError {
        #[error("{0}")]
        UpdateLocations(#[from] cqrs_es::AggregateError<UpdateLocationsError>),
    }
}
