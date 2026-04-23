use sqlx::{Pool, Postgres};
use std::sync::Arc;

pub struct SchemaValidator {
    pool: Arc<Pool<Postgres>>,
}

#[derive(Debug, Default, Clone)]
pub struct SchemaValidationResult {
    pub is_valid: bool,
    pub is_healthy: bool,
    pub missing_tables: Vec<String>,
    pub missing_columns: Vec<String>,
    pub missing_indexes: Vec<String>,
    pub schema_info: Vec<TableSchemaInfo>,
}

#[derive(Debug, Clone, Default)]
pub struct TableSchemaInfo {
    pub table_name: String,
    pub missing_columns: Vec<String>,
    pub missing_indexes: Vec<String>,
    pub missing_constraints: Vec<String>,
}

const REQUIRED_TABLES: &[&str] = &[
    "users",
    "rooms",
    "events",
    "devices",
    "access_tokens",
    "presence",
    "notifications",
    "room_memberships",
    "account_data",
    "push_rules",
    "schema_migrations",
];

const REQUIRED_COLUMNS: &[(&str, &str)] = &[
    ("users", "user_id"),
    ("users", "password_hash"),
    ("users", "created_ts"),
    ("rooms", "room_id"),
    ("rooms", "room_version"),
    ("rooms", "is_public"),
    ("devices", "device_id"),
    ("devices", "user_id"),
    ("events", "event_id"),
    ("events", "room_id"),
    ("presence", "user_id"),
    ("presence", "presence"),
    ("notifications", "user_id"),
];

impl SchemaValidator {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    pub async fn validate_table_exists(&self, table_name: &str) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.tables \
             WHERE table_name = $1 AND table_schema = current_schema()",
        )
        .bind(table_name)
        .fetch_one(&*self.pool)
        .await?;
        Ok(count > 0)
    }

    pub async fn validate_column_exists(
        &self,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.columns \
             WHERE table_name = $1 AND column_name = $2",
        )
        .bind(table_name)
        .bind(column_name)
        .fetch_one(&*self.pool)
        .await?;
        Ok(count > 0)
    }

    pub async fn validate_all(&self) -> Result<SchemaValidationResult, sqlx::Error> {
        let mut missing_tables = Vec::new();
        let mut missing_columns = Vec::new();

        for table in REQUIRED_TABLES {
            if !self.validate_table_exists(table).await? {
                missing_tables.push(table.to_string());
            }
        }

        for (table, column) in REQUIRED_COLUMNS {
            if !self.validate_column_exists(table, column).await? {
                missing_columns.push(format!("{}.{}", table, column));
            }
        }

        let is_valid = missing_tables.is_empty() && missing_columns.is_empty();

        Ok(SchemaValidationResult {
            is_valid,
            is_healthy: is_valid,
            missing_tables,
            missing_columns,
            missing_indexes: Vec::new(),
            schema_info: Vec::new(),
        })
    }

    pub async fn validate_indexes(&self) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT indexname FROM pg_indexes WHERE schemaname = current_schema() ORDER BY indexname",
        )
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn validate_required_tables(
        &self,
        tables: &[&str],
    ) -> Result<Vec<String>, sqlx::Error> {
        let mut missing = Vec::new();
        for table in tables {
            if !self.validate_table_exists(table).await? {
                missing.push(table.to_string());
            }
        }
        Ok(missing)
    }

    pub async fn validate_required_columns(
        &self,
        requirements: &[(&str, &str)],
    ) -> Result<Vec<String>, sqlx::Error> {
        let mut missing = Vec::new();
        for (table, column) in requirements {
            if !self.validate_column_exists(table, column).await? {
                missing.push(format!("{}.{}", table, column));
            }
        }
        Ok(missing)
    }

    #[cfg(feature = "runtime-ddl")]
    pub async fn repair_missing_columns(&self) -> Result<Vec<String>, sqlx::Error> {
        let mut repaired = Vec::new();
        let columns_to_add = vec![
            ("rooms", "name", "VARCHAR(255)"),
            ("rooms", "topic", "TEXT"),
            ("rooms", "avatar_url", "TEXT"),
            ("rooms", "canonical_alias", "VARCHAR(255)"),
            ("rooms", "member_count", "BIGINT DEFAULT 0"),
            (
                "rooms",
                "history_visibility",
                "VARCHAR(50) DEFAULT 'joined'",
            ),
            ("rooms", "encryption", "VARCHAR(50)"),
        ];
        for (table, column, col_type) in columns_to_add {
            if !self.validate_column_exists(table, column).await? {
                let sql = format!(
                    "ALTER TABLE {} ADD COLUMN IF NOT EXISTS {} {}",
                    table, column, col_type
                );
                sqlx::query(&sql).execute(&*self.pool).await?;
                repaired.push(format!("{}.{}", table, column));
            }
        }
        Ok(repaired)
    }

    #[cfg(feature = "runtime-ddl")]
    pub async fn create_missing_indexes(&self) -> Result<Vec<String>, sqlx::Error> {
        let mut created = Vec::new();
        let indexes = vec![
            ("idx_rooms_name", "rooms(name)"),
            ("idx_rooms_member_count", "rooms(member_count)"),
            ("idx_notifications_user_id", "notifications(user_id)"),
            ("idx_notifications_ts", "notifications(ts DESC)"),
        ];
        for (name, def) in indexes {
            let exists: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM pg_indexes WHERE schemaname = current_schema() AND indexname = $1",
            )
            .bind(name)
            .fetch_one(&*self.pool)
            .await?;
            if exists == 0 {
                let sql = format!("CREATE INDEX IF NOT EXISTS {} ON {}", name, def);
                sqlx::query(&sql).execute(&*self.pool).await?;
                created.push(name.to_string());
            }
        }
        Ok(created)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_validation_result_success() {
        let result = SchemaValidationResult {
            is_valid: true,
            is_healthy: true,
            missing_tables: vec![],
            missing_columns: vec![],
            missing_indexes: vec![],
            schema_info: vec![],
        };
        assert!(result.is_valid);
    }
}
