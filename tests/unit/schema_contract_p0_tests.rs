#![cfg(test)]
mod schema_contract_p0_suite {
    use crate::common::get_test_pool_async;
    use std::sync::Arc;

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
    }
}
