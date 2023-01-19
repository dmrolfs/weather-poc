use crate::model::{QualityControl, QuantitativeValue, WeatherFrame};
use geojson::feature::Id;
use geojson::{Feature, FeatureCollection, GeoJson, Geometry, Value};
use iso8601_timestamp::Timestamp;
use itertools::min;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use strum::{Display, EnumString, EnumVariantNames};
use utoipa::ToSchema;

#[derive(Debug, Serialize)]
pub struct Observation {
    pub id: String,
    pub features: FeatureCollection,
}

impl From<Observation> for WeatherFrame {
    fn from(observation: Observation) -> Self {
        observation
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

    for p_name in QuantitativeProperty::VARIANTS {
        if let Some(property) = feature.property(p_name) {
            let detail: PropertyDetail = match serde_json::from_value(property.clone()) {
                Ok(d) => d,
                Err(err) => {
                    tracing::error!(error=?err, "failed to parse property detail: {property:?}");
                    continue;
                },
            };

            acc += detail;
        }
    }

    acc
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PropertyDetail {
    value: f32,
    unit_code: String,
    quality_control: QualityControl,
}

#[derive(Debug, PartialEq)]
struct QuantitativeAggregation {
    count: usize,
    value_sum: f32,
    max_value: f32,
    min_value: f32,
    pub unit_code: Cow<'static, str>,
    pub quality_control: QualityControl,
}

impl QuantitativeAggregation {
    pub fn new(unit_code: Cow<'static, str>, quality_control: QualityControl) -> Self {
        Self {
            count: 0,
            value_sum: 0.0,
            max_value: 0.0,
            min_value: 0.0,
            unit_code,
            quality_control,
        }
    }
}

impl std::ops::Add<PropertyDetail> for QuantitativeAggregation {
    type Output = Self;

    fn add(self, rhs: PropertyDetail) -> Self::Output {
        // combination strategy:
        // lessor rhs quality control => ignore value
        // same rhs quality control => combine avg and min/max
        // higher rhs quality control => reset aggregation with rhs

        match self.quality_control.cmp(&rhs.quality_control) {
            Ordering::Less => self,
            Ordering::Greater => Self {
                count: 1,
                value_sum: rhs.value,
                max_value: rhs.value,
                min_value: rhs.value,
                quality_control: rhs.quality_control,
                ..self
            },
            Ordering::Equal => Self {
                count: self.count + 1,
                value_sum: self.value_sum + rhs.value,
                max_value: self.max_value.max(rhs.value),
                min_value: self.min_value.min(rhs.value),
                ..self
            },
        }
    }
}

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

#[derive(
    Debug,
    Copy,
    Clone,
    PartialEq,
    Eq,
    Hash,
    Display,
    EnumString,
    EnumVariantNames,
    ToSchema,
    Serialize,
    Deserialize,
)]
#[strum(serialize_all = "camelCase", ascii_case_insensitive)]
pub enum DescriptiveProperty {
    // Id,
    // Type,
    // Elevation,
    // Station,
    Timestamp,
    // RawMessage,
    // TextDescription,
    // Icon,
    // PresentWeather,
    // CloudLayers,
}

#[derive(
    Debug,
    Copy,
    Clone,
    PartialEq,
    Eq,
    Hash,
    Display,
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
