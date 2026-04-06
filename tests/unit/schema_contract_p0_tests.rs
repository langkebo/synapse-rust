#![cfg(test)]
mod schema_contract_p0_suite {
    use crate::common::get_test_pool_async;
    use sqlx::Row;
    use std::sync::Arc;
    use synapse_rust::services::room_summary_service::RoomSummaryService;
    use synapse_rust::storage::event::{CreateEventParams, EventStorage};
    use synapse_rust::storage::room_summary::{
        CreateRoomSummaryRequest, CreateSummaryMemberRequest, RoomSummaryStorage,
        UpdateSummaryMemberRequest,
    };
    use synapse_rust::storage::space::{AddChildRequest, CreateSpaceRequest, SpaceStorage};

    async fn connect_pool() -> Option<Arc<sqlx::PgPool>> {
        match get_test_pool_async().await {
            Ok(pool) => Some(pool),
            Err(error) => {
                eprintln!(
                    "Skipping schema contract tests because test database is unavailable: {}",
                    error
                );
                None
            }
        }
    }

    async fn seed_space_users(pool: &sqlx::PgPool, suffix: &str) -> (String, String) {
        let creator = format!("@schema-space-creator-{suffix}:localhost");
        let member = format!("@schema-space-member-{suffix}:localhost");

        for (user_id, username) in [
            (&creator, format!("schema_space_creator_{suffix}")),
            (&member, format!("schema_space_member_{suffix}")),
        ] {
            sqlx::query(
                "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
            )
            .bind(user_id)
            .bind(username)
            .bind(0_i64)
            .execute(pool)
            .await
            .expect("Failed to seed space user fixture");
        }

        (creator, member)
    }

    async fn cleanup_space_fixtures(pool: &sqlx::PgPool, space_id: &str, user_ids: &[String]) {
        sqlx::query("DELETE FROM space_events WHERE space_id = $1")
            .bind(space_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup space_events");

        sqlx::query("DELETE FROM space_statistics WHERE space_id = $1")
            .bind(space_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup space_statistics");

        sqlx::query("DELETE FROM space_summaries WHERE space_id = $1")
            .bind(space_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup space_summaries");

        sqlx::query("DELETE FROM space_children WHERE space_id = $1")
            .bind(space_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup space_children");

        sqlx::query("DELETE FROM space_members WHERE space_id = $1")
            .bind(space_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup space_members");

        sqlx::query("DELETE FROM spaces WHERE space_id = $1")
            .bind(space_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup spaces");

        for user_id in user_ids {
            sqlx::query("DELETE FROM users WHERE user_id = $1")
                .bind(user_id)
                .execute(pool)
                .await
                .expect("Failed to cleanup space user fixture");
        }
    }

    async fn assert_table_exists(pool: &sqlx::PgPool, table_name: &str) {
        let regclass: Option<String> = sqlx::query_scalar("SELECT to_regclass($1)::text")
            .bind(format!("public.{table_name}"))
            .fetch_one(pool)
            .await
            .expect("Failed to query table existence");
        assert!(
            regclass.as_deref() == Some(table_name)
                || regclass.as_deref() == Some(format!("public.{table_name}").as_str()),
            "Expected table '{table_name}' to exist, got: {:?}",
            regclass
        );
    }

    async fn assert_column(
        pool: &sqlx::PgPool,
        table_name: &str,
        column_name: &str,
        expected_data_types: &[&str],
        expected_nullable: bool,
        expected_default_substring: Option<&str>,
        expected_char_max_len: Option<i32>,
    ) {
        let row: Option<(String, String, Option<String>, Option<i32>)> = sqlx::query_as(
            r#"
            SELECT data_type, is_nullable, column_default, character_maximum_length
            FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = $1 AND column_name = $2
            "#,
        )
        .bind(table_name)
        .bind(column_name)
        .fetch_optional(pool)
        .await
        .expect("Failed to query information_schema.columns");

        let (data_type, is_nullable, column_default, char_max_len) =
            row.unwrap_or_else(|| panic!("Missing column {table_name}.{column_name}"));

        assert_eq!(
            expected_data_types
                .iter()
                .any(|expected| data_type == *expected),
            true,
            "Unexpected type for {table_name}.{column_name}: {data_type}"
        );
        let is_nullable_bool = is_nullable == "YES";
        assert_eq!(
            is_nullable_bool, expected_nullable,
            "Unexpected nullability for {table_name}.{column_name}"
        );

        if let Some(expected) = expected_default_substring {
            let default = column_default.unwrap_or_default();
            assert!(
                default.contains(expected),
                "Unexpected default for {table_name}.{column_name}: {default}"
            );
        }

        if let Some(expected_len) = expected_char_max_len {
            assert_eq!(
                char_max_len,
                Some(expected_len),
                "Unexpected varchar length for {table_name}.{column_name}"
            );
        }
    }

    async fn has_column(pool: &sqlx::PgPool, table_name: &str, column_name: &str) -> bool {
        let exists: bool = sqlx::query_scalar(
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
        .expect("Failed to query column existence");
        exists
    }

    async fn primary_key_columns(pool: &sqlx::PgPool, table_name: &str) -> Vec<String> {
        let columns: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT kcu.column_name
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
              ON tc.constraint_name = kcu.constraint_name
             AND tc.table_schema = kcu.table_schema
            WHERE tc.table_schema = 'public'
              AND tc.table_name = $1
              AND tc.constraint_type = 'PRIMARY KEY'
            ORDER BY kcu.ordinal_position
            "#,
        )
        .bind(table_name)
        .fetch_all(pool)
        .await
        .expect("Failed to query primary key columns");
        columns
    }

    async fn has_unique_constraint_on(
        pool: &sqlx::PgPool,
        table_name: &str,
        columns: &[&str],
    ) -> bool {
        let rows: Vec<(String, Vec<String>)> = sqlx::query_as(
            r#"
            SELECT tc.constraint_name,
                   array_agg(kcu.column_name::text ORDER BY kcu.ordinal_position) AS cols
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
              ON tc.constraint_name = kcu.constraint_name
             AND tc.table_schema = kcu.table_schema
            WHERE tc.table_schema = 'public'
              AND tc.table_name = $1
              AND tc.constraint_type = 'UNIQUE'
            GROUP BY tc.constraint_name
            "#,
        )
        .bind(table_name)
        .fetch_all(pool)
        .await
        .expect("Failed to query unique constraints");

        rows.into_iter().any(|(_, cols)| {
            cols.len() == columns.len()
                && cols
                    .iter()
                    .zip(columns.iter())
                    .all(|(actual, expected)| actual == expected)
        })
    }

    async fn has_index_named(pool: &sqlx::PgPool, index_name: &str) -> bool {
        sqlx::query_scalar(
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

    async fn seed_users_and_room(pool: &sqlx::PgPool, suffix: &str) -> (String, String, String) {
        let creator = format!("@schema-summary-creator-{suffix}:localhost");
        let hero = format!("@schema-summary-hero-{suffix}:localhost");
        let room_id = format!("!schema-summary-room-{suffix}:localhost");

        for (user_id, username) in [
            (&creator, format!("schema_summary_creator_{suffix}")),
            (&hero, format!("schema_summary_hero_{suffix}")),
        ] {
            sqlx::query(
                "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
            )
            .bind(user_id)
            .bind(username)
            .bind(0_i64)
            .execute(pool)
            .await
            .expect("Failed to seed user fixture");
        }

        sqlx::query(
            "INSERT INTO rooms (room_id, creator, created_ts) VALUES ($1, $2, $3) ON CONFLICT (room_id) DO NOTHING",
        )
        .bind(&room_id)
        .bind(&creator)
        .bind(0_i64)
        .execute(pool)
        .await
        .expect("Failed to seed room fixture");

        (creator, hero, room_id)
    }

    async fn cleanup_room_summary_fixtures(
        pool: &sqlx::PgPool,
        room_id: &str,
        user_ids: &[String],
    ) {
        sqlx::query("DELETE FROM room_children WHERE parent_room_id = $1 OR child_room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room_children");

        sqlx::query("DELETE FROM room_summary_update_queue WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room_summary_update_queue");

        sqlx::query("DELETE FROM room_summary_stats WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room_summary_stats");

        sqlx::query("DELETE FROM room_summary_state WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room_summary_state");

        sqlx::query("DELETE FROM room_summary_members WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room_summary_members");

        sqlx::query("DELETE FROM room_summaries WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room_summaries");

        sqlx::query("DELETE FROM rooms WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room fixture");

        for user_id in user_ids {
            sqlx::query("DELETE FROM users WHERE user_id = $1")
                .bind(user_id)
                .execute(pool)
                .await
                .expect("Failed to cleanup user fixture");
        }
    }

    #[tokio::test]
    async fn test_schema_contract_p0_tables_exist() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        for table_name in [
            "room_memberships",
            "events",
            "account_data",
            "room_account_data",
            "push_rules",
            "room_retention_policies",
            "device_verification_request",
            "search_index",
        ] {
            assert_table_exists(&pool, table_name).await;
        }
    }

    #[tokio::test]
    async fn test_schema_contract_room_memberships_shape() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        assert_column(
            &pool,
            "room_memberships",
            "room_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_memberships",
            "user_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_memberships",
            "membership",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_memberships",
            "updated_ts",
            &["bigint"],
            true,
            None,
            None,
        )
        .await;

        let pk_cols = primary_key_columns(&pool, "room_memberships").await;
        let has_id = has_column(&pool, "room_memberships", "id").await;
        if has_id {
            assert_column(
                &pool,
                "room_memberships",
                "id",
                &["bigint"],
                false,
                Some("nextval("),
                None,
            )
            .await;
            assert!(
                pk_cols == vec!["id".to_string()] || has_unique_constraint_on(&pool, "room_memberships", &["room_id", "user_id"]).await,
                "Expected room_memberships to have PK(id) and UNIQUE(room_id,user_id), got PK={pk_cols:?}"
            );
        } else {
            assert!(
                pk_cols == vec!["room_id".to_string(), "user_id".to_string()],
                "Expected room_memberships PK(room_id,user_id) when id column is absent, got PK={pk_cols:?}"
            );
        }
    }

    #[tokio::test]
    async fn test_schema_contract_events_shape() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        assert_column(
            &pool,
            "events",
            "event_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "events",
            "room_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "events",
            "sender",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "events",
            "event_type",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(&pool, "events", "content", &["jsonb"], false, None, None).await;
        assert_column(
            &pool,
            "events",
            "origin_server_ts",
            &["bigint"],
            false,
            None,
            None,
        )
        .await;

        if has_column(&pool, "events", "is_redacted").await {
            assert_column(
                &pool,
                "events",
                "is_redacted",
                &["boolean"],
                true,
                Some("false"),
                None,
            )
            .await;
        }

        if has_column(&pool, "events", "unsigned").await {
            assert_column(&pool, "events", "unsigned", &["jsonb"], true, None, None).await;
        }
    }

    #[tokio::test]
    async fn test_schema_contract_account_data_shape() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        assert_column(
            &pool,
            "account_data",
            "user_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "account_data",
            "data_type",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "account_data",
            "content",
            &["jsonb"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "account_data",
            "created_ts",
            &["bigint"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "account_data",
            "updated_ts",
            &["bigint"],
            false,
            None,
            None,
        )
        .await;
        assert!(
            has_unique_constraint_on(&pool, "account_data", &["user_id", "data_type"]).await,
            "Expected account_data UNIQUE(user_id,data_type)"
        );
        assert!(
            has_index_named(&pool, "idx_account_data_user").await,
            "Expected account_data index idx_account_data_user"
        );

        assert_column(
            &pool,
            "room_account_data",
            "user_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_account_data",
            "room_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_account_data",
            "data_type",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_account_data",
            "data",
            &["jsonb"],
            false,
            None,
            None,
        )
        .await;
        assert!(
            has_unique_constraint_on(
                &pool,
                "room_account_data",
                &["user_id", "room_id", "data_type"]
            )
            .await,
            "Expected room_account_data UNIQUE(user_id,room_id,data_type)"
        );
    }

    #[tokio::test]
    async fn test_schema_contract_account_data_query_and_write_read_closure() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        let user_id = format!("@schema_account_{}:localhost", uuid::Uuid::new_v4());
        let data_type = format!("com.example.contract.{}", uuid::Uuid::new_v4());
        let original_content = serde_json::json!({
            "theme": "dark",
            "layout": "compact"
        });
        let updated_content = serde_json::json!({
            "theme": "light",
            "layout": "expanded"
        });
        let created_ts = chrono::Utc::now().timestamp_millis();
        let updated_ts = created_ts + 1000;

        sqlx::query(
            r#"
            INSERT INTO account_data (user_id, data_type, content, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $4)
            ON CONFLICT (user_id, data_type) DO UPDATE
            SET content = EXCLUDED.content, updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(&user_id)
        .bind(&data_type)
        .bind(&original_content)
        .bind(created_ts)
        .execute(&*pool)
        .await
        .expect("Failed to insert account_data fixture");

        let inserted_row = sqlx::query(
            "SELECT content, created_ts, updated_ts FROM account_data WHERE user_id = $1 AND data_type = $2",
        )
        .bind(&user_id)
        .bind(&data_type)
        .fetch_one(&*pool)
        .await
        .expect("Failed to query inserted account_data");

        assert_eq!(
            inserted_row.get::<Option<serde_json::Value>, _>("content"),
            Some(original_content.clone())
        );
        assert_eq!(inserted_row.get::<i64, _>("created_ts"), created_ts);
        assert_eq!(inserted_row.get::<i64, _>("updated_ts"), created_ts);

        sqlx::query(
            r#"
            INSERT INTO account_data (user_id, data_type, content, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (user_id, data_type) DO UPDATE
            SET content = EXCLUDED.content, updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(&user_id)
        .bind(&data_type)
        .bind(&updated_content)
        .bind(created_ts)
        .bind(updated_ts)
        .execute(&*pool)
        .await
        .expect("Failed to upsert account_data fixture");

        let updated_row = sqlx::query(
            "SELECT content, created_ts, updated_ts FROM account_data WHERE user_id = $1 AND data_type = $2",
        )
        .bind(&user_id)
        .bind(&data_type)
        .fetch_one(&*pool)
        .await
        .expect("Failed to query updated account_data");

        assert_eq!(
            updated_row.get::<Option<serde_json::Value>, _>("content"),
            Some(updated_content)
        );
        assert_eq!(updated_row.get::<i64, _>("created_ts"), created_ts);
        assert_eq!(updated_row.get::<i64, _>("updated_ts"), updated_ts);

        sqlx::query("DELETE FROM account_data WHERE user_id = $1 AND data_type = $2")
            .bind(&user_id)
            .bind(&data_type)
            .execute(&*pool)
            .await
            .expect("Failed to clean account_data fixture");
    }

    #[tokio::test]
    async fn test_schema_contract_room_account_data_query_and_write_read_closure() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        let user_id = format!("@schema_room_account_{}:localhost", uuid::Uuid::new_v4());
        let room_id = format!("!schema-room-{}:localhost", uuid::Uuid::new_v4());
        let data_type = "m.tag";
        let original_data = serde_json::json!({
            "tags": { "m.favourite": { "order": 0.1 } }
        });
        let updated_data = serde_json::json!({
            "tags": { "m.lowpriority": { "order": 0.9 } }
        });
        let created_ts = chrono::Utc::now().timestamp_millis();
        let updated_ts = created_ts + 2000;

        sqlx::query(
            r#"
            INSERT INTO room_account_data (user_id, room_id, data_type, data, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $5)
            ON CONFLICT (user_id, room_id, data_type) DO UPDATE
            SET data = EXCLUDED.data, updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(&user_id)
        .bind(&room_id)
        .bind(data_type)
        .bind(&original_data)
        .bind(created_ts)
        .execute(&*pool)
        .await
        .expect("Failed to insert room_account_data fixture");

        let inserted_row = sqlx::query(
            "SELECT data, created_ts, updated_ts FROM room_account_data WHERE user_id = $1 AND room_id = $2 AND data_type = $3",
        )
        .bind(&user_id)
        .bind(&room_id)
        .bind(data_type)
        .fetch_one(&*pool)
        .await
        .expect("Failed to query inserted room_account_data");

        assert_eq!(
            inserted_row.get::<Option<serde_json::Value>, _>("data"),
            Some(original_data.clone())
        );
        assert_eq!(inserted_row.get::<i64, _>("created_ts"), created_ts);
        assert_eq!(inserted_row.get::<i64, _>("updated_ts"), created_ts);

        sqlx::query(
            r#"
            INSERT INTO room_account_data (user_id, room_id, data_type, data, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (user_id, room_id, data_type) DO UPDATE
            SET data = EXCLUDED.data, updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(&user_id)
        .bind(&room_id)
        .bind(data_type)
        .bind(&updated_data)
        .bind(created_ts)
        .bind(updated_ts)
        .execute(&*pool)
        .await
        .expect("Failed to upsert room_account_data fixture");

        let updated_row = sqlx::query(
            "SELECT data, created_ts, updated_ts FROM room_account_data WHERE user_id = $1 AND room_id = $2 AND data_type = $3",
        )
        .bind(&user_id)
        .bind(&room_id)
        .bind(data_type)
        .fetch_one(&*pool)
        .await
        .expect("Failed to query updated room_account_data");

        assert_eq!(
            updated_row.get::<Option<serde_json::Value>, _>("data"),
            Some(updated_data)
        );
        assert_eq!(updated_row.get::<i64, _>("created_ts"), created_ts);
        assert_eq!(updated_row.get::<i64, _>("updated_ts"), updated_ts);

        sqlx::query(
            "DELETE FROM room_account_data WHERE user_id = $1 AND room_id = $2 AND data_type = $3",
        )
        .bind(&user_id)
        .bind(&room_id)
        .bind(data_type)
        .execute(&*pool)
        .await
        .expect("Failed to clean room_account_data fixture");
    }

    #[tokio::test]
    async fn test_schema_contract_push_rules_shape() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        assert_column(
            &pool,
            "push_rules",
            "user_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "push_rules",
            "scope",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "push_rules",
            "rule_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "push_rules",
            "kind",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "push_rules",
            "priority_class",
            &["integer"],
            false,
            None,
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
            None,
        )
        .await;
        assert_column(
            &pool,
            "push_rules",
            "conditions",
            &["jsonb"],
            true,
            Some("[]"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "push_rules",
            "actions",
            &["jsonb"],
            true,
            Some("[]"),
            None,
        )
        .await;
        if has_column(&pool, "push_rules", "is_enabled").await {
            assert_column(
                &pool,
                "push_rules",
                "is_enabled",
                &["boolean"],
                true,
                Some("true"),
                None,
            )
            .await;
        }
        assert_column(
            &pool,
            "push_rules",
            "created_ts",
            &["bigint"],
            false,
            None,
            None,
        )
        .await;
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
            "Expected push_rules index idx_push_rules_user_priority because push queries sort by priority"
        );
    }

    #[tokio::test]
    async fn test_schema_contract_push_rules_query_and_write_read_closure() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        let user_id = format!("@schema_push_{}:localhost", uuid::Uuid::new_v4());
        let scope = "global";
        let kind = "override";
        let rule_id = format!("rule.{}", uuid::Uuid::new_v4());
        let actions = serde_json::json!(["notify", {"set_tweak": "sound", "value": "default"}]);
        let conditions = serde_json::json!([{
            "kind": "event_match",
            "key": "content.body",
            "pattern": "schema-contract"
        }]);
        let pattern = Some("schema-contract".to_string());
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
        .bind(&pattern)
        .bind(&conditions)
        .bind(&actions)
        .bind(created_ts)
        .execute(&*pool)
        .await
        .expect("Failed to insert push rule fixture");

        let rows = sqlx::query(
            r#"
            SELECT rule_id, pattern, conditions, actions, is_enabled, is_default
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

        assert_eq!(rows.len(), 1, "Expected one push rule row");
        let row = &rows[0];
        assert_eq!(
            row.get::<Option<String>, _>("rule_id").as_deref(),
            Some(rule_id.as_str())
        );
        assert_eq!(
            row.get::<Option<String>, _>("pattern").as_deref(),
            pattern.as_deref()
        );
        assert_eq!(
            row.get::<Option<serde_json::Value>, _>("conditions"),
            Some(conditions.clone())
        );
        assert_eq!(
            row.get::<Option<serde_json::Value>, _>("actions"),
            Some(actions.clone())
        );
        assert_eq!(row.get::<Option<bool>, _>("is_enabled"), Some(true));
        assert_eq!(row.get::<Option<bool>, _>("is_default"), Some(false));

        let toggled_actions = serde_json::json!(["dont_notify"]);
        sqlx::query(
            r#"
            UPDATE push_rules
            SET actions = $5, is_enabled = $4
            WHERE user_id = $1 AND scope = $2 AND kind = $3 AND rule_id = $6
            "#,
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

        let enabled_row = sqlx::query(
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
            enabled_row.get::<Option<bool>, _>("is_enabled"),
            Some(false)
        );
        assert_eq!(
            enabled_row.get::<Option<serde_json::Value>, _>("actions"),
            Some(toggled_actions)
        );

        sqlx::query("DELETE FROM push_rules WHERE user_id = $1 AND scope = $2 AND kind = $3 AND rule_id = $4")
            .bind(&user_id)
            .bind(scope)
            .bind(kind)
            .bind(&rule_id)
            .execute(&*pool)
            .await
            .expect("Failed to clean push rule fixture");
    }

    #[tokio::test]
    async fn test_schema_contract_room_retention_policies_shape() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        assert_column(
            &pool,
            "room_retention_policies",
            "room_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_retention_policies",
            "min_lifetime",
            &["bigint"],
            false,
            Some("0"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_retention_policies",
            "max_lifetime",
            &["bigint"],
            true,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_retention_policies",
            "expire_on_clients",
            &["boolean"],
            false,
            Some("false"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_retention_policies",
            "is_server_default",
            &["boolean"],
            false,
            Some("false"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_retention_policies",
            "created_ts",
            &["bigint"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_retention_policies",
            "updated_ts",
            &["bigint"],
            false,
            None,
            None,
        )
        .await;

        assert!(
            has_unique_constraint_on(&pool, "room_retention_policies", &["room_id"]).await,
            "Expected room_retention_policies UNIQUE(room_id)"
        );
    }

    #[tokio::test]
    async fn test_schema_contract_device_verification_request_shape() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        assert_column(
            &pool,
            "device_verification_request",
            "user_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "device_verification_request",
            "new_device_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "device_verification_request",
            "status",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "device_verification_request",
            "request_token",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "device_verification_request",
            "expires_at",
            &["timestamp with time zone"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "device_verification_request",
            "completed_at",
            &["timestamp with time zone"],
            true,
            None,
            None,
        )
        .await;

        assert!(
            sqlx::query_scalar::<_, bool>(
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM information_schema.table_constraints tc
                    JOIN information_schema.key_column_usage kcu
                      ON tc.constraint_name = kcu.constraint_name
                     AND tc.table_schema = kcu.table_schema
                    WHERE tc.table_schema = 'public'
                      AND tc.table_name = 'device_verification_request'
                      AND tc.constraint_type IN ('UNIQUE', 'PRIMARY KEY')
                      AND kcu.column_name = 'request_token'
                )
                "#,
            )
            .fetch_one(&*pool)
            .await
            .unwrap_or(false),
            "Expected device_verification_request to have uniqueness on request_token"
        );
    }

    #[tokio::test]
    async fn test_schema_contract_room_summary_tables_shape() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        assert_eq!(
            primary_key_columns(&pool, "room_summaries").await,
            vec!["room_id".to_string()],
            "Expected room_summaries PRIMARY KEY(room_id)"
        );
        assert!(
            has_unique_constraint_on(&pool, "room_summary_members", &["room_id", "user_id"]).await,
            "Expected room_summary_members UNIQUE(room_id,user_id)"
        );

        assert_column(
            &pool,
            "room_summaries",
            "hero_users",
            &["jsonb"],
            false,
            Some("'[]'::jsonb"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summaries",
            "updated_ts",
            &["bigint"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_members",
            "membership",
            &["text"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_members",
            "is_hero",
            &["boolean"],
            false,
            Some("false"),
            None,
        )
        .await;

        assert!(
            has_index_named(&pool, "idx_room_summaries_last_event_ts").await,
            "Expected room_summaries index idx_room_summaries_last_event_ts"
        );
        assert!(
            has_index_named(&pool, "idx_room_summaries_space").await,
            "Expected room_summaries index idx_room_summaries_space"
        );
        assert!(
            has_index_named(&pool, "idx_room_summary_members_user_membership_room").await,
            "Expected room_summary_members index idx_room_summary_members_user_membership_room"
        );
        assert!(
            has_index_named(&pool, "idx_room_summary_members_room_membership_hero_active").await,
            "Expected room_summary_members index idx_room_summary_members_room_membership_hero_active"
        );
        assert!(
            has_index_named(&pool, "idx_room_summary_members_room_hero_user").await,
            "Expected room_summary_members index idx_room_summary_members_room_hero_user"
        );
    }

    #[tokio::test]
    async fn test_schema_contract_room_summary_query_and_relation_closure() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RoomSummaryStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string();
        let (creator, hero, room_id) = seed_users_and_room(&pool, &suffix).await;

        let summary = storage
            .create_summary(CreateRoomSummaryRequest {
                room_id: room_id.clone(),
                room_type: Some("m.room".to_string()),
                name: Some("Schema Summary".to_string()),
                topic: Some("Room summary contract".to_string()),
                avatar_url: None,
                canonical_alias: Some("#schema-summary:localhost".to_string()),
                join_rule: Some("invite".to_string()),
                history_visibility: Some("shared".to_string()),
                guest_access: Some("forbidden".to_string()),
                is_direct: Some(false),
                is_space: Some(false),
            })
            .await
            .expect("Failed to create room summary contract fixture");

        assert_eq!(summary.room_id, room_id);
        assert_eq!(summary.member_count, 0);
        assert_eq!(summary.joined_member_count, 0);
        assert_eq!(summary.invited_member_count, 0);

        storage
            .add_member(CreateSummaryMemberRequest {
                room_id: room_id.clone(),
                user_id: creator.clone(),
                display_name: Some("Creator".to_string()),
                avatar_url: None,
                membership: "join".to_string(),
                is_hero: Some(false),
                last_active_ts: Some(100),
            })
            .await
            .expect("Failed to add creator summary member");

        storage
            .add_member(CreateSummaryMemberRequest {
                room_id: room_id.clone(),
                user_id: hero.clone(),
                display_name: Some("Hero".to_string()),
                avatar_url: None,
                membership: "invite".to_string(),
                is_hero: Some(false),
                last_active_ts: Some(150),
            })
            .await
            .expect("Failed to add hero summary member");

        let after_insert = storage
            .get_summary(&room_id)
            .await
            .expect("Failed to reload room summary after insert")
            .expect("Room summary should exist");

        assert_eq!(after_insert.member_count, 2);
        assert_eq!(after_insert.joined_member_count, 1);
        assert_eq!(after_insert.invited_member_count, 1);

        let updated_member = storage
            .update_member(
                &room_id,
                &hero,
                UpdateSummaryMemberRequest {
                    display_name: Some("Hero Updated".to_string()),
                    avatar_url: None,
                    membership: Some("join".to_string()),
                    is_hero: Some(true),
                    last_active_ts: Some(999),
                },
            )
            .await
            .expect("Failed to update hero summary member");

        assert_eq!(updated_member.membership, "join");
        assert!(updated_member.is_hero);

        let after_update = storage
            .get_summary(&room_id)
            .await
            .expect("Failed to reload room summary after member update")
            .expect("Room summary should still exist");

        assert_eq!(after_update.member_count, 2);
        assert_eq!(after_update.joined_member_count, 2);
        assert_eq!(after_update.invited_member_count, 0);

        let visible_to_hero = storage
            .get_summaries_for_user(&hero)
            .await
            .expect("Failed to query room summaries for hero");
        assert_eq!(visible_to_hero.len(), 1);
        assert_eq!(visible_to_hero[0].room_id, room_id);

        let heroes = storage
            .get_heroes(&room_id, 5)
            .await
            .expect("Failed to query room summary heroes");
        assert_eq!(heroes.len(), 2);
        assert_eq!(heroes[0].user_id, hero);
        assert_eq!(heroes[0].membership, "join");
        assert!(heroes[0].is_hero);

        cleanup_room_summary_fixtures(&pool, &room_id, &[creator, hero]).await;
    }

    #[tokio::test]
    async fn test_schema_contract_room_summary_state_and_stats_shape() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        assert_column(
            &pool,
            "room_summary_state",
            "id",
            &["bigint"],
            false,
            Some("nextval("),
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_state",
            "room_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_state",
            "event_type",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_state",
            "state_key",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_state",
            "event_id",
            &["text", "character varying"],
            true,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_state",
            "content",
            &["jsonb"],
            false,
            Some("'{}'::jsonb"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_state",
            "updated_ts",
            &["bigint"],
            false,
            None,
            None,
        )
        .await;
        assert!(
            has_unique_constraint_on(
                &pool,
                "room_summary_state",
                &["room_id", "event_type", "state_key"]
            )
            .await,
            "Expected room_summary_state UNIQUE(room_id,event_type,state_key)"
        );
        assert!(
            has_index_named(&pool, "idx_room_summary_state_room").await,
            "Expected room_summary_state index idx_room_summary_state_room"
        );

        assert_column(
            &pool,
            "room_summary_stats",
            "id",
            &["bigint"],
            false,
            Some("nextval("),
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_stats",
            "room_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_stats",
            "total_events",
            &["bigint"],
            false,
            Some("0"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_stats",
            "total_state_events",
            &["bigint"],
            false,
            Some("0"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_stats",
            "total_messages",
            &["bigint"],
            false,
            Some("0"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_stats",
            "total_media",
            &["bigint"],
            false,
            Some("0"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_stats",
            "storage_size",
            &["bigint"],
            false,
            Some("0"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_stats",
            "last_updated_ts",
            &["bigint"],
            false,
            None,
            None,
        )
        .await;
        assert!(
            has_unique_constraint_on(&pool, "room_summary_stats", &["room_id"]).await,
            "Expected room_summary_stats UNIQUE(room_id)"
        );
    }

    #[tokio::test]
    async fn test_schema_contract_room_summary_state_and_stats_query_and_write_read_closure() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RoomSummaryStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string();
        let (creator, hero, room_id) = seed_users_and_room(&pool, &suffix).await;

        let initial_state = storage
            .set_state(
                &room_id,
                "m.room.name",
                "",
                Some("$summary-state-initial"),
                serde_json::json!({ "name": "Schema Summary Initial" }),
            )
            .await
            .expect("Failed to insert room summary state");
        assert_eq!(initial_state.room_id, room_id);
        assert_eq!(initial_state.event_type, "m.room.name");
        assert_eq!(initial_state.state_key, "");

        let updated_state = storage
            .set_state(
                &room_id,
                "m.room.name",
                "",
                Some("$summary-state-updated"),
                serde_json::json!({ "name": "Schema Summary Updated" }),
            )
            .await
            .expect("Failed to upsert room summary state");
        assert_eq!(
            updated_state.event_id.as_deref(),
            Some("$summary-state-updated")
        );
        assert_eq!(
            updated_state.content,
            serde_json::json!({ "name": "Schema Summary Updated" })
        );

        let topic_state = storage
            .set_state(
                &room_id,
                "m.room.topic",
                "",
                Some("$summary-topic"),
                serde_json::json!({ "topic": "Schema Topic" }),
            )
            .await
            .expect("Failed to insert second room summary state");
        assert_eq!(topic_state.event_type, "m.room.topic");

        let fetched_state = storage
            .get_state(&room_id, "m.room.name", "")
            .await
            .expect("Failed to fetch room summary state")
            .expect("Expected room summary state to exist");
        assert_eq!(
            fetched_state.event_id.as_deref(),
            Some("$summary-state-updated")
        );
        assert_eq!(
            fetched_state.content,
            serde_json::json!({ "name": "Schema Summary Updated" })
        );

        let all_state = storage
            .get_all_state(&room_id)
            .await
            .expect("Failed to fetch all room summary state");
        assert_eq!(all_state.len(), 2, "Expected two room summary state rows");
        assert!(all_state.iter().any(|row| row.event_type == "m.room.name"));
        assert!(all_state.iter().any(|row| row.event_type == "m.room.topic"));

        let initial_stats = storage
            .update_stats(&room_id, 10, 2, 8, 1, 1024)
            .await
            .expect("Failed to insert room summary stats");
        assert_eq!(initial_stats.room_id, room_id);
        assert_eq!(initial_stats.total_events, 10);
        assert_eq!(initial_stats.total_messages, 8);

        let updated_stats = storage
            .update_stats(&room_id, 15, 4, 11, 2, 4096)
            .await
            .expect("Failed to update room summary stats");
        assert_eq!(updated_stats.total_events, 15);
        assert_eq!(updated_stats.total_state_events, 4);
        assert_eq!(updated_stats.total_messages, 11);
        assert_eq!(updated_stats.total_media, 2);
        assert_eq!(updated_stats.storage_size, 4096);

        let fetched_stats = storage
            .get_stats(&room_id)
            .await
            .expect("Failed to fetch room summary stats")
            .expect("Expected room summary stats to exist");
        assert_eq!(fetched_stats.total_events, 15);
        assert_eq!(fetched_stats.total_state_events, 4);
        assert_eq!(fetched_stats.total_messages, 11);
        assert_eq!(fetched_stats.total_media, 2);
        assert_eq!(fetched_stats.storage_size, 4096);

        cleanup_room_summary_fixtures(&pool, &room_id, &[creator, hero]).await;
    }

    #[tokio::test]
    async fn test_schema_contract_room_summary_queue_and_children_shape() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        assert_column(
            &pool,
            "room_summary_update_queue",
            "id",
            &["bigint"],
            false,
            Some("nextval("),
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_update_queue",
            "room_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_update_queue",
            "event_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_update_queue",
            "event_type",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_update_queue",
            "state_key",
            &["text", "character varying"],
            true,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_update_queue",
            "priority",
            &["integer"],
            false,
            Some("0"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_update_queue",
            "status",
            &["text", "character varying"],
            false,
            Some("pending"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_update_queue",
            "created_ts",
            &["bigint"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_update_queue",
            "processed_ts",
            &["bigint"],
            true,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_update_queue",
            "error_message",
            &["text", "character varying"],
            true,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_summary_update_queue",
            "retry_count",
            &["integer"],
            false,
            Some("0"),
            None,
        )
        .await;
        assert!(
            has_index_named(
                &pool,
                "idx_room_summary_update_queue_status_priority_created"
            )
            .await,
            "Expected room_summary_update_queue index idx_room_summary_update_queue_status_priority_created"
        );

        assert_column(
            &pool,
            "room_children",
            "id",
            &["bigint"],
            false,
            Some("nextval("),
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_children",
            "parent_room_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_children",
            "child_room_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_children",
            "state_key",
            &["text", "character varying"],
            true,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_children",
            "content",
            &["jsonb"],
            false,
            Some("'{}'::jsonb"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_children",
            "suggested",
            &["boolean"],
            false,
            Some("false"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_children",
            "created_ts",
            &["bigint"],
            false,
            Some("0"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "room_children",
            "updated_ts",
            &["bigint"],
            true,
            None,
            None,
        )
        .await;
        assert!(
            has_unique_constraint_on(&pool, "room_children", &["parent_room_id", "child_room_id"])
                .await,
            "Expected room_children UNIQUE(parent_room_id,child_room_id)"
        );
        assert!(
            has_index_named(&pool, "idx_room_children_parent_suggested").await,
            "Expected room_children index idx_room_children_parent_suggested"
        );
        assert!(
            has_index_named(&pool, "idx_room_children_child").await,
            "Expected room_children index idx_room_children_child"
        );
    }

    #[tokio::test]
    async fn test_schema_contract_room_summary_queue_and_children_query_and_write_read_closure() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = RoomSummaryStorage::new(&pool);
        let parent_suffix = uuid::Uuid::new_v4().to_string();
        let child_suffix = format!("{parent_suffix}-child");
        let (creator, hero, room_id) = seed_users_and_room(&pool, &parent_suffix).await;
        let (child_creator, child_hero, child_room_id) =
            seed_users_and_room(&pool, &child_suffix).await;

        storage
            .queue_update(&room_id, "$summary-high", "m.room.message", None, 9)
            .await
            .expect("Failed to queue high priority room summary update");
        storage
            .queue_update(&room_id, "$summary-low", "m.room.topic", Some(""), 3)
            .await
            .expect("Failed to queue low priority room summary update");

        let pending_updates = storage
            .get_pending_updates(10)
            .await
            .expect("Failed to query pending room summary updates");
        assert!(
            pending_updates.len() >= 2,
            "Expected at least two pending room summary updates"
        );
        assert_eq!(pending_updates[0].event_id, "$summary-high");
        assert_eq!(pending_updates[0].priority, 9);
        assert_eq!(pending_updates[1].event_id, "$summary-low");
        assert_eq!(pending_updates[1].priority, 3);

        storage
            .mark_update_processed(pending_updates[0].id)
            .await
            .expect("Failed to mark room summary update as processed");
        storage
            .mark_update_failed(pending_updates[1].id, "schema-contract failure")
            .await
            .expect("Failed to mark room summary update as failed");

        let processed_row = sqlx::query(
            "SELECT status, processed_ts, retry_count FROM room_summary_update_queue WHERE id = $1",
        )
        .bind(pending_updates[0].id)
        .fetch_one(&*pool)
        .await
        .expect("Failed to fetch processed room summary update");
        assert_eq!(
            processed_row.get::<String, _>("status"),
            "processed",
            "Expected processed room summary update status"
        );
        assert!(
            processed_row
                .get::<Option<i64>, _>("processed_ts")
                .is_some(),
            "Expected processed_ts to be set after mark_update_processed"
        );
        assert_eq!(processed_row.get::<i32, _>("retry_count"), 0);

        let failed_row = sqlx::query(
            "SELECT status, error_message, retry_count FROM room_summary_update_queue WHERE id = $1",
        )
        .bind(pending_updates[1].id)
        .fetch_one(&*pool)
        .await
        .expect("Failed to fetch failed room summary update");
        assert_eq!(
            failed_row.get::<String, _>("status"),
            "failed",
            "Expected failed room summary update status"
        );
        assert_eq!(
            failed_row
                .get::<Option<String>, _>("error_message")
                .as_deref(),
            Some("schema-contract failure")
        );
        assert_eq!(failed_row.get::<i32, _>("retry_count"), 1);

        sqlx::query(
            r#"
            INSERT INTO room_children (parent_room_id, child_room_id, state_key, content, suggested, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, NULL)
            ON CONFLICT (parent_room_id, child_room_id) DO UPDATE SET
                state_key = EXCLUDED.state_key,
                content = EXCLUDED.content,
                suggested = EXCLUDED.suggested,
                updated_ts = EXCLUDED.created_ts
            "#,
        )
        .bind(&room_id)
        .bind(&child_room_id)
        .bind("child-state")
        .bind(serde_json::json!({ "via": ["localhost"], "order": "1" }))
        .bind(true)
        .bind(100_i64)
        .execute(&*pool)
        .await
        .expect("Failed to insert room_children fixture");

        sqlx::query(
            r#"
            INSERT INTO room_children (parent_room_id, child_room_id, state_key, content, suggested, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, NULL)
            ON CONFLICT (parent_room_id, child_room_id) DO UPDATE SET
                state_key = EXCLUDED.state_key,
                content = EXCLUDED.content,
                suggested = EXCLUDED.suggested,
                updated_ts = EXCLUDED.created_ts
            "#,
        )
        .bind(&room_id)
        .bind(&child_room_id)
        .bind("child-state-updated")
        .bind(serde_json::json!({ "via": ["localhost"], "order": "2" }))
        .bind(false)
        .bind(200_i64)
        .execute(&*pool)
        .await
        .expect("Failed to upsert room_children fixture");

        let child_row = sqlx::query(
            r#"
            SELECT state_key, content, suggested, created_ts, updated_ts
            FROM room_children
            WHERE parent_room_id = $1 AND child_room_id = $2
            "#,
        )
        .bind(&room_id)
        .bind(&child_room_id)
        .fetch_one(&*pool)
        .await
        .expect("Failed to fetch room_children row");
        assert_eq!(
            child_row.get::<Option<String>, _>("state_key").as_deref(),
            Some("child-state-updated")
        );
        assert_eq!(
            child_row.get::<serde_json::Value, _>("content"),
            serde_json::json!({ "via": ["localhost"], "order": "2" })
        );
        assert!(!child_row.get::<bool, _>("suggested"));
        assert_eq!(child_row.get::<i64, _>("created_ts"), 100_i64);
        assert_eq!(child_row.get::<Option<i64>, _>("updated_ts"), Some(200_i64));

        let child_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM room_children WHERE parent_room_id = $1 AND child_room_id = $2",
        )
        .bind(&room_id)
        .bind(&child_room_id)
        .fetch_one(&*pool)
        .await
        .expect("Failed to count room_children rows");
        assert_eq!(
            child_count, 1,
            "Expected room_children upsert to keep one row"
        );

        cleanup_room_summary_fixtures(&pool, &room_id, &[creator, hero]).await;
        cleanup_room_summary_fixtures(&pool, &child_room_id, &[child_creator, child_hero]).await;
    }

    #[tokio::test]
    async fn test_schema_contract_search_index_shape() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        assert_column(
            &pool,
            "search_index",
            "event_id",
            &["character varying"],
            false,
            None,
            Some(255),
        )
        .await;
        assert_column(
            &pool,
            "search_index",
            "room_id",
            &["character varying"],
            false,
            None,
            Some(255),
        )
        .await;
        assert_column(
            &pool,
            "search_index",
            "user_id",
            &["character varying"],
            false,
            None,
            Some(255),
        )
        .await;
        assert_column(
            &pool,
            "search_index",
            "event_type",
            &["character varying"],
            false,
            None,
            Some(255),
        )
        .await;
        assert_column(
            &pool,
            "search_index",
            "content",
            &["text"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "search_index",
            "created_ts",
            &["bigint"],
            false,
            None,
            None,
        )
        .await;

        assert!(
            has_unique_constraint_on(&pool, "search_index", &["event_id"]).await,
            "Expected search_index UNIQUE(event_id)"
        );
        assert!(
            has_index_named(&pool, "idx_search_index_room").await,
            "Expected search_index index idx_search_index_room"
        );
        assert!(
            has_index_named(&pool, "idx_search_index_user").await,
            "Expected search_index index idx_search_index_user"
        );
        assert!(
            has_index_named(&pool, "idx_search_index_type").await,
            "Expected search_index index idx_search_index_type"
        );
    }

    #[tokio::test]
    async fn test_schema_contract_search_index_query_and_write_read_closure() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        let event_id_old = format!("$search-old-{}", uuid::Uuid::new_v4());
        let event_id_new = format!("$search-new-{}", uuid::Uuid::new_v4());
        let room_id = format!("!search-room-{}:localhost", uuid::Uuid::new_v4());
        let user_id = format!("@search-user-{}:localhost", uuid::Uuid::new_v4());
        let event_type = "m.room.message";
        let content_type = "m.text";
        let created_ts_old = chrono::Utc::now().timestamp_millis();
        let created_ts_new = created_ts_old + 5000;
        let updated_ts = created_ts_new + 1000;
        let original_content = "Hello Search Contract";
        let updated_content = "Updated Search Contract";

        sqlx::query(
            r#"
            INSERT INTO search_index (event_id, room_id, user_id, event_type, type, content, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, NULL)
            "#,
        )
        .bind(&event_id_old)
        .bind(&room_id)
        .bind(&user_id)
        .bind(event_type)
        .bind(content_type)
        .bind(original_content)
        .bind(created_ts_old)
        .execute(&*pool)
        .await
        .expect("Failed to insert initial search_index fixture");

        sqlx::query(
            r#"
            INSERT INTO search_index (event_id, room_id, user_id, event_type, type, content, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, NULL)
            "#,
        )
        .bind(&event_id_new)
        .bind(&room_id)
        .bind(&user_id)
        .bind(event_type)
        .bind(content_type)
        .bind("Newest Search Contract")
        .bind(created_ts_new)
        .execute(&*pool)
        .await
        .expect("Failed to insert second search_index fixture");

        let search_rows = sqlx::query(
            r#"
            SELECT event_id, room_id, user_id, event_type, content, created_ts
            FROM search_index
            WHERE LOWER(content) LIKE $1
            ORDER BY created_ts DESC
            LIMIT 20 OFFSET 0
            "#,
        )
        .bind("%search contract%")
        .fetch_all(&*pool)
        .await
        .expect("Failed to query search_index contract");

        assert_eq!(search_rows.len(), 2, "Expected two search results");
        assert_eq!(
            search_rows[0].get::<String, _>("event_id"),
            event_id_new,
            "Expected newest search row first because search storage orders by created_ts DESC"
        );
        assert_eq!(search_rows[1].get::<String, _>("event_id"), event_id_old);
        assert_eq!(search_rows[0].get::<String, _>("room_id"), room_id);
        assert_eq!(search_rows[0].get::<String, _>("user_id"), user_id);
        assert_eq!(search_rows[0].get::<String, _>("event_type"), event_type);

        sqlx::query(
            r#"
            INSERT INTO search_index (event_id, room_id, user_id, event_type, type, content, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (event_id) DO UPDATE SET
                content = EXCLUDED.content,
                updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(&event_id_old)
        .bind(&room_id)
        .bind(&user_id)
        .bind(event_type)
        .bind(content_type)
        .bind(updated_content)
        .bind(created_ts_old)
        .bind(updated_ts)
        .execute(&*pool)
        .await
        .expect("Failed to upsert search_index fixture");

        let updated_row = sqlx::query(
            "SELECT content, created_ts, updated_ts FROM search_index WHERE event_id = $1",
        )
        .bind(&event_id_old)
        .fetch_one(&*pool)
        .await
        .expect("Failed to fetch updated search_index row");

        assert_eq!(updated_row.get::<String, _>("content"), updated_content);
        assert_eq!(updated_row.get::<i64, _>("created_ts"), created_ts_old);
        assert_eq!(
            updated_row.get::<Option<i64>, _>("updated_ts"),
            Some(updated_ts)
        );

        sqlx::query("DELETE FROM search_index WHERE event_id = $1 OR event_id = $2")
            .bind(&event_id_old)
            .bind(&event_id_new)
            .execute(&*pool)
            .await
            .expect("Failed to clean search_index fixtures");
    }

    #[tokio::test]
    async fn test_schema_contract_space_summary_tables_shape() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        assert_column(
            &pool,
            "space_members",
            "space_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "space_members",
            "user_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "space_members",
            "membership",
            &["text", "character varying"],
            false,
            Some("join"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "space_members",
            "joined_ts",
            &["bigint"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "space_members",
            "updated_ts",
            &["bigint"],
            true,
            None,
            None,
        )
        .await;
        assert!(
            has_unique_constraint_on(&pool, "space_members", &["space_id", "user_id"]).await,
            "Expected space_members UNIQUE(space_id,user_id)"
        );
        assert!(
            has_index_named(&pool, "idx_space_members_space").await,
            "Expected space_members index idx_space_members_space"
        );
        assert!(
            has_index_named(&pool, "idx_space_members_user").await,
            "Expected space_members index idx_space_members_user"
        );
        assert!(
            has_index_named(&pool, "idx_space_members_membership").await,
            "Expected space_members index idx_space_members_membership"
        );

        assert_column(
            &pool,
            "space_summaries",
            "space_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "space_summaries",
            "summary",
            &["jsonb"],
            true,
            Some("'{}'::jsonb"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "space_summaries",
            "children_count",
            &["bigint"],
            true,
            Some("0"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "space_summaries",
            "member_count",
            &["bigint"],
            true,
            Some("0"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "space_summaries",
            "updated_ts",
            &["bigint"],
            false,
            None,
            None,
        )
        .await;
        assert!(
            has_unique_constraint_on(&pool, "space_summaries", &["space_id"]).await,
            "Expected space_summaries UNIQUE(space_id)"
        );
        assert!(
            has_index_named(&pool, "idx_space_summary_space").await,
            "Expected space_summaries index idx_space_summary_space"
        );

        assert_eq!(
            primary_key_columns(&pool, "space_events").await,
            vec!["event_id".to_string()],
            "Expected space_events PRIMARY KEY(event_id)"
        );
        assert_column(
            &pool,
            "space_events",
            "space_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "space_events",
            "event_type",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "space_events",
            "sender",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "space_events",
            "content",
            &["jsonb"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "space_events",
            "origin_server_ts",
            &["bigint"],
            false,
            None,
            None,
        )
        .await;
        assert!(
            has_index_named(&pool, "idx_space_events_space").await,
            "Expected space_events index idx_space_events_space"
        );
        assert!(
            has_index_named(&pool, "idx_space_events_space_type_ts").await,
            "Expected space_events index idx_space_events_space_type_ts"
        );
        assert!(
            has_index_named(&pool, "idx_space_events_space_ts").await,
            "Expected space_events index idx_space_events_space_ts"
        );

        assert_eq!(
            primary_key_columns(&pool, "space_statistics").await,
            vec!["space_id".to_string()],
            "Expected space_statistics PRIMARY KEY(space_id)"
        );
        assert_column(
            &pool,
            "space_statistics",
            "is_public",
            &["boolean"],
            false,
            Some("false"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "space_statistics",
            "child_room_count",
            &["bigint"],
            true,
            Some("0"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "space_statistics",
            "member_count",
            &["bigint"],
            true,
            Some("0"),
            None,
        )
        .await;
        assert!(
            has_index_named(&pool, "idx_space_statistics_member_count").await,
            "Expected space_statistics index idx_space_statistics_member_count"
        );
    }

    #[tokio::test]
    async fn test_schema_contract_space_summary_query_and_write_read_closure() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = SpaceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string();
        let (creator, member) = seed_space_users(&pool, &suffix).await;
        let child_room_id = format!("!schema-space-child-{suffix}:localhost");

        let space = storage
            .create_space(CreateSpaceRequest {
                room_id: format!("!schema-space-root-{suffix}:localhost"),
                name: Some("Schema Space".to_string()),
                topic: Some("Space contract".to_string()),
                avatar_url: None,
                creator: creator.clone(),
                join_rule: Some("invite".to_string()),
                visibility: Some("private".to_string()),
                is_public: Some(false),
                parent_space_id: None,
            })
            .await
            .expect("Failed to create space contract fixture");

        storage
            .add_space_member(&space.space_id, &member, "join", Some(&creator))
            .await
            .expect("Failed to add second space member");
        storage
            .add_child(AddChildRequest {
                space_id: space.space_id.clone(),
                room_id: child_room_id.clone(),
                sender: creator.clone(),
                is_suggested: true,
                via_servers: vec!["localhost".to_string()],
            })
            .await
            .expect("Failed to add space child");
        storage
            .update_space_summary(&space.space_id)
            .await
            .expect("Failed to update space summary");

        let summary = storage
            .get_space_summary(&space.space_id)
            .await
            .expect("Failed to query space summary")
            .expect("Space summary should exist");
        assert_eq!(summary.space_id, space.space_id);
        assert_eq!(summary.children_count, 1);
        assert_eq!(summary.member_count, 2);
        assert_eq!(
            summary.summary,
            serde_json::json!({
                "children_count": 1,
                "member_count": 2
            })
        );

        sqlx::query(
            r#"
            INSERT INTO space_statistics (space_id, name, is_public, child_room_count, member_count, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (space_id) DO UPDATE SET
                name = EXCLUDED.name,
                is_public = EXCLUDED.is_public,
                child_room_count = EXCLUDED.child_room_count,
                member_count = EXCLUDED.member_count,
                updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(&space.space_id)
        .bind("Schema Space")
        .bind(false)
        .bind(1_i64)
        .bind(2_i64)
        .bind(0_i64)
        .bind(100_i64)
        .execute(&*pool)
        .await
        .expect("Failed to upsert space_statistics fixture");

        let statistics = storage
            .get_space_statistics()
            .await
            .expect("Failed to query space statistics");
        let statistics_row = statistics
            .iter()
            .find(|row| row["space_id"] == space.space_id)
            .expect("Expected space statistics row");
        assert_eq!(statistics_row["child_room_count"], 1);
        assert_eq!(statistics_row["member_count"], 2);

        let event_id_old = format!("$space-event-old-{suffix}");
        let event_id_new = format!("$space-event-new-{suffix}");
        sqlx::query(
            r#"
            INSERT INTO space_events (event_id, space_id, event_type, sender, content, state_key, origin_server_ts, processed_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, NULL), ($8, $2, $3, $4, $9, $10, $11, NULL)
            "#,
        )
        .bind(&event_id_old)
        .bind(&space.space_id)
        .bind("m.space.child")
        .bind(&creator)
        .bind(serde_json::json!({"room_id": child_room_id, "via": ["localhost"]}))
        .bind(Some(child_room_id.as_str()))
        .bind(100_i64)
        .bind(&event_id_new)
        .bind(serde_json::json!({"room_id": "!schema-space-child-new", "via": ["localhost"]}))
        .bind(Some("!schema-space-child-new"))
        .bind(200_i64)
        .execute(&*pool)
        .await
        .expect("Failed to seed space_events fixtures");

        let events = storage
            .get_space_events(&space.space_id, Some("m.space.child"), 10)
            .await
            .expect("Failed to query filtered space events");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_id, event_id_new);
        assert_eq!(events[1].event_id, event_id_old);
        assert_eq!(events[0].space_id, space.space_id);
        assert_eq!(events[0].event_type, "m.space.child");

        storage
            .remove_space_member(&space.space_id, &member)
            .await
            .expect("Failed to remove joined space member");
        storage
            .remove_child(&space.space_id, &child_room_id)
            .await
            .expect("Failed to remove space child");
        storage
            .update_space_summary(&space.space_id)
            .await
            .expect("Failed to refresh space summary after removals");

        let summary_after_update = storage
            .get_space_summary(&space.space_id)
            .await
            .expect("Failed to reload space summary")
            .expect("Space summary should still exist");
        assert_eq!(summary_after_update.children_count, 0);
        assert_eq!(summary_after_update.member_count, 1);
        assert_eq!(
            summary_after_update.summary,
            serde_json::json!({
                "children_count": 0,
                "member_count": 1
            })
        );

        cleanup_space_fixtures(&pool, &space.space_id, &[creator, member]).await;
    }

    #[tokio::test]
    async fn test_schema_contract_space_children_and_hierarchy_shape() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        assert_column(
            &pool,
            "space_children",
            "id",
            &["bigint"],
            false,
            Some("nextval("),
            None,
        )
        .await;
        assert_column(
            &pool,
            "space_children",
            "space_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "space_children",
            "room_id",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "space_children",
            "sender",
            &["text", "character varying"],
            false,
            None,
            None,
        )
        .await;
        assert_column(
            &pool,
            "space_children",
            "is_suggested",
            &["boolean"],
            true,
            Some("false"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "space_children",
            "via_servers",
            &["jsonb"],
            true,
            Some("'[]'::jsonb"),
            None,
        )
        .await;
        assert_column(
            &pool,
            "space_children",
            "added_ts",
            &["bigint"],
            false,
            None,
            None,
        )
        .await;
        assert!(
            has_unique_constraint_on(&pool, "space_children", &["space_id", "room_id"]).await,
            "Expected space_children UNIQUE(space_id,room_id)"
        );
        assert!(
            has_index_named(&pool, "idx_space_children_space").await,
            "Expected space_children index idx_space_children_space"
        );
        assert!(
            has_index_named(&pool, "idx_space_children_room").await,
            "Expected space_children index idx_space_children_room"
        );
    }

    #[tokio::test]
    async fn test_schema_contract_space_children_and_hierarchy_query_and_write_read_closure() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = SpaceStorage::new(&pool);
        let root_suffix = uuid::Uuid::new_v4().to_string();
        let child_suffix = format!("{root_suffix}-nested");
        let (root_creator, root_member) = seed_space_users(&pool, &root_suffix).await;
        let (child_creator, child_member) = seed_space_users(&pool, &child_suffix).await;
        let leaf_room_id = format!("!schema-space-leaf-{root_suffix}:localhost");

        let root_space = storage
            .create_space(CreateSpaceRequest {
                room_id: format!("!schema-space-root-room-{root_suffix}:localhost"),
                name: Some("Root Space".to_string()),
                topic: Some("Hierarchy root".to_string()),
                avatar_url: None,
                creator: root_creator.clone(),
                join_rule: Some("invite".to_string()),
                visibility: Some("private".to_string()),
                is_public: Some(false),
                parent_space_id: None,
            })
            .await
            .expect("Failed to create root space fixture");
        let child_space = storage
            .create_space(CreateSpaceRequest {
                room_id: format!("!schema-space-child-room-{root_suffix}:localhost"),
                name: Some("Nested Space".to_string()),
                topic: Some("Hierarchy child".to_string()),
                avatar_url: None,
                creator: child_creator.clone(),
                join_rule: Some("public".to_string()),
                visibility: Some("public".to_string()),
                is_public: Some(true),
                parent_space_id: Some(root_space.space_id.clone()),
            })
            .await
            .expect("Failed to create child space fixture");

        storage
            .add_space_member(
                &root_space.space_id,
                &root_member,
                "join",
                Some(&root_creator),
            )
            .await
            .expect("Failed to add root space member");
        storage
            .add_space_member(
                &child_space.space_id,
                &child_member,
                "join",
                Some(&child_creator),
            )
            .await
            .expect("Failed to add child space member");

        let root_child = storage
            .add_child(AddChildRequest {
                space_id: root_space.space_id.clone(),
                room_id: child_space.room_id.clone(),
                sender: root_creator.clone(),
                is_suggested: true,
                via_servers: vec!["localhost".to_string()],
            })
            .await
            .expect("Failed to add child space relation");
        assert_eq!(root_child.room_id, child_space.room_id);
        assert!(root_child.is_suggested);

        let nested_leaf = storage
            .add_child(AddChildRequest {
                space_id: child_space.space_id.clone(),
                room_id: leaf_room_id.clone(),
                sender: child_creator.clone(),
                is_suggested: false,
                via_servers: vec!["localhost".to_string(), "example.com".to_string()],
            })
            .await
            .expect("Failed to add nested leaf relation");
        assert_eq!(nested_leaf.room_id, leaf_room_id);
        assert!(!nested_leaf.is_suggested);

        let root_children = storage
            .get_space_children(&root_space.space_id)
            .await
            .expect("Failed to query root space children");
        assert_eq!(root_children.len(), 1);
        assert_eq!(root_children[0].room_id, child_space.room_id);
        assert_eq!(root_children[0].via_servers, vec!["localhost".to_string()]);

        let nested_children = storage
            .get_child_spaces(&leaf_room_id)
            .await
            .expect("Failed to query nested child spaces");
        assert_eq!(nested_children.len(), 1);
        assert_eq!(nested_children[0].room_id, leaf_room_id);
        assert_eq!(
            nested_children[0].via_servers,
            vec!["localhost".to_string(), "example.com".to_string()]
        );

        let recursive = storage
            .get_recursive_hierarchy(&root_space.space_id, 3, false)
            .await
            .expect("Failed to query recursive space hierarchy");
        assert_eq!(recursive.len(), 2);
        assert_eq!(recursive[0].room_id, child_space.room_id);
        assert!(recursive[0].is_space);
        assert_eq!(recursive[0].depth, 0);
        assert_eq!(recursive[1].room_id, leaf_room_id);
        assert!(!recursive[1].is_space);
        assert_eq!(recursive[1].depth, 1);

        let paginated = storage
            .get_space_hierarchy_paginated(&root_space.space_id, 3, false, Some(1), None)
            .await
            .expect("Failed to query paginated space hierarchy");
        assert_eq!(paginated.rooms.len(), 1);
        assert_eq!(paginated.rooms[0].room_id, child_space.room_id);
        assert_eq!(paginated.rooms[0].room_type.as_deref(), Some("m.space"));
        assert_eq!(paginated.rooms[0].join_rule, "public");
        assert!(paginated.rooms[0].world_readable);
        assert!(paginated.rooms[0].guest_can_join);
        assert_eq!(paginated.rooms[0].num_joined_members, 2);
        assert_eq!(paginated.next_batch.as_deref(), Some(leaf_room_id.as_str()));
        assert_eq!(paginated.rooms[0].children_state.len(), 1);
        assert_eq!(
            paginated.rooms[0].children_state[0]["state_key"],
            serde_json::json!(leaf_room_id)
        );
        assert_eq!(
            paginated.rooms[0].children_state[0]["content"]["suggested"],
            serde_json::json!(false)
        );

        cleanup_space_fixtures(&pool, &root_space.space_id, &[root_creator, root_member]).await;
        cleanup_space_fixtures(&pool, &child_space.space_id, &[child_creator, child_member]).await;
    }

    #[tokio::test]
    async fn test_schema_contract_room_summary_queue_processor_service_closure() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = Arc::new(RoomSummaryStorage::new(&pool));
        let event_storage = Arc::new(EventStorage::new(&pool, "localhost".to_string()));
        let service = RoomSummaryService::new(storage.clone(), event_storage.clone(), None);

        let suffix = uuid::Uuid::new_v4().to_string();
        let (creator, hero, room_id) = seed_users_and_room(&pool, &suffix).await;

        storage
            .create_summary(CreateRoomSummaryRequest {
                room_id: room_id.clone(),
                room_type: None,
                name: Some("Processor Before".to_string()),
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: Some("invite".to_string()),
                history_visibility: Some("shared".to_string()),
                guest_access: Some("forbidden".to_string()),
                is_direct: Some(false),
                is_space: Some(false),
            })
            .await
            .expect("Failed to create room summary processor fixture");

        let state_event_id = format!("$summary-service-state-{suffix}");
        let message_event_id = format!("$summary-service-message-{suffix}");
        let missing_event_id = format!("$summary-service-missing-{suffix}");

        event_storage
            .create_event(
                CreateEventParams {
                    event_id: state_event_id.clone(),
                    room_id: room_id.clone(),
                    user_id: creator.clone(),
                    event_type: "m.room.name".to_string(),
                    content: serde_json::json!({ "name": "Processor Updated" }),
                    state_key: Some(String::new()),
                    origin_server_ts: 100_i64,
                },
                None,
            )
            .await
            .expect("Failed to create state event fixture");
        event_storage
            .create_event(
                CreateEventParams {
                    event_id: message_event_id.clone(),
                    room_id: room_id.clone(),
                    user_id: creator.clone(),
                    event_type: "org.example.signal".to_string(),
                    content: serde_json::json!({ "body": "service update" }),
                    state_key: None,
                    origin_server_ts: 200_i64,
                },
                None,
            )
            .await
            .expect("Failed to create message event fixture");

        service
            .queue_update(&room_id, &state_event_id, "m.room.name", Some(""))
            .await
            .expect("Failed to queue state update through service");
        service
            .queue_update(&room_id, &message_event_id, "org.example.signal", None)
            .await
            .expect("Failed to queue message update through service");
        service
            .queue_update(&room_id, &missing_event_id, "m.room.topic", Some(""))
            .await
            .expect("Failed to queue missing update through service");

        let queued_rows = sqlx::query(
            r#"
            SELECT event_id, priority, status
            FROM room_summary_update_queue
            WHERE room_id = $1
            ORDER BY priority DESC, created_ts ASC, id ASC
            "#,
        )
        .bind(&room_id)
        .fetch_all(&*pool)
        .await
        .expect("Failed to query queued room summary updates");
        assert_eq!(queued_rows.len(), 3);
        assert_eq!(queued_rows[0].get::<String, _>("event_id"), state_event_id);
        assert_eq!(queued_rows[0].get::<i32, _>("priority"), 10);
        assert_eq!(
            queued_rows[1].get::<String, _>("event_id"),
            missing_event_id
        );
        assert_eq!(queued_rows[1].get::<i32, _>("priority"), 10);
        assert_eq!(
            queued_rows[2].get::<String, _>("event_id"),
            message_event_id
        );
        assert_eq!(queued_rows[2].get::<i32, _>("priority"), 0);

        let processed = service
            .process_pending_updates(10)
            .await
            .expect("Failed to process room summary queue through service");
        assert_eq!(processed, 2, "Expected two successful queue updates");

        let processed_queue_rows = sqlx::query(
            r#"
            SELECT event_id, status, processed_ts, error_message, retry_count
            FROM room_summary_update_queue
            WHERE room_id = $1
            ORDER BY priority DESC, created_ts ASC, id ASC
            "#,
        )
        .bind(&room_id)
        .fetch_all(&*pool)
        .await
        .expect("Failed to query processed room summary queue rows");
        assert_eq!(
            processed_queue_rows[0].get::<String, _>("status"),
            "processed"
        );
        assert!(processed_queue_rows[0]
            .get::<Option<i64>, _>("processed_ts")
            .is_some());
        assert_eq!(processed_queue_rows[1].get::<String, _>("status"), "failed");
        assert_eq!(
            processed_queue_rows[1]
                .get::<Option<String>, _>("error_message")
                .as_deref(),
            Some("Not found: Event not found")
        );
        assert_eq!(processed_queue_rows[1].get::<i32, _>("retry_count"), 1);
        assert_eq!(
            processed_queue_rows[2].get::<String, _>("status"),
            "processed"
        );

        let updated_name_state = service
            .get_state(&room_id, "m.room.name", "")
            .await
            .expect("Failed to fetch processed room summary state")
            .expect("Expected processed room summary state");
        assert_eq!(
            updated_name_state.event_id.as_deref(),
            Some(state_event_id.as_str())
        );
        assert_eq!(
            updated_name_state.content,
            serde_json::json!({ "name": "Processor Updated" })
        );

        let updated_summary = storage
            .get_summary(&room_id)
            .await
            .expect("Failed to fetch updated room summary")
            .expect("Expected updated room summary");
        assert_eq!(updated_summary.name.as_deref(), Some("Processor Updated"));
        assert_eq!(
            updated_summary.last_event_id.as_deref(),
            Some(message_event_id.as_str())
        );
        assert_eq!(updated_summary.last_event_ts, Some(200_i64));
        assert_eq!(updated_summary.last_message_ts, None);

        cleanup_room_summary_fixtures(&pool, &room_id, &[creator, hero]).await;
    }
}
