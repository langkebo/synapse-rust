use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;

pub struct SchemaValidator {
    pool: Arc<Pool<Postgres>>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SchemaValidationResult {
    pub is_valid: bool,
    pub is_healthy: bool,
    pub missing_tables: Vec<String>,
    pub missing_columns: Vec<String>,
    pub missing_indexes: Vec<String>,
    pub schema_info: Vec<TableSchemaInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

    pub async fn validate_column_exists(&self, table_name: &str, column_name: &str) -> Result<bool, sqlx::Error> {
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
                missing_columns.push(format!("{table}.{column}"));
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
        sqlx::query_scalar("SELECT indexname FROM pg_indexes WHERE schemaname = current_schema() ORDER BY indexname")
            .fetch_all(&*self.pool)
            .await
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

    pub async fn validate_required_columns(&self, requirements: &[(&str, &str)]) -> Result<Vec<String>, sqlx::Error> {
        let mut missing = Vec::new();
        for (table, column) in requirements {
            if !self.validate_column_exists(table, column).await? {
                missing.push(format!("{table}.{column}"));
            }
        }
        Ok(missing)
    }

    #[cfg(feature = "runtime-ddl")]
    fn is_valid_sql_identifier(s: &str) -> bool {
        !s.is_empty()
            && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '(' || c == ')' || c == ' ' || c == ',')
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
            ("rooms", "history_visibility", "VARCHAR(50) DEFAULT 'joined'"),
            ("rooms", "encryption", "VARCHAR(50)"),
        ];
        for (table, column, col_type) in columns_to_add {
            if !self.validate_column_exists(table, column).await? {
                let sql = format!("ALTER TABLE {} ADD COLUMN IF NOT EXISTS {} {}", table, column, col_type);
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
            if !Self::is_valid_sql_identifier(name) || !Self::is_valid_sql_identifier(def) {
                tracing::warn!("Skipping invalid index identifier: {}", name);
                continue;
            }
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
    fn test_schema_validation_result_defaults() {
        let result = SchemaValidationResult::default();
        assert!(!result.is_valid);
        assert!(!result.is_healthy);
        assert!(result.missing_tables.is_empty());
        assert!(result.missing_columns.is_empty());
        assert!(result.missing_indexes.is_empty());
        assert!(result.schema_info.is_empty());
    }

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

    #[test]
    fn test_schema_validation_result_with_missing() {
        let result = SchemaValidationResult {
            is_valid: false,
            is_healthy: false,
            missing_tables: vec!["events".to_string()],
            missing_columns: vec!["users.display_name".to_string()],
            missing_indexes: vec!["idx_rooms_name".to_string()],
            schema_info: vec![],
        };
        assert!(!result.is_valid);
        assert!(!result.is_healthy);
        assert_eq!(result.missing_tables.len(), 1);
        assert_eq!(result.missing_columns.len(), 1);
        assert_eq!(result.missing_indexes.len(), 1);
    }

    #[test]
    fn test_table_schema_info_defaults() {
        let info = TableSchemaInfo::default();
        assert!(info.table_name.is_empty());
        assert!(info.missing_columns.is_empty());
        assert!(info.missing_indexes.is_empty());
        assert!(info.missing_constraints.is_empty());
    }

    #[test]
    fn test_table_schema_info_with_data() {
        let info = TableSchemaInfo {
            table_name: "users".to_string(),
            missing_columns: vec!["avatar_url".to_string()],
            missing_indexes: vec!["idx_users_name".to_string()],
            missing_constraints: vec!["fk_users_room".to_string()],
        };
        assert_eq!(info.table_name, "users");
        assert_eq!(info.missing_columns.len(), 1);
        assert_eq!(info.missing_indexes.len(), 1);
        assert_eq!(info.missing_constraints.len(), 1);
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use std::sync::Arc;

    async fn test_pool() -> Arc<PgPool> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&db_url)
            .await
            .expect("Failed to connect to test database");
        Arc::new(pool)
    }

    fn make_suffix() -> String {
        uuid::Uuid::new_v4().to_string().replace('-', "")
    }

    // ── validate_table_exists ──────────────────────────────────────────

    #[tokio::test]
    async fn test_validate_table_exists_known_table() {
        let pool = test_pool().await;
        let validator = SchemaValidator::new(pool);

        // "users" is a core synapse table that must exist in any migrated database
        let exists = validator.validate_table_exists("users").await;
        assert!(exists.is_ok(), "validate_table_exists should not error");
        assert!(exists.unwrap(), "users table should exist in the test database");
    }

    #[tokio::test]
    async fn test_validate_table_exists_missing_table() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let validator = SchemaValidator::new(pool);

        let table_name = format!("nonexistent_table_{suffix}");
        let exists = validator.validate_table_exists(&table_name).await;
        assert!(exists.is_ok(), "validate_table_exists should not error for missing table");
        assert!(!exists.unwrap(), "UUID-suffixed table should not exist");
    }

    #[tokio::test]
    async fn test_validate_table_exists_multiple_known_tables() {
        let pool = test_pool().await;
        let validator = SchemaValidator::new(pool);

        // All tables in REQUIRED_TABLES that are core to any synapse deployment
        for table in ["users", "rooms", "devices", "access_tokens", "events"] {
            let exists = validator.validate_table_exists(table).await;
            assert!(exists.is_ok(), "validate_table_exists should not error for {table}");
            assert!(exists.unwrap(), "{table} table should exist");
        }
    }

    // ── validate_column_exists ─────────────────────────────────────────

    #[tokio::test]
    async fn test_validate_column_exists_known_column() {
        let pool = test_pool().await;
        let validator = SchemaValidator::new(pool);

        // users.user_id is a required column and core to synapse
        let exists = validator.validate_column_exists("users", "user_id").await;
        assert!(exists.is_ok(), "validate_column_exists should not error");
        assert!(exists.unwrap(), "users.user_id column should exist");
    }

    #[tokio::test]
    async fn test_validate_column_exists_missing_column() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let validator = SchemaValidator::new(pool);

        let col_name = format!("nonexistent_col_{suffix}");
        let exists = validator.validate_column_exists("users", &col_name).await;
        assert!(exists.is_ok(), "validate_column_exists should not error for missing column");
        assert!(!exists.unwrap(), "UUID-suffixed column should not exist on users table");
    }

    #[tokio::test]
    async fn test_validate_column_exists_missing_table() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let validator = SchemaValidator::new(pool);

        let table_name = format!("nonexistent_tbl_{suffix}");
        let exists = validator.validate_column_exists(&table_name, "user_id").await;
        assert!(exists.is_ok(), "validate_column_exists should not error for missing table");
        assert!(!exists.unwrap(), "column on non-existent table should not be found");
    }

    #[tokio::test]
    async fn test_validate_column_exists_multiple_required_columns() {
        let pool = test_pool().await;
        let validator = SchemaValidator::new(pool);

        // Check several columns from REQUIRED_COLUMNS
        let checks = [
            ("users", "user_id"),
            ("users", "password_hash"),
            ("rooms", "room_id"),
            ("rooms", "room_version"),
            ("devices", "device_id"),
            ("events", "event_id"),
        ];
        for (table, column) in &checks {
            let exists = validator.validate_column_exists(table, column).await;
            assert!(exists.is_ok(), "validate_column_exists should not error for {table}.{column}");
            assert!(exists.unwrap(), "{table}.{column} column should exist");
        }
    }

    // ── validate_all ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_validate_all_runs_without_error() {
        let pool = test_pool().await;
        let validator = SchemaValidator::new(pool);

        let result = validator.validate_all().await;
        assert!(result.is_ok(), "validate_all should not return an error");
    }

    #[tokio::test]
    async fn test_validate_all_result_structure() {
        let pool = test_pool().await;
        let validator = SchemaValidator::new(pool);

        let result = validator.validate_all().await.unwrap();

        // is_healthy should mirror is_valid
        assert_eq!(result.is_healthy, result.is_valid);

        // If the DB has all required tables/columns, the validator reports valid
        if result.is_valid {
            assert!(result.missing_tables.is_empty());
            assert!(result.missing_columns.is_empty());
        } else {
            // If invalid, there must be at least one missing table or column
            assert!(!result.missing_tables.is_empty() || !result.missing_columns.is_empty());
        }

        // missing_indexes and schema_info are always empty (validate_all does not populate them)
        assert!(result.missing_indexes.is_empty());
        assert!(result.schema_info.is_empty());
    }

    // ── validate_indexes ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_validate_indexes_returns_list() {
        let pool = test_pool().await;
        let validator = SchemaValidator::new(pool);

        let indexes = validator.validate_indexes().await;
        assert!(indexes.is_ok(), "validate_indexes should not error");

        let indexes = indexes.unwrap();
        // A properly migrated synapse database should have at least some indexes
        assert!(!indexes.is_empty(), "should have at least some indexes");

        // All entries should be non-empty strings
        for idx in &indexes {
            assert!(!idx.is_empty(), "index names should not be empty");
        }
    }

    #[tokio::test]
    async fn test_validate_indexes_includes_primary_keys() {
        let pool = test_pool().await;
        let validator = SchemaValidator::new(pool);

        let indexes = validator.validate_indexes().await.unwrap();

        // Primary key indexes should exist for core tables (pk_<table> convention)
        let has_users_pk = indexes.iter().any(|i| i == "pk_users");
        let has_rooms_pk = indexes.iter().any(|i| i == "pk_rooms");
        assert!(has_users_pk, "should have a primary key index named pk_users");
        assert!(has_rooms_pk, "should have a primary key index named pk_rooms");
    }

    // ── validate_required_tables ───────────────────────────────────────

    #[tokio::test]
    async fn test_validate_required_tables_all_known() {
        let pool = test_pool().await;
        let validator = SchemaValidator::new(pool);

        let missing = validator.validate_required_tables(&["users", "rooms", "devices"]).await;
        assert!(missing.is_ok(), "validate_required_tables should not error");
        assert!(missing.unwrap().is_empty(), "known tables should not be reported missing");
    }

    #[tokio::test]
    async fn test_validate_required_tables_detects_missing() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let validator = SchemaValidator::new(pool);

        let missing_table = format!("nonexistent_{suffix}");
        let tables: Vec<&str> = vec!["users", &missing_table];
        let missing = validator.validate_required_tables(&tables).await;
        assert!(missing.is_ok());

        let missing = missing.unwrap();
        assert_eq!(missing.len(), 1, "exactly one table should be reported missing");
        assert_eq!(missing[0], missing_table, "the missing table name should match");
    }

    #[tokio::test]
    async fn test_validate_required_tables_all_missing() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let validator = SchemaValidator::new(pool);

        let t1 = format!("nonesuch_a_{suffix}");
        let t2 = format!("nonesuch_b_{suffix}");
        let missing = validator.validate_required_tables(&[&t1, &t2]).await;
        assert!(missing.is_ok());

        let missing = missing.unwrap();
        assert_eq!(missing.len(), 2);
        assert!(missing.contains(&t1));
        assert!(missing.contains(&t2));
    }

    #[tokio::test]
    async fn test_validate_required_tables_empty_list() {
        let pool = test_pool().await;
        let validator = SchemaValidator::new(pool);

        let missing = validator.validate_required_tables(&[]).await;
        assert!(missing.is_ok());
        assert!(missing.unwrap().is_empty(), "empty list should return empty missing set");
    }

    // ── validate_required_columns ──────────────────────────────────────

    #[tokio::test]
    async fn test_validate_required_columns_all_known() {
        let pool = test_pool().await;
        let validator = SchemaValidator::new(pool);

        let missing = validator
            .validate_required_columns(&[("users", "user_id"), ("users", "password_hash"), ("rooms", "room_id")])
            .await;
        assert!(missing.is_ok(), "validate_required_columns should not error");
        assert!(missing.unwrap().is_empty(), "known columns should not be reported missing");
    }

    #[tokio::test]
    async fn test_validate_required_columns_detects_missing() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let validator = SchemaValidator::new(pool);

        let missing_col = format!("nonexistent_col_{suffix}");
        let missing = validator
            .validate_required_columns(&[("users", "user_id"), ("users", &missing_col)])
            .await;
        assert!(missing.is_ok());

        let missing = missing.unwrap();
        assert_eq!(missing.len(), 1, "exactly one column should be reported missing");
        assert_eq!(missing[0], format!("users.{missing_col}"), "the missing column descriptor should match");
    }

    #[tokio::test]
    async fn test_validate_required_columns_missing_table() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        let validator = SchemaValidator::new(pool);

        let missing_table = format!("nonesuch_tbl_{suffix}");
        let missing = validator
            .validate_required_columns(&[("users", "user_id"), (&missing_table, "id")])
            .await;
        assert!(missing.is_ok());

        let missing = missing.unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0], format!("{missing_table}.id"));
    }

    #[tokio::test]
    async fn test_validate_required_columns_empty_list() {
        let pool = test_pool().await;
        let validator = SchemaValidator::new(pool);

        let missing = validator.validate_required_columns(&[]).await;
        assert!(missing.is_ok());
        assert!(missing.unwrap().is_empty(), "empty list should return empty missing set");
    }

    // ── edge case: validate_table_exists with empty string ─────────────

    #[tokio::test]
    async fn test_validate_table_exists_empty_string() {
        let pool = test_pool().await;
        let validator = SchemaValidator::new(pool);

        let exists = validator.validate_table_exists("").await;
        assert!(exists.is_ok());
        assert!(!exists.unwrap(), "empty string should not match any table");
    }

    // ── Consistency: validate_all agrees with individual checks ────────

    #[tokio::test]
    async fn test_validate_all_consistent_with_individual_checks() {
        let pool = test_pool().await;
        let validator = SchemaValidator::new(pool);

        let users_exists = validator.validate_table_exists("users").await.unwrap();
        let user_id_exists = validator.validate_column_exists("users", "user_id").await.unwrap();

        let all_result = validator.validate_all().await.unwrap();

        if users_exists && user_id_exists {
            // If individual checks pass for these, they should not appear in the missing lists
            assert!(
                !all_result.missing_tables.contains(&"users".to_string()),
                "users table should not be in missing_tables if it exists"
            );
            assert!(
                !all_result.missing_columns.contains(&"users.user_id".to_string()),
                "users.user_id should not be in missing_columns if it exists"
            );
        }
    }

    // ── SchemaValidationResult serde round-trip ────────────────────────

    #[test]
    fn test_schema_validation_result_serde_roundtrip() {
        let original = SchemaValidationResult {
            is_valid: false,
            is_healthy: false,
            missing_tables: vec!["events".to_string(), "presence".to_string()],
            missing_columns: vec!["users.avatar_url".to_string()],
            missing_indexes: vec!["idx_foo".to_string()],
            schema_info: vec![TableSchemaInfo {
                table_name: "rooms".to_string(),
                missing_columns: vec!["topic".to_string()],
                missing_indexes: vec![],
                missing_constraints: vec![],
            }],
        };

        let json = serde_json::to_string(&original).expect("serialization should succeed");
        let roundtripped: SchemaValidationResult =
            serde_json::from_str(&json).expect("deserialization should succeed");

        assert_eq!(roundtripped.is_valid, original.is_valid);
        assert_eq!(roundtripped.is_healthy, original.is_healthy);
        assert_eq!(roundtripped.missing_tables, original.missing_tables);
        assert_eq!(roundtripped.missing_columns, original.missing_columns);
        assert_eq!(roundtripped.missing_indexes, original.missing_indexes);
        assert_eq!(roundtripped.schema_info.len(), original.schema_info.len());
        assert_eq!(roundtripped.schema_info[0].table_name, original.schema_info[0].table_name);
    }
}
