use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentScanResult {
    pub safe: bool,
    pub threat_type: Option<String>,
    pub threat_message: Option<String>,
    pub scan_timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanRequest {
    pub content_id: String,
    pub content_type: ContentType,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    MediaImage,
    MediaVideo,
    MediaAudio,
    MediaFile,
    MessageText,
    FileAttachment,
}

impl ContentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MediaImage => "media_image",
            Self::MediaVideo => "media_video",
            Self::MediaAudio => "media_audio",
            Self::MediaFile => "media_file",
            Self::MessageText => "message_text",
            Self::FileAttachment => "file_attachment",
        }
    }
}

impl std::str::FromStr for ContentType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "media_image" => Ok(Self::MediaImage),
            "media_video" => Ok(Self::MediaVideo),
            "media_audio" => Ok(Self::MediaAudio),
            "media_file" => Ok(Self::MediaFile),
            "message_text" => Ok(Self::MessageText),
            "file_attachment" => Ok(Self::FileAttachment),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentScannerConfig {
    pub enabled: bool,
    pub scanner_type: ScannerType,
    pub clamav_socket_path: Option<String>,
    pub clamav_host: Option<String>,
    pub clamav_port: Option<u16>,
    pub webhook_url: Option<String>,
    pub webhook_secret: Option<String>,
    pub allowed_threat_types: Vec<String>,
    pub block_on_scan_failure: bool,
    pub scan_timeout_ms: u64,
}

impl Default for ContentScannerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            scanner_type: ScannerType::ClamAv,
            clamav_socket_path: None,
            clamav_host: Some("127.0.0.1".to_string()),
            clamav_port: Some(3310),
            webhook_url: None,
            webhook_secret: None,
            allowed_threat_types: vec![],
            block_on_scan_failure: true,
            scan_timeout_ms: 30000,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScannerType {
    #[default]
    ClamAv,
    Webhook,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookScanRequest {
    pub content_id: String,
    pub content_type: String,
    pub file_name: Option<String>,
    pub file_size: u64,
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookScanResponse {
    pub safe: bool,
    pub threat_type: Option<String>,
    pub threat_message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_type_as_str() {
        assert_eq!(ContentType::MediaImage.as_str(), "media_image");
        assert_eq!(ContentType::MediaVideo.as_str(), "media_video");
        assert_eq!(ContentType::MediaAudio.as_str(), "media_audio");
        assert_eq!(ContentType::MediaFile.as_str(), "media_file");
        assert_eq!(ContentType::MessageText.as_str(), "message_text");
        assert_eq!(ContentType::FileAttachment.as_str(), "file_attachment");
    }

    #[test]
    fn test_content_type_from_str_valid() {
        assert_eq!("media_image".parse::<ContentType>().unwrap(), ContentType::MediaImage);
        assert_eq!("media_video".parse::<ContentType>().unwrap(), ContentType::MediaVideo);
        assert_eq!("media_audio".parse::<ContentType>().unwrap(), ContentType::MediaAudio);
        assert_eq!("media_file".parse::<ContentType>().unwrap(), ContentType::MediaFile);
        assert_eq!("message_text".parse::<ContentType>().unwrap(), ContentType::MessageText);
        assert_eq!("file_attachment".parse::<ContentType>().unwrap(), ContentType::FileAttachment);
    }

    #[test]
    fn test_content_type_from_str_invalid() {
        assert!("invalid".parse::<ContentType>().is_err());
        assert!("".parse::<ContentType>().is_err());
        assert!("MediaImage".parse::<ContentType>().is_err());
    }

    #[test]
    fn test_content_type_roundtrip() {
        let variants = [
            ContentType::MediaImage,
            ContentType::MediaVideo,
            ContentType::MediaAudio,
            ContentType::MediaFile,
            ContentType::MessageText,
            ContentType::FileAttachment,
        ];
        for v in &variants {
            let s = v.as_str();
            let parsed: ContentType = s.parse().unwrap();
            assert_eq!(*v, parsed);
        }
    }

    #[test]
    fn test_content_scan_result() {
        let result = ContentScanResult {
            safe: false,
            threat_type: Some("malware".to_string()),
            threat_message: Some("Detected malware".to_string()),
            scan_timestamp: 1700000000000,
        };
        assert!(!result.safe);
        assert_eq!(result.threat_type.as_deref(), Some("malware"));
    }

    #[test]
    fn test_content_scan_result_safe() {
        let result =
            ContentScanResult { safe: true, threat_type: None, threat_message: None, scan_timestamp: 1700000000000 };
        assert!(result.safe);
        assert!(result.threat_type.is_none());
    }

    #[test]
    fn test_scan_request() {
        let request = ScanRequest {
            content_id: "content_123".to_string(),
            content_type: ContentType::MediaImage,
            data: vec![1, 2, 3, 4],
        };
        assert_eq!(request.content_id, "content_123");
        assert_eq!(request.content_type, ContentType::MediaImage);
        assert_eq!(request.data.len(), 4);
    }

    #[test]
    fn test_content_scanner_config_default() {
        let config = ContentScannerConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.scanner_type, ScannerType::ClamAv);
        assert_eq!(config.clamav_host.as_deref(), Some("127.0.0.1"));
        assert_eq!(config.clamav_port, Some(3310));
        assert!(config.block_on_scan_failure);
        assert_eq!(config.scan_timeout_ms, 30000);
        assert!(config.allowed_threat_types.is_empty());
    }

    #[test]
    fn test_content_scanner_config_custom() {
        let config = ContentScannerConfig {
            enabled: true,
            scanner_type: ScannerType::Webhook,
            webhook_url: Some("https://scan.example.com".to_string()),
            webhook_secret: Some("secret".to_string()),
            allowed_threat_types: vec!["malware".to_string()],
            block_on_scan_failure: false,
            scan_timeout_ms: 10000,
            ..Default::default()
        };
        assert!(config.enabled);
        assert_eq!(config.scanner_type, ScannerType::Webhook);
        assert_eq!(config.webhook_url.as_deref(), Some("https://scan.example.com"));
        assert!(!config.block_on_scan_failure);
    }

    #[test]
    fn test_scanner_type_default() {
        assert_eq!(ScannerType::default(), ScannerType::ClamAv);
    }

    #[test]
    fn test_scanner_type_serialization() {
        let clamav = ScannerType::ClamAv;
        let json = serde_json::to_string(&clamav).unwrap();
        assert_eq!(json, r#""clam_av""#);

        let webhook = ScannerType::Webhook;
        let json = serde_json::to_string(&webhook).unwrap();
        assert_eq!(json, r#""webhook""#);

        let disabled = ScannerType::Disabled;
        let json = serde_json::to_string(&disabled).unwrap();
        assert_eq!(json, r#""disabled""#);
    }

    #[test]
    fn test_webhook_scan_request() {
        let request = WebhookScanRequest {
            content_id: "content_456".to_string(),
            content_type: "media_image".to_string(),
            file_name: Some("test.jpg".to_string()),
            file_size: 1024,
            checksum: Some("abc123".to_string()),
        };
        assert_eq!(request.content_id, "content_456");
        assert_eq!(request.file_name.as_deref(), Some("test.jpg"));
        assert_eq!(request.file_size, 1024);
    }

    #[test]
    fn test_webhook_scan_response_safe() {
        let response = WebhookScanResponse { safe: true, threat_type: None, threat_message: None };
        assert!(response.safe);
    }

    #[test]
    fn test_webhook_scan_response_threat() {
        let response = WebhookScanResponse {
            safe: false,
            threat_type: Some("phishing".to_string()),
            threat_message: Some("Phishing content detected".to_string()),
        };
        assert!(!response.safe);
        assert_eq!(response.threat_type.as_deref(), Some("phishing"));
    }
}
