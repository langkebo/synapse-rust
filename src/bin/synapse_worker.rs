use synapse_rust::common::background_job::BackgroundJob;
use synapse_rust::common::task_queue::RedisTaskQueue;
use synapse_rust::common::config::Config;
use synapse_rust::storage::event::EventStorage;
use std::sync::Arc;
use tokio::signal;
use std::net::SocketAddr;
use axum::{
    routing::get,
    Router,
    response::IntoResponse,
    extract::State,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    tracing::info!("Starting Synapse Worker...");

    // Load config (simplified for worker)
    // In a real scenario, we'd reuse the same config loading logic as the main server
    let config = Config::load().await?;
    let redis_url = config.redis_url();

    tracing::info!("Connecting to Redis at {}", redis_url);
    // RedisTaskQueue::new now accepts &RedisConfig, not &str
    let queue = Arc::new(RedisTaskQueue::new(&config.redis).await?);
    
    // Connect to Database
    let db_url = config.database_url();
    tracing::info!("Connecting to Database...");
    let pool = sqlx::PgPool::connect(&db_url).await?;
    let event_storage = Arc::new(EventStorage::new(&Arc::new(pool)));

    let worker_id = uuid::Uuid::new_v4().to_string();
    let consumer_name = format!("worker-{}", worker_id);
    let group_name = "synapse_workers";

    tracing::info!("Worker {} started. Joining group {}", consumer_name, group_name);

    // Job Handler Logic
    let event_storage_clone = event_storage.clone();
    let job_handler = move |job: BackgroundJob| {
        let event_storage = event_storage_clone.clone();
        async move {
            match job {
                BackgroundJob::SendEmail { to, subject, body: _ } => {
                    tracing::info!("📧 [EMAIL] Sending email to: {}", to);
                    tracing::info!("   Subject: {}", subject);
                    // In a real app, we would use an SMTP client here (e.g. lettre)
                    // For now, we simulate network latency
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    tracing::info!("✅ [EMAIL] Sent successfully to {}", to);
                    Ok(())
                }
                BackgroundJob::ProcessMedia { file_id } => {
                    tracing::info!("🎬 [MEDIA] Processing media file: {}", file_id);
                    // Simulate thumbnail generation
                    // 1. Read file from disk
                    // 2. Resize image
                    // 3. Save thumbnails
                    tracing::info!("   Generating thumbnails for {}...", file_id);
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    tracing::info!("✅ [MEDIA] Thumbnails generated for {}", file_id);
                    Ok(())
                }
                BackgroundJob::FederationTransaction { txn_id, destination } => {
                    tracing::info!("🌐 [FEDERATION] Processing transaction {} to {}", txn_id, destination);
                    // Simulate HTTP request
                    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                    Ok(())
                }
                BackgroundJob::Generic { name, payload } => {
                    tracing::info!("🔧 [GENERIC] Processing job '{}'", name);
                    
                    if name == "notify_device_revocation" {
                        if let Some(dest) = payload.get("destination").and_then(|v| v.as_str()) {
                            tracing::info!("   📣 Notifying device revocation to {}", dest);
                            // Simulate the actual PUT request here
                            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
                            tracing::info!("   ✅ Device revocation notified to {}", dest);
                        }
                    } else {
                         tracing::info!("   Payload: {:?}", payload);
                    }
                    
                    Ok(())
                }
                BackgroundJob::RedactEvent { room_id, event_id, reason } => {
                    tracing::info!("🔥 [REDACT] Executing redaction for room {} event {}", room_id, event_id);
                    if let Some(r) = reason {
                        tracing::info!("   Reason: {}", r);
                    }
                    
                    if let Err(e) = event_storage.redact_event_content(&event_id).await {
                         tracing::error!("❌ [REDACT] Failed to redact event {}: {}", event_id, e);
                         return Err(e.to_string());
                    }
                    
                    tracing::info!("✅ [REDACT] Event {} physically redacted/burnt", event_id);
                    Ok(())
                }
            }
        }
    };

    // Run consume loop in background
    let queue_clone = queue.clone();
    let group_name_clone = group_name.to_string();
    let handle = tokio::spawn(async move {
        if let Err(e) = queue_clone.consume_loop(&group_name_clone, &consumer_name, job_handler).await {
            tracing::error!("Worker loop terminated with error: {}", e);
        }
    });

    // Start Metrics Server
    // Default port 9090 for metrics
    let metrics_addr = SocketAddr::from(([0, 0, 0, 0], 9090));
    let metrics_queue = queue.clone();
    let metrics_app = Router::new()
        .route("/metrics", get(get_metrics))
        .with_state(metrics_queue);

    tracing::info!("Metrics server listening on {}", metrics_addr);
    
    let metrics_handle = tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(metrics_addr).await.unwrap();
        axum::serve(listener, metrics_app).await.unwrap();
    });

    // Alerting / Monitoring Loop
    let monitor_queue = queue.clone();
    let monitor_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(15));
        loop {
            interval.tick().await;
            match monitor_queue.get_metrics("synapse_workers").await {
                Ok(metrics) => {
                    // Alert if queue length > 1000
                    if metrics.queue_length > 1000 {
                        tracing::warn!("🚨 High Queue Depth: {} tasks pending!", metrics.queue_length);
                    }
                    // Alert if total consumer lag > 500
                    if metrics.consumer_lag > 500 {
                        tracing::warn!("🚨 High Consumer Lag: {} unacknowledged tasks!", metrics.consumer_lag);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to fetch metrics for monitoring: {}", e);
                }
            }
        }
    });

    // Wait for shutdown signal
    match signal::ctrl_c().await {
        Ok(()) => {
            tracing::info!("Shutdown signal received");
        },
        Err(err) => {
            tracing::error!("Unable to listen for shutdown signal: {}", err);
        },
    }

    // Abort worker tasks
    handle.abort();
    metrics_handle.abort();
    monitor_handle.abort();
    tracing::info!("Synapse Worker shut down gracefully");

    Ok(())
}

async fn get_metrics(
    State(queue): State<Arc<RedisTaskQueue>>,
) -> impl IntoResponse {
    match queue.get_metrics("synapse_workers").await {
        Ok(metrics) => {
            // Generate Prometheus format output
            let mut body = String::new();
            
            // Queue Length
            body.push_str("# HELP synapse_worker_queue_length Current length of the job queue\n");
            body.push_str("# TYPE synapse_worker_queue_length gauge\n");
            body.push_str(&format!("synapse_worker_queue_length {}\n", metrics.queue_length));
            
            // Consumer Lag
            body.push_str("# HELP synapse_worker_consumer_lag Total unacknowledged messages\n");
            body.push_str("# TYPE synapse_worker_consumer_lag gauge\n");
            body.push_str(&format!("synapse_worker_consumer_lag {}\n", metrics.consumer_lag));
            
            // Per-consumer Lag (optional, but useful)
            body.push_str("# HELP synapse_worker_consumer_pending Pending messages per consumer\n");
            body.push_str("# TYPE synapse_worker_consumer_pending gauge\n");
            for (consumer, pending) in metrics.consumers {
                body.push_str(&format!("synapse_worker_consumer_pending{{consumer=\"{}\"}} {}\n", consumer, pending));
            }

            ([(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4")], body).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to get metrics: {}", e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}
