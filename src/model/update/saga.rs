use super::errors::UpdateLocationsError;
use crate::model::update::service::UpdateLocationsServices;
use crate::model::update::{UpdateLocationsCommand, UpdateLocationsEvent};
use crate::model::{AggregateState, LocationZoneCode};
use async_trait::async_trait;
use cqrs_es::Aggregate;
use either::{Either, Left, Right};
use enumflags2::{bitflags, BitFlags};
use once_cell::sync::Lazy;
use postgres_es::PostgresCqrs;
use pretty_snowflake::{Id, Label};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use strum_macros::Display;

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

    #[tracing::instrument(level = "debug")]
    async fn handle(
        &self, command: Self::Command, services: &Self::Services,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        use UpdateLocationsCommand as Cmd;
        use UpdateLocationsEvent as Evt;

        match command {
            Cmd::UpdateLocations(aggregate_id, zones) if !zones.is_empty() => {
                services.add_subscriber(aggregate_id.pretty(), zones.as_slice()).await;
                Ok(vec![Evt::Started(aggregate_id, zones)])
            },

            cmd => Err(Self::Error::RejectedCommand(format!(
                "UpdateLocations saga cannot handle command until starts an update: {cmd:?}"
            ))),
        }
    }

    #[tracing::instrument(level = "debug")]
    fn apply(&self, event: Self::Event) -> Option<Self::State> {
        use UpdateLocationsEvent as Evt;

        match event {
            Evt::Started(aggregate_id, zones) => {
                let location_statuses =
                    zones.into_iter().map(|z| (z, *DEFAULT_LOCATION_UPDATE_STATUS)).collect();
                Some(UpdateLocationsState::Active(ActiveLocationsUpdate {
                    aggregate_id,
                    location_statuses,
                }))
            },

            event => {
                tracing::warn!(
                    ?event,
                    "unrecognized update locations saga event while quiescent -- ignored"
                );
                None
            },
        }
    }
}

pub type LocationUpdatedSteps = BitFlags<LocationUpdatedStep>;

#[bitflags]
#[repr(u8)]
#[derive(Debug, Display, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocationUpdatedStep {
    Observation = 0b0001,
    Forecast = 0b0010,
    Alert = 0b0100,
}

#[derive(Debug, Display, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateCompletionStatus {
    Succeeded,
    Failed,
}

pub static DEFAULT_LOCATION_UPDATE_STATUS: Lazy<LocationUpdateStatus> =
    Lazy::new(|| Left(LocationUpdatedSteps::default()));

pub type LocationUpdateStatus = Either<LocationUpdatedSteps, UpdateCompletionStatus>;

pub trait LocationUpdateStatusExt {
    fn is_active(&self) -> bool;
    fn is_complete(&self) -> bool;
}

impl LocationUpdateStatusExt for LocationUpdateStatus {
    fn is_active(&self) -> bool {
        self.is_left()
    }

    fn is_complete(&self) -> bool {
        self.is_right()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ActiveLocationsUpdate {
    pub aggregate_id: Id<UpdateLocations>,
    pub location_statuses: HashMap<LocationZoneCode, LocationUpdateStatus>,
}

#[async_trait]
impl AggregateState for ActiveLocationsUpdate {
    type State = UpdateLocationsState;
    type Command = <UpdateLocations as Aggregate>::Command;
    type Event = <UpdateLocations as Aggregate>::Event;
    type Error = <UpdateLocations as Aggregate>::Error;
    type Services = <UpdateLocations as Aggregate>::Services;

    #[tracing::instrument(level = "debug")]
    async fn handle(
        &self, command: Self::Command, services: &Self::Services,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        use LocationUpdatedStep as Step;
        use UpdateLocationsCommand as Cmd;

        match command {
            Cmd::UpdateLocations(_, _) => {
                // Ok(vec![Evt::Started(aggregate_id, locations)])
                Err(UpdateLocationsError::RejectedCommand(
                    "UpdateLocations saga a already updating locations".to_string(),
                ))
            },
            Cmd::NoteLocationObservationUpdated(zone) => {
                self.handle_location_update(zone, Step::Observation, services)
            },
            Cmd::NoteLocationForecastUpdated(zone) => {
                self.handle_location_update(zone, Step::Forecast, services)
            },
            Cmd::NoteLocationAlertStatusUpdated(zone) => {
                self.handle_location_update(zone, Step::Alert, services)
            },
            Cmd::NoteLocationUpdateFailure(zone) => self.handle_location_failure(zone, services),
        }
    }

    #[tracing::instrument(level = "debug")]
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

            Evt::Started(_, _) => {
                tracing::warn!(
                    ?event,
                    "unrecognized update locations saga event while active -- ignored"
                );
                None
            },
        }
    }
}

impl ActiveLocationsUpdate {
    // #[tracing::instrument(level = "debug")]
    // async fn start_update_locations(
    //     locations: Vec<LocationZoneIdentifier>,
    // ) -> Result<Vec<UpdateLocationsEvent>, UpdateLocationsError> {
    //     // let actions: Vec<_> = locations
    //     //     .into_iter()
    //     //     .map(|zone| {
    //     //         service.update_zone_weather(&zone)
    //     //             .map(|status| {
    //     //                 let zone_status = match status {
    //     //                     Ok(()) => ZoneUpdateStatus::Started,
    //     //                     Err(error) => {
    //     //                         tracing::warn!(?error, %zone, "failed to initiate zone weather update.");
    //     //                         ZoneUpdateStatus::Failed
    //     //                     },
    //     //                 };
    //     //                 (zone, zone_status)
    //     //             })
    //     //     })
    //     //     .collect();
    //     //
    //     // let updates = futures::future::join_all(actions).await.into_iter().collect();
    //     Ok(vec![UpdateLocationsEvent::Started(locations)])
    // }

    #[tracing::instrument(level = "debug")]
    fn handle_location_update(
        &self, zone: LocationZoneCode, step: LocationUpdatedStep,
        services: &UpdateLocationsServices,
    ) -> Result<Vec<UpdateLocationsEvent>, UpdateLocationsError> {
        use UpdateLocationsEvent as Evt;

        // if previous.is_none() => completed, since no steps
        let previous = self
            .location_statuses
            .get(&zone)
            .unwrap_or(&DEFAULT_LOCATION_UPDATE_STATUS)
            .left();

        let events = match (previous, step) {
            (None, _) => vec![],
            (Some(previous), current) if previous.contains(current) => vec![],
            (Some(mut zone_steps), current) if self.is_only_active_zone(&zone) => {
                zone_steps.toggle(current);
                if zone_steps.is_all() {
                    vec![Evt::LocationUpdated(zone, Left(zone_steps)), Evt::Completed]
                } else {
                    vec![Evt::LocationUpdated(zone, Left(zone_steps))]
                }
            },
            (Some(mut zone_steps), current) => {
                zone_steps.toggle(current);
                if zone_steps.is_all() {
                    vec![Evt::LocationUpdated(zone, Left(zone_steps)), Evt::Completed]
                } else {
                    vec![Evt::LocationUpdated(zone, Left(zone_steps))]
                }
            },
        };

        Ok(events)
    }

    #[tracing::instrument(level = "debug")]
    fn handle_location_failure(
        &self, zone: LocationZoneCode, services: &UpdateLocationsServices,
    ) -> Result<Vec<UpdateLocationsEvent>, UpdateLocationsError> {
        use UpdateLocationsEvent as Evt;

        let previous = self
            .location_statuses
            .get(&zone)
            .unwrap_or(&DEFAULT_LOCATION_UPDATE_STATUS);

        let events = match previous {
            Left(_steps) if self.is_only_active_zone(&zone) => vec![
                Evt::LocationUpdated(zone, Right(UpdateCompletionStatus::Failed)),
                Evt::Failed,
            ],
            Left(_steps) => vec![Evt::LocationUpdated(
                zone,
                Right(UpdateCompletionStatus::Failed),
            )],
            Right(_status) => vec![],
        };

        Ok(events)
    }

    // fn any_status_of(&self, status: ZoneUpdateStatus) -> bool {
    //     self.location_statuses.iter().any(|(_, s)| *s == status)
    // }

    fn is_only_active_zone(&self, zone: &LocationZoneCode) -> bool {
        self.location_statuses
            .get(zone)
            .map(|status| {
                if status.is_active() {
                    let nr_active =
                        self.location_statuses.values().filter(|s| s.is_active()).count();
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

    #[tracing::instrument(level = "debug", skip(self, command, _services))]
    async fn handle(
        &self, command: Self::Command, _services: &Self::Services,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        Err(Self::Error::RejectedCommand(format!(
            "Finished UpdateLocations saga does not handle commands: {command:?}"
        )))
    }

    #[tracing::instrument(level = "debug", skip(self, event))]
    fn apply(&self, event: Self::Event) -> Option<Self::State> {
        tracing::warn!(
            ?event,
            "unrecognized update locations saga event while finished -- ignored"
        );
        None
    }
}
