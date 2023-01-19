pub use errors::RegistrarError;
pub use protocol::{RegistrarCommand, RegistrarEvent};
pub use service::{HappyPathServices, RegistrarServices};

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
    use super::RegistrarError;
    use crate::model::registrar::RegistrarEvent;
    use crate::model::{Location, LocationZoneCode};
    use async_trait::async_trait;
    use chrono::format::Item;

    #[async_trait]
    pub trait RegistrarApi: Sync + Send {
        async fn update_weather(
            &self, zones: &[(&Location, &LocationZoneCode)],
        ) -> Result<Vec<RegistrarEvent>, RegistrarError>;
    }

    #[derive(Debug, Clone)]
    pub enum RegistrarServices {
        // AggregatePath(AggregatePathRegistrarServices),
        HappyPath(HappyPathServices),
    }

    #[async_trait]
    impl RegistrarApi for RegistrarServices {
        async fn update_weather(
            &self, zones: &[(&Location, &LocationZoneCode)],
        ) -> Result<Vec<RegistrarEvent>, RegistrarError> {
            match self {
                // Self::AggregatePath(svc) => svc.update_weather(zones).await,
                Self::HappyPath(svc) => svc.update_weather(zones).await,
            }
        }
    }

    // #[derive(Debug, Copy, Clone)]
    // pub struct AggregatePathRegistrarServices;
    //
    // #[async_trait]
    // impl RegistrarApi for AggregatePathRegistrarServices {
    //     async fn update_weather(&self, zones: &[LocationZone]) -> Result<Vec<RegistrarEvent>, RegistrarServiceError> {
    //         let aggregate_id = update_weather::generate_id();
    //         todo!()
    //     }
    // }

    #[derive(Debug, Copy, Clone)]
    pub struct HappyPathServices;

    #[async_trait]
    impl RegistrarApi for HappyPathServices {
        async fn update_weather(
            &self, zones: &[(&Location, &LocationZoneCode)],
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
    use crate::model::{Location, LocationZoneCode};
    use cqrs_es::DomainEvent;
    use serde::{Deserialize, Serialize};
    use strum::Display;

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
    use std::fmt;

    #[derive(Debug)]
    pub struct RegistrarError;

    impl fmt::Display for RegistrarError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", "RegistrarError")
        }
    }

    impl std::error::Error for RegistrarError {}
}
