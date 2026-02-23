use synapse_rust::common::config::Config;
use tracing_subscriber::fmt;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env_filter = EnvFilter::builder()
        .parse(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "info,synapse_rust=debug,tower_http=debug".to_string()),
        )
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_timer(fmt::time::uptime())
        .init();

    tracing::info!("Loading configuration from homeserver.yaml...");
    let config = Config::load().await?;

    tracing::info!("Starting Synapse Rust Matrix Server...");
    tracing::info!("Server name: {}", config.server.name);
    tracing::info!(
        "Listening on: {}:{}",
        config.server.host,
        config.server.port
    );

    let server = synapse_rust::SynapseServer::new(config).await?;

    server.run().await?;

    Ok(())
}
