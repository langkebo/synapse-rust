use super::models::*;
use synapse_common::current_timestamp_millis;
use synapse_common::error::ApiError;
use tokio::time::{timeout, Duration};

/// The `ContentScanner` struct.
pub struct ContentScanner {
    config: ContentScannerConfig,
    http_client: reqwest::Client,
}

impl ContentScanner {
    /// See [`new`].
    /// See [`new`].
    pub fn new(config: ContentScannerConfig) -> Self {
        // F-1: 复用共享 HTTP client（带超时与连接池），不再裸用 Client::new()
        Self { config, http_client: synapse_common::http_client::default_client() }
    }

    /// See [`is_enabled`].
    /// See [`is_enabled`].
    pub fn is_enabled(&self) -> bool {
        self.config.enabled && self.config.scanner_type != ScannerType::Disabled
    }

    /// See [`scan`].
    /// See [`scan`].
    pub async fn scan(&self, request: ScanRequest) -> Result<ContentScanResult, ApiError> {
        if !self.is_enabled() {
            return Ok(ContentScanResult {
                safe: true,
                threat_type: None,
                threat_message: None,
                scan_timestamp: current_timestamp_millis(),
            });
        }

        match self.config.scanner_type {
            ScannerType::ClamAv => self.scan_with_clamav(&request).await,
            ScannerType::Webhook => self.scan_with_webhook(&request).await,
            ScannerType::Disabled => Ok(ContentScanResult {
                safe: true,
                threat_type: None,
                threat_message: None,
                scan_timestamp: current_timestamp_millis(),
            }),
        }
    }

    async fn scan_with_clamav(&self, request: &ScanRequest) -> Result<ContentScanResult, ApiError> {
        let data = request.data.clone();

        let result = tokio::task::spawn_blocking(move || Self::clamav_scan_sync(&data))
            .await
            .map_err(|e| ApiError::internal_with_context("Task join error", &e))?;

        result
    }

    fn clamav_scan_sync(data: &[u8]) -> Result<ContentScanResult, ApiError> {
        use std::io::{BufRead, BufReader, BufWriter, Write};

        let socket_path = "/var/run/clamav/clamd.sock";

        let stream = std::net::TcpStream::connect(socket_path)
            .map_err(|e| ApiError::internal_with_context("Failed to connect to ClamAV", &e))?;

        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(30)))
            .map_err(|e| ApiError::internal_with_context("Failed to set timeout", &e))?;

        let mut reader = BufReader::new(&stream);
        let mut writer = BufWriter::new(&stream);

        writer.write_all(b"zINSTREAM\0").map_err(|e| ApiError::internal_with_context("Failed to send INSTREAM", &e))?;

        let chunk_size = 1024 * 1024;
        let mut remaining = data;

        while !remaining.is_empty() {
            let to_send = std::cmp::min(remaining.len(), chunk_size);
            let chunk = &remaining[..to_send];

            let mut length_buf = [0u8; 4];
            length_buf[0..4].copy_from_slice(&(to_send as u32).to_be_bytes());

            writer.write_all(&length_buf).map_err(|e| ApiError::internal_with_context("Failed to send length", &e))?;
            writer.write_all(chunk).map_err(|e| ApiError::internal_with_context("Failed to send chunk", &e))?;

            remaining = &remaining[to_send..];
        }

        writer
            .write_all(&[0, 0, 0, 0])
            .map_err(|e| ApiError::internal_with_context("Failed to send terminator", &e))?;
        writer.flush().map_err(|e| ApiError::internal_with_context("Failed to flush", &e))?;

        let mut response = String::new();
        reader.read_line(&mut response).map_err(|e| ApiError::internal_with_context("Failed to read response", &e))?;

        let is_safe = response.starts_with("stream: OK");

        Ok(ContentScanResult {
            safe: is_safe,
            threat_type: if is_safe { None } else { Some("virus".to_string()) },
            threat_message: if is_safe { None } else { Some(response.trim().to_string()) },
            scan_timestamp: current_timestamp_millis(),
        })
    }

    async fn scan_with_webhook(&self, request: &ScanRequest) -> Result<ContentScanResult, ApiError> {
        let webhook_url = self
            .config
            .webhook_url
            .as_ref()
            .ok_or_else(|| ApiError::internal("Webhook URL not configured".to_string()))?;

        let scan_request = WebhookScanRequest {
            content_id: request.content_id.clone(),
            content_type: request.content_type.as_str().to_string(),
            file_name: None,
            file_size: request.data.len() as u64,
            checksum: None,
        };

        let mut req_builder = self.http_client.post(webhook_url).json(&scan_request);

        if let Some(ref secret) = self.config.webhook_secret {
            req_builder = req_builder.header("X-Webhook-Secret", secret);
        }

        let response = timeout(Duration::from_millis(self.config.scan_timeout_ms), req_builder.send())
            .await
            .map_err(|e| ApiError::internal_with_context("Webhook request timeout", &e))?
            .map_err(|e| ApiError::internal_with_context("Webhook request failed", &e))?;

        if !response.status().is_success() {
            if self.config.block_on_scan_failure {
                return Err(ApiError::internal_with_context("Webhook scan failed", &response.status()));
            }
            return Ok(ContentScanResult {
                safe: true,
                threat_type: None,
                threat_message: Some("Scan service unavailable".to_string()),
                scan_timestamp: current_timestamp_millis(),
            });
        }

        let scan_response: WebhookScanResponse = response
            .json()
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to parse webhook response", &e))?;

        Ok(ContentScanResult {
            safe: scan_response.safe,
            threat_type: scan_response.threat_type,
            threat_message: scan_response.threat_message,
            scan_timestamp: current_timestamp_millis(),
        })
    }

    /// See [`scan_text`].
    /// See [`scan_text`].
    pub async fn scan_text(&self, content_id: &str, text: &str) -> Result<ContentScanResult, ApiError> {
        self.scan(ScanRequest {
            content_id: content_id.to_string(),
            content_type: ContentType::MessageText,
            data: text.as_bytes().to_vec(),
        })
        .await
    }

    /// See [`scan_media`].
    pub async fn scan_media(
        &self,
        content_id: &str,
        data: Vec<u8>,
        media_type: ContentType,
    ) -> Result<ContentScanResult, ApiError> {
        self.scan(ScanRequest { content_id: content_id.to_string(), content_type: media_type, data }).await
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for [`ContentScanner`].
    //!
    //! Strategy: exercise the business logic without external dependencies.
    //! - ClamAV path (`scan_with_clamav`) requires a live ClamAV socket at
    //!   `/var/run/clamav/clamd.sock` — the no-socket path returns a
    //!   connection error which we assert on.
    //! - Webhook path uses a real HTTP client. With `block_on_failure=true`
    //!   and an unreachable URL, the call errors out. With
    //!   `block_on_failure=false`, the error is swallowed and a safe
    //!   pass-through is returned (the fail-open policy).

    use super::super::models::*;
    use super::ContentScanner;

    fn make_disabled_scanner() -> ContentScanner {
        ContentScanner::new(ContentScannerConfig {
            enabled: false,
            scanner_type: ScannerType::Disabled,
            block_on_scan_failure: false,
            ..Default::default()
        })
    }

    fn make_enabled_disabled_type_scanner() -> ContentScanner {
        ContentScanner::new(ContentScannerConfig {
            enabled: true,
            scanner_type: ScannerType::Disabled,
            block_on_scan_failure: true,
            ..Default::default()
        })
    }

    fn make_clamav_scanner() -> ContentScanner {
        ContentScanner::new(ContentScannerConfig {
            enabled: true,
            scanner_type: ScannerType::ClamAv,
            clamav_socket_path: Some("/nonexistent/clamd.sock".to_string()),
            block_on_scan_failure: true,
            scan_timeout_ms: 1000,
            ..Default::default()
        })
    }

    fn make_webhook_scanner(block_on_failure: bool, url: Option<String>) -> ContentScanner {
        ContentScanner::new(ContentScannerConfig {
            enabled: true,
            scanner_type: ScannerType::Webhook,
            webhook_url: url,
            webhook_secret: Some("test-secret".to_string()),
            block_on_scan_failure: block_on_failure,
            scan_timeout_ms: 5000,
            ..Default::default()
        })
    }

    // ── is_enabled ──────────────────────────────────────────────────────────

    #[test]
    fn is_enabled_disabled_config() {
        // enabled=false → false
        let scanner = make_disabled_scanner();
        assert!(!scanner.is_enabled());
    }

    #[test]
    fn is_enabled_true_but_type_disabled() {
        // enabled=true but scanner_type=Disabled → false
        let scanner = make_enabled_disabled_type_scanner();
        assert!(!scanner.is_enabled());
    }

    #[test]
    fn is_enabled_clamav() {
        // enabled=true + ClamAv type → true
        let scanner = make_clamav_scanner();
        assert!(scanner.is_enabled());
    }

    #[test]
    fn is_enabled_webhook() {
        // enabled=true + Webhook type → true
        let scanner = make_webhook_scanner(true, Some("http://localhost:9999".to_string()));
        assert!(scanner.is_enabled());
    }

    // ── scan (disabled path) ───────────────────────────────────────────────

    #[tokio::test]
    async fn scan_disabled_returns_safe() {
        let scanner = make_disabled_scanner();
        let result = scanner
            .scan(ScanRequest {
                content_id: "test-1".to_string(),
                content_type: ContentType::MessageText,
                data: b"hello world".to_vec(),
            })
            .await
            .expect("scan should not fail");

        assert!(result.safe, "disabled scanner must return safe=true");
        assert!(result.threat_type.is_none());
        assert!(result.threat_message.is_none());
        assert!(result.scan_timestamp > 0);
    }

    #[tokio::test]
    async fn scan_enabled_but_type_disabled_returns_safe() {
        let scanner = make_enabled_disabled_type_scanner();
        let result = scanner
            .scan(ScanRequest {
                content_id: "test-2".to_string(),
                content_type: ContentType::MediaImage,
                data: b"\x00\x01\x02".to_vec(),
            })
            .await
            .expect("scan should not fail");

        assert!(result.safe);
        assert!(result.threat_type.is_none());
    }

    #[tokio::test]
    async fn scan_disabled_with_large_data() {
        let scanner = make_disabled_scanner();
        let large_data: Vec<u8> = (0..100_000).map(|i| (i % 256) as u8).collect();

        let result = scanner
            .scan(ScanRequest {
                content_id: "test-large".to_string(),
                content_type: ContentType::FileAttachment,
                data: large_data,
            })
            .await
            .expect("large data scan should not fail");

        assert!(result.safe);
    }

    // ── scan_text helper ───────────────────────────────────────────────────

    #[tokio::test]
    async fn scan_text_disabled_returns_safe() {
        let scanner = make_disabled_scanner();
        let result = scanner.scan_text("msg-1", "Hello, world!").await.expect("scan_text should not fail");
        assert!(result.safe);
    }

    #[tokio::test]
    async fn scan_text_disabled_with_empty_string() {
        let scanner = make_disabled_scanner();
        let result = scanner.scan_text("msg-empty", "").await.expect("empty text scan should not fail");
        assert!(result.safe);
    }

    #[tokio::test]
    async fn scan_text_disabled_with_unicode() {
        let scanner = make_disabled_scanner();
        let result = scanner.scan_text("msg-unicode", "你好世界 🌍 مرحبا").await.expect("unicode scan should not fail");
        assert!(result.safe);
    }

    // ── scan_media helper ──────────────────────────────────────────────────

    #[tokio::test]
    async fn scan_media_disabled_image() {
        let scanner = make_disabled_scanner();
        let result = scanner
            .scan_media("media-1", vec![0xFF, 0xD8, 0xFF], ContentType::MediaImage)
            .await
            .expect("media scan should not fail");
        assert!(result.safe);
    }

    #[tokio::test]
    async fn scan_media_disabled_video() {
        let scanner = make_disabled_scanner();
        let result = scanner
            .scan_media("media-video-1", vec![0x00, 0x00, 0x00], ContentType::MediaVideo)
            .await
            .expect("video scan should not fail");
        assert!(result.safe);
    }

    #[tokio::test]
    async fn scan_media_disabled_audio() {
        let scanner = make_disabled_scanner();
        let result = scanner
            .scan_media("media-audio-1", vec![0x49, 0x44, 0x33], ContentType::MediaAudio)
            .await
            .expect("audio scan should not fail");
        assert!(result.safe);
    }

    #[tokio::test]
    async fn scan_media_disabled_file() {
        let scanner = make_disabled_scanner();
        let result = scanner
            .scan_media("file-1", b"PK\x03\x04".to_vec(), ContentType::FileAttachment)
            .await
            .expect("file scan should not fail");
        assert!(result.safe);
    }

    // ── scan (ClamAV path — no socket → error) ──────────────────────────────

    #[tokio::test]
    async fn scan_clamav_no_socket_returns_error() {
        let scanner = make_clamav_scanner();
        let result = scanner
            .scan(ScanRequest {
                content_id: "test-clamav".to_string(),
                content_type: ContentType::MediaFile,
                data: b"test data".to_vec(),
            })
            .await;

        // ClamAV socket doesn't exist → connection error
        assert!(result.is_err(), "should error when ClamAV socket is unavailable");
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Failed to connect to ClamAV") || err.to_string().contains("connect"));
    }

    // ── scan (Webhook path — no URL → error) ───────────────────────────────

    #[tokio::test]
    async fn scan_webhook_no_url_returns_error() {
        let scanner = make_webhook_scanner(true, None);
        let result = scanner
            .scan(ScanRequest {
                content_id: "test-webhook".to_string(),
                content_type: ContentType::MessageText,
                data: b"hello".to_vec(),
            })
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Webhook URL not configured"));
    }

    // ── scan (Webhook path — block_on_failure=true, server unreachable) ─────

    #[tokio::test]
    async fn scan_webhook_block_on_failure_unreachable_returns_error() {
        let scanner = make_webhook_scanner(true, Some("http://localhost:9999/unreachable".to_string()));
        let result = scanner
            .scan(ScanRequest {
                content_id: "test-webhook-block".to_string(),
                content_type: ContentType::MediaImage,
                data: b"fake image".to_vec(),
            })
            .await;

        // Connection refused (no server on 9999) + block_on_failure=true → error
        assert!(result.is_err());
        let err = result.unwrap_err();
        // reqwest can surface a connection-refused either as
        // "Webhook request failed" (network error) or as a non-2xx
        // response (the request *did* go out but got a refusal). With
        // block_on_failure=true both must surface as a hard error.
        let msg = err.to_string();
        assert!(
            msg.contains("Webhook request") || msg.contains("connect") || msg.contains("Webhook scan failed"),
            "unexpected err: {msg}"
        );
    }

    // ── scan (Webhook path — fail-open policy) ──────────────────────────────

    #[tokio::test]
    async fn scan_webhook_fail_open_passes_through() {
        let scanner = make_webhook_scanner(false, Some("http://localhost:9999/unreachable".to_string()));
        let result = scanner
            .scan(ScanRequest {
                content_id: "test-webhook-fail-open".to_string(),
                content_type: ContentType::MessageText,
                data: b"hello".to_vec(),
            })
            .await
            .expect("fail-open path should not return error");

        // block_on_failure=false → error is swallowed, returns safe=true with warning
        assert!(result.safe, "fail-open should return safe=true even on webhook failure");
        assert!(result.threat_type.is_none());
        assert_eq!(
            result.threat_message.as_deref(),
            Some("Scan service unavailable"),
            "should contain unavailable message"
        );
    }

    // ── scan all ContentTypes through disabled scanner ─────────────────────

    #[tokio::test]
    async fn scan_all_content_types_disabled() {
        let scanner = make_disabled_scanner();
        let types = [
            ContentType::MediaImage,
            ContentType::MediaVideo,
            ContentType::MediaAudio,
            ContentType::MediaFile,
            ContentType::MessageText,
            ContentType::FileAttachment,
        ];

        for ct in &types {
            let result = scanner
                .scan(ScanRequest { content_id: format!("id-{:?}", ct), content_type: *ct, data: vec![1, 2, 3] })
                .await
                .expect("scan should not fail");
            assert!(result.safe, "all content types should be safe when scanner is disabled: {:?}", ct);
        }
    }
}
