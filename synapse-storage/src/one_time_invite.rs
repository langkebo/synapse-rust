//! One-time invite tokens (B-9) — storage layer.
//!
//! The table stores only `token_hash` (SHA-256 of the shareable token), never the
//! plaintext. Consumption is a single conditional `UPDATE`, so two concurrent
//! claims of the same token can never both win: the second one sees
//! `claimed_by IS NOT NULL` and updates zero rows.

use serde_json::Value;
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

/// The data needed to materialise a room after a successful claim.
#[derive(Debug, Clone)]
pub struct ClaimedOneTimeInvite {
    /// MXID of the user who minted the invite.
    pub creator: String,
    /// `CreateRoomConfig` shape captured at mint time (serialised JSON).
    pub room_config: Value,
}

/// Outcome of [`OneTimeInviteStorage::claim`].
#[derive(Debug, Clone)]
pub enum ClaimOutcome {
    /// The token was valid, unexpired and unclaimed; it is now consumed.
    Claimed(ClaimedOneTimeInvite),
    /// No invite row exists for this token.
    NotFound,
    /// The invite exists but its `expires_at` has passed.
    Expired,
    /// The invite exists but was already consumed.
    AlreadyClaimed,
}

/// The `OneTimeInviteStorage` struct.
#[derive(Clone)]
pub struct OneTimeInviteStorage {
    pool: Arc<PgPool>,
}

impl OneTimeInviteStorage {
    /// See [`new`].
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Persist a new invite. Only the hash of the token is written.
    pub async fn create(
        &self,
        token_hash: &str,
        creator: &str,
        room_config: &Value,
        expires_at: i64,
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        sqlx::query!(
            r#"
            INSERT INTO one_time_invites (token_hash, creator, room_config, expires_at, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            token_hash,
            creator,
            room_config,
            expires_at,
            now,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// Atomically consume `token_hash` on behalf of `claimed_by`.
    ///
    /// The conditional `UPDATE ... WHERE claimed_by IS NULL` is the single-use
    /// gate: under concurrent claims exactly one statement observes zero rows
    /// before its write and therefore returns the claimed row.
    pub async fn claim(&self, token_hash: &str, claimed_by: &str) -> Result<ClaimOutcome, sqlx::Error> {
        let now = current_timestamp_millis();
        let claimed = sqlx::query!(
            r#"
            UPDATE one_time_invites
            SET claimed_by = $2, claimed_at = $3
            WHERE token_hash = $1 AND claimed_by IS NULL AND expires_at > $3
            RETURNING creator, room_config
            "#,
            token_hash,
            claimed_by,
            now,
        )
        .fetch_optional(&*self.pool)
        .await?;

        if let Some(row) = claimed {
            return Ok(ClaimOutcome::Claimed(ClaimedOneTimeInvite {
                creator: row.creator,
                room_config: row.room_config,
            }));
        }

        // The UPDATE touched nothing: classify why so the caller can return a
        // useful error instead of a blanket "invalid token".
        let existing = sqlx::query!(
            r#"
            SELECT claimed_by FROM one_time_invites WHERE token_hash = $1
            "#,
            token_hash,
        )
        .fetch_optional(&*self.pool)
        .await?;

        match existing {
            None => Ok(ClaimOutcome::NotFound),
            Some(row) if row.claimed_by.is_some() => Ok(ClaimOutcome::AlreadyClaimed),
            Some(_) => Ok(ClaimOutcome::Expired),
        }
    }

    /// Undo a claim (used when room creation fails after the token was consumed).
    ///
    /// Without this a transient failure while creating the room would burn a
    /// still-valid invite; the update is scoped to `claimed_by` so it cannot
    /// release somebody else's claim.
    pub async fn release_claim(&self, token_hash: &str, claimed_by: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE one_time_invites
            SET claimed_by = NULL, claimed_at = NULL
            WHERE token_hash = $1 AND claimed_by = $2
            "#,
            token_hash,
            claimed_by,
        )
        .execute(&*self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
// 测试断言依赖显式 panic（expect/assert!）标记失败位置，测试代码里 panic 是预期语义。
#[allow(clippy::panic)]
mod db_tests {
    use super::*;
    use serde_json::json;

    async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<PgPool>) {
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
        let pool = isolated.pool();
        (isolated, pool)
    }

    /// `one_time_invites.creator` has an FK to `users`, so a fixture user is
    /// required before any row can be inserted.
    async fn ensure_test_user(pool: &PgPool, user_id: &str) {
        let now = current_timestamp_millis();
        sqlx::query(
            r#"INSERT INTO users (user_id, username, created_ts)
               VALUES ($1, $2, $3)
               ON CONFLICT (user_id) DO NOTHING"#,
        )
        .bind(user_id)
        .bind(user_id.trim_start_matches('@').split(':').next().unwrap_or(user_id))
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test user");
    }

    async fn fixture() -> (crate::test_isolation::IsolatedTestPool, Arc<PgPool>, OneTimeInviteStorage, String) {
        let (isolated, pool) = test_pool().await;
        let suffix = uuid::Uuid::new_v4();
        let creator = format!("@creator_{suffix}:test.com");
        ensure_test_user(&pool, &creator).await;
        let storage = OneTimeInviteStorage::new(pool.clone());
        (isolated, pool, storage, creator)
    }

    #[tokio::test]
    async fn claim_succeeds_once_then_reports_already_claimed() {
        let (_isolated, _pool, storage, creator) = fixture().await;
        let token_hash = format!("hash-{}", uuid::Uuid::new_v4());
        let config = json!({"preset": "private_chat"});
        let expires_at = current_timestamp_millis() + 60_000;

        storage.create(&token_hash, &creator, &config, expires_at).await.expect("create invite");

        let first = storage.claim(&token_hash, "@claimer_a:test.com").await.expect("claim");
        match first {
            ClaimOutcome::Claimed(claimed) => {
                assert_eq!(claimed.creator, creator);
                assert_eq!(claimed.room_config, config);
            }
            other => panic!("first claim must succeed, got {other:?}"),
        }

        let second = storage.claim(&token_hash, "@claimer_b:test.com").await.expect("claim");
        assert!(matches!(second, ClaimOutcome::AlreadyClaimed), "replay must be rejected, got {second:?}");
    }

    #[tokio::test]
    async fn expired_invite_cannot_be_claimed() {
        let (_isolated, _pool, storage, creator) = fixture().await;
        let token_hash = format!("hash-{}", uuid::Uuid::new_v4());
        let expired_at = current_timestamp_millis() - 1_000;

        storage.create(&token_hash, &creator, &json!({}), expired_at).await.expect("create invite");

        let outcome = storage.claim(&token_hash, "@claimer:test.com").await.expect("claim");
        assert!(matches!(outcome, ClaimOutcome::Expired), "expired invite must not be claimable, got {outcome:?}");
    }

    #[tokio::test]
    async fn concurrent_claims_only_one_wins() {
        let (_isolated, _pool, storage, creator) = fixture().await;
        let token_hash = format!("hash-{}", uuid::Uuid::new_v4());
        let expires_at = current_timestamp_millis() + 60_000;
        storage.create(&token_hash, &creator, &json!({}), expires_at).await.expect("create invite");

        let mut handles = Vec::new();
        for index in 0..8 {
            let storage = storage.clone();
            let token_hash = token_hash.clone();
            let claimer = format!("@claimer_{index}:test.com");
            handles.push(tokio::spawn(async move { storage.claim(&token_hash, &claimer).await }));
        }

        let mut wins = 0;
        for handle in handles {
            if matches!(handle.await.expect("join").expect("claim"), ClaimOutcome::Claimed(_)) {
                wins += 1;
            }
        }
        assert_eq!(wins, 1, "exactly one concurrent claim may consume the token");
    }

    #[tokio::test]
    async fn release_claim_restores_claimable_state() {
        let (_isolated, _pool, storage, creator) = fixture().await;
        let token_hash = format!("hash-{}", uuid::Uuid::new_v4());
        let expires_at = current_timestamp_millis() + 60_000;
        storage.create(&token_hash, &creator, &json!({}), expires_at).await.expect("create invite");

        assert!(matches!(
            storage.claim(&token_hash, "@claimer:test.com").await.expect("claim"),
            ClaimOutcome::Claimed(_)
        ));
        assert!(storage.release_claim(&token_hash, "@claimer:test.com").await.expect("release"));

        assert!(matches!(
            storage.claim(&token_hash, "@claimer2:test.com").await.expect("claim"),
            ClaimOutcome::Claimed(_)
        ));
    }

    #[tokio::test]
    async fn unknown_token_reports_not_found() {
        let (_isolated, _pool, storage, _creator) = fixture().await;
        let outcome = storage.claim("does-not-exist", "@claimer:test.com").await.expect("claim");
        assert!(matches!(outcome, ClaimOutcome::NotFound));
    }
}
