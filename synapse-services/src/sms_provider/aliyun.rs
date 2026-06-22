//! Aliyun SMS provider — production-grade SMS delivery via Alibaba Cloud SMS API.
//!
//! Implements the [Aliyun SMS SendSms API](https://help.aliyun.com/document_detail/419273.html)
//! with HMAC-SHA1 signature V1.0.

use async_trait::async_trait;
use chrono::Utc;
use hmac::{Hmac, Mac};
use rand::Rng;
use reqwest::Client;
use sha1::Sha1;
use synapse_common::config::sms::SmsConfig;
use synapse_common::error::ApiError;

use super::SmsProvider;

/// RFC 3986 percent-encoding for Aliyun SMS API signature.
///
/// Encodes all characters except unreserved characters (A-Z, a-z, 0-9, -, _, .).
/// Additionally encodes `~`, `*`, `+`, and `%` as required by Aliyun.
fn aliyun_percent_encode(input: &str) -> String {
    let mut result = String::with_capacity(input.len() * 3);
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => {
                result.push(byte as char);
            }
            b' ' => {
                result.push_str("%20");
            }
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
}

/// Aliyun SMS provider implementing the `SmsProvider` trait.
///
/// Uses Alibaba Cloud's SendSms API with HMAC-SHA1 signature.
/// Requires `AccessKey ID`, `AccessKey Secret`, `SignName`, and `TemplateCode`.
pub struct AliyunSmsProvider {
    client: Client,
    endpoint: String,
    access_key_id: String,
    access_key_secret: String,
    sign_name: String,
    template_code: String,
}

impl AliyunSmsProvider {
    pub fn new(config: &SmsConfig) -> Self {
        Self {
            client: Client::new(),
            endpoint: if config.endpoint.is_empty() {
                "dysmsapi.aliyuncs.com".to_string()
            } else {
                config.endpoint.trim().to_string()
            },
            access_key_id: config.api_key.clone(),
            access_key_secret: config.api_secret.clone(),
            sign_name: config.sender_id.clone(),
            template_code: config.template_code.clone(),
        }
    }

    /// Build the canonical query string for Aliyun SMS API signature.
    fn build_query(&self, phone_numbers: &str, template_param: &str) -> String {
        let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let nonce: String = rand::rng().sample_iter(&rand::distr::Alphanumeric).take(32).map(char::from).collect();

        let mut params: Vec<(&str, &str)> = vec![
            ("AccessKeyId", &self.access_key_id),
            ("Action", "SendSms"),
            ("Format", "JSON"),
            ("PhoneNumbers", phone_numbers),
            ("SignName", &self.sign_name),
            ("SignatureMethod", "HMAC-SHA1"),
            ("SignatureNonce", &nonce),
            ("SignatureVersion", "1.0"),
            ("TemplateCode", &self.template_code),
            ("TemplateParam", template_param),
            ("Timestamp", &timestamp),
            ("Version", "2017-05-25"),
        ];

        params.sort_by(|a, b| a.0.cmp(b.0));

        let mut query = String::new();
        for (i, (k, v)) in params.iter().enumerate() {
            if i > 0 {
                query.push('&');
            }
            query.push_str(&aliyun_percent_encode(k));
            query.push('=');
            query.push_str(&aliyun_percent_encode(v));
        }
        query
    }

    /// Compute HMAC-SHA1 signature for Aliyun SMS API.
    fn sign(&self, query: &str) -> Result<String, ApiError> {
        let string_to_sign = format!("GET&{}&{}", aliyun_percent_encode("/"), aliyun_percent_encode(query));
        let mut mac = Hmac::<Sha1>::new_from_slice(format!("{}&", self.access_key_secret).as_bytes())
            .map_err(|e| ApiError::internal(format!("HMAC key initialization failed: {e}")))?;
        mac.update(string_to_sign.as_bytes());
        let result = mac.finalize();
        Ok(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, result.into_bytes()))
    }
}

#[async_trait]
impl SmsProvider for AliyunSmsProvider {
    async fn send(&self, to: &str, content: &str) -> Result<(), ApiError> {
        // Build template parameter JSON: {"code":"123456"}
        let template_param = format!(r#"{{"code":"{}"}}"#, content);

        let query = self.build_query(to, &template_param);
        let signature = self.sign(&query)?;

        // Allow endpoints with an explicit scheme (e.g. for tests pointing at a
        // mock server). Bare hostnames default to HTTPS as required by Aliyun.
        let url = if self.endpoint.starts_with("http://") || self.endpoint.starts_with("https://") {
            format!("{}/?{}&Signature={}", self.endpoint, query, aliyun_percent_encode(&signature))
        } else {
            format!("https://{}/?{}&Signature={}", self.endpoint, query, aliyun_percent_encode(&signature))
        };

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to call Aliyun SMS API", &e))?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            let truncated = if body.len() > 200 { format!("{}...", &body[..200]) } else { body };
            return Err(ApiError::internal(format!("Aliyun SMS API returned HTTP {}: {}", status.as_u16(), truncated)));
        }

        // Parse response — Aliyun returns {"Code":"OK",...} or {"Code":"isv.*",...}
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&body) {
            let code = parsed.get("Code").and_then(|v| v.as_str()).unwrap_or("");
            if code == "OK" {
                tracing::info!(
                    to = %to,
                    request_id = %parsed.get("RequestId").and_then(|v| v.as_str()).unwrap_or(""),
                    "Aliyun SMS sent successfully"
                );
                return Ok(());
            }

            let message = parsed.get("Message").and_then(|v| v.as_str()).unwrap_or("unknown error");
            return Err(ApiError::internal(format!("Aliyun SMS API error: Code={}, Message={}", code, message)));
        }

        Err(ApiError::internal(format!("Aliyun SMS API unexpected response: {}", body)))
    }

    fn provider_name(&self) -> &'static str {
        "aliyun"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn test_aliyun_percent_encode() {
        // Standard RFC 3986
        let encoded = aliyun_percent_encode("hello world");
        assert!(encoded.contains("%20"), "space should be encoded, got: {encoded}");

        // Aliyun-specific: ~ * + should be encoded
        assert_eq!(aliyun_percent_encode("~"), "%7E");
        assert_eq!(aliyun_percent_encode("*"), "%2A");
        assert_eq!(aliyun_percent_encode("+"), "%2B");
    }

    #[test]
    fn test_aliyun_signature() {
        let config = SmsConfig {
            enabled: true,
            provider: "aliyun".to_string(),
            api_key: "test-access-key".to_string(),
            api_secret: "test-access-secret".to_string(),
            sender_id: "TestSign".to_string(),
            template_code: "SMS_123456789".to_string(),
            ..Default::default()
        };
        let provider = AliyunSmsProvider::new(&config);

        // Use a fixed query to verify signature is deterministic
        let query = "AccessKeyId=test-access-key&Action=SendSms&Format=JSON&PhoneNumbers=13800138000&SignName=TestSign&SignatureMethod=HMAC-SHA1&SignatureNonce=abc123&SignatureVersion=1.0&TemplateCode=SMS_123456789&TemplateParam=%7B%22code%22%3A%22123456%22%7D&Timestamp=2024-01-01T00%3A00%3A00Z&Version=2017-05-25";
        let signature = provider.sign(query).expect("HMAC-SHA1 signing with a valid key should succeed");

        // Signature should be a valid base64 string
        assert!(!signature.is_empty());
        assert!(base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &signature).is_ok());
    }

    #[test]
    fn test_aliyun_query_contains_required_params() {
        let config = SmsConfig {
            enabled: true,
            provider: "aliyun".to_string(),
            api_key: "test-key".to_string(),
            api_secret: "test-secret".to_string(),
            sender_id: "TestSign".to_string(),
            template_code: "SMS_001".to_string(),
            ..Default::default()
        };
        let provider = AliyunSmsProvider::new(&config);
        let query = provider.build_query("13800138000", r#"{"code":"123456"}"#);

        assert!(query.contains("AccessKeyId=test-key"));
        assert!(query.contains("Action=SendSms"));
        assert!(query.contains("SignName=TestSign"));
        assert!(query.contains("TemplateCode=SMS_001"));
        assert!(query.contains("SignatureMethod=HMAC-SHA1"));
        assert!(query.contains("Version=2017-05-25"));
    }

    #[tokio::test]
    async fn test_aliyun_send_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/"))
            .and(query_param("AccessKeyId", "test-access-key"))
            .and(query_param("Action", "SendSms"))
            .and(query_param("SignName", "TestSign"))
            .and(query_param("TemplateCode", "SMS_123456789"))
            .and(query_param("PhoneNumbers", "13800138000"))
            .and(query_param("TemplateParam", r#"{"code":"123456"}"#))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "Code": "OK",
                "Message": "OK",
                "RequestId": "test-123",
                "BizId": "biz-456"
            })))
            .mount(&server)
            .await;

        let provider = AliyunSmsProvider::new(&SmsConfig {
            enabled: true,
            provider: "aliyun".to_string(),
            api_key: "test-access-key".to_string(),
            api_secret: "test-access-secret".to_string(),
            sender_id: "TestSign".to_string(),
            template_code: "SMS_123456789".to_string(),
            endpoint: server.uri(),
            ..Default::default()
        });

        provider.send("13800138000", "123456").await.expect("aliyun sms send should succeed when API returns Code=OK");
    }

    #[tokio::test]
    async fn test_aliyun_send_failure() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "Code": "isv.BUSINESS_LIMIT_CONTROL",
                "Message": "Minutes: Limit Reached",
                "RequestId": "test-456"
            })))
            .mount(&server)
            .await;

        let provider = AliyunSmsProvider::new(&SmsConfig {
            enabled: true,
            provider: "aliyun".to_string(),
            api_key: "test-access-key".to_string(),
            api_secret: "test-access-secret".to_string(),
            sender_id: "TestSign".to_string(),
            template_code: "SMS_123456789".to_string(),
            endpoint: server.uri(),
            ..Default::default()
        });

        let err = provider
            .send("13800138000", "123456")
            .await
            .expect_err("aliyun sms send should fail when API returns non-OK Code");
        assert!(err.is_internal(), "error should be internal, got: {err:?}");
        let msg = err.to_string();
        assert!(msg.contains("isv.BUSINESS_LIMIT_CONTROL"), "error should contain Aliyun Code, got: {msg}");
        assert!(msg.contains("Minutes: Limit Reached"), "error should contain Aliyun Message, got: {msg}");
    }

    #[test]
    fn test_aliyun_signature_deterministic() {
        let config = SmsConfig {
            enabled: true,
            provider: "aliyun".to_string(),
            api_key: "test-access-key".to_string(),
            api_secret: "test-access-secret".to_string(),
            sender_id: "TestSign".to_string(),
            template_code: "SMS_123456789".to_string(),
            ..Default::default()
        };
        let provider = AliyunSmsProvider::new(&config);

        // build_query generates a unique nonce and timestamp per call, so we
        // verify determinism of the HMAC-SHA1 sign() on a fixed query string.
        // The signature must be reproducible for the same input.
        let query = provider.build_query("13800138000", r#"{"code":"123456"}"#);
        let signature1 = provider.sign(&query).expect("HMAC-SHA1 signing with a valid key should succeed");
        let signature2 = provider.sign(&query).expect("HMAC-SHA1 signing with a valid key should succeed");

        assert_eq!(signature1, signature2, "HMAC-SHA1 signature must be deterministic for the same query");
        assert!(!signature1.is_empty(), "signature should not be empty");
    }
}
