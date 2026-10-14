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

/// Rejection returned when the submitted token does not match its session.
///
/// Shared with the unknown-session branch of
/// [`EmailVerificationStorage::validate_and_consume_token`], which has to be
/// byte-identical to it when unknown sessions are inhibited (upstream
/// `request_token_inhibit_3pid_errors`, changelog `20243`). Kept as a constant
/// so the two branches cannot drift apart into an enumeration oracle.
pub const INVALID_TOKEN_MESSAGE: &str = "Invalid verification token";

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

        // 单列 `RETURNING id` ⇒ `query_scalar!` + `fetch_one`（原元组包装结构体 `TokenIdRow`
        // 只服务这一处，随之删除）。`user_id` / `session_data` 是 `Option`，按值直传即可。
        let id = sqlx::query_scalar!(
            r#"
            INSERT INTO email_verification_tokens (email, token, expires_at, created_ts, is_used, user_id, session_data)
            VALUES ($1, $2, $3, $4, FALSE, $5, $6)
            RETURNING id
            "#,
            email,
            token,
            expires_at,
            now,
            user_id,
            session_data
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(id)
    }

    /// See [`mark_token_used`].
    pub async fn mark_token_used(&self, token_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE email_verification_tokens SET is_used = TRUE WHERE id = $1
            "#,
            token_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// Shared validation for submitToken: checks existence, token match, usage,
    /// expiration and client_secret match against session_data. Marks the token
    /// as used on success.
    ///
    /// `inhibit_unknown_session` mirrors upstream's
    /// `RegistrationStore._ignore_unknown_session_error`, driven by
    /// `request_token_inhibit_3pid_errors`. When set, an unknown `token_id` is
    /// rejected with the **same** error as a wrong token, so a caller cannot
    /// probe which session ids exist. That matters here: `sid` is the token row
    /// id (a small sequential integer), so the oracle would otherwise be trivial
    /// to walk.
    ///
    /// # Why the token is compared **first** (order is load-bearing)
    ///
    /// Only the unknown-`sid` branch used to be masked, which left the session
    /// *lifecycle* as an oracle: a caller who probed an existing `sid` with a
    /// garbage token learned `has already been used` or `has expired` before
    /// the token comparison ever ran, and those two messages differ from the
    /// unknown-session one. Comparing the token first makes every rejection
    /// below reachable **only by a caller that already knows the token** — an
    /// unknown `sid`, a used session and an expired one all answer with the
    /// identical [`INVALID_TOKEN_MESSAGE`] to someone guessing.
    ///
    /// The lookup itself is an unconditional `SELECT` for both hits and misses,
    /// so the reorder does not introduce a query-count difference either.
    ///
    /// The reorder is **unconditional**; `inhibit_unknown_session` only decides
    /// whether the *unknown-`sid`* message is masked. An operator who sets it to
    /// `false` gets the distinct `not found` message back for a session that
    /// does not exist, but still has to present the token to see `used` /
    /// `expired` — which is all a legitimate client ever does.
    pub async fn validate_and_consume_token(
        &self,
        token_id: i64,
        submitted_token: &str,
        client_secret: &str,
        inhibit_unknown_session: bool,
    ) -> Result<EmailVerificationToken, ApiError> {
        let verification_token = self
            .get_verification_token_by_id(token_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get verification token", e))?;

        let verification_token = match verification_token {
            Some(row) => row,
            // Indistinguishable from the wrong-token rejection below: same
            // status, same errcode, same message.
            None if inhibit_unknown_session => return Err(ApiError::bad_request(INVALID_TOKEN_MESSAGE.to_string())),
            None => return Err(ApiError::bad_request("Invalid session ID or session not found".to_string())),
        };

        // First, before any state check — see the ordering note above.
        if verification_token.token != submitted_token {
            return Err(ApiError::bad_request(INVALID_TOKEN_MESSAGE.to_string()));
        }

        if verification_token.is_used {
            return Err(ApiError::bad_request("Verification token has already been used".to_string()));
        }

        let now = current_timestamp_millis();
        if verification_token.expires_at.is_none_or(|expires_at| expires_at < now) {
            return Err(ApiError::bad_request("Verification token has expired".to_string()));
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
        let token_record = sqlx::query_as!(
            EmailVerificationToken,
            r#"
            SELECT id, user_id, email, token, expires_at, created_ts, is_used, session_data
            FROM email_verification_tokens
            WHERE id = $1
            "#,
            token_id
        )
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
        let row = sqlx::query_as!(
            EmailVerificationToken,
            r#"
            DELETE FROM email_verification_tokens
            WHERE id = $1 AND is_used = TRUE AND expires_at > $2
            RETURNING id, user_id, email, token, expires_at, created_ts, is_used, session_data
            "#,
            token_id,
            now
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    /// See [`cleanup_expired_tokens`].
    pub async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error> {
        let now = current_timestamp_millis();
        let result = sqlx::query!(
            r#"
            DELETE FROM email_verification_tokens WHERE expires_at < $1
            "#,
            now
        )
        .execute(&*self.pool)
        .await?;
        Ok(result.rows_affected() as i64)
    }
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
        assert!(storage.validate_and_consume_token(with_session, "tok-session", "wrong", false).await.is_err());
        assert!(storage.validate_and_consume_token(with_session, "wrong-token", "s3cret", false).await.is_err());
        assert!(!storage.get_verification_token_by_id(with_session).await.unwrap().unwrap().is_used);

        let consumed = storage
            .validate_and_consume_token(with_session, "tok-session", "s3cret", false)
            .await
            .expect("valid token must be consumed");
        assert_eq!(consumed.id, with_session);
        assert!(storage.get_verification_token_by_id(with_session).await.unwrap().unwrap().is_used);
        // 已用过 ⇒ 再校验必须失败（不可重放）。
        assert!(storage.validate_and_consume_token(with_session, "tok-session", "s3cret", false).await.is_err());

        // 过期：`expires_in_seconds` 为负 ⇒ `expires_at` 已在过去。
        let expired =
            storage.create_verification_token("expired@example.com", "tok-expired", -10, None, None).await.unwrap();
        assert!(storage.validate_and_consume_token(expired, "tok-expired", "", false).await.is_err());

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

    /// 存储层半边：`request_token_inhibit_3pid_errors` 打开时，**未知会话 ID**
    /// 必须与"token 不符"被完全相同地拒绝；关掉时两者恢复可区分。
    ///
    /// 断言的是两条错误彼此相等，而不是某个字面量消息 —— 这样即使没有本用例，
    /// 也能保证两分支不会各自漂移到可区分。
    #[tokio::test]
    async fn unknown_session_is_indistinguishable_from_a_wrong_token_when_inhibiting() {
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
        let pool = isolated.pool();
        let storage = EmailVerificationStorage::new(&pool);

        let real = storage
            .create_verification_token(
                "oracle@example.com",
                "tok-real",
                3600,
                None,
                Some(serde_json::json!({"client_secret": "s3cret", "purpose": "register"})),
            )
            .await
            .expect("create");

        // 一个真实存在但与 token 不符的会话，与一个根本不存在的 sid。
        let missing_id = real + 1_000_000;
        assert!(storage.get_verification_token_by_id(missing_id).await.unwrap().is_none());

        let hidden = storage
            .validate_and_consume_token(missing_id, "whatever", "s3cret", true)
            .await
            .expect_err("unknown session must be rejected");
        let wrong_token = storage
            .validate_and_consume_token(real, "not-the-token", "s3cret", true)
            .await
            .expect_err("wrong token must be rejected");

        // `ApiError` 的 `PartialEq` 比较 (kind, code, message) —— 正是客户端能观察到的三元组。
        assert_eq!(
            hidden, wrong_token,
            "with inhibition on, an unknown sid must be rejected exactly like a wrong token"
        );
        assert_eq!(hidden.message, INVALID_TOKEN_MESSAGE, "both branches share the constant");

        // 关掉抑制 ⇒ 未知会话重新可区分（operator 显式选择的行为）。
        let revealed = storage
            .validate_and_consume_token(missing_id, "whatever", "s3cret", false)
            .await
            .expect_err("unknown session must still be rejected");
        assert_eq!(revealed.message, "Invalid session ID or session not found");
        assert_ne!(revealed, hidden, "with inhibition off the unknown-session rejection is intentionally distinct");
    }

    /// 顺序守卫：`is_used` / 已过期**不得**在 token 比对之前被拒。
    ///
    /// 只屏蔽"未知 sid"会留下**生命周期**预言机：拿垃圾 token 去探一个既有
    /// `sid`，在 token 比对之前就能拿到"已用过"/"已过期"，而这两条消息与
    /// "未知会话"不同 —— 于是 `sid`（自增整数）仍可被逐个分类。
    ///
    /// 断言四种探测路径的错误**彼此相等**（而非某个字面量），这样即使将来
    /// 有人把校验顺序改回去，也会被这条用例抓住。
    #[tokio::test]
    async fn session_lifecycle_is_indistinguishable_from_an_unknown_sid() {
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
        let pool = isolated.pool();
        let storage = EmailVerificationStorage::new(&pool);

        let session_data = || serde_json::json!({"client_secret": "s3cret", "purpose": "register"});

        // 1) 一个已被正常消费的会话。
        let used_id = storage
            .create_verification_token("used@example.com", "tok-used", 3600, None, Some(session_data()))
            .await
            .expect("create used");
        storage
            .validate_and_consume_token(used_id, "tok-used", "s3cret", true)
            .await
            .expect("the real token must be consumable");
        assert!(storage.get_verification_token_by_id(used_id).await.unwrap().unwrap().is_used);

        // 2) 一个已过期的会话（expires_in 为负）。
        let expired_id = storage
            .create_verification_token("expired@example.com", "tok-expired", -10, None, Some(session_data()))
            .await
            .expect("create expired");

        // 3) 一个根本不存在的 sid。
        let unknown_id = expired_id + 1_000_000;
        assert!(storage.get_verification_token_by_id(unknown_id).await.unwrap().is_none());

        // 用**垃圾 token** 探测三者：必须给出完全相同的拒绝，才说明生命周期
        // 不再是可区分的信号。
        let unknown = storage
            .validate_and_consume_token(unknown_id, "guessed", "s3cret", true)
            .await
            .expect_err("unknown sid must be rejected");
        let used = storage
            .validate_and_consume_token(used_id, "guessed", "s3cret", true)
            .await
            .expect_err("used session must be rejected");
        let expired = storage
            .validate_and_consume_token(expired_id, "guessed", "s3cret", true)
            .await
            .expect_err("expired session must be rejected");

        assert_eq!(unknown, used, "a used session must not be distinguishable from an unknown sid");
        assert_eq!(unknown, expired, "an expired session must not be distinguishable from an unknown sid");
        assert_eq!(unknown.message, INVALID_TOKEN_MESSAGE);

        // 抑制关闭只影响"未知 sid"那一条：token 比对仍在最前，所以拿垃圾 token
        // 探测已用过的会话依旧得到 token 错误 —— 而只有持有 token 的调用方才会
        // 看到"已用过/已过期"。
        let revealed_used = storage
            .validate_and_consume_token(used_id, "guessed", "s3cret", false)
            .await
            .expect_err("used session must still be rejected");
        assert_eq!(revealed_used.message, INVALID_TOKEN_MESSAGE);
        let revealed_unknown = storage
            .validate_and_consume_token(unknown_id, "guessed", "s3cret", false)
            .await
            .expect_err("unknown sid must still be rejected");
        assert_eq!(revealed_unknown.message, "Invalid session ID or session not found");

        // 而正确的 token 仍然能看到精确状态（合法客户端的可调试性未被牺牲）。
        let honest_expired = storage
            .validate_and_consume_token(expired_id, "tok-expired", "s3cret", true)
            .await
            .expect_err("expired session must be rejected");
        assert_eq!(honest_expired.message, "Verification token has expired");
    }
}
