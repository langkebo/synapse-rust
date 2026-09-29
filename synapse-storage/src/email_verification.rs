use serde_json::Value;
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::error::ApiError;

/// The `EmailVerificationToken` struct.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EmailVerificationToken {
    /// The `id` field.
    pub id: i64,
    /// The `user_id` field.
    pub user_id: Option<String>,
    /// The `email` field.
    pub email: String,
    /// The `token` field.
    pub token: String,
    /// The `expires_at` field.
    pub expires_at: Option<i64>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `is_used` field.
    pub is_used: bool,
    /// The `session_data` field.
    pub session_data: Option<serde_json::Value>,
}

// ── Trait ───────────────────────────────────────────────────────────────

// ── Postgres implementation ─────────────────────────────────────────────

/// The `EmailVerificationStorage` struct.
#[derive(Clone)]
pub struct EmailVerificationStorage {
    /// The `pool` field.
    pub pool: Arc<Pool<Postgres>>,
}

impl EmailVerificationStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    /// See [`create_verification_token`].
    pub async fn create_verification_token(
        &self,
        email: &str,
        token: &str,
        expires_in_seconds: i64,
        user_id: Option<&str>,
        session_data: Option<serde_json::Value>,
    ) -> Result<i64, sqlx::Error> {
        let now = current_timestamp_millis();
        let expires_at = now + expires_in_seconds * 1000;

        let row = sqlx::query_as::<_, TokenIdRow>(
            r"
            INSERT INTO email_verification_tokens (email, token, expires_at, created_ts, is_used, user_id, session_data)
            VALUES ($1, $2, $3, $4, FALSE, $5, $6)
            RETURNING id
            ",
        )
        .bind(email)
        .bind(token)
        .bind(expires_at)
        .bind(now)
        .bind(user_id)
        .bind(session_data)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.id)
    }

    /// See [`mark_token_used`].
    pub async fn mark_token_used(&self, token_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE email_verification_tokens SET is_used = TRUE WHERE id = $1
            ",
        )
        .bind(token_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// Shared validation for submitToken: checks existence, usage, expiration,
    /// token match, and client_secret match against session_data. Marks the
    /// token as used on success.
    pub async fn validate_and_consume_token(
        &self,
        token_id: i64,
        submitted_token: &str,
        client_secret: &str,
    ) -> Result<EmailVerificationToken, ApiError> {
        let verification_token = self
            .get_verification_token_by_id(token_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get verification token", e))?;

        let verification_token = verification_token
            .ok_or_else(|| ApiError::bad_request("Invalid session ID or session not found".to_string()))?;

        if verification_token.is_used {
            return Err(ApiError::bad_request("Verification token has already been used".to_string()));
        }

        let now = current_timestamp_millis();
        if verification_token.expires_at.is_none_or(|expires_at| expires_at < now) {
            return Err(ApiError::bad_request("Verification token has expired".to_string()));
        }

        if verification_token.token != submitted_token {
            return Err(ApiError::bad_request("Invalid verification token".to_string()));
        }

        // Mirror the legacy session_data->client_secret check.
        let stored_secret = match verification_token.session_data.as_ref() {
            Some(Value::String(s)) => Some(s.as_str()),
            Some(Value::Object(map)) => map.get("client_secret").and_then(|v| v.as_str()),
            _ => None,
        };
        if stored_secret != Some(client_secret) {
            return Err(ApiError::bad_request("Client secret mismatch".to_string()));
        }

        self.mark_token_used(token_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to mark token as used", e))?;
        Ok(verification_token)
    }

    /// See [`get_verification_token_by_id`].
    pub async fn get_verification_token_by_id(
        &self,
        token_id: i64,
    ) -> Result<Option<EmailVerificationToken>, sqlx::Error> {
        let token_record = sqlx::query_as::<_, EmailVerificationToken>(
            r"
            SELECT id, user_id, email, token, expires_at, created_ts, is_used, session_data
            FROM email_verification_tokens
            WHERE id = $1
            ",
        )
        .bind(token_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(token_record)
    }

    /// 原子地"消费"一次已校验的会话：DELETE ... RETURNING 在单条 SQL 中
    /// 完成"取出 + 删除"，保证两个并发请求里只有一个能拿到行，另一个
    /// 拿到 `Ok(None)`。配合 `expires_at > now` 与 `is_used = TRUE`
    /// 一并放在 WHERE 里，避免单独 SELECT/UPDATE 之间的 TOCTOU 窗口。
    ///
    /// 仅返回 `email`、`user_id`、`session_data`，调用方据此完成业务校验
    /// （client_secret、purpose 等）。一旦此函数返回 `Some`，行就已物理
    /// 删除，无法被重放。
    pub async fn claim_used_token(&self, token_id: i64) -> Result<Option<EmailVerificationToken>, sqlx::Error> {
        let now = current_timestamp_millis();
        let row = sqlx::query_as::<_, EmailVerificationToken>(
            r"
            DELETE FROM email_verification_tokens
            WHERE id = $1 AND is_used = TRUE AND expires_at > $2
            RETURNING id, user_id, email, token, expires_at, created_ts, is_used, session_data
            ",
        )
        .bind(token_id)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`cleanup_expired_tokens`].
    pub async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error> {
        let now = current_timestamp_millis();
        let result = sqlx::query(
            r"
            DELETE FROM email_verification_tokens WHERE expires_at < $1
            ",
        )
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(result.rows_affected() as i64)
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct TokenIdRow {
    pub id: i64,
}

// ── Delegation impl ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_verification_token_struct() {
        let token = EmailVerificationToken {
            id: 1,
            user_id: Some("@test:example.com".to_string()),
            email: "test@example.com".to_string(),
            token: "abc123".to_string(),
            expires_at: Some(current_timestamp_millis() + 3600000),
            created_ts: current_timestamp_millis(),
            is_used: false,
            session_data: None,
        };

        assert_eq!(token.id, 1);
        assert_eq!(token.email, "test@example.com");
        assert!(!token.is_used);
    }

    #[test]
    fn test_email_verification_token_with_session_data() {
        let token = EmailVerificationToken {
            id: 2,
            user_id: Some("@user:example.com".to_string()),
            email: "user@example.com".to_string(),
            token: "token456".to_string(),
            expires_at: Some(current_timestamp_millis() + 3600000),
            created_ts: current_timestamp_millis(),
            is_used: false,
            session_data: Some(serde_json::json!({"key": "value"})),
        };
        assert!(token.session_data.is_some());
    }

    #[test]
    fn test_email_verification_token_expired() {
        let current_ts = current_timestamp_millis();
        let token = EmailVerificationToken {
            id: 3,
            user_id: Some("@expired:example.com".to_string()),
            email: "expired@example.com".to_string(),
            token: "expired_token".to_string(),
            expires_at: Some(current_ts - 3600000),
            created_ts: current_ts - 7200000,
            is_used: false,
            session_data: None,
        };
        assert!(token.expires_at.unwrap() < current_ts);
    }

    #[test]
    fn test_email_verification_token_already_used() {
        let token = EmailVerificationToken {
            id: 4,
            user_id: Some("@used:example.com".to_string()),
            email: "used@example.com".to_string(),
            token: "used_token".to_string(),
            expires_at: Some(current_timestamp_millis() + 3600000),
            created_ts: current_timestamp_millis(),
            is_used: true,
            session_data: None,
        };
        assert!(token.is_used);
    }

    /// 真 baseline 上的电子邮件验证会话往返（C48-0）。
    ///
    /// 替换掉原先那条 **R9 违规**用例：它在 `prepare_empty_isolated_test_pool()` 造的**空 schema**
    /// 上手写 `CREATE TABLE email_verification_tokens`，而那份 DDL **漏掉了真 schema 的
    /// `token TEXT NOT NULL UNIQUE`** —— 正是 D-31 家族"手搭夹具掩盖真约束"的形态；
    /// 它还在拿不到库时静默 `return`（门禁从此看不出它没跑）。
    /// 现在一律用 `crate::test_isolation::isolated_test_pool()`（克隆真 v12 模板，R9），
    /// 且**只用被测 API 造数据**（不引入测试区自建 DDL ⇒ 白名单条目同时删除）。
    #[tokio::test]
    async fn email_verification_lifecycle_round_trip_on_the_migration_template() {
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
        let pool = isolated.pool();
        let storage = EmailVerificationStorage::new(&pool);

        // create + get_by_id：可空列（user_id / session_data）两侧都要往返。
        let with_session = storage
            .create_verification_token(
                "with-session@example.com",
                "tok-session",
                3600,
                Some("@alice:test"),
                Some(serde_json::json!({"client_secret": "s3cret", "purpose": "password_reset"})),
            )
            .await
            .expect("create with session");
        let bare = storage
            .create_verification_token("bare@example.com", "tok-bare", 3600, None, None)
            .await
            .expect("create bare");
        assert_ne!(with_session, bare);

        let row = storage.get_verification_token_by_id(with_session).await.unwrap().expect("row");
        assert_eq!(row.email, "with-session@example.com");
        assert_eq!(row.token, "tok-session");
        assert_eq!(row.user_id.as_deref(), Some("@alice:test"));
        assert!(!row.is_used);
        assert!(row.session_data.is_some());
        assert!(row.expires_at.is_some());
        let bare_row = storage.get_verification_token_by_id(bare).await.unwrap().expect("bare row");
        assert_eq!(bare_row.user_id, None);
        assert_eq!(bare_row.session_data, None);
        assert!(storage.get_verification_token_by_id(9_999_999).await.unwrap().is_none());

        // 真 schema 的 `token` 是 **UNIQUE** —— 手写 DDL 版本漏掉了这条，重复 token 会静默成功。
        let duplicate = storage.create_verification_token("dup@example.com", "tok-session", 3600, None, None).await;
        assert!(duplicate.is_err(), "token UNIQUE 约束必须拒绝重复 token");

        // validate_and_consume_token：客户端密钥不符 / token 不符 / 过期 / 已用过 都必须被拒。
        assert!(storage.validate_and_consume_token(with_session, "tok-session", "wrong").await.is_err());
        assert!(storage.validate_and_consume_token(with_session, "wrong-token", "s3cret").await.is_err());
        assert!(!storage.get_verification_token_by_id(with_session).await.unwrap().unwrap().is_used);

        let consumed = storage
            .validate_and_consume_token(with_session, "tok-session", "s3cret")
            .await
            .expect("valid token must be consumed");
        assert_eq!(consumed.id, with_session);
        assert!(storage.get_verification_token_by_id(with_session).await.unwrap().unwrap().is_used);
        // 已用过 ⇒ 再校验必须失败（不可重放）。
        assert!(storage.validate_and_consume_token(with_session, "tok-session", "s3cret").await.is_err());

        // 过期：`expires_in_seconds` 为负 ⇒ `expires_at` 已在过去。
        let expired =
            storage.create_verification_token("expired@example.com", "tok-expired", -10, None, None).await.unwrap();
        assert!(storage.validate_and_consume_token(expired, "tok-expired", "").await.is_err());

        // claim_used_token：未使用过 ⇒ None；校验（=is_used）之后 ⇒ Some 且行被**物理删除**（不可重放）。
        assert!(storage.claim_used_token(bare).await.unwrap().is_none());
        let claimed = storage.claim_used_token(with_session).await.unwrap().expect("used token is claimable");
        assert_eq!(claimed.token, "tok-session");
        assert!(storage.get_verification_token_by_id(with_session).await.unwrap().is_none());
        assert!(storage.claim_used_token(with_session).await.unwrap().is_none());

        // cleanup_expired_tokens：只清过期行，活着的行必须留下。
        let removed = storage.cleanup_expired_tokens().await.unwrap();
        assert!(removed >= 1, "at least the expired row must be removed");
        assert!(storage.get_verification_token_by_id(expired).await.unwrap().is_none());
        assert!(storage.get_verification_token_by_id(bare).await.unwrap().is_some());
    }
}
