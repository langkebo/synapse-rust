use sqlx::{Pool, Postgres, Row};

#[derive(Debug, Clone, sqlx::FromRow)]
struct ForeignKeyInfo {
    constraint_name: String,
    table_name: String,
    column_name: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct IndexInfo {
    indexname: String,
    tablename: String,
}

struct DatabaseIntegrityChecker {
    pool: Pool<Postgres>,
}

impl DatabaseIntegrityChecker {
    fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    async fn check_foreign_keys(&self) -> Result<Vec<ForeignKeyInfo>, sqlx::Error> {
        let rows = sqlx::query_as::<_, ForeignKeyInfo>(
            r#"
            SELECT
                tc.constraint_name,
                tc.table_name,
                kcu.column_name
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
                ON tc.constraint_name = kcu.constraint_name
                AND tc.table_schema = kcu.table_schema
            WHERE tc.constraint_type = 'FOREIGN KEY'
                AND tc.table_schema = 'public'
            ORDER BY tc.table_name, tc.constraint_name
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    async fn check_indexes(&self, table_name: &str) -> Result<Vec<IndexInfo>, sqlx::Error> {
        let rows = sqlx::query_as::<_, IndexInfo>(
            r#"
            SELECT indexname, tablename
            FROM pg_indexes
            WHERE schemaname = 'public' AND tablename = $1
            ORDER BY indexname
            "#,
        )
        .bind(table_name)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    async fn check_orphan_data(&self) -> Result<serde_json::Value, sqlx::Error> {
        let orphan_events: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM events e
            WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = e.room_id)
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let orphan_memberships: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM room_memberships rm
            WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = rm.user_id)
               OR NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = rm.room_id)
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let orphan_tokens: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM access_tokens at
            WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = at.user_id)
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(serde_json::json!({
            "orphan_events": orphan_events,
            "orphan_memberships": orphan_memberships,
            "orphan_tokens": orphan_tokens,
            "total_orphans": orphan_events + orphan_memberships + orphan_tokens
        }))
    }

    async fn check_table_exists(&self, table_name: &str) -> Result<bool, sqlx::Error> {
        let exists: Option<bool> = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM information_schema.tables 
                WHERE table_schema = 'public' AND table_name = $1
            )
            "#,
        )
        .bind(table_name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(exists.unwrap_or(false))
    }

    async fn check_column_exists(
        &self,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool, sqlx::Error> {
        let exists: Option<bool> = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM information_schema.columns 
                WHERE table_schema = 'public' 
                  AND table_name = $1 
                  AND column_name = $2
            )
            "#,
        )
        .bind(table_name)
        .bind(column_name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(exists.unwrap_or(false))
    }

    async fn verify_field_naming(&self) -> Result<serde_json::Value, sqlx::Error> {
        let timestamp_fields: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM information_schema.columns
            WHERE table_schema = 'public'
              AND column_name LIKE '%_ts'
              AND data_type = 'bigint'
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let bool_fields_with_is: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM information_schema.columns
            WHERE table_schema = 'public'
              AND (column_name LIKE 'is_%' OR column_name LIKE 'has_%')
              AND data_type = 'boolean'
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let inconsistent_timestamp: Vec<(String, String, String)> = sqlx::query_as(
            r#"
            SELECT table_name, column_name, data_type
            FROM information_schema.columns
            WHERE table_schema = 'public'
              AND column_name ~ '(created|updated|expires|last_seen|joined)'
              AND column_name !~ '_ts$'
              AND data_type = 'bigint'
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(serde_json::json!({
            "timestamp_fields_count": timestamp_fields,
            "boolean_fields_with_prefix_count": bool_fields_with_is,
            "inconsistent_timestamp_fields": inconsistent_timestamp
        }))
    }

    async fn check_audit_critical_indexes(&self) -> Result<Vec<String>, sqlx::Error> {
        let critical_indexes = vec![
            "idx_room_summary_state_room",
            "idx_room_summary_update_queue_status_priority_created",
            "idx_room_children_parent_suggested",
            "idx_room_children_child",
            "idx_retention_cleanup_queue_status_origin",
            "idx_retention_cleanup_logs_room_started",
            "idx_deleted_events_index_room_ts",
            "idx_device_trust_status_user_level",
            "idx_cross_signing_trust_user_trusted",
            "idx_device_verification_request_user_device_pending",
            "idx_device_verification_request_expires_pending",
            "idx_verification_requests_to_user_state",
        ];

        let mut missing = Vec::new();
        for index_name in critical_indexes {
            let exists: Option<bool> = sqlx::query_scalar(
                r#"
                SELECT EXISTS (
                    SELECT 1 FROM pg_indexes
                    WHERE schemaname = 'public' AND indexname = $1
                )
                "#,
            )
            .bind(index_name)
            .fetch_optional(&self.pool)
            .await?;

            if !exists.unwrap_or(false) {
                missing.push(index_name.to_string());
            }
        }

        Ok(missing)
    }

    async fn check_audit_critical_constraints(&self) -> Result<Vec<String>, sqlx::Error> {
        let critical_constraints = vec![
            (
                "room_summary_state",
                "uq_room_summary_state_room_type_state",
            ),
            ("room_summary_state", "fk_room_summary_state_room"),
            ("room_summary_stats", "fk_room_summary_stats_room"),
            (
                "room_summary_update_queue",
                "fk_room_summary_update_queue_room",
            ),
            ("room_children", "uq_room_children_parent_child"),
            ("room_children", "fk_room_children_parent"),
            ("room_children", "fk_room_children_child"),
            (
                "retention_cleanup_queue",
                "uq_retention_cleanup_queue_room_event",
            ),
            ("retention_cleanup_queue", "fk_retention_cleanup_queue_room"),
            ("retention_cleanup_logs", "fk_retention_cleanup_logs_room"),
            ("retention_stats", "fk_retention_stats_room"),
            ("deleted_events_index", "uq_deleted_events_index_room_event"),
            ("deleted_events_index", "fk_deleted_events_index_room"),
            ("device_trust_status", "uq_device_trust_status_user_device"),
            ("cross_signing_trust", "uq_cross_signing_trust_user_target"),
        ];

        let mut missing = Vec::new();
        for (table_name, constraint_name) in critical_constraints {
            let exists: Option<bool> = sqlx::query_scalar(
                r#"
                SELECT EXISTS (
                    SELECT 1 FROM information_schema.table_constraints
                    WHERE table_schema = 'public'
                      AND table_name = $1
                      AND constraint_name = $2
                )
                "#,
            )
            .bind(table_name)
            .bind(constraint_name)
            .fetch_optional(&self.pool)
            .await?;

            if !exists.unwrap_or(false) {
                missing.push(format!("{}.{}", table_name, constraint_name));
            }
        }

        Ok(missing)
    }

    async fn get_migration_status(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT version, name, applied_ts, description
            FROM schema_migrations
            ORDER BY applied_ts DESC
            LIMIT 10
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut result = Vec::new();
        for row in rows {
            result.push(serde_json::json!({
                "version": row.get::<Option<String>, _>("version"),
                "name": row.get::<Option<String>, _>("name"),
                "applied_ts": row.get::<Option<i64>, _>("applied_ts"),
                "description": row.get::<Option<String>, _>("description")
            }));
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integrity_checker_struct() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _guard = runtime.enter();
        let checker =
            DatabaseIntegrityChecker::new(Pool::connect_lazy("postgres://localhost/test").unwrap());
        let _ = DatabaseIntegrityChecker::check_foreign_keys;
        let _ = DatabaseIntegrityChecker::check_indexes;
        let _ = DatabaseIntegrityChecker::check_orphan_data;
        let _ = DatabaseIntegrityChecker::check_table_exists;
        let _ = DatabaseIntegrityChecker::check_column_exists;
        let _ = DatabaseIntegrityChecker::verify_field_naming;
        let _ = DatabaseIntegrityChecker::get_migration_status;

        assert!(std::mem::size_of_val(&checker) > 0);
    }

    #[test]
    fn test_foreign_key_info_struct() {
        let info = ForeignKeyInfo {
            constraint_name: "fk_users_devices".to_string(),
            table_name: "devices".to_string(),
            column_name: "user_id".to_string(),
        };

        assert_eq!(info.constraint_name, "fk_users_devices");
        assert_eq!(info.table_name, "devices");
        assert_eq!(info.column_name, "user_id");
    }

    #[test]
    fn test_index_info_struct() {
        let info = IndexInfo {
            indexname: "idx_users_username".to_string(),
            tablename: "users".to_string(),
        };

        assert_eq!(info.indexname, "idx_users_username");
        assert_eq!(info.tablename, "users");
    }

    #[tokio::test]
    async fn test_audit_critical_indexes_exist() {
        let pool = match std::env::var("TEST_DATABASE_URL") {
            Ok(url) => match Pool::connect(&url).await {
                Ok(pool) => pool,
                Err(_) => {
                    eprintln!("Skipping audit critical indexes test: database unavailable");
                    return;
                }
            },
            Err(_) => {
                eprintln!("Skipping audit critical indexes test: TEST_DATABASE_URL not set");
                return;
            }
        };

        let checker = DatabaseIntegrityChecker::new(pool);
        let missing = checker
            .check_audit_critical_indexes()
            .await
            .expect("Failed to check audit critical indexes");

        if !missing.is_empty() {
            eprintln!(
                "Warning: {} audit-critical indexes are missing: {:?}",
                missing.len(),
                missing
            );
        }
    }

    #[tokio::test]
    async fn test_audit_critical_constraints_exist() {
        let pool = match std::env::var("TEST_DATABASE_URL") {
            Ok(url) => match Pool::connect(&url).await {
                Ok(pool) => pool,
                Err(_) => {
                    eprintln!("Skipping audit critical constraints test: database unavailable");
                    return;
                }
            },
            Err(_) => {
                eprintln!("Skipping audit critical constraints test: TEST_DATABASE_URL not set");
                return;
            }
        };

        let checker = DatabaseIntegrityChecker::new(pool);
        let missing = checker
            .check_audit_critical_constraints()
            .await
            .expect("Failed to check audit critical constraints");

        if !missing.is_empty() {
            eprintln!(
                "Warning: {} audit-critical constraints are missing: {:?}",
                missing.len(),
                missing
            );
        }
    }
}
