#![cfg(test)]

#[path = "../common/mod.rs"]
mod common;

use sqlx::Row;
use std::sync::Arc;

async fn connect_pool() -> Option<Arc<sqlx::PgPool>> {
    match common::get_test_pool_async().await {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!(
                "Skipping account data schema contract integration tests because test database is unavailable: {}",
                error
            );
            None
        }
    }
}

async fn has_unique_constraint_on(pool: &sqlx::PgPool, table_name: &str, columns: &[&str]) -> bool {
    let rows = sqlx::query(
        r#"
        SELECT tc.constraint_name, kcu.column_name, kcu.ordinal_position
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
          ON tc.constraint_name = kcu.constraint_name
         AND tc.table_schema = kcu.table_schema
        WHERE tc.table_schema = 'public'
          AND tc.table_name = $1
          AND tc.constraint_type = 'UNIQUE'
        ORDER BY tc.constraint_name, kcu.ordinal_position
        "#,
    )
    .bind(table_name)
    .fetch_all(pool)
    .await
    .expect("Failed to query unique constraints");

    let mut current_name: Option<String> = None;
    let mut current_columns: Vec<String> = Vec::new();
    for row in rows {
        let name = row.get::<String, _>("constraint_name");
        let column = row.get::<String, _>("column_name");
        if current_name.as_deref() != Some(name.as_str()) {
            if current_columns == columns.iter().map(|c| (*c).to_string()).collect::<Vec<_>>() {
                return true;
            }
            current_name = Some(name);
            current_columns.clear();
        }
        current_columns.push(column);
    }

    current_columns == columns.iter().map(|c| (*c).to_string()).collect::<Vec<_>>()
}

async fn has_index_named(pool: &sqlx::PgPool, index_name: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM pg_indexes
            WHERE schemaname = 'public' AND indexname = $1
        )
        "#,
    )
    .bind(index_name)
    .fetch_one(pool)
    .await
    .expect("Failed to query pg_indexes")
}

async fn has_column(pool: &sqlx::PgPool, table_name: &str, column_name: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = $1 AND column_name = $2
        )
        "#,
    )
    .bind(table_name)
    .bind(column_name)
    .fetch_one(pool)
    .await
    .expect("Failed to query information_schema.columns")
}

async fn assert_column(
    pool: &sqlx::PgPool,
    table_name: &str,
    column_name: &str,
    expected_types: &[&str],
    expected_nullable: bool,
    expected_default_contains: Option<&str>,
) {
    let row = sqlx::query(
        r#"
        SELECT data_type, is_nullable, column_default
        FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = $1 AND column_name = $2
        "#,
    )
    .bind(table_name)
    .bind(column_name)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|_| panic!("Expected column {}.{} to exist", table_name, column_name));

    let data_type = row.get::<String, _>("data_type");
    assert!(
        expected_types
            .iter()
            .any(|ty| data_type.eq_ignore_ascii_case(ty)),
        "Expected {}.{} type in {:?}, got {}",
        table_name,
        column_name,
        expected_types,
        data_type
    );

    let is_nullable = row.get::<String, _>("is_nullable");
    assert_eq!(
        is_nullable.eq_ignore_ascii_case("YES"),
        expected_nullable,
        "Unexpected nullable flag for {}.{}",
        table_name,
        column_name
    );

    if let Some(expected_default_fragment) = expected_default_contains {
        let column_default = row
            .get::<Option<String>, _>("column_default")
            .unwrap_or_default();
        assert!(
            column_default.contains(expected_default_fragment),
            "Expected {}.{} default to contain {:?}, got {:?}",
            table_name,
            column_name,
            expected_default_fragment,
            column_default
        );
    }
}

#[tokio::test]
async fn test_schema_contract_account_data_tables_shape() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    for column in [
        "user_id",
        "data_type",
        "content",
        "created_ts",
        "updated_ts",
    ] {
        assert_column(
            &pool,
            "account_data",
            column,
            match column {
                "content" => &["jsonb"],
                "created_ts" | "updated_ts" => &["bigint"],
                _ => &["text", "character varying"],
            },
            false,
            None,
        )
        .await;
    }

    for column in ["user_id", "room_id", "data_type"] {
        assert_column(
            &pool,
            "room_account_data",
            column,
            &["text", "character varying"],
            false,
            None,
        )
        .await;
    }
    assert_column(&pool, "room_account_data", "data", &["jsonb"], false, None).await;

    assert!(
        has_unique_constraint_on(&pool, "account_data", &["user_id", "data_type"]).await,
        "Expected account_data UNIQUE(user_id,data_type)"
    );
    assert!(
        has_unique_constraint_on(
            &pool,
            "room_account_data",
            &["user_id", "room_id", "data_type"]
        )
        .await,
        "Expected room_account_data UNIQUE(user_id,room_id,data_type)"
    );
    assert!(
        has_index_named(&pool, "idx_account_data_user").await,
        "Expected account_data index idx_account_data_user"
    );
}

#[tokio::test]
async fn test_schema_contract_account_data_query_and_write_read_closure() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let user_id = format!("@schema-account-target-{}:localhost", uuid::Uuid::new_v4());
    let room_id = format!("!schema-account-room-{}:localhost", uuid::Uuid::new_v4());
    let account_type = format!("com.example.account.{}", uuid::Uuid::new_v4());
    let original_content = serde_json::json!({"theme": "dark", "layout": "compact"});
    let updated_content = serde_json::json!({"theme": "light", "layout": "expanded"});
    let original_room_data = serde_json::json!({"tags": {"m.favourite": {"order": 0.1}}});
    let updated_room_data = serde_json::json!({"tags": {"m.lowpriority": {"order": 0.9}}});
    let created_ts = chrono::Utc::now().timestamp_millis();
    let updated_ts = created_ts + 2000;

    sqlx::query(
        r#"
        INSERT INTO account_data (user_id, data_type, content, created_ts, updated_ts)
        VALUES ($1, $2, $3, $4, $4)
        ON CONFLICT (user_id, data_type) DO UPDATE
        SET content = EXCLUDED.content, updated_ts = EXCLUDED.updated_ts
        "#,
    )
    .bind(&user_id)
    .bind(&account_type)
    .bind(&original_content)
    .bind(created_ts)
    .execute(&*pool)
    .await
    .expect("Failed to insert account_data fixture");

    sqlx::query(
        r#"
        INSERT INTO room_account_data (user_id, room_id, data_type, data, created_ts, updated_ts)
        VALUES ($1, $2, 'm.tag', $3, $4, $4)
        ON CONFLICT (user_id, room_id, data_type) DO UPDATE
        SET data = EXCLUDED.data, updated_ts = EXCLUDED.updated_ts
        "#,
    )
    .bind(&user_id)
    .bind(&room_id)
    .bind(&original_room_data)
    .bind(created_ts)
    .execute(&*pool)
    .await
    .expect("Failed to insert room_account_data fixture");

    let account_row = sqlx::query(
        "SELECT content, created_ts, updated_ts FROM account_data WHERE user_id = $1 AND data_type = $2",
    )
    .bind(&user_id)
    .bind(&account_type)
    .fetch_one(&*pool)
    .await
    .expect("Failed to query inserted account_data");
    assert_eq!(
        account_row.get::<Option<serde_json::Value>, _>("content"),
        Some(original_content.clone())
    );
    assert_eq!(account_row.get::<i64, _>("created_ts"), created_ts);

    let room_row = sqlx::query(
        "SELECT data, created_ts, updated_ts FROM room_account_data WHERE user_id = $1 AND room_id = $2 AND data_type = 'm.tag'",
    )
    .bind(&user_id)
    .bind(&room_id)
    .fetch_one(&*pool)
    .await
    .expect("Failed to query inserted room_account_data");
    assert_eq!(
        room_row.get::<Option<serde_json::Value>, _>("data"),
        Some(original_room_data.clone())
    );

    sqlx::query(
        r#"
        INSERT INTO account_data (user_id, data_type, content, created_ts, updated_ts)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (user_id, data_type) DO UPDATE
        SET content = EXCLUDED.content, updated_ts = EXCLUDED.updated_ts
        "#,
    )
    .bind(&user_id)
    .bind(&account_type)
    .bind(&updated_content)
    .bind(created_ts)
    .bind(updated_ts)
    .execute(&*pool)
    .await
    .expect("Failed to upsert account_data fixture");

    sqlx::query(
        r#"
        INSERT INTO room_account_data (user_id, room_id, data_type, data, created_ts, updated_ts)
        VALUES ($1, $2, 'm.tag', $3, $4, $5)
        ON CONFLICT (user_id, room_id, data_type) DO UPDATE
        SET data = EXCLUDED.data, updated_ts = EXCLUDED.updated_ts
        "#,
    )
    .bind(&user_id)
    .bind(&room_id)
    .bind(&updated_room_data)
    .bind(created_ts)
    .bind(updated_ts)
    .execute(&*pool)
    .await
    .expect("Failed to upsert room_account_data fixture");

    let updated_account = sqlx::query(
        "SELECT content, updated_ts FROM account_data WHERE user_id = $1 AND data_type = $2",
    )
    .bind(&user_id)
    .bind(&account_type)
    .fetch_one(&*pool)
    .await
    .expect("Failed to query updated account_data");
    assert_eq!(
        updated_account.get::<Option<serde_json::Value>, _>("content"),
        Some(updated_content)
    );
    assert_eq!(updated_account.get::<i64, _>("updated_ts"), updated_ts);

    let updated_room = sqlx::query(
        "SELECT data, updated_ts FROM room_account_data WHERE user_id = $1 AND room_id = $2 AND data_type = 'm.tag'",
    )
    .bind(&user_id)
    .bind(&room_id)
    .fetch_one(&*pool)
    .await
    .expect("Failed to query updated room_account_data");
    assert_eq!(
        updated_room.get::<Option<serde_json::Value>, _>("data"),
        Some(updated_room_data)
    );
    assert_eq!(updated_room.get::<i64, _>("updated_ts"), updated_ts);

    sqlx::query("DELETE FROM room_account_data WHERE user_id = $1 AND room_id = $2")
        .bind(&user_id)
        .bind(&room_id)
        .execute(&*pool)
        .await
        .expect("Failed to cleanup room_account_data fixture");
    sqlx::query("DELETE FROM account_data WHERE user_id = $1")
        .bind(&user_id)
        .execute(&*pool)
        .await
        .expect("Failed to cleanup account_data fixture");
}

#[tokio::test]
async fn test_schema_contract_push_rules_shape() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    for column in ["user_id", "scope", "rule_id", "kind"] {
        assert_column(
            &pool,
            "push_rules",
            column,
            &["text", "character varying"],
            false,
            None,
        )
        .await;
    }
    assert_column(
        &pool,
        "push_rules",
        "priority_class",
        &["integer"],
        false,
        None,
    )
    .await;
    assert_column(
        &pool,
        "push_rules",
        "priority",
        &["integer"],
        true,
        Some("0"),
    )
    .await;
    assert_column(
        &pool,
        "push_rules",
        "conditions",
        &["jsonb"],
        true,
        Some("[]"),
    )
    .await;
    assert_column(&pool, "push_rules", "actions", &["jsonb"], true, Some("[]")).await;
    assert_column(&pool, "push_rules", "created_ts", &["bigint"], false, None).await;
    if has_column(&pool, "push_rules", "is_enabled").await {
        assert_column(
            &pool,
            "push_rules",
            "is_enabled",
            &["boolean"],
            true,
            Some("true"),
        )
        .await;
    }

    assert!(
        has_unique_constraint_on(
            &pool,
            "push_rules",
            &["user_id", "scope", "kind", "rule_id"]
        )
        .await,
        "Expected push_rules UNIQUE(user_id,scope,kind,rule_id)"
    );
    assert!(
        has_index_named(&pool, "idx_push_rules_user").await,
        "Expected push_rules index idx_push_rules_user"
    );
    assert!(
        has_index_named(&pool, "idx_push_rules_user_priority").await,
        "Expected push_rules index idx_push_rules_user_priority"
    );
}

#[tokio::test]
async fn test_schema_contract_push_rules_query_and_write_read_closure() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let user_id = format!("@schema-push-target-{}:localhost", uuid::Uuid::new_v4());
    let scope = "global";
    let kind = "override";
    let rule_id = format!("rule.{}", uuid::Uuid::new_v4());
    let actions = serde_json::json!(["notify", {"set_tweak": "sound", "value": "default"}]);
    let conditions = serde_json::json!([{
        "kind": "event_match",
        "key": "content.body",
        "pattern": "schema-contract"
    }]);
    let created_ts = chrono::Utc::now().timestamp_millis();

    sqlx::query(
        r#"
        INSERT INTO push_rules (
            user_id, scope, kind, rule_id, pattern, conditions, actions,
            is_enabled, is_default, priority_class, priority, created_ts
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, TRUE, FALSE, 5, 42, $8)
        ON CONFLICT (user_id, scope, kind, rule_id) DO UPDATE SET
            pattern = EXCLUDED.pattern,
            conditions = EXCLUDED.conditions,
            actions = EXCLUDED.actions,
            priority = EXCLUDED.priority
        "#,
    )
    .bind(&user_id)
    .bind(scope)
    .bind(kind)
    .bind(&rule_id)
    .bind(Some("schema-contract"))
    .bind(&conditions)
    .bind(&actions)
    .bind(created_ts)
    .execute(&*pool)
    .await
    .expect("Failed to insert push rule fixture");

    let rows = sqlx::query(
        r#"
        SELECT rule_id, conditions, actions, is_enabled
        FROM push_rules
        WHERE user_id = $1 AND scope = $2 AND kind = $3
        ORDER BY priority DESC, created_ts ASC
        "#,
    )
    .bind(&user_id)
    .bind(scope)
    .bind(kind)
    .fetch_all(&*pool)
    .await
    .expect("Failed to query push rules contract");

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get::<Option<String>, _>("rule_id").as_deref(),
        Some(rule_id.as_str())
    );
    assert_eq!(
        rows[0].get::<Option<serde_json::Value>, _>("conditions"),
        Some(conditions.clone())
    );
    assert_eq!(
        rows[0].get::<Option<serde_json::Value>, _>("actions"),
        Some(actions.clone())
    );
    assert_eq!(rows[0].get::<Option<bool>, _>("is_enabled"), Some(true));

    let toggled_actions = serde_json::json!(["dont_notify"]);
    sqlx::query(
        "UPDATE push_rules SET actions = $5, is_enabled = $4 WHERE user_id = $1 AND scope = $2 AND kind = $3 AND rule_id = $6",
    )
    .bind(&user_id)
    .bind(scope)
    .bind(kind)
    .bind(false)
    .bind(&toggled_actions)
    .bind(&rule_id)
    .execute(&*pool)
    .await
    .expect("Failed to update push rule fixture");

    let updated_row = sqlx::query(
        "SELECT is_enabled, actions FROM push_rules WHERE user_id = $1 AND scope = $2 AND kind = $3 AND rule_id = $4",
    )
    .bind(&user_id)
    .bind(scope)
    .bind(kind)
    .bind(&rule_id)
    .fetch_one(&*pool)
    .await
    .expect("Failed to fetch updated push rule");

    assert_eq!(
        updated_row.get::<Option<bool>, _>("is_enabled"),
        Some(false)
    );
    assert_eq!(
        updated_row.get::<Option<serde_json::Value>, _>("actions"),
        Some(toggled_actions)
    );

    sqlx::query(
        "DELETE FROM push_rules WHERE user_id = $1 AND scope = $2 AND kind = $3 AND rule_id = $4",
    )
    .bind(&user_id)
    .bind(scope)
    .bind(kind)
    .bind(&rule_id)
    .execute(&*pool)
    .await
    .expect("Failed to cleanup push rule fixture");
}
