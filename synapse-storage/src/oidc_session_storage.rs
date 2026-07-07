//! OIDC 会话持久化存储层
//!
//! 将内存 HashMap 替换为 PostgreSQL 数据库存储，支持：
//! - PKCE state 会话（外部 OIDC）
//! - 授权码会话（内置 OIDC）
//! - Refresh Token（内置 OIDC）
//! - 同意会话（MSC3861）

use async_trait::async_trait;
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

/// Trait abstraction over [`OidcSessionStorage`] for testability.
#[async_trait]
pub trait OidcSessionStoreApi: Send + Sync {
    async fn save_auth_session(&self, session: &OidcAuthSession) -> Result<(), sqlx::Error>;
    async fn get_and_delete_auth_session(&self, session_key: &str) -> Result<Option<OidcAuthSession>, sqlx::Error>;
    async fn save_refresh_token(&self, token: &OidcRefreshToken) -> Result<(), sqlx::Error>;
    async fn get_refresh_token(&self, token_hash: &str) -> Result<Option<OidcRefreshToken>, sqlx::Error>;
    async fn revoke_refresh_token(&self, token_hash: &str, now_ts: i64) -> Result<bool, sqlx::Error>;
    async fn revoke_user_refresh_tokens(&self, user_id: &str, now_ts: i64) -> Result<u64, sqlx::Error>;
    async fn save_consent_session(&self, session: &OidcConsentSession) -> Result<(), sqlx::Error>;
    async fn get_and_delete_consent_session(&self, session_id: &str)
        -> Result<Option<OidcConsentSession>, sqlx::Error>;
    async fn get_consent_session(&self, session_id: &str) -> Result<Option<OidcConsentSession>, sqlx::Error>;
    async fn delete_consent_session(&self, session_id: &str) -> Result<(), sqlx::Error>;
    async fn cleanup_expired_sessions(&self, now_ts: i64) -> Result<u64, sqlx::Error>;
}

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
    pub async fn get_and_delete_auth_session(&self, session_key: &str) -> Result<Option<OidcAuthSession>, sqlx::Error> {
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
    pub async fn get_consent_session(&self, session_id: &str) -> Result<Option<OidcConsentSession>, sqlx::Error> {
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

#[async_trait]
impl OidcSessionStoreApi for OidcSessionStorage {
    async fn save_auth_session(&self, session: &OidcAuthSession) -> Result<(), sqlx::Error> {
        self.save_auth_session(session).await
    }
    async fn get_and_delete_auth_session(&self, session_key: &str) -> Result<Option<OidcAuthSession>, sqlx::Error> {
        self.get_and_delete_auth_session(session_key).await
    }
    async fn save_refresh_token(&self, token: &OidcRefreshToken) -> Result<(), sqlx::Error> {
        self.save_refresh_token(token).await
    }
    async fn get_refresh_token(&self, token_hash: &str) -> Result<Option<OidcRefreshToken>, sqlx::Error> {
        self.get_refresh_token(token_hash).await
    }
    async fn revoke_refresh_token(&self, token_hash: &str, now_ts: i64) -> Result<bool, sqlx::Error> {
        self.revoke_refresh_token(token_hash, now_ts).await
    }
    async fn revoke_user_refresh_tokens(&self, user_id: &str, now_ts: i64) -> Result<u64, sqlx::Error> {
        self.revoke_user_refresh_tokens(user_id, now_ts).await
    }
    async fn save_consent_session(&self, session: &OidcConsentSession) -> Result<(), sqlx::Error> {
        self.save_consent_session(session).await
    }
    async fn get_and_delete_consent_session(
        &self,
        session_id: &str,
    ) -> Result<Option<OidcConsentSession>, sqlx::Error> {
        self.get_and_delete_consent_session(session_id).await
    }
    async fn get_consent_session(&self, session_id: &str) -> Result<Option<OidcConsentSession>, sqlx::Error> {
        self.get_consent_session(session_id).await
    }
    async fn delete_consent_session(&self, session_id: &str) -> Result<(), sqlx::Error> {
        self.delete_consent_session(session_id).await
    }
    async fn cleanup_expired_sessions(&self, now_ts: i64) -> Result<u64, sqlx::Error> {
        self.cleanup_expired_sessions(now_ts).await
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

    // ===== Database-dependent tests =====
    //
    // These tests use `prepare_empty_isolated_test_pool()` to get an isolated
    // PostgreSQL schema. The schema starts empty, so each test creates the
    // `oidc_auth_sessions`, `oidc_refresh_tokens`, and `oidc_consent_sessions`
    // tables matching the unified migration before running.

    use sqlx::PgPool;
    use std::sync::Arc;

    async fn get_test_pool() -> Option<Arc<PgPool>> {
        match crate::test_utils::prepare_empty_isolated_test_pool().await {
            Ok(pool) => Some(pool),
            Err(error) => {
                tracing::warn!("Skipping oidc_session DB test because test database is unavailable: {error}");
                None
            }
        }
    }

    async fn setup_oidc_session_db(pool: &Arc<PgPool>) {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS oidc_auth_sessions (
                id BIGSERIAL PRIMARY KEY,
                session_key TEXT NOT NULL UNIQUE,
                session_type TEXT NOT NULL,
                client_id TEXT NOT NULL DEFAULT '',
                redirect_uri TEXT NOT NULL DEFAULT '',
                scope TEXT NOT NULL DEFAULT '',
                state TEXT NOT NULL DEFAULT '',
                nonce TEXT,
                code_verifier TEXT,
                code_challenge TEXT,
                code_challenge_method TEXT,
                user_id TEXT,
                consent_given BOOLEAN NOT NULL DEFAULT FALSE,
                created_ts BIGINT NOT NULL,
                expires_at BIGINT NOT NULL
            )
            "#,
        )
        .execute(&**pool)
        .await
        .expect("Failed to create oidc_auth_sessions table");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS oidc_refresh_tokens (
                id BIGSERIAL PRIMARY KEY,
                token_hash TEXT NOT NULL UNIQUE,
                user_id TEXT NOT NULL,
                client_id TEXT NOT NULL,
                scope TEXT NOT NULL DEFAULT '',
                created_ts BIGINT NOT NULL,
                expires_at BIGINT,
                is_revoked BOOLEAN NOT NULL DEFAULT FALSE,
                revoked_at BIGINT
            )
            "#,
        )
        .execute(&**pool)
        .await
        .expect("Failed to create oidc_refresh_tokens table");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS oidc_consent_sessions (
                id BIGSERIAL PRIMARY KEY,
                session_id TEXT NOT NULL UNIQUE,
                client_id TEXT NOT NULL,
                client_name TEXT,
                redirect_uri TEXT NOT NULL,
                scope TEXT NOT NULL DEFAULT '',
                state TEXT NOT NULL DEFAULT '',
                nonce TEXT,
                code_challenge TEXT,
                user_id TEXT NOT NULL,
                created_ts BIGINT NOT NULL,
                expires_at BIGINT NOT NULL
            )
            "#,
        )
        .execute(&**pool)
        .await
        .expect("Failed to create oidc_consent_sessions table");
    }

    fn make_auth_session(session_key: String, created_ts: i64, expires_at: i64) -> OidcAuthSession {
        OidcAuthSession {
            id: 0,
            session_key,
            session_type: "pkce".to_string(),
            client_id: "client1".to_string(),
            redirect_uri: "https://app.example.com/callback".to_string(),
            scope: "openid profile".to_string(),
            state: "state".to_string(),
            nonce: Some("nonce_abc".to_string()),
            code_verifier: Some("verifier_xyz".to_string()),
            code_challenge: Some("challenge_def".to_string()),
            code_challenge_method: Some("S256".to_string()),
            user_id: None,
            consent_given: false,
            created_ts,
            expires_at,
        }
    }

    fn make_refresh_token(token_hash: String, created_ts: i64, expires_at: Option<i64>) -> OidcRefreshToken {
        OidcRefreshToken {
            id: 0,
            token_hash,
            user_id: "@user:server".to_string(),
            client_id: "client1".to_string(),
            scope: "openid".to_string(),
            created_ts,
            expires_at,
            is_revoked: false,
            revoked_at: None,
        }
    }

    fn make_consent_session(session_id: String, created_ts: i64, expires_at: i64) -> OidcConsentSession {
        OidcConsentSession {
            id: 0,
            session_id,
            client_id: "client1".to_string(),
            client_name: Some("Test App".to_string()),
            redirect_uri: "https://app.example.com/callback".to_string(),
            scope: "openid profile".to_string(),
            state: "state".to_string(),
            nonce: Some("nonce_abc".to_string()),
            code_challenge: Some("challenge_def".to_string()),
            user_id: "@user:server".to_string(),
            created_ts,
            expires_at,
        }
    }

    #[tokio::test]
    async fn test_save_and_get_auth_session() {
        let pool = match get_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_oidc_session_db(&pool).await;

        let storage = OidcSessionStorage::new(&pool);
        let session_key = format!("pkce_state_{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().timestamp_millis();
        let expires_at = now + 3_600_000;
        let session = make_auth_session(session_key.clone(), now, expires_at);

        storage.save_auth_session(&session).await.expect("save_auth_session should succeed");

        // OidcSessionStorage only exposes the atomic get_and_delete retrieval for auth
        // sessions; verify the saved row is returned with all fields intact.
        let fetched = storage
            .get_and_delete_auth_session(&session_key)
            .await
            .expect("get_and_delete_auth_session should succeed");
        assert!(fetched.is_some(), "saved session should be retrievable");
        let fetched = fetched.unwrap();
        assert_eq!(fetched.session_key, session_key);
        assert_eq!(fetched.session_type, "pkce");
        assert_eq!(fetched.client_id, "client1");
        assert_eq!(fetched.redirect_uri, "https://app.example.com/callback");
        assert_eq!(fetched.scope, "openid profile");
        assert_eq!(fetched.state, "state");
        assert_eq!(fetched.nonce.as_deref(), Some("nonce_abc"));
        assert_eq!(fetched.code_verifier.as_deref(), Some("verifier_xyz"));
        assert_eq!(fetched.code_challenge.as_deref(), Some("challenge_def"));
        assert_eq!(fetched.code_challenge_method.as_deref(), Some("S256"));
        assert!(fetched.user_id.is_none());
        assert!(!fetched.consent_given);
        assert_eq!(fetched.created_ts, now);
        assert_eq!(fetched.expires_at, expires_at);
        assert!(fetched.id > 0, "id should be populated by DB");
    }

    #[tokio::test]
    async fn test_get_and_delete_auth_session_atomic() {
        let pool = match get_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_oidc_session_db(&pool).await;

        let storage = OidcSessionStorage::new(&pool);
        let session_key = format!("pkce_state_{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().timestamp_millis();
        let session = make_auth_session(session_key.clone(), now, now + 3_600_000);

        storage.save_auth_session(&session).await.expect("save_auth_session should succeed");

        // First call should return the session.
        let first =
            storage.get_and_delete_auth_session(&session_key).await.expect("first get_and_delete should succeed");
        assert!(first.is_some(), "first call should return the session");
        assert_eq!(first.unwrap().session_key, session_key);

        // Second call should return None — the row was atomically deleted.
        let second =
            storage.get_and_delete_auth_session(&session_key).await.expect("second get_and_delete should succeed");
        assert!(second.is_none(), "second call should return None after atomic deletion");
    }

    #[tokio::test]
    async fn test_get_nonexistent_session_returns_none() {
        let pool = match get_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_oidc_session_db(&pool).await;

        let storage = OidcSessionStorage::new(&pool);
        let missing = storage
            .get_and_delete_auth_session("nonexistent_session_key")
            .await
            .expect("get_and_delete_auth_session should succeed");
        assert!(missing.is_none(), "non-existent session should return None");
    }

    #[tokio::test]
    async fn test_cleanup_expired_sessions() {
        let pool = match get_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_oidc_session_db(&pool).await;

        let storage = OidcSessionStorage::new(&pool);
        let now = chrono::Utc::now().timestamp_millis();
        let past_ts = now - 3_600_000;
        let future_ts = now + 3_600_000;

        // Expired + active auth sessions.
        storage
            .save_auth_session(&make_auth_session(
                format!("expired_auth_{}", uuid::Uuid::new_v4()),
                past_ts - 3_600_000,
                past_ts,
            ))
            .await
            .expect("save_auth_session should succeed");
        storage
            .save_auth_session(&make_auth_session(format!("active_auth_{}", uuid::Uuid::new_v4()), now, future_ts))
            .await
            .expect("save_auth_session should succeed");

        // Expired + active consent sessions.
        storage
            .save_consent_session(&make_consent_session(
                format!("expired_consent_{}", uuid::Uuid::new_v4()),
                past_ts - 3_600_000,
                past_ts,
            ))
            .await
            .expect("save_consent_session should succeed");
        storage
            .save_consent_session(&make_consent_session(
                format!("active_consent_{}", uuid::Uuid::new_v4()),
                now,
                future_ts,
            ))
            .await
            .expect("save_consent_session should succeed");

        // Expired refresh token (past expires_at).
        storage
            .save_refresh_token(&make_refresh_token(
                format!("expired_token_{}", uuid::Uuid::new_v4()),
                past_ts - 3_600_000,
                Some(past_ts),
            ))
            .await
            .expect("save_refresh_token should succeed");
        // Active refresh token (no expiry → not deleted).
        storage
            .save_refresh_token(&make_refresh_token(format!("no_expiry_token_{}", uuid::Uuid::new_v4()), now, None))
            .await
            .expect("save_refresh_token should succeed");
        // Active refresh token (future expiry → not deleted).
        storage
            .save_refresh_token(&make_refresh_token(
                format!("future_token_{}", uuid::Uuid::new_v4()),
                now,
                Some(future_ts),
            ))
            .await
            .expect("save_refresh_token should succeed");

        // cleanup_expired_sessions(now) should delete:
        //   1 expired auth + 1 expired consent + 1 expired refresh token = 3
        let deleted = storage.cleanup_expired_sessions(now).await.expect("cleanup_expired_sessions should succeed");
        assert_eq!(deleted, 3, "should delete 1 expired auth session + 1 expired consent + 1 expired refresh token");

        // Verify remaining rows via direct SQL counts.
        let auth_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM oidc_auth_sessions")
            .fetch_one(pool.as_ref())
            .await
            .expect("auth count should succeed");
        assert_eq!(auth_count, 1, "1 active auth session should remain");

        let consent_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM oidc_consent_sessions")
            .fetch_one(pool.as_ref())
            .await
            .expect("consent count should succeed");
        assert_eq!(consent_count, 1, "1 active consent session should remain");

        let token_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM oidc_refresh_tokens")
            .fetch_one(pool.as_ref())
            .await
            .expect("token count should succeed");
        assert_eq!(token_count, 2, "2 active refresh tokens should remain");

        // Second cleanup should delete nothing.
        let deleted_again = storage.cleanup_expired_sessions(now).await.expect("cleanup should succeed");
        assert_eq!(deleted_again, 0, "no expired sessions on second cleanup");
    }

    #[tokio::test]
    async fn test_save_and_get_refresh_token() {
        let pool = match get_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_oidc_session_db(&pool).await;

        let storage = OidcSessionStorage::new(&pool);
        let token_hash = format!("hash_{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().timestamp_millis();
        let future_ts = now + 3_600_000;
        let token = make_refresh_token(token_hash.clone(), now, Some(future_ts));

        storage.save_refresh_token(&token).await.expect("save_refresh_token should succeed");

        let fetched = storage.get_refresh_token(&token_hash).await.expect("get_refresh_token should succeed");
        assert!(fetched.is_some(), "token should exist and not be revoked");
        let fetched = fetched.unwrap();
        assert_eq!(fetched.token_hash, token_hash);
        assert_eq!(fetched.user_id, "@user:server");
        assert_eq!(fetched.client_id, "client1");
        assert_eq!(fetched.scope, "openid");
        assert_eq!(fetched.created_ts, now);
        assert_eq!(fetched.expires_at, Some(future_ts));
        assert!(!fetched.is_revoked);
        assert!(fetched.revoked_at.is_none());
        assert!(fetched.id > 0, "id should be populated by DB");

        // Non-existent token returns None.
        let missing = storage.get_refresh_token("nonexistent_hash").await.expect("get_refresh_token should succeed");
        assert!(missing.is_none(), "non-existent token should return None");
    }

    #[tokio::test]
    async fn test_revoke_refresh_token() {
        let pool = match get_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_oidc_session_db(&pool).await;

        let storage = OidcSessionStorage::new(&pool);
        let token_hash = format!("hash_{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().timestamp_millis();
        let token = make_refresh_token(token_hash.clone(), now, Some(now + 3_600_000));

        storage.save_refresh_token(&token).await.expect("save_refresh_token should succeed");

        // Verify token exists before revocation.
        let fetched = storage.get_refresh_token(&token_hash).await.expect("get_refresh_token should succeed");
        assert!(fetched.is_some(), "token should exist and not be revoked");

        // Revoke the token.
        let revoked =
            storage.revoke_refresh_token(&token_hash, now).await.expect("revoke_refresh_token should succeed");
        assert!(revoked, "revoke should return true for active token");

        // get_refresh_token returns None for revoked tokens (query has AND is_revoked = FALSE).
        let after_revoke = storage.get_refresh_token(&token_hash).await.expect("get_refresh_token should succeed");
        assert!(after_revoke.is_none(), "revoked token should not be returned by get_refresh_token");

        // Revoke again should return false (already revoked).
        let revoke_again = storage.revoke_refresh_token(&token_hash, now).await.expect("revoke should succeed");
        assert!(!revoke_again, "revoke should return false for already-revoked token");

        // Revoke non-existent token should return false.
        let revoke_missing =
            storage.revoke_refresh_token("nonexistent_hash", now).await.expect("revoke should succeed");
        assert!(!revoke_missing, "revoke should return false for non-existent token");
    }

    #[tokio::test]
    async fn test_revoke_user_refresh_tokens() {
        let pool = match get_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_oidc_session_db(&pool).await;

        let storage = OidcSessionStorage::new(&pool);
        let now = chrono::Utc::now().timestamp_millis();
        let future_ts = now + 3_600_000;
        let user_id = format!("@user_{}:server", uuid::Uuid::new_v4());

        // Create 3 refresh tokens for the user.
        for i in 0..3u8 {
            let token = OidcRefreshToken {
                id: 0,
                token_hash: format!("hash_{}_{}", uuid::Uuid::new_v4(), i),
                user_id: user_id.clone(),
                client_id: "client1".to_string(),
                scope: "openid".to_string(),
                created_ts: now,
                expires_at: Some(future_ts),
                is_revoked: false,
                revoked_at: None,
            };
            storage.save_refresh_token(&token).await.expect("save_refresh_token should succeed");
        }

        // Revoke all tokens for the user.
        let revoked_count =
            storage.revoke_user_refresh_tokens(&user_id, now).await.expect("revoke_user_refresh_tokens should succeed");
        assert_eq!(revoked_count, 3, "should revoke 3 tokens");

        // Second call should revoke 0 (already revoked).
        let revoked_again = storage.revoke_user_refresh_tokens(&user_id, now).await.expect("revoke should succeed");
        assert_eq!(revoked_again, 0, "no tokens to revoke on second call");

        // Verify via SQL: 0 active, 3 revoked.
        let active_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM oidc_refresh_tokens WHERE user_id = $1 AND is_revoked = FALSE")
                .bind(&user_id)
                .fetch_one(pool.as_ref())
                .await
                .expect("active count should succeed");
        assert_eq!(active_count, 0, "no active tokens should remain");

        let revoked_db_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM oidc_refresh_tokens WHERE user_id = $1 AND is_revoked = TRUE")
                .bind(&user_id)
                .fetch_one(pool.as_ref())
                .await
                .expect("revoked count should succeed");
        assert_eq!(revoked_db_count, 3, "3 tokens should be revoked");
    }

    #[tokio::test]
    async fn test_save_and_get_consent_session() {
        let pool = match get_test_pool().await {
            Some(p) => p,
            None => return,
        };
        setup_oidc_session_db(&pool).await;

        let storage = OidcSessionStorage::new(&pool);
        let session_id = format!("consent_{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().timestamp_millis();
        let future_ts = now + 3_600_000;
        let session = make_consent_session(session_id.clone(), now, future_ts);

        storage.save_consent_session(&session).await.expect("save_consent_session should succeed");

        // get_consent_session returns the session without deleting it.
        let fetched = storage.get_consent_session(&session_id).await.expect("get_consent_session should succeed");
        assert!(fetched.is_some(), "consent session should be retrievable");
        let fetched = fetched.unwrap();
        assert_eq!(fetched.session_id, session_id);
        assert_eq!(fetched.client_id, "client1");
        assert_eq!(fetched.client_name.as_deref(), Some("Test App"));
        assert_eq!(fetched.redirect_uri, "https://app.example.com/callback");
        assert_eq!(fetched.scope, "openid profile");
        assert_eq!(fetched.state, "state");
        assert_eq!(fetched.nonce.as_deref(), Some("nonce_abc"));
        assert_eq!(fetched.code_challenge.as_deref(), Some("challenge_def"));
        assert_eq!(fetched.user_id, "@user:server");
        assert_eq!(fetched.created_ts, now);
        assert_eq!(fetched.expires_at, future_ts);
        assert!(fetched.id > 0, "id should be populated by DB");

        // get_consent_session does NOT delete — second call should still return the session.
        let fetched_again = storage.get_consent_session(&session_id).await.expect("get_consent_session should succeed");
        assert!(fetched_again.is_some(), "session should still exist after get (no delete)");

        // Non-existent session returns None.
        let missing =
            storage.get_consent_session("nonexistent_session").await.expect("get_consent_session should succeed");
        assert!(missing.is_none(), "non-existent session should return None");
    }
}
