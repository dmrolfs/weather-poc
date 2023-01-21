use thiserror::Error;
use utoipa::ToSchema;

#[derive(Debug, ToSchema, Error)]
#[non_exhaustive]
pub enum WeatherError {
    #[error("failed to convert GeoJson Feature: {0}")]
    GeoJson(#[from] geojson::Error),

    #[error("empty quantitative aggregation")]
    EmptyAggregation,

    #[error("missing GeoJson Feature:{0}")]
    MissingFeature(String),

    #[error("failed to parse Json: {0}")]
    Json(#[from] serde_json::Error),

    // Api(#[from] server::ApiError),
    #[error("Encountered a technical failure: {source}")]
    Unexpected { source: anyhow::Error },
}

// impl fmt::Display for WeatherError {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         write!(f, "{}", "WeatherError")
//     }
// }
//
