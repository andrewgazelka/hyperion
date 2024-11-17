use eyre::Context;
use hyperion_bot::bootstrap;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt::init();

    // it is not an error if a .env file is missing
    drop(dotenvy::dotenv());

    let config = envy::from_env().wrap_err("Failed to load config from environment variables")?;

    tracing::info!("{config:?}");

    bootstrap(&config).await;
    Ok(())
}
