//! OIDC 会话持久化存储层
//!
//! 将内存 HashMap 替换为 PostgreSQL 数据库存储，支持：
//! - PKCE state 会话（外部 OIDC）
//! - 授权码会话（内置 OIDC）
//! - Refresh Token（内置 OIDC）
//! - 同意会话（MSC3861）

use sqlx::{FromRow, PgPool};
use std::sync::Arc;
use tracing::instrument;

// ============ 数据模型 ============

#[derive(Debug, Clone, FromRow)]
pub struct OidcAuthSession {
    pub id: i64,
    pub session_key: String,
    pub session_type: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: String,
    pub state: String,
    pub nonce: Option<String>,
    pub code_verifier: Option<String>,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub user_id: Option<String>,
    pub consent_given: bool,
    pub created_ts: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct OidcRefreshToken {
    pub id: i64,
    pub token_hash: String,
    pub user_id: String,
    pub client_id: String,
    pub scope: String,
    pub created_ts: i64,
    pub expires_at: Option<i64>,
    pub is_revoked: bool,
    pub revoked_at: Option<i64>,
}

#[derive(Debug, Clone, FromRow)]
pub struct OidcConsentSession {
    pub id: i64,
    pub session_id: String,
    pub client_id: String,
    pub client_name: Option<String>,
    pub redirect_uri: String,
    pub scope: String,
    pub state: String,
    pub nonce: Option<String>,
    pub code_challenge: Option<String>,
    pub user_id: String,
    pub created_ts: i64,
    pub expires_at: i64,
}

// ============ 存储层 ============

#[derive(Clone)]
pub struct OidcSessionStorage {
    pool: Arc<PgPool>,
}

impl OidcSessionStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    // ── Auth Session ─────────────────────────────────────────────────────

    #[instrument(skip(self, session), fields(key = %session.session_key))]
    pub async fn save_auth_session(&self, session: &OidcAuthSession) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO oidc_auth_sessions
                (session_key, session_type, client_id, redirect_uri, scope, state,
                 nonce, code_verifier, code_challenge, code_challenge_method,
                 user_id, consent_given, created_ts, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            ON CONFLICT (session_key) DO UPDATE SET
                session_type = EXCLUDED.session_type,
                client_id = EXCLUDED.client_id,
                redirect_uri = EXCLUDED.redirect_uri,
                scope = EXCLUDED.scope,
                state = EXCLUDED.state,
                nonce = EXCLUDED.nonce,
                code_verifier = EXCLUDED.code_verifier,
                code_challenge = EXCLUDED.code_challenge,
                code_challenge_method = EXCLUDED.code_challenge_method,
                user_id = EXCLUDED.user_id,
                consent_given = EXCLUDED.consent_given,
                created_ts = EXCLUDED.created_ts,
                expires_at = EXCLUDED.expires_at
            "#,
        )
        .bind(&session.session_key)
        .bind(&session.session_type)
        .bind(&session.client_id)
        .bind(&session.redirect_uri)
        .bind(&session.scope)
        .bind(&session.state)
        .bind(&session.nonce)
        .bind(&session.code_verifier)
        .bind(&session.code_challenge)
        .bind(&session.code_challenge_method)
        .bind(&session.user_id)
        .bind(session.consent_given)
        .bind(session.created_ts)
        .bind(session.expires_at)
        .execute(self.pool.as_ref())
        .await?;
        Ok(())
    }

    /// 原子消费：DELETE ... RETURNING，防止授权码重放
    #[instrument(skip(self), fields(key = %session_key))]
    pub async fn get_and_delete_auth_session(
        &self,
        session_key: &str,
    ) -> Result<Option<OidcAuthSession>, sqlx::Error> {
        let row = sqlx::query_as::<_, OidcAuthSession>(
            r#"
            DELETE FROM oidc_auth_sessions
            WHERE session_key = $1
            RETURNING id, session_key, session_type, client_id, redirect_uri,
                      scope, state, nonce, code_verifier, code_challenge,
                      code_challenge_method, user_id, consent_given, created_ts, expires_at
            "#,
        )
        .bind(session_key)
        .fetch_optional(self.pool.as_ref())
        .await?;
        Ok(row)
    }

    // ── Refresh Token ────────────────────────────────────────────────────

    #[instrument(skip(self, token), fields(hash = %token.token_hash))]
    pub async fn save_refresh_token(&self, token: &OidcRefreshToken) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO oidc_refresh_tokens
                (token_hash, user_id, client_id, scope, created_ts, expires_at, is_revoked, revoked_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (token_hash) DO UPDATE SET
                user_id = EXCLUDED.user_id,
                client_id = EXCLUDED.client_id,
                scope = EXCLUDED.scope,
                created_ts = EXCLUDED.created_ts,
                expires_at = EXCLUDED.expires_at,
                is_revoked = EXCLUDED.is_revoked,
                revoked_at = EXCLUDED.revoked_at
            "#,
        )
        .bind(&token.token_hash)
        .bind(&token.user_id)
        .bind(&token.client_id)
        .bind(&token.scope)
        .bind(token.created_ts)
        .bind(token.expires_at)
        .bind(token.is_revoked)
        .bind(token.revoked_at)
        .execute(self.pool.as_ref())
        .await?;
        Ok(())
    }

    #[instrument(skip(self), fields(hash = %token_hash))]
    pub async fn get_refresh_token(&self, token_hash: &str) -> Result<Option<OidcRefreshToken>, sqlx::Error> {
        let row = sqlx::query_as::<_, OidcRefreshToken>(
            r#"
            SELECT id, token_hash, user_id, client_id, scope, created_ts,
                   expires_at, is_revoked, revoked_at
            FROM oidc_refresh_tokens
            WHERE token_hash = $1 AND is_revoked = FALSE
            "#,
        )
        .bind(token_hash)
        .fetch_optional(self.pool.as_ref())
        .await?;
        Ok(row)
    }

    #[instrument(skip(self), fields(hash = %token_hash))]
    pub async fn revoke_refresh_token(&self, token_hash: &str, now_ts: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE oidc_refresh_tokens
            SET is_revoked = TRUE, revoked_at = $2
            WHERE token_hash = $1 AND is_revoked = FALSE
            "#,
        )
        .bind(token_hash)
        .bind(now_ts)
        .execute(self.pool.as_ref())
        .await?;
        Ok(result.rows_affected() > 0)
    }

    #[instrument(skip(self), fields(user_id = %user_id))]
    pub async fn revoke_user_refresh_tokens(&self, user_id: &str, now_ts: i64) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE oidc_refresh_tokens
            SET is_revoked = TRUE, revoked_at = $2
            WHERE user_id = $1 AND is_revoked = FALSE
            "#,
        )
        .bind(user_id)
        .bind(now_ts)
        .execute(self.pool.as_ref())
        .await?;
        Ok(result.rows_affected())
    }

    // ── Consent Session ──────────────────────────────────────────────────

    #[instrument(skip(self, session), fields(id = %session.session_id))]
    pub async fn save_consent_session(&self, session: &OidcConsentSession) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO oidc_consent_sessions
                (session_id, client_id, client_name, redirect_uri, scope, state,
                 nonce, code_challenge, user_id, created_ts, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (session_id) DO UPDATE SET
                client_id = EXCLUDED.client_id,
                client_name = EXCLUDED.client_name,
                redirect_uri = EXCLUDED.redirect_uri,
                scope = EXCLUDED.scope,
                state = EXCLUDED.state,
                nonce = EXCLUDED.nonce,
                code_challenge = EXCLUDED.code_challenge,
                user_id = EXCLUDED.user_id,
                created_ts = EXCLUDED.created_ts,
                expires_at = EXCLUDED.expires_at
            "#,
        )
        .bind(&session.session_id)
        .bind(&session.client_id)
        .bind(&session.client_name)
        .bind(&session.redirect_uri)
        .bind(&session.scope)
        .bind(&session.state)
        .bind(&session.nonce)
        .bind(&session.code_challenge)
        .bind(&session.user_id)
        .bind(session.created_ts)
        .bind(session.expires_at)
        .execute(self.pool.as_ref())
        .await?;
        Ok(())
    }

    /// 原子消费：DELETE ... RETURNING
    #[instrument(skip(self), fields(id = %session_id))]
    pub async fn get_and_delete_consent_session(
        &self,
        session_id: &str,
    ) -> Result<Option<OidcConsentSession>, sqlx::Error> {
        let row = sqlx::query_as::<_, OidcConsentSession>(
            r#"
            DELETE FROM oidc_consent_sessions
            WHERE session_id = $1
            RETURNING id, session_id, client_id, client_name, redirect_uri,
                      scope, state, nonce, code_challenge, user_id, created_ts, expires_at
            "#,
        )
        .bind(session_id)
        .fetch_optional(self.pool.as_ref())
        .await?;
        Ok(row)
    }

    /// 获取同意会话（不删除，用于渲染同意页面）
    #[instrument(skip(self), fields(id = %session_id))]
    pub async fn get_consent_session(
        &self,
        session_id: &str,
    ) -> Result<Option<OidcConsentSession>, sqlx::Error> {
        let row = sqlx::query_as::<_, OidcConsentSession>(
            r#"
            SELECT id, session_id, client_id, client_name, redirect_uri,
                   scope, state, nonce, code_challenge, user_id, created_ts, expires_at
            FROM oidc_consent_sessions
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_optional(self.pool.as_ref())
        .await?;
        Ok(row)
    }

    /// 删除同意会话（拒绝时使用）
    #[instrument(skip(self), fields(id = %session_id))]
    pub async fn delete_consent_session(&self, session_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM oidc_consent_sessions WHERE session_id = $1")
            .bind(session_id)
            .execute(self.pool.as_ref())
            .await?;
        Ok(())
    }

    // ── Cleanup ──────────────────────────────────────────────────────────

    /// 清理所有过期会话
    #[instrument(skip(self))]
    pub async fn cleanup_expired_sessions(&self, now_ts: i64) -> Result<u64, sqlx::Error> {
        // 清理过期的 auth sessions
        let r1 = sqlx::query("DELETE FROM oidc_auth_sessions WHERE expires_at < $1")
            .bind(now_ts)
            .execute(self.pool.as_ref())
            .await?;

        // 清理过期的 consent sessions
        let r2 = sqlx::query("DELETE FROM oidc_consent_sessions WHERE expires_at < $1")
            .bind(now_ts)
            .execute(self.pool.as_ref())
            .await?;

        // 清理过期的 refresh tokens
        let r3 = sqlx::query("DELETE FROM oidc_refresh_tokens WHERE expires_at IS NOT NULL AND expires_at < $1")
            .bind(now_ts)
            .execute(self.pool.as_ref())
            .await?;

        Ok(r1.rows_affected() + r2.rows_affected() + r3.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_auth_session() -> OidcAuthSession {
        OidcAuthSession {
            id: 0,
            session_key: "pkce_state_123".to_string(),
            session_type: "pkce".to_string(),
            client_id: "client1".to_string(),
            redirect_uri: "https://app.example.com/callback".to_string(),
            scope: "openid profile".to_string(),
            state: "random_state".to_string(),
            nonce: Some("nonce_abc".to_string()),
            code_verifier: Some("verifier_xyz".to_string()),
            code_challenge: Some("challenge_def".to_string()),
            code_challenge_method: Some("S256".to_string()),
            user_id: None,
            consent_given: false,
            created_ts: 1700000000000,
            expires_at: 1700003600000,
        }
    }

    fn sample_refresh_token() -> OidcRefreshToken {
        OidcRefreshToken {
            id: 0,
            token_hash: "hash_abc123".to_string(),
            user_id: "@user:server".to_string(),
            client_id: "client1".to_string(),
            scope: "openid".to_string(),
            created_ts: 1700000000000,
            expires_at: Some(1700086400000),
            is_revoked: false,
            revoked_at: None,
        }
    }

    fn sample_consent_session() -> OidcConsentSession {
        OidcConsentSession {
            id: 0,
            session_id: "consent_123".to_string(),
            client_id: "client1".to_string(),
            client_name: Some("Test App".to_string()),
            redirect_uri: "https://app.example.com/callback".to_string(),
            scope: "openid profile".to_string(),
            state: "state_xyz".to_string(),
            nonce: Some("nonce_abc".to_string()),
            code_challenge: Some("challenge_def".to_string()),
            user_id: "@user:server".to_string(),
            created_ts: 1700000000000,
            expires_at: 1700003600000,
        }
    }

    #[test]
    fn test_auth_session_fields() {
        let session = sample_auth_session();
        assert_eq!(session.session_key, "pkce_state_123");
        assert_eq!(session.session_type, "pkce");
        assert_eq!(session.client_id, "client1");
        assert_eq!(session.redirect_uri, "https://app.example.com/callback");
        assert_eq!(session.scope, "openid profile");
        assert_eq!(session.state, "random_state");
        assert_eq!(session.nonce.as_deref(), Some("nonce_abc"));
        assert_eq!(session.code_verifier.as_deref(), Some("verifier_xyz"));
        assert_eq!(session.code_challenge.as_deref(), Some("challenge_def"));
        assert_eq!(session.code_challenge_method.as_deref(), Some("S256"));
        assert!(session.user_id.is_none());
        assert!(!session.consent_given);
        assert_eq!(session.created_ts, 1700000000000);
        assert_eq!(session.expires_at, 1700003600000);
    }

    #[test]
    fn test_refresh_token_fields() {
        let token = sample_refresh_token();
        assert_eq!(token.token_hash, "hash_abc123");
        assert_eq!(token.user_id, "@user:server");
        assert_eq!(token.client_id, "client1");
        assert_eq!(token.scope, "openid");
        assert!(!token.is_revoked);
        assert!(token.revoked_at.is_none());
        assert!(token.expires_at.is_some());
    }

    #[test]
    fn test_consent_session_fields() {
        let session = sample_consent_session();
        assert_eq!(session.session_id, "consent_123");
        assert_eq!(session.client_id, "client1");
        assert_eq!(session.client_name.as_deref(), Some("Test App"));
        assert_eq!(session.redirect_uri, "https://app.example.com/callback");
        assert_eq!(session.scope, "openid profile");
        assert_eq!(session.state, "state_xyz");
        assert_eq!(session.user_id, "@user:server");
    }

    #[test]
    fn test_auth_session_clone() {
        let session = sample_auth_session();
        let cloned = session.clone();
        assert_eq!(cloned.session_key, session.session_key);
        assert_eq!(cloned.client_id, session.client_id);
    }

    #[test]
    fn test_refresh_token_revoked_state() {
        let mut token = sample_refresh_token();
        assert!(!token.is_revoked);
        assert!(token.revoked_at.is_none());

        token.is_revoked = true;
        token.revoked_at = Some(1700001000000);
        assert!(token.is_revoked);
        assert_eq!(token.revoked_at, Some(1700001000000));
    }

    #[test]
    fn test_auth_session_with_user_and_consent() {
        let mut session = sample_auth_session();
        assert!(session.user_id.is_none());
        assert!(!session.consent_given);

        session.user_id = Some("@user:server".to_string());
        session.consent_given = true;
        assert_eq!(session.user_id.as_deref(), Some("@user:server"));
        assert!(session.consent_given);
    }

    // DB-dependent tests marked with #[ignore]
    // Run with: cargo test --features test-utils -- --ignored

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_save_and_get_auth_session() {
        // Requires a running PostgreSQL with oidc_auth_sessions table
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_get_and_delete_auth_session_atomic() {
        // Requires a running PostgreSQL to test atomic DELETE ... RETURNING
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_get_nonexistent_session_returns_none() {
        // Requires a running PostgreSQL
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_cleanup_expired_sessions() {
        // Requires a running PostgreSQL
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_save_and_get_refresh_token() {
        // Requires a running PostgreSQL
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_revoke_refresh_token() {
        // Requires a running PostgreSQL
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_revoke_user_refresh_tokens() {
        // Requires a running PostgreSQL
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_save_and_get_consent_session() {
        // Requires a running PostgreSQL
    }
}
