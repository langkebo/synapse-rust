use async_trait::async_trait;
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

#[async_trait]
pub trait ApplicationServiceStoreApi: Send + Sync {
    async fn register(&self, request: RegisterApplicationServiceRequest) -> Result<ApplicationService, sqlx::Error>;
    async fn upsert_registration(
        &self,
        request: RegisterApplicationServiceRequest,
    ) -> Result<ApplicationService, sqlx::Error>;
    async fn get_by_id(&self, as_id: &str) -> Result<Option<ApplicationService>, sqlx::Error>;
    async fn get_by_token(&self, as_token: &str) -> Result<Option<ApplicationService>, sqlx::Error>;
    async fn get_by_hs_token(&self, hs_token: &str) -> Result<Option<ApplicationService>, sqlx::Error>;
    async fn get_all_active(&self) -> Result<Vec<ApplicationService>, sqlx::Error>;
    async fn update(
        &self,
        as_id: &str,
        request: &UpdateApplicationServiceRequest,
    ) -> Result<ApplicationService, sqlx::Error>;
    async fn update_timestamp(&self, as_id: &str) -> Result<(), sqlx::Error>;
    async fn unregister(&self, as_id: &str) -> Result<(), sqlx::Error>;
    async fn set_state(
        &self,
        as_id: &str,
        state_key: &str,
        state_value: &str,
    ) -> Result<ApplicationServiceState, sqlx::Error>;
    async fn get_state(&self, as_id: &str, state_key: &str) -> Result<Option<ApplicationServiceState>, sqlx::Error>;
    async fn get_all_states(&self, as_id: &str) -> Result<Vec<ApplicationServiceState>, sqlx::Error>;
    #[allow(clippy::too_many_arguments)]
    async fn add_event(
        &self,
        event_id: &str,
        as_id: &str,
        room_id: &str,
        event_type: &str,
        _sender: &str,
        _content: serde_json::Value,
        _state_key: Option<&str>,
    ) -> Result<ApplicationServiceEvent, sqlx::Error>;
    async fn get_pending_events(&self, as_id: &str, limit: i64) -> Result<Vec<ApplicationServiceEvent>, sqlx::Error>;
    async fn count_pending_events(&self, as_id: &str) -> Result<i64, sqlx::Error>;
    async fn mark_event_processed(&self, event_id: &str) -> Result<(), sqlx::Error>;
    async fn create_transaction(
        &self,
        as_id: &str,
        transaction_id: &str,
        events: &[serde_json::Value],
    ) -> Result<ApplicationServiceTransaction, sqlx::Error>;
    async fn complete_transaction(&self, as_id: &str, transaction_id: &str) -> Result<(), sqlx::Error>;
    async fn fail_transaction(
        &self,
        as_id: &str,
        transaction_id: &str,
        error: &str,
    ) -> Result<ApplicationServiceTransaction, sqlx::Error>;
    async fn get_pending_transactions(&self, as_id: &str) -> Result<Vec<ApplicationServiceTransaction>, sqlx::Error>;
    async fn count_pending_transactions(&self, as_id: &str) -> Result<i64, sqlx::Error>;
    async fn register_virtual_user(
        &self,
        as_id: &str,
        user_id: &str,
        displayname: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<ApplicationServiceUser, sqlx::Error>;
    async fn get_virtual_users(&self, as_id: &str) -> Result<Vec<ApplicationServiceUser>, sqlx::Error>;
    async fn has_exclusive_user_namespace_match(&self, as_id: &str, user_id: &str) -> Result<bool, sqlx::Error>;
    async fn find_user_namespace_conflict(
        &self,
        as_id: &str,
        namespace_pattern: &str,
    ) -> Result<Option<String>, sqlx::Error>;
    async fn find_room_alias_namespace_conflict(
        &self,
        as_id: &str,
        namespace_pattern: &str,
    ) -> Result<Option<String>, sqlx::Error>;
    async fn find_room_namespace_conflict(
        &self,
        as_id: &str,
        namespace_pattern: &str,
    ) -> Result<Option<String>, sqlx::Error>;
    async fn is_user_in_namespace(&self, user_id: &str) -> Result<Option<String>, sqlx::Error>;
    async fn is_room_alias_in_namespace(&self, alias: &str) -> Result<Option<String>, sqlx::Error>;
    async fn is_room_id_in_namespace(&self, room_id: &str) -> Result<Option<String>, sqlx::Error>;
    async fn get_user_namespaces(&self, as_id: &str) -> Result<Vec<ApplicationServiceNamespace>, sqlx::Error>;
    async fn get_room_alias_namespaces(&self, as_id: &str) -> Result<Vec<ApplicationServiceNamespace>, sqlx::Error>;
    async fn get_room_namespaces(&self, as_id: &str) -> Result<Vec<ApplicationServiceNamespace>, sqlx::Error>;
    async fn get_statistics(&self) -> Result<Vec<serde_json::Value>, sqlx::Error>;
    async fn update_last_seen(&self, as_id: &str) -> Result<(), sqlx::Error>;
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
            r"UPDATE application_service_events SET is_processed = TRUE, processed_ts = $2 WHERE event_id = $1 AND is_processed = FALSE",
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
            r"UPDATE application_service_transactions SET completed_ts = $3, is_processed = TRUE, processed_ts = $3 WHERE as_id = $1 AND (txn_id = $2 OR transaction_id = $2) AND is_processed = FALSE",
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

#[async_trait]
impl ApplicationServiceStoreApi for ApplicationServiceStorage {
    async fn register(&self, request: RegisterApplicationServiceRequest) -> Result<ApplicationService, sqlx::Error> {
        self.register(request).await
    }

    async fn upsert_registration(
        &self,
        request: RegisterApplicationServiceRequest,
    ) -> Result<ApplicationService, sqlx::Error> {
        self.upsert_registration(request).await
    }

    async fn get_by_id(&self, as_id: &str) -> Result<Option<ApplicationService>, sqlx::Error> {
        self.get_by_id(as_id).await
    }

    async fn get_by_token(&self, as_token: &str) -> Result<Option<ApplicationService>, sqlx::Error> {
        self.get_by_token(as_token).await
    }

    async fn get_by_hs_token(&self, hs_token: &str) -> Result<Option<ApplicationService>, sqlx::Error> {
        self.get_by_hs_token(hs_token).await
    }

    async fn get_all_active(&self) -> Result<Vec<ApplicationService>, sqlx::Error> {
        self.get_all_active().await
    }

    async fn update(
        &self,
        as_id: &str,
        request: &UpdateApplicationServiceRequest,
    ) -> Result<ApplicationService, sqlx::Error> {
        self.update(as_id, request).await
    }

    async fn update_timestamp(&self, as_id: &str) -> Result<(), sqlx::Error> {
        self.update_timestamp(as_id).await
    }

    async fn unregister(&self, as_id: &str) -> Result<(), sqlx::Error> {
        self.unregister(as_id).await
    }

    async fn set_state(
        &self,
        as_id: &str,
        state_key: &str,
        state_value: &str,
    ) -> Result<ApplicationServiceState, sqlx::Error> {
        self.set_state(as_id, state_key, state_value).await
    }

    async fn get_state(&self, as_id: &str, state_key: &str) -> Result<Option<ApplicationServiceState>, sqlx::Error> {
        self.get_state(as_id, state_key).await
    }

    async fn get_all_states(&self, as_id: &str) -> Result<Vec<ApplicationServiceState>, sqlx::Error> {
        self.get_all_states(as_id).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn add_event(
        &self,
        event_id: &str,
        as_id: &str,
        room_id: &str,
        event_type: &str,
        _sender: &str,
        _content: serde_json::Value,
        _state_key: Option<&str>,
    ) -> Result<ApplicationServiceEvent, sqlx::Error> {
        self.add_event(event_id, as_id, room_id, event_type, _sender, _content, _state_key).await
    }

    async fn get_pending_events(&self, as_id: &str, limit: i64) -> Result<Vec<ApplicationServiceEvent>, sqlx::Error> {
        self.get_pending_events(as_id, limit).await
    }

    async fn count_pending_events(&self, as_id: &str) -> Result<i64, sqlx::Error> {
        self.count_pending_events(as_id).await
    }

    async fn mark_event_processed(&self, event_id: &str) -> Result<(), sqlx::Error> {
        self.mark_event_processed(event_id).await
    }

    async fn create_transaction(
        &self,
        as_id: &str,
        transaction_id: &str,
        events: &[serde_json::Value],
    ) -> Result<ApplicationServiceTransaction, sqlx::Error> {
        self.create_transaction(as_id, transaction_id, events).await
    }

    async fn complete_transaction(&self, as_id: &str, transaction_id: &str) -> Result<(), sqlx::Error> {
        self.complete_transaction(as_id, transaction_id).await
    }

    async fn fail_transaction(
        &self,
        as_id: &str,
        transaction_id: &str,
        error: &str,
    ) -> Result<ApplicationServiceTransaction, sqlx::Error> {
        self.fail_transaction(as_id, transaction_id, error).await
    }

    async fn get_pending_transactions(&self, as_id: &str) -> Result<Vec<ApplicationServiceTransaction>, sqlx::Error> {
        self.get_pending_transactions(as_id).await
    }

    async fn count_pending_transactions(&self, as_id: &str) -> Result<i64, sqlx::Error> {
        self.count_pending_transactions(as_id).await
    }

    async fn register_virtual_user(
        &self,
        as_id: &str,
        user_id: &str,
        displayname: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<ApplicationServiceUser, sqlx::Error> {
        self.register_virtual_user(as_id, user_id, displayname, avatar_url).await
    }

    async fn get_virtual_users(&self, as_id: &str) -> Result<Vec<ApplicationServiceUser>, sqlx::Error> {
        self.get_virtual_users(as_id).await
    }

    async fn has_exclusive_user_namespace_match(&self, as_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        self.has_exclusive_user_namespace_match(as_id, user_id).await
    }

    async fn find_user_namespace_conflict(
        &self,
        as_id: &str,
        namespace_pattern: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        self.find_user_namespace_conflict(as_id, namespace_pattern).await
    }

    async fn find_room_alias_namespace_conflict(
        &self,
        as_id: &str,
        namespace_pattern: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        self.find_room_alias_namespace_conflict(as_id, namespace_pattern).await
    }

    async fn find_room_namespace_conflict(
        &self,
        as_id: &str,
        namespace_pattern: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        self.find_room_namespace_conflict(as_id, namespace_pattern).await
    }

    async fn is_user_in_namespace(&self, user_id: &str) -> Result<Option<String>, sqlx::Error> {
        self.is_user_in_namespace(user_id).await
    }

    async fn is_room_alias_in_namespace(&self, alias: &str) -> Result<Option<String>, sqlx::Error> {
        self.is_room_alias_in_namespace(alias).await
    }

    async fn is_room_id_in_namespace(&self, room_id: &str) -> Result<Option<String>, sqlx::Error> {
        self.is_room_id_in_namespace(room_id).await
    }

    async fn get_user_namespaces(&self, as_id: &str) -> Result<Vec<ApplicationServiceNamespace>, sqlx::Error> {
        self.get_user_namespaces(as_id).await
    }

    async fn get_room_alias_namespaces(&self, as_id: &str) -> Result<Vec<ApplicationServiceNamespace>, sqlx::Error> {
        self.get_room_alias_namespaces(as_id).await
    }

    async fn get_room_namespaces(&self, as_id: &str) -> Result<Vec<ApplicationServiceNamespace>, sqlx::Error> {
        self.get_room_namespaces(as_id).await
    }

    async fn get_statistics(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        self.get_statistics().await
    }

    async fn update_last_seen(&self, as_id: &str) -> Result<(), sqlx::Error> {
        self.update_last_seen(as_id).await
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

    #[test]
    fn test_update_request_builder_chains_all_fields() {
        let req = UpdateApplicationServiceRequest::new()
            .url("https://new-url.example.com")
            .description("Updated bridge")
            .is_rate_limited(true)
            .protocols(vec!["irc".to_string(), "matrix".to_string()])
            .is_enabled(false)
            .api_key("new_api_key")
            .config(serde_json::json!({"feature": "enabled"}));

        assert_eq!(req.url.as_deref(), Some("https://new-url.example.com"));
        assert_eq!(req.description.as_deref(), Some("Updated bridge"));
        assert_eq!(req.is_rate_limited, Some(true));
        assert_eq!(req.protocols.as_ref().map(|p| p.len()), Some(2));
        assert_eq!(req.is_enabled, Some(false));
        assert_eq!(req.api_key.as_deref(), Some("new_api_key"));
        assert!(req.config.is_some());
    }

    #[test]
    fn test_update_request_builder_optional_fields_none_by_default() {
        let req = UpdateApplicationServiceRequest::new();
        assert!(req.url.is_none());
        assert!(req.description.is_none());
        assert!(req.is_rate_limited.is_none());
        assert!(req.protocols.is_none());
        assert!(req.is_enabled.is_none());
        assert!(req.api_key.is_none());
        assert!(req.config.is_none());
    }

    #[test]
    fn test_update_request_builder_partial_chain() {
        let req = UpdateApplicationServiceRequest::new().url("https://partial.example.com").is_enabled(true);

        assert_eq!(req.url.as_deref(), Some("https://partial.example.com"));
        assert_eq!(req.is_enabled, Some(true));
        // Other fields should still be None
        assert!(req.description.is_none());
        assert!(req.is_rate_limited.is_none());
        assert!(req.protocols.is_none());
        assert!(req.api_key.is_none());
        assert!(req.config.is_none());
    }

    #[test]
    fn test_update_request_serde_roundtrip() {
        let req = UpdateApplicationServiceRequest {
            url: Some("https://test.example.com".to_string()),
            description: Some("A test update".to_string()),
            is_rate_limited: Some(true),
            protocols: Some(vec!["test".to_string()]),
            is_enabled: Some(true),
            api_key: Some("key123".to_string()),
            config: Some(serde_json::json!({"k": "v"})),
        };

        let json = serde_json::to_string(&req).unwrap();
        let deserialized: UpdateApplicationServiceRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.url, req.url);
        assert_eq!(deserialized.description, req.description);
        assert_eq!(deserialized.is_rate_limited, req.is_rate_limited);
        assert_eq!(deserialized.protocols, req.protocols);
        assert_eq!(deserialized.is_enabled, req.is_enabled);
        assert_eq!(deserialized.api_key, req.api_key);
    }

    #[test]
    fn test_namespace_rule_without_group_id() {
        let rule = NamespaceRule { is_exclusive: false, regex: "@test:example.com".to_string(), group_id: None };
        let json = serde_json::to_string(&rule).unwrap();
        let deserialized: NamespaceRule = serde_json::from_str(&json).unwrap();
        assert!(!deserialized.is_exclusive);
        assert_eq!(deserialized.group_id, None);
    }

    #[test]
    fn test_application_service_state_serde() {
        let state = ApplicationServiceState {
            as_id: "my_service".to_string(),
            state_key: "config".to_string(),
            state_value: "{\"key\":\"value\"}".to_string(),
            updated_ts: 1700000000000,
        };
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: ApplicationServiceState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.as_id, state.as_id);
        assert_eq!(deserialized.state_key, state.state_key);
        assert_eq!(deserialized.updated_ts, state.updated_ts);
    }

    #[test]
    fn test_application_service_namespace_serde() {
        let ns = ApplicationServiceNamespace {
            id: 1,
            as_id: "irc-bridge".to_string(),
            namespace_pattern: "@_irc_.*:example.com".to_string(),
            is_exclusive: true,
            regex: "@_irc_.*:example.com".to_string(),
            created_ts: 1700000000000,
        };
        let json = serde_json::to_string(&ns).unwrap();
        let deserialized: ApplicationServiceNamespace = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.as_id, ns.as_id);
        assert_eq!(deserialized.is_exclusive, ns.is_exclusive);
        assert_eq!(deserialized.namespace_pattern, ns.namespace_pattern);
    }

    #[test]
    fn test_application_service_user_serde() {
        let user = ApplicationServiceUser {
            as_id: "irc-bridge".to_string(),
            user_id: "@_irc_alice:example.com".to_string(),
            displayname: Some("Alice (IRC)".to_string()),
            avatar_url: Some("mxc://example.com/avatar".to_string()),
            created_ts: 1700000000000,
        };
        let json = serde_json::to_string(&user).unwrap();
        let deserialized: ApplicationServiceUser = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.user_id, user.user_id);
        assert_eq!(deserialized.displayname, user.displayname);
        assert_eq!(deserialized.avatar_url, user.avatar_url);
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::env;

    async fn test_pool() -> Arc<sqlx::PgPool> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    /// Clean all application-service-related tables for rows matching the given suffix pattern.
    /// Deletes from child/reference tables first to avoid FK violations.
    async fn cleanup_with_suffix(pool: &sqlx::PgPool, suffix: &str) {
        let pattern = format!("%{suffix}");
        let _ = sqlx::query("DELETE FROM application_service_user_namespaces WHERE as_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM application_service_room_alias_namespaces WHERE as_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM application_service_room_namespaces WHERE as_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM application_service_statistics WHERE as_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await;
        let _ =
            sqlx::query("DELETE FROM application_service_state WHERE as_id LIKE $1").bind(&pattern).execute(pool).await;
        let _ = sqlx::query("DELETE FROM application_service_events WHERE as_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM application_service_transactions WHERE as_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await;
        let _ =
            sqlx::query("DELETE FROM application_service_users WHERE as_id LIKE $1").bind(&pattern).execute(pool).await;
        let _ = sqlx::query("DELETE FROM application_services WHERE as_id LIKE $1").bind(&pattern).execute(pool).await;
    }

    fn make_registration(
        as_id: &str,
        url: &str,
        as_token: &str,
        hs_token: &str,
        sender: &str,
    ) -> RegisterApplicationServiceRequest {
        RegisterApplicationServiceRequest {
            as_id: as_id.to_string(),
            url: url.to_string(),
            as_token: as_token.to_string(),
            hs_token: hs_token.to_string(),
            sender: sender.to_string(),
            description: Some("Integration test bridge".to_string()),
            is_rate_limited: Some(false),
            protocols: Some(vec!["test".to_string()]),
            namespaces: Some(serde_json::json!({
                "users": [{"exclusive": true, "regex": "@_test_.*:example.com"}],
                "aliases": [],
                "rooms": []
            })),
            api_key: None,
            config: Some(serde_json::json!({"source": "db_test"})),
        }
    }

    // ---- register ----

    #[tokio::test]
    async fn test_register_creates_service() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        let svc = storage.register(req).await.expect("register should succeed");

        assert_eq!(svc.as_id, as_id);
        assert_eq!(svc.url, "http://localhost:9001");
        assert_eq!(svc.as_token, as_token);
        assert_eq!(svc.hs_token, hs_token);
        assert_eq!(svc.sender_localpart, sender);
        assert!(svc.is_enabled);
        assert!(!svc.is_rate_limited);
        assert_eq!(svc.protocols, vec!["test"]);
        assert_eq!(svc.description.as_deref(), Some("Integration test bridge"));

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_register_duplicate_as_id_fails() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req1 = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        storage.register(req1).await.expect("first register should succeed");

        let req2 = make_registration(&as_id, "http://localhost:9002", &as_token, &hs_token, &sender);
        let result = storage.register(req2).await;
        assert!(result.is_err(), "second register with same as_id should fail");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- upsert_registration ----

    #[tokio::test]
    async fn test_upsert_registration_inserts_new() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        let svc = storage.upsert_registration(req).await.expect("upsert_registration should succeed");

        assert_eq!(svc.as_id, as_id);
        assert!(svc.is_enabled);

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_upsert_registration_updates_existing() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        // First upsert = insert
        let req1 = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        let svc1 = storage.upsert_registration(req1).await.expect("first upsert should succeed");
        assert_eq!(svc1.url, "http://localhost:9001");

        // Second upsert with different URL = update
        let req2 = RegisterApplicationServiceRequest {
            url: "http://localhost:9002".to_string(),
            ..make_registration(&as_id, "http://localhost:9002", &as_token, &hs_token, &sender)
        };
        let svc2 = storage.upsert_registration(req2).await.expect("second upsert should succeed");
        assert_eq!(svc2.as_id, as_id);
        assert_eq!(svc2.url, "http://localhost:9002");

        // Verify only one row exists
        let fetched = storage.get_by_id(&as_id).await.expect("get_by_id should succeed");
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().url, "http://localhost:9002");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- get_by_id ----

    #[tokio::test]
    async fn test_get_by_id_found() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        storage.register(req).await.expect("register should succeed");

        let found = storage.get_by_id(&as_id).await.expect("get_by_id should succeed");
        assert!(found.is_some());
        assert_eq!(found.unwrap().as_id, as_id);

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_by_id_not_found() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_nonexistent_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        let found = storage.get_by_id(&as_id).await.expect("get_by_id should succeed");
        assert!(found.is_none());

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- get_by_token ----

    #[tokio::test]
    async fn test_get_by_token_found() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        storage.register(req).await.expect("register should succeed");

        let found = storage.get_by_token(&as_token).await.expect("get_by_token should succeed");
        assert!(found.is_some());
        assert_eq!(found.unwrap().as_id, as_id);

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_by_token_not_found() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");

        cleanup_with_suffix(&pool, &suffix).await;

        let found = storage.get_by_token(&format!("bogus_token_{suffix}")).await.expect("get_by_token should succeed");
        assert!(found.is_none());

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_by_token_ignores_disabled_service() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        storage.register(req).await.expect("register should succeed");

        // Disable the service
        storage
            .update(&as_id, &UpdateApplicationServiceRequest::new().is_enabled(false))
            .await
            .expect("update should succeed");

        let found = storage.get_by_token(&as_token).await.expect("get_by_token should succeed");
        assert!(found.is_none(), "disabled service should not be found by token");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- get_by_hs_token ----

    #[tokio::test]
    async fn test_get_by_hs_token_found() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        storage.register(req).await.expect("register should succeed");

        let found = storage.get_by_hs_token(&hs_token).await.expect("get_by_hs_token should succeed");
        assert!(found.is_some());
        assert_eq!(found.unwrap().as_id, as_id);

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_by_hs_token_not_found() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");

        cleanup_with_suffix(&pool, &suffix).await;

        let found =
            storage.get_by_hs_token(&format!("bogus_hs_{suffix}")).await.expect("get_by_hs_token should succeed");
        assert!(found.is_none());

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- get_all_active ----

    #[tokio::test]
    async fn test_get_all_active_returns_only_enabled() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id1 = format!("as_active_{suffix}");
        let as_id2 = format!("as_inactive_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        let req1 = make_registration(
            &as_id1,
            "http://localhost:9001",
            &format!("tok1_{suffix}"),
            &format!("hs1_{suffix}"),
            &format!("@bot1_{suffix}:t.example.com"),
        );
        storage.register(req1).await.expect("register active should succeed");

        let req2 = make_registration(
            &as_id2,
            "http://localhost:9002",
            &format!("tok2_{suffix}"),
            &format!("hs2_{suffix}"),
            &format!("@bot2_{suffix}:t.example.com"),
        );
        storage.register(req2).await.expect("register inactive should succeed");
        storage
            .update(&as_id2, &UpdateApplicationServiceRequest::new().is_enabled(false))
            .await
            .expect("disable should succeed");

        let active = storage.get_all_active().await.expect("get_all_active should succeed");
        let active_ids: Vec<_> = active.iter().map(|s| s.as_id.as_str()).collect();
        assert!(active_ids.contains(&as_id1.as_str()), "active list should contain enabled service");
        assert!(!active_ids.contains(&as_id2.as_str()), "active list should NOT contain disabled service");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_all_active_returns_empty_when_no_enabled() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_inactive_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(
            &as_id,
            "http://localhost:9001",
            &format!("tok_{suffix}"),
            &format!("hs_{suffix}"),
            &format!("@bot_{suffix}:t.example.com"),
        );
        storage.register(req).await.expect("register should succeed");
        storage
            .update(&as_id, &UpdateApplicationServiceRequest::new().is_enabled(false))
            .await
            .expect("disable should succeed");

        let active = storage.get_all_active().await.expect("get_all_active should succeed");
        // Filter out pre-existing data — only check our test rows
        let our_active: Vec<_> = active.into_iter().filter(|s| s.as_id.ends_with(&suffix)).collect();
        assert!(our_active.is_empty(), "no active services for our test suffix");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- update ----

    #[tokio::test]
    async fn test_update_modifies_fields() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:t.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        storage.register(req).await.expect("register should succeed");

        let update_req = UpdateApplicationServiceRequest::new()
            .url("http://localhost:9999")
            .description("Updated description")
            .is_rate_limited(true)
            .protocols(vec!["irc".to_string(), "matrix".to_string()])
            .api_key("new_key");

        let updated = storage.update(&as_id, &update_req).await.expect("update should succeed");
        assert_eq!(updated.url, "http://localhost:9999");
        assert_eq!(updated.description.as_deref(), Some("Updated description"));
        assert!(updated.is_rate_limited);
        assert_eq!(updated.protocols, vec!["irc", "matrix"]);
        assert_eq!(updated.api_key.as_deref(), Some("new_key"));

        // Verify persisted
        let fetched = storage.get_by_id(&as_id).await.expect("get_by_id should succeed");
        let fetched = fetched.unwrap();
        assert_eq!(fetched.url, "http://localhost:9999");
        assert_eq!(fetched.description.as_deref(), Some("Updated description"));

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- update_timestamp ----

    #[tokio::test]
    async fn test_update_timestamp_sets_updated_ts() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:t.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        let created = storage.register(req).await.expect("register should succeed");
        assert!(created.updated_ts.is_none(), "newly created should have no updated_ts");

        storage.update_timestamp(&as_id).await.expect("update_timestamp should succeed");

        let fetched = storage.get_by_id(&as_id).await.expect("get_by_id should succeed");
        let fetched = fetched.unwrap();
        assert!(fetched.updated_ts.is_some(), "should have updated_ts after update_timestamp");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- unregister ----

    #[tokio::test]
    async fn test_unregister_removes_service() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let as_token = format!("token_{suffix}");
        let hs_token = format!("hs_{suffix}");
        let sender = format!("@bot_{suffix}:t.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(&as_id, "http://localhost:9001", &as_token, &hs_token, &sender);
        storage.register(req).await.expect("register should succeed");

        // Verify it exists
        let found = storage.get_by_id(&as_id).await.expect("get_by_id should succeed");
        assert!(found.is_some(), "should exist before unregister");

        storage.unregister(&as_id).await.expect("unregister should succeed");

        let after = storage.get_by_id(&as_id).await.expect("get_by_id should succeed");
        assert!(after.is_none(), "should be gone after unregister");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- set_state / get_state / get_all_states ----

    #[tokio::test]
    async fn test_set_and_get_state() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        // Must register AS first due to FK constraint on application_service_state
        let req = make_registration(
            &as_id,
            "http://localhost:9001",
            &format!("tok_{suffix}"),
            &format!("hs_{suffix}"),
            &format!("@bot_{suffix}:t.example.com"),
        );
        storage.register(req).await.expect("register should succeed");

        let state =
            storage.set_state(&as_id, "config", "{\"theme\":\"dark\"}").await.expect("set_state should succeed");
        assert_eq!(state.as_id, as_id);
        assert_eq!(state.state_key, "config");
        assert_eq!(state.state_value, "{\"theme\":\"dark\"}");

        let fetched = storage.get_state(&as_id, "config").await.expect("get_state should succeed");
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().state_value, "{\"theme\":\"dark\"}");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_set_state_overwrites_existing() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(
            &as_id,
            "http://localhost:9001",
            &format!("tok_{suffix}"),
            &format!("hs_{suffix}"),
            &format!("@bot_{suffix}:t.example.com"),
        );
        storage.register(req).await.expect("register should succeed");

        storage.set_state(&as_id, "counter", "1").await.expect("first set_state should succeed");
        storage.set_state(&as_id, "counter", "2").await.expect("second set_state should succeed");

        let fetched = storage.get_state(&as_id, "counter").await.expect("get_state should succeed");
        assert_eq!(fetched.unwrap().state_value, "2");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_all_states_for_as_id() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(
            &as_id,
            "http://localhost:9001",
            &format!("tok_{suffix}"),
            &format!("hs_{suffix}"),
            &format!("@bot_{suffix}:t.example.com"),
        );
        storage.register(req).await.expect("register should succeed");

        storage.set_state(&as_id, "key1", "val1").await.expect("set_state key1 should succeed");
        storage.set_state(&as_id, "key2", "val2").await.expect("set_state key2 should succeed");

        let all = storage.get_all_states(&as_id).await.expect("get_all_states should succeed");
        assert_eq!(all.len(), 2);
        let keys: Vec<_> = all.iter().map(|s| s.state_key.as_str()).collect();
        assert!(keys.contains(&"key1"));
        assert!(keys.contains(&"key2"));

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_state_not_found() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        let state = storage.get_state(&as_id, "nonexistent_key").await.expect("get_state should succeed");
        assert!(state.is_none());

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- add_event / get_pending_events / count_pending_events / mark_event_processed ----

    #[tokio::test]
    async fn test_event_lifecycle() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let event_id = format!("ev_{suffix}");
        let room_id = format!("!room_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        // Add event
        let event = storage
            .add_event(
                &event_id,
                &as_id,
                &room_id,
                "m.room.message",
                "sender",
                serde_json::json!({"body": "hello"}),
                None,
            )
            .await
            .expect("add_event should succeed");
        assert_eq!(event.event_id, event_id);
        assert_eq!(event.as_id, as_id);
        assert_eq!(event.room_id, room_id);
        assert!(event.processed_ts.is_none(), "new event should be unprocessed");

        // Count pending
        let count = storage.count_pending_events(&as_id).await.expect("count_pending_events should succeed");
        assert_eq!(count, 1);

        // Get pending
        let pending = storage.get_pending_events(&as_id, 10).await.expect("get_pending_events should succeed");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].event_id, event_id);

        // Mark processed
        storage.mark_event_processed(&event_id).await.expect("mark_event_processed should succeed");

        let count_after = storage.count_pending_events(&as_id).await.expect("count_pending_events should succeed");
        assert_eq!(count_after, 0, "no pending events after marking processed");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_pending_events_respects_limit() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let room_id = format!("!room_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        for i in 0..5 {
            storage
                .add_event(
                    &format!("ev_{suffix}_{i}"),
                    &as_id,
                    &room_id,
                    "m.room.message",
                    "sender",
                    serde_json::json!({"idx": i}),
                    None,
                )
                .await
                .expect("add_event should succeed");
        }

        let pending = storage.get_pending_events(&as_id, 3).await.expect("get_pending_events should succeed");
        assert_eq!(pending.len(), 3, "should respect limit of 3");

        let total = storage.count_pending_events(&as_id).await.expect("count_pending_events should succeed");
        assert_eq!(total, 5, "total should still be 5");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_mark_event_processed_idempotent() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let event_id = format!("ev_{suffix}");
        let room_id = format!("!room_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        storage
            .add_event(&event_id, &as_id, &room_id, "m.room.message", "sender", serde_json::json!({}), None)
            .await
            .expect("add_event should succeed");

        // Mark processed twice
        storage.mark_event_processed(&event_id).await.expect("first mark should succeed");
        storage.mark_event_processed(&event_id).await.expect("second mark should succeed (idempotent)");

        let count = storage.count_pending_events(&as_id).await.expect("count_pending_events should succeed");
        assert_eq!(count, 0);

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- create_transaction / complete_transaction / fail_transaction / get_pending_transactions / count_pending_transactions ----

    #[tokio::test]
    async fn test_create_and_complete_transaction() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let txn_id = format!("txn_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        let events = vec![serde_json::json!({"type": "m.room.message", "body": "hi"})];
        let txn =
            storage.create_transaction(&as_id, &txn_id, &events).await.expect("create_transaction should succeed");
        assert_eq!(txn.as_id, as_id);
        assert_eq!(txn.txn_id, txn_id);
        assert!(txn.completed_ts.is_none(), "new transaction should be incomplete");

        // Count pending
        let pending_count =
            storage.count_pending_transactions(&as_id).await.expect("count_pending_transactions should succeed");
        assert_eq!(pending_count, 1);

        // Get pending transactions
        let pending = storage.get_pending_transactions(&as_id).await.expect("get_pending_transactions should succeed");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].txn_id, txn_id);

        // Complete
        storage.complete_transaction(&as_id, &txn_id).await.expect("complete_transaction should succeed");

        let after_count =
            storage.count_pending_transactions(&as_id).await.expect("count_pending_transactions should succeed");
        assert_eq!(after_count, 0, "no pending after completion");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_fail_transaction_increments_retry() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let txn_id = format!("txn_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        storage.create_transaction(&as_id, &txn_id, &[]).await.expect("create_transaction should succeed");

        let failed = storage
            .fail_transaction(&as_id, &txn_id, "connection refused")
            .await
            .expect("fail_transaction should succeed");
        assert_eq!(failed.retry_count, 1);
        assert_eq!(failed.last_error.as_deref(), Some("connection refused"));

        let failed2 = storage.fail_transaction(&as_id, &txn_id, "timeout").await.expect("second fail should succeed");
        assert_eq!(failed2.retry_count, 2);
        assert_eq!(failed2.last_error.as_deref(), Some("timeout"));

        // Failed transaction is still pending (completed_ts still null)
        let pending_count =
            storage.count_pending_transactions(&as_id).await.expect("count_pending_transactions should succeed");
        assert_eq!(pending_count, 1, "failed txns are still pending");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- register_virtual_user / get_virtual_users ----

    #[tokio::test]
    async fn test_register_and_get_virtual_users() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let user1 = format!("@vu1_{suffix}:test.example.com");
        let user2 = format!("@vu2_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        // Must register AS first due to FK constraint on application_service_users
        let req = make_registration(
            &as_id,
            "http://localhost:9001",
            &format!("tok_{suffix}"),
            &format!("hs_{suffix}"),
            &format!("@bot_{suffix}:t.example.com"),
        );
        storage.register(req).await.expect("register should succeed");

        let vu1 = storage
            .register_virtual_user(&as_id, &user1, Some("VU One"), Some("mxc://avatar1"))
            .await
            .expect("register_virtual_user should succeed");
        assert_eq!(vu1.user_id, user1);
        assert_eq!(vu1.displayname.as_deref(), Some("VU One"));

        let vu2 = storage
            .register_virtual_user(&as_id, &user2, None, None)
            .await
            .expect("register_virtual_user should succeed");
        assert_eq!(vu2.user_id, user2);
        assert!(vu2.displayname.is_none());

        let users = storage.get_virtual_users(&as_id).await.expect("get_virtual_users should succeed");
        assert_eq!(users.len(), 2);
        let user_ids: Vec<_> = users.iter().map(|u| u.user_id.as_str()).collect();
        assert!(user_ids.contains(&user1.as_str()));
        assert!(user_ids.contains(&user2.as_str()));

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_register_virtual_user_upserts() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_test_{suffix}");
        let user_id = format!("@vu_{suffix}:test.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let req = make_registration(
            &as_id,
            "http://localhost:9001",
            &format!("tok_{suffix}"),
            &format!("hs_{suffix}"),
            &format!("@bot_{suffix}:t.example.com"),
        );
        storage.register(req).await.expect("register should succeed");

        let vu1 = storage
            .register_virtual_user(&as_id, &user_id, Some("Original"), None)
            .await
            .expect("first register should succeed");
        assert_eq!(vu1.displayname.as_deref(), Some("Original"));

        // Re-register with new displayname
        let vu2 = storage
            .register_virtual_user(&as_id, &user_id, Some("Updated"), Some("mxc://avatar"))
            .await
            .expect("second register should succeed (upsert)");
        assert_eq!(vu2.displayname.as_deref(), Some("Updated"));
        assert_eq!(vu2.avatar_url.as_deref(), Some("mxc://avatar"));

        let users = storage.get_virtual_users(&as_id).await.expect("get_virtual_users should succeed");
        assert_eq!(users.len(), 1, "only one row for the same user");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_virtual_users_empty_for_unknown_as_id() {
        let pool = test_pool().await;
        let storage = ApplicationServiceStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let as_id = format!("as_unknown_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        let users = storage.get_virtual_users(&as_id).await.expect("get_virtual_users should succeed");
        assert!(users.is_empty());

        cleanup_with_suffix(&pool, &suffix).await;
    }
}
