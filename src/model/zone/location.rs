use crate::model::zone::errors::LocationZoneError;
use crate::model::zone::service::LocationServices;
use crate::model::zone::{LocationZoneCommand, LocationZoneEvent};
use crate::model::{
    AggregateState, LocationZoneCode, LocationZoneType, WeatherFrame, ZoneForecast,
};
use crate::services::noaa::ZoneWeatherApi;
use async_trait::async_trait;
use cqrs_es::Aggregate;
use postgres_es::PostgresCqrs;
use pretty_snowflake::{Id, Label};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub type LocationZoneAggregate = Arc<PostgresCqrs<LocationZone>>;

pub const AGGREGATE_TYPE: &str = "location_zone";

#[inline]
pub fn generate_id() -> Id<LocationZone> {
    pretty_snowflake::generator::next_id()
}

#[derive(Debug, Default, Clone, Label, PartialEq, Serialize, Deserialize)]
pub struct LocationZone {
    state: LocationZoneState,
}

#[async_trait]
impl Aggregate for LocationZone {
    type Command = LocationZoneCommand;
    type Event = LocationZoneEvent;
    type Error = LocationZoneError;
    type Services = LocationServices;

    fn aggregate_type() -> String {
        AGGREGATE_TYPE.to_string()
    }

    #[tracing::instrument(level = "trace", skip())]
    async fn handle(
        &self, command: Self::Command, services: &Self::Services,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        self.state.handle(command, services).await
    }

    fn apply(&mut self, event: Self::Event) {
        if let Some(new_state) = self.state.apply(event) {
            self.state = new_state;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum LocationZoneState {
    Quiescent(QuiescentLocationZone),
    Active(ActiveLocationZone),
}

impl Default for LocationZoneState {
    fn default() -> Self {
        Self::Quiescent(QuiescentLocationZone::default())
    }
}

#[async_trait]
impl AggregateState for LocationZoneState {
    type State = Self;
    type Command = <LocationZone as Aggregate>::Command;
    type Event = <LocationZone as Aggregate>::Event;
    type Error = <LocationZone as Aggregate>::Error;
    type Services = <LocationZone as Aggregate>::Services;

    async fn handle(
        &self, command: Self::Command, services: &Self::Services,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        match self {
            Self::Quiescent(state) => state.handle(command, services).await,
            Self::Active(state) => state.handle(command, services).await,
        }
    }

    fn apply(&self, event: Self::Event) -> Option<Self::State> {
        match self {
            Self::Quiescent(state) => state.apply(event),
            Self::Active(state) => state.apply(event),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
struct QuiescentLocationZone;

#[async_trait]
impl AggregateState for QuiescentLocationZone {
    type State = LocationZoneState;
    type Command = <LocationZone as Aggregate>::Command;
    type Event = <LocationZone as Aggregate>::Event;
    type Error = <LocationZone as Aggregate>::Error;
    type Services = <LocationZone as Aggregate>::Services;

    #[tracing::instrument(level = "trace")]
    async fn handle(
        &self, command: Self::Command, _services: &Self::Services,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            LocationZoneCommand::WatchZone(zone_type, zone_code) => {
                Ok(vec![LocationZoneEvent::ZoneSet(zone_type, zone_code)])
            },

            cmd => Err(LocationZoneError::RejectedCommand(format!(
                "LocationZone cannot handle command until it targets a zone: {cmd:?}"
            ))),
        }
    }

    #[tracing::instrument(level = "trace")]
    fn apply(&self, event: Self::Event) -> Option<Self::State> {
        match event {
            LocationZoneEvent::ZoneSet(zone_type, zone_code) => {
                Some(Self::State::Active(ActiveLocationZone {
                    zone_type,
                    zone_id: zone_code,
                    weather: None,
                    forecast: None,
                    active_alert: false,
                }))
            },

            event => {
                tracing::warn!(?event, "invalid quiescent location zone event -- ignored");
                None
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ActiveLocationZone {
    pub zone_type: LocationZoneType,
    pub zone_id: LocationZoneCode,
    pub weather: Option<WeatherFrame>,
    pub forecast: Option<ZoneForecast>,
    pub active_alert: bool,
}

#[async_trait]
impl AggregateState for ActiveLocationZone {
    type State = LocationZoneState;
    type Command = <LocationZone as Aggregate>::Command;
    type Event = <LocationZone as Aggregate>::Event;
    type Error = <LocationZone as Aggregate>::Error;
    type Services = <LocationZone as Aggregate>::Services;

    #[tracing::instrument(level = "trace")]
    async fn handle(
        &self, command: Self::Command, services: &Self::Services,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            LocationZoneCommand::Observe => {
                let frame = services.zone_observation(&self.zone_id).await?;
                Ok(vec![LocationZoneEvent::ObservationAdded(frame)])
            },

            LocationZoneCommand::Forecast => {
                let forecast = services.zone_forecast(self.zone_type, &self.zone_id).await?;
                Ok(vec![LocationZoneEvent::ForecastUpdated(forecast)])
            },

            LocationZoneCommand::NoteAlert(alert) => {
                let event = match (self.active_alert, alert) {
                    (false, Some(alert)) => Some(LocationZoneEvent::AlertActivated(alert)),
                    (true, None) => Some(LocationZoneEvent::AlertDeactivated),
                    _ => None,
                };

                Ok(event.into_iter().collect())
            },

            LocationZoneCommand::WatchZone(new_zone_type, new_zone_code) => Err(LocationZoneError::RejectedCommand(format!(
                "LocationZone already watching zone, {}, cannot change to watch: {new_zone_type}/{new_zone_code}",
                self.zone_id
            ))),
        }
    }

    #[tracing::instrument(level = "trace")]
    fn apply(&self, event: Self::Event) -> Option<Self::State> {
        match event {
            LocationZoneEvent::ObservationAdded(frame) => Some(LocationZoneState::Active(Self {
                weather: Some(frame),
                ..self.clone()
            })),

            LocationZoneEvent::ForecastUpdated(forecast) => Some(LocationZoneState::Active(Self {
                forecast: Some(forecast),
                ..self.clone()
            })),

            LocationZoneEvent::AlertActivated(_) => Some(LocationZoneState::Active(Self {
                active_alert: true,
                ..self.clone()
            })),

            LocationZoneEvent::AlertDeactivated => Some(LocationZoneState::Active(Self {
                active_alert: false,
                ..self.clone()
            })),

            event => {
                tracing::warn!(?event, "invalid active location zone event -- ignored");
                None
            },
        }
    }
}
