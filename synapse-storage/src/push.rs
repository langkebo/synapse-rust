use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

/// Trait abstraction over [`PushStorage`] for testability and service wiring.
#[async_trait]
pub trait PushStoreApi: Send + Sync {
    async fn get_pushers(
        &self,
        user_id: &str,
        device_id: Option<&str>,
    ) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error>;

    #[allow(clippy::too_many_arguments)]
    async fn upsert_pusher(
        &self,
        user_id: &str,
        device_id: &str,
        pushkey: &str,
        kind: &str,
        app_id: &str,
        app_display_name: &str,
        device_display_name: &str,
        profile_tag: &Option<String>,
        lang: &str,
        data: &Option<Value>,
        now: i64,
    ) -> Result<(), sqlx::Error>;

    async fn delete_pusher(&self, user_id: &str, device_id: &str, pushkey: &str) -> Result<(), sqlx::Error>;

    #[allow(clippy::too_many_arguments)]
    async fn upsert_push_rule(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        pattern: &Option<String>,
        conditions: &Option<Value>,
        actions: &Value,
        now: i64,
    ) -> Result<(), sqlx::Error>;

    async fn delete_push_rule(&self, user_id: &str, scope: &str, kind: &str, rule_id: &str)
        -> Result<u64, sqlx::Error>;

    async fn update_push_rule_actions(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        actions: &Value,
    ) -> Result<(), sqlx::Error>;

    async fn get_push_rule_enabled(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
    ) -> Result<Option<bool>, sqlx::Error>;

    async fn set_push_rule_enabled(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        enabled: bool,
    ) -> Result<(), sqlx::Error>;

    async fn get_user_push_rules(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
    ) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error>;

    async fn get_notifications(&self, user_id: &str, limit: i64) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error>;

    async fn ack_notification(
        &self,
        id: i64,
        user_id: &str,
        now: i64,
    ) -> Result<Option<sqlx::postgres::PgRow>, sqlx::Error>;
}

#[derive(Clone)]
pub struct PushStorage {
    pool: Arc<sqlx::PgPool>,
}

impl PushStorage {
    pub fn new(pool: Arc<sqlx::PgPool>) -> Self {
        Self { pool }
    }

    // ── pushers ──────────────────────────────────────────────────────────

    pub async fn get_pushers(
        &self,
        user_id: &str,
        device_id: Option<&str>,
    ) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error> {
        sqlx::query(
            "SELECT pushkey, kind, app_id, app_display_name, device_display_name, \
             profile_tag, lang, data, device_id \
             FROM pushers WHERE user_id = $1 AND device_id IS NOT DISTINCT FROM $2 ORDER BY created_ts DESC",
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_all(&*self.pool)
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_pusher(
        &self,
        user_id: &str,
        device_id: &str,
        pushkey: &str,
        kind: &str,
        app_id: &str,
        app_display_name: &str,
        device_display_name: &str,
        profile_tag: &Option<String>,
        lang: &str,
        data: &Option<Value>,
        now: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO pushers (user_id, device_id, pushkey, pushkey_ts, kind, app_id, app_display_name, \
             device_display_name, profile_tag, lang, data, created_ts, updated_ts) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) \
             ON CONFLICT (user_id, device_id, pushkey) DO UPDATE SET \
             pushkey_ts = $4, kind = $5, app_id = $6, app_display_name = $7, \
             device_display_name = $8, profile_tag = $9, lang = $10, data = $11, updated_ts = $13",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(pushkey)
        .bind(now)
        .bind(kind)
        .bind(app_id)
        .bind(app_display_name)
        .bind(device_display_name)
        .bind(profile_tag)
        .bind(lang)
        .bind(data)
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_pusher(&self, user_id: &str, device_id: &str, pushkey: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM pushers WHERE user_id = $1 AND pushkey = $2 AND device_id = $3")
            .bind(user_id)
            .bind(pushkey)
            .bind(device_id)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    // ── push_rules ───────────────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_push_rule(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        pattern: &Option<String>,
        conditions: &Option<Value>,
        actions: &Value,
        now: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO push_rules (user_id, scope, kind, rule_id, pattern, conditions, actions, \
             is_enabled, is_default, priority_class, created_ts) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, true, false, 5, $8) \
             ON CONFLICT (user_id, scope, kind, rule_id) DO UPDATE SET \
             pattern = $5, conditions = $6, actions = $7",
        )
        .bind(user_id)
        .bind(scope)
        .bind(kind)
        .bind(rule_id)
        .bind(pattern)
        .bind(conditions)
        .bind(actions)
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_push_rule(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
    ) -> Result<u64, sqlx::Error> {
        let result =
            sqlx::query("DELETE FROM push_rules WHERE user_id = $1 AND scope = $2 AND kind = $3 AND rule_id = $4")
                .bind(user_id)
                .bind(scope)
                .bind(kind)
                .bind(rule_id)
                .execute(&*self.pool)
                .await?;
        Ok(result.rows_affected())
    }

    pub async fn update_push_rule_actions(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        actions: &Value,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE push_rules SET actions = $4 WHERE user_id = $1 AND scope = $2 AND kind = $3 AND rule_id = $5",
        )
        .bind(user_id)
        .bind(scope)
        .bind(kind)
        .bind(actions)
        .bind(rule_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_push_rule_enabled(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
    ) -> Result<Option<bool>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT is_enabled FROM push_rules WHERE user_id = $1 AND scope = $2 AND kind = $3 AND rule_id = $4",
        )
        .bind(user_id)
        .bind(scope)
        .bind(kind)
        .bind(rule_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn set_push_rule_enabled(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        enabled: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE push_rules SET is_enabled = $4 WHERE user_id = $1 AND scope = $2 AND kind = $3 AND rule_id = $5",
        )
        .bind(user_id)
        .bind(scope)
        .bind(kind)
        .bind(enabled)
        .bind(rule_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_user_push_rules(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
    ) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error> {
        sqlx::query(
            "SELECT rule_id, pattern, conditions, actions, is_enabled, is_default \
             FROM push_rules \
             WHERE user_id = $1 AND scope = $2 AND kind = $3 \
             ORDER BY priority DESC, created_ts ASC",
        )
        .bind(user_id)
        .bind(scope)
        .bind(kind)
        .fetch_all(&*self.pool)
        .await
    }

    // ── notifications ────────────────────────────────────────────────────

    pub async fn get_notifications(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error> {
        sqlx::query(
            "SELECT id, event_id, room_id, ts, notification_type, is_read \
             FROM notifications WHERE user_id = $1 ORDER BY ts DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn ack_notification(
        &self,
        id: i64,
        user_id: &str,
        now: i64,
    ) -> Result<Option<sqlx::postgres::PgRow>, sqlx::Error> {
        sqlx::query(
            "UPDATE notifications SET is_read = true, updated_ts = $3 \
             WHERE id = $1 AND user_id = $2 RETURNING id",
        )
        .bind(id)
        .bind(user_id)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await
    }
}

#[async_trait]
impl PushStoreApi for PushStorage {
    async fn get_pushers(
        &self,
        user_id: &str,
        device_id: Option<&str>,
    ) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error> {
        self.get_pushers(user_id, device_id).await
    }

    async fn upsert_pusher(
        &self,
        user_id: &str,
        device_id: &str,
        pushkey: &str,
        kind: &str,
        app_id: &str,
        app_display_name: &str,
        device_display_name: &str,
        profile_tag: &Option<String>,
        lang: &str,
        data: &Option<Value>,
        now: i64,
    ) -> Result<(), sqlx::Error> {
        self.upsert_pusher(
            user_id,
            device_id,
            pushkey,
            kind,
            app_id,
            app_display_name,
            device_display_name,
            profile_tag,
            lang,
            data,
            now,
        )
        .await
    }

    async fn delete_pusher(&self, user_id: &str, device_id: &str, pushkey: &str) -> Result<(), sqlx::Error> {
        self.delete_pusher(user_id, device_id, pushkey).await
    }

    async fn upsert_push_rule(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        pattern: &Option<String>,
        conditions: &Option<Value>,
        actions: &Value,
        now: i64,
    ) -> Result<(), sqlx::Error> {
        self.upsert_push_rule(user_id, scope, kind, rule_id, pattern, conditions, actions, now).await
    }

    async fn delete_push_rule(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
    ) -> Result<u64, sqlx::Error> {
        self.delete_push_rule(user_id, scope, kind, rule_id).await
    }

    async fn update_push_rule_actions(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        actions: &Value,
    ) -> Result<(), sqlx::Error> {
        self.update_push_rule_actions(user_id, scope, kind, rule_id, actions).await
    }

    async fn get_push_rule_enabled(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
    ) -> Result<Option<bool>, sqlx::Error> {
        self.get_push_rule_enabled(user_id, scope, kind, rule_id).await
    }

    async fn set_push_rule_enabled(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
        rule_id: &str,
        enabled: bool,
    ) -> Result<(), sqlx::Error> {
        self.set_push_rule_enabled(user_id, scope, kind, rule_id, enabled).await
    }

    async fn get_user_push_rules(
        &self,
        user_id: &str,
        scope: &str,
        kind: &str,
    ) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error> {
        self.get_user_push_rules(user_id, scope, kind).await
    }

    async fn get_notifications(&self, user_id: &str, limit: i64) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error> {
        self.get_notifications(user_id, limit).await
    }

    async fn ack_notification(
        &self,
        id: i64,
        user_id: &str,
        now: i64,
    ) -> Result<Option<sqlx::postgres::PgRow>, sqlx::Error> {
        self.ack_notification(id, user_id, now).await
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use serde_json::json;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::Row;
    use std::env;

    async fn test_pool() -> Arc<sqlx::PgPool> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    fn unique_user_id(prefix: &str) -> String {
        format!("{}_pushtest_{}:test.com", prefix, uuid::Uuid::new_v4())
    }

    // ── helpers ─────────────────────────────────────────────────────────

    async fn cleanup_pushers(pool: &sqlx::PgPool, user_id: &str) {
        let _ = sqlx::query("DELETE FROM pushers WHERE user_id = $1").bind(user_id).execute(pool).await;
    }

    async fn cleanup_push_rules(pool: &sqlx::PgPool, user_id: &str) {
        let _ = sqlx::query("DELETE FROM push_rules WHERE user_id = $1").bind(user_id).execute(pool).await;
    }

    async fn cleanup_notifications(pool: &sqlx::PgPool, user_id: &str) {
        let _ = sqlx::query("DELETE FROM notifications WHERE user_id = $1").bind(user_id).execute(pool).await;
    }

    async fn insert_notification(
        pool: &sqlx::PgPool,
        user_id: &str,
        event_id: &str,
        room_id: &str,
        ts: i64,
        notification_type: &str,
        is_read: bool,
        created_ts: i64,
    ) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO notifications (user_id, event_id, room_id, ts, notification_type, is_read, created_ts) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
        )
        .bind(user_id)
        .bind(event_id)
        .bind(room_id)
        .bind(ts)
        .bind(notification_type)
        .bind(is_read)
        .bind(created_ts)
        .fetch_one(pool)
        .await
        .expect("failed to insert test notification")
    }

    // ── pushers tests ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_upsert_and_get_pushers_with_device_id() {
        let pool = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let device_id = "device1";
        let pushkey = "pushkey1";
        let now = chrono::Utc::now().timestamp_millis();

        cleanup_pushers(&pool, &user_id).await;

        storage
            .upsert_pusher(
                &user_id,
                device_id,
                pushkey,
                "http",
                "app.id",
                "My App",
                "My Device",
                &None,
                "en",
                &None,
                now,
            )
            .await
            .expect("upsert_pusher should succeed");

        let rows = storage.get_pushers(&user_id, Some(device_id)).await.expect("get_pushers should succeed");
        assert!(!rows.is_empty(), "should return at least one pusher");

        let row = &rows[0];
        assert_eq!(row.get::<String, _>("pushkey"), "pushkey1");
        assert_eq!(row.get::<String, _>("device_id"), device_id);

        cleanup_pushers(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_get_pushers_filters_by_specific_device_id() {
        let pool = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");

        cleanup_pushers(&pool, &user_id).await;

        let now = chrono::Utc::now().timestamp_millis();
        storage
            .upsert_pusher(&user_id, "d1", "pk1", "http", "app.id", "App", "Dev", &None, "en", &None, now)
            .await
            .expect("upsert d1 should succeed");
        storage
            .upsert_pusher(&user_id, "d2", "pk2", "http", "app.id", "App", "Dev", &None, "en", &None, now)
            .await
            .expect("upsert d2 should succeed");

        // Filtering by a specific device_id should return only that pusher
        let rows_d1 =
            storage.get_pushers(&user_id, Some("d1")).await.expect("get_pushers with Some(d1) should succeed");
        assert_eq!(rows_d1.len(), 1, "should return exactly one pusher for d1");
        assert_eq!(rows_d1[0].get::<String, _>("device_id"), "d1");

        let rows_d2 =
            storage.get_pushers(&user_id, Some("d2")).await.expect("get_pushers with Some(d2) should succeed");
        assert_eq!(rows_d2.len(), 1, "should return exactly one pusher for d2");
        assert_eq!(rows_d2[0].get::<String, _>("device_id"), "d2");

        cleanup_pushers(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_get_pushers_with_none_device_id() {
        let pool = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");

        cleanup_pushers(&pool, &user_id).await;

        let now = chrono::Utc::now().timestamp_millis();
        storage
            .upsert_pusher(&user_id, "d1", "pk1", "http", "app.id", "App", "Dev", &None, "en", &None, now)
            .await
            .expect("upsert should succeed");

        // When device_id is None, it binds as SQL NULL, and
        // `device_id IS NOT DISTINCT FROM NULL` only matches rows
        // where device_id IS NULL. Since the column is NOT NULL,
        // this currently returns zero rows.
        let rows = storage.get_pushers(&user_id, None).await.expect("get_pushers with None should succeed");
        assert!(rows.is_empty(), "None device_id currently returns empty result set");

        cleanup_pushers(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_get_pushers_returns_empty_for_unknown_user() {
        let pool = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@unknown");

        let rows = storage.get_pushers(&user_id, None).await.expect("get_pushers should succeed");
        assert!(rows.is_empty(), "unknown user should have no pushers");
    }

    #[tokio::test]
    async fn test_upsert_pusher_updates_existing() {
        let pool = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let device_id = "dev_update";
        let pushkey = "pk_update";
        let now = chrono::Utc::now().timestamp_millis();

        cleanup_pushers(&pool, &user_id).await;

        // Insert initial
        storage
            .upsert_pusher(&user_id, device_id, pushkey, "http", "app.id", "App1", "Dev1", &None, "en", &None, now)
            .await
            .expect("first upsert should succeed");

        // Update with new app_display_name
        let now2 = now + 1000;
        storage
            .upsert_pusher(&user_id, device_id, pushkey, "http", "app.id", "App2", "Dev2", &None, "en", &None, now2)
            .await
            .expect("second upsert should succeed");

        let rows = storage.get_pushers(&user_id, Some(device_id)).await.expect("get_pushers should succeed");
        assert_eq!(rows.len(), 1, "should still have exactly one pusher after upsert");
        assert_eq!(rows[0].get::<String, _>("app_display_name"), "App2");

        cleanup_pushers(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_delete_pusher() {
        let pool = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let device_id = "dev_del";
        let pushkey = "pk_del";
        let now = chrono::Utc::now().timestamp_millis();

        cleanup_pushers(&pool, &user_id).await;

        storage
            .upsert_pusher(&user_id, device_id, pushkey, "http", "app.id", "App", "Dev", &None, "en", &None, now)
            .await
            .expect("upsert should succeed");

        storage.delete_pusher(&user_id, device_id, pushkey).await.expect("delete_pusher should succeed");

        let rows = storage.get_pushers(&user_id, Some(device_id)).await.expect("get_pushers should succeed");
        assert!(rows.is_empty(), "pusher should be deleted");

        cleanup_pushers(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_delete_pusher_nonexistent_does_not_error() {
        let pool = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");

        storage
            .delete_pusher(&user_id, "no_dev", "no_pk")
            .await
            .expect("delete_pusher on non-existent row should not error");
    }

    // ── push_rules tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_upsert_and_get_push_rule() {
        let pool = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let now = chrono::Utc::now().timestamp_millis();

        cleanup_push_rules(&pool, &user_id).await;

        storage
            .upsert_push_rule(&user_id, "global", "override", "rule1", &None, &None, &json!(["notify"]), now)
            .await
            .expect("upsert_push_rule should succeed");

        let rows = storage
            .get_user_push_rules(&user_id, "global", "override")
            .await
            .expect("get_user_push_rules should succeed");
        assert!(!rows.is_empty(), "should return at least one rule");

        let enabled = storage
            .get_push_rule_enabled(&user_id, "global", "override", "rule1")
            .await
            .expect("get_push_rule_enabled should succeed");
        assert_eq!(enabled, Some(true), "new rule should be enabled by default");

        cleanup_push_rules(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_get_user_push_rules_returns_empty_for_no_rules() {
        let pool = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");

        let rows = storage
            .get_user_push_rules(&user_id, "global", "override")
            .await
            .expect("get_user_push_rules should succeed");
        assert!(rows.is_empty(), "unknown user should have no push rules");
    }

    #[tokio::test]
    async fn test_upsert_push_rule_updates_existing() {
        let pool = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let now = chrono::Utc::now().timestamp_millis();

        cleanup_push_rules(&pool, &user_id).await;

        storage
            .upsert_push_rule(
                &user_id,
                "global",
                "override",
                "rule2",
                &Some("original".to_string()),
                &None,
                &json!(["notify"]),
                now,
            )
            .await
            .expect("first upsert should succeed");

        storage
            .upsert_push_rule(
                &user_id,
                "global",
                "override",
                "rule2",
                &Some("updated".to_string()),
                &None,
                &json!(["dont_notify"]),
                now,
            )
            .await
            .expect("second upsert should succeed");

        let rows = storage
            .get_user_push_rules(&user_id, "global", "override")
            .await
            .expect("get_user_push_rules should succeed");
        assert_eq!(rows.len(), 1, "should still have exactly one rule after upsert");
        assert_eq!(rows[0].get::<String, _>("pattern"), "updated");

        cleanup_push_rules(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_update_push_rule_actions() {
        let pool = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let now = chrono::Utc::now().timestamp_millis();

        cleanup_push_rules(&pool, &user_id).await;

        storage
            .upsert_push_rule(&user_id, "global", "override", "rule_actions", &None, &None, &json!(["notify"]), now)
            .await
            .expect("upsert should succeed");

        storage
            .update_push_rule_actions(
                &user_id,
                "global",
                "override",
                "rule_actions",
                &json!(["dont_notify", {"set_tweak": "highlight"}]),
            )
            .await
            .expect("update_push_rule_actions should succeed");

        let rows = storage
            .get_user_push_rules(&user_id, "global", "override")
            .await
            .expect("get_user_push_rules should succeed");
        assert!(!rows.is_empty());
        let actions: serde_json::Value = rows[0].get("actions");
        assert!(actions.as_array().map_or(false, |a| a.len() >= 2));

        cleanup_push_rules(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_delete_push_rule() {
        let pool = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let now = chrono::Utc::now().timestamp_millis();

        cleanup_push_rules(&pool, &user_id).await;

        storage
            .upsert_push_rule(&user_id, "global", "override", "rule_del", &None, &None, &json!(["notify"]), now)
            .await
            .expect("upsert should succeed");

        let affected = storage
            .delete_push_rule(&user_id, "global", "override", "rule_del")
            .await
            .expect("delete_push_rule should succeed");
        assert_eq!(affected, 1, "should delete exactly one row");

        let affected2 = storage
            .delete_push_rule(&user_id, "global", "override", "rule_del")
            .await
            .expect("second delete should succeed");
        assert_eq!(affected2, 0, "second delete should affect zero rows");

        cleanup_push_rules(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_get_push_rule_enabled_returns_none_for_nonexistent() {
        let pool = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");

        let enabled = storage
            .get_push_rule_enabled(&user_id, "global", "override", "no_such_rule")
            .await
            .expect("get_push_rule_enabled should succeed");
        assert_eq!(enabled, None, "non-existent rule should return None");
    }

    #[tokio::test]
    async fn test_set_push_rule_enabled() {
        let pool = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let now = chrono::Utc::now().timestamp_millis();

        cleanup_push_rules(&pool, &user_id).await;

        storage
            .upsert_push_rule(&user_id, "global", "override", "rule_toggle", &None, &None, &json!(["notify"]), now)
            .await
            .expect("upsert should succeed");

        storage
            .set_push_rule_enabled(&user_id, "global", "override", "rule_toggle", false)
            .await
            .expect("set_push_rule_enabled should succeed");

        let enabled = storage
            .get_push_rule_enabled(&user_id, "global", "override", "rule_toggle")
            .await
            .expect("get_push_rule_enabled should succeed");
        assert_eq!(enabled, Some(false), "rule should be disabled");

        storage
            .set_push_rule_enabled(&user_id, "global", "override", "rule_toggle", true)
            .await
            .expect("re-enable should succeed");

        let enabled = storage
            .get_push_rule_enabled(&user_id, "global", "override", "rule_toggle")
            .await
            .expect("get_push_rule_enabled should succeed");
        assert_eq!(enabled, Some(true), "rule should be re-enabled");

        cleanup_push_rules(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_get_user_push_rules_scoped_by_scope_and_kind() {
        let pool = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let now = chrono::Utc::now().timestamp_millis();

        cleanup_push_rules(&pool, &user_id).await;

        // Create rules in different scope/kind combinations
        storage
            .upsert_push_rule(&user_id, "global", "override", "r1", &None, &None, &json!(["notify"]), now)
            .await
            .expect("upsert r1");
        storage
            .upsert_push_rule(&user_id, "global", "content", "r2", &None, &None, &json!(["notify"]), now)
            .await
            .expect("upsert r2");
        storage
            .upsert_push_rule(&user_id, "devices/DEV1", "override", "r3", &None, &None, &json!(["notify"]), now)
            .await
            .expect("upsert r3");

        let global_override =
            storage.get_user_push_rules(&user_id, "global", "override").await.expect("get global override");
        assert_eq!(global_override.len(), 1, "should return only global/override rules");
        assert_eq!(global_override[0].get::<String, _>("rule_id"), "r1");

        let global_content =
            storage.get_user_push_rules(&user_id, "global", "content").await.expect("get global content");
        assert_eq!(global_content.len(), 1);
        assert_eq!(global_content[0].get::<String, _>("rule_id"), "r2");

        let device_override =
            storage.get_user_push_rules(&user_id, "devices/DEV1", "override").await.expect("get device override");
        assert_eq!(device_override.len(), 1);
        assert_eq!(device_override[0].get::<String, _>("rule_id"), "r3");

        cleanup_push_rules(&pool, &user_id).await;
    }

    // ── notifications tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_notifications_returns_rows() {
        let pool = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let now = chrono::Utc::now().timestamp_millis();

        cleanup_notifications(&pool, &user_id).await;

        insert_notification(&pool, &user_id, "$ev1", "!room1:test.com", now, "message", false, now).await;
        insert_notification(&pool, &user_id, "$ev2", "!room1:test.com", now + 1, "invite", false, now).await;

        let rows = storage.get_notifications(&user_id, 10).await.expect("get_notifications should succeed");
        assert_eq!(rows.len(), 2, "should return both notifications");
        // Results ordered by ts DESC, so newest (ev2) first
        assert_eq!(rows[0].get::<String, _>("event_id"), "$ev2");

        cleanup_notifications(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_get_notifications_respects_limit() {
        let pool = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let now = chrono::Utc::now().timestamp_millis();

        cleanup_notifications(&pool, &user_id).await;

        for i in 0..5 {
            insert_notification(&pool, &user_id, &format!("$ev{i}"), "!room1:test.com", now + i, "message", false, now)
                .await;
        }

        let rows = storage.get_notifications(&user_id, 3).await.expect("get_notifications should succeed");
        assert_eq!(rows.len(), 3, "should respect limit");

        cleanup_notifications(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_get_notifications_empty_for_unknown_user() {
        let pool = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@unknown");

        let rows = storage.get_notifications(&user_id, 10).await.expect("get_notifications should succeed");
        assert!(rows.is_empty(), "unknown user should have no notifications");
    }

    #[tokio::test]
    async fn test_ack_notification_marks_read_and_returns_id() {
        let pool = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let now = chrono::Utc::now().timestamp_millis();

        cleanup_notifications(&pool, &user_id).await;

        let nid = insert_notification(&pool, &user_id, "$ev_ack", "!room1:test.com", now, "message", false, now).await;

        let result =
            storage.ack_notification(nid, &user_id, now + 1000).await.expect("ack_notification should succeed");
        assert!(result.is_some(), "should return the acknowledged row");
        assert_eq!(result.unwrap().get::<i64, _>("id"), nid);

        cleanup_notifications(&pool, &user_id).await;
    }

    #[tokio::test]
    async fn test_ack_notification_returns_none_for_wrong_user() {
        let pool = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");
        let other_user = unique_user_id("@other");
        let now = chrono::Utc::now().timestamp_millis();

        cleanup_notifications(&pool, &user_id).await;
        cleanup_notifications(&pool, &other_user).await;

        let nid =
            insert_notification(&pool, &user_id, "$ev_wrong", "!room1:test.com", now, "message", false, now).await;

        let result =
            storage.ack_notification(nid, &other_user, now + 1000).await.expect("ack_notification should succeed");
        assert!(result.is_none(), "other user should not be able to ack this notification");

        cleanup_notifications(&pool, &user_id).await;
        cleanup_notifications(&pool, &other_user).await;
    }

    #[tokio::test]
    async fn test_ack_notification_returns_none_for_nonexistent_id() {
        let pool = test_pool().await;
        let storage = PushStorage::new(Arc::clone(&pool));
        let user_id = unique_user_id("@test");

        let result = storage
            .ack_notification(99999999, &user_id, chrono::Utc::now().timestamp_millis())
            .await
            .expect("ack_notification should succeed");
        assert!(result.is_none(), "non-existent notification id should return None");
    }
}
