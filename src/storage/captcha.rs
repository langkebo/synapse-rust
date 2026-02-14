use crate::common::error::ApiError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RegistrationCaptcha {
    pub id: i32,
    pub captcha_id: String,
    pub captcha_type: String,
    pub target: String,
    pub code: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub verified_at: Option<DateTime<Utc>>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub status: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaptchaSendLog {
    pub id: i32,
    pub captcha_id: Option<String>,
    pub captcha_type: String,
    pub target: String,
    pub sent_at: DateTime<Utc>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub success: bool,
    pub error_message: Option<String>,
    pub provider: Option<String>,
    pub provider_response: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaptchaRateLimit {
    pub id: i32,
    pub target: String,
    pub ip_address: Option<String>,
    pub captcha_type: String,
    pub request_count: i32,
    pub first_request_at: DateTime<Utc>,
    pub last_request_at: DateTime<Utc>,
    pub blocked_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaptchaTemplate {
    pub id: i32,
    pub template_name: String,
    pub captcha_type: String,
    pub subject: Option<String>,
    pub content: String,
    pub variables: serde_json::Value,
    pub is_default: bool,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaptchaConfig {
    pub id: i32,
    pub config_key: String,
    pub config_value: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateCaptchaRequest {
    pub captcha_type: String,
    pub target: String,
    pub code: String,
    pub expires_in_seconds: i64,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub max_attempts: i32,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSendLogRequest {
    pub captcha_id: Option<String>,
    pub captcha_type: String,
    pub target: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub success: bool,
    pub error_message: Option<String>,
    pub provider: Option<String>,
    pub provider_response: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CaptchaStorage {
    pool: Arc<PgPool>,
}

impl CaptchaStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_captcha(
        &self,
        request: CreateCaptchaRequest,
    ) -> Result<RegistrationCaptcha, ApiError> {
        let captcha_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(request.expires_in_seconds);
        let metadata = request.metadata.unwrap_or(serde_json::json!({}));

        let row = sqlx::query_as::<_, RegistrationCaptcha>(
            r#"
            INSERT INTO registration_captcha (
                captcha_id, captcha_type, target, code, created_at, expires_at,
                ip_address, user_agent, max_attempts, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING *
            "#,
        )
        .bind(&captcha_id)
        .bind(&request.captcha_type)
        .bind(&request.target)
        .bind(&request.code)
        .bind(now)
        .bind(expires_at)
        .bind(&request.ip_address)
        .bind(&request.user_agent)
        .bind(request.max_attempts)
        .bind(&metadata)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create captcha: {}", e)))?;

        info!("Created captcha: {} for target: {}", captcha_id, request.target);
        Ok(row)
    }

    pub async fn get_captcha(&self, captcha_id: &str) -> Result<Option<RegistrationCaptcha>, ApiError> {
        let row = sqlx::query_as::<_, RegistrationCaptcha>(
            "SELECT * FROM registration_captcha WHERE captcha_id = $1"
        )
        .bind(captcha_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get captcha: {}", e)))?;

        Ok(row)
    }

    pub async fn get_latest_captcha(&self, target: &str, captcha_type: &str) -> Result<Option<RegistrationCaptcha>, ApiError> {
        let row = sqlx::query_as::<_, RegistrationCaptcha>(
            r#"
            SELECT * FROM registration_captcha 
            WHERE target = $1 AND captcha_type = $2 AND status = 'pending' AND expires_at > NOW()
            ORDER BY created_at DESC
            LIMIT 1
            "#
        )
        .bind(target)
        .bind(captcha_type)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get latest captcha: {}", e)))?;

        Ok(row)
    }

    pub async fn verify_captcha(&self, captcha_id: &str, code: &str) -> Result<bool, ApiError> {
        let now = Utc::now();
        
        let captcha = self.get_captcha(captcha_id).await?
            .ok_or_else(|| ApiError::bad_request("Captcha not found"))?;

        if captcha.status != "pending" {
            return Ok(false);
        }

        if captcha.expires_at < now {
            sqlx::query(
                "UPDATE registration_captcha SET status = 'expired' WHERE captcha_id = $1"
            )
            .bind(captcha_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update captcha: {}", e)))?;
            
            return Ok(false);
        }

        if captcha.attempt_count >= captcha.max_attempts {
            sqlx::query(
                "UPDATE registration_captcha SET status = 'exhausted' WHERE captcha_id = $1"
            )
            .bind(captcha_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update captcha: {}", e)))?;
            
            return Ok(false);
        }

        sqlx::query(
            "UPDATE registration_captcha SET attempt_count = attempt_count + 1 WHERE captcha_id = $1"
        )
        .bind(captcha_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to increment attempt count: {}", e)))?;

        if captcha.code == code {
            sqlx::query(
                r#"
                UPDATE registration_captcha 
                SET status = 'verified', verified_at = $1, used_at = $1
                WHERE captcha_id = $2
                "#
            )
            .bind(now)
            .bind(captcha_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to verify captcha: {}", e)))?;
            
            info!("Captcha verified: {}", captcha_id);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn invalidate_captcha(&self, captcha_id: &str) -> Result<(), ApiError> {
        let now = Utc::now();
        
        sqlx::query(
            "UPDATE registration_captcha SET status = 'used', used_at = $1 WHERE captcha_id = $2"
        )
        .bind(now)
        .bind(captcha_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to invalidate captcha: {}", e)))?;

        info!("Captcha invalidated: {}", captcha_id);
        Ok(())
    }

    pub async fn create_send_log(&self, request: CreateSendLogRequest) -> Result<CaptchaSendLog, ApiError> {
        let row = sqlx::query_as::<_, CaptchaSendLog>(
            r#"
            INSERT INTO captcha_send_log (
                captcha_id, captcha_type, target, ip_address, user_agent,
                success, error_message, provider, provider_response
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING *
            "#,
        )
        .bind(&request.captcha_id)
        .bind(&request.captcha_type)
        .bind(&request.target)
        .bind(&request.ip_address)
        .bind(&request.user_agent)
        .bind(request.success)
        .bind(&request.error_message)
        .bind(&request.provider)
        .bind(&request.provider_response)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create send log: {}", e)))?;

        Ok(row)
    }

    pub async fn check_rate_limit(&self, target: &str, captcha_type: &str, max_per_hour: i32) -> Result<bool, ApiError> {
        let one_hour_ago = Utc::now() - chrono::Duration::hours(1);
        
        let count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM captcha_send_log 
            WHERE target = $1 AND captcha_type = $2 AND sent_at > $3
            "#
        )
        .bind(target)
        .bind(captcha_type)
        .bind(one_hour_ago)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check rate limit: {}", e)))?;

        Ok(count.0 < max_per_hour as i64)
    }

    pub async fn check_ip_rate_limit(&self, ip_address: &str, max_per_hour: i32) -> Result<bool, ApiError> {
        let one_hour_ago = Utc::now() - chrono::Duration::hours(1);
        
        let count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM captcha_send_log 
            WHERE ip_address = $1 AND sent_at > $2
            "#
        )
        .bind(ip_address)
        .bind(one_hour_ago)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check IP rate limit: {}", e)))?;

        Ok(count.0 < max_per_hour as i64)
    }

    pub async fn get_template(&self, template_name: &str) -> Result<Option<CaptchaTemplate>, ApiError> {
        let row = sqlx::query_as::<_, CaptchaTemplate>(
            "SELECT * FROM captcha_template WHERE template_name = $1 AND enabled = true"
        )
        .bind(template_name)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get template: {}", e)))?;

        Ok(row)
    }

    pub async fn get_default_template(&self, captcha_type: &str) -> Result<Option<CaptchaTemplate>, ApiError> {
        let row = sqlx::query_as::<_, CaptchaTemplate>(
            "SELECT * FROM captcha_template WHERE captcha_type = $1 AND is_default = true AND enabled = true"
        )
        .bind(captcha_type)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get default template: {}", e)))?;

        Ok(row)
    }

    pub async fn get_config(&self, config_key: &str) -> Result<Option<String>, ApiError> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT config_value FROM captcha_config WHERE config_key = $1"
        )
        .bind(config_key)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get config: {}", e)))?;

        Ok(row.map(|r| r.0))
    }

    pub async fn get_config_as_int(&self, config_key: &str, default: i32) -> Result<i32, ApiError> {
        let value = self.get_config(config_key).await?;
        
        Ok(match value {
            Some(v) => v.parse().unwrap_or(default),
            None => default,
        })
    }

    pub async fn cleanup_expired_captchas(&self) -> Result<u64, ApiError> {
        let result = sqlx::query(
            "DELETE FROM registration_captcha WHERE expires_at < NOW() AND status = 'pending'"
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to cleanup captchas: {}", e)))?;

        info!("Cleaned up {} expired captchas", result.rows_affected());
        Ok(result.rows_affected())
    }
}
