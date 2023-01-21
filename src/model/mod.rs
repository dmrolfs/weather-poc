mod frame;
pub mod registrar;
pub mod zone;

pub use frame::WeatherFrame;
pub use registrar::{Registrar, RegistrarAggregate};
pub use zone::{LocationZone, LocationZoneAggregate};

use crate::errors::WeatherError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use geojson::Feature;
use serde::{Deserialize, Serialize};
use std::borrow::{Borrow, Cow};
use std::cmp::Ordering;
use strum::{Display, EnumMessage, EnumString, EnumVariantNames};
use utoipa::ToSchema;

#[async_trait]
pub trait AggregateState {
    type State;

    type Command;
    type Event: cqrs_es::DomainEvent;
    type Error: std::error::Error;
    type Services: Send + Sync;

    async fn handle(
        &self, command: Self::Command, services: &Self::Services,
    ) -> Result<Vec<Self::Event>, Self::Error>;

    fn apply(&self, event: Self::Event) -> Option<Self::State>;
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
#[strum(serialize_all = "PascalCase", ascii_case_insensitive)]
pub enum Location {
    Chicago,
    Seattle,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, ToSchema, Serialize, Deserialize)]
#[schema(example = json!("WAZ558"))]
#[serde(transparent)]
#[repr(transparent)]
pub struct LocationZoneCode(String);

impl std::fmt::Display for LocationZoneCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for LocationZoneCode {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Debug, PartialEq, Clone, ToSchema, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuantitativeValue {
    pub value: f32,
    pub max_value: f32,
    pub min_value: f32,
    pub unit_code: Cow<'static, str>,
    pub quality_control: QualityControl,
}

impl QuantitativeValue {
    pub fn new(
        value: f32, min_value: f32, max_value: f32, unit_code: &str,
        quality_control: QualityControl,
    ) -> Self {
        Self {
            value,
            max_value,
            min_value,
            unit_code: unit_code.into(),
            quality_control,
        }
    }

    pub fn unit_code(&self) -> &str {
        self.unit_code.borrow()
    }
}

const LEVEL: &str = "Level";

#[derive(
    Debug,
    Display,
    Copy,
    Clone,
    PartialEq,
    Eq,
    Hash,
    EnumString,
    EnumVariantNames,
    EnumMessage,
    // EnumProperty,
    ToSchema,
    Serialize,
    Deserialize,
)]
#[strum(serialize_all = "UPPERCASE")]
pub enum QualityControl {
    #[strum(message = "Verified, passed levels 1, 2, and 3")]
    // #[strum(props(Level = 9))]
    V,

    #[strum(message = "Subjective good")]
    // #[strum(props(Level = 8))]
    G,

    #[strum(message = "Screened, passed levels 1 and 2")]
    // #[strum(props(Level = 7))]
    S,

    #[strum(message = "Coarse pass, passed level 1")]
    // #[strum(props(Level = 6))]
    C,

    #[strum(message = "Preliminary, no QC")]
    // #[strum(props(Level = 5))]
    Z,

    #[strum(
        message = "Questioned, passed level 1, failed 2 or 3 where: level 1 = validity; level 2 = internal consistency, temporal consistency, statistical spatial consistency checks; level 3 = spatial consistency check"
    )]
    // #[strum(props(Level = 4))]
    Q,

    #[strum(
        message = "Virtual temperature could not be calculated, air temperature passing all QC checks has been returned"
    )]
    // #[strum(props(Level = 3))]
    T,

    #[strum(message = "Subjective bad")]
    // #[strum(props(Level = 2))]
    B,

    #[strum(message = "Rejected/erroneous, failed level 1")]
    // #[strum(props(Level = 1))]
    X,
}

impl QualityControl {
    pub fn level(&self) -> usize {
        match self {
            Self::V => 9,
            Self::G => 8,
            Self::S => 7,
            Self::C => 6,
            Self::Z => 5,
            Self::Q => 4,
            Self::T => 3,
            Self::B => 2,
            Self::X => 1,
        }
    }
}

impl PartialOrd for QualityControl {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QualityControl {
    fn cmp(&self, other: &Self) -> Ordering {
        self.level().cmp(&other.level())
    }
}

#[derive(Debug, Clone, PartialEq, ToSchema, Serialize, Deserialize)]
pub struct ZoneForecast {
    // #[serde(deserialize_with = "ZoneForecast::deserialize_zone_from_url")]
    pub zone: String,

    pub updated: DateTime<Utc>,

    pub periods: Vec<ForecastDetail>,
}

impl ZoneForecast {
    // pub fn deserialize_zone_from_url<'de, D>(deserializer: D) -> String
    //     where
    //         D: serde::Deserializer<'de>,
    // {
    //     let zone_url = String::deserialize(deserializer)?;
    //     let split_url: Vec<_> = zone_url.split('/').collect();
    //     split_url[split_url.len() - 1].to_string()
    // }
}

impl TryFrom<Feature> for ZoneForecast {
    type Error = WeatherError;

    fn try_from(feature: Feature) -> Result<Self, Self::Error> {
        let zone = feature
            .property("zone")
            .and_then(|p| p.as_str())
            .ok_or_else(|| Self::Error::MissingFeature("zone".to_string()))?
            .to_string();

        let updated = Utc::now();

        let periods: Vec<Result<ForecastDetail, Self::Error>> = feature
            .property("periods")
            .and_then(|p| p.as_array())
            .cloned()
            .map(|ps| {
                ps.into_iter()
                    .map(|detail| serde_json::from_value(detail).map_err(|err| err.into()))
                    .collect()
            })
            .ok_or_else(|| Self::Error::MissingFeature("periods".to_string()))?;

        // let periods: Vec<Result<Self, Self::Error>> = feature
        //     .property("periods").ok_or_else(|| Self::Error::MissingFeature("periods".to_string()))?
        //     .as_array().ok_or_else(|| Self::Error::MissingFeature("periods".to_string()))?
        //     .clone()
        //     .into_iter()
        //     .map(|detail| serde_json::from_value(detail).map_err(|err| err.into()))
        //     .collect();
        let periods: Vec<ForecastDetail> =
            periods
                .into_iter()
                .fold(Ok(Vec::with_capacity(periods.len())), |acc, res| {
                    match (acc, res) {
                        (Ok(mut acc0), Ok(p)) => {
                            acc0.push(p);
                            Ok(acc0)
                        },
                        (Ok(_), Err(err)) => Err(err),
                        (Err(err), _) => Err(err),
                    }
                })?;

        Ok(Self { zone, updated, periods })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToSchema, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastDetail {
    pub name: String,

    #[serde(alias = "detailedForecast")]
    pub forecast: String,
}

#[derive(Debug, Clone, PartialEq, PartialOrd, ToSchema, Serialize, Deserialize)]
#[schema(example = json!("360.0"))]
#[serde(transparent)]
#[repr(transparent)]
pub struct Direction(f32);

impl std::fmt::Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub fn average_direction(directions: &[Direction]) -> Option<Direction> {
    if directions.is_empty() {
        return None;
    }
    let n = directions.len() as f32;
    let sum_x = directions.iter().map(|&d| d.0.to_radians().cos()).sum::<f32>();
    let sum_y = directions.iter().map(|&d| d.0.to_radians().sin()).sum::<f32>();
    let avg_x = sum_x / n;
    let avg_y = sum_y / n;
    Some(Direction((avg_y.atan2(avg_x).to_degrees() + 360.0) % 360.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use pretty_assertions::assert_eq;
    use proptest::prelude::*;

    // - property test for sane averages
    proptest! {
        #[test]
        fn test_average_direction(directions in vec(any::<f64>().prop_filter("valid angle", |d| *d>=0.0 && *d<=360.0), 0..10)) {
            let result = average_direction(&directions);
            prop_assert!(
                match result {
                    None => directions.is_empty(),
                    Some(average) => average >= 0.0 && average <= 360.0,
                }
            );
        }
    }

    #[test]
    fn test_average_direction_single() {
        let directions = [90.0];
        assert_eq!(average_direction(&directions), Some(90.0));
    }

    #[test]
    fn test_average_direction_opposite() {
        let directions = [90.0, 270.0];
        assert_relative_eq!(average_direction(&directions), Some(180.0), epsilon = 1e-9);
    }

    #[test]
    fn test_average_direction_not_opposite() {
        let directions = [45.0, 135.0];
        assert_relative_eq!(average_direction(&directions), Some(90.0), epsilon = 1e-9);
    }

    #[test]
    fn test_average_direction_three() {
        let directions = [0.0, 120.0, 240.0];
        assert_relative_eq!(average_direction(&directions), Some(160.0), epsilon = 1e-9);
    }

    #[test]
    fn test_average_direction_multiple() {
        let directions = [0.0, 45.0, 90.0, 360.0];
        assert_relative_eq!(average_direction(&directions), Some(45.0), epsilon = 1e-9);
    }

    #[test]
    fn test_average_direction_across_0_360() {
        let directions = [0.0, 5.0, 355.0, 360.0];
        assert_relative_eq!(average_direction(&directions), Some(0.0), epsilon = 1e-9);
    }

    #[test]
    fn test_average_direction_empty() {
        let directions: &[f64] = &[];
        assert_eq!(average_direction(directions), None);
    }
}