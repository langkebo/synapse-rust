use serde_json::Value;

pub struct PushStorage;

impl PushStorage {
    // ── pushers ──────────────────────────────────────────────────────────

    pub async fn get_pushers(
        pool: &sqlx::PgPool,
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
        .fetch_all(pool)
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_pusher(
        pool: &sqlx::PgPool,
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
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn delete_pusher(
        pool: &sqlx::PgPool,
        user_id: &str,
        device_id: &str,
        pushkey: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM pushers WHERE user_id = $1 AND pushkey = $2 AND device_id = $3")
            .bind(user_id)
            .bind(pushkey)
            .bind(device_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    // ── push_rules ───────────────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_push_rule(
        pool: &sqlx::PgPool,
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
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn delete_push_rule(
        pool: &sqlx::PgPool,
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
                .execute(pool)
                .await?;
        Ok(result.rows_affected())
    }

    pub async fn update_push_rule_actions(
        pool: &sqlx::PgPool,
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
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn get_push_rule_enabled(
        pool: &sqlx::PgPool,
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
        .fetch_optional(pool)
        .await
    }

    pub async fn set_push_rule_enabled(
        pool: &sqlx::PgPool,
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
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn get_user_push_rules(
        pool: &sqlx::PgPool,
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
        .fetch_all(pool)
        .await
    }

    // ── notifications ────────────────────────────────────────────────────

    pub async fn get_notifications(
        pool: &sqlx::PgPool,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error> {
        sqlx::query(
            "SELECT id, event_id, room_id, ts, notification_type, is_read \
             FROM notifications WHERE user_id = $1 ORDER BY ts DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(pool)
        .await
    }

    pub async fn ack_notification(
        pool: &sqlx::PgPool,
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
        .fetch_optional(pool)
        .await
    }
}
