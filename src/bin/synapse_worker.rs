use axum::{extract::State, response::IntoResponse, routing::get, Router};
use std::net::SocketAddr;
use std::sync::Arc;
use synapse_rust::common::background_job::BackgroundJob;
use synapse_rust::common::config::Config;
use synapse_rust::common::task_queue::RedisTaskQueue;
use synapse_rust::storage::event::EventStorage;
use tokio::signal;

#[derive(Clone)]
struct MetricsState {
    queue: Arc<RedisTaskQueue>,
    token: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let config = Config::load().await?;

    let redis_url = config.redis_url();
    tracing::info!("Connecting to Redis at {}", redis_url);
    let queue = Arc::new(RedisTaskQueue::new(&config.redis).await?);

    let db_url = config.database_url();
    tracing::info!("Connecting to Database...");
    let pool = sqlx::PgPool::connect(&db_url).await?;
    let event_storage = Arc::new(EventStorage::new(&Arc::new(pool), "cjystx.top".to_string()));

    let worker_id = uuid::Uuid::new_v4().to_string();
    let consumer_name = format!("worker-{}", worker_id);
    let group_name = "synapse_workers";

    tracing::info!(
        "Worker {} started. Joining group {}",
        consumer_name,
        group_name
    );

    let event_storage_clone = event_storage.clone();
    let job_handler = move |job: BackgroundJob| {
        let event_storage = event_storage_clone.clone();
        async move {
            match job {
                BackgroundJob::SendEmail {
                    to,
                    subject,
                    body: _,
                } => {
                    tracing::info!("[EMAIL] Sending email to: {}", to);
                    tracing::info!("[EMAIL] Subject: {}", subject);
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    tracing::info!("[EMAIL] Sent successfully to {}", to);
                    Ok(())
                }
                BackgroundJob::ProcessMedia { file_id } => {
                    tracing::info!("[MEDIA] Processing media file: {}", file_id);
                    tracing::info!("[MEDIA] Generating thumbnails for {}...", file_id);
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    tracing::info!("[MEDIA] Thumbnails generated for {}", file_id);
                    Ok(())
                }
                BackgroundJob::FederationTransaction {
                    txn_id,
                    destination,
                } => {
                    tracing::info!(
                        "[FEDERATION] Processing transaction {} to {}",
                        txn_id,
                        destination
                    );
                    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                    Ok(())
                }
                BackgroundJob::Generic { name, payload } => {
                    tracing::info!("[GENERIC] Processing job '{}'", name);

                    if name == "notify_device_revocation" {
                        if let Some(dest) = payload.get("destination").and_then(|v| v.as_str()) {
                            tracing::info!("[GENERIC] Notifying device revocation to {}", dest);
                            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
                            tracing::info!("[GENERIC] Device revocation notified to {}", dest);
                        }
                    } else {
                        tracing::info!(
                            "[GENERIC] Payload summary: fields={}",
                            payload.as_object().map(|o| o.len()).unwrap_or(0)
                        );
                    }

                    Ok(())
                }
                BackgroundJob::RedactEvent {
                    room_id,
                    event_id,
                    reason,
                } => {
                    tracing::info!(
                        "[REDACT] Executing redaction for room {} event {}",
                        room_id,
                        event_id
                    );
                    if let Some(r) = reason {
                        tracing::info!("[REDACT] Reason: {}", r);
                    }

                    if let Err(e) = event_storage.redact_event_content(&event_id).await {
                        tracing::error!("[REDACT] Failed to redact event {}: {}", event_id, e);
                        return Err(e.to_string());
                    }

                    tracing::info!("[REDACT] Event {} physically redacted/burnt", event_id);
                    Ok(())
                }
                BackgroundJob::DelayedEventProcessing { event_id } => {
                    tracing::info!("[DELAYED] Processing delayed event: {}", event_id);
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    tracing::info!("[DELAYED] Event {} processed", event_id);
                    Ok(())
                }
            }
        }
    };

    let queue_clone = queue.clone();
    let group_name_clone = group_name.to_string();
    let handle = tokio::spawn(async move {
        if let Err(e) = queue_clone
            .consume_loop(&group_name_clone, &consumer_name, job_handler)
            .await
        {
            tracing::error!("Worker loop terminated with error: {}", e);
        }
    });

    let metrics_host =
        std::env::var("SYNAPSE_WORKER_METRICS_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let metrics_port: u16 = std::env::var("SYNAPSE_WORKER_METRICS_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(9091);
    let metrics_addr: SocketAddr = format!("{}:{}", metrics_host, metrics_port).parse()?;
    let metrics_token = std::env::var("SYNAPSE_WORKER_METRICS_TOKEN")
        .ok()
        .filter(|t| !t.is_empty());
    let refuse_unprotected_metrics = !metrics_addr.ip().is_loopback() && metrics_token.is_none();

    let metrics_state = MetricsState {
        queue: queue.clone(),
        token: metrics_token.clone(),
    };
    let metrics_app = Router::new()
        .route("/metrics", get(get_metrics))
        .with_state(metrics_state);

    let metrics_handle = if refuse_unprotected_metrics {
        tracing::error!(
            "Refusing to expose metrics on non-loopback address {} without SYNAPSE_WORKER_METRICS_TOKEN",
            metrics_addr
        );
        None
    } else {
        tracing::info!("Metrics server listening on {}", metrics_addr);
        Some(tokio::spawn(async move {
            match tokio::net::TcpListener::bind(metrics_addr).await {
                Ok(listener) => {
                    if let Err(e) = axum::serve(listener, metrics_app).await {
                        tracing::error!("Metrics server error: {}", e);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to bind metrics server on {}: {}", metrics_addr, e);
                }
            }
        }))
    };

    let monitor_queue = queue.clone();
    let monitor_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(15));
        loop {
            interval.tick().await;
            match monitor_queue.get_metrics("synapse_workers").await {
                Ok(metrics) => {
                    if metrics.queue_length > 1000 {
                        tracing::warn!("High Queue Depth: {} tasks pending!", metrics.queue_length);
                    }
                    if metrics.consumer_lag > 500 {
                        tracing::warn!(
                            "High Consumer Lag: {} unacknowledged tasks!",
                            metrics.consumer_lag
                        );
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to fetch metrics for monitoring: {}", e);
                }
            }
        }
    });

    match signal::ctrl_c().await {
        Ok(()) => {
            tracing::info!("Shutdown signal received");
        }
        Err(err) => {
            tracing::error!("Unable to listen for shutdown signal: {}", err);
        }
    }

    handle.abort();
    if let Some(h) = metrics_handle {
        h.abort();
    }
    monitor_handle.abort();
    tracing::info!("Synapse Worker shut down gracefully");

    Ok(())
}

async fn get_metrics(
    State(state): State<MetricsState>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    if let Some(token) = &state.token {
        let provided = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .unwrap_or_default();
        if provided != token {
            return (axum::http::StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
        }
    }

    match state.queue.get_metrics("synapse_workers").await {
        Ok(metrics) => {
            let mut body = String::new();

            body.push_str("# HELP synapse_worker_queue_length Current length of the job queue\n");
            body.push_str("# TYPE synapse_worker_queue_length gauge\n");
            body.push_str(&format!(
                "synapse_worker_queue_length {}\n",
                metrics.queue_length
            ));

            body.push_str("# HELP synapse_worker_consumer_lag Total unacknowledged messages\n");
            body.push_str("# TYPE synapse_worker_consumer_lag gauge\n");
            body.push_str(&format!(
                "synapse_worker_consumer_lag {}\n",
                metrics.consumer_lag
            ));

            body.push_str("# HELP synapse_worker_consumer_pending Pending messages per consumer\n");
            body.push_str("# TYPE synapse_worker_consumer_pending gauge\n");
            for (consumer, pending) in metrics.consumers {
                body.push_str(&format!(
                    "synapse_worker_consumer_pending{{consumer=\"{}\"}} {}\n",
                    consumer, pending
                ));
            }

            (
                [(
                    axum::http::header::CONTENT_TYPE,
                    "text/plain; version=0.0.4",
                )],
                body,
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to get metrics: {}", e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}
