use super::{LocationServiceError, WeatherApi, ZoneForecast};
use crate::model::{LocationZoneCode, WeatherFrame};
use async_trait::async_trait;
use futures_util::{FutureExt, TryFutureExt};
use geojson::{Feature, FeatureCollection, GeoJson};
use reqwest::header::{HeaderMap, USER_AGENT};
use reqwest_middleware::ClientWithMiddleware;
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use std::convert::TryFrom;
use std::time::Duration;
use tracing::Instrument;
use url::Url;

#[derive(Debug, Clone)]
pub struct AppLocationServices {
    weather_client: ClientWithMiddleware,
    weather_zones_base: Url,
}

impl AppLocationServices {
    pub fn new(weather_base_url: impl Into<Url>) -> Result<Self, LocationServiceError> {
        let weather_client = Self::make_http_client()?;

        let mut weather_zones_base = weather_base_url.into();
        weather_zones_base
            .path_segments_mut()
            .map_err(|_| LocationServiceError::NotABaseUrl(weather_zones_base.clone()))?
            .push("zones");

        Ok(Self { weather_client, weather_zones_base })
    }

    fn make_http_client() -> Result<ClientWithMiddleware, LocationServiceError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            "(here.com, damon.rolfs@here.com)"
                .parse()
                .expect("failed to parse User-Agent for weather.gov"),
        );

        let client = reqwest::Client::builder()
            .pool_idle_timeout(Duration::from_secs(60))
            .default_headers(headers)
            .pool_max_idle_per_host(5)
            .build()?;

        let retry_policy = ExponentialBackoff::builder()
            .retry_bounds(Duration::from_millis(1000), Duration::from_secs(300))
            .build_with_max_retries(3);

        Ok(reqwest_middleware::ClientBuilder::new(client)
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build())
    }

    #[tracing::instrument(level = "debug")]
    async fn get_observations(
        &self, zone: &LocationZoneCode,
    ) -> Result<FeatureCollection, LocationServiceError> {
        const LABEL: &str = "observations";

        let mut url = self.weather_zones_base.clone();
        url.path_segments_mut()
            .unwrap()
            .push("forecast")
            .push(zone.as_ref())
            .push("observations");

        let span = tracing::debug_span!("get_observations", %zone);

        self.weather_client
            .get(url.clone())
            .send()
            .map_err(|error| error.into())
            .and_then(|response| {
                log_response(LABEL, &url, &response);
                response
                    .text()
                    .map(|body| {
                        body
                            .map_err(|err| err.into())
                            .and_then(|geojson_str| {
                                let geojson: Result<GeoJson, _> = geojson_str.parse::<GeoJson>();
                                let features: Result<FeatureCollection, LocationServiceError> = geojson
                                    .and_then(FeatureCollection::try_from)
                                    .map_err(|err| err.into())
                                    ;
                                // let result = serde_json::from_str(&b).map_err(|err| err.into());
                                tracing::debug!(body=%geojson_str, response=?features, "{LABEL} response body");
                                features
                            })
                    })
            })
            .instrument(span)
            .await
    }

    #[tracing::instrument(level = "debug")]
    async fn get_forecast(
        &self, zone: &LocationZoneCode,
    ) -> Result<ZoneForecast, LocationServiceError> {
        const LABEL: &str = "forecast";

        let mut url = self.weather_zones_base.clone();
        url.path_segments_mut()
            .unwrap()
            .push("public")
            .push(zone.as_ref())
            .push("forecast");

        let span = tracing::debug_span!("get_forecast", %zone);

        self.weather_client
            .get(url.clone())
            .send()
            .map_err(|error| error.into())
            .and_then(|response| {
                log_response(LABEL, &url, &response);
                response
                    .text()
                    .map(|body| {
                        body
                            .map_err(|err| LocationServiceError::HttpRequest(err))
                            .and_then(|geojson_str| {
                                let geojson: Result<GeoJson, _> = geojson_str.parse::<GeoJson>();
                                let forecast = geojson
                                    .and_then(Feature::try_from)
                                    .map_err(|err: geojson::Error| LocationServiceError::GeoJson(err))
                                    .and_then(|feature| ZoneForecast::try_from(feature).map_err(|err| err.into()));

                                // let result = serde_json::from_str(&b).map_err(|err| err.into());
                                tracing::debug!(body=%geojson_str, response=?forecast, "{LABEL} response body");
                                forecast
                            })
                    })
            })
            .instrument(span)
            .await
    }
}

#[async_trait]
impl WeatherApi for AppLocationServices {
    #[tracing::instrument(level = "debug")]
    async fn zone_observations(
        &self, zone: &LocationZoneCode,
    ) -> Result<WeatherFrame, LocationServiceError> {
        Ok(self.get_observations(zone).await?.into())
    }

    #[tracing::instrument(level = "debug")]
    async fn zone_forecast(
        &self, zone: &LocationZoneCode,
    ) -> Result<ZoneForecast, LocationServiceError> {
        self.get_forecast(zone).await
    }
}

fn log_response(label: &str, endpoint: &Url, response: &reqwest::Response) {
    const MESSAGE: &str = "response recd from weather.gov";
    let status = response.status();
    if status.is_success() || status.is_informational() {
        tracing::debug!(?endpoint, ?response, "{label}: {MESSAGE}");
    } else if status.is_client_error() {
        tracing::warn!(?endpoint, ?response, "{label}: {MESSAGE}");
    } else {
        tracing::warn!(?endpoint, ?response, "{label}: {MESSAGE}");
    }
}
