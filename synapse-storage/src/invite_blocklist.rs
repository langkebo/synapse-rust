// Invite Blocking Storage - MSC4380
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

    /// Set the invite blocklist for a room (users that cannot be invited).
    ///
    /// The clear-then-insert pair runs in one transaction: a reader can never
    /// observe the empty window between them, which would otherwise read as
    /// "no restriction" and let a blocked invitee through.
    pub async fn set_invite_blocklist(&self, room_id: &str, user_ids: Vec<String>) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        let mut tx = self.pool.begin().await?;

        sqlx::query("DELETE FROM room_invite_blocklist WHERE room_id = $1").bind(room_id).execute(&mut *tx).await?;

        if !user_ids.is_empty() {
            sqlx::query(
                r"
                INSERT INTO room_invite_blocklist (room_id, user_id, created_ts)
                SELECT $1, unnest($2::text[]), $3
                ON CONFLICT DO NOTHING
                ",
            )
            .bind(room_id)
            .bind(&user_ids)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Get the invite blocklist for a room
    pub async fn get_invite_blocklist(&self, room_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String,)>(
            r"
            SELECT user_id FROM room_invite_blocklist WHERE room_id = $1
            ",
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    /// Evaluate both room lists for one invitee in a single round-trip.
    pub async fn evaluate(&self, room_id: &str, user_id: &str) -> Result<InviteRestriction, sqlx::Error> {
        let (blocked, allowed, allowlist_set) = sqlx::query_as::<_, (bool, bool, bool)>(
            r"
            SELECT
                EXISTS (SELECT 1 FROM room_invite_blocklist b WHERE b.room_id = $1 AND b.user_id = $2),
                EXISTS (SELECT 1 FROM room_invite_allowlist a WHERE a.room_id = $1 AND a.user_id = $2),
                EXISTS (SELECT 1 FROM room_invite_allowlist w WHERE w.room_id = $1)
            ",
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_one(&*self.pool)
        .await?;

        Ok(InviteRestriction { blocked, allowlist_set, allowed })
    }

    /// Set the invite allowlist for a room (only these users can be invited).
    ///
    /// Runs in one transaction for the same reason as
    /// [`Self::set_invite_blocklist`].
    pub async fn set_invite_allowlist(&self, room_id: &str, user_ids: Vec<String>) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        let mut tx = self.pool.begin().await?;

        sqlx::query("DELETE FROM room_invite_allowlist WHERE room_id = $1").bind(room_id).execute(&mut *tx).await?;

        if !user_ids.is_empty() {
            sqlx::query(
                r"
                INSERT INTO room_invite_allowlist (room_id, user_id, created_ts)
                SELECT $1, unnest($2::text[]), $3
                ON CONFLICT DO NOTHING
                ",
            )
            .bind(room_id)
            .bind(&user_ids)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Get the invite allowlist for a room
    pub async fn get_invite_allowlist(&self, room_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String,)>(
            r"
            SELECT user_id FROM room_invite_allowlist WHERE room_id = $1
            ",
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    /// Get global invite blocklist (all rooms)
    pub async fn get_global_invite_blocklist(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String, i64)>(
            r"
            SELECT room_id, user_id, created_ts FROM room_invite_blocklist
            ORDER BY created_ts DESC, room_id ASC, user_id ASC
            ",
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(room_id, user_id, created_ts)| {
                serde_json::json!({
                    "room_id": room_id,
                    "user_id": user_id,
                    "created_ts": created_ts
                })
            })
            .collect())
    }

    /// Get global invite allowlist (all rooms)
    pub async fn get_global_invite_allowlist(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String, i64)>(
            r"
            SELECT room_id, user_id, created_ts FROM room_invite_allowlist
            ORDER BY created_ts DESC, room_id ASC, user_id ASC
            ",
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(room_id, user_id, created_ts)| {
                serde_json::json!({
                    "room_id": room_id,
                    "user_id": user_id,
                    "created_ts": created_ts
                })
            })
            .collect())
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

    /// Cleanup allowlist rows by suffix pattern (for test isolation).
    async fn cleanup_allowlist_by_suffix(pool: &PgPool, suffix: &uuid::Uuid) {
        let pattern = format!("%{suffix}%");
        sqlx::query("DELETE FROM room_invite_allowlist WHERE room_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await
            .expect(
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
        let room_a = format!("!room_ga_{suffix}:test.com");
        let room_b = format!("!room_gb_{suffix}:test.com");
        let user_a = format!("@global_user_a_{suffix}:test.com");
        let user_b = format!("@global_user_b_{suffix}:test.com");

        cleanup_blocklist(&pool, &room_a).await;
        cleanup_blocklist(&pool, &room_b).await;
        ensure_test_room(&pool, &room_a).await;
        ensure_test_room(&pool, &room_b).await;

        storage
            .set_invite_blocklist(&room_a, vec![user_a.clone()])
            .await
            .expect("set blocklist for room_a should succeed");
        storage
            .set_invite_blocklist(&room_b, vec![user_b.clone()])
            .await
            .expect("set blocklist for room_b should succeed");

        let global = storage.get_global_invite_blocklist().await.expect("get_global_invite_blocklist should succeed");

        assert!(global.len() >= 2, "global blocklist should have at least 2 entries across 2 rooms");

        let room_ids: Vec<&str> = global.iter().map(|v| v["room_id"].as_str().unwrap()).collect();
        assert!(room_ids.contains(&room_a.as_str()), "global should contain room_a");
        assert!(room_ids.contains(&room_b.as_str()), "global should contain room_b");

        let user_ids: Vec<&str> = global.iter().map(|v| v["user_id"].as_str().unwrap()).collect();
        assert!(user_ids.contains(&user_a.as_str()), "global should contain user_a");
        assert!(user_ids.contains(&user_b.as_str()), "global should contain user_b");

        cleanup_blocklist(&pool, &room_a).await;
        cleanup_blocklist(&pool, &room_b).await;
    }

    #[tokio::test]
    async fn test_get_global_invite_allowlist() {
        // IsolatedTestPool: each test gets a fresh schema, so parallel tests
        // can't add rows to our isolated room_invite_allowlist. This restores
        // the exact `== 2` assertion from the original design.
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
        let pool = isolated.pool();
        let storage = InviteBlocklistStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let room_a = format!("!room_gal_a_{suffix}:test.com");
        let room_b = format!("!room_gal_b_{suffix}:test.com");
        let user_a = format!("@global_al_a_{suffix}:test.com");
        let user_b = format!("@global_al_b_{suffix}:test.com");

        // Clean up by suffix pattern to ensure test isolation
        cleanup_allowlist_by_suffix(&pool, &suffix).await;
        ensure_test_room(&pool, &room_a).await;
        ensure_test_room(&pool, &room_b).await;

        storage
            .set_invite_allowlist(&room_a, vec![user_a.clone()])
            .await
            .expect("set allowlist for room_a should succeed");
        storage
            .set_invite_allowlist(&room_b, vec![user_b.clone()])
            .await
            .expect("set allowlist for room_b should succeed");

        let global = storage.get_global_invite_allowlist().await.expect("get_global_invite_allowlist should succeed");

        // Isolated schema: only our 2 rows exist, so exact count is stable.
        assert_eq!(
            global.len(),
            2,
            "global allowlist should have exactly 2 entries in isolated schema, got {}",
            global.len()
        );

        let room_ids: Vec<&str> = global.iter().map(|v| v["room_id"].as_str().unwrap()).collect();
        assert!(room_ids.contains(&room_a.as_str()));
        assert!(room_ids.contains(&room_b.as_str()));

        let user_ids: Vec<&str> = global.iter().map(|v| v["user_id"].as_str().unwrap()).collect();
        assert!(user_ids.contains(&user_a.as_str()));
        assert!(user_ids.contains(&user_b.as_str()));

        // Clean up by suffix pattern to ensure test isolation
        cleanup_allowlist_by_suffix(&pool, &suffix).await;
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
}
