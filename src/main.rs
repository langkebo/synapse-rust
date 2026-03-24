use std::sync::Arc;
use synapse_rust::common::config::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::panic::set_hook(Box::new(|panic_info| {
        let location = panic_info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());
        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };
        eprintln!("PANIC at {}: {}", location, message);
        eprintln!("Backtrace: {:?}", std::backtrace::Backtrace::capture());
    }));

    // 1. 预加载配置
    let config = match Config::load().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    // 2. 初始化遥测服务 (OpenTelemetry)
    let telemetry_service = Arc::new(
        synapse_rust::services::telemetry_service::TelemetryService::new(
            Arc::new(config.telemetry.clone()),
            Arc::new(config.prometheus.clone()),
        ),
    );

    let tracer_provider = match telemetry_service.initialize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to initialize telemetry: {}", e);
            None
        }
    };

    // 3. 初始化全局日志与追踪
    if let Err(e) = synapse_rust::common::logging::init_logging(
        &config.logging,
        Some(telemetry_service.clone()),
        tracer_provider,
    ) {
        eprintln!("Failed to initialize logging: {}", e);
        std::process::exit(1);
    }

    tracing::info!("Starting Synapse Rust Matrix Server...");
    tracing::info!("Server name: {}", config.server.name);
    tracing::info!(
        "Listening on: {}:{}",
        config.server.host,
        config.server.port
    );

    let server = synapse_rust::SynapseServer::new(config).await?;

    server.run().await?;

    // 4. 优雅停机
    telemetry_service.shutdown();

    Ok(())
}
