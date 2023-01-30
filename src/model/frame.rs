use super::{QualityControl, QuantitativeValue};
use geojson::{Feature, FeatureCollection};
use iso8601_timestamp::Timestamp;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::num::TryFromIntError;
use strum::{IntoEnumIterator, VariantNames};
use strum_macros::{Display, EnumIter, EnumString, EnumVariantNames, IntoStaticStr};
use utoipa::ToSchema;

#[derive(Debug, PartialEq, Clone, ToSchema, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherFrame {
    pub timestamp: Timestamp,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<QuantitativeValue>,
    // pub dewpoint: Option<QuantitativeValue>,
    // pub wind_direction: Option<QuantitativeValue>,
    // pub wind_speed: Option<QuantitativeValue>,
    // pub wind_gust: Option<QuantitativeValue>,
    // pub barometric_pressure: Option<QuantitativeValue>,
    // pub sea_level_pressure: Option<QuantitativeValue>,
    // pub visibility: Option<QuantitativeValue>,
    // pub max_temperature_last_24_hours: Option<QuantitativeValue>,
    // pub min_temperature_last_24_hours: Option<QuantitativeValue>,
    // pub precipitation_last_hour: Option<QuantitativeValue>,
    // pub precipitation_last_3_hours: Option<QuantitativeValue>,
    // pub precipitation_last_6_hours: Option<QuantitativeValue>,
    // pub relative_humidity: Option<QuantitativeValue>,
    // pub wind_chill: Option<QuantitativeValue>,
    // pub heat_index: Option<QuantitativeValue>,
}

impl From<FeatureCollection> for WeatherFrame {
    fn from(geojson: FeatureCollection) -> Self {
        geojson
            .features
            .into_iter()
            .fold(PropertyAggregations::new(), fold_feature)
            .into()
    }
}

#[derive(Debug)]
struct PropertyAggregations {
    timestamp: Timestamp,
    properties: HashMap<QuantitativeProperty, QuantitativeAggregation>,
}

impl PropertyAggregations {
    pub fn new() -> Self {
        Self {
            timestamp: Timestamp::now_utc(),
            properties: HashMap::with_capacity(QuantitativeProperty::VARIANTS.len()),
        }
    }

    pub fn property(&self, q_prop: &QuantitativeProperty) -> Option<QuantitativeValue> {
        self.properties.get(q_prop).cloned().map(|v| v.into())
    }
}

impl From<PropertyAggregations> for WeatherFrame {
    fn from(agg: PropertyAggregations) -> Self {
        Self {
            timestamp: agg.timestamp,
            temperature: agg.property(&QuantitativeProperty::Temperature),
        }
    }
}

fn fold_feature(mut acc: PropertyAggregations, feature: Feature) -> PropertyAggregations {
    if feature.properties.is_none() {
        return acc;
    }

    // let acc_props: &mut HashMap<QuantitativeProperty, QuantitativeAggregation> = &mut acc.properties;

    for q_prop in QuantitativeProperty::iter() {
        let prop_name: &'static str = q_prop.into();
        if let Some(property) = feature.property(prop_name) {
            match serde_json::from_value::<PropertyDetail>(property.clone()) {
                Ok(detail) => {
                    acc.properties
                        .entry(q_prop)
                        .and_modify(|prop_agg| prop_agg.add_detail(detail.clone()))
                        .or_insert(QuantitativeAggregation::new(detail));
                },
                Err(err) => {
                    tracing::error!(error=?err, "failed to parse property detail: {property:?}");
                },
            }
        }
    }

    acc
}

#[derive(
    Debug,
    Copy,
    Clone,
    PartialEq,
    Eq,
    Hash,
    Display,
    IntoStaticStr,
    EnumIter,
    EnumString,
    EnumVariantNames,
    ToSchema,
    Serialize,
    Deserialize,
)]
#[strum(serialize_all = "camelCase", ascii_case_insensitive)]
pub enum QuantitativeProperty {
    Temperature,
    // Dewpoint,
    // WindDirection,
    // WindSpeed,
    // WindGust,
    // BarometricPressure,
    // SeaLevelPressure,
    // Visibility,
    // MaxTemperatureLast24Hours,
    // MinTemperatureLast24Hours,
    // PrecipitationLastHour,
    // PrecipitationLast3Hours,
    // PrecipitationLast6Hours,
    // RelativeHumidity,
    // WindChill,
    // HeatIndex,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PropertyDetail {
    value: f32,
    unit_code: String,
    quality_control: QualityControl,
}

#[derive(Debug, Clone, PartialEq)]
struct QuantitativeAggregation {
    count: usize,
    value_sum: f32,
    max_value: f32,
    min_value: f32,
    pub unit_code: Cow<'static, str>,
    pub quality_control: QualityControl,
}

impl QuantitativeAggregation {
    pub fn new(detail: PropertyDetail) -> Self {
        Self {
            count: 1,
            value_sum: detail.value,
            max_value: detail.value,
            min_value: detail.value,
            unit_code: detail.unit_code.into(),
            quality_control: detail.quality_control,
        }
    }

    pub fn average_value(&self) -> f32 {
        self.value_sum / try_usize_to_f32(self.count).unwrap_or(f32::MAX)
    }

    pub fn add_detail(&mut self, detail: PropertyDetail) {
        // combination strategy:
        // lessor rhs quality control => ignore value
        // same rhs quality control => combine avg and min/max
        // higher rhs quality control => reset aggregation with rhs

        match self.quality_control.cmp(&detail.quality_control) {
            Ordering::Less => (),

            Ordering::Greater => {
                self.count = 1;
                self.value_sum = detail.value;
                self.max_value = detail.value;
                self.min_value = detail.value;
                self.quality_control = detail.quality_control;
            },

            Ordering::Equal => {
                self.count += 1;
                self.value_sum += detail.value;
                self.max_value = detail.value.max(self.max_value);
                self.min_value = detail.value.min(self.min_value);
            },
        }
    }
}

#[inline]
fn try_usize_to_f32(value: usize) -> Result<f32, TryFromIntError> {
    u16::try_from(value).map(f32::from)
}

// impl std::ops::Add<PropertyDetail> for QuantitativeAggregation {
//     type Output = Self;
//
//     fn add(self, rhs: PropertyDetail) -> Self::Output {
//         // combination strategy:
//         // lessor rhs quality control => ignore value
//         // same rhs quality control => combine avg and min/max
//         // higher rhs quality control => reset aggregation with rhs
//
//         match self.quality_control.cmp(&rhs.quality_control) {
//             Ordering::Less => self,
//             Ordering::Greater => Self {
//                 count: 1,
//                 value_sum: rhs.value,
//                 max_value: rhs.value,
//                 min_value: rhs.value,
//                 quality_control: rhs.quality_control,
//                 ..self
//             },
//             Ordering::Equal => Self {
//                 count: self.count + 1,
//                 value_sum: self.value_sum + rhs.value,
//                 max_value: self.max_value.max(rhs.value),
//                 min_value: self.min_value.min(rhs.value),
//                 ..self
//             },
//         }
//     }
// }

impl From<QuantitativeAggregation> for QuantitativeValue {
    fn from(agg: QuantitativeAggregation) -> Self {
        Self {
            value: agg.average_value(),
            max_value: agg.max_value,
            min_value: agg.min_value,
            unit_code: agg.unit_code,
            quality_control: agg.quality_control,
        }
    }
}
