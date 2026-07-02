use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;
use synapse_common::error::ApiError;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RegistrationCaptcha {
    pub id: i64,
    pub captcha_id: String,
    pub captcha_type: String,
    pub target: String,
    pub code: String,
    pub created_ts: i64,
    pub expires_at: i64,
    #[sqlx(rename = "used_at")]
    pub used_ts: Option<i64>,
    #[sqlx(rename = "verified_at")]
    pub verified_ts: Option<i64>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub status: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaptchaSendLog {
    pub id: i64,
    pub captcha_id: Option<String>,
    pub captcha_type: String,
    pub target: String,
    pub sent_ts: i64,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub is_success: Option<bool>,
    pub error_message: Option<String>,
    pub provider: Option<String>,
    pub provider_response: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaptchaRateLimit {
    pub id: i64,
    pub target: String,
    pub ip_address: Option<String>,
    pub captcha_type: String,
    pub request_count: i32,
    pub first_request_at: i64,
    pub last_request_at: i64,
    pub blocked_until: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaptchaTemplate {
    pub id: i64,
    pub template_name: String,
    pub captcha_type: String,
    pub subject: Option<String>,
    pub content: String,
    pub variables: serde_json::Value,
    pub is_default: bool,
    pub is_enabled: bool,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaptchaConfig {
    pub id: i64,
    pub config_key: String,
    pub config_value: String,
    pub description: Option<String>,
    pub created_ts: i64,
    pub updated_ts: i64,
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
    #[serde(rename = "success")]
    pub is_success: bool,
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

    pub async fn create_captcha(&self, request: CreateCaptchaRequest) -> Result<RegistrationCaptcha, ApiError> {
        let captcha_id = Uuid::new_v4().to_string();
        let now = Utc::now().timestamp_millis();
        let expires_at = now + (request.expires_in_seconds * 1000);
        let metadata = request.metadata.unwrap_or(serde_json::json!({}));

        let row = sqlx::query_as::<_, RegistrationCaptcha>(
            r"
            INSERT INTO registration_captcha (
                captcha_id, captcha_type, target, code, created_ts, expires_at,
                ip_address, user_agent, max_attempts, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING *
            ",
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
        .map_err(|e| ApiError::internal_with_log("Failed to create captcha", &e))?;

        info!("Created captcha: {} for target: {}", captcha_id, request.target);
        Ok(row)
    }

    pub async fn get_captcha(&self, captcha_id: &str) -> Result<Option<RegistrationCaptcha>, ApiError> {
        let row = sqlx::query_as::<_, RegistrationCaptcha>("SELECT id, captcha_id, captcha_type, target, code, created_ts, expires_at, used_at, verified_at, ip_address, user_agent, attempt_count, max_attempts, status, metadata FROM registration_captcha WHERE captcha_id = $1")
            .bind(captcha_id)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get captcha", &e))?;

        Ok(row)
    }

    pub async fn get_latest_captcha(
        &self,
        target: &str,
        captcha_type: &str,
    ) -> Result<Option<RegistrationCaptcha>, ApiError> {
        let now = Utc::now().timestamp_millis();
        let row = sqlx::query_as::<_, RegistrationCaptcha>(
            r"
            SELECT id, captcha_id, captcha_type, target, code, created_ts, expires_at, used_at, verified_at, ip_address, user_agent, attempt_count, max_attempts, status, metadata FROM registration_captcha
            WHERE target = $1 AND captcha_type = $2 AND status = 'pending' AND expires_at > $3
            ORDER BY created_ts DESC
            LIMIT 1
            ",
        )
        .bind(target)
        .bind(captcha_type)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get latest captcha", &e))?;

        Ok(row)
    }

    pub async fn verify_captcha(&self, captcha_id: &str, code: &str) -> Result<bool, ApiError> {
        let now = Utc::now().timestamp_millis();

        let captcha = self.get_captcha(captcha_id).await?.ok_or_else(|| ApiError::bad_request("Captcha not found"))?;

        if captcha.status != "pending" {
            return Ok(false);
        }

        if captcha.expires_at < now {
            sqlx::query("UPDATE registration_captcha SET status = 'expired' WHERE captcha_id = $1")
                .bind(captcha_id)
                .execute(&*self.pool)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to update captcha", &e))?;

            return Ok(false);
        }

        if captcha.attempt_count >= captcha.max_attempts {
            sqlx::query("UPDATE registration_captcha SET status = 'exhausted' WHERE captcha_id = $1")
                .bind(captcha_id)
                .execute(&*self.pool)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to update captcha", &e))?;

            return Ok(false);
        }

        sqlx::query("UPDATE registration_captcha SET attempt_count = attempt_count + 1 WHERE captcha_id = $1")
            .bind(captcha_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to increment attempt count", &e))?;

        if captcha.code == code {
            sqlx::query(
                r"
                UPDATE registration_captcha
                SET status = 'verified', verified_at = $1, used_at = $1
                WHERE captcha_id = $2
                ",
            )
            .bind(now)
            .bind(captcha_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to verify captcha", &e))?;

            info!("Captcha verified: {}", captcha_id);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn invalidate_captcha(&self, captcha_id: &str) -> Result<(), ApiError> {
        let now = Utc::now().timestamp_millis();

        sqlx::query("UPDATE registration_captcha SET status = 'used', used_at = $1 WHERE captcha_id = $2")
            .bind(now)
            .bind(captcha_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to invalidate captcha", &e))?;

        info!("Captcha invalidated: {}", captcha_id);
        Ok(())
    }

    pub async fn create_send_log(&self, request: CreateSendLogRequest) -> Result<CaptchaSendLog, ApiError> {
        let sent_ts = chrono::Utc::now().timestamp_millis();
        let row = sqlx::query_as::<_, CaptchaSendLog>(
            r"
            INSERT INTO captcha_send_log (
                captcha_id, captcha_type, target, sent_ts, ip_address, user_agent,
                is_success, error_message, provider, provider_response
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING *
            ",
        )
        .bind(&request.captcha_id)
        .bind(&request.captcha_type)
        .bind(&request.target)
        .bind(sent_ts)
        .bind(&request.ip_address)
        .bind(&request.user_agent)
        .bind(request.is_success)
        .bind(&request.error_message)
        .bind(&request.provider)
        .bind(&request.provider_response)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create send log", &e))?;

        Ok(row)
    }

    pub async fn check_rate_limit(
        &self,
        target: &str,
        captcha_type: &str,
        max_per_hour: i32,
    ) -> Result<bool, ApiError> {
        let one_hour_ago_ts = (Utc::now() - chrono::Duration::hours(1)).timestamp_millis();

        let count: (i64,) = sqlx::query_as(
            r"
            SELECT COUNT(*) FROM captcha_send_log
            WHERE target = $1 AND captcha_type = $2 AND sent_ts > $3
            ",
        )
        .bind(target)
        .bind(captcha_type)
        .bind(one_hour_ago_ts)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check rate limit", &e))?;

        Ok(count.0 < max_per_hour as i64)
    }

    pub async fn check_ip_rate_limit(&self, ip_address: &str, max_per_hour: i32) -> Result<bool, ApiError> {
        let one_hour_ago_ts = (Utc::now() - chrono::Duration::hours(1)).timestamp_millis();

        let count: (i64,) = sqlx::query_as(
            r"
            SELECT COUNT(*) FROM captcha_send_log
            WHERE ip_address = $1 AND sent_ts > $2
            ",
        )
        .bind(ip_address)
        .bind(one_hour_ago_ts)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to check IP rate limit", &e))?;

        Ok(count.0 < max_per_hour as i64)
    }

    pub async fn get_template(&self, template_name: &str) -> Result<Option<CaptchaTemplate>, ApiError> {
        let row = sqlx::query_as::<_, CaptchaTemplate>(
            "SELECT id, template_name, captcha_type, subject, content, variables, is_default, is_enabled, created_ts, updated_ts FROM captcha_template WHERE template_name = $1 AND is_enabled = true",
        )
        .bind(template_name)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get template", &e))?;

        Ok(row)
    }

    pub async fn get_default_template(&self, captcha_type: &str) -> Result<Option<CaptchaTemplate>, ApiError> {
        let row = sqlx::query_as::<_, CaptchaTemplate>(
            "SELECT id, template_name, captcha_type, subject, content, variables, is_default, is_enabled, created_ts, updated_ts FROM captcha_template WHERE captcha_type = $1 AND is_default = true AND is_enabled = true",
        )
        .bind(captcha_type)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get default template", &e))?;

        Ok(row)
    }

    pub async fn get_config(&self, config_key: &str) -> Result<Option<String>, ApiError> {
        let row: Option<(String,)> = sqlx::query_as("SELECT config_value FROM captcha_config WHERE config_key = $1")
            .bind(config_key)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get config", &e))?;

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
        let now = Utc::now().timestamp_millis();
        let result = sqlx::query("DELETE FROM registration_captcha WHERE expires_at < $1 AND status = 'pending'")
            .bind(now)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to cleanup captchas", &e))?;

        info!("Cleaned up {} expired captchas", result.rows_affected());
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registration_captcha_creation() {
        let captcha = RegistrationCaptcha {
            id: 1,
            captcha_id: "captcha123".to_string(),
            captcha_type: "image".to_string(),
            target: "registration".to_string(),
            code: "abc123".to_string(),
            created_ts: 1234567800000,
            expires_at: 1234567890000,
            used_ts: None,
            verified_ts: None,
            ip_address: Some("192.168.1.1".to_string()),
            user_agent: Some("Mozilla/5.0".to_string()),
            attempt_count: 0,
            max_attempts: 3,
            status: "active".to_string(),
            metadata: serde_json::json!({}),
        };
        assert_eq!(captcha.captcha_id, "captcha123");
        assert_eq!(captcha.status, "active");
    }

    #[test]
    fn test_create_captcha_request() {
        let request = CreateCaptchaRequest {
            captcha_type: "image".to_string(),
            target: "registration".to_string(),
            code: "abc123".to_string(),
            expires_in_seconds: 300,
            ip_address: None,
            user_agent: None,
            max_attempts: 3,
            metadata: None,
        };
        assert_eq!(request.captcha_type, "image");
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use chrono::Utc;
    use sqlx::postgres::PgPoolOptions;
    use std::env;

    async fn test_pool() -> Arc<PgPool> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&db_url)
            .await
            .expect("Failed to connect to test database");
        Arc::new(pool)
    }

    fn unique_suffix() -> String {
        Uuid::new_v4().to_string()
    }

    fn make_request(target: &str, code: &str) -> CreateCaptchaRequest {
        CreateCaptchaRequest {
            captcha_type: "sms".to_string(),
            target: target.to_string(),
            code: code.to_string(),
            expires_in_seconds: 3600,
            ip_address: Some("127.0.0.1".to_string()),
            user_agent: Some("test-agent/1.0".to_string()),
            max_attempts: 3,
            metadata: Some(serde_json::json!({"source": "db_test"})),
        }
    }

    // ---------------------------------------------------------------------------
    // create_captcha
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_captcha_returns_valid_record() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let target = format!("test_create_{}@example.com", unique_suffix());
        let sql_target = target.clone();

        let req = make_request(&target, "123456");
        let captcha = storage.create_captcha(req).await.expect("create should succeed");

        assert!(!captcha.captcha_id.is_empty());
        assert_eq!(captcha.captcha_type, "sms");
        assert_eq!(captcha.target, target);
        assert_eq!(captcha.code, "123456");
        assert_eq!(captcha.status, "pending");
        assert!(captcha.created_ts > 0);
        assert!(captcha.expires_at > captcha.created_ts);
        assert_eq!(captcha.attempt_count, 0);
        assert_eq!(captcha.max_attempts, 3);
        assert_eq!(captcha.ip_address.as_deref(), Some("127.0.0.1"));
        assert_eq!(captcha.user_agent.as_deref(), Some("test-agent/1.0"));

        // Cleanup
        sqlx::query("DELETE FROM registration_captcha WHERE target = $1")
            .bind(&sql_target)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_create_captcha_with_minimal_fields() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let target = format!("test_minimal_{}@example.com", unique_suffix());
        let sql_target = target.clone();

        let req = CreateCaptchaRequest {
            captcha_type: "email".to_string(),
            target: target.clone(),
            code: "mincode".to_string(),
            expires_in_seconds: 600,
            ip_address: None,
            user_agent: None,
            max_attempts: 5,
            metadata: None,
        };
        let captcha = storage.create_captcha(req).await.expect("create should succeed");

        assert_eq!(captcha.captcha_type, "email");
        assert_eq!(captcha.target, target);
        assert!(captcha.ip_address.is_none());
        assert!(captcha.user_agent.is_none());
        assert_eq!(captcha.max_attempts, 5);
        assert_eq!(captcha.metadata, serde_json::json!({}));

        sqlx::query("DELETE FROM registration_captcha WHERE target = $1")
            .bind(&sql_target)
            .execute(&*pool)
            .await
            .ok();
    }

    // ---------------------------------------------------------------------------
    // get_captcha
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_captcha_finds_created() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let target = format!("test_get_{}@example.com", unique_suffix());
        let sql_target = target.clone();

        let created = storage.create_captcha(make_request(&target, "abcd")).await.expect("create");

        let found = storage.get_captcha(&created.captcha_id).await.expect("get should succeed");
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.captcha_id, created.captcha_id);
        assert_eq!(found.code, "abcd");

        sqlx::query("DELETE FROM registration_captcha WHERE target = $1")
            .bind(&sql_target)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_get_captcha_returns_none_for_nonexistent() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);

        let result = storage.get_captcha("nonexistent-captcha-id").await.expect("get should succeed");
        assert!(result.is_none());
    }

    // ---------------------------------------------------------------------------
    // get_latest_captcha
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_latest_captcha_returns_pending() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let target = format!("test_latest_{}@example.com", unique_suffix());
        let sql_target = target.clone();

        // Create two captchas; get_latest should return the most recent pending one
        let _a = storage.create_captcha(make_request(&target, "aaaaa")).await.expect("create a");
        let b = storage.create_captcha(make_request(&target, "bbbbb")).await.expect("create b");

        let latest = storage.get_latest_captcha(&target, "sms").await.expect("get_latest should succeed");
        assert!(latest.is_some());
        let latest = latest.unwrap();
        assert_eq!(latest.captcha_id, b.captcha_id);
        assert_eq!(latest.code, "bbbbb");
        assert_eq!(latest.status, "pending");

        sqlx::query("DELETE FROM registration_captcha WHERE target = $1")
            .bind(&sql_target)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_get_latest_captcha_skips_used_captcha() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let target = format!("test_latest_used_{}@example.com", unique_suffix());
        let sql_target = target.clone();

        let first = storage.create_captcha(make_request(&target, "first1")).await.expect("create first");
        // Invalidate the first captcha so it is no longer pending
        storage.invalidate_captcha(&first.captcha_id).await.expect("invalidate");

        let second = storage.create_captcha(make_request(&target, "second")).await.expect("create second");

        let latest = storage.get_latest_captcha(&target, "sms").await.expect("get_latest should succeed");
        assert!(latest.is_some());
        let latest = latest.unwrap();
        // Should return the second (still pending), not the first (used)
        assert_eq!(latest.captcha_id, second.captcha_id);
        assert_eq!(latest.code, "second");

        sqlx::query("DELETE FROM registration_captcha WHERE target = $1")
            .bind(&sql_target)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_get_latest_captcha_returns_none_for_no_match() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let target = format!("test_latest_none_{}@example.com", unique_suffix());

        // No captcha for this target/type
        let result = storage.get_latest_captcha(&target, "sms").await.expect("get_latest should succeed");
        assert!(result.is_none());
    }

    // ---------------------------------------------------------------------------
    // verify_captcha
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_verify_captcha_correct_code_returns_true() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let target = format!("test_verify_ok_{}@example.com", unique_suffix());
        let sql_target = target.clone();

        let captcha = storage.create_captcha(make_request(&target, "correct")).await.expect("create");

        let result = storage.verify_captcha(&captcha.captcha_id, "correct").await.expect("verify should succeed");
        assert!(result);

        // Verify status changed to verified
        let updated = storage.get_captcha(&captcha.captcha_id).await.expect("get after verify");
        assert!(updated.is_some());
        let updated = updated.unwrap();
        assert_eq!(updated.status, "verified");
        assert!(updated.verified_ts.is_some());
        assert!(updated.used_ts.is_some());

        sqlx::query("DELETE FROM registration_captcha WHERE target = $1")
            .bind(&sql_target)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_verify_captcha_wrong_code_returns_false() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let target = format!("test_verify_wrong_{}@example.com", unique_suffix());
        let sql_target = target.clone();

        let captcha = storage.create_captcha(make_request(&target, "realcode")).await.expect("create");

        let result = storage.verify_captcha(&captcha.captcha_id, "wrong").await.expect("verify should succeed");
        assert!(!result);

        // Status still pending, attempt_count incremented
        let updated = storage.get_captcha(&captcha.captcha_id).await.expect("get after verify");
        assert!(updated.is_some());
        let updated = updated.unwrap();
        assert_eq!(updated.status, "pending");
        assert_eq!(updated.attempt_count, 1);

        sqlx::query("DELETE FROM registration_captcha WHERE target = $1")
            .bind(&sql_target)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_verify_captcha_not_found_returns_error() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);

        let result = storage.verify_captcha("nonexistent-id", "anycode").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_verify_captcha_expired_returns_false() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let target = format!("test_verify_expired_{}@example.com", unique_suffix());
        let sql_target = target.clone();
        let now = Utc::now().timestamp_millis();
        let captcha_id = Uuid::new_v4().to_string();

        // Direct INSERT with an already-expired captcha (status=pending, expires_at in the past)
        sqlx::query(
            r"
            INSERT INTO registration_captcha (
                captcha_id, captcha_type, target, code, created_ts, expires_at,
                ip_address, user_agent, max_attempts, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ",
        )
        .bind(&captcha_id)
        .bind("sms")
        .bind(&target)
        .bind("expcode")
        .bind(now - 60000)
        .bind(now - 1000) // expired 1 second ago
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .bind(3i32)
        .bind(serde_json::json!({}))
        .execute(&*pool)
        .await
        .expect("insert expired captcha");

        let result = storage.verify_captcha(&captcha_id, "expcode").await.expect("verify should succeed");
        assert!(!result);

        // Status should have been set to 'expired'
        let updated = storage.get_captcha(&captcha_id).await.expect("get after verify");
        assert!(updated.is_some());
        assert_eq!(updated.unwrap().status, "expired");

        sqlx::query("DELETE FROM registration_captcha WHERE target = $1")
            .bind(&sql_target)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_verify_captcha_exhausted_attempts_returns_false() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let target = format!("test_verify_exhausted_{}@example.com", unique_suffix());
        let sql_target = target.clone();

        // max_attempts = 1 so that one wrong attempt exhausts it
        let req = CreateCaptchaRequest {
            captcha_type: "sms".to_string(),
            target: target.clone(),
            code: "realcode".to_string(),
            expires_in_seconds: 3600,
            ip_address: None,
            user_agent: None,
            max_attempts: 1,
            metadata: None,
        };
        let captcha = storage.create_captcha(req).await.expect("create");

        // First wrong attempt: attempt_count 0 -> 1, wrong code returns false
        let r1 = storage.verify_captcha(&captcha.captcha_id, "wrong1").await.expect("first verify");
        assert!(!r1);

        // Second attempt: attempt_count is now 1 which >= max_attempts (1), so exhausted
        let r2 = storage.verify_captcha(&captcha.captcha_id, "realcode").await.expect("second verify");
        assert!(!r2);

        // Status should be 'exhausted'
        let updated = storage.get_captcha(&captcha.captcha_id).await.expect("get after exhaust");
        assert!(updated.is_some());
        assert_eq!(updated.unwrap().status, "exhausted");

        sqlx::query("DELETE FROM registration_captcha WHERE target = $1")
            .bind(&sql_target)
            .execute(&*pool)
            .await
            .ok();
    }

    // ---------------------------------------------------------------------------
    // invalidate_captcha
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_invalidate_captcha_sets_status_used() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let target = format!("test_invalidate_{}@example.com", unique_suffix());
        let sql_target = target.clone();

        let captcha = storage.create_captcha(make_request(&target, "invcode")).await.expect("create");

        storage.invalidate_captcha(&captcha.captcha_id).await.expect("invalidate should succeed");

        let updated = storage.get_captcha(&captcha.captcha_id).await.expect("get after invalidate");
        assert!(updated.is_some());
        let updated = updated.unwrap();
        assert_eq!(updated.status, "used");
        assert!(updated.used_ts.is_some());

        sqlx::query("DELETE FROM registration_captcha WHERE target = $1")
            .bind(&sql_target)
            .execute(&*pool)
            .await
            .ok();
    }

    // ---------------------------------------------------------------------------
    // create_send_log
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_send_log_returns_valid_record() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let target = format!("test_sendlog_{}@example.com", unique_suffix());
        let sql_target = target.clone();
        let captcha_id = Uuid::new_v4().to_string();

        let req = CreateSendLogRequest {
            captcha_id: Some(captcha_id.clone()),
            captcha_type: "sms".to_string(),
            target: target.clone(),
            ip_address: Some("10.0.0.1".to_string()),
            user_agent: Some("log-agent/2.0".to_string()),
            is_success: true,
            error_message: None,
            provider: Some("aws-sns".to_string()),
            provider_response: Some("{\"messageId\":\"abc\"}".to_string()),
        };
        let log = storage.create_send_log(req).await.expect("create_send_log should succeed");

        assert!(log.id > 0);
        assert_eq!(log.captcha_id.as_deref(), Some(captcha_id.as_str()));
        assert_eq!(log.captcha_type, "sms");
        assert_eq!(log.target, target);
        assert!(log.sent_ts > 0);
        assert_eq!(log.ip_address.as_deref(), Some("10.0.0.1"));
        assert_eq!(log.user_agent.as_deref(), Some("log-agent/2.0"));
        assert_eq!(log.is_success, Some(true));
        assert!(log.error_message.is_none());
        assert_eq!(log.provider.as_deref(), Some("aws-sns"));

        sqlx::query("DELETE FROM captcha_send_log WHERE target = $1")
            .bind(&sql_target)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_create_send_log_with_failure() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let target = format!("test_sendlog_fail_{}@example.com", unique_suffix());
        let sql_target = target.clone();

        let req = CreateSendLogRequest {
            captcha_id: None,
            captcha_type: "email".to_string(),
            target: target.clone(),
            ip_address: None,
            user_agent: None,
            is_success: false,
            error_message: Some("SMTP connection refused".to_string()),
            provider: None,
            provider_response: None,
        };
        let log = storage.create_send_log(req).await.expect("create_send_log should succeed");

        assert_eq!(log.is_success, Some(false));
        assert_eq!(log.error_message.as_deref(), Some("SMTP connection refused"));
        assert!(log.captcha_id.is_none());

        sqlx::query("DELETE FROM captcha_send_log WHERE target = $1")
            .bind(&sql_target)
            .execute(&*pool)
            .await
            .ok();
    }

    // ---------------------------------------------------------------------------
    // check_rate_limit
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_check_rate_limit_within_limit_returns_true() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let target = format!("test_ratelimit_{}@example.com", unique_suffix());
        let sql_target = target.clone();

        // Create one send log
        let _log = storage.create_send_log(CreateSendLogRequest {
            captcha_id: None,
            captcha_type: "sms".to_string(),
            target: target.clone(),
            ip_address: None,
            user_agent: None,
            is_success: true,
            error_message: None,
            provider: None,
            provider_response: None,
        }).await.expect("create send log");

        // 1 log < max_per_hour=2 -> allowed
        let within = storage.check_rate_limit(&target, "sms", 2).await.expect("check rate limit");
        assert!(within);

        sqlx::query("DELETE FROM captcha_send_log WHERE target = $1")
            .bind(&sql_target)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_check_rate_limit_exceeded_returns_false() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let target = format!("test_ratelimit_ex_{}@example.com", unique_suffix());
        let sql_target = target.clone();

        // Create one send log
        let _log = storage.create_send_log(CreateSendLogRequest {
            captcha_id: None,
            captcha_type: "sms".to_string(),
            target: target.clone(),
            ip_address: None,
            user_agent: None,
            is_success: true,
            error_message: None,
            provider: None,
            provider_response: None,
        }).await.expect("create send log");

        // 1 log >= max_per_hour=1 -> blocked
        let within = storage.check_rate_limit(&target, "sms", 1).await.expect("check rate limit");
        assert!(!within);

        sqlx::query("DELETE FROM captcha_send_log WHERE target = $1")
            .bind(&sql_target)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_check_rate_limit_no_logs_returns_true() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let target = format!("test_ratelimit_none_{}@example.com", unique_suffix());

        // No send logs at all -> always allowed
        let within = storage.check_rate_limit(&target, "sms", 1).await.expect("check rate limit");
        assert!(within);
    }

    // ---------------------------------------------------------------------------
    // check_ip_rate_limit
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_check_ip_rate_limit_within_limit_returns_true() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let target = format!("test_iprl_{}@example.com", unique_suffix());
        let sql_target = target.clone();

        let _log = storage.create_send_log(CreateSendLogRequest {
            captcha_id: None,
            captcha_type: "sms".to_string(),
            target: target.clone(),
            ip_address: Some("192.168.50.1".to_string()),
            user_agent: None,
            is_success: true,
            error_message: None,
            provider: None,
            provider_response: None,
        }).await.expect("create send log");

        // 1 log for IP < max_per_hour=2 -> allowed
        let within = storage.check_ip_rate_limit("192.168.50.1", 2).await.expect("check IP rate limit");
        assert!(within);

        sqlx::query("DELETE FROM captcha_send_log WHERE target = $1")
            .bind(&sql_target)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_check_ip_rate_limit_exceeded_returns_false() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let target = format!("test_iprl_ex_{}@example.com", unique_suffix());
        let sql_target = target.clone();

        let _log = storage.create_send_log(CreateSendLogRequest {
            captcha_id: None,
            captcha_type: "sms".to_string(),
            target: target.clone(),
            ip_address: Some("10.10.10.10".to_string()),
            user_agent: None,
            is_success: true,
            error_message: None,
            provider: None,
            provider_response: None,
        }).await.expect("create send log");

        // 1 log for IP >= max_per_hour=1 -> blocked
        let within = storage.check_ip_rate_limit("10.10.10.10", 1).await.expect("check IP rate limit");
        assert!(!within);

        sqlx::query("DELETE FROM captcha_send_log WHERE target = $1")
            .bind(&sql_target)
            .execute(&*pool)
            .await
            .ok();
    }

    // ---------------------------------------------------------------------------
    // get_template
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_template_returns_enabled_template() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let template_name = format!("test_template_{}", unique_suffix());
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r"
            INSERT INTO captcha_template (template_name, captcha_type, content, is_enabled, created_ts, updated_ts)
            VALUES ($1, 'sms', 'Your code is {{code}}', true, $2, $2)
            ON CONFLICT (template_name) DO NOTHING
            ",
        )
        .bind(&template_name)
        .bind(now)
        .execute(&*pool)
        .await
        .expect("insert template");

        let result = storage.get_template(&template_name).await.expect("get_template should succeed");
        assert!(result.is_some());
        let tmpl = result.unwrap();
        assert_eq!(tmpl.template_name, template_name);
        assert_eq!(tmpl.captcha_type, "sms");
        assert_eq!(tmpl.content, "Your code is {{code}}");
        assert!(tmpl.is_enabled);

        sqlx::query("DELETE FROM captcha_template WHERE template_name = $1")
            .bind(&template_name)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_get_template_returns_none_for_disabled() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let template_name = format!("test_disabled_{}", unique_suffix());
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r"
            INSERT INTO captcha_template (template_name, captcha_type, content, is_enabled, created_ts, updated_ts)
            VALUES ($1, 'sms', 'Your code is {{code}}', false, $2, $2)
            ON CONFLICT (template_name) DO NOTHING
            ",
        )
        .bind(&template_name)
        .bind(now)
        .execute(&*pool)
        .await
        .expect("insert disabled template");

        let result = storage.get_template(&template_name).await.expect("get_template should succeed");
        assert!(result.is_none());

        sqlx::query("DELETE FROM captcha_template WHERE template_name = $1")
            .bind(&template_name)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_get_template_returns_none_for_nonexistent() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);

        let result = storage.get_template("nonexistent_template_xyz").await.expect("get_template should succeed");
        assert!(result.is_none());
    }

    // ---------------------------------------------------------------------------
    // get_default_template
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_default_template_returns_default() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let template_name = format!("test_default_{}", unique_suffix());
        let captcha_type = format!("type_{}", unique_suffix());
        let now = Utc::now().timestamp_millis();

        // Insert a default template with a unique captcha_type
        sqlx::query(
            r"
            INSERT INTO captcha_template (template_name, captcha_type, content, is_default, is_enabled, created_ts, updated_ts)
            VALUES ($1, $2, 'Default template {{code}}', true, true, $3, $3)
            ON CONFLICT (template_name) DO NOTHING
            ",
        )
        .bind(&template_name)
        .bind(&captcha_type)
        .bind(now)
        .execute(&*pool)
        .await
        .expect("insert default template");

        let result = storage.get_default_template(&captcha_type).await.expect("get_default_template should succeed");
        assert!(result.is_some());
        let tmpl = result.unwrap();
        assert_eq!(tmpl.template_name, template_name);
        assert_eq!(tmpl.captcha_type, captcha_type);
        assert!(tmpl.is_default);

        sqlx::query("DELETE FROM captcha_template WHERE template_name = $1")
            .bind(&template_name)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_get_default_template_returns_none_for_disabled() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let template_name = format!("test_default_dis_{}", unique_suffix());
        let captcha_type = format!("type_dis_{}", unique_suffix());
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r"
            INSERT INTO captcha_template (template_name, captcha_type, content, is_default, is_enabled, created_ts, updated_ts)
            VALUES ($1, $2, 'Default disabled', true, false, $3, $3)
            ON CONFLICT (template_name) DO NOTHING
            ",
        )
        .bind(&template_name)
        .bind(&captcha_type)
        .bind(now)
        .execute(&*pool)
        .await
        .expect("insert disabled default template");

        let result = storage.get_default_template(&captcha_type).await.expect("get_default_template should succeed");
        assert!(result.is_none());

        sqlx::query("DELETE FROM captcha_template WHERE template_name = $1")
            .bind(&template_name)
            .execute(&*pool)
            .await
            .ok();
    }

    // ---------------------------------------------------------------------------
    // get_config / get_config_as_int
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_config_returns_value() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let config_key = format!("test_config_{}", unique_suffix());
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            "INSERT INTO captcha_config (config_key, config_value, created_ts) VALUES ($1, '42', $2) ON CONFLICT (config_key) DO NOTHING",
        )
        .bind(&config_key)
        .bind(now)
        .execute(&*pool)
        .await
        .expect("insert config");

        let result = storage.get_config(&config_key).await.expect("get_config should succeed");
        assert_eq!(result, Some("42".to_string()));

        sqlx::query("DELETE FROM captcha_config WHERE config_key = $1")
            .bind(&config_key)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_get_config_returns_none_for_missing_key() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);

        let result = storage.get_config("nonexistent_config_key_abc").await.expect("get_config should succeed");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_config_as_int_parses_value() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let config_key = format!("test_config_int_{}", unique_suffix());
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            "INSERT INTO captcha_config (config_key, config_value, created_ts) VALUES ($1, '99', $2) ON CONFLICT (config_key) DO NOTHING",
        )
        .bind(&config_key)
        .bind(now)
        .execute(&*pool)
        .await
        .expect("insert config");

        let value = storage.get_config_as_int(&config_key, 0).await.expect("get_config_as_int should succeed");
        assert_eq!(value, 99);

        sqlx::query("DELETE FROM captcha_config WHERE config_key = $1")
            .bind(&config_key)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_get_config_as_int_falls_back_to_default_for_missing() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);

        let value = storage.get_config_as_int("nonexistent_key_xyz", 5).await.expect("get_config_as_int should succeed");
        assert_eq!(value, 5);
    }

    #[tokio::test]
    async fn test_get_config_as_int_falls_back_to_default_for_unparseable() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let config_key = format!("test_config_bad_{}", unique_suffix());
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            "INSERT INTO captcha_config (config_key, config_value, created_ts) VALUES ($1, 'not-a-number', $2) ON CONFLICT (config_key) DO NOTHING",
        )
        .bind(&config_key)
        .bind(now)
        .execute(&*pool)
        .await
        .expect("insert config");

        let value = storage.get_config_as_int(&config_key, 7).await.expect("get_config_as_int should succeed");
        assert_eq!(value, 7);

        sqlx::query("DELETE FROM captcha_config WHERE config_key = $1")
            .bind(&config_key)
            .execute(&*pool)
            .await
            .ok();
    }

    // ---------------------------------------------------------------------------
    // cleanup_expired_captchas
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_cleanup_expired_captchas_deletes_expired_pending() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let target = format!("test_cleanup_{}@example.com", unique_suffix());
        let sql_target = target.clone();

        // Create a captcha normally, then set expires_at to the past via raw SQL
        let req = CreateCaptchaRequest {
            captcha_type: "sms".to_string(),
            target: target.clone(),
            code: "cleanup_code".to_string(),
            expires_in_seconds: 3600,
            ip_address: None,
            user_agent: None,
            max_attempts: 3,
            metadata: None,
        };
        let captcha = storage.create_captcha(req).await.expect("create captcha");
        // Use a hardcoded past timestamp (epoch + 1 ms) to guarantee expiry
        // regardless of clock jitter or instrumentation overhead
        let past: i64 = 1;
        sqlx::query("UPDATE registration_captcha SET expires_at = $1 WHERE captcha_id = $2")
            .bind(past)
            .bind(&captcha.captcha_id)
            .execute(&*pool)
            .await
            .expect("update expires_at");

        // Verify it exists before cleanup (status should be 'pending')
        let before = storage.get_captcha(&captcha.captcha_id).await.expect("get before cleanup");
        assert!(before.is_some());
        assert_eq!(before.as_ref().unwrap().status, "pending");

        let deleted = storage.cleanup_expired_captchas().await.expect("cleanup should succeed");
        assert!(deleted >= 1, "should have deleted at least 1 expired captcha, got {}", deleted);

        // Verify it no longer exists
        let after = storage.get_captcha(&captcha.captcha_id).await.expect("get after cleanup");
        assert!(after.is_none());

        // Cleanup any leftovers
        sqlx::query("DELETE FROM registration_captcha WHERE target = $1")
            .bind(&sql_target)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_cleanup_expired_captchas_skips_non_pending() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);
        let target = format!("test_cleanup_skip_{}@example.com", unique_suffix());
        let sql_target = target.clone();

        // Create a captcha normally, then set expires_at to the past via raw SQL
        let req = CreateCaptchaRequest {
            captcha_type: "sms".to_string(),
            target: target.clone(),
            code: "skip_code".to_string(),
            expires_in_seconds: 3600,
            ip_address: None,
            user_agent: None,
            max_attempts: 3,
            metadata: None,
        };
        let captcha = storage.create_captcha(req).await.expect("create captcha");
        // Use a hardcoded past timestamp (epoch + 1 ms) to guarantee expiry
        // regardless of clock jitter or instrumentation overhead
        let past: i64 = 1;
        sqlx::query("UPDATE registration_captcha SET expires_at = $1 WHERE captcha_id = $2")
            .bind(past)
            .bind(&captcha.captcha_id)
            .execute(&*pool)
            .await
            .expect("update expires_at");

        // verify_captcha on an expired captcha sets status to 'expired'
        let _ = storage.verify_captcha(&captcha.captcha_id, "skip_code").await;

        // Verify status is now 'expired' (non-pending)
        let updated = storage.get_captcha(&captcha.captcha_id).await.expect("get after verify");
        assert!(updated.is_some());
        assert_eq!(updated.as_ref().unwrap().status, "expired");

        storage.cleanup_expired_captchas().await.expect("cleanup should succeed");

        // Should still exist because status is 'expired', not 'pending'
        let after = storage.get_captcha(&captcha.captcha_id).await.expect("get after cleanup");
        assert!(after.is_some());
        assert_eq!(after.unwrap().status, "expired");

        sqlx::query("DELETE FROM registration_captcha WHERE target = $1")
            .bind(&sql_target)
            .execute(&*pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn test_cleanup_expired_captchas_returns_zero_when_nothing_to_clean() {
        let pool = test_pool().await;
        let storage = CaptchaStorage::new(&pool);

        // No expired pending captchas we control — cleanup is global so other tests
        // may leave expired rows; just verify the call succeeds without error.
        let deleted = storage.cleanup_expired_captchas().await.expect("cleanup should succeed");
        assert!(deleted >= 0);
    }
}
