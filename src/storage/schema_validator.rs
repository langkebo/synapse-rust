use sqlx::{Pool, Postgres};
use std::collections::HashMap;
use std::sync::Arc;

pub struct SchemaValidator {
    pool: Arc<Pool<Postgres>>,
}

#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub column_name: String,
    pub data_type: String,
    pub is_nullable: String,
}

#[derive(Debug, Clone)]
pub struct TableSchema {
    pub table_name: String,
    pub columns: Vec<ColumnInfo>,
}

#[derive(Debug, Clone)]
pub struct TableSchemaInfo {
    pub table_name: String,
    pub missing_columns: Vec<String>,
}

impl SchemaValidator {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    pub async fn get_table_columns(&self, table_name: &str) -> Result<Vec<ColumnInfo>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String, String)>(
            r#"
            SELECT column_name, data_type, is_nullable
            FROM information_schema.columns
            WHERE table_name = $1
            ORDER BY ordinal_position
            "#,
        )
        .bind(table_name)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(column_name, data_type, is_nullable)| ColumnInfo {
                column_name,
                data_type,
                is_nullable,
            })
            .collect())
    }

    pub async fn get_all_tables(&self) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT table_name
            FROM information_schema.tables
            WHERE table_schema = 'public'
            AND table_type = 'BASE TABLE'
            ORDER BY table_name
            "#,
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn validate_column_exists(&self, table_name: &str, column_name: &str) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM information_schema.columns
            WHERE table_name = $1 AND column_name = $2
            "#,
        )
        .bind(table_name)
        .bind(column_name)
        .fetch_one(&*self.pool)
        .await?;

        Ok(count > 0)
    }

    pub async fn validate_table_exists(&self, table_name: &str) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM information_schema.tables
            WHERE table_name = $1 AND table_schema = 'public'
            "#,
        )
        .bind(table_name)
        .fetch_one(&*self.pool)
        .await?;

        Ok(count > 0)
    }

    pub async fn get_schema_map(&self) -> Result<HashMap<String, Vec<String>>, sqlx::Error> {
        let tables = self.get_all_tables().await?;
        let mut schema_map = HashMap::new();

        for table in tables {
            let columns = self.get_table_columns(&table).await?;
            let column_names: Vec<String> = columns.into_iter().map(|c| c.column_name).collect();
            schema_map.insert(table.clone(), column_names);
        }

        Ok(schema_map)
    }

    pub async fn validate_required_columns(&self, requirements: &[(&str, &str)]) -> Result<Vec<String>, sqlx::Error> {
        let mut missing = Vec::new();

        for (table, column) in requirements {
            if !self.validate_column_exists(table, column).await? {
                missing.push(format!("{}.{}", table, column));
            }
        }

        Ok(missing)
    }

    pub async fn validate_required_tables(&self, tables: &[&str]) -> Result<Vec<String>, sqlx::Error> {
        let mut missing = Vec::new();

        for table in tables {
            if !self.validate_table_exists(table).await? {
                missing.push(table.to_string());
            }
        }

        Ok(missing)
    }

    pub async fn check_schema_consistency(&self) -> Result<SchemaConsistencyReport, sqlx::Error> {
        let mut report = SchemaConsistencyReport::default();

        let required_tables = vec![
            "users", "rooms", "events", "devices", "access_tokens",
            "presence", "federation_signing_keys", "notifications",
            "room_memberships", "account_data", "push_rules",
        ];

        report.missing_tables = self.validate_required_tables(&required_tables).await?;

        let required_columns: Vec<(&str, &str)> = vec![
            ("rooms", "room_id"),
            ("rooms", "name"),
            ("rooms", "join_rules"),
            ("rooms", "creator"),
            ("rooms", "room_version"),
            ("rooms", "is_public"),
            ("rooms", "member_count"),
            ("rooms", "creation_ts"),
            ("users", "user_id"),
            ("users", "password_hash"),
            ("users", "creation_ts"),
            ("presence", "user_id"),
            ("presence", "presence"),
            ("presence", "updated_ts"),
            ("federation_signing_keys", "server_name"),
            ("federation_signing_keys", "key_id"),
            ("federation_signing_keys", "created_ts"),
            ("notifications", "user_id"),
            ("notifications", "ts"),
        ];

        report.missing_columns = self.validate_required_columns(&required_columns).await?;

        report.is_valid = report.missing_tables.is_empty() && report.missing_columns.is_empty();

        Ok(report)
    }

    pub async fn validate_all(&self) -> Result<SchemaValidationResult, sqlx::Error> {
        let report = self.check_schema_consistency().await?;
        
        let mut schema_info = Vec::new();
        let tables_to_check = vec!["rooms", "users", "presence", "federation_signing_keys", "notifications"];
        
        for table in tables_to_check {
            let columns = self.get_table_columns(table).await?;
            let column_names: Vec<String> = columns.iter().map(|c| c.column_name.clone()).collect();
            let missing: Vec<String> = column_names.iter().filter(|c| !column_names.contains(c)).cloned().collect();
            schema_info.push(TableSchemaInfo {
                table_name: table.to_string(),
                missing_columns: missing,
            });
        }

        Ok(SchemaValidationResult {
            is_valid: report.is_valid,
            is_healthy: report.is_valid,
            missing_tables: report.missing_tables,
            missing_columns: report.missing_columns,
            missing_indexes: Vec::new(),
            schema_info,
        })
    }

    pub async fn repair_missing_columns(&self) -> Result<Vec<String>, sqlx::Error> {
        let mut repaired = Vec::new();

        let columns_to_add = vec![
            ("rooms", "name", "VARCHAR(255)"),
            ("rooms", "topic", "TEXT"),
            ("rooms", "avatar_url", "TEXT"),
            ("rooms", "canonical_alias", "VARCHAR(255)"),
            ("rooms", "member_count", "BIGINT DEFAULT 0"),
            ("rooms", "history_visibility", "VARCHAR(50) DEFAULT 'joined'"),
            ("rooms", "encryption", "VARCHAR(50)"),
            ("rooms", "last_activity_ts", "BIGINT"),
        ];

        for (table, column, col_type) in columns_to_add {
            if !self.validate_column_exists(table, column).await? {
                let sql = format!("ALTER TABLE {} ADD COLUMN IF NOT EXISTS {} {}", table, column, col_type);
                sqlx::query(&sql)
                    .execute(&*self.pool)
                    .await?;
                repaired.push(format!("{}.{}", table, column));
            }
        }

        Ok(repaired)
    }

    pub async fn validate_indexes(&self) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT indexname
            FROM pg_indexes
            WHERE schemaname = 'public'
            ORDER BY indexname
            "#,
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn create_missing_indexes(&self) -> Result<Vec<String>, sqlx::Error> {
        let mut created = Vec::new();

        let indexes_to_create = vec![
            ("idx_rooms_name", "rooms(name)"),
            ("idx_rooms_member_count", "rooms(member_count)"),
            ("idx_notifications_user_id", "notifications(user_id)"),
            ("idx_notifications_ts", "notifications(ts DESC)"),
        ];

        for (index_name, index_def) in indexes_to_create {
            let exists: bool = self.index_exists(index_name).await?;
            if !exists {
                let sql = format!("CREATE INDEX IF NOT EXISTS {} ON {}", index_name, index_def);
                sqlx::query(&sql)
                    .execute(&*self.pool)
                    .await?;
                created.push(index_name.to_string());
            }
        }

        Ok(created)
    }

    async fn index_exists(&self, index_name: &str) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM pg_indexes
            WHERE schemaname = 'public' AND indexname = $1
            "#,
        )
        .bind(index_name)
        .fetch_one(&*self.pool)
        .await?;

        Ok(count > 0)
    }
}

#[derive(Debug, Default)]
pub struct SchemaConsistencyReport {
    pub is_valid: bool,
    pub missing_tables: Vec<String>,
    pub missing_columns: Vec<String>,
}

impl SchemaConsistencyReport {
    pub fn to_string(&self) -> String {
        if self.is_valid {
            "Schema is consistent".to_string()
        } else {
            let mut msg = String::new();
            if !self.missing_tables.is_empty() {
                msg.push_str(&format!("Missing tables: {}\n", self.missing_tables.join(", ")));
            }
            if !self.missing_columns.is_empty() {
                msg.push_str(&format!("Missing columns: {}\n", self.missing_columns.join(", ")));
            }
            msg
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_schema_validator() {
        let _ = SchemaConsistencyReport::default();
    }
}
