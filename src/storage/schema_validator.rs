use sqlx::{Pool, Postgres};
use std::collections::HashMap;
use std::sync::Arc;

struct TableContract {
    columns: &'static [&'static str],
    indexes: &'static [&'static str],
    constraints: &'static [&'static str],
}

const TABLE_CONTRACTS: &[(&str, TableContract)] = &[
    (
        "room_summary_state",
        TableContract {
            columns: &[
                "room_id",
                "event_type",
                "state_key",
                "event_id",
                "content",
                "updated_ts",
            ],
            indexes: &["idx_room_summary_state_room"],
            constraints: &[
                "uq_room_summary_state_room_type_state",
                "fk_room_summary_state_room",
            ],
        },
    ),
    (
        "room_summary_stats",
        TableContract {
            columns: &[
                "room_id",
                "total_events",
                "total_state_events",
                "total_messages",
                "total_media",
                "storage_size",
                "last_updated_ts",
            ],
            indexes: &[],
            constraints: &["fk_room_summary_stats_room"],
        },
    ),
    (
        "room_summary_update_queue",
        TableContract {
            columns: &[
                "room_id",
                "event_id",
                "event_type",
                "state_key",
                "priority",
                "status",
                "created_ts",
                "processed_ts",
                "error_message",
                "retry_count",
            ],
            indexes: &["idx_room_summary_update_queue_status_priority_created"],
            constraints: &["fk_room_summary_update_queue_room"],
        },
    ),
    (
        "retention_cleanup_queue",
        TableContract {
            columns: &[
                "room_id",
                "event_id",
                "event_type",
                "origin_server_ts",
                "scheduled_ts",
                "status",
                "created_ts",
                "processed_ts",
                "error_message",
                "retry_count",
            ],
            indexes: &["idx_retention_cleanup_queue_status_origin"],
            constraints: &[
                "uq_retention_cleanup_queue_room_event",
                "fk_retention_cleanup_queue_room",
            ],
        },
    ),
    (
        "retention_cleanup_logs",
        TableContract {
            columns: &[
                "room_id",
                "events_deleted",
                "state_events_deleted",
                "media_deleted",
                "bytes_freed",
                "started_ts",
                "completed_ts",
                "status",
                "error_message",
            ],
            indexes: &["idx_retention_cleanup_logs_room_started"],
            constraints: &["fk_retention_cleanup_logs_room"],
        },
    ),
    (
        "retention_stats",
        TableContract {
            columns: &[
                "room_id",
                "total_events",
                "events_in_retention",
                "events_expired",
                "last_cleanup_ts",
                "next_cleanup_ts",
            ],
            indexes: &[],
            constraints: &["fk_retention_stats_room"],
        },
    ),
    (
        "deleted_events_index",
        TableContract {
            columns: &["room_id", "event_id", "deletion_ts", "reason"],
            indexes: &["idx_deleted_events_index_room_ts"],
            constraints: &[
                "uq_deleted_events_index_room_event",
                "fk_deleted_events_index_room",
            ],
        },
    ),
    (
        "device_trust_status",
        TableContract {
            columns: &[
                "user_id",
                "device_id",
                "trust_level",
                "verified_by_device_id",
                "verified_at",
                "created_ts",
                "updated_ts",
            ],
            indexes: &["idx_device_trust_status_user_level"],
            constraints: &["uq_device_trust_status_user_device"],
        },
    ),
    (
        "cross_signing_trust",
        TableContract {
            columns: &[
                "user_id",
                "target_user_id",
                "master_key_id",
                "is_trusted",
                "trusted_at",
                "created_ts",
                "updated_ts",
            ],
            indexes: &["idx_cross_signing_trust_user_trusted"],
            constraints: &["uq_cross_signing_trust_user_target"],
        },
    ),
    (
        "device_verification_request",
        TableContract {
            columns: &[
                "user_id",
                "new_device_id",
                "requesting_device_id",
                "verification_method",
                "status",
                "request_token",
                "commitment",
                "pubkey",
                "created_ts",
                "expires_at",
                "completed_at",
            ],
            indexes: &[
                "idx_device_verification_request_user_device_pending",
                "idx_device_verification_request_expires_pending",
            ],
            constraints: &[],
        },
    ),
    (
        "verification_requests",
        TableContract {
            columns: &[
                "transaction_id",
                "from_user",
                "from_device",
                "to_user",
                "to_device",
                "method",
                "state",
                "created_ts",
                "updated_ts",
            ],
            indexes: &["idx_verification_requests_to_user_state"],
            constraints: &[],
        },
    ),
    (
        "verification_sas",
        TableContract {
            columns: &[
                "tx_id",
                "from_device",
                "to_device",
                "method",
                "state",
                "exchange_hashes",
                "commitment",
                "pubkey",
                "sas_bytes",
                "mac",
            ],
            indexes: &[],
            constraints: &[],
        },
    ),
    (
        "verification_qr",
        TableContract {
            columns: &[
                "tx_id",
                "from_device",
                "to_device",
                "state",
                "qr_code_data",
                "scanned_data",
            ],
            indexes: &[],
            constraints: &[],
        },
    ),
];

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

#[derive(Debug, Clone, Default)]
pub struct TableSchemaInfo {
    pub table_name: String,
    pub missing_columns: Vec<String>,
    pub missing_indexes: Vec<String>,
    pub missing_constraints: Vec<String>,
}

impl SchemaValidator {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    pub async fn get_table_columns(
        &self,
        table_name: &str,
    ) -> Result<Vec<ColumnInfo>, sqlx::Error> {
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

    pub async fn validate_column_exists(
        &self,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool, sqlx::Error> {
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

    pub async fn get_table_indexes(&self) -> Result<HashMap<String, Vec<String>>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String)>(
            r#"
            SELECT tablename, indexname
            FROM pg_indexes
            WHERE schemaname = 'public'
            ORDER BY tablename, indexname
            "#,
        )
        .fetch_all(&*self.pool)
        .await?;

        let mut index_map: HashMap<String, Vec<String>> = HashMap::new();
        for (table_name, index_name) in rows {
            index_map.entry(table_name).or_default().push(index_name);
        }

        Ok(index_map)
    }

    pub async fn get_table_constraints(&self) -> Result<HashMap<String, Vec<String>>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String)>(
            r#"
            SELECT tc.table_name, tc.constraint_name
            FROM information_schema.table_constraints tc
            WHERE tc.table_schema = 'public'
            ORDER BY tc.table_name, tc.constraint_name
            "#,
        )
        .fetch_all(&*self.pool)
        .await?;

        let mut constraint_map: HashMap<String, Vec<String>> = HashMap::new();
        for (table_name, constraint_name) in rows {
            constraint_map
                .entry(table_name)
                .or_default()
                .push(constraint_name);
        }

        Ok(constraint_map)
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

    pub async fn check_schema_consistency(&self) -> Result<SchemaConsistencyReport, sqlx::Error> {
        let mut report = SchemaConsistencyReport::default();

        let required_tables = vec![
            "users",
            "rooms",
            "events",
            "devices",
            "access_tokens",
            "presence",
            "federation_signing_keys",
            "notifications",
            "room_memberships",
            "account_data",
            "push_rules",
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
        let table_names = self.get_all_tables().await?;
        let table_columns = self.get_schema_map().await?;
        let table_indexes = self.get_table_indexes().await?;
        let table_constraints = self.get_table_constraints().await?;

        let mut schema_info = Vec::new();
        let mut missing_tables = report.missing_tables;
        let mut missing_columns = report.missing_columns;
        let mut missing_indexes = Vec::new();

        for (table_name, contract) in TABLE_CONTRACTS {
            if !table_names.iter().any(|name| name == table_name) {
                missing_tables.push((*table_name).to_string());
                schema_info.push(TableSchemaInfo {
                    table_name: (*table_name).to_string(),
                    missing_columns: contract.columns.iter().map(|c| (*c).to_string()).collect(),
                    missing_indexes: contract.indexes.iter().map(|i| (*i).to_string()).collect(),
                    missing_constraints: contract
                        .constraints
                        .iter()
                        .map(|c| (*c).to_string())
                        .collect(),
                });
                continue;
            }

            let actual_columns = table_columns.get(*table_name);
            let actual_indexes = table_indexes.get(*table_name);
            let actual_constraints = table_constraints.get(*table_name);

            let table_missing_columns: Vec<String> = contract
                .columns
                .iter()
                .filter(|column| {
                    !actual_columns
                        .map(|columns| columns.iter().any(|name| name == **column))
                        .unwrap_or(false)
                })
                .map(|column| (*column).to_string())
                .collect();

            let table_missing_indexes: Vec<String> = contract
                .indexes
                .iter()
                .filter(|index| {
                    !actual_indexes
                        .map(|indexes| indexes.iter().any(|name| name == **index))
                        .unwrap_or(false)
                })
                .map(|index| (*index).to_string())
                .collect();

            let table_missing_constraints: Vec<String> = contract
                .constraints
                .iter()
                .filter(|constraint| {
                    !actual_constraints
                        .map(|constraints| constraints.iter().any(|name| name == **constraint))
                        .unwrap_or(false)
                })
                .map(|constraint| (*constraint).to_string())
                .collect();

            missing_columns.extend(
                table_missing_columns
                    .iter()
                    .map(|column| format!("{}.{}", table_name, column)),
            );
            missing_indexes.extend(table_missing_indexes.iter().cloned());

            if !table_missing_columns.is_empty()
                || !table_missing_indexes.is_empty()
                || !table_missing_constraints.is_empty()
            {
                schema_info.push(TableSchemaInfo {
                    table_name: (*table_name).to_string(),
                    missing_columns: table_missing_columns,
                    missing_indexes: table_missing_indexes,
                    missing_constraints: table_missing_constraints,
                });
            }
        }

        missing_tables.sort();
        missing_tables.dedup();
        missing_columns.sort();
        missing_columns.dedup();
        missing_indexes.sort();
        missing_indexes.dedup();

        let is_valid =
            missing_tables.is_empty() && missing_columns.is_empty() && missing_indexes.is_empty();

        Ok(SchemaValidationResult {
            is_valid,
            is_healthy: is_valid,
            missing_tables,
            missing_columns,
            missing_indexes,
            schema_info,
        })
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
            ("rooms", "last_activity_ts", "BIGINT"),
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

    #[cfg(feature = "runtime-ddl")]
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
                sqlx::query(&sql).execute(&*self.pool).await?;
                created.push(index_name.to_string());
            }
        }

        Ok(created)
    }

    #[cfg(feature = "runtime-ddl")]
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

impl std::fmt::Display for SchemaConsistencyReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_valid {
            write!(f, "Schema is consistent")
        } else {
            if !self.missing_tables.is_empty() {
                writeln!(f, "Missing tables: {}", self.missing_tables.join(", "))?;
            }
            if !self.missing_columns.is_empty() {
                writeln!(f, "Missing columns: {}", self.missing_columns.join(", "))?;
            }
            Ok(())
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

    #[test]
    fn test_column_info_creation() {
        let column = ColumnInfo {
            column_name: "id".to_string(),
            data_type: "bigint".to_string(),
            is_nullable: "NO".to_string(),
        };
        assert_eq!(column.column_name, "id");
        assert_eq!(column.data_type, "bigint");
    }

    #[test]
    fn test_table_schema_creation() {
        let schema = TableSchema {
            table_name: "users".to_string(),
            columns: vec![
                ColumnInfo {
                    column_name: "id".to_string(),
                    data_type: "bigint".to_string(),
                    is_nullable: "NO".to_string(),
                },
                ColumnInfo {
                    column_name: "name".to_string(),
                    data_type: "varchar".to_string(),
                    is_nullable: "YES".to_string(),
                },
            ],
        };
        assert_eq!(schema.table_name, "users");
        assert_eq!(schema.columns.len(), 2);
    }

    #[test]
    fn test_table_schema_info_creation() {
        let info = TableSchemaInfo {
            table_name: "events".to_string(),
            missing_columns: vec!["email".to_string()],
            missing_indexes: vec![],
            missing_constraints: vec![],
        };
        assert_eq!(info.table_name, "events");
    }

    #[test]
    fn test_schema_consistency_report_default() {
        let report = SchemaConsistencyReport::default();
        assert!(report.missing_tables.is_empty());
        assert!(report.missing_columns.is_empty());
    }

    #[test]
    fn test_schema_consistency_report_with_issues() {
        let report = SchemaConsistencyReport {
            is_valid: false,
            missing_tables: vec!["missing_table".to_string()],
            missing_columns: vec!["users:email".to_string()],
        };
        assert!(!report.is_valid);
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
    fn test_schema_validation_result_with_errors() {
        let result = SchemaValidationResult {
            is_valid: false,
            is_healthy: false,
            missing_tables: vec!["missing_table".to_string()],
            missing_columns: vec!["users:email".to_string()],
            missing_indexes: vec!["events:event_id".to_string()],
            schema_info: vec![],
        };
        assert!(!result.is_valid);
    }
}
