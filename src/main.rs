use synapse_rust::common::config::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::panic::set_hook(Box::new(|panic_info| {
        let location = panic_info
            .location()
            .map_or_else(|| "unknown".to_string(), |l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };
        eprintln!("PANIC at {location}: {message}");
        eprintln!("Backtrace: {:?}", std::backtrace::Backtrace::capture());
    }));

    // 1. Load configuration
    let config = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load configuration: {e}");
            std::process::exit(1);
        }
    };

    // 2. Initialize telemetry and global tracing/logging.
    //    The returned guard flushes telemetry on drop.
    let _telemetry_guard = synapse_rust::server::telemetry::init_telemetry(&config);

    tracing::info!("Starting Synapse Rust Matrix Server...");
    tracing::info!("Server name: {}", config.server.name);
    tracing::info!("Listening on: {}:{}", config.server.host, config.server.port);

    let server = synapse_rust::SynapseServer::new(config).await?;

    server.run().await?;

    // Graceful shutdown — telemetry is flushed when _telemetry_guard is dropped.
    Ok(())
}
