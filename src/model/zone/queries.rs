use crate::model::{ForecastDetail, LocationZone, WeatherAlert, WeatherFrame};
use cqrs_es::persist::GenericQuery;
use cqrs_es::{EventEnvelope, View};
use iso8601_timestamp::Timestamp;
use postgres_es::PostgresViewRepository;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

pub const WEATHER_QUERY_VIEW: &str = "weather_query";

pub type WeatherViewRepository = PostgresViewRepository<WeatherView, LocationZone>;
pub type WeatherViewProjection = Arc<WeatherViewRepository>;

pub type WeatherQuery = GenericQuery<WeatherViewRepository, WeatherView, LocationZone>;

#[derive(Debug, Clone, PartialEq, ToSchema, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherView {
    pub zone_code: String,

    pub timestamp: Timestamp,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert: Option<WeatherAlert>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<WeatherFrame>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forecast: Vec<ForecastDetail>,
}

impl Default for WeatherView {
    fn default() -> Self {
        Self {
            zone_code: String::default(),
            timestamp: Timestamp::now_utc(),
            alert: None,
            current: None,
            forecast: Vec::new(),
        }
    }
}

impl WeatherView {
    pub fn new(zone_code: impl Into<String>) -> Self {
        Self {
            zone_code: zone_code.into(),
            timestamp: Timestamp::now_utc(),
            ..Default::default()
        }
    }
}

// Updates the CQRS view from events as they are committed
impl View<LocationZone> for WeatherView {
    fn update(&mut self, event: &EventEnvelope<LocationZone>) {
        use super::LocationZoneEvent as Evt;

        match &event.payload {
            Evt::ZoneSet(zone_id) => {
                self.zone_code = zone_id.to_string();
            },

            Evt::ObservationAdded(frame) => {
                self.current = Some(*frame.clone());
            },

            Evt::ForecastUpdated(forecast) => {
                self.forecast = forecast.periods.clone();
            },

            Evt::AlertActivated(alert) => {
                self.alert = Some(alert.clone());
            },

            Evt::AlertDeactivated => {
                self.alert = None;
            },
        }
    }
}
