use super::models::*;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

pub struct WebhookNotifier {
    config: WebhookConfig,
    sender: Option<mpsc::Sender<WebhookEvent>>,
}

impl WebhookNotifier {
    pub fn new(config: WebhookConfig) -> Self {
        let (sender, receiver) = mpsc::channel(100);
        let enabled = config.enabled;
        
        if enabled {
            let http_client = reqwest::Client::new();
            let config_clone = config.clone();
            
            tokio::spawn(async move {
                Self::event_processor(http_client, config_clone, receiver).await;
            });
        }

        Self {
            config,
            sender: if enabled { Some(sender) } else { None },
        }
    }

    async fn event_processor(
        http_client: reqwest::Client,
        config: WebhookConfig,
        mut receiver: mpsc::Receiver<WebhookEvent>,
    ) {
        while let Some(event) = receiver.recv().await {
            if !config.events.contains(&event.event_type) {
                continue;
            }

            let result = Self::deliver_with_retry(&http_client, &config, &event).await;
            
            if !result.success {
                tracing::warn!(
                    "Webhook delivery failed after {} attempts: {:?}",
                    result.attempts, result.error_message
                );
            }
        }
    }

    async fn deliver_with_retry(
        http_client: &reqwest::Client,
        config: &WebhookConfig,
        event: &WebhookEvent,
    ) -> WebhookDeliveryResult {
        let mut last_error = None;

        for attempt in 1..=config.retry_count {
            match Self::deliver(http_client, config, event).await {
                Ok(result) => return result,
                Err(e) => {
                    last_error = Some(e);
                    if attempt < config.retry_count {
                        sleep(Duration::from_millis(config.retry_delay_ms)).await;
                    }
                }
            }
        }

        WebhookDeliveryResult {
            success: false,
            status_code: None,
            response_body: None,
            attempts: config.retry_count,
            error_message: last_error,
        }
    }

    async fn deliver(
        http_client: &reqwest::Client,
        config: &WebhookConfig,
        event: &WebhookEvent,
    ) -> Result<WebhookDeliveryResult, String> {
        let mut request = http_client
            .post(&config.url)
            .timeout(Duration::from_millis(config.timeout_ms))
            .json(event);

        if let Some(ref secret) = config.secret {
            request = request.header("X-Webhook-Secret", secret);
        }

        let response = request.send().await
            .map_err(|e| e.to_string())?;

        let status = response.status();
        let body = response.text().await.ok();

        if status.is_success() {
            Ok(WebhookDeliveryResult {
                success: true,
                status_code: Some(status.as_u16()),
                response_body: body,
                attempts: 1,
                error_message: None,
            })
        } else {
            Err(format!("HTTP {}: {:?}", status.as_u16(), body))
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub async fn notify(&self, event: WebhookEvent) {
        if let Some(ref sender) = self.sender {
            let _ = sender.send(event).await;
        }
    }

    pub async fn notify_login(&self, user_id: &str, device_id: Option<&str>, ip: Option<&str>, user_agent: Option<&str>) {
        self.notify(WebhookEvent {
            event_type: WebhookEventType::UserLogin,
            timestamp: chrono::Utc::now().timestamp_millis(),
            payload: WebhookPayload {
                user_id: user_id.to_string(),
                device_id: device_id.map(String::from),
                ip_address: ip.map(String::from),
                user_agent: user_agent.map(String::from),
                country: None,
                city: None,
                extra: None,
            },
        }).await;
    }

    pub async fn notify_logout(&self, user_id: &str, device_id: Option<&str>) {
        self.notify(WebhookEvent {
            event_type: WebhookEventType::UserLogout,
            timestamp: chrono::Utc::now().timestamp_millis(),
            payload: WebhookPayload {
                user_id: user_id.to_string(),
                device_id: device_id.map(String::from),
                ip_address: None,
                user_agent: None,
                country: None,
                city: None,
                extra: None,
            },
        }).await;
    }

    pub async fn notify_failed_login(&self, user_id: &str, ip: &str, reason: &str) {
        self.notify(WebhookEvent {
            event_type: WebhookEventType::UserFailedLogin,
            timestamp: chrono::Utc::now().timestamp_millis(),
            payload: WebhookPayload {
                user_id: user_id.to_string(),
                device_id: None,
                ip_address: Some(ip.to_string()),
                user_agent: None,
                country: None,
                city: None,
                extra: Some(serde_json::json!({ "reason": reason })),
            },
        }).await;
    }
}
