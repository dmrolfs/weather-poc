use super::{
    model::{Forecast, Observation, Properties},
    WeatherApi, WeatherApiError, ZoneForecast,
};
use crate::model::{LocationZoneCode, WeatherFrame};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, USER_AGENT};
use reqwest_middleware::ClientWithMiddleware;
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use std::time::Duration;
use tracing::Instrument;
use url::Url;

#[derive(Debug, Clone)]
pub struct AppLocationServices {
    weather_client: ClientWithMiddleware,
    weather_base_url: Url,
}

impl AppLocationServices {
    pub fn new(weather_base_url: impl Into<Url>) -> Self {
        let weather_client = Self::make_http_client().expect("failed to make weather.gov client");
        Self {
            weather_client,
            weather_base_url: weather_base_url.into(),
        }
    }

    fn make_http_client() -> Result<ClientWithMiddleware, WeatherApiError> {
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
    ) -> Result<Observation, WeatherApiError> {
        const LABEL: &str = "observations";

        let mut url = self.weather_base_url.clone();
        url.path_segments_mut()
            .map_err(|_| WeatherApiError::NotABaseUrl(self.weather_base_url.clone()))?
            .push("zones")
            .push("forecast")
            .push(zone.get_ref())
            .push("observations");

        let span = tracing::debug_span!("get_observations", %zone);

        self.weather_client
            .get(url.clone())
            .send()
            .map_err(|error| error.into())
            .and_then(|response| {
                log_response(LABEL, &url, &response);
                response.text().map(|body| {
                    body.map_err(|err| err.into()).and_then(|body| {
                        let result = serde_json::from_str(&body).map_err(|err| err.into());
                        tracing::debug!(%body, response=?result, "{LABEL} response body");
                        result
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
    ) -> Result<WeatherFrame, WeatherApiError> {
    }

    #[tracing::instrument(level = "debug")]
    async fn zone_forecast(
        &self, zone: &LocationZoneCode,
    ) -> Result<ZoneForecast, WeatherApiError> {
        todo!()
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
