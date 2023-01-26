use super::errors::UpdateLocationsError;
use crate::model::update::service::UpdateLocationsServices;
use crate::model::update::{UpdateLocationsCommand, UpdateLocationsEvent};
use crate::model::{AggregateState, LocationZoneIdentifier, ZoneUpdateStatus};
use async_trait::async_trait;
use cqrs_es::Aggregate;
use postgres_es::PostgresCqrs;
use pretty_snowflake::{Id, Label};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

pub type UpdateLocationsSaga = Arc<PostgresCqrs<UpdateLocations>>;

pub const AGGREGATE_TYPE: &str = "update_locations";

#[inline]
pub fn generate_id() -> Id<UpdateLocations> {
    pretty_snowflake::generator::next_id()
}

#[derive(Debug, Default, Clone, Label, PartialEq, Serialize, Deserialize)]
pub struct UpdateLocations {
    state: UpdateLocationsState,
}

// impl Default for UpdateLocations {
//     fn default() -> Self {
//         Self { state: Arc::new(Mutex::new(UpdateLocationsState::default())) }
//     }
// }

#[async_trait]
impl Aggregate for UpdateLocations {
    type Command = UpdateLocationsCommand;
    type Event = UpdateLocationsEvent;
    type Error = UpdateLocationsError;
    type Services = UpdateLocationsServices;

    fn aggregate_type() -> String {
        AGGREGATE_TYPE.to_string()
    }

    async fn handle(
        &self, command: Self::Command, service: &Self::Services,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        self.state.handle(command, service).await
    }

    fn apply(&mut self, event: Self::Event) {
        if let Some(new_state) = self.state.apply(event) {
            self.state = new_state;
        }
    }
}


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum UpdateLocationsState {
    Quiescent(QuiescentLocationsUpdate),
    Active(ActiveLocationsUpdate),
    Finished(FinishedLocationsUpdate),
}

impl Default for UpdateLocationsState {
    fn default() -> Self {
        Self::Quiescent(QuiescentLocationsUpdate::default())
    }
}

#[async_trait]
impl AggregateState for UpdateLocationsState {
    type State = Self;
    type Command = <UpdateLocations as Aggregate>::Command;
    type Event = <UpdateLocations as Aggregate>::Event;
    type Error = <UpdateLocations as Aggregate>::Error;
    type Services = <UpdateLocations as Aggregate>::Services;

    async fn handle(
        &self, command: Self::Command, services: &Self::Services,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        match self {
            Self::Quiescent(state) => state.handle(command, services).await,
            Self::Active(state) => state.handle(command, services).await,
            Self::Finished(state) => state.handle(command, services).await,
        }
    }

    fn apply(&self, event: Self::Event) -> Option<Self::State> {
        match self {
            Self::Quiescent(state) => state.apply(event),
            Self::Active(state) => state.apply(event),
            Self::Finished(state) => state.apply(event),
        }
    }
}

// -- Quiescent

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
struct QuiescentLocationsUpdate;

#[async_trait]
impl AggregateState for QuiescentLocationsUpdate {
    type State = UpdateLocationsState;
    type Command = <UpdateLocations as Aggregate>::Command;
    type Event = <UpdateLocations as Aggregate>::Event;
    type Error = <UpdateLocations as Aggregate>::Error;
    type Services = <UpdateLocations as Aggregate>::Services;

    async fn handle(
        &self, command: Self::Command, _services: &Self::Services,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            Self::Command::UpdateLocations(zones)  if !zones.is_empty() => {
                Ok(vec![Self::Event::Started(zones)])
            },

            cmd => Err(Self::Error::RejectedCommand(format!(
                "UpdateLocations saga cannot handle command until starts an update: {cmd:?}"
            ))),
        }
    }

    fn apply(&self, event: Self::Event) -> Option<Self::State> {
        match event {
            Self::Event::Started(zones) => {
                let location_statuses =
                    zones.into_iter().map(|z| (z, ZoneUpdateStatus::Started)).collect();
                Some(Self::State::Active(ActiveLocationsUpdate {
                    location_statuses,
                }))
            },

            event => {
                tracing::warn!(?event, "unrecognized update locations saga event while quiescent -- ignored");
                None
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ActiveLocationsUpdate {
    pub location_statuses: HashMap<LocationZoneIdentifier, ZoneUpdateStatus>,
}

#[async_trait]
impl AggregateState for ActiveLocationsUpdate {
    type State = UpdateLocationsState;
    type Command = <UpdateLocations as Aggregate>::Command;
    type Event = <UpdateLocations as Aggregate>::Event;
    type Error = <UpdateLocations as Aggregate>::Error;
    type Services = <UpdateLocations as Aggregate>::Services;

    async fn handle(
        &self, command: Self::Command, _services: &Self::Services,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        use UpdateLocationsCommand as Cmd;
        use UpdateLocationsEvent as Evt;

        match command {
            Cmd::UpdateLocations(locations) => Ok(vec![Evt::Started(locations)]),
            Cmd::NoteLocationUpdate(zone, status) => self.handle_location_update(zone, status),
        }
    }

    fn apply(&self, event: Self::Event) -> Option<Self::State> {
        use UpdateLocationsEvent as Evt;

        match event {
            Evt::LocationUpdated(zone, status) => {
                let mut new_state = self.clone();

                if let Some(previous) = new_state.location_statuses.insert(zone.clone(), status) {
                    tracing::info!(%zone, "updated location zone status: {previous} => {status}");
                }

               Some(Self::State::Active(new_state))
            },

            Evt::Completed | Evt::Failed => Some(Self::State::Finished(FinishedLocationsUpdate)),

            Evt::Started(_) => {
                tracing::warn!(?event, "unrecognized update locations saga event while active -- ignored");
                None
            }
        }
    }
}

impl ActiveLocationsUpdate {
    #[tracing::instrument(level = "debug")]
    async fn start_update_locations(locations: Vec<LocationZoneIdentifier>) -> Result<Vec<UpdateLocationsEvent>, UpdateLocationsError> {
        // let actions: Vec<_> = locations
        //     .into_iter()
        //     .map(|zone| {
        //         service.update_zone_weather(&zone)
        //             .map(|status| {
        //                 let zone_status = match status {
        //                     Ok(()) => ZoneUpdateStatus::Started,
        //                     Err(error) => {
        //                         tracing::warn!(?error, %zone, "failed to initiate zone weather update.");
        //                         ZoneUpdateStatus::Failed
        //                     },
        //                 };
        //                 (zone, zone_status)
        //             })
        //     })
        //     .collect();
        //
        // let updates = futures::future::join_all(actions).await.into_iter().collect();
        Ok(vec![UpdateLocationsEvent::Started(locations)])
    }

    #[tracing::instrument(level = "debug")]
    fn handle_location_update(
        &self, zone: LocationZoneIdentifier, status: ZoneUpdateStatus,
    ) -> Result<Vec<UpdateLocationsEvent>, UpdateLocationsError> {
        use UpdateLocationsEvent as Evt;

        let previous = self.location_statuses.get(&zone);

        let events = match (previous, &status) {
            // zone with same status means no change
            (Some(previous), current) if previous == current => {
                vec![]
            },

            (Some(_), current) if self.is_only_active_zone(&zone) => {
                let mut events = vec![Evt::LocationUpdated(zone, status)];

                if current != &ZoneUpdateStatus::Failed && !self.any_status_of(ZoneUpdateStatus::Failed) {
                    events.push(Evt::Completed);
                } else {
                    events.push(Evt::Failed);
                }

                events
            },

            // since zone wasn't in state before, a possible completion doesn't impact outstanding.
            (None, _) => {
                vec![Evt::LocationUpdated(zone, status)]
            },

            (Some(_), _) => {
                vec![Evt::LocationUpdated(zone, status)]
            },
        };

        Ok(events)
    }

    fn any_status_of(&self, status: ZoneUpdateStatus) -> bool {
        self.location_statuses.iter().find(|(_, s)| *s == &status).is_some()
    }

    fn is_only_active_zone(&self, zone: &LocationZoneIdentifier) -> bool {
        self.location_statuses
            .get(zone)
            .map(|status| {
                if status.is_active() {
                    let nr_active = self.location_statuses.values().filter(|s| s.is_active()).count();
                    nr_active == 1
                } else {
                    false
                }
            })
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct FinishedLocationsUpdate;

#[async_trait]
impl AggregateState for FinishedLocationsUpdate {
    type State = UpdateLocationsState;
    type Command = <UpdateLocations as Aggregate>::Command;
    type Event = <UpdateLocations as Aggregate>::Event;
    type Error = <UpdateLocations as Aggregate>::Error;
    type Services = <UpdateLocations as Aggregate>::Services;

    async fn handle(
        &self, command: Self::Command, _services: &Self::Services,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        Err(Self::Error::RejectedCommand(format!("Finished UpdateLocations saga does not handle commands: {command:?}")))
    }

    fn apply(&self, event: Self::Event) -> Option<Self::State> {
        tracing::warn!(?event, "unrecognized update locations saga event while finished -- ignored");
        None
    }
}
