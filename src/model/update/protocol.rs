use crate::model::{LocationZoneIdentifier, ZoneUpdateStatus};
use cqrs_es::DomainEvent;
use serde::{Deserialize, Serialize};
use strum::Display;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateLocationsCommand {
    UpdateLocations(Vec<LocationZoneIdentifier>),
    NoteLocationUpdate(LocationZoneIdentifier, ZoneUpdateStatus),
}

const VERSION: &str = "1.0";

#[derive(Debug, Display, Clone, PartialEq, Serialize, Deserialize)]
#[strum(serialize_all = "snake_case")]
pub enum UpdateLocationsEvent {
    Started(Vec<LocationZoneIdentifier>),
    LocationUpdated(LocationZoneIdentifier, ZoneUpdateStatus),
    Completed,
    Failed,
}

impl DomainEvent for UpdateLocationsEvent {
    fn event_type(&self) -> String {
        self.to_string()
    }

    fn event_version(&self) -> String {
        VERSION.to_string()
    }
}
