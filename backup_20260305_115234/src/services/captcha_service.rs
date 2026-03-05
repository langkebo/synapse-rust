use crate::common::error::ApiError;
use crate::storage::captcha::*;
use rand::Rng;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone)]
pub struct CaptchaService {
    storage: Arc<CaptchaStorage>,
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
    pub fn new(storage: Arc<CaptchaStorage>) -> Self {
        Self { storage }
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
                let length = self
                    .storage
                    .get_config_as_int("email.code_length", 6)
                    .await?;
                let expiry = self
                    .storage
                    .get_config_as_int("email.code_expiry_minutes", 10)
                    .await?;
                let attempts = self
                    .storage
                    .get_config_as_int("email.max_attempts", 5)
                    .await?;
                let limit = self
                    .storage
                    .get_config_as_int("email.rate_limit_per_hour", 5)
                    .await?;
                (length, expiry, attempts, limit)
            }
            "sms" => {
                let length = self.storage.get_config_as_int("sms.code_length", 6).await?;
                let expiry = self
                    .storage
                    .get_config_as_int("sms.code_expiry_minutes", 5)
                    .await?;
                let attempts = self
                    .storage
                    .get_config_as_int("sms.max_attempts", 5)
                    .await?;
                let limit = self
                    .storage
                    .get_config_as_int("sms.rate_limit_per_hour", 5)
                    .await?;
                (length, expiry, attempts, limit)
            }
            "image" => {
                let length = self
                    .storage
                    .get_config_as_int("image.code_length", 4)
                    .await?;
                let expiry = self
                    .storage
                    .get_config_as_int("image.code_expiry_minutes", 5)
                    .await?;
                let attempts = self
                    .storage
                    .get_config_as_int("image.max_attempts", 3)
                    .await?;
                let limit = self
                    .storage
                    .get_config_as_int("global.ip_rate_limit_per_hour", 20)
                    .await?;
                (length, expiry, attempts, limit)
            }
            _ => return Err(ApiError::bad_request("Invalid captcha type")),
        };

        if !self
            .storage
            .check_rate_limit(&request.target, captcha_type, rate_limit)
            .await?
        {
            return Err(ApiError::rate_limited(
                "Rate limit exceeded for this target",
            ));
        }

        if let Some(ip) = ip_address {
            let ip_limit = self
                .storage
                .get_config_as_int("global.ip_rate_limit_per_hour", 20)
                .await?;
            if !self.storage.check_ip_rate_limit(ip, ip_limit).await? {
                return Err(ApiError::rate_limited("Rate limit exceeded for this IP"));
            }
        }

        let code = self.generate_code(code_length as usize);

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

        let send_result = self
            .send_captcha_via_provider(
                &captcha,
                &code,
                expiry_minutes,
                request.template_name.as_deref(),
            )
            .await;

        self.storage
            .create_send_log(CreateSendLogRequest {
                captcha_id: Some(captcha.captcha_id.clone()),
                captcha_type: captcha.captcha_type.clone(),
                target: captcha.target.clone(),
                ip_address: ip_address.map(|s| s.to_string()),
                user_agent: user_agent.map(|s| s.to_string()),
                success: send_result.is_ok(),
                error_message: send_result.as_ref().err().map(|e| e.to_string()),
                provider: Some(captcha_type.to_string()),
                provider_response: None,
            })
            .await?;

        send_result?;

        info!("Captcha sent: {} to {}", captcha.captcha_id, captcha.target);

        Ok(CaptchaResponse {
            captcha_id: captcha.captcha_id,
            expires_in: expires_in_seconds,
            captcha_type: request.captcha_type,
        })
    }

    pub async fn verify_captcha(&self, request: VerifyCaptchaRequest) -> Result<bool, ApiError> {
        let verified = self
            .storage
            .verify_captcha(&request.captcha_id, &request.code)
            .await?;

        if verified {
            info!("Captcha verified successfully: {}", request.captcha_id);
        } else {
            info!("Captcha verification failed: {}", request.captcha_id);
        }

        Ok(verified)
    }

    pub async fn get_captcha(
        &self,
        captcha_id: &str,
    ) -> Result<Option<RegistrationCaptcha>, ApiError> {
        self.storage.get_captcha(captcha_id).await
    }

    pub async fn invalidate_captcha(&self, captcha_id: &str) -> Result<(), ApiError> {
        self.storage.invalidate_captcha(captcha_id).await
    }

    fn generate_code(&self, length: usize) -> String {
        let mut rng = rand::thread_rng();
        (0..length)
            .map(|_| rng.gen_range(0..10).to_string())
            .collect()
    }

    async fn send_captcha_via_provider(
        &self,
        captcha: &RegistrationCaptcha,
        code: &str,
        expiry_minutes: i32,
        template_name: Option<&str>,
    ) -> Result<(), ApiError> {
        let template = if let Some(name) = template_name {
            self.storage
                .get_template(name)
                .await?
                .ok_or_else(|| ApiError::bad_request("Template not found"))?
        } else {
            self.storage
                .get_default_template(&captcha.captcha_type)
                .await?
                .ok_or_else(|| ApiError::internal("No default template found"))?
        };

        let content = self.render_template(&template, code, expiry_minutes);

        match captcha.captcha_type.as_str() {
            "email" => {
                self.send_email(&captcha.target, template.subject.as_deref(), &content)
                    .await
            }
            "sms" => self.send_sms(&captcha.target, &content).await,
            "image" => Ok(()),
            _ => Err(ApiError::bad_request("Invalid captcha type")),
        }
    }

    fn render_template(
        &self,
        template: &CaptchaTemplate,
        code: &str,
        expiry_minutes: i32,
    ) -> String {
        let mut content = template.content.clone();
        content = content.replace("{{code}}", code);
        content = content.replace("{{expiry_minutes}}", &expiry_minutes.to_string());
        content
    }

    async fn send_email(
        &self,
        to: &str,
        _subject: Option<&str>,
        content: &str,
    ) -> Result<(), ApiError> {
        info!("Sending email to {}: {:?}", to, content);

        Ok(())
    }

    async fn send_sms(&self, to: &str, content: &str) -> Result<(), ApiError> {
        info!("Sending SMS to {}: {:?}", to, content);

        Ok(())
    }

    pub async fn cleanup_expired(&self) -> Result<u64, ApiError> {
        self.storage.cleanup_expired_captchas().await
    }
}
