pub mod registrar;

pub use registrar::RegistrarAggregate;

use std::borrow::{Borrow, Cow};
use serde::{Deserialize, Serialize};
use iso8601_timestamp::Timestamp;
use uom::si::f32::*;
use uom::si::ratio::ratio;
use uom::si::thermodynamic_temperature::degree_fahrenheit;
use utoipa::ToSchema;
use strum::{Display, EnumString, EnumVariantNames};


#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Display, EnumString, EnumVariantNames, ToSchema, Serialize, Deserialize)]
#[strum(serialize_all="PascalCase", ascii_case_insensitive)]
pub enum Location {
    Chicago,
    Seattle,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, ToSchema, Serialize, Deserialize)]
#[schema(example = json!("WAZ558"))]
#[serde(transparent)]
#[repr(transparent)]
pub struct LocationZoneCode(String);

#[derive(Debug, PartialEq, Clone, ToSchema, Serialize, Deserialize)]
#[serde(rename_all="camelCase")]
pub struct QuantitativeValue {
    pub value: f32,
    pub max_value: f32,
    pub min_value: f32,
    pub unit_code: Cow<'static, str>,
    pub quality_control: QualityControl,
}

impl QuantitativeValue {
    pub fn new(value: f32, min_value: f32, max_value: f32, unit_code: &str, quality_control: QualityControl,) -> Self {
        Self { value, max_value, min_value, unit_code: unit_code.into(), quality_control, }
    }

    pub fn unit_code(&self) -> &str {
        self.unit_code.borrow()
    }
}

#[derive(Debug, Display, Copy, Clone, PartialEq, Eq, Hash, EnumString, EnumVariantNames, ToSchema, Serialize, Deserialize)]
#[strum(serialize_all="UPPERCASE")]
pub enum QualityControl {
    Z, C, S, V, X, Q, G, B, T,
}

#[derive(Debug, PartialEq, Clone, ToSchema, Serialize)]
#[serde(rename_all="camelCase")]
pub struct LocationWeather {
    pub timestamp: Timestamp,
    pub temperature: QuantitativeValue,
    pub dewpoint: QuantitativeValue,
    pub wind_direction: QuantitativeValue,
    pub wind_speed: QuantitativeValue,
    pub wind_gust: QuantitativeValue,
    pub barometric_pressure: QuantitativeValue,
    pub sea_level_pressure: QuantitativeValue,
    pub visibility: QuantitativeValue,
    pub max_temperature_last_24_hours: QuantitativeValue,
    pub min_temperature_last_24_hours: QuantitativeValue,
    pub precipitation_last_hour: QuantitativeValue,
    pub precipitation_last_3_hours: QuantitativeValue,
    pub precipitation_last_6_hours: QuantitativeValue,
    pub relative_humidity: QuantitativeValue,
    pub wind_chill: QuantitativeValue,
    pub heat_index: QuantitativeValue,
}
