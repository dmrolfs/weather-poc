mod errors;
mod health_routes;
mod queries;
mod result;
mod state;
mod weather_routes;

use crate::settings::HttpApiSettings;
use crate::Settings;
pub use result::HttpResult;

use axum::error_handling::HandleErrorLayer;
use axum::http::{Response, StatusCode, Uri};
use axum::{BoxError, Router};
use errors::ApiError;
use settings_loader::common::database::DatabaseSettings;
use sqlx::PgPool;
use std::net::TcpListener;
use tokio::signal;
use tokio::task::JoinHandle;
use tower::ServiceBuilder;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_governor::GovernorLayer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tower_http::ServiceBuilderExt;
use utoipa::OpenApi;
use utoipa_swagger_ui::{SwaggerUi, Url as SwaggerUrl};

pub type HttpJoinHandle = JoinHandle<Result<(), ApiError>>;

pub struct Server {
    port: u16,
    server_handle: HttpJoinHandle,
}

impl Server {
    #[tracing::instrument(level = "debug", skip(settings))]
    pub async fn build(settings: &Settings) -> Result<Self, ApiError> {
        let connection_pool = get_connection_pool(&settings.database);
        let address = settings.api.server.address();
        let listener = tokio::net::TcpListener::bind(&address).await?;
        tracing::info!(
            "{:?} API listening on {address}: {listener:?}",
            std::env::current_exe()
        );
        let std_listener = listener.into_std()?;
        let port = std_listener.local_addr()?.port();

        let server_handle = run_http_server(
            std_listener,
            connection_pool,
            &RunParameters::from_settings(settings),
        )
        .await?;

        Ok(Self { port, server_handle })
    }

    pub const fn port(&self) -> u16 {
        self.port
    }

    pub async fn run_until_stopped(self) -> Result<(), ApiError> {
        self.server_handle.await?
    }
}

pub fn get_connection_pool(settings: &DatabaseSettings) -> PgPool {
    let connection_options = settings.pg_connect_options_with_db();
    settings.pg_pool_options().connect_lazy_with(connection_options)
}

#[derive(Debug, Clone)]
pub struct RunParameters {
    pub http_api: HttpApiSettings,
}

impl RunParameters {
    pub fn from_settings(settings: &Settings) -> Self {
        Self { http_api: settings.api.clone() }
    }
}

#[tracing::instrument(level = "trace")]
pub async fn run_http_server(
    listener: TcpListener, db_pool: PgPool, params: &RunParameters,
) -> Result<HttpJoinHandle, ApiError> {
    let state = state::initialize_app_state(db_pool).await?;

    let governor_conf = Box::new(
        GovernorConfigBuilder::default()
            .burst_size(params.http_api.rate_limit.burst_size)
            .period(params.http_api.rate_limit.per_duration)
            .key_extractor(SmartIpKeyExtractor)
            .finish()
            .unwrap(),
    );

    let middleware_stack = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(handle_api_error))
        .layer(GovernorLayer {
            config: Box::leak(governor_conf) // okay to leak because it is created once and then used by layer
        })
        .timeout(params.http_api.timeout)
        .compression()
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().include_headers(true))
                .on_response(DefaultOnResponse::new().include_headers(true))
        )
        // .set_x_request_id(unimplemented!())
        .propagate_x_request_id();

    let api_routes = Router::new()
        .nest("/health", health_routes::api())
        .nest("/weather", weather_routes::api())
        .with_state(state);

    let app = Router::new()
        .merge(SwaggerUi::new("/swagger-ui").urls(vec![
            (
                SwaggerUrl::with_primary("weather_api", "/api-doc/weather-openapi.json", true),
                weather_routes::WeatherApiDoc::openapi(),
            ),
            (
                SwaggerUrl::new("health_api", "/api-doc/health-openapi.json"),
                health_routes::HealthApiDoc::openapi(),
            ),
        ]))
        .nest("/api/v1", api_routes)
        .fallback(fallback)
        .layer(middleware_stack);

    let handle = tokio::spawn(async move {
        tracing::debug!(app_routes=?app, "starting API server...");
        let builder = axum::Server::from_tcp(listener)?;
        let server = builder.serve(app.into_make_service());
        let graceful = server.with_graceful_shutdown(shutdown_signal());
        graceful.await?;
        tracing::info!("{:?} API shutting down", std::env::current_exe());
        Ok(())
    });

    Ok(handle)
}

async fn fallback(uri: Uri) -> (StatusCode, String) {
    (StatusCode::NOT_FOUND, format!("No route found for {uri}"))
}

async fn handle_api_error(error: BoxError) -> Response<String> {
    if error.is::<tower::timeout::error::Elapsed>() {
        let response = Response::new(format!("REQUEST TIMEOUT: {error}"));
        let (mut parts, body) = response.into_parts();
        parts.status = StatusCode::REQUEST_TIMEOUT;
        Response::from_parts(parts, body)
    } else if error.is::<tower_governor::errors::GovernorError>() {
        tower_governor::errors::display_error(error)
    } else {
        let response = Response::new(format!("INTERNAL SERVER ERROR: {error}"));
        let (mut parts, body) = response.into_parts();
        parts.status = StatusCode::INTERNAL_SERVER_ERROR;
        Response::from_parts(parts, body)
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("signal received, starting graceful shutdown");
}
