use crate::model::{LocationZoneCode, WeatherAlert, WeatherFrame, ZoneForecast};
use cqrs_es::DomainEvent;
use serde::{Deserialize, Serialize};
use strum_macros::Display;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LocationZoneCommand {
    WatchZone(LocationZoneCode),
    Observe,
    Forecast,
    NoteAlert(Option<WeatherAlert>),
}

const VERSION: &str = "1.0";

#[derive(Debug, Display, Clone, PartialEq, Serialize, Deserialize)]
#[strum(serialize_all = "snake_case")]
pub enum LocationZoneEvent {
    ZoneSet(LocationZoneCode),
    ObservationAdded(Box<WeatherFrame>),
    ForecastUpdated(ZoneForecast),
    AlertActivated(WeatherAlert),
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
