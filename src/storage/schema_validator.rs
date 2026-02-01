use sqlx::{Pool, Postgres};
use std::sync::Arc;
use tracing::info;

/// Stores information about a single table's schema validation results.
/// Contains expected columns, actual columns, and validation status.
#[derive(Debug, Clone)]
pub struct SchemaInfo {
    /// The name of the table being validated
    pub table_name: String,
    /// Columns expected by the application schema
    pub expected_columns: Vec<String>,
    /// Columns actually present in the database
    pub actual_columns: Vec<String>,
    /// Columns missing from the database
    pub missing_columns: Vec<String>,
    /// Extra columns in the database not in the schema
    pub extra_columns: Vec<String>,
    /// Whether the table schema is valid (no missing columns)
    pub is_valid: bool,
}

/// Result of validating all critical tables in the database.
/// Provides a comprehensive overview of database schema health.
#[derive(Debug, Clone)]
pub struct SchemaValidationResult {
    /// Overall health status of the database schema
    pub is_healthy: bool,
    /// Detailed validation info for each table
    pub schema_info: Vec<SchemaInfo>,
    /// Recommended repair actions for invalid tables
    pub repair_actions: Vec<String>,
    /// Errors encountered during validation
    pub errors: Vec<String>,
}

/// Validates database schema against expected structure.
/// Provides detailed information about missing or extra columns.
pub struct SchemaValidator {
    pool: Arc<Pool<Postgres>>,
}

impl SchemaValidator {
    /// Creates a new SchemaValidator with the given database pool.
    ///
    /// # Arguments
    /// * `pool` - Shared PostgreSQL connection pool
    ///
    /// # Returns
    /// A new SchemaValidator instance
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    /// Validates all critical tables in the database.
    ///
    /// Validates the following tables:
    /// - users
    /// - devices
    /// - rooms
    /// - room_memberships
    /// - room_events
    /// - private_sessions
    /// - private_messages
    /// - friends
    /// - friend_requests
    /// - blocked_users
    ///
    /// # Returns
    /// Result containing SchemaValidationResult with detailed validation info
    ///
    /// # Errors
    /// Returns sqlx::Error if database queries fail
    pub async fn validate_all(&self) -> Result<SchemaValidationResult, sqlx::Error> {
        let mut result = SchemaValidationResult {
            is_healthy: true,
            schema_info: Vec::new(),
            repair_actions: Vec::new(),
            errors: Vec::new(),
        };

        let critical_tables = vec![
            "users",
            "devices",
            "rooms",
            "room_memberships",
            "events",
            "private_sessions",
            "private_messages",
            "friends",
            "friend_requests",
            "blocked_users",
        ];

        for table in critical_tables {
            match self.validate_table(table).await {
                Ok(info) => {
                    let is_valid = info.is_valid;
                    let missing_columns = info.missing_columns.clone();
                    result.schema_info.push(info);
                    if !is_valid {
                        result.is_healthy = false;
                        result.repair_actions.extend(
                            missing_columns
                                .iter()
                                .map(|c| format!("ADD COLUMN {} TO {}", c, table)),
                        );
                    }
                }
                Err(e) => {
                    result.is_healthy = false;
                    result.errors.push(format!("{}: {}", table, e));
                }
            }
        }

        Ok(result)
    }

    async fn validate_table(&self, table_name: &str) -> Result<SchemaInfo, sqlx::Error> {
        let expected_columns = self.get_expected_columns(table_name);
        let actual_columns = self.get_actual_columns(table_name).await?;

        let missing_columns: Vec<String> = expected_columns
            .iter()
            .filter(|c| !actual_columns.contains(c))
            .cloned()
            .collect();

        let extra_columns: Vec<String> = actual_columns
            .iter()
            .filter(|c| !expected_columns.contains(c))
            .cloned()
            .collect();

        Ok(SchemaInfo {
            table_name: table_name.to_string(),
            expected_columns: expected_columns.clone(),
            actual_columns,
            missing_columns: missing_columns.clone(),
            extra_columns,
            is_valid: missing_columns.is_empty(),
        })
    }

    fn get_expected_columns(&self, table_name: &str) -> Vec<String> {
        match table_name {
            "users" => vec![
                "user_id".to_string(),
                "username".to_string(),
                "password_hash".to_string(),
                "displayname".to_string(),
                "avatar_url".to_string(),
                "is_admin".to_string(),
                "deactivated".to_string(),
                "is_guest".to_string(),
                "consent_version".to_string(),
                "appservice_id".to_string(),
                "user_type".to_string(),
                "shadow_banned".to_string(),
                "generation".to_string(),
                "invalid_update_ts".to_string(),
                "migration_state".to_string(),
                "creation_ts".to_string(),
                "updated_ts".to_string(),
            ],
            "devices" => vec![
                "device_id".to_string(),
                "user_id".to_string(),
                "display_name".to_string(),
                "created_ts".to_string(),
            ],
            "rooms" => vec![
                "room_id".to_string(),
                "creator".to_string(),
                "is_public".to_string(),
                "name".to_string(),
                "topic".to_string(),
                "avatar_url".to_string(),
                "creation_ts".to_string(),
                "last_activity_ts".to_string(),
            ],
            "room_memberships" => vec![
                "room_id".to_string(),
                "user_id".to_string(),
                "sender".to_string(),
                "membership".to_string(),
                "event_id".to_string(),
                "event_type".to_string(),
                "display_name".to_string(),
                "avatar_url".to_string(),
                "is_banned".to_string(),
                "invite_token".to_string(),
                "updated_ts".to_string(),
                "joined_ts".to_string(),
                "left_ts".to_string(),
                "reason".to_string(),
                "banned_by".to_string(),
                "ban_reason".to_string(),
                "ban_ts".to_string(),
                "join_reason".to_string(),
            ],
            "events" => vec![
                "event_id".to_string(),
                "room_id".to_string(),
                "user_id".to_string(),
                "sender".to_string(),
                "event_type".to_string(),
                "content".to_string(),
                "state_key".to_string(),
                "origin_server_ts".to_string(),
                "processed_ts".to_string(),
            ],
            "private_sessions" => vec![
                "id".to_string(),
                "user_id_1".to_string(),
                "user_id_2".to_string(),
                "session_type".to_string(),
                "encryption_key".to_string(),
                "created_ts".to_string(),
                "last_activity_ts".to_string(),
                "updated_ts".to_string(),
                "unread_count".to_string(),
            ],
            "private_messages" => vec![
                "id".to_string(),
                "session_id".to_string(),
                "sender_id".to_string(),
                "message_type".to_string(),
                "content".to_string(),
                "encrypted_content".to_string(),
                "read_by_receiver".to_string(),
                "created_ts".to_string(),
            ],
            "friends" => vec![
                "user_id".to_string(),
                "friend_id".to_string(),
                "created_ts".to_string(),
            ],
            "friend_requests" => vec![
                "id".to_string(),
                "from_user_id".to_string(),
                "to_user_id".to_string(),
                "message".to_string(),
                "status".to_string(),
                "created_ts".to_string(),
                "updated_ts".to_string(),
            ],
            "blocked_users" => vec![
                "user_id".to_string(),
                "blocked_user_id".to_string(),
                "reason".to_string(),
                "created_ts".to_string(),
            ],
            _ => vec![],
        }
    }

    async fn get_actual_columns(&self, table_name: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<String> = sqlx::query_scalar::<_, String>(
            r#"
            SELECT column_name FROM information_schema.columns
            WHERE table_name = $1
            ORDER BY ordinal_position
            "#,
        )
        .bind(table_name)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn repair_missing_columns(&self) -> Result<Vec<String>, sqlx::Error> {
        let mut repairs = Vec::new();
        let validation = self.validate_all().await?;

        for info in validation.schema_info {
            for column in &info.missing_columns {
                let repair = self.add_column(&info.table_name, column).await?;
                repairs.push(repair);
            }
        }

        Ok(repairs)
    }

    async fn add_column(&self, table_name: &str, column_name: &str) -> Result<String, sqlx::Error> {
        let column_def = self
            .get_column_definition(table_name, column_name)
            .map_err(sqlx::Error::Protocol)?;

        sqlx::query(&format!(
            "ALTER TABLE {} ADD COLUMN IF NOT EXISTS {} {}",
            table_name, column_name, column_def
        ))
        .execute(&*self.pool)
        .await?;

        let msg = format!("Added column {} to table {}", column_name, table_name);
        info!("{}", msg);
        Ok(msg)
    }

    fn get_column_definition(&self, table_name: &str, column_name: &str) -> Result<String, String> {
        match table_name {
            "private_sessions" => match column_name {
                "id" => Ok("VARCHAR(255)".to_string()),
                "user_id_1" => Ok("VARCHAR(255)".to_string()),
                "user_id_2" => Ok("VARCHAR(255)".to_string()),
                "session_type" => Ok("VARCHAR(50) DEFAULT 'direct'".to_string()),
                "encryption_key" => Ok("VARCHAR(255)".to_string()),
                "created_ts" => Ok("BIGINT NOT NULL".to_string()),
                "last_activity_ts" => Ok("BIGINT".to_string()),
                "updated_ts" => Ok("BIGINT".to_string()),
                "unread_count" => Ok("INT DEFAULT 0".to_string()),
                _ => Err(format!("Unknown column: {}", column_name)),
            },
            "private_messages" => match column_name {
                "id" => Ok("BIGSERIAL PRIMARY KEY".to_string()),
                "session_id" => Ok("VARCHAR(255)".to_string()),
                "sender_id" => Ok("VARCHAR(255)".to_string()),
                "message_type" => Ok("VARCHAR(50) DEFAULT 'text'".to_string()),
                "content" => Ok("TEXT".to_string()),
                "encrypted_content" => Ok("TEXT".to_string()),
                "read_by_receiver" => Ok("BOOLEAN DEFAULT FALSE".to_string()),
                "created_ts" => Ok("BIGINT NOT NULL".to_string()),
                _ => Err(format!("Unknown column: {}", column_name)),
            },
            _ => Err(format!("Unknown table: {}", table_name)),
        }
    }

    pub async fn validate_indexes(&self) -> Result<Vec<String>, sqlx::Error> {
        let mut issues = Vec::new();

        let required_indexes = vec![
            ("private_sessions", "idx_private_sessions_user1"),
            ("private_sessions", "idx_private_sessions_user2"),
            ("private_sessions", "idx_private_sessions_activity"),
            ("private_messages", "idx_private_messages_session"),
            ("private_messages", "idx_private_messages_sender"),
            ("friends", "idx_friends_user"),
            ("friends", "idx_friends_friend"),
            ("friend_requests", "idx_friend_requests_target"),
        ];

        for (table, index_name) in required_indexes {
            let exists = self.index_exists(index_name).await?;
            if !exists {
                let issue = format!("Missing index: {} on table {}", index_name, table);
                info!("{}", issue);
                issues.push(issue);
            }
        }

        Ok(issues)
    }

    async fn index_exists(&self, index_name: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar::<_, i32>(
            r#"
            SELECT 1 as "exists" FROM pg_indexes WHERE indexname = $1
            "#,
        )
        .bind(index_name)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.is_some())
    }

    pub async fn create_missing_indexes(&self) -> Result<Vec<String>, sqlx::Error> {
        let mut created = Vec::new();

        let index_creations = vec![
            ("idx_private_sessions_user1", "CREATE INDEX IF NOT EXISTS idx_private_sessions_user1 ON private_sessions(user_id_1)"),
            ("idx_private_sessions_user2", "CREATE INDEX IF NOT EXISTS idx_private_sessions_user2 ON private_sessions(user_id_2)"),
            ("idx_private_sessions_activity", "CREATE INDEX IF NOT EXISTS idx_private_sessions_activity ON private_sessions(last_activity_ts DESC)"),
            ("idx_private_messages_session", "CREATE INDEX IF NOT EXISTS idx_private_messages_session ON private_messages(session_id)"),
            ("idx_private_messages_sender", "CREATE INDEX IF NOT EXISTS idx_private_messages_sender ON private_messages(sender_id)"),
            ("idx_friends_user", "CREATE INDEX IF NOT EXISTS idx_friends_user ON friends(user_id)"),
            ("idx_friends_friend", "CREATE INDEX IF NOT EXISTS idx_friends_friend ON friends(friend_id)"),
            ("idx_friend_requests_target", "CREATE INDEX IF NOT EXISTS idx_friend_requests_target ON friend_requests(to_user_id)"),
        ];

        for (name, index_query) in index_creations {
            let exists = self.index_exists(name).await?;
            if !exists {
                sqlx::query(index_query).execute(&*self.pool).await?;
                let msg = format!("Created index: {}", name);
                info!("{}", msg);
                created.push(msg);
            }
        }

        Ok(created)
    }
}
