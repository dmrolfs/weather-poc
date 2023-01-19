use crate::model::{LocationZoneCode, WeatherFrame};
use async_trait::async_trait;
use cqrs_es::Aggregate;
use postgres_es::PostgresCqrs;
use pretty_snowflake::{Id, Label};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub type LocationZoneAggregate = Arc<PostgresCqrs<LocationZone>>;

pub const AGGREGATE_TYPE: &str = "location_zone";

#[inline]
pub fn generate_id() -> Id<LocationZone> {
    pretty_snowflake::generator::next_id()
}

#[derive(Debug, Default, Clone, Label, PartialEq, Serialize, Deserialize)]
pub struct LocationZone {
    pub zone_code: LocationZoneCode,
    pub weather: WeatherFrame,
}
