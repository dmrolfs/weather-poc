use crate::errors::WeatherError;
use crate::model;
use crate::model::{
    transpose_result, LocationZoneIdentifier, WeatherAlert, WeatherFrame, ZoneForecast,
};
use async_trait::async_trait;
use chrono::Utc;
use geojson::{Feature, FeatureCollection, GeoJson};
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use reqwest_middleware::ClientWithMiddleware;
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use std::convert::TryFrom;
use std::time;
use thiserror::Error;
use trim_margin::MarginTrimmable;
use url::Url;

#[async_trait]
pub trait ZoneWeatherApi: Send + Sync {
    async fn zone_observation(
        &self, zone: &LocationZoneIdentifier,
    ) -> Result<WeatherFrame, NoaaWeatherError>;

    async fn zone_forecast(
        &self, zone: &LocationZoneIdentifier,
    ) -> Result<ZoneForecast, NoaaWeatherError>;
}

#[async_trait]
pub trait AlertApi: Send + Sync {
    async fn active_alerts(&self) -> Result<Vec<WeatherAlert>, NoaaWeatherError>;
}

#[derive(Debug, Clone)]
pub enum NoaaWeatherServices {
    NOAA(NoaaWeatherApi),
    HappyPath(HappyPathWeatherServices),
}

#[async_trait]
impl ZoneWeatherApi for NoaaWeatherServices {
    async fn zone_observation(
        &self, zone: &LocationZoneIdentifier,
    ) -> Result<WeatherFrame, NoaaWeatherError> {
        match self {
            Self::NOAA(svc) => svc.zone_observation(zone).await,
            Self::HappyPath(svc) => svc.zone_observation(zone).await,
        }
    }

    async fn zone_forecast(
        &self, zone: &LocationZoneIdentifier,
    ) -> Result<ZoneForecast, NoaaWeatherError> {
        match self {
            Self::NOAA(svc) => svc.zone_forecast(zone).await,
            Self::HappyPath(svc) => svc.zone_forecast(zone).await,
        }
    }
}

#[async_trait]
impl AlertApi for NoaaWeatherServices {
    async fn active_alerts(&self) -> Result<Vec<WeatherAlert>, NoaaWeatherError> {
        match self {
            Self::NOAA(svc) => svc.active_alerts().await,
            Self::HappyPath(svc) => svc.active_alerts().await,
        }
    }
}

#[derive(Debug, Error)]
pub enum NoaaWeatherError {
    #[error("supplied Weather API url is not a base url to query: {0}")]
    NotABaseUrl(Url),

    #[error("Weather API call failed: {0}")]
    HttpRequest(#[from] reqwest::Error),

    #[error("error occurred in HTTP middleware calling Weather API: {0}")]
    HttpMiddleware(#[from] reqwest_middleware::Error),

    #[error("failed to parse Weather API GeoJson response: {0}")]
    GeoJson(#[from] geojson::Error),

    #[error("{0}")]
    Weather(#[from] WeatherError),
}

#[derive(Debug, Clone)]
pub struct NoaaWeatherApi {
    client: ClientWithMiddleware,
    base_url: Url,
}

impl NoaaWeatherApi {
    pub fn new(
        base_url: impl Into<Url>, user_agent: HeaderValue,
    ) -> Result<Self, NoaaWeatherError> {
        let base_url = base_url.into();
        if base_url.cannot_be_a_base() {
            return Err(NoaaWeatherError::NotABaseUrl(base_url));
        }

        let client = Self::make_http_client(user_agent)?;

        Ok(Self { client, base_url })
    }

    fn make_http_client(user_agent: HeaderValue) -> Result<ClientWithMiddleware, NoaaWeatherError> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, user_agent);

        let client = reqwest::Client::builder()
            .pool_idle_timeout(time::Duration::from_secs(60))
            .default_headers(headers)
            .pool_max_idle_per_host(5)
            .build()?;

        let retry_policy = ExponentialBackoff::builder()
            .retry_bounds(time::Duration::from_millis(1000), time::Duration::from_secs(300))
            .build_with_max_retries(3);

        Ok(reqwest_middleware::ClientBuilder::new(client)
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build())
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn fetch_geojson(&self, label: &str, url: Url) -> Result<GeoJson, NoaaWeatherError> {
        let response = self.client.get(url.clone()).send().await?;
        log_response(label, &url, &response);

        let status_code = response.status();
        let body = response.text().await?;
        tracing::debug!(%body, ?status_code, %url, "{label} response body");

        let geojson = body.parse()?;
        Ok(geojson)
    }
}

#[async_trait]
impl ZoneWeatherApi for NoaaWeatherApi {
    #[tracing::instrument(level = "debug", skip(self))]
    async fn zone_observation(
        &self, zone: &LocationZoneIdentifier,
    ) -> Result<WeatherFrame, NoaaWeatherError> {
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .unwrap()
            .push("zones")
            .push("forecast")
            .push(zone.code.as_str())
            .push("observations");

        let geojson = self.fetch_geojson("observations", url).await?;
        let features = FeatureCollection::try_from(geojson)?;
        Ok(features.into())
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn zone_forecast(
        &self, zone: &LocationZoneIdentifier,
    ) -> Result<ZoneForecast, NoaaWeatherError> {
        let mut url = self.base_url.clone();
        let zone_type: &'static str = zone.zone_type.into();
        url.path_segments_mut()
            .unwrap()
            .push("zones")
            .push(zone_type)
            .push(zone.code.as_str())
            .push("forecast");

        let geojson = self.fetch_geojson("forecast", url).await?;
        let feature = Feature::try_from(geojson)?;
        Ok(ZoneForecast::try_from(feature)?)
    }
}

#[async_trait]
impl AlertApi for NoaaWeatherApi {
    #[tracing::instrument(level = "debug", skip(self))]
    async fn active_alerts(&self) -> Result<Vec<WeatherAlert>, NoaaWeatherError> {
        let mut url = self.base_url.clone();
        url.path_segments_mut().unwrap().push("alerts").push("active");

        let geojson = self.fetch_geojson("active_alerts", url).await?;
        let features: FeatureCollection = FeatureCollection::try_from(geojson)?;
        let alerts = features.features.into_iter().map(WeatherAlert::try_from);
        transpose_result(alerts).map_err(|err| err.into())
    }
}

fn log_response(label: &str, endpoint: &Url, response: &reqwest::Response) {
    const MESSAGE: &str = "response recd from services.gov";
    let status = response.status();
    if status.is_success() || status.is_informational() {
        tracing::debug!(?endpoint, ?response, "{label}: {MESSAGE}");
    } else if status.is_client_error() {
        tracing::warn!(?endpoint, ?response, "{label}: {MESSAGE}");
    } else {
        tracing::warn!(?endpoint, ?response, "{label}: {MESSAGE}");
    }
}

#[derive(Debug, Copy, Clone)]
pub struct HappyPathWeatherServices;

#[async_trait]
impl ZoneWeatherApi for HappyPathWeatherServices {
    async fn zone_observation(
        &self, _zone: &LocationZoneIdentifier,
    ) -> Result<WeatherFrame, NoaaWeatherError> {
        Ok(WeatherFrame {
            timestamp: iso8601_timestamp::Timestamp::now_utc(),
            temperature: Some(crate::model::QuantitativeValue {
                value: 72.0,
                max_value: 80.0,
                min_value: 60.0,
                unit_code: "DegreesF".into(),
                quality_control: crate::model::QualityControl::V,
            }),
        })
    }

    async fn zone_forecast(
        &self, zone: &LocationZoneIdentifier,
    ) -> Result<ZoneForecast, NoaaWeatherError> {
        Ok(ZoneForecast {
            zone_code: zone.code.to_string(),
            updated: Utc::now(),
            periods: vec![crate::model::ForecastDetail {
                name: "Rest of Day".to_string(),
                forecast: "Mostly cloudy. Highs in the lower to mid 70s. Light wind.".to_string(),
            }],
        })
    }
}

#[async_trait]
impl AlertApi for HappyPathWeatherServices {
    async fn active_alerts(&self) -> Result<Vec<WeatherAlert>, NoaaWeatherError> {
        Ok(vec![
            WeatherAlert {
                affected_zones: vec![
                    LocationZoneIdentifier { zone_type: model::LocationZoneType::County, code: "MDC031".to_string() }
                ],
                status: model::AlertStatus::Actual,
                message_type: model::AlertMessageType::Alert,
                sent: Utc::now() - chrono::Duration::hours(1),
                effective: Utc::now() - chrono::Duration::minutes(55),
                onset: Some(Utc::now() - chrono::Duration::minutes(30)),
                expires: Utc::now() + chrono::Duration::minutes(55),
                ends: Some(Utc::now() + chrono::Duration::hours(1)),
                category: model::AlertCategory::Met,
                severity: model::AlertSeverity::Severe,
                certainty: model::AlertCertainty::Possible,
                urgency: model::AlertUrgency::Immediate,
                event: "High Wind Watch".to_string(),
                headline: "High Wind Watch issued".to_string(),
                description: r##"* WHAT...South winds 30 to 40 mph with gusts up to 50 mph possible.
                    |* WHERE...Portions of southeast Louisiana and southeast and southern Mississippi.
                    |* WHEN...From Tuesday afternoon through late Tuesday night.
                    |* IMPACTS...Damaging winds could blow down trees and power lines.
                    |Widespread power outages are possible.
                    |Travel could be difficult, especially for high profile vehicles.
                    |"##.trim_margin().unwrap(),
                instruction: r##"Monitor the latest forecasts and warnings for updates on this
                    |situation. Fasten loose objects or shelter objects in a safe location prior
                    |to the onset of winds."##.trim_margin(),
                response: model::AlertResponse::Prepare,
            }
        ])
    }
}
