use crate::sms_provider::{create_sms_provider, SmsProvider};
use rand::Rng;
use std::sync::Arc;
use synapse_common::background_job::BackgroundJob;
use synapse_common::config::sms::SmsConfig;
use synapse_common::error::ApiError;
use synapse_common::task_queue::RedisTaskQueue;
use synapse_storage::captcha::*;
use tracing::{info, warn};

#[derive(Clone)]
pub struct CaptchaService {
    storage: Arc<dyn synapse_storage::captcha::CaptchaStoreApi>,
    task_queue: Option<Arc<RedisTaskQueue>>,
    smtp_enabled: bool,
    sms_provider: Option<Arc<dyn SmsProvider>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CaptchaResponse {
    pub captcha_id: String,
    pub expires_in: i64,
    pub captcha_type: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SendCaptchaRequest {
    pub captcha_type: String,
    pub target: String,
    pub template_name: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct VerifyCaptchaRequest {
    pub captcha_id: String,
    pub code: String,
}

impl CaptchaService {
    pub fn new(storage: Arc<dyn synapse_storage::captcha::CaptchaStoreApi>) -> Self {
        Self::with_sms_provider(storage, None, false, None)
    }

    pub fn with_delivery(
        storage: Arc<dyn synapse_storage::captcha::CaptchaStoreApi>,
        task_queue: Option<Arc<RedisTaskQueue>>,
        smtp_enabled: bool,
    ) -> Self {
        Self::with_sms_provider(storage, task_queue, smtp_enabled, None)
    }

    pub fn with_sms_provider(
        storage: Arc<dyn synapse_storage::captcha::CaptchaStoreApi>,
        task_queue: Option<Arc<RedisTaskQueue>>,
        smtp_enabled: bool,
        sms_provider: Option<Arc<dyn SmsProvider>>,
    ) -> Self {
        Self { storage, task_queue, smtp_enabled, sms_provider }
    }

    /// Create a CaptchaService with SMS provider built from config.
    pub fn with_sms_config(
        storage: Arc<dyn synapse_storage::captcha::CaptchaStoreApi>,
        task_queue: Option<Arc<RedisTaskQueue>>,
        smtp_enabled: bool,
        sms_config: &SmsConfig,
    ) -> Self {
        let provider = create_sms_provider(sms_config);
        let sms_provider = if sms_config.enabled { Some(Arc::from(provider)) } else { None };
        Self { storage, task_queue, smtp_enabled, sms_provider }
    }

    pub async fn send_captcha(
        &self,
        request: SendCaptchaRequest,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<CaptchaResponse, ApiError> {
        let captcha_type = request.captcha_type.as_str();

        if !matches!(captcha_type, "email" | "sms" | "image") {
            return Err(ApiError::bad_request("Invalid captcha type"));
        }

        let (code_length, expiry_minutes, max_attempts, rate_limit) = match captcha_type {
            "email" => {
                let length = self.storage.get_config_as_int("email.code_length", 6).await?;
                let expiry = self.storage.get_config_as_int("email.code_expiry_minutes", 10).await?;
                let attempts = self.storage.get_config_as_int("email.max_attempts", 5).await?;
                let limit = self.storage.get_config_as_int("email.rate_limit_per_hour", 5).await?;
                (length, expiry, attempts, limit)
            }
            "sms" => {
                let length = self.storage.get_config_as_int("sms.code_length", 6).await?;
                let expiry = self.storage.get_config_as_int("sms.code_expiry_minutes", 5).await?;
                let attempts = self.storage.get_config_as_int("sms.max_attempts", 5).await?;
                let limit = self.storage.get_config_as_int("sms.rate_limit_per_hour", 5).await?;
                (length, expiry, attempts, limit)
            }
            "image" => {
                let length = self.storage.get_config_as_int("image.code_length", 4).await?;
                let expiry = self.storage.get_config_as_int("image.code_expiry_minutes", 5).await?;
                let attempts = self.storage.get_config_as_int("image.max_attempts", 3).await?;
                let limit = self.storage.get_config_as_int("global.ip_rate_limit_per_hour", 20).await?;
                (length, expiry, attempts, limit)
            }
            _ => return Err(ApiError::bad_request("Invalid captcha type")),
        };

        if !self.storage.check_rate_limit(&request.target, captcha_type, rate_limit).await? {
            return Err(ApiError::rate_limited("Rate limit exceeded for this target"));
        }

        if let Some(ip) = ip_address {
            let ip_limit = self.storage.get_config_as_int("global.ip_rate_limit_per_hour", 20).await?;
            if !self.storage.check_ip_rate_limit(ip, ip_limit).await? {
                return Err(ApiError::rate_limited("Rate limit exceeded for this IP"));
            }
        }

        let code = Self::generate_code(code_length as usize);

        let expires_in_seconds = (expiry_minutes as i64) * 60;

        let captcha = self
            .storage
            .create_captcha(CreateCaptchaRequest {
                captcha_type: request.captcha_type.clone(),
                target: request.target.clone(),
                code: code.clone(),
                expires_in_seconds,
                ip_address: ip_address.map(|s| s.to_string()),
                user_agent: user_agent.map(|s| s.to_string()),
                max_attempts,
                metadata: None,
            })
            .await?;

        let send_result =
            self.send_captcha_via_provider(&captcha, &code, expiry_minutes, request.template_name.as_deref()).await;

        self.storage
            .create_send_log(CreateSendLogRequest {
                captcha_id: Some(captcha.captcha_id.clone()),
                captcha_type: captcha.captcha_type.clone(),
                target: captcha.target.clone(),
                ip_address: ip_address.map(|s| s.to_string()),
                user_agent: user_agent.map(|s| s.to_string()),
                is_success: send_result.is_ok(),
                error_message: send_result.as_ref().err().map(|e| e.to_string()),
                provider: Some(self.delivery_provider_name(captcha_type)),
                provider_response: None,
            })
            .await?;

        send_result?;

        info!(
            captcha_type = %request.captcha_type,
            expires_in_seconds,
            template_selected = request.template_name.is_some(),
            delivery_channel = %captcha.captcha_type,
            "Captcha sent"
        );

        Ok(CaptchaResponse {
            captcha_id: captcha.captcha_id,
            expires_in: expires_in_seconds,
            captcha_type: request.captcha_type,
        })
    }

    pub async fn verify_captcha(&self, request: VerifyCaptchaRequest) -> Result<bool, ApiError> {
        let verified = self.storage.verify_captcha(&request.captcha_id, &request.code).await?;

        info!(verification_result = verified, "Captcha verification completed");

        Ok(verified)
    }

    pub async fn get_captcha(&self, captcha_id: &str) -> Result<Option<RegistrationCaptcha>, ApiError> {
        self.storage.get_captcha(captcha_id).await
    }

    pub async fn invalidate_captcha(&self, captcha_id: &str) -> Result<(), ApiError> {
        self.storage.invalidate_captcha(captcha_id).await
    }

    fn generate_code(length: usize) -> String {
        let mut rng = rand::rng();
        (0..length).map(|_| rng.random_range(0..10).to_string()).collect()
    }

    #[cfg(test)]
    fn generate_code_static(length: usize) -> String {
        let mut rng = rand::rng();
        (0..length).map(|_| rng.random_range(0..10).to_string()).collect()
    }

    #[cfg(test)]
    fn render_template_static(template: &CaptchaTemplate, code: &str, expiry_minutes: i32) -> String {
        let mut content = template.content.clone();
        content = content.replace("{{code}}", code);
        content = content.replace("{{expiry_minutes}}", &expiry_minutes.to_string());
        content
    }

    async fn send_captcha_via_provider(
        &self,
        captcha: &RegistrationCaptcha,
        code: &str,
        expiry_minutes: i32,
        template_name: Option<&str>,
    ) -> Result<(), ApiError> {
        let template = if let Some(name) = template_name {
            self.storage.get_template(name).await?.ok_or_else(|| ApiError::bad_request("Template not found"))?
        } else {
            self.storage.get_default_template(&captcha.captcha_type).await?.ok_or_else(|| {
                ApiError::bad_request(
                    "No captcha template configured for this type. Please contact the server administrator.",
                )
            })?
        };

        let content = Self::render_template(&template, code, expiry_minutes);

        match captcha.captcha_type.as_str() {
            "email" => self.send_email(&captcha.target, template.subject.as_deref(), &content).await,
            "sms" => self.send_sms_async(&captcha.target, &content).await,
            "image" => Ok(()),
            _ => Err(ApiError::bad_request("Invalid captcha type")),
        }
    }

    fn render_template(template: &CaptchaTemplate, code: &str, expiry_minutes: i32) -> String {
        let mut content = template.content.clone();
        content = content.replace("{{code}}", code);
        content = content.replace("{{expiry_minutes}}", &expiry_minutes.to_string());
        content
    }

    fn delivery_provider_name(&self, captcha_type: &str) -> String {
        match captcha_type {
            "email" => {
                if self.smtp_enabled {
                    "smtp".to_string()
                } else {
                    "email".to_string()
                }
            }
            "sms" => self
                .sms_provider
                .as_ref()
                .map(|provider| provider.provider_name().to_string())
                .unwrap_or_else(|| "sms".to_string()),
            "image" => "image".to_string(),
            other => other.to_string(),
        }
    }

    async fn send_email(&self, to: &str, subject: Option<&str>, content: &str) -> Result<(), ApiError> {
        if !self.smtp_enabled {
            warn!(target = %to, "captcha email delivery requested but smtp is disabled");
            return Err(ApiError::not_implemented(
                "Captcha email delivery is not configured. Enable SMTP and the background task queue first.",
            ));
        }

        let queue = self.task_queue.as_ref().ok_or_else(|| {
            warn!(target = %to, "captcha email delivery requested without configured background task queue");
            ApiError::not_implemented(
                "Captcha email delivery queue is not configured. Enable the background worker before using email captcha.",
            )
        })?;

        queue
            .submit(BackgroundJob::SendEmail {
                to: to.to_string(),
                subject: subject.unwrap_or("Your verification code").to_string(),
                body: content.to_string(),
            })
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to enqueue captcha email delivery", &e))?;

        Ok(())
    }

    async fn send_sms_async(&self, to: &str, content: &str) -> Result<(), ApiError> {
        if let Some(provider) = &self.sms_provider {
            info!(target = %to, provider = %provider.provider_name(), "Sending captcha SMS");
            return provider.send(to, content).await;
        }
        warn!(target = %to, "captcha sms delivery requested but no sms provider is configured");
        Err(ApiError::not_implemented(
            "Captcha SMS delivery is not configured. Connect an SMS provider before using sms captcha.",
        ))
    }

    pub async fn cleanup_expired(&self) -> Result<u64, ApiError> {
        self.storage.cleanup_expired_captchas().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_code_length() {
        let code = CaptchaService::generate_code_static(6);
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_render_template() {
        let template = CaptchaTemplate {
            id: 1,
            template_name: "test".to_string(),
            captcha_type: "email".to_string(),
            subject: Some("验证码".to_string()),
            content: "您的验证码是 {{code}}，有效期 {{expiry_minutes}} 分钟。".to_string(),
            variables: serde_json::Value::Null,
            is_default: true,
            is_enabled: true,
            created_ts: 0,
            updated_ts: 0,
        };

        let content = CaptchaService::render_template_static(&template, "123456", 10);
        assert_eq!(content, "您的验证码是 123456，有效期 10 分钟。");
    }

    #[test]
    fn test_send_captcha_request_deserialization() {
        let json = r#"{
            "captcha_type": "email",
            "target": "test@example.com",
            "template_name": "welcome"
        }"#;

        let request: SendCaptchaRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.captcha_type, "email");
        assert_eq!(request.target, "test@example.com");
        assert_eq!(request.template_name, Some("welcome".to_string()));
    }

    #[test]
    fn test_verify_captcha_request_deserialization() {
        let json = r#"{
            "captcha_id": "captcha123",
            "code": "123456"
        }"#;

        let request: VerifyCaptchaRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.captcha_id, "captcha123");
        assert_eq!(request.code, "123456");
    }

    #[test]
    fn test_captcha_response_serialization() {
        let response = CaptchaResponse {
            captcha_id: "captcha123".to_string(),
            expires_in: 600,
            captcha_type: "email".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"captcha_id\":\"captcha123\""));
        assert!(json.contains("\"expires_in\":600"));
        assert!(json.contains("\"captcha_type\":\"email\""));
    }

    #[test]
    fn test_send_captcha_request_without_template() {
        let json = r#"{
            "captcha_type": "sms",
            "target": "+8613800138000"
        }"#;

        let request: SendCaptchaRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.captcha_type, "sms");
        assert_eq!(request.target, "+8613800138000");
        assert!(request.template_name.is_none());
    }
}
