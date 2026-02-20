use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Row};
use std::sync::Arc;
use chrono::Utc;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApplicationService {
    pub id: i64,
    pub as_id: String,
    pub url: String,
    pub as_token: String,
    pub hs_token: String,
    pub sender: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub rate_limited: bool,
    #[sqlx(json)]
    pub protocols: Vec<String>,
    pub namespaces: serde_json::Value,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub last_seen_ts: Option<i64>,
    pub is_enabled: bool,
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
    pub exclusive: bool,
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
    pub name: Option<String>,
    pub description: Option<String>,
    pub rate_limited: Option<bool>,
    pub protocols: Option<Vec<String>>,
    pub namespaces: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateApplicationServiceRequest {
    pub url: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub rate_limited: Option<bool>,
    pub protocols: Option<Vec<String>>,
    pub is_enabled: Option<bool>,
}

impl UpdateApplicationServiceRequest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn rate_limited(mut self, rate_limited: bool) -> Self {
        self.rate_limited = Some(rate_limited);
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Namespaces {
    pub users: Vec<NamespaceRule>,
    pub aliases: Vec<NamespaceRule>,
    pub rooms: Vec<NamespaceRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceRule {
    pub exclusive: bool,
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
        let namespaces = request.namespaces.unwrap_or(serde_json::json!({
            "users": [],
            "aliases": [],
            "rooms": []
        }));

        let service = sqlx::query_as::<_, ApplicationService>(
            r#"
            INSERT INTO application_services (
                as_id, url, as_token, hs_token, sender, name, description,
                rate_limited, protocols, namespaces, created_ts, is_enabled
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, TRUE)
            RETURNING *
            "#,
        )
        .bind(&request.as_id)
        .bind(&request.url)
        .bind(&request.as_token)
        .bind(&request.hs_token)
        .bind(&request.sender)
        .bind(&request.name)
        .bind(&request.description)
        .bind(request.rate_limited.unwrap_or(false))
        .bind(request.protocols.unwrap_or_default())
        .bind(&namespaces)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        self.insert_namespaces(&service).await?;

        Ok(service)
    }

    async fn insert_namespaces(&self, service: &ApplicationService) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        if let Some(users) = service.namespaces.get("users").and_then(|v| v.as_array()) {
            for rule in users {
                if let (Some(regex), Some(exclusive)) = (
                    rule.get("regex").and_then(|r| r.as_str()),
                    rule.get("exclusive").and_then(|e| e.as_bool()),
                ) {
                    sqlx::query(
                        r#"
                        INSERT INTO application_service_user_namespaces (as_id, namespace_pattern, exclusive, regex, created_ts)
                        VALUES ($1, $2, $3, $4, $5)
                        "#,
                    )
                    .bind(&service.as_id)
                    .bind(regex)
                    .bind(exclusive)
                    .bind(regex)
                    .bind(now)
                    .execute(&*self.pool)
                    .await?;
                }
            }
        }

        if let Some(aliases) = service.namespaces.get("aliases").and_then(|v| v.as_array()) {
            for rule in aliases {
                if let (Some(regex), Some(exclusive)) = (
                    rule.get("regex").and_then(|r| r.as_str()),
                    rule.get("exclusive").and_then(|e| e.as_bool()),
                ) {
                    sqlx::query(
                        r#"
                        INSERT INTO application_service_room_alias_namespaces (as_id, namespace_pattern, exclusive, regex, created_ts)
                        VALUES ($1, $2, $3, $4, $5)
                        "#,
                    )
                    .bind(&service.as_id)
                    .bind(regex)
                    .bind(exclusive)
                    .bind(regex)
                    .bind(now)
                    .execute(&*self.pool)
                    .await?;
                }
            }
        }

        if let Some(rooms) = service.namespaces.get("rooms").and_then(|v| v.as_array()) {
            for rule in rooms {
                if let (Some(regex), Some(exclusive)) = (
                    rule.get("regex").and_then(|r| r.as_str()),
                    rule.get("exclusive").and_then(|e| e.as_bool()),
                ) {
                    sqlx::query(
                        r#"
                        INSERT INTO application_service_room_namespaces (as_id, namespace_pattern, exclusive, regex, created_ts)
                        VALUES ($1, $2, $3, $4, $5)
                        "#,
                    )
                    .bind(&service.as_id)
                    .bind(regex)
                    .bind(exclusive)
                    .bind(regex)
                    .bind(now)
                    .execute(&*self.pool)
                    .await?;
                }
            }
        }

        Ok(())
    }

    pub async fn get_by_id(&self, as_id: &str) -> Result<Option<ApplicationService>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationService>(
            r#"SELECT * FROM application_services WHERE as_id = $1"#
        )
        .bind(as_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_by_token(&self, as_token: &str) -> Result<Option<ApplicationService>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationService>(
            r#"SELECT * FROM application_services WHERE as_token = $1 AND is_enabled = TRUE"#
        )
        .bind(as_token)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_by_hs_token(&self, hs_token: &str) -> Result<Option<ApplicationService>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationService>(
            r#"SELECT * FROM application_services WHERE hs_token = $1 AND is_enabled = TRUE"#
        )
        .bind(hs_token)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_all_active(&self) -> Result<Vec<ApplicationService>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationService>(
            r#"SELECT * FROM application_services WHERE is_enabled = TRUE ORDER BY created_ts DESC"#
        )
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn update(
        &self,
        as_id: &str,
        request: &UpdateApplicationServiceRequest,
    ) -> Result<ApplicationService, sqlx::Error> {
        sqlx::query_as::<_, ApplicationService>(
            r#"
            UPDATE application_services SET
                url = COALESCE($2, url),
                name = COALESCE($3, name),
                description = COALESCE($4, description),
                rate_limited = COALESCE($5, rate_limited),
                protocols = COALESCE($6, protocols),
                is_enabled = COALESCE($7, is_enabled)
            WHERE as_id = $1
            RETURNING *
            "#,
        )
        .bind(as_id)
        .bind(&request.url)
        .bind(&request.name)
        .bind(&request.description)
        .bind(request.rate_limited)
        .bind(&request.protocols)
        .bind(request.is_enabled)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn update_last_seen(&self, as_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        sqlx::query(r#"UPDATE application_services SET last_seen_ts = $2 WHERE as_id = $1"#)
            .bind(as_id)
            .bind(now)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn unregister(&self, as_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(r#"DELETE FROM application_services WHERE as_id = $1"#)
            .bind(as_id)
            .execute(&*self.pool)
            .await?;
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
            r#"
            INSERT INTO application_service_state (as_id, state_key, state_value, updated_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (as_id, state_key) DO UPDATE SET
                state_value = EXCLUDED.state_value,
                updated_ts = EXCLUDED.updated_ts
            RETURNING *
            "#,
        )
        .bind(as_id)
        .bind(state_key)
        .bind(state_value)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_state(&self, as_id: &str, state_key: &str) -> Result<Option<ApplicationServiceState>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationServiceState>(
            r#"SELECT * FROM application_service_state WHERE as_id = $1 AND state_key = $2"#
        )
        .bind(as_id)
        .bind(state_key)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_all_states(&self, as_id: &str) -> Result<Vec<ApplicationServiceState>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationServiceState>(
            r#"SELECT * FROM application_service_state WHERE as_id = $1"#
        )
        .bind(as_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn add_event(
        &self,
        event_id: &str,
        as_id: &str,
        room_id: &str,
        event_type: &str,
        sender: &str,
        content: serde_json::Value,
        state_key: Option<&str>,
    ) -> Result<ApplicationServiceEvent, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        sqlx::query_as::<_, ApplicationServiceEvent>(
            r#"
            INSERT INTO application_service_events (
                event_id, as_id, room_id, event_type, sender, content, state_key, origin_server_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            "#,
        )
        .bind(event_id)
        .bind(as_id)
        .bind(room_id)
        .bind(event_type)
        .bind(sender)
        .bind(&content)
        .bind(state_key)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_pending_events(&self, as_id: &str, limit: i64) -> Result<Vec<ApplicationServiceEvent>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationServiceEvent>(
            r#"SELECT * FROM application_service_events WHERE as_id = $1 AND processed_ts IS NULL ORDER BY origin_server_ts ASC LIMIT $2"#
        )
        .bind(as_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn mark_event_processed(&self, event_id: &str, transaction_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        sqlx::query(
            r#"UPDATE application_service_events SET processed_ts = $2, transaction_id = $3 WHERE event_id = $1"#
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
        sqlx::query_as::<_, ApplicationServiceTransaction>(
            r#"
            INSERT INTO application_service_transactions (as_id, transaction_id, events, sent_ts)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#,
        )
        .bind(as_id)
        .bind(transaction_id)
        .bind(serde_json::json!(events))
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn complete_transaction(&self, as_id: &str, transaction_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        sqlx::query(
            r#"UPDATE application_service_transactions SET completed_ts = $3 WHERE as_id = $1 AND transaction_id = $2"#
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
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE application_service_transactions SET retry_count = retry_count + 1, last_error = $3 WHERE as_id = $1 AND transaction_id = $2"#
        )
        .bind(as_id)
        .bind(transaction_id)
        .bind(error)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_pending_transactions(&self, as_id: &str) -> Result<Vec<ApplicationServiceTransaction>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationServiceTransaction>(
            r#"SELECT * FROM application_service_transactions WHERE as_id = $1 AND completed_ts IS NULL ORDER BY sent_ts ASC"#
        )
        .bind(as_id)
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
        sqlx::query_as::<_, ApplicationServiceUser>(
            r#"
            INSERT INTO application_service_users (as_id, user_id, displayname, avatar_url, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (as_id, user_id) DO UPDATE SET
                displayname = COALESCE(EXCLUDED.displayname, application_service_users.displayname),
                avatar_url = COALESCE(EXCLUDED.avatar_url, application_service_users.avatar_url)
            RETURNING *
            "#,
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
        sqlx::query_as::<_, ApplicationServiceUser>(
            r#"SELECT * FROM application_service_users WHERE as_id = $1"#
        )
        .bind(as_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn is_user_in_namespace(&self, user_id: &str) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query(
            r#"SELECT as_id FROM application_service_user_namespaces WHERE $1 ~ regex"#
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map(|row| row.get("as_id")))
    }

    pub async fn is_room_alias_in_namespace(&self, alias: &str) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query(
            r#"SELECT as_id FROM application_service_room_alias_namespaces WHERE $1 ~ regex"#
        )
        .bind(alias)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map(|row| row.get("as_id")))
    }

    pub async fn is_room_id_in_namespace(&self, room_id: &str) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query(
            r#"SELECT as_id FROM application_service_room_namespaces WHERE $1 ~ regex"#
        )
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map(|row| row.get("as_id")))
    }

    pub async fn get_user_namespaces(&self, as_id: &str) -> Result<Vec<ApplicationServiceNamespace>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationServiceNamespace>(
            r#"SELECT * FROM application_service_user_namespaces WHERE as_id = $1"#
        )
        .bind(as_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_room_alias_namespaces(&self, as_id: &str) -> Result<Vec<ApplicationServiceNamespace>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationServiceNamespace>(
            r#"SELECT * FROM application_service_room_alias_namespaces WHERE as_id = $1"#
        )
        .bind(as_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_room_namespaces(&self, as_id: &str) -> Result<Vec<ApplicationServiceNamespace>, sqlx::Error> {
        sqlx::query_as::<_, ApplicationServiceNamespace>(
            r#"SELECT * FROM application_service_room_namespaces WHERE as_id = $1"#
        )
        .bind(as_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_statistics(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        sqlx::query(r#"SELECT * FROM application_service_statistics"#)
            .fetch_all(&*self.pool)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|row| {
                        serde_json::json!({
                            "id": row.get::<i64, _>("id"),
                            "as_id": row.get::<String, _>("as_id"),
                            "name": row.get::<Option<String>, _>("name"),
                            "is_active": row.get::<bool, _>("is_active"),
                            "rate_limited": row.get::<bool, _>("rate_limited"),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_rule_serialization() {
        let rule = NamespaceRule {
            exclusive: true,
            regex: "@_.*:example.com".to_string(),
            group_id: Some("group:example.com".to_string()),
        };

        let json = serde_json::to_string(&rule).unwrap();
        let deserialized: NamespaceRule = serde_json::from_str(&json).unwrap();
        
        assert_eq!(rule.exclusive, deserialized.exclusive);
        assert_eq!(rule.regex, deserialized.regex);
        assert_eq!(rule.group_id, deserialized.group_id);
    }

    #[test]
    fn test_namespaces_serialization() {
        let namespaces = Namespaces {
            users: vec![NamespaceRule {
                exclusive: true,
                regex: "@_.*:example.com".to_string(),
                group_id: None,
            }],
            aliases: vec![NamespaceRule {
                exclusive: false,
                regex: "#_.*:example.com".to_string(),
                group_id: None,
            }],
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
            name: Some("IRC Bridge".to_string()),
            description: Some("IRC to Matrix bridge".to_string()),
            rate_limited: Some(false),
            protocols: Some(vec!["irc".to_string()]),
            namespaces: Some(serde_json::json!({
                "users": [{"exclusive": true, "regex": "@_irc_.*:example.com"}],
                "aliases": [{"exclusive": true, "regex": "#_irc_.*:example.com"}],
                "rooms": []
            })),
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
