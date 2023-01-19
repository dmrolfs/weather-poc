use clap::Parser;
use settings_loader::{LoadingOptions, SettingsLoader};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let subscriber = weather::tracing::get_tracing_subscriber("info");
    weather::tracing::init_subscriber(subscriber);

    let options = parse_options();
    let settings = load_settings(&options);

    let server = weather::Server::build(&settings).await?;
    server.run_until_stopped().await.map_err(|err| err.into())
}

fn parse_options() -> weather::CliOptions {
    let options = weather::CliOptions.parse();
    if options.secrets.is_none() {
        tracing::warn!("No secrets configuration provided. Passwords (e.g., for the database) should be confined in a secret configuration and sourced in a secure manner.");
    }

    options
}

fn load_settings(options: &weather::CliOptions) -> anyhow::Result<weather::Settings> {
    let app_environment = std::env::var(weather::CliOptions::env_app_environment()).ok();
    if app_environment.is_none() {
        tracing::info!("No environment configuration override provided.");
    }

    weather::Settings::load(options)
}
