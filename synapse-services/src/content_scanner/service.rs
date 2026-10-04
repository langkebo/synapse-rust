use super::models::*;
use synapse_common::current_timestamp_millis;
use synapse_common::error::ApiError;
use tokio::time::{timeout, Duration};

/// ClamAV 默认 socket 路径，仅在 `ContentScannerConfig::clamav_socket_path` 未配置时使用。
const DEFAULT_CLAMAV_SOCKET_PATH: &str = "/var/run/clamav/clamd.sock";

/// The `ContentScanner` struct.
pub struct ContentScanner {
    config: synapse_common::content_scanner::ContentScannerConfig,
    http_client: reqwest::Client,
}

impl ContentScanner {
    /// See [`new`].
    pub fn new(config: synapse_common::content_scanner::ContentScannerConfig) -> Self {
        // F-1: 复用共享 HTTP client（带超时与连接池），不再裸用 Client::new()
        Self { config, http_client: synapse_common::http_client::default_client() }
    }

    /// See [`is_enabled`].
    pub fn is_enabled(&self) -> bool {
        self.config.enabled && self.config.scanner_type != ScannerType::Disabled
    }

    /// See [`scan`].
    pub async fn scan(&self, request: ScanRequest) -> Result<ContentScanResult, ApiError> {
        if !self.is_enabled() {
            return Err(ApiError::content_scan_disabled("Content scan service is disabled"));
        }

        match self.config.scanner_type {
            ScannerType::ClamAv => self.scan_with_clamav(&request).await,
            ScannerType::Webhook => self.scan_with_webhook(&request).await,
            ScannerType::Disabled => Err(ApiError::content_scan_disabled("Content scan service is disabled")),
        }
    }

    async fn scan_with_clamav(&self, request: &ScanRequest) -> Result<ContentScanResult, ApiError> {
        let data = request.data.clone();
        // 从配置透传 socket 路径：非默认部署路径下若仍用硬编码值，会 fail-closed
        // 误拒媒体上传。
        let socket_path =
            self.config.clamav_socket_path.clone().unwrap_or_else(|| DEFAULT_CLAMAV_SOCKET_PATH.to_string());

        // Fail-closed: any ClamAV transport/protocol failure maps to
        // M_CONTENT_SCAN_FAILED, exactly like the webhook path.
        let result = tokio::task::spawn_blocking(move || Self::clamav_scan_sync(&data, &socket_path))
            .await
            .map_err(|e| ApiError::internal_with_cause("Task join error", e));

        match result {
            Ok(inner) => inner.map_err(|e| self.on_scan_failure(e)),
            Err(e) => Err(self.on_scan_failure(e)),
        }
    }

    fn clamav_scan_sync(data: &[u8], socket_path: &str) -> Result<ContentScanResult, ApiError> {
        use std::io::{BufRead, BufReader, BufWriter, Write};

        let stream = std::net::TcpStream::connect(socket_path)
            .map_err(|e| ApiError::internal_with_cause("Failed to connect to ClamAV", e))?;

        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(30)))
            .map_err(|e| ApiError::internal_with_cause("Failed to set timeout", e))?;

        let mut reader = BufReader::new(&stream);
        let mut writer = BufWriter::new(&stream);

        writer.write_all(b"zINSTREAM\0").map_err(|e| ApiError::internal_with_cause("Failed to send INSTREAM", e))?;

        let chunk_size = 1024 * 1024;
        let mut remaining = data;

        while !remaining.is_empty() {
            let to_send = std::cmp::min(remaining.len(), chunk_size);
            let chunk = &remaining[..to_send];

            let mut length_buf = [0u8; 4];
            length_buf[0..4].copy_from_slice(&(to_send as u32).to_be_bytes());

            writer.write_all(&length_buf).map_err(|e| ApiError::internal_with_cause("Failed to send length", e))?;
            writer.write_all(chunk).map_err(|e| ApiError::internal_with_cause("Failed to send chunk", e))?;

            remaining = &remaining[to_send..];
        }

        writer.write_all(&[0, 0, 0, 0]).map_err(|e| ApiError::internal_with_cause("Failed to send terminator", e))?;
        writer.flush().map_err(|e| ApiError::internal_with_cause("Failed to flush", e))?;

        let mut response = String::new();
        reader.read_line(&mut response).map_err(|e| ApiError::internal_with_cause("Failed to read response", e))?;

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

        let response = match timeout(Duration::from_millis(self.config.scan_timeout_ms), req_builder.send()).await {
            Ok(Ok(response)) => response,
            Ok(Err(e)) => return Err(self.on_scan_failure(ApiError::internal_with_cause("Webhook request failed", e))),
            Err(e) => return Err(self.on_scan_failure(ApiError::internal_with_cause("Webhook request timeout", e))),
        };

        if !response.status().is_success() {
            return Err(
                self.on_scan_failure(ApiError::internal_with_context("Webhook scan failed", &response.status()))
            );
        }

        let scan_response: WebhookScanResponse = match response.json().await {
            Ok(parsed) => parsed,
            Err(e) => {
                return Err(self.on_scan_failure(ApiError::internal_with_cause("Failed to parse webhook response", e)))
            }
        };

        Ok(ContentScanResult {
            safe: scan_response.safe,
            threat_type: scan_response.threat_type,
            threat_message: scan_response.threat_message,
            scan_timestamp: current_timestamp_millis(),
        })
    }

    /// Maps a scan failure to `M_CONTENT_SCAN_FAILED` (fail-closed).
    /// All scanner types — ClamAV and webhook — must route through here.
    fn on_scan_failure(&self, source: ApiError) -> ApiError {
        tracing::error!(%source, "Content scan failed");
        ApiError::content_scan_failed(source.to_string())
    }

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
    //! - Webhook path uses a real HTTP client. Any failure (transport error,
    //!   timeout, non-2xx status, unparsable body) returns `M_CONTENT_SCAN_FAILED`
    //!   and blocks content (fail-closed policy — `block_on_scan_failure` is ignored).

    use super::super::models::*;
    use super::ContentScanner;

    fn make_disabled_scanner() -> ContentScanner {
        ContentScanner::new(synapse_common::content_scanner::ContentScannerConfig {
            enabled: false,
            scanner_type: ScannerType::Disabled,
            block_on_scan_failure: false,
            ..Default::default()
        })
    }

    fn make_enabled_disabled_type_scanner() -> ContentScanner {
        ContentScanner::new(synapse_common::content_scanner::ContentScannerConfig {
            enabled: true,
            scanner_type: ScannerType::Disabled,
            block_on_scan_failure: true,
            ..Default::default()
        })
    }

    fn make_clamav_scanner() -> ContentScanner {
        ContentScanner::new(synapse_common::content_scanner::ContentScannerConfig {
            enabled: true,
            scanner_type: ScannerType::ClamAv,
            clamav_socket_path: Some("/nonexistent/clamd.sock".to_string()),
            block_on_scan_failure: true,
            scan_timeout_ms: 1000,
            ..Default::default()
        })
    }

    fn make_webhook_scanner(block_on_failure: bool, url: Option<String>) -> ContentScanner {
        ContentScanner::new(synapse_common::content_scanner::ContentScannerConfig {
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
    async fn scan_disabled_returns_error() {
        let scanner = make_disabled_scanner();
        let result = scanner
            .scan(ScanRequest {
                content_id: "test-1".to_string(),
                content_type: ContentType::MessageText,
                data: b"hello world".to_vec(),
            })
            .await;

        // disabled scanner must return Err with M_CONTENT_SCAN_DISABLED
        assert!(result.is_err(), "disabled scanner must return error");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("M_CONTENT_SCAN_DISABLED")
                || err.to_string().contains("Content scan service is disabled"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn scan_enabled_but_type_disabled_returns_error() {
        let scanner = make_enabled_disabled_type_scanner();
        let result = scanner
            .scan(ScanRequest {
                content_id: "test-2".to_string(),
                content_type: ContentType::MediaImage,
                data: b"\x00\x01\x02".to_vec(),
            })
            .await;

        assert!(result.is_err(), "scanner_type=Disabled must return error");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("M_CONTENT_SCAN_DISABLED")
                || err.to_string().contains("Content scan service is disabled"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn scan_disabled_with_large_data_returns_error() {
        let scanner = make_disabled_scanner();
        let large_data: Vec<u8> = (0..100_000).map(|i| (i % 256) as u8).collect();

        let result = scanner
            .scan(ScanRequest {
                content_id: "test-large".to_string(),
                content_type: ContentType::FileAttachment,
                data: large_data,
            })
            .await;

        assert!(result.is_err(), "disabled scanner must return error on large data");
    }

    // ── scan_text helper ───────────────────────────────────────────────────

    #[tokio::test]
    async fn scan_text_disabled_returns_error() {
        let scanner = make_disabled_scanner();
        let result = scanner.scan_text("msg-1", "Hello, world!").await;
        assert!(result.is_err(), "disabled scanner must return error from scan_text");
    }

    #[tokio::test]
    async fn scan_text_disabled_with_empty_string_returns_error() {
        let scanner = make_disabled_scanner();
        let result = scanner.scan_text("msg-empty", "").await;
        assert!(result.is_err(), "disabled scanner must return error on empty text");
    }

    #[tokio::test]
    async fn scan_text_disabled_with_unicode_returns_error() {
        let scanner = make_disabled_scanner();
        let result = scanner.scan_text("msg-unicode", "你好世界 🌍 مرحبا").await;
        assert!(result.is_err(), "disabled scanner must return error on unicode text");
    }

    // ── scan_media helper ──────────────────────────────────────────────────

    #[tokio::test]
    async fn scan_media_disabled_image_returns_error() {
        let scanner = make_disabled_scanner();
        let result = scanner.scan_media("media-1", vec![0xFF, 0xD8, 0xFF], ContentType::MediaImage).await;
        assert!(result.is_err(), "disabled scanner must return error from scan_media");
    }

    #[tokio::test]
    async fn scan_media_disabled_video_returns_error() {
        let scanner = make_disabled_scanner();
        let result = scanner.scan_media("media-video-1", vec![0x00, 0x00, 0x00], ContentType::MediaVideo).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn scan_media_disabled_audio_returns_error() {
        let scanner = make_disabled_scanner();
        let result = scanner.scan_media("media-audio-1", vec![0x49, 0x44, 0x33], ContentType::MediaAudio).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn scan_media_disabled_file_returns_error() {
        let scanner = make_disabled_scanner();
        let result = scanner.scan_media("file-1", b"PK\x03\x04".to_vec(), ContentType::FileAttachment).await;
        assert!(result.is_err());
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

    /// 配置的 socket 路径必须真正生效：起一个本地 TCP 监听，指向它验证扫描器
    /// 连的是配置值而非硬编码默认路径。
    #[tokio::test]
    async fn scan_clamav_uses_configured_socket_path() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind listener");
        let addr = listener.local_addr().expect("local_addr");

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut received = Vec::new();
            let mut buf = [0u8; 1024];
            loop {
                match socket.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        received.extend_from_slice(&buf[..n]);
                        // INSTREAM 以 4 字节零终止符结束。
                        if received.len() >= 4 && received[received.len() - 4..] == [0, 0, 0, 0] {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            socket.write_all(b"stream: OK\0").await.expect("write response");
            socket.flush().await.expect("flush response");
        });

        let scanner = ContentScanner::new(synapse_common::content_scanner::ContentScannerConfig {
            enabled: true,
            scanner_type: ScannerType::ClamAv,
            clamav_socket_path: Some(addr.to_string()),
            block_on_scan_failure: true,
            scan_timeout_ms: 2000,
            ..Default::default()
        });

        let result = scanner
            .scan(ScanRequest {
                content_id: "test-clamav-ok".to_string(),
                content_type: ContentType::MediaFile,
                data: b"clean bytes".to_vec(),
            })
            .await;

        server.await.expect("server task");
        let result = result.expect("配置的 socket 路径应可连通并完成扫描");
        assert!(result.safe, "stream: OK 应判定为安全");
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

    // ── scan (Webhook path — unreachable server → always error) ────────────

    #[tokio::test]
    async fn scan_webhook_unreachable_returns_content_scan_failed() {
        let scanner = make_webhook_scanner(true, Some("http://localhost:9999/unreachable".to_string()));
        let result = scanner
            .scan(ScanRequest {
                content_id: "test-webhook-block".to_string(),
                content_type: ContentType::MediaImage,
                data: b"fake image".to_vec(),
            })
            .await;

        // Any webhook failure returns M_CONTENT_SCAN_FAILED (fail-closed)
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("M_CONTENT_SCAN_FAILED") || err.to_string().contains("Content scan failed"),
            "unexpected err: {err}"
        );
    }

    // ── scan (Webhook path — failure → M_CONTENT_SCAN_FAILED) ───────────────

    #[tokio::test]
    async fn scan_webhook_failure_returns_content_scan_failed() {
        let scanner = make_webhook_scanner(false, Some("http://localhost:9999/unreachable".to_string()));
        let result = scanner
            .scan(ScanRequest {
                content_id: "test-webhook-fail".to_string(),
                content_type: ContentType::MessageText,
                data: b"hello".to_vec(),
            })
            .await;

        // block_on_failure is now ignored; all webhook failures return M_CONTENT_SCAN_FAILED
        assert!(result.is_err(), "webhook failure must return error");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("M_CONTENT_SCAN_FAILED") || err.to_string().contains("Content scan failed"),
            "unexpected error: {err}"
        );
    }

    // ── scan all ContentTypes through disabled scanner ─────────────────────

    #[tokio::test]
    async fn scan_all_content_types_disabled_returns_error() {
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
                .await;
            assert!(result.is_err(), "disabled scanner must return error for all content types: {:?}", ct);
        }
    }
}
