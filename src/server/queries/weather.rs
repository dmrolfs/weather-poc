use crate::model::{QuantitativeValue, WeatherFrame};
use cqrs_es::persist::GenericQuery;
use cqrs_es::{EventEnvelope, View};
use iso8601_timestamp::Timestamp;
use postgres_es::PostgresViewRepository;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

pub type WeatherViewRepository = PostgresViewRepository<WeatherView, LocationZone>;
pub type WeatherViewProjection = Arc<WeatherViewRepository>;

pub type WeatherQuery = GenericQuery<WeatherViewRepository, WeatherView, LocationZone>;

#[derive(Debug, Clone, PartialEq, ToSchema, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherView {
    pub timestamp: Timestamp,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<QuantitativeValue>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dewpoint: Option<QuantitativeValue>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_direction: Option<QuantitativeValue>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_speed: Option<QuantitativeValue>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_gust: Option<QuantitativeValue>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub barometric_pressure: Option<QuantitativeValue>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sea_level_pressure: Option<QuantitativeValue>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<QuantitativeValue>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_temperature_last_24_hours: Option<QuantitativeValue>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_temperature_last_24_hours: Option<QuantitativeValue>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precipitation_last_hour: Option<QuantitativeValue>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precipitation_last_3_hours: Option<QuantitativeValue>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precipitation_last_6_hours: Option<QuantitativeValue>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_humidity: Option<QuantitativeValue>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_chill: Option<QuantitativeValue>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heat_index: Option<QuantitativeValue>,
}

impl Default for WeatherView {
    fn default() -> Self {
        Self {
            timestamp: Timestamp::now_utc(),
            temperature: None,
            dewpoint: None,
            wind_direction: None,
            wind_speed: None,
            wind_gust: None,
            barometric_pressure: None,
            sea_level_pressure: None,
            visibility: None,
            max_temperature_last_24_hours: None,
            min_temperature_last_24_hours: None,
            precipitation_last_hour: None,
            precipitation_last_3_hours: None,
            precipitation_last_6_hours: None,
            relative_humidity: None,
            wind_chill: None,
            heat_index: None,
        }
    }
}

impl From<WeatherFrame> for WeatherView {
    fn from(value: WeatherFrame) -> Self {
        Self {
            timestamp: value.timestamp,
            temperature: Some(value.temperature),
            ..Default::default() // dewpoint: Some(value.dewpoint),
                                 // wind_direction: Some(value.wind_direction),
                                 // wind_speed: Some(value.wind_speed),
                                 // wind_gust: Some(value.wind_gust),
                                 // barometric_pressure: Some(value.barometric_pressure),
                                 // sea_level_pressure: Some(value.sea_level_pressure),
                                 // visibility: Some(value.visibility),
                                 // max_temperature_last_24_hours: Some(value.max_temperature_last_24_hours),
                                 // min_temperature_last_24_hours: Some(value.min_temperature_last_24_hours),
                                 // precipitation_last_hour: Some(value.precipitation_last_hour),
                                 // precipitation_last_3_hours: Some(value.precipitation_last_3_hours),
                                 // precipitation_last_6_hours: Some(value.precipitation_last_6_hours),
                                 // relative_humidity: Some(value.relative_humidity),
                                 // wind_chill: Some(value.wind_chill),
                                 // heat_index: Some(value.heat_index),
        }
    }
}

impl View<LocationZone> for WeatherView {
    fn update(&mut self, event: &EventEnvelope<LocationZone>) {
        match &event.payload {}
    }
}
