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

pub struct DatabaseIntegrityChecker {
    pool: Pool<Postgres>,
}

impl DatabaseIntegrityChecker {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    pub async fn check_foreign_keys(&self) -> Result<Vec<ForeignKeyInfo>, sqlx::Error> {
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

    pub async fn check_indexes(&self, table_name: &str) -> Result<Vec<IndexInfo>, sqlx::Error> {
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

    pub async fn check_orphan_data(&self) -> Result<serde_json::Value, sqlx::Error> {
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

    pub async fn check_table_exists(&self, table_name: &str) -> Result<bool, sqlx::Error> {
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

    pub async fn check_column_exists(
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

    pub async fn verify_field_naming(&self) -> Result<serde_json::Value, sqlx::Error> {
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

    pub async fn get_migration_status(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
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
        let checker = DatabaseIntegrityChecker {
            pool: Pool::connect_lazy("postgres://localhost/test").unwrap(),
        };

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
}
