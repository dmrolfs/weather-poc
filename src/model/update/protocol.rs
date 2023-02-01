use crate::model::update::saga::LocationUpdateStatus;
use crate::model::{EventEnvelope, LocationZone, LocationZoneCode, UpdateLocations};
use cqrs_es::DomainEvent;
use pretty_snowflake::Id;
use serde::{Deserialize, Serialize};
use strum_macros::Display;

pub fn location_event_to_command(
    envelope: EventEnvelope<LocationZone>,
) -> Vec<UpdateLocationsCommand> {
    use crate::model::zone::LocationZoneEvent as ZoneEvent;
    use UpdateLocationsCommand as C;

    let zone = LocationZoneCode::new(envelope.publisher_id());
    match envelope.payload() {
        ZoneEvent::ObservationAdded(_) => vec![C::NoteLocationObservationUpdated(zone)],
        ZoneEvent::ForecastUpdated(_) => vec![C::NoteLocationForecastUpdated(zone)],
        ZoneEvent::AlertDeactivated | ZoneEvent::AlertActivated(_) => {
            vec![C::NoteLocationAlertStatusUpdated(zone)]
        },
        _ => vec![],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateLocationsCommand {
    UpdateLocations(Id<UpdateLocations>, Vec<LocationZoneCode>),
    NoteLocationObservationUpdated(LocationZoneCode),
    NoteLocationForecastUpdated(LocationZoneCode),
    NoteLocationAlertStatusUpdated(LocationZoneCode),
    NoteLocationUpdateFailure(LocationZoneCode),
}

const VERSION: &str = "1.0";

#[derive(Debug, Display, Clone, PartialEq, Serialize, Deserialize)]
#[strum(serialize_all = "snake_case")]
pub enum UpdateLocationsEvent {
    Started(Id<UpdateLocations>, Vec<LocationZoneCode>),
    LocationUpdated(LocationZoneCode, LocationUpdateStatus),
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
