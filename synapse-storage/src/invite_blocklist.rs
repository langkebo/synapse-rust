// 房间级与全局邀请名单（本仓扩展）—— 存储层。
//
// ⚠️ 归号订正（2026-10-16）：本文件**不是** MSC4380。MSC4380（Matrix v1.18 已纳入）定义的是
// **用户自己**拒收邀请的单个开关（`default_action: "block" | "allow"`），其正文把
// 「按 user ID / 服务器 / Space 屏蔽」列为**未来扩展**并指向 MSC4155。本文件实现的是
// **房主控制谁能被邀请进本房间**的名单，属本仓扩展。
// 依据：docs/audit/INVITE_PERUSER_POLICY_PROPOSAL_2026-10-16.md §7.2。
// Allows room admins to control who can be invited to a room
// Following project field naming standards

use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

/// Room-level invite restriction verdict for one invitee.
///
/// Produced by [`InviteBlocklistStorage::evaluate`] in a single round-trip so
/// the blocklist and the allowlist can never be read from different snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InviteRestriction {
    /// The invitee is explicitly listed on the room blocklist.
    pub blocked: bool,
    /// The room has a non-empty allowlist, which therefore acts as a whitelist.
    pub allowlist_set: bool,
    /// The invitee is listed on the room allowlist.
    pub allowed: bool,
}

impl InviteRestriction {
    /// `true` when the room's lists refuse this invitee.
    pub fn is_denied(&self) -> bool {
        self.blocked || (self.allowlist_set && !self.allowed)
    }
}

/// Advisory lock key that serialises writers of the two global invite lists.
///
/// The per-user `add_to_global_*` writers enforce cross-table exclusion (a user
/// must not sit on both lists), and the replace-all writers delete every row
/// before inserting. Neither invariant can be expressed as a lock on one row:
/// the delete of an empty table takes no lock at all, so two concurrent writers
/// would each insert their own set and the stored list would become the union of
/// two writes that each promised to be the complete list. One transaction-scoped
/// advisory lock covers both tables and releases on commit/rollback.
pub const GLOBAL_INVITE_LISTS_LOCK_KEY: i64 = 0x474C_4F42_494E_5649_u64 as i64;

/// The `InviteBlocklistStorage` struct.
#[derive(Clone)]
pub struct InviteBlocklistStorage {
    pool: Arc<PgPool>,
}

impl InviteBlocklistStorage {
    /// See [`new`].
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Serialise writers that rewrite one room's invite lists.
    ///
    /// `DELETE` takes no lock when it matches nothing, so two rewrites of an
    /// empty list would each insert their own set and both commit. Locking the
    /// owning `rooms` row makes the whole rewrite mutually exclusive, and the
    /// lock releases with the transaction. A missing room needs no lock — the
    /// foreign key rejects the insert anyway.
    async fn lock_room_row(conn: &mut sqlx::PgConnection, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query_scalar!(r#"SELECT 1 AS "locked!" FROM rooms WHERE room_id = $1 FOR UPDATE"#, room_id)
            .fetch_optional(conn)
            .await?;
        Ok(())
    }

    /// Serialise writers of the global invite lists (see
    /// [`GLOBAL_INVITE_LISTS_LOCK_KEY`]). Held until the transaction ends.
    async fn lock_global_invite_lists(conn: &mut sqlx::PgConnection) -> Result<(), sqlx::Error> {
        sqlx::query_scalar!("SELECT pg_advisory_xact_lock($1) IS NULL", GLOBAL_INVITE_LISTS_LOCK_KEY)
            .fetch_one(conn)
            .await?;
        Ok(())
    }

    /// Set the invite blocklist for a room (users that cannot be invited).
    ///
    /// The clear-then-insert pair runs in one transaction: a reader can never
    /// observe the empty window between them, which would otherwise read as
    /// "no restriction" and let a blocked invitee through.
    pub async fn set_invite_blocklist(&self, room_id: &str, user_ids: Vec<String>) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        let mut tx = self.pool.begin().await?;

        Self::lock_room_row(&mut tx, room_id).await?;

        sqlx::query!("DELETE FROM room_invite_blocklist WHERE room_id = $1", room_id).execute(&mut *tx).await?;

        if !user_ids.is_empty() {
            sqlx::query!(
                r#"
                INSERT INTO room_invite_blocklist (room_id, user_id, created_ts)
                SELECT $1, unnest($2::text[]), $3
                ON CONFLICT DO NOTHING
                "#,
                room_id,
                &user_ids,
                now,
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Get the invite blocklist for a room
    pub async fn get_invite_blocklist(&self, room_id: &str) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar!(
            r#"
            SELECT user_id FROM room_invite_blocklist WHERE room_id = $1
            "#,
            room_id,
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// Evaluate both room lists for one invitee in a single round-trip.
    pub async fn evaluate(&self, room_id: &str, user_id: &str) -> Result<InviteRestriction, sqlx::Error> {
        let row = sqlx::query!(
            r#"
            SELECT
                EXISTS (SELECT 1 FROM room_invite_blocklist b WHERE b.room_id = $1 AND b.user_id = $2) AS "blocked!",
                EXISTS (SELECT 1 FROM room_invite_allowlist a WHERE a.room_id = $1 AND a.user_id = $2) AS "allowed!",
                EXISTS (SELECT 1 FROM room_invite_allowlist w WHERE w.room_id = $1) AS "allowlist_set!"
            "#,
            room_id,
            user_id,
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(InviteRestriction { blocked: row.blocked, allowlist_set: row.allowlist_set, allowed: row.allowed })
    }

    /// Set the invite allowlist for a room (only these users can be invited).
    ///
    /// Runs in one transaction for the same reason as
    /// [`Self::set_invite_blocklist`].
    pub async fn set_invite_allowlist(&self, room_id: &str, user_ids: Vec<String>) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        let mut tx = self.pool.begin().await?;

        Self::lock_room_row(&mut tx, room_id).await?;

        sqlx::query!("DELETE FROM room_invite_allowlist WHERE room_id = $1", room_id).execute(&mut *tx).await?;

        if !user_ids.is_empty() {
            sqlx::query!(
                r#"
                INSERT INTO room_invite_allowlist (room_id, user_id, created_ts)
                SELECT $1, unnest($2::text[]), $3
                ON CONFLICT DO NOTHING
                "#,
                room_id,
                &user_ids,
                now,
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Get the invite allowlist for a room
    pub async fn get_invite_allowlist(&self, room_id: &str) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar!(
            r#"
            SELECT user_id FROM room_invite_allowlist WHERE room_id = $1
            "#,
            room_id,
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// Check whether `user_id` appears in the global blocklist.
    pub async fn is_user_in_global_blocklist(&self, user_id: &str) -> Result<bool, sqlx::Error> {
        let exists = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (SELECT 1 FROM global_invite_blocklist WHERE user_id = $1) AS "exists!"
            "#,
            user_id,
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(exists)
    }

    /// Check whether `user_id` appears in the global allowlist.
    pub async fn is_user_in_global_allowlist(&self, user_id: &str) -> Result<bool, sqlx::Error> {
        let exists = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (SELECT 1 FROM global_invite_allowlist WHERE user_id = $1) AS "exists!"
            "#,
            user_id,
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(exists)
    }

    /// Set the global invite blocklist (replace-all semantics).
    ///
    /// Removes all existing global blocklist entries and inserts the
    /// given users. Runs in one transaction for atomicity.
    pub async fn set_global_invite_blocklist(&self, user_ids: Vec<String>) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        let mut tx = self.pool.begin().await?;

        Self::lock_global_invite_lists(&mut tx).await?;

        sqlx::query!("DELETE FROM global_invite_blocklist").execute(&mut *tx).await?;

        if !user_ids.is_empty() {
            sqlx::query!(
                r#"
                INSERT INTO global_invite_blocklist (user_id, created_ts)
                SELECT unnest($1::text[]), $2
                ON CONFLICT (user_id) DO NOTHING
                "#,
                &user_ids,
                now,
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Set the global invite allowlist (replace-all semantics).
    pub async fn set_global_invite_allowlist(&self, user_ids: Vec<String>) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        let mut tx = self.pool.begin().await?;

        Self::lock_global_invite_lists(&mut tx).await?;

        sqlx::query!("DELETE FROM global_invite_allowlist").execute(&mut *tx).await?;

        if !user_ids.is_empty() {
            sqlx::query!(
                r#"
                INSERT INTO global_invite_allowlist (user_id, created_ts)
                SELECT unnest($1::text[]), $2
                ON CONFLICT (user_id) DO NOTHING
                "#,
                &user_ids,
                now,
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Add one user to the global invite blocklist (per-user, idempotent).
    ///
    /// Q1（2026-10-16 决策 D3/D4/D5）三段语义：
    ///   * **幂等**：重复调用不报错，且**不覆盖**既有 `reason` / `created_by`
    ///     （`ON CONFLICT (user_id) DO NOTHING`）—— 重复 POST 应当是"状态不变"，不是"改写记录"；
    ///   * **跨表互斥**：加入 blocklist 时把同一 user 从 allowlist 移除，使"同一人同时出现在
    ///     两个全局名单"不再因单条增删而产生。
    ///     ⚠️ 整体替换端点（[`Self::set_global_invite_blocklist`]）**不做**互斥：它是声明式
    ///     全量设置，调用方给出什么名单就存什么名单；
    ///   * **审计**：`reason` / `created_by` 落库（调用方不得自行伪造 `created_by`）。
    ///
    /// 返回 `(inserted, removed_from_other_list)`：前者 `false` 表示"本来就在名单里"。
    pub async fn add_to_global_invite_blocklist(
        &self,
        user_id: &str,
        reason: Option<&str>,
        created_by: Option<&str>,
    ) -> Result<(bool, bool), sqlx::Error> {
        let now = current_timestamp_millis();
        let mut tx = self.pool.begin().await?;

        Self::lock_global_invite_lists(&mut tx).await?;

        let deleted =
            sqlx::query!("DELETE FROM global_invite_allowlist WHERE user_id = $1", user_id).execute(&mut *tx).await?;

        let inserted_row = sqlx::query!(
            r#"
            INSERT INTO global_invite_blocklist (user_id, created_ts, reason, created_by)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id) DO NOTHING
            "#,
            user_id,
            now,
            reason,
            created_by,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok((inserted_row.rows_affected() > 0, deleted.rows_affected() > 0))
    }

    /// Remove one user from the global invite blocklist (per-user, idempotent).
    ///
    /// 返回 `true` 表示确实删掉了一行；`false` 表示本来就不在名单里（端点同样返回 200）。
    pub async fn remove_from_global_invite_blocklist(&self, user_id: &str) -> Result<bool, sqlx::Error> {
        let deleted = sqlx::query!("DELETE FROM global_invite_blocklist WHERE user_id = $1", user_id)
            .execute(&*self.pool)
            .await?;
        Ok(deleted.rows_affected() > 0)
    }

    /// Add one user to the global invite allowlist (per-user, idempotent).
    ///
    /// 与 [`Self::add_to_global_invite_blocklist`] 对称：幂等、审计、并把同一 user
    /// 从 **blocklist** 移除（跨表互斥）。返回 `(inserted, removed_from_other_list)`。
    pub async fn add_to_global_invite_allowlist(
        &self,
        user_id: &str,
        reason: Option<&str>,
        created_by: Option<&str>,
    ) -> Result<(bool, bool), sqlx::Error> {
        let now = current_timestamp_millis();
        let mut tx = self.pool.begin().await?;

        Self::lock_global_invite_lists(&mut tx).await?;

        let deleted =
            sqlx::query!("DELETE FROM global_invite_blocklist WHERE user_id = $1", user_id).execute(&mut *tx).await?;

        let inserted_row = sqlx::query!(
            r#"
            INSERT INTO global_invite_allowlist (user_id, created_ts, reason, created_by)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id) DO NOTHING
            "#,
            user_id,
            now,
            reason,
            created_by,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok((inserted_row.rows_affected() > 0, deleted.rows_affected() > 0))
    }

    /// Remove one user from the global invite allowlist (per-user, idempotent).
    ///
    /// 返回 `true` 表示确实删掉了一行；`false` 表示本来就不在名单里（端点同样返回 200）。
    pub async fn remove_from_global_invite_allowlist(&self, user_id: &str) -> Result<bool, sqlx::Error> {
        let deleted = sqlx::query!("DELETE FROM global_invite_allowlist WHERE user_id = $1", user_id)
            .execute(&*self.pool)
            .await?;
        Ok(deleted.rows_affected() > 0)
    }

    /// Get global invite blocklist (all users) with pagination.
    pub async fn get_global_invite_blocklist_paginated(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT user_id, created_ts, reason, created_by FROM global_invite_blocklist
            ORDER BY created_ts DESC, user_id ASC
            LIMIT $1 OFFSET $2
            "#,
            limit,
            offset,
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                serde_json::json!({
                    "user_id": row.user_id,
                    "created_ts": row.created_ts,
                    // Q1（2026-10-16）：per-user 端点写入的审计归因。整体替换端点与存量行
                    // 这两列是 NULL（序列化为 null），消费方必须容忍。
                    "reason": row.reason,
                    "created_by": row.created_by
                })
            })
            .collect())
    }

    /// Get global invite allowlist (all users) with pagination.
    pub async fn get_global_invite_allowlist_paginated(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT user_id, created_ts, reason, created_by FROM global_invite_allowlist
            ORDER BY created_ts DESC, user_id ASC
            LIMIT $1 OFFSET $2
            "#,
            limit,
            offset,
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                serde_json::json!({
                    "user_id": row.user_id,
                    "created_ts": row.created_ts,
                    // Q1（2026-10-16）：per-user 端点写入的审计归因。整体替换端点与存量行
                    // 这两列是 NULL（序列化为 null），消费方必须容忍。
                    "reason": row.reason,
                    "created_by": row.created_by
                })
            })
            .collect())
    }

    /// Return the total row count for the global invite blocklist.
    pub async fn global_invite_blocklist_count(&self) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar!(r#"SELECT COUNT(*) AS "count!" FROM global_invite_blocklist"#)
            .fetch_one(&*self.pool)
            .await?;
        Ok(count)
    }

    /// Return the total row count for the global invite allowlist.
    pub async fn global_invite_allowlist_count(&self) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar!(r#"SELECT COUNT(*) AS "count!" FROM global_invite_allowlist"#)
            .fetch_one(&*self.pool)
            .await?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_user_id_format() {
        let valid_users = vec!["@user:localhost", "@alice:example.com"];

        for user in valid_users {
            assert!(user.starts_with('@'), "User ID should start with @");
            assert!(user.contains(':'), "User ID should contain : separator");
        }
    }

    #[test]
    fn test_room_id_format() {
        let valid_rooms = vec!["!room:localhost", "!abc123:matrix.org"];

        for room in valid_rooms {
            assert!(room.starts_with('!'), "Room ID should start with !");
            assert!(room.contains(':'), "Room ID should contain : separator");
        }
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;

    /// Shared `public` is deliberately replaced by a per-test schema here:
    /// the blocklist/allowlist helpers are keyed on `room_id`, so on shared `public` a
    /// sibling test that reused a fixture room could clear the rows asserted here.
    ///
    /// Eliminating the shared state removes the race instead of serialising around it
    /// (AGENTS.md rule 7). The guard is returned with the pool so the schema outlives
    /// the whole test — dropping it early spawns a background `DROP SCHEMA`.
    async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<PgPool>) {
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
        let pool = isolated.pool();
        (isolated, pool)
    }

    /// Insert a minimal room row so the FK constraint on
    /// room_invite_blocklist/room_invite_allowlist.room_id is satisfied.
    async fn ensure_test_room(pool: &PgPool, room_id: &str) {
        let now = current_timestamp_millis();
        sqlx::query(
            r#"INSERT INTO rooms (room_id, created_ts)
               VALUES ($1, $2)
               ON CONFLICT (room_id) DO NOTHING"#,
        )
        .bind(room_id)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test room");
    }

    /// Cleanup blocklist rows for a given room (idempotent, skips errors).
    async fn cleanup_blocklist(pool: &PgPool, room_id: &str) {
        sqlx::query("DELETE FROM room_invite_blocklist WHERE room_id = $1").bind(room_id).execute(pool).await.expect(
            "test fixture: delete must succeed — a swallowed error here surfaces later as an unrelated failure",
        );
    }

    /// Cleanup allowlist rows for a given room (idempotent, skips errors).
    async fn cleanup_allowlist(pool: &PgPool, room_id: &str) {
        sqlx::query("DELETE FROM room_invite_allowlist WHERE room_id = $1").bind(room_id).execute(pool).await.expect(
            "test fixture: delete must succeed — a swallowed error here surfaces later as an unrelated failure",
        );
    }

    #[tokio::test]
    async fn test_set_and_get_invite_blocklist() {
        let (_isolated, pool) = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_bl_{suffix}:test.com");
        let user_a = format!("@user_a_{suffix}:test.com");
        let user_b = format!("@user_b_{suffix}:test.com");

        cleanup_blocklist(&pool, &room_id).await;
        ensure_test_room(&pool, &room_id).await;

        storage
            .set_invite_blocklist(&room_id, vec![user_a.clone(), user_b.clone()])
            .await
            .expect("set_invite_blocklist should succeed");

        let blocklist = storage.get_invite_blocklist(&room_id).await.expect("get_invite_blocklist should succeed");

        assert_eq!(blocklist.len(), 2);
        assert!(blocklist.contains(&user_a), "blocklist should contain user_a");
        assert!(blocklist.contains(&user_b), "blocklist should contain user_b");

        cleanup_blocklist(&pool, &room_id).await;
    }

    #[tokio::test]
    async fn test_evaluate_blocklist_denies_listed_user() {
        let (_isolated, pool) = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_blocked_{suffix}:test.com");
        let blocked_user = format!("@blocked_{suffix}:test.com");
        let free_user = format!("@free_{suffix}:test.com");

        cleanup_blocklist(&pool, &room_id).await;
        ensure_test_room(&pool, &room_id).await;

        storage
            .set_invite_blocklist(&room_id, vec![blocked_user.clone()])
            .await
            .expect("set_invite_blocklist should succeed");

        let listed = storage.evaluate(&room_id, &blocked_user).await.expect("evaluate should succeed");
        assert!(listed.blocked, "listed user should be reported as blocked");
        assert!(listed.is_denied(), "listed user must be denied");

        let unlisted = storage.evaluate(&room_id, &free_user).await.expect("evaluate should succeed");
        assert!(!unlisted.blocked, "unlisted user should not be reported as blocked");
        assert!(!unlisted.is_denied(), "unlisted user must not be denied by an empty allowlist");

        cleanup_blocklist(&pool, &room_id).await;
    }

    #[tokio::test]
    async fn test_set_invite_blocklist_overwrites_previous() {
        let (_isolated, pool) = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_overwrite_{suffix}:test.com");
        let user_a = format!("@user_a_{suffix}:test.com");
        let user_b = format!("@user_b_{suffix}:test.com");
        let user_c = format!("@user_c_{suffix}:test.com");

        cleanup_blocklist(&pool, &room_id).await;
        ensure_test_room(&pool, &room_id).await;

        // Set initial blocklist with A and B
        storage
            .set_invite_blocklist(&room_id, vec![user_a.clone(), user_b.clone()])
            .await
            .expect("first set should succeed");

        // Overwrite with only C
        storage.set_invite_blocklist(&room_id, vec![user_c.clone()]).await.expect("second set should succeed");

        let blocklist = storage.get_invite_blocklist(&room_id).await.expect("get_invite_blocklist should succeed");

        assert_eq!(blocklist.len(), 1, "blocklist should have exactly 1 entry after overwrite");
        assert!(blocklist.contains(&user_c), "blocklist should contain only user_c");
        assert!(!blocklist.contains(&user_a), "blocklist should not contain user_a");
        assert!(!blocklist.contains(&user_b), "blocklist should not contain user_b");

        cleanup_blocklist(&pool, &room_id).await;
    }

    #[tokio::test]
    async fn test_set_and_get_invite_allowlist() {
        let (_isolated, pool) = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_al_{suffix}:test.com");
        let user_a = format!("@allowed_a_{suffix}:test.com");
        let user_b = format!("@allowed_b_{suffix}:test.com");

        cleanup_allowlist(&pool, &room_id).await;
        ensure_test_room(&pool, &room_id).await;

        storage
            .set_invite_allowlist(&room_id, vec![user_a.clone(), user_b.clone()])
            .await
            .expect("set_invite_allowlist should succeed");

        let allowlist = storage.get_invite_allowlist(&room_id).await.expect("get_invite_allowlist should succeed");

        assert_eq!(allowlist.len(), 2);
        assert!(allowlist.contains(&user_a), "allowlist should contain user_a");
        assert!(allowlist.contains(&user_b), "allowlist should contain user_b");

        cleanup_allowlist(&pool, &room_id).await;
    }

    #[tokio::test]
    async fn test_evaluate_allowlist_is_whitelist_when_non_empty() {
        let (_isolated, pool) = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_allowed_{suffix}:test.com");
        let allowed_user = format!("@allowed_{suffix}:test.com");
        let not_allowed_user = format!("@not_allowed_{suffix}:test.com");

        cleanup_allowlist(&pool, &room_id).await;
        ensure_test_room(&pool, &room_id).await;

        // An empty allowlist is not a whitelist: everyone passes.
        let before = storage.evaluate(&room_id, &not_allowed_user).await.expect("evaluate should succeed");
        assert!(!before.allowlist_set, "no allowlist rows means the allowlist is not in force");
        assert!(!before.is_denied(), "empty allowlist must not deny");

        storage
            .set_invite_allowlist(&room_id, vec![allowed_user.clone()])
            .await
            .expect("set_invite_allowlist should succeed");

        let listed = storage.evaluate(&room_id, &allowed_user).await.expect("evaluate should succeed");
        assert!(listed.allowed && listed.allowlist_set, "listed user should be reported as allowed");
        assert!(!listed.is_denied(), "allowlisted user must not be denied");

        let unlisted = storage.evaluate(&room_id, &not_allowed_user).await.expect("evaluate should succeed");
        assert!(!unlisted.allowed, "unlisted user should not be reported as allowed");
        assert!(unlisted.is_denied(), "non-empty allowlist must deny users missing from it");

        cleanup_allowlist(&pool, &room_id).await;
    }

    /// A rewrite that fails halfway must leave the previous list intact.
    ///
    /// The failure is injected with a `BEFORE INSERT` trigger rather than a
    /// constraint violation because the only constraints on these tables are
    /// the FK (already satisfied by `ensure_test_room`) and a UNIQUE that the
    /// statement's `ON CONFLICT DO NOTHING` swallows.
    #[tokio::test]
    async fn test_set_invite_blocklist_is_atomic_on_insert_failure() {
        let (_isolated, pool) = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_atomic_{suffix}:test.com");
        let keep_user = format!("@keep_{suffix}:test.com");
        let poison_user = format!("@poison_{suffix}:test.com");

        cleanup_blocklist(&pool, &room_id).await;
        ensure_test_room(&pool, &room_id).await;

        storage.set_invite_blocklist(&room_id, vec![keep_user.clone()]).await.expect("initial set should succeed");

        sqlx::query(
            r#"
            CREATE OR REPLACE FUNCTION invite_blocklist_poison_insert() RETURNS trigger AS $$
            BEGIN
                IF NEW.user_id = current_setting('synapse.test_poison_user') THEN
                    RAISE EXCEPTION 'injected insert failure';
                END IF;
                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql
            "#,
        )
        .execute(&*pool)
        .await
        .expect("test fixture: poison function must be created");

        sqlx::query("CREATE TRIGGER trg_invite_blocklist_poison BEFORE INSERT ON room_invite_blocklist FOR EACH ROW EXECUTE FUNCTION invite_blocklist_poison_insert()")
            .execute(&*pool)
            .await
            .expect("test fixture: poison trigger must be created");

        sqlx::query("SELECT set_config('synapse.test_poison_user', $1, false)")
            .bind(&poison_user)
            .execute(&*pool)
            .await
            .expect("test fixture: poison target must be set");

        let err = storage
            .set_invite_blocklist(&room_id, vec![keep_user.clone(), poison_user.clone()])
            .await
            .expect_err("the poisoned insert must surface as an error");
        assert!(err.to_string().contains("injected insert failure"), "expected the injected trigger error, got: {err}");

        let surviving = storage.get_invite_blocklist(&room_id).await.expect("get_invite_blocklist should succeed");
        assert_eq!(
            surviving,
            vec![keep_user.clone()],
            "a failed rewrite must roll back to the previous blocklist, not leave it empty"
        );

        sqlx::query("DROP TRIGGER IF EXISTS trg_invite_blocklist_poison ON room_invite_blocklist")
            .execute(&*pool)
            .await
            .expect("test fixture: trigger cleanup must succeed");
        cleanup_blocklist(&pool, &room_id).await;
    }

    #[tokio::test]
    async fn test_evaluate_reports_allowlist_scope_per_room() {
        let (_isolated, pool) = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_no_restrict = format!("!room_none_{suffix}:test.com");
        let room_block = format!("!room_block_{suffix}:test.com");
        let room_allow = format!("!room_allow_{suffix}:test.com");
        let room_both = format!("!room_both_{suffix}:test.com");
        let user = format!("@user_{suffix}:test.com");

        // Ensure all rooms exist
        for rid in [&room_no_restrict, &room_block, &room_allow, &room_both] {
            cleanup_blocklist(&pool, rid).await;
            cleanup_allowlist(&pool, rid).await;
            ensure_test_room(&pool, rid).await;
        }

        // Room with no entries at all.
        let none = storage.evaluate(&room_no_restrict, &user).await.expect("evaluate should succeed");
        assert_eq!(none, InviteRestriction { blocked: false, allowlist_set: false, allowed: false });
        assert!(!none.is_denied());

        // Room with only a blocklist.
        storage
            .set_invite_blocklist(&room_block, vec![user.clone()])
            .await
            .expect("set_invite_blocklist should succeed");
        let blocked = storage.evaluate(&room_block, &user).await.expect("evaluate should succeed");
        assert!(blocked.blocked && !blocked.allowlist_set);
        assert!(blocked.is_denied());

        // Room with only an allowlist: the list is in force for everyone.
        storage
            .set_invite_allowlist(&room_allow, vec![user.clone()])
            .await
            .expect("set_invite_allowlist should succeed");
        let allow_only = storage.evaluate(&room_allow, &user).await.expect("evaluate should succeed");
        assert!(allow_only.allowlist_set && allow_only.allowed);
        assert!(!allow_only.is_denied());

        // Room with both lists: the blocklist wins over the allowlist.
        storage
            .set_invite_blocklist(&room_both, vec![user.clone()])
            .await
            .expect("set_invite_blocklist for both should succeed");
        storage
            .set_invite_allowlist(&room_both, vec![user.clone()])
            .await
            .expect("set_invite_allowlist for both should succeed");
        let both = storage.evaluate(&room_both, &user).await.expect("evaluate should succeed");
        assert!(both.blocked && both.allowlist_set && both.allowed);
        assert!(both.is_denied(), "an explicitly blocked user stays blocked even when allowlisted");

        // Cleanup
        for rid in [&room_no_restrict, &room_block, &room_allow, &room_both] {
            cleanup_blocklist(&pool, rid).await;
            cleanup_allowlist(&pool, rid).await;
        }
    }

    #[tokio::test]
    async fn test_get_global_invite_blocklist() {
        let (_isolated, pool) = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let user_a = format!("@global_user_a_{suffix}:test.com");
        let user_b = format!("@global_user_b_{suffix}:test.com");

        cleanup_global_blocklist(&pool).await;

        storage
            .set_global_invite_blocklist(vec![user_a.clone(), user_b.clone()])
            .await
            .expect("set global blocklist should succeed");

        let global = storage
            .get_global_invite_blocklist_paginated(100, 0)
            .await
            .expect("get_global_invite_blocklist_paginated should succeed");

        assert_eq!(global.len(), 2, "global blocklist should have exactly 2 entries");

        let user_ids: Vec<&str> = global.iter().map(|v| v["user_id"].as_str().unwrap()).collect();
        assert!(user_ids.contains(&user_a.as_str()), "global should contain user_a");
        assert!(user_ids.contains(&user_b.as_str()), "global should contain user_b");

        cleanup_global_blocklist(&pool).await;
    }

    #[tokio::test]
    async fn test_get_global_invite_allowlist() {
        let (_isolated, pool) = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let user_a = format!("@global_al_a_{suffix}:test.com");
        let user_b = format!("@global_al_b_{suffix}:test.com");

        cleanup_global_allowlist(&pool).await;

        storage
            .set_global_invite_allowlist(vec![user_a.clone(), user_b.clone()])
            .await
            .expect("set global allowlist should succeed");

        let global = storage
            .get_global_invite_allowlist_paginated(100, 0)
            .await
            .expect("get_global_invite_allowlist_paginated should succeed");

        assert_eq!(
            global.len(),
            2,
            "global allowlist should have exactly 2 entries in isolated schema, got {}",
            global.len()
        );

        let user_ids: Vec<&str> = global.iter().map(|v| v["user_id"].as_str().unwrap()).collect();
        assert!(user_ids.contains(&user_a.as_str()));
        assert!(user_ids.contains(&user_b.as_str()));

        cleanup_global_allowlist(&pool).await;
    }

    #[tokio::test]
    async fn test_round_trip_blocklist_set_and_clear() {
        let (_isolated, pool) = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_clear_{suffix}:test.com");
        let user = format!("@to_clear_{suffix}:test.com");

        cleanup_blocklist(&pool, &room_id).await;
        ensure_test_room(&pool, &room_id).await;

        // Set blocklist
        storage.set_invite_blocklist(&room_id, vec![user.clone()]).await.expect("set_invite_blocklist should succeed");

        let blocklist = storage.get_invite_blocklist(&room_id).await.expect("get_invite_blocklist should succeed");
        assert_eq!(blocklist.len(), 1, "blocklist should have 1 entry after set");

        // Clear by setting empty vec
        storage
            .set_invite_blocklist(&room_id, vec![])
            .await
            .expect("set_invite_blocklist with empty vec should succeed");

        let cleared = storage.get_invite_blocklist(&room_id).await.expect("get_invite_blocklist should succeed");
        assert!(cleared.is_empty(), "blocklist should be empty after setting empty vec");

        cleanup_blocklist(&pool, &room_id).await;
    }

    /// Cleanup global blocklist rows (idempotent, skips errors).
    async fn cleanup_global_blocklist(pool: &PgPool) {
        sqlx::query("DELETE FROM global_invite_blocklist")
            .execute(pool)
            .await
            .expect("cleanup global blocklist must succeed");
    }

    /// Cleanup global allowlist rows (idempotent, skips errors).
    async fn cleanup_global_allowlist(pool: &PgPool) {
        sqlx::query("DELETE FROM global_invite_allowlist")
            .execute(pool)
            .await
            .expect("cleanup global allowlist must succeed");
    }

    #[tokio::test]
    async fn test_set_global_invite_blocklist() {
        let (_isolated, pool) = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());

        cleanup_global_blocklist(&pool).await;

        storage
            .set_global_invite_blocklist(vec!["@user_a:test.com".into(), "@user_b:test.com".into()])
            .await
            .expect("set_global_invite_blocklist should succeed");

        let exists_a = storage.is_user_in_global_blocklist("@user_a:test.com").await.expect("check user_a");
        let exists_b = storage.is_user_in_global_blocklist("@user_b:test.com").await.expect("check user_b");
        assert!(exists_a, "user_a should be in global blocklist");
        assert!(exists_b, "user_b should be in global blocklist");

        // Replace with different users
        storage.set_global_invite_blocklist(vec!["@user_c:test.com".into()]).await.expect("replace should succeed");

        let exists_a2 =
            storage.is_user_in_global_blocklist("@user_a:test.com").await.expect("check user_a after replace");
        let exists_c = storage.is_user_in_global_blocklist("@user_c:test.com").await.expect("check user_c");
        assert!(!exists_a2, "user_a should be removed after replace");
        assert!(exists_c, "user_c should be in global blocklist");

        cleanup_global_blocklist(&pool).await;
    }

    #[tokio::test]
    async fn test_set_global_invite_allowlist() {
        let (_isolated, pool) = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());

        cleanup_global_allowlist(&pool).await;

        storage
            .set_global_invite_allowlist(vec!["@user_a:test.com".into(), "@user_b:test.com".into()])
            .await
            .expect("set_global_invite_allowlist should succeed");

        let exists_a = storage.is_user_in_global_allowlist("@user_a:test.com").await.expect("check user_a");
        let exists_b = storage.is_user_in_global_allowlist("@user_b:test.com").await.expect("check user_b");
        assert!(exists_a, "user_a should be in global allowlist");
        assert!(exists_b, "user_b should be in global allowlist");

        // Replace with different user
        storage.set_global_invite_allowlist(vec!["@user_c:test.com".into()]).await.expect("replace should succeed");

        let exists_a2 =
            storage.is_user_in_global_allowlist("@user_a:test.com").await.expect("check user_a after replace");
        let exists_c = storage.is_user_in_global_allowlist("@user_c:test.com").await.expect("check user_c");
        assert!(!exists_a2, "user_a should be removed after replace");
        assert!(exists_c, "user_c should be in global allowlist");

        cleanup_global_allowlist(&pool).await;
    }

    #[tokio::test]
    async fn test_global_list_empty_after_set() {
        let (_isolated, pool) = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());

        cleanup_global_blocklist(&pool).await;
        cleanup_global_allowlist(&pool).await;

        // Empty vec should clear the list
        storage
            .set_global_invite_blocklist(vec![])
            .await
            .expect("set_global_invite_blocklist with empty vec should succeed");

        let exists = storage.is_user_in_global_blocklist("@nobody:test.com").await.expect("check");
        assert!(!exists, "global blocklist should be empty");

        cleanup_global_blocklist(&pool).await;
    }

    // ── PERF-09: two rewrites in flight must not merge ────────────────────
    //
    // A rewrite is delete-then-insert inside one transaction. When the list is
    // empty the `DELETE` matches no rows and therefore takes no lock, so two
    // writers can each insert their own set and both commit: the stored list
    // becomes the union of two writes that each promised to be the complete
    // list. Writers are now serialised — room lists on the `rooms` row, the
    // global lists on an advisory lock — and the tests below hold those locks
    // from a second session to prove a writer really waits.

    /// An extra session pinned to the isolated schema.
    ///
    /// `IsolatedTestPool` caps its pool at one connection so that dozens of
    /// parallel per-test schemas cannot exhaust Postgres `max_connections`.
    /// That cap makes "the writer is still running 200ms later" vacuous — the
    /// writer would be queued for a connection, not for a lock. Tests that need
    /// two live sessions therefore open this one by hand, keeping the pool's
    /// single connection free for the storage call under test.
    async fn lock_session(isolated: &crate::test_isolation::IsolatedTestPool) -> sqlx::PgConnection {
        use sqlx::Connection as _;

        let mut conn = sqlx::PgConnection::connect(&synapse_common::test_isolation::test_database_url())
            .await
            .expect("lock session must connect");
        // Unqualified SQL in this session must resolve to the test schema, not
        // to `public` — otherwise the fixture locks the wrong `rooms` row.
        sqlx::query(&format!(r#"SET search_path TO "{}", public"#, isolated.schema_name()))
            .execute(&mut conn)
            .await
            .expect("lock session must join the isolated schema");
        conn
    }

    #[tokio::test]
    async fn test_set_invite_blocklist_waits_for_the_room_row_lock() {
        let (isolated, pool) = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_rowlock_{suffix}:test.com");
        let listed_user = format!("@rowlock_{suffix}:test.com");
        ensure_test_room(&pool, &room_id).await;
        storage.set_invite_blocklist(&room_id, vec![listed_user]).await.expect("seed the blocklist");

        let mut blocker = lock_session(&isolated).await;
        sqlx::query("BEGIN").execute(&mut blocker).await.expect("test fixture: begin blocker transaction");
        sqlx::query("SELECT 1 FROM rooms WHERE room_id = $1 FOR UPDATE")
            .bind(&room_id)
            .fetch_one(&mut blocker)
            .await
            .expect("test fixture: the room row lock must be held");

        // The rewrite clears the list to empty. No rows are inserted, so the
        // foreign-key check on `rooms` takes no lock and cannot be the reason
        // the writer waits: only an explicit row lock placed by the rewrite
        // itself can hold it back.
        let writer = {
            let storage = storage.clone();
            let room_id = room_id.clone();
            tokio::spawn(async move { storage.set_invite_blocklist(&room_id, vec![]).await })
        };

        // sleep-reason: 等封禁过期（该用例的 TTL 是 200ms）
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(
            !writer.is_finished(),
            "a concurrent rewrite must wait for the room row lock; without it two writers each \
             insert their own set and the winner is a union neither caller asked for"
        );

        sqlx::query("ROLLBACK")
            .execute(&mut blocker)
            .await
            .expect("test fixture: the blocker must release the row lock");
        writer.await.expect("writer task must not panic").expect("rewrite must succeed after the lock is released");

        let after = storage.get_invite_blocklist(&room_id).await.expect("read the blocklist back");
        assert!(after.is_empty(), "the serialised rewrite must still have cleared the list, got {after:?}");

        cleanup_blocklist(&pool, &room_id).await;
    }

    #[tokio::test]
    async fn test_set_invite_allowlist_waits_for_the_room_row_lock() {
        let (isolated, pool) = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!room_rowlock_al_{suffix}:test.com");
        let listed_user = format!("@rowlock_al_{suffix}:test.com");
        ensure_test_room(&pool, &room_id).await;
        storage.set_invite_allowlist(&room_id, vec![listed_user]).await.expect("seed the allowlist");

        let mut blocker = lock_session(&isolated).await;
        sqlx::query("BEGIN").execute(&mut blocker).await.expect("test fixture: begin blocker transaction");
        sqlx::query("SELECT 1 FROM rooms WHERE room_id = $1 FOR UPDATE")
            .bind(&room_id)
            .fetch_one(&mut blocker)
            .await
            .expect("test fixture: the room row lock must be held");

        let writer = {
            let storage = storage.clone();
            let room_id = room_id.clone();
            tokio::spawn(async move { storage.set_invite_allowlist(&room_id, vec![]).await })
        };

        // sleep-reason: 等封禁过期（同上）
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(
            !writer.is_finished(),
            "a concurrent allowlist rewrite must wait for the room row lock, for the same reason as \
             the blocklist rewrite"
        );

        sqlx::query("ROLLBACK")
            .execute(&mut blocker)
            .await
            .expect("test fixture: the blocker must release the row lock");
        writer.await.expect("writer task must not panic").expect("rewrite must succeed after the lock is released");

        let after = storage.get_invite_allowlist(&room_id).await.expect("read the allowlist back");
        assert!(after.is_empty(), "the serialised rewrite must still have cleared the list, got {after:?}");

        cleanup_allowlist(&pool, &room_id).await;
    }

    #[tokio::test]
    async fn test_global_list_writers_wait_for_the_advisory_lock() {
        let (isolated, pool) = test_pool().await;
        let storage = InviteBlocklistStorage::new(pool.clone());
        let user_id = format!("@advlock_{}:test.com", uuid::Uuid::new_v4());

        let mut blocker = lock_session(&isolated).await;
        sqlx::query("BEGIN").execute(&mut blocker).await.expect("test fixture: begin blocker transaction");
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(GLOBAL_INVITE_LISTS_LOCK_KEY)
            .execute(&mut blocker)
            .await
            .expect("test fixture: the global invite-list lock must be held");

        let writer = {
            let storage = storage.clone();
            let user_id = user_id.clone();
            tokio::spawn(async move { storage.add_to_global_invite_blocklist(&user_id, None, None).await })
        };

        // sleep-reason: 等封禁过期（同上）
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(
            !writer.is_finished(),
            "global list writes must serialise on one lock: add-to-blocklist deletes from the \
             allowlist and then inserts, so two racing writes can leave a user on both lists"
        );

        sqlx::query("ROLLBACK").execute(&mut blocker).await.expect("test fixture: the blocker must release the lock");
        let (inserted, _) =
            writer.await.expect("writer task must not panic").expect("write must succeed after the lock is released");
        assert!(inserted, "the user was not on the list, so the write must report an insert");

        cleanup_global_blocklist(&pool).await;
    }
}
