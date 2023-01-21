use crate::model::{LocationZoneCode, WeatherFrame, ZoneForecast};
use cqrs_es::DomainEvent;
use serde::{Deserialize, Serialize};
use strum::Display;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocationZoneCommand {
    WatchZone(LocationZoneCode),
    Observe,
    Forecast,
    NoteAlertStatus(bool),
}

const VERSION: &str = "1.0";

#[derive(Debug, Display, Clone, PartialEq, Serialize, Deserialize)]
#[strum(serialize_all = "snake_case")]
pub enum LocationZoneEvent {
    ZoneSet(LocationZoneCode),
    ObservationAdded(WeatherFrame),
    ForecastUpdated(ZoneForecast),
    AlertActivated,
    AlertDeactivated,
}

impl DomainEvent for LocationZoneEvent {
    fn event_type(&self) -> String {
        self.to_string()
    }

    fn event_version(&self) -> String {
        VERSION.to_string()
    }
}
