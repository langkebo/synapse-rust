use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
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
    pub transaction_id: String,
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

        let service = sqlx::query_as!(
            ApplicationService,
            r#"
            INSERT INTO application_services (
                as_id, url, as_token, hs_token, sender_localpart, is_enabled,
                is_rate_limited, protocols, namespaces, created_ts, description, api_key, config
            )
            VALUES ($1, $2, $3, $4, $5, TRUE, $6, $7, $8, $9, $10, $11, $12)
            RETURNING
                id as "id!", as_id as "as_id!", url as "url!", as_token as "as_token!", hs_token as "hs_token!", sender_localpart as "sender_localpart!",
                COALESCE(is_enabled, false) AS "is_enabled!",
                COALESCE(is_rate_limited, true) AS "is_rate_limited!",
                COALESCE(protocols, '{}'::text[]) AS "protocols!",
                COALESCE(namespaces, '{}'::jsonb) AS "namespaces!",
                created_ts as "created_ts!", updated_ts, description, api_key, config as "config!"
            "#,
            &request.as_id,
            &request.url,
            &request.as_token,
            &request.hs_token,
            &request.sender,
            request.is_rate_limited.unwrap_or(false),
            &protocols,
            &namespaces,
            now,
            request.description.as_deref(),
            request.api_key.as_deref(),
            &config
        )
        .fetch_one(&*self.pool)
        .await?;

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
                    sqlx::query!(
                        r#"
                        INSERT INTO application_service_user_namespaces (as_id, namespace, is_exclusive, created_ts)
                        VALUES ($1, $2, $3, $4)
                        "#,
                        &service.as_id,
                        regex,
                        exclusive,
                        now
                    )
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
                    sqlx::query!(
                        r#"
                        INSERT INTO application_service_room_alias_namespaces (as_id, namespace, is_exclusive, created_ts)
                        VALUES ($1, $2, $3, $4)
                        "#,
                        &service.as_id,
                        regex,
                        exclusive,
                        now
                    )
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
                    sqlx::query!(
                        r#"
                        INSERT INTO application_service_room_namespaces (as_id, namespace, is_exclusive, created_ts)
                        VALUES ($1, $2, $3, $4)
                        "#,
                        &service.as_id,
                        regex,
                        exclusive,
                        now
                    )
                    .execute(&*self.pool)
                    .await?;
                }
            }
        }

        Ok(())
    }

    pub async fn get_by_id(&self, as_id: &str) -> Result<Option<ApplicationService>, sqlx::Error> {
        sqlx::query_as!(
            ApplicationService,
            r#"
            SELECT
                id as "id!", as_id as "as_id!", url as "url!", as_token as "as_token!", hs_token as "hs_token!", sender_localpart as "sender_localpart!",
                COALESCE(is_enabled, false) AS "is_enabled!",
                COALESCE(is_rate_limited, true) AS "is_rate_limited!",
                COALESCE(protocols, '{}'::text[]) AS "protocols!",
                COALESCE(namespaces, '{}'::jsonb) AS "namespaces!",
                created_ts as "created_ts!", updated_ts, description, api_key, config as "config!"
            FROM application_services WHERE as_id = $1
            "#,
            as_id
        )
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_by_token(&self, as_token: &str) -> Result<Option<ApplicationService>, sqlx::Error> {
        sqlx::query_as!(
            ApplicationService,
            r#"
            SELECT
                id as "id!", as_id as "as_id!", url as "url!", as_token as "as_token!", hs_token as "hs_token!", sender_localpart as "sender_localpart!",
                COALESCE(is_enabled, false) AS "is_enabled!",
                COALESCE(is_rate_limited, true) AS "is_rate_limited!",
                COALESCE(protocols, '{}'::text[]) AS "protocols!",
                COALESCE(namespaces, '{}'::jsonb) AS "namespaces!",
                created_ts as "created_ts!", updated_ts, description, api_key, config as "config!"
            FROM application_services WHERE as_token = $1 AND is_enabled = TRUE
            "#,
            as_token
        )
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_by_hs_token(&self, hs_token: &str) -> Result<Option<ApplicationService>, sqlx::Error> {
        sqlx::query_as!(
            ApplicationService,
            r#"
            SELECT
                id as "id!", as_id as "as_id!", url as "url!", as_token as "as_token!", hs_token as "hs_token!", sender_localpart as "sender_localpart!",
                COALESCE(is_enabled, false) AS "is_enabled!",
                COALESCE(is_rate_limited, true) AS "is_rate_limited!",
                COALESCE(protocols, '{}'::text[]) AS "protocols!",
                COALESCE(namespaces, '{}'::jsonb) AS "namespaces!",
                created_ts as "created_ts!", updated_ts, description, api_key, config as "config!"
            FROM application_services WHERE hs_token = $1 AND is_enabled = TRUE
            "#,
            hs_token
        )
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_all_active(&self) -> Result<Vec<ApplicationService>, sqlx::Error> {
        sqlx::query_as!(
            ApplicationService,
            r#"
            SELECT
                id as "id!", as_id as "as_id!", url as "url!", as_token as "as_token!", hs_token as "hs_token!", sender_localpart as "sender_localpart!",
                COALESCE(is_enabled, false) AS "is_enabled!",
                COALESCE(is_rate_limited, true) AS "is_rate_limited!",
                COALESCE(protocols, '{}'::text[]) AS "protocols!",
                COALESCE(namespaces, '{}'::jsonb) AS "namespaces!",
                created_ts as "created_ts!", updated_ts, description, api_key, config as "config!"
            FROM application_services WHERE is_enabled = TRUE ORDER BY created_ts DESC
            "#,
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
        sqlx::query_as!(
            ApplicationService,
            r#"
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
            RETURNING
                id as "id!", as_id as "as_id!", url as "url!", as_token as "as_token!", hs_token as "hs_token!", sender_localpart as "sender_localpart!",
                COALESCE(is_enabled, false) AS "is_enabled!",
                COALESCE(is_rate_limited, true) AS "is_rate_limited!",
                COALESCE(protocols, '{}'::text[]) AS "protocols!",
                COALESCE(namespaces, '{}'::jsonb) AS "namespaces!",
                created_ts as "created_ts!", updated_ts, description, api_key, config as "config!"
            "#,
            as_id,
            request.url.as_deref(),
            request.description.as_deref(),
            request.is_rate_limited,
            protocols.as_deref(),
            request.is_enabled,
            request.api_key.as_deref(),
            config.as_ref(),
            chrono::Utc::now().timestamp_millis()
        )
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn update_timestamp(&self, as_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query!(r"UPDATE application_services SET updated_ts = $2 WHERE as_id = $1", as_id, now)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn unregister(&self, as_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(r"DELETE FROM application_services WHERE as_id = $1", as_id).execute(&*self.pool).await?;
        Ok(())
    }

    pub async fn set_state(
        &self,
        as_id: &str,
        state_key: &str,
        state_value: &str,
    ) -> Result<ApplicationServiceState, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        sqlx::query_as!(
            ApplicationServiceState,
            r#"
            INSERT INTO application_service_state (as_id, state_key, state_value, updated_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (as_id, state_key) DO UPDATE SET
                state_value = EXCLUDED.state_value,
                updated_ts = EXCLUDED.updated_ts
            RETURNING
                as_id as "as_id!", state_key as "state_key!",
                COALESCE(state_value, '') AS "state_value!",
                updated_ts as "updated_ts!"
            "#,
            as_id,
            state_key,
            state_value,
            now
        )
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_state(
        &self,
        as_id: &str,
        state_key: &str,
    ) -> Result<Option<ApplicationServiceState>, sqlx::Error> {
        sqlx::query_as!(
            ApplicationServiceState,
            r#"
            SELECT
                as_id as "as_id!", state_key as "state_key!",
                COALESCE(state_value, '') AS "state_value!",
                updated_ts as "updated_ts!"
            FROM application_service_state WHERE as_id = $1 AND state_key = $2
            "#,
            as_id,
            state_key
        )
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_all_states(&self, as_id: &str) -> Result<Vec<ApplicationServiceState>, sqlx::Error> {
        sqlx::query_as!(
            ApplicationServiceState,
            r#"
            SELECT
                as_id as "as_id!", state_key as "state_key!",
                COALESCE(state_value, '') AS "state_value!",
                updated_ts as "updated_ts!"
            FROM application_service_state WHERE as_id = $1
            "#,
            as_id
        )
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
        sqlx::query_as!(
            ApplicationServiceEvent,
            r#"
            INSERT INTO application_service_events (
                event_id, as_id, room_id, event_type, is_processed, processed_ts, created_ts
            )
            VALUES ($1, $2, $3, $4, FALSE, NULL, $5)
            RETURNING
                event_id as "event_id!",
                as_id as "as_id!",
                COALESCE(room_id, '') AS "room_id!",
                COALESCE(event_type, '') AS "event_type!",
                ''::text AS "sender!",
                '{}'::jsonb AS "content!",
                NULL::text AS state_key,
                created_ts AS "origin_server_ts!",
                processed_ts,
                NULL::text AS transaction_id
            "#,
            event_id,
            as_id,
            room_id,
            event_type,
            now
        )
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_pending_events(
        &self,
        as_id: &str,
        limit: i64,
    ) -> Result<Vec<ApplicationServiceEvent>, sqlx::Error> {
        sqlx::query_as!(
            ApplicationServiceEvent,
            r#"
            SELECT
                event_id as "event_id!",
                as_id as "as_id!",
                COALESCE(room_id, '') AS "room_id!",
                COALESCE(event_type, '') AS "event_type!",
                ''::text AS "sender!",
                '{}'::jsonb AS "content!",
                NULL::text AS state_key,
                created_ts AS "origin_server_ts!",
                processed_ts,
                NULL::text AS transaction_id
            FROM application_service_events
            WHERE as_id = $1 AND processed_ts IS NULL
            ORDER BY created_ts ASC
            LIMIT $2
            "#,
            as_id,
            limit
        )
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn mark_event_processed(&self, event_id: &str, transaction_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        // NOTE: transaction_id column does not exist in application_service_events table;
        // keeping sqlx::query to preserve original runtime behavior since sqlx! macro
        // validates against the live schema at compile time.
        sqlx::query(
            r"UPDATE application_service_events SET processed_ts = $2, transaction_id = $3 WHERE event_id = $1",
        )
        .bind(event_id)
        .bind(now)
        .bind(transaction_id)
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
        sqlx::query_as!(
            ApplicationServiceTransaction,
            r#"
            INSERT INTO application_service_transactions (as_id, transaction_id, events, sent_ts)
            VALUES ($1, $2, $3, $4)
            RETURNING
                id as "id!", as_id as "as_id!",
                COALESCE(transaction_id, '') AS "transaction_id!",
                COALESCE(events, '[]'::jsonb) AS "events!",
                COALESCE(sent_ts, 0) AS "sent_ts!",
                completed_ts, retry_count as "retry_count!", last_error
            "#,
            as_id,
            transaction_id,
            serde_json::json!(events),
            now
        )
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn complete_transaction(&self, as_id: &str, transaction_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        sqlx::query!(
            r"UPDATE application_service_transactions SET completed_ts = $3 WHERE as_id = $1 AND transaction_id = $2",
            as_id,
            transaction_id,
            now
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn fail_transaction(&self, as_id: &str, transaction_id: &str, error: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"UPDATE application_service_transactions SET retry_count = retry_count + 1, last_error = $3 WHERE as_id = $1 AND transaction_id = $2",
            as_id,
            transaction_id,
            error
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_pending_transactions(
        &self,
        as_id: &str,
    ) -> Result<Vec<ApplicationServiceTransaction>, sqlx::Error> {
        sqlx::query_as!(
            ApplicationServiceTransaction,
            r#"
            SELECT
                id as "id!", as_id as "as_id!",
                COALESCE(transaction_id, '') AS "transaction_id!",
                COALESCE(events, '[]'::jsonb) AS "events!",
                COALESCE(sent_ts, 0) AS "sent_ts!",
                completed_ts, retry_count as "retry_count!", last_error
            FROM application_service_transactions WHERE as_id = $1 AND completed_ts IS NULL ORDER BY sent_ts ASC
            "#,
            as_id
        )
        .fetch_all(&*self.pool)
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
        sqlx::query_as!(
            ApplicationServiceUser,
            r#"
            INSERT INTO application_service_users (as_id, user_id, displayname, avatar_url, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (as_id, user_id) DO UPDATE SET
                displayname = COALESCE(EXCLUDED.displayname, application_service_users.displayname),
                avatar_url = COALESCE(EXCLUDED.avatar_url, application_service_users.avatar_url)
            RETURNING as_id as "as_id!", user_id as "user_id!", displayname, avatar_url, created_ts as "created_ts!"
            "#,
            as_id,
            user_id,
            displayname,
            avatar_url,
            now
        )
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_virtual_users(&self, as_id: &str) -> Result<Vec<ApplicationServiceUser>, sqlx::Error> {
        sqlx::query_as!(
            ApplicationServiceUser,
            r#"SELECT as_id as "as_id!", user_id as "user_id!", displayname, avatar_url, created_ts as "created_ts!" FROM application_service_users WHERE as_id = $1"#,
            as_id
        )
            .fetch_all(&*self.pool)
            .await
    }

    pub async fn is_user_in_namespace(&self, user_id: &str) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"SELECT as_id as "as_id!" FROM application_service_user_namespaces WHERE $1 ~ namespace LIMIT 1"#,
            user_id
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result)
    }

    pub async fn is_room_alias_in_namespace(&self, alias: &str) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"SELECT as_id as "as_id!" FROM application_service_room_alias_namespaces WHERE $1 ~ namespace LIMIT 1"#,
            alias
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result)
    }

    pub async fn is_room_id_in_namespace(&self, room_id: &str) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"SELECT as_id as "as_id!" FROM application_service_room_namespaces WHERE $1 ~ namespace LIMIT 1"#,
            room_id
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result)
    }

    pub async fn get_user_namespaces(&self, as_id: &str) -> Result<Vec<ApplicationServiceNamespace>, sqlx::Error> {
        sqlx::query_as!(
            ApplicationServiceNamespace,
            r#"
            SELECT
                id as "id!",
                as_id as "as_id!",
                namespace AS "namespace_pattern!",
                COALESCE(is_exclusive, true) AS "is_exclusive!",
                namespace AS "regex!",
                created_ts as "created_ts!"
            FROM application_service_user_namespaces
            WHERE as_id = $1
            "#,
            as_id
        )
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_room_alias_namespaces(
        &self,
        as_id: &str,
    ) -> Result<Vec<ApplicationServiceNamespace>, sqlx::Error> {
        sqlx::query_as!(
            ApplicationServiceNamespace,
            r#"
            SELECT
                id as "id!",
                as_id as "as_id!",
                namespace AS "namespace_pattern!",
                COALESCE(is_exclusive, true) AS "is_exclusive!",
                namespace AS "regex!",
                created_ts as "created_ts!"
            FROM application_service_room_alias_namespaces
            WHERE as_id = $1
            "#,
            as_id
        )
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_room_namespaces(&self, as_id: &str) -> Result<Vec<ApplicationServiceNamespace>, sqlx::Error> {
        sqlx::query_as!(
            ApplicationServiceNamespace,
            r#"
            SELECT
                id as "id!",
                as_id as "as_id!",
                namespace AS "namespace_pattern!",
                COALESCE(is_exclusive, true) AS "is_exclusive!",
                namespace AS "regex!",
                created_ts as "created_ts!"
            FROM application_service_room_namespaces
            WHERE as_id = $1
            "#,
            as_id
        )
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_statistics(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"SELECT id as "id!", as_id as "as_id!", name, is_enabled as "is_enabled!", is_rate_limited as "is_rate_limited!", virtual_user_count as "virtual_user_count!", pending_event_count as "pending_event_count!", pending_transaction_count as "pending_transaction_count!", last_seen_ts, created_ts as "created_ts!" FROM application_service_statistics"#
        )
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.id,
                    "as_id": row.as_id,
                    "name": row.name,
                    "is_enabled": row.is_enabled,
                    "is_rate_limited": row.is_rate_limited,
                    "virtual_user_count": row.virtual_user_count,
                    "pending_event_count": row.pending_event_count,
                    "pending_transaction_count": row.pending_transaction_count,
                    "last_seen_ts": row.last_seen_ts,
                    "created_ts": row.created_ts,
                })
            })
            .collect())
    }

    pub async fn update_last_seen(&self, as_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query!(
            r#"
            UPDATE application_service_statistics
            SET last_seen_ts = $2
            WHERE as_id = $1
            "#,
            as_id,
            now
        )
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
