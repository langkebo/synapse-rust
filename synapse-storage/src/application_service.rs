use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Row};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApplicationService {
    pub id: i64,
    pub as_id: String,
    pub url: String,
    #[serde(skip_serializing)]
    pub as_token: String,
    #[serde(skip_serializing)]
    pub hs_token: String,
    #[serde(rename = "sender")]
    #[sqlx(rename = "sender_localpart")]
    pub sender_localpart: String,
    pub is_enabled: bool,
    pub is_rate_limited: bool,
    pub protocols: Vec<String>,
    pub namespaces: serde_json::Value,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub description: Option<String>,
    #[serde(skip_serializing)]
    pub api_key: Option<String>,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApplicationServiceState {
    pub as_id: String,
    pub state_key: String,
    pub state_value: String,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApplicationServiceEvent {
    pub event_id: String,
    pub as_id: String,
    pub room_id: String,
    pub event_type: String,
    pub sender: String,
    pub content: serde_json::Value,
    pub state_key: Option<String>,
    pub origin_server_ts: i64,
    pub processed_ts: Option<i64>,
    pub transaction_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApplicationServiceTransaction {
    pub id: i64,
    pub as_id: String,
    pub txn_id: String,
    pub transaction_id: Option<String>,
    pub events: serde_json::Value,
    pub sent_ts: i64,
    pub completed_ts: Option<i64>,
    pub retry_count: i32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApplicationServiceNamespace {
    pub id: i64,
    pub as_id: String,
    pub namespace_pattern: String,
    pub is_exclusive: bool,
    pub regex: String,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApplicationServiceUser {
    pub as_id: String,
    pub user_id: String,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterApplicationServiceRequest {
    pub as_id: String,
    pub url: String,
    pub as_token: String,
    pub hs_token: String,
    pub sender: String,
    pub description: Option<String>,
    pub is_rate_limited: Option<bool>,
    pub protocols: Option<Vec<String>>,
    pub namespaces: Option<serde_json::Value>,
    pub api_key: Option<String>,
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateApplicationServiceRequest {
    pub url: Option<String>,
    pub description: Option<String>,
    pub is_rate_limited: Option<bool>,
    pub protocols: Option<Vec<String>>,
    pub is_enabled: Option<bool>,
    pub api_key: Option<String>,
    pub config: Option<serde_json::Value>,
}

impl UpdateApplicationServiceRequest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn is_rate_limited(mut self, is_rate_limited: bool) -> Self {
        self.is_rate_limited = Some(is_rate_limited);
        self
    }

    pub fn protocols(mut self, protocols: Vec<String>) -> Self {
        self.protocols = Some(protocols);
        self
    }

    pub fn is_enabled(mut self, is_enabled: bool) -> Self {
        self.is_enabled = Some(is_enabled);
        self
    }

    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn config(mut self, config: serde_json::Value) -> Self {
        self.config = Some(config);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Namespaces {
    pub users: Vec<NamespaceRule>,
    pub aliases: Vec<NamespaceRule>,
    pub rooms: Vec<NamespaceRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceRule {
    pub is_exclusive: bool,
    pub regex: String,
    #[serde(default)]
    pub group_id: Option<String>,
}

#[derive(Clone)]
pub struct ApplicationServiceStorage {
    pool: Arc<PgPool>,
}

impl ApplicationServiceStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn register(
        &self,
        request: RegisterApplicationServiceRequest,
    ) -> Result<ApplicationService, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let protocols = request.protocols.clone().unwrap_or_default();
        let namespaces = request.namespaces.unwrap_or(serde_json::json!({
            "users": [],
            "aliases": [],
            "rooms": []
        }));
        let config = request.config.unwrap_or(serde_json::json!({}));

        let service = sqlx::query_as::<_, ApplicationService>(
            r"
            INSERT INTO application_services (
                as_id, url, as_token, hs_token, sender_localpart, is_enabled,
                is_rate_limited, protocols, namespaces, created_ts, description, api_key, config
            )
            VALUES ($1, $2, $3, $4, $5, TRUE, $6, $7, $8, $9, $10, $11, $12)
            RETURNING *
            ",
        )
        .bind(&request.as_id)
        .bind(&request.url)
        .bind(&request.as_token)
        .bind(&request.hs_token)
        .bind(&request.sender)
        .bind(request.is_rate_limited.unwrap_or(false))
        .bind(&protocols)
        .bind(&namespaces)
        .bind(now)
        .bind(&request.description)
        .bind(&request.api_key)
        .bind(&config)
        .fetch_one(&*self.pool)
        .await?;

        self.insert_namespaces(&service).await?;

        Ok(service)
    }

    pub async fn upsert_registration(
        &self,
        request: RegisterApplicationServiceRequest,
    ) -> Result<ApplicationService, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let protocols = request.protocols.clone().unwrap_or_default();
        let namespaces = request.namespaces.unwrap_or(serde_json::json!({
            "users": [],
            "aliases": [],
            "rooms": []
        }));
        let config = request.config.unwrap_or(serde_json::json!({}));

        let service = sqlx::query_as::<_, ApplicationService>(
            r"
            INSERT INTO application_services (
                as_id, url, as_token, hs_token, sender_localpart, is_enabled,
                is_rate_limited, protocols, namespaces, created_ts, description, api_key, config
            )
            VALUES ($1, $2, $3, $4, $5, TRUE, $6, $7, $8, $9, $10, $11, $12)
            ON CONFLICT (as_id) DO UPDATE SET
                url = EXCLUDED.url,
                as_token = EXCLUDED.as_token,
                hs_token = EXCLUDED.hs_token,
                sender_localpart = EXCLUDED.sender_localpart,
                is_enabled = TRUE,
                is_rate_limited = EXCLUDED.is_rate_limited,
                protocols = EXCLUDED.protocols,
                namespaces = EXCLUDED.namespaces,
                description = EXCLUDED.description,
                api_key = EXCLUDED.api_key,
                config = EXCLUDED.config,
                updated_ts = $13
            RETURNING *
            ",
        )
        .bind(&request.as_id)
        .bind(&request.url)
        .bind(&request.as_token)
        .bind(&request.hs_token)
        .bind(&request.sender)
        .bind(request.is_rate_limited.unwrap_or(false))
        .bind(&protocols)
        .bind(&namespaces)
        .bind(now)
        .bind(&request.description)
        .bind(&request.api_key)
        .bind(&config)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        self.clear_namespaces(&service.as_id).await?;
        self.insert_namespaces(&service).await?;

        Ok(service)
    }

    async fn insert_namespaces(&self, service: &ApplicationService) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        if let Some(users) = service.namespaces.get("users").and_then(|v| v.as_array()) {
            for rule in users {
                if let (Some(regex), Some(exclusive)) =
                    (rule.get("regex").and_then(|r| r.as_str()), rule.get("exclusive").and_then(|e| e.as_bool()))
                {
                    sqlx::query(
                        r"
                        INSERT INTO application_service_user_namespaces (as_id, namespace, is_exclusive, created_ts)
                        VALUES ($1, $2, $3, $4)
                        ",
                    )
                    .bind(&service.as_id)
                    .bind(regex)
                    .bind(exclusive)
                    .bind(now)
                    .execute(&*self.pool)
                    .await?;
                }
            }
        }

        if let Some(aliases) = service.namespaces.get("aliases").and_then(|v| v.as_array()) {
            for rule in aliases {
                if let (Some(regex), Some(exclusive)) =
                    (rule.get("regex").and_then(|r| r.as_str()), rule.get("exclusive").and_then(|e| e.as_bool()))
                {
                    sqlx::query(
                        r"
                        INSERT INTO application_service_room_alias_namespaces (as_id, namespace, is_exclusive, created_ts)
                        VALUES ($1, $2, $3, $4)
                        ",
                    )
                    .bind(&service.as_id)
                    .bind(regex)
                    .bind(exclusive)
                    .bind(now)
                    .execute(&*self.pool)
                    .await?;
                }
            }
        }

        if let Some(rooms) = service.namespaces.get("rooms").and_then(|v| v.as_array()) {
            for rule in rooms {
                if let (Some(regex), Some(exclusive)) =
                    (rule.get("regex").and_then(|r| r.as_str()), rule.get("exclusive").and_then(|e| e.as_bool()))
                {
                    sqlx::query(
                        r"
                        INSERT INTO application_service_room_namespaces (as_id, namespace, is_exclusive, created_ts)
                        VALUES ($1, $2, $3, $4)
                        ",
                    )
                    .bind(&service.as_id)
                    .bind(regex)
                    .bind(exclusive)
                    .bind(now)
                    .execute(&*self.pool)
                    .await?;
                }
            }
        }

        Ok(())
    }

    async fn clear_namespaces(&self, as_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(r"DELETE FROM application_service_user_namespaces WHERE as_id = $1")
            .bind(as_id)
            .execute(&*self.pool)
            .await?;
        sqlx::query(r"DELETE FROM application_service_room_alias_namespaces WHERE as_id = $1")
            .bind(as_id)
            .execute(&*self.pool)
            .await?;
        sqlx::query(r"DELETE FROM application_service_room_namespaces WHERE as_id = $1")
            .bind(as_id)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    pub async fn get_by_id(&self, as_id: &str) -> Result<Option<ApplicationService>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationService>(r"SELECT id, as_id, url, as_token, hs_token, sender_localpart, is_enabled, is_rate_limited, protocols, namespaces, created_ts, updated_ts, description, api_key, config FROM application_services WHERE as_id = $1")
            .bind(as_id)
            .fetch_optional(&*self.pool)
            .await
    }

    pub async fn get_by_token(&self, as_token: &str) -> Result<Option<ApplicationService>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationService>(
            r"SELECT id, as_id, url, as_token, hs_token, sender_localpart, is_enabled, is_rate_limited, protocols, namespaces, created_ts, updated_ts, description, api_key, config FROM application_services WHERE as_token = $1 AND is_enabled = TRUE",
        )
        .bind(as_token)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_by_hs_token(&self, hs_token: &str) -> Result<Option<ApplicationService>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationService>(
            r"SELECT id, as_id, url, as_token, hs_token, sender_localpart, is_enabled, is_rate_limited, protocols, namespaces, created_ts, updated_ts, description, api_key, config FROM application_services WHERE hs_token = $1 AND is_enabled = TRUE",
        )
        .bind(hs_token)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_all_active(&self) -> Result<Vec<ApplicationService>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationService>(
            r"SELECT id, as_id, url, as_token, hs_token, sender_localpart, is_enabled, is_rate_limited, protocols, namespaces, created_ts, updated_ts, description, api_key, config FROM application_services WHERE is_enabled = TRUE ORDER BY created_ts DESC",
        )
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn update(
        &self,
        as_id: &str,
        request: &UpdateApplicationServiceRequest,
    ) -> Result<ApplicationService, sqlx::Error> {
        let protocols = request.protocols.clone();
        let config = request.config.clone();
        sqlx::query_as::<_, ApplicationService>(
            r"
            UPDATE application_services SET
                url = COALESCE($2, url),
                description = COALESCE($3, description),
                is_rate_limited = COALESCE($4, is_rate_limited),
                protocols = COALESCE($5::text[], protocols),
                is_enabled = COALESCE($6, is_enabled),
                api_key = COALESCE($7, api_key),
                config = COALESCE($8::jsonb, config),
                updated_ts = $9
            WHERE as_id = $1
            RETURNING *
            ",
        )
        .bind(as_id)
        .bind(&request.url)
        .bind(&request.description)
        .bind(request.is_rate_limited)
        .bind(protocols)
        .bind(request.is_enabled)
        .bind(&request.api_key)
        .bind(&config)
        .bind(chrono::Utc::now().timestamp_millis())
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn update_timestamp(&self, as_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(r"UPDATE application_services SET updated_ts = $2 WHERE as_id = $1")
            .bind(as_id)
            .bind(now)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn unregister(&self, as_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(r"DELETE FROM application_services WHERE as_id = $1").bind(as_id).execute(&*self.pool).await?;
        Ok(())
    }

    pub async fn set_state(
        &self,
        as_id: &str,
        state_key: &str,
        state_value: &str,
    ) -> Result<ApplicationServiceState, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        sqlx::query_as::<_, ApplicationServiceState>(
            r"
            INSERT INTO application_service_state (as_id, state_key, value, state_value, updated_ts)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (as_id, state_key) DO UPDATE SET
                value = EXCLUDED.value,
                state_value = EXCLUDED.state_value,
                updated_ts = EXCLUDED.updated_ts
            RETURNING
                as_id,
                state_key,
                COALESCE(state_value, value #>> '{}') AS state_value,
                updated_ts
            ",
        )
        .bind(as_id)
        .bind(state_key)
        .bind(serde_json::json!(state_value))
        .bind(state_value)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_state(
        &self,
        as_id: &str,
        state_key: &str,
    ) -> Result<Option<ApplicationServiceState>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationServiceState>(
            r"
            SELECT
                as_id,
                state_key,
                COALESCE(state_value, value #>> '{}') AS state_value,
                updated_ts
            FROM application_service_state
            WHERE as_id = $1 AND state_key = $2
            ",
        )
        .bind(as_id)
        .bind(state_key)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_all_states(&self, as_id: &str) -> Result<Vec<ApplicationServiceState>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationServiceState>(
            r"
            SELECT
                as_id,
                state_key,
                COALESCE(state_value, value #>> '{}') AS state_value,
                updated_ts
            FROM application_service_state
            WHERE as_id = $1
            ",
        )
        .bind(as_id)
        .fetch_all(&*self.pool)
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn add_event(
        &self,
        event_id: &str,
        as_id: &str,
        room_id: &str,
        event_type: &str,
        _sender: &str,
        _content: serde_json::Value,
        _state_key: Option<&str>,
    ) -> Result<ApplicationServiceEvent, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        sqlx::query_as::<_, ApplicationServiceEvent>(
            r"
            INSERT INTO application_service_events (
                event_id, as_id, room_id, event_type, is_processed, processed_ts, created_ts
            )
            VALUES ($1, $2, $3, $4, FALSE, NULL, $5)
            ON CONFLICT (event_id) DO UPDATE SET
                as_id = EXCLUDED.as_id,
                room_id = EXCLUDED.room_id,
                event_type = EXCLUDED.event_type
            RETURNING
                event_id,
                as_id,
                room_id,
                event_type,
                ''::text AS sender,
                '{}'::jsonb AS content,
                NULL::text AS state_key,
                created_ts AS origin_server_ts,
                processed_ts,
                NULL::text AS transaction_id
            ",
        )
        .bind(event_id)
        .bind(as_id)
        .bind(room_id)
        .bind(event_type)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_pending_events(
        &self,
        as_id: &str,
        limit: i64,
    ) -> Result<Vec<ApplicationServiceEvent>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationServiceEvent>(
            r"
            SELECT
                event_id,
                as_id,
                room_id,
                event_type,
                ''::text AS sender,
                '{}'::jsonb AS content,
                NULL::text AS state_key,
                created_ts AS origin_server_ts,
                processed_ts,
                NULL::text AS transaction_id
            FROM application_service_events
            WHERE as_id = $1 AND is_processed = FALSE
            ORDER BY created_ts ASC
            LIMIT $2
            ",
        )
        .bind(as_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn count_pending_events(&self, as_id: &str) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, i64>(
            r"
            SELECT COUNT(*)
            FROM application_service_events
            WHERE as_id = $1 AND is_processed = FALSE
            ",
        )
        .bind(as_id)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn mark_event_processed(&self, event_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        sqlx::query(
            r"UPDATE application_service_events SET is_processed = TRUE, processed_ts = $2 WHERE event_id = $1",
        )
        .bind(event_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn create_transaction(
        &self,
        as_id: &str,
        transaction_id: &str,
        events: &[serde_json::Value],
    ) -> Result<ApplicationServiceTransaction, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        sqlx::query_as::<_, ApplicationServiceTransaction>(
            r"
            INSERT INTO application_service_transactions (as_id, txn_id, transaction_id, events, sent_ts, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            ",
        )
        .bind(as_id)
        .bind(transaction_id)
        .bind(Some(transaction_id))
        .bind(serde_json::json!(events))
        .bind(now)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn complete_transaction(&self, as_id: &str, transaction_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        sqlx::query(
            r"UPDATE application_service_transactions SET completed_ts = $3, is_processed = TRUE, processed_ts = $3 WHERE as_id = $1 AND (txn_id = $2 OR transaction_id = $2)",
        )
        .bind(as_id)
        .bind(transaction_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn fail_transaction(
        &self,
        as_id: &str,
        transaction_id: &str,
        error: &str,
    ) -> Result<ApplicationServiceTransaction, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        sqlx::query_as::<_, ApplicationServiceTransaction>(
            r"
            UPDATE application_service_transactions
            SET retry_count = retry_count + 1, last_error = $3, sent_ts = $4
            WHERE as_id = $1 AND (txn_id = $2 OR transaction_id = $2)
            RETURNING *
            ",
        )
        .bind(as_id)
        .bind(transaction_id)
        .bind(error)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_pending_transactions(
        &self,
        as_id: &str,
    ) -> Result<Vec<ApplicationServiceTransaction>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationServiceTransaction>(
            r"SELECT id, as_id, txn_id, transaction_id, events, sent_ts, completed_ts, retry_count, last_error
              FROM application_service_transactions
              WHERE as_id = $1 AND completed_ts IS NULL ORDER BY sent_ts ASC",
        )
        .bind(as_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn count_pending_transactions(&self, as_id: &str) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, i64>(
            r"
            SELECT COUNT(*)
            FROM application_service_transactions
            WHERE as_id = $1 AND completed_ts IS NULL
            ",
        )
        .bind(as_id)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn register_virtual_user(
        &self,
        as_id: &str,
        user_id: &str,
        displayname: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<ApplicationServiceUser, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        sqlx::query_as::<_, ApplicationServiceUser>(
            r"
            INSERT INTO application_service_users (as_id, user_id, displayname, avatar_url, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (as_id, user_id) DO UPDATE SET
                displayname = COALESCE(EXCLUDED.displayname, application_service_users.displayname),
                avatar_url = COALESCE(EXCLUDED.avatar_url, application_service_users.avatar_url)
            RETURNING *
            ",
        )
        .bind(as_id)
        .bind(user_id)
        .bind(displayname)
        .bind(avatar_url)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_virtual_users(&self, as_id: &str) -> Result<Vec<ApplicationServiceUser>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationServiceUser>(r"SELECT as_id, user_id, displayname, avatar_url, created_ts FROM application_service_users WHERE as_id = $1")
            .bind(as_id)
            .fetch_all(&*self.pool)
            .await
    }

    pub async fn has_exclusive_user_namespace_match(&self, as_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        let matched = sqlx::query_scalar::<_, i64>(
            r"
            SELECT 1
            FROM application_service_user_namespaces
            WHERE as_id = $1
              AND is_exclusive = TRUE
              AND $2 ~ namespace
            LIMIT 1
            ",
        )
        .bind(as_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(matched.is_some())
    }

    pub async fn find_user_namespace_conflict(
        &self,
        as_id: &str,
        namespace_pattern: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query(
            r"
            SELECT as_id
            FROM application_service_user_namespaces
            WHERE namespace = $1
              AND is_exclusive = TRUE
              AND as_id <> $2
            LIMIT 1
            ",
        )
        .bind(namespace_pattern)
        .bind(as_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map(|row| row.get("as_id")))
    }

    pub async fn find_room_alias_namespace_conflict(
        &self,
        as_id: &str,
        namespace_pattern: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query(
            r"
            SELECT as_id
            FROM application_service_room_alias_namespaces
            WHERE namespace = $1
              AND is_exclusive = TRUE
              AND as_id <> $2
            LIMIT 1
            ",
        )
        .bind(namespace_pattern)
        .bind(as_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map(|row| row.get("as_id")))
    }

    pub async fn find_room_namespace_conflict(
        &self,
        as_id: &str,
        namespace_pattern: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query(
            r"
            SELECT as_id
            FROM application_service_room_namespaces
            WHERE namespace = $1
              AND is_exclusive = TRUE
              AND as_id <> $2
            LIMIT 1
            ",
        )
        .bind(namespace_pattern)
        .bind(as_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map(|row| row.get("as_id")))
    }

    pub async fn is_user_in_namespace(&self, user_id: &str) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query(
            r"
            SELECT as_id
            FROM application_service_user_namespaces
            WHERE $1 ~ namespace
            ORDER BY is_exclusive DESC, created_ts ASC
            LIMIT 1
            ",
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map(|row| row.get("as_id")))
    }

    pub async fn is_room_alias_in_namespace(&self, alias: &str) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query(
            r"
            SELECT as_id
            FROM application_service_room_alias_namespaces
            WHERE $1 ~ namespace
            ORDER BY is_exclusive DESC, created_ts ASC
            LIMIT 1
            ",
        )
        .bind(alias)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map(|row| row.get("as_id")))
    }

    pub async fn is_room_id_in_namespace(&self, room_id: &str) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query(
            r"
            SELECT as_id
            FROM application_service_room_namespaces
            WHERE $1 ~ namespace
            ORDER BY is_exclusive DESC, created_ts ASC
            LIMIT 1
            ",
        )
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map(|row| row.get("as_id")))
    }

    pub async fn get_user_namespaces(&self, as_id: &str) -> Result<Vec<ApplicationServiceNamespace>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationServiceNamespace>(
            r"
            SELECT
                id,
                as_id,
                namespace AS namespace_pattern,
                is_exclusive,
                namespace AS regex,
                created_ts
            FROM application_service_user_namespaces
            WHERE as_id = $1
            ",
        )
        .bind(as_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_room_alias_namespaces(
        &self,
        as_id: &str,
    ) -> Result<Vec<ApplicationServiceNamespace>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationServiceNamespace>(
            r"
            SELECT
                id,
                as_id,
                namespace AS namespace_pattern,
                is_exclusive,
                namespace AS regex,
                created_ts
            FROM application_service_room_alias_namespaces
            WHERE as_id = $1
            ",
        )
        .bind(as_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_room_namespaces(&self, as_id: &str) -> Result<Vec<ApplicationServiceNamespace>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationServiceNamespace>(
            r"
            SELECT
                id,
                as_id,
                namespace AS namespace_pattern,
                is_exclusive,
                namespace AS regex,
                created_ts
            FROM application_service_room_namespaces
            WHERE as_id = $1
            ",
        )
        .bind(as_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_statistics(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        sqlx::query(
            r#"
            WITH user_counts AS (
                SELECT as_id, COUNT(*) AS virtual_user_count
                FROM application_service_users
                GROUP BY as_id
            ),
            pending_event_counts AS (
                SELECT as_id, COUNT(*) AS pending_event_count
                FROM application_service_events
                WHERE is_processed = FALSE
                GROUP BY as_id
            ),
            pending_transaction_counts AS (
                SELECT as_id, COUNT(*) AS pending_transaction_count
                FROM application_service_transactions
                WHERE completed_ts IS NULL
                GROUP BY as_id
            )
            SELECT
                svc.id,
                svc.as_id,
                COALESCE(stats.name, svc.description) AS name,
                svc.is_enabled,
                svc.is_rate_limited,
                COALESCE(users.virtual_user_count, 0) AS virtual_user_count,
                COALESCE(events.pending_event_count, 0) AS pending_event_count,
                COALESCE(txns.pending_transaction_count, 0) AS pending_transaction_count,
                stats.last_seen_ts,
                svc.created_ts
            FROM application_services svc
            LEFT JOIN application_service_statistics stats
              ON stats.as_id = svc.as_id
            LEFT JOIN user_counts users
              ON users.as_id = svc.as_id
            LEFT JOIN pending_event_counts events
              ON events.as_id = svc.as_id
            LEFT JOIN pending_transaction_counts txns
              ON txns.as_id = svc.as_id
            ORDER BY svc.created_ts DESC
            "#,
        )
        .fetch_all(&*self.pool)
        .await
        .map(|rows| {
            rows.into_iter()
                .map(|row| {
                    serde_json::json!({
                        "id": row.get::<i64, _>("id"),
                        "as_id": row.get::<String, _>("as_id"),
                        "name": row.get::<Option<String>, _>("name"),
                        "is_enabled": row.get::<bool, _>("is_enabled"),
                        "is_rate_limited": row.get::<bool, _>("is_rate_limited"),
                        "virtual_user_count": row.get::<i64, _>("virtual_user_count"),
                        "pending_event_count": row.get::<i64, _>("pending_event_count"),
                        "pending_transaction_count": row.get::<i64, _>("pending_transaction_count"),
                        "last_seen_ts": row.get::<Option<i64>, _>("last_seen_ts"),
                        "created_ts": row.get::<i64, _>("created_ts"),
                    })
                })
                .collect()
        })
    }

    pub async fn update_last_seen(&self, as_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r"
            INSERT INTO application_service_statistics (
                as_id,
                name,
                is_enabled,
                is_rate_limited,
                virtual_user_count,
                pending_event_count,
                pending_transaction_count,
                last_seen_ts,
                created_ts
            )
            SELECT
                svc.as_id,
                svc.description,
                svc.is_enabled,
                svc.is_rate_limited,
                0,
                0,
                0,
                $2,
                svc.created_ts
            FROM application_services svc
            WHERE svc.as_id = $1
            ON CONFLICT (as_id) DO UPDATE
            SET last_seen_ts = EXCLUDED.last_seen_ts,
                is_enabled = EXCLUDED.is_enabled,
                is_rate_limited = EXCLUDED.is_rate_limited,
                name = COALESCE(application_service_statistics.name, EXCLUDED.name)
            ",
        )
        .bind(as_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_rule_serialization() {
        let rule = NamespaceRule {
            is_exclusive: true,
            regex: "@_.*:example.com".to_string(),
            group_id: Some("group:example.com".to_string()),
        };

        let json = serde_json::to_string(&rule).unwrap();
        let deserialized: NamespaceRule = serde_json::from_str(&json).unwrap();

        assert_eq!(rule.is_exclusive, deserialized.is_exclusive);
        assert_eq!(rule.regex, deserialized.regex);
        assert_eq!(rule.group_id, deserialized.group_id);
    }

    #[test]
    fn test_namespaces_serialization() {
        let namespaces = Namespaces {
            users: vec![NamespaceRule { is_exclusive: true, regex: "@_.*:example.com".to_string(), group_id: None }],
            aliases: vec![NamespaceRule { is_exclusive: false, regex: "#_.*:example.com".to_string(), group_id: None }],
            rooms: vec![],
        };

        let json = serde_json::to_string(&namespaces).unwrap();
        let deserialized: Namespaces = serde_json::from_str(&json).unwrap();

        assert_eq!(namespaces.users.len(), deserialized.users.len());
        assert_eq!(namespaces.aliases.len(), deserialized.aliases.len());
        assert_eq!(namespaces.rooms.len(), deserialized.rooms.len());
    }

    #[test]
    fn test_register_application_service_request() {
        let request = RegisterApplicationServiceRequest {
            as_id: "irc-bridge".to_string(),
            url: "http://localhost:9999".to_string(),
            as_token: "secret_token".to_string(),
            hs_token: "hs_secret".to_string(),
            sender: "@irc-bot:example.com".to_string(),
            description: Some("IRC to Matrix bridge".to_string()),
            is_rate_limited: Some(false),
            protocols: Some(vec!["irc".to_string()]),
            namespaces: Some(serde_json::json!({
                "users": [{"exclusive": true, "regex": "@_irc_.*:example.com"}],
                "aliases": [{"exclusive": true, "regex": "#_irc_.*:example.com"}],
                "rooms": []
            })),
            api_key: None,
            config: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: RegisterApplicationServiceRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(request.as_id, deserialized.as_id);
        assert_eq!(request.url, deserialized.url);
        assert_eq!(request.as_token, deserialized.as_token);
        assert_eq!(request.sender, deserialized.sender);
        assert_eq!(request.protocols.unwrap().len(), 1);
    }
}
