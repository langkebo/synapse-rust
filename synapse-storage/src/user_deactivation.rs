//! `user_pending_deactivations` — accounts whose room parting is still owed.
//!
//! Upstream Synapse deactivates an account **without** parting it from its
//! joined rooms inline: `deactivate_account()` writes one row here and returns,
//! and `_user_parter_loop` drains the table in the background. Two properties
//! follow, and both are the point of the table:
//!
//! * a user in thousands of rooms cannot block the deactivation request;
//! * an unfinished parting survives a restart (the loop is restarted from the
//!   table, not from memory).
//!
//! The table is created by the unified schema migration. Queries use the
//! runtime `sqlx::query*` API rather than the compile-time macros because the
//! offline metadata cache (`.sqlx`) does not know this table yet.

use std::sync::Arc;

use synapse_common::ApiError;

/// Storage for the pending-deactivation queue.
pub struct UserDeactivationStorage {
    pool: Arc<sqlx::PgPool>,
}

impl UserDeactivationStorage {
    /// Build the storage over `pool`.
    pub fn new(pool: Arc<sqlx::PgPool>) -> Self {
        Self { pool }
    }

    /// Enqueue `user_id` for background parting.
    ///
    /// Idempotent (`ON CONFLICT DO NOTHING`): a retried deactivation must not
    /// fail just because a previous attempt already wrote the row.
    pub async fn add_user_pending_deactivation(&self, user_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO user_pending_deactivations (user_id, created_ts)
            VALUES ($1, $2)
            ON CONFLICT (user_id) DO NOTHING
            "#,
        )
        .bind(user_id)
        .bind(synapse_common::current_timestamp_millis())
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to enqueue pending deactivation", e))?;
        Ok(())
    }

    /// The oldest queued user, if any.
    ///
    /// `created_ts` first, then `user_id`, so two rows written in the same
    /// millisecond still drain in a deterministic order.
    pub async fn get_user_pending_deactivation(&self) -> Result<Option<String>, ApiError> {
        sqlx::query_scalar::<_, String>(
            r#"
            SELECT user_id FROM user_pending_deactivations
            ORDER BY created_ts, user_id
            LIMIT 1
            "#,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to read pending deactivation", e))
    }

    /// Remove `user_id` from the queue; `true` when a row was deleted.
    pub async fn del_user_pending_deactivation(&self, user_id: &str) -> Result<bool, ApiError> {
        let result = sqlx::query("DELETE FROM user_pending_deactivations WHERE user_id = $1")
            .bind(user_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to clear pending deactivation", e))?;
        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;

    async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<sqlx::PgPool>) {
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
        let pool = isolated.pool();
        (isolated, pool)
    }

    #[tokio::test]
    async fn queued_deactivations_round_trip_fifo_and_are_idempotent() {
        let (_isolated, pool) = test_pool().await;
        let storage = UserDeactivationStorage::new(pool.clone());

        assert_eq!(storage.get_user_pending_deactivation().await.unwrap(), None);

        storage.add_user_pending_deactivation("@alice:test").await.unwrap();
        // Re-enqueueing must not fail or duplicate.
        storage.add_user_pending_deactivation("@alice:test").await.unwrap();
        storage.add_user_pending_deactivation("@bob:test").await.unwrap();

        assert_eq!(storage.get_user_pending_deactivation().await.unwrap().as_deref(), Some("@alice:test"));

        assert!(storage.del_user_pending_deactivation("@alice:test").await.unwrap());
        assert!(!storage.del_user_pending_deactivation("@alice:test").await.unwrap());
        assert_eq!(storage.get_user_pending_deactivation().await.unwrap().as_deref(), Some("@bob:test"));

        storage.del_user_pending_deactivation("@bob:test").await.unwrap();
        assert_eq!(storage.get_user_pending_deactivation().await.unwrap(), None);
    }
}
