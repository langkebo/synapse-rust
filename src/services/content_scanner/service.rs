use super::models::*;
use crate::error::ApiError;
use tokio::time::{timeout, Duration};

pub struct ContentScanner {
    config: ContentScannerConfig,
    http_client: reqwest::Client,
}

impl ContentScanner {
    pub fn new(config: ContentScannerConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled && self.config.scanner_type != ScannerType::Disabled
    }

    pub async fn scan(&self, request: ScanRequest) -> Result<ContentScanResult, ApiError> {
        if !self.is_enabled() {
            return Ok(ContentScanResult {
                safe: true,
                threat_type: None,
                threat_message: None,
                scan_timestamp: chrono::Utc::now().timestamp_millis(),
            });
        }

        match self.config.scanner_type {
            ScannerType::ClamAv => self.scan_with_clamav(&request).await,
            ScannerType::Webhook => self.scan_with_webhook(&request).await,
            ScannerType::Disabled => Ok(ContentScanResult {
                safe: true,
                threat_type: None,
                threat_message: None,
                scan_timestamp: chrono::Utc::now().timestamp_millis(),
            }),
        }
    }

    async fn scan_with_clamav(&self, request: &ScanRequest) -> Result<ContentScanResult, ApiError> {
        let data = request.data.clone();
        
        let result = tokio::task::spawn_blocking(move || {
            Self::clamav_scan_sync(&data)
        }).await
        .map_err(|e| ApiError::internal(format!("Task join error: {}", e)))?;

        result
    }

    fn clamav_scan_sync(data: &[u8]) -> Result<ContentScanResult, ApiError> {
        use std::io::{BufRead, BufReader, BufWriter, Write};
        
        let socket_path = "/var/run/clamav/clamd.sock";
        
        let stream = std::net::TcpStream::connect(socket_path)
            .map_err(|e| ApiError::internal(format!("Failed to connect to ClamAV: {}", e)))?;
        
        stream.set_read_timeout(Some(std::time::Duration::from_secs(30)))
            .map_err(|e| ApiError::internal(format!("Failed to set timeout: {}", e)))?;

        let mut reader = BufReader::new(&stream);
        let mut writer = BufWriter::new(&stream);

        writer.write_all(b"zINSTREAM\0")
            .map_err(|e| ApiError::internal(format!("Failed to send INSTREAM: {}", e)))?;
        
        let chunk_size = 1024 * 1024;
        let mut remaining = data;
        
        while !remaining.is_empty() {
            let to_send = std::cmp::min(remaining.len(), chunk_size);
            let chunk = &remaining[..to_send];
            
            let mut length_buf = [0u8; 4];
            length_buf[0..4].copy_from_slice(&(to_send as u32).to_be_bytes());
            
            writer.write_all(&length_buf)
                .map_err(|e| ApiError::internal(format!("Failed to send length: {}", e)))?;
            writer.write_all(chunk)
                .map_err(|e| ApiError::internal(format!("Failed to send chunk: {}", e)))?;
            
            remaining = &remaining[to_send..];
        }
        
        writer.write_all(&[0, 0, 0, 0])
            .map_err(|e| ApiError::internal(format!("Failed to send terminator: {}", e)))?;
        writer.flush()
            .map_err(|e| ApiError::internal(format!("Failed to flush: {}", e)))?;

        let mut response = String::new();
        reader.read_line(&mut response)
            .map_err(|e| ApiError::internal(format!("Failed to read response: {}", e)))?;

        let is_safe = response.starts_with("stream: OK");

        Ok(ContentScanResult {
            safe: is_safe,
            threat_type: if is_safe { None } else { Some("virus".to_string()) },
            threat_message: if is_safe { None } else { Some(response.trim().to_string()) },
            scan_timestamp: chrono::Utc::now().timestamp_millis(),
        })
    }

    async fn scan_with_webhook(&self, request: &ScanRequest) -> Result<ContentScanResult, ApiError> {
        let webhook_url = self.config.webhook_url.as_ref()
            .ok_or_else(|| ApiError::internal("Webhook URL not configured".to_string()))?;

        let scan_request = WebhookScanRequest {
            content_id: request.content_id.clone(),
            content_type: request.content_type.as_str().to_string(),
            file_name: None,
            file_size: request.data.len() as u64,
            checksum: None,
        };

        let mut req_builder = self.http_client
            .post(webhook_url)
            .json(&scan_request);

        if let Some(ref secret) = self.config.webhook_secret {
            req_builder = req_builder.header("X-Webhook-Secret", secret);
        }

        let response = timeout(
            Duration::from_millis(self.config.scan_timeout_ms),
            req_builder.send()
        ).await
        .map_err(|_| ApiError::internal("Webhook request timeout".to_string()))?
        .map_err(|e| ApiError::internal(format!("Webhook request failed: {}", e)))?;

        if !response.status().is_success() {
            if self.config.block_on_scan_failure {
                return Err(ApiError::internal(format!(
                    "Webhook scan failed with status: {}",
                    response.status()
                )));
            } else {
                return Ok(ContentScanResult {
                    safe: true,
                    threat_type: None,
                    threat_message: Some("Scan service unavailable".to_string()),
                    scan_timestamp: chrono::Utc::now().timestamp_millis(),
                });
            }
        }

        let scan_response: WebhookScanResponse = response.json().await
            .map_err(|e| ApiError::internal(format!("Failed to parse webhook response: {}", e)))?;

        Ok(ContentScanResult {
            safe: scan_response.safe,
            threat_type: scan_response.threat_type,
            threat_message: scan_response.threat_message,
            scan_timestamp: chrono::Utc::now().timestamp_millis(),
        })
    }

    pub async fn scan_text(&self, content_id: &str, text: &str) -> Result<ContentScanResult, ApiError> {
        self.scan(ScanRequest {
            content_id: content_id.to_string(),
            content_type: ContentType::MessageText,
            data: text.as_bytes().to_vec(),
        }).await
    }

    pub async fn scan_media(&self, content_id: &str, data: Vec<u8>, media_type: ContentType) -> Result<ContentScanResult, ApiError> {
        self.scan(ScanRequest {
            content_id: content_id.to_string(),
            content_type: media_type,
            data,
        }).await
    }
}
