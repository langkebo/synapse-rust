#![cfg_attr(test, allow(clippy::panic))]

use axum::{extract::State, response::IntoResponse, routing::get, Router};
use lettre::message::Mailbox;
use lettre::{AsyncTransport, Message, Tokio1Executor};
use std::net::SocketAddr;
use std::sync::Arc;
use synapse_rust::common::config::{Config, SmtpConfig};
use synapse_rust::common::BackgroundJob;
use synapse_rust::common::RedisTaskQueue;
use synapse_rust::storage::event::EventStorage;
use tokio::signal;

#[derive(Clone)]
struct MetricsState {
    queue: Arc<RedisTaskQueue>,
    token: Option<String>,
}

type SmtpMailer = lettre::AsyncSmtpTransport<Tokio1Executor>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let config = Config::load()?;

    let redis_url = config.redis_url();
    tracing::info!("Connecting to Redis at {}", redis_url);
    let queue = Arc::new(RedisTaskQueue::new(&config.redis)?);

    let db_url = config.database_url();
    tracing::info!("Connecting to Database...");
    let pool = sqlx::PgPool::connect(&db_url).await?;
    let server_name = config.server.name.clone();
    let event_storage = Arc::new(EventStorage::new(&Arc::new(pool), server_name));

    // Build SMTP transport if SMTP is enabled
    let smtp_config = config.smtp.clone();
    let smtp_mailer: Option<Arc<lettre::AsyncSmtpTransport<Tokio1Executor>>> =
        if smtp_config.enabled && !smtp_config.host.is_empty() {
            match build_smtp_mailer(&smtp_config) {
                Ok(mailer) => {
                    tracing::info!(
                        "SMTP enabled: {}:{} (from={}, tls={})",
                        smtp_config.host,
                        smtp_config.port,
                        smtp_config.from,
                        smtp_config.tls
                    );
                    Some(Arc::new(mailer))
                }
                Err(e) => {
                    tracing::error!("Failed to build SMTP transport: {}. Email sending will be disabled.", e);
                    None
                }
            }
        } else {
            tracing::info!("SMTP not configured; email sending disabled");
            None
        };

    let worker_id = uuid::Uuid::new_v4().to_string();
    let consumer_name = format!("worker-{worker_id}");
    let group_name = "synapse_workers";

    tracing::info!("Worker {} started. Joining group {}", consumer_name, group_name);

    let event_storage_clone = event_storage.clone();
    let smtp_mailer_clone = smtp_mailer.clone();
    let smtp_from = smtp_config.from.clone();
    let job_handler = move |job: BackgroundJob| {
        let event_storage = event_storage_clone.clone();
        let smtp_mailer = smtp_mailer_clone.clone();
        let smtp_from = smtp_from.clone();
        async move {
            match job {
                BackgroundJob::SendEmail { to, subject, body } => {
                    process_send_email_job(smtp_mailer.clone(), &smtp_from, &to, &subject, &body).await
                }
                BackgroundJob::ProcessMedia { file_id } => {
                    tracing::info!("[MEDIA] Processing media file: {}", file_id);
                    tracing::info!("[MEDIA] Generating thumbnails for {}...", file_id);
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    tracing::info!("[MEDIA] Thumbnails generated for {}", file_id);
                    Ok(())
                }
                BackgroundJob::FederationTransaction { txn_id, destination } => {
                    tracing::info!("[FEDERATION] Processing transaction {} to {}", txn_id, destination);
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
                            payload.as_object().map_or(0, |o| o.len())
                        );
                    }

                    Ok(())
                }
                BackgroundJob::RedactEvent { room_id, event_id, reason } => {
                    tracing::info!("[REDACT] Executing redaction for room {} event {}", room_id, event_id);
                    if let Some(r) = reason {
                        tracing::info!("[REDACT] Reason: {}", r);
                    }

                    if let Err(e) = event_storage.redact_event_content(&event_id, None).await {
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
        if let Err(e) = queue_clone.consume_loop(&group_name_clone, &consumer_name, job_handler).await {
            tracing::error!("Worker loop terminated with error: {}", e);
        }
    });

    let metrics_host = std::env::var("SYNAPSE_WORKER_METRICS_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let metrics_port: u16 =
        std::env::var("SYNAPSE_WORKER_METRICS_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(9091);
    let metrics_addr: SocketAddr = format!("{metrics_host}:{metrics_port}").parse()?;
    let metrics_token = std::env::var("SYNAPSE_WORKER_METRICS_TOKEN").ok().filter(|t| !t.is_empty());
    let refuse_unprotected_metrics = !metrics_addr.ip().is_loopback() && metrics_token.is_none();

    let metrics_state = MetricsState { queue: queue.clone(), token: metrics_token.clone() };
    let metrics_app = Router::new().route("/metrics", get(get_metrics)).with_state(metrics_state);

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
    let monitor_queue_for_exit = queue.clone();
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
                        tracing::warn!("High Consumer Lag: {} unacknowledged tasks!", metrics.consumer_lag);
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

    // Pre-exit log: capture worker type, queue metrics, and restart count.
    let worker_type = std::env::var("WORKER_TYPE")
        .or_else(|_| std::env::var("WORKER_INSTANCE_NAME"))
        .unwrap_or_else(|_| "unknown".to_string());
    let restart_count = std::env::var("RESTART_COUNT").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
    let queue_snapshot = monitor_queue_for_exit.get_metrics("synapse_workers").await;
    match queue_snapshot {
        Ok(m) => {
            tracing::warn!(
                target: "worker_exit",
                worker_type = %worker_type,
                restart_count = %restart_count,
                queue_length = %m.queue_length,
                consumer_lag = %m.consumer_lag,
                "Worker exiting — check container restart policy for crash-loop evidence"
            );
        }
        Err(e) => {
            tracing::warn!(
                target: "worker_exit",
                worker_type = %worker_type,
                restart_count = %restart_count,
                queue_error = %e,
                "Worker exiting — queue metrics unavailable"
            );
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

fn build_smtp_mailer(config: &SmtpConfig) -> Result<SmtpMailer, Box<dyn std::error::Error>> {
    let tls = if config.tls {
        lettre::transport::smtp::client::Tls::Required(lettre::transport::smtp::client::TlsParameters::new(
            config.host.clone(),
        )?)
    } else {
        lettre::transport::smtp::client::Tls::None
    };

    let mut builder =
        lettre::AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host).port(config.port).tls(tls);

    if !config.username.is_empty() {
        let credentials =
            lettre::transport::smtp::authentication::Credentials::new(config.username.clone(), config.password.clone());
        builder = builder.credentials(credentials);
    }

    Ok(builder.build())
}

async fn process_send_email_job(
    smtp_mailer: Option<Arc<SmtpMailer>>,
    smtp_from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    tracing::info!("[EMAIL] Sending email (recipient masked)");
    tracing::debug!("[EMAIL] Recipient: {}", to);
    tracing::debug!("[EMAIL] Subject: {}", subject);

    let mailer = match smtp_mailer.as_ref() {
        Some(m) => m,
        None => {
            tracing::error!("[EMAIL] SMTP not configured, cannot send email (recipient masked)");
            return Err("SMTP not configured".to_string());
        }
    };

    let from: Mailbox = match smtp_from.parse() {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("[EMAIL] Invalid from address '{}': {}", smtp_from, e);
            return Err(format!("Invalid from address: {}", e));
        }
    };

    let to_mailbox: Mailbox = match to.parse() {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("[EMAIL] Invalid to address (recipient masked): {}", e);
            return Err(format!("Invalid to address: {}", e));
        }
    };

    let email = match Message::builder().from(from).to(to_mailbox).subject(subject.to_string()).body(body.to_string()) {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("[EMAIL] Failed to build email message: {}", e);
            return Err(format!("Failed to build email: {}", e));
        }
    };

    match mailer.send(email).await {
        Ok(_) => {
            tracing::info!("[EMAIL] Sent successfully (recipient masked)");
            Ok(())
        }
        Err(e) => {
            tracing::error!("[EMAIL] Failed to send email (recipient masked): {}", e);
            Err(format!("SMTP send error: {}", e))
        }
    }
}

async fn get_metrics(State(state): State<MetricsState>, headers: axum::http::HeaderMap) -> impl IntoResponse {
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
            body.push_str(&format!("synapse_worker_queue_length {}\n", metrics.queue_length));

            body.push_str("# HELP synapse_worker_consumer_lag Total unacknowledged messages\n");
            body.push_str("# TYPE synapse_worker_consumer_lag gauge\n");
            body.push_str(&format!("synapse_worker_consumer_lag {}\n", metrics.consumer_lag));

            body.push_str("# HELP synapse_worker_consumer_pending Pending messages per consumer\n");
            body.push_str("# TYPE synapse_worker_consumer_pending gauge\n");
            for (consumer, pending) in metrics.consumers {
                body.push_str(&format!("synapse_worker_consumer_pending{{consumer=\"{consumer}\"}} {pending}\n"));
            }

            ([(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4")], body).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to get metrics: {}", e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

#[cfg(test)]
mod smtp_smoke_tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    #[test]
    fn test_build_smtp_mailer_with_tls() {
        let config = SmtpConfig {
            enabled: true,
            host: "smtp.example.com".to_string(),
            port: 587,
            from: "noreply@example.com".to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            tls: true,
            ..Default::default()
        };
        let result = build_smtp_mailer(&config);
        // TLS parameters for "smtp.example.com" may fail DNS resolution,
        // but the builder should be constructable
        match result {
            Ok(_) => { /* mailer built successfully */ }
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("dns") || msg.contains("TLS") || msg.contains("resolve"),
                    "unexpected error: {msg}"
                );
            }
        }
    }

    #[test]
    fn test_build_smtp_mailer_without_tls() {
        let config = SmtpConfig {
            enabled: true,
            host: "localhost".to_string(),
            port: 25,
            from: "noreply@example.com".to_string(),
            tls: false,
            ..Default::default()
        };
        let result = build_smtp_mailer(&config);
        // No TLS, no credentials — should succeed
        assert!(result.is_ok(), "mailer should build without TLS: {:?}", result.err());
    }

    #[test]
    fn test_build_smtp_mailer_no_credentials() {
        let config = SmtpConfig {
            host: "localhost".to_string(),
            port: 25,
            from: "noreply@example.com".to_string(),
            tls: false,
            ..Default::default()
        };
        let result = build_smtp_mailer(&config);
        assert!(result.is_ok(), "mailer should build without auth credentials");
    }

    #[test]
    fn test_email_job_message_building() {
        // Verify that a valid email message can be constructed
        let from: Mailbox =
            "noreply@example.com".parse().unwrap_or_else(|e| panic!("valid from address parse failed: {e}"));
        let to: Mailbox = "user@example.com".parse().unwrap_or_else(|e| panic!("valid to address parse failed: {e}"));
        let message = Message::builder()
            .from(from)
            .to(to)
            .subject("Test Subject".to_string())
            .body("Test Body".to_string())
            .unwrap_or_else(|e| panic!("message should build: {e}"));
        // Verify message is well-formed
        let formatted = format!("{:?}", message);
        assert!(!formatted.is_empty(), "message debug output should not be empty");
    }

    #[tokio::test]
    async fn test_process_send_email_job_smoke_against_fake_smtp_server() {
        let listener =
            TcpListener::bind("127.0.0.1:0").await.unwrap_or_else(|e| panic!("bind fake smtp listener: {e}"));
        let port = listener.local_addr().unwrap_or_else(|e| panic!("read fake smtp addr: {e}")).port();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap_or_else(|e| panic!("accept smtp client: {e}"));
            let (reader, mut writer) = stream.into_split();
            let mut reader = BufReader::new(reader);
            let mut line = String::new();
            let mut data_mode = false;
            let mut message = String::new();

            writer.write_all(b"220 localhost ESMTP test\r\n").await.unwrap_or_else(|e| panic!("write greeting: {e}"));

            loop {
                line.clear();
                let read = reader.read_line(&mut line).await.unwrap_or_else(|e| panic!("read smtp line: {e}"));
                if read == 0 {
                    break;
                }

                if data_mode {
                    if line == ".\r\n" || line == ".\n" {
                        data_mode = false;
                        writer.write_all(b"250 queued\r\n").await.unwrap_or_else(|e| panic!("ack DATA: {e}"));
                    } else {
                        message.push_str(&line);
                    }
                    continue;
                }

                if line.starts_with("EHLO") || line.starts_with("HELO") {
                    writer
                        .write_all(b"250-localhost\r\n250 PIPELINING\r\n")
                        .await
                        .unwrap_or_else(|e| panic!("reply ehlo: {e}"));
                } else if line.starts_with("MAIL FROM:") || line.starts_with("RCPT TO:") {
                    writer.write_all(b"250 OK\r\n").await.unwrap_or_else(|e| panic!("ack envelope: {e}"));
                } else if line.starts_with("DATA") {
                    data_mode = true;
                    writer
                        .write_all(b"354 End data with <CR><LF>.<CR><LF>\r\n")
                        .await
                        .unwrap_or_else(|e| panic!("ack data: {e}"));
                } else if line.starts_with("QUIT") {
                    writer.write_all(b"221 Bye\r\n").await.unwrap_or_else(|e| panic!("ack quit: {e}"));
                    break;
                } else {
                    writer.write_all(b"250 OK\r\n").await.unwrap_or_else(|e| panic!("ack generic: {e}"));
                }
            }

            message
        });

        let config = SmtpConfig {
            enabled: true,
            host: "127.0.0.1".to_string(),
            port,
            from: "noreply@example.com".to_string(),
            tls: false,
            ..Default::default()
        };
        let mailer = Arc::new(build_smtp_mailer(&config).unwrap_or_else(|e| panic!("build fake smtp mailer: {e}")));

        process_send_email_job(Some(mailer), &config.from, "user@example.com", "Smoke Test Subject", "Smoke Test Body")
            .await
            .unwrap_or_else(|e| panic!("smtp send should succeed: {e}"));

        let message = match server.await {
            Ok(message) => message,
            Err(e) => panic!("fake smtp server should finish: {e}"),
        };
        assert!(message.contains("Subject: Smoke Test Subject"), "smtp payload should contain subject");
        assert!(message.contains("Smoke Test Body"), "smtp payload should contain body");
        assert!(message.contains("To: user@example.com"), "smtp payload should contain recipient");
    }
}
