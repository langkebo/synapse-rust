use std::env;
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

    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:5432/synapse".to_string());
    let server_name = env::var("SERVER_NAME").unwrap_or_else(|_| "localhost".to_string());
    let jwt_secret = env::var("JWT_SECRET").unwrap_or_else(|_| {
        use rand::RngCore;
        let mut secret = [0u8; 64];
        rand::thread_rng().fill_bytes(&mut secret);
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, secret)
    });
    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port: u16 = env::var("PORT")
        .unwrap_or_else(|_| "8008".to_string())
        .parse()?;
    let media_path = env::var("MEDIA_PATH").unwrap_or_else(|_| "./media".to_string());

    tracing::info!("Starting Synapse Rust Matrix Server...");
    tracing::info!("Database URL: {}", database_url);
    tracing::info!("Server name: {}", server_name);
    tracing::info!("Host: {}:{}", host, port);
    tracing::info!("Media path: {}", media_path);

    let address = format!("{}:{}", host, port).parse::<std::net::SocketAddr>()?;
    let server = synapse_rust::SynapseServer::new(
        &database_url,
        &server_name,
        &jwt_secret,
        address,
        std::path::PathBuf::from(media_path),
    )
    .await?;

    server.run().await?;

    Ok(())
}
