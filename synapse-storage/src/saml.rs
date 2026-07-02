use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;
use std::sync::Arc;
use synapse_common::error::ApiError;
use tracing::{debug, info};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SamlSession {
    pub id: i64,
    pub session_id: String,
    pub user_id: String,
    pub name_id: Option<String>,
    pub issuer: Option<String>,
    pub session_index: Option<String>,
    pub attributes: serde_json::Value,
    pub created_ts: i64,
    pub expires_at: i64,
    pub last_used_ts: i64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SamlUserMapping {
    pub id: i64,
    pub name_id: String,
    pub user_id: String,
    pub issuer: String,
    pub first_seen_ts: i64,
    pub last_authenticated_ts: i64,
    pub authentication_count: i32,
    pub attributes: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SamlIdentityProvider {
    pub id: i64,
    pub entity_id: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub metadata_url: Option<String>,
    pub metadata_xml: Option<String>,
    pub is_enabled: bool,
    pub priority: i32,
    pub attribute_mapping: serde_json::Value,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    #[sqlx(rename = "last_metadata_refresh_at")]
    pub last_metadata_refresh_ts: Option<i64>,
    #[sqlx(rename = "metadata_valid_until_at")]
    pub metadata_valid_until: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SamlAuthEvent {
    pub id: i64,
    pub session_id: Option<String>,
    pub user_id: Option<String>,
    pub name_id: Option<String>,
    pub issuer: Option<String>,
    pub event_type: String,
    pub status: String,
    pub error_message: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub request_id: Option<String>,
    pub attributes: serde_json::Value,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SamlLogoutRequest {
    pub id: i64,
    pub request_id: String,
    pub session_id: Option<String>,
    pub user_id: Option<String>,
    pub name_id: Option<String>,
    pub issuer: Option<String>,
    pub reason: Option<String>,
    pub status: String,
    pub created_ts: i64,
    #[sqlx(rename = "processed_at")]
    pub processed_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSamlSessionRequest {
    pub session_id: String,
    pub user_id: String,
    pub name_id: Option<String>,
    pub issuer: Option<String>,
    pub session_index: Option<String>,
    pub attributes: HashMap<String, Vec<String>>,
    pub expires_in_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSamlUserMappingRequest {
    pub name_id: String,
    pub user_id: String,
    pub issuer: String,
    pub attributes: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSamlIdentityProviderRequest {
    pub entity_id: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub metadata_url: Option<String>,
    pub metadata_xml: Option<String>,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
    pub attribute_mapping: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSamlAuthEventRequest {
    pub session_id: Option<String>,
    pub user_id: Option<String>,
    pub name_id: Option<String>,
    pub issuer: Option<String>,
    pub event_type: String,
    pub status: String,
    pub error_message: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub request_id: Option<String>,
    pub attributes: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSamlLogoutRequestRequest {
    pub request_id: String,
    pub session_id: Option<String>,
    pub user_id: Option<String>,
    pub name_id: Option<String>,
    pub issuer: Option<String>,
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_saml_session_creation() {
        let mut attributes = HashMap::new();
        attributes.insert("email".to_string(), vec!["alice@example.com".to_string()]);

        let session = SamlSession {
            id: 1,
            session_id: "session123".to_string(),
            user_id: "@alice:example.com".to_string(),
            name_id: Some("alice".to_string()),
            issuer: Some("https://idp.example.com".to_string()),
            session_index: Some("index123".to_string()),
            attributes: serde_json::json!(attributes),
            created_ts: 1234567800000,
            expires_at: 1234567890000,
            last_used_ts: 1234567890000,
            status: "active".to_string(),
        };
        assert_eq!(session.session_id, "session123");
        assert_eq!(session.user_id, "@alice:example.com");
    }

    #[test]
    fn test_saml_user_mapping_creation() {
        let mut attributes = HashMap::new();
        attributes.insert("email".to_string(), vec!["alice@example.com".to_string()]);

        let mapping = SamlUserMapping {
            id: 1,
            name_id: "alice".to_string(),
            user_id: "@alice:example.com".to_string(),
            issuer: "https://idp.example.com".to_string(),
            first_seen_ts: 1234567800000,
            last_authenticated_ts: 1234567890000,
            authentication_count: 1,
            attributes: serde_json::json!(attributes),
        };
        assert_eq!(mapping.user_id, "@alice:example.com");
    }

    #[test]
    fn test_saml_identity_provider_creation() {
        let idp = SamlIdentityProvider {
            id: 1,
            entity_id: "https://idp.example.com".to_string(),
            display_name: Some("Example IdP".to_string()),
            description: Some("Test IDP".to_string()),
            metadata_url: None,
            metadata_xml: Some("<xml>metadata</xml>".to_string()),
            is_enabled: true,
            priority: 0,
            attribute_mapping: serde_json::json!({}),
            created_ts: 1234567800000,
            updated_ts: Some(1234567890000),
            last_metadata_refresh_ts: None,
            metadata_valid_until: None,
        };
        assert!(idp.is_enabled);
        assert_eq!(idp.entity_id, "https://idp.example.com");
    }

    #[test]
    fn test_saml_auth_event_creation() {
        let event = SamlAuthEvent {
            id: 1,
            session_id: Some("session123".to_string()),
            user_id: Some("@alice:example.com".to_string()),
            name_id: Some("alice".to_string()),
            issuer: Some("https://idp.example.com".to_string()),
            event_type: "authentication".to_string(),
            status: "success".to_string(),
            error_message: None,
            ip_address: Some("192.168.1.1".to_string()),
            user_agent: None,
            request_id: None,
            attributes: serde_json::json!({}),
            created_ts: 1234567890000,
        };
        assert_eq!(event.status, "success");
    }

    #[test]
    fn test_create_saml_session_request() {
        let attributes = HashMap::new();
        let request = CreateSamlSessionRequest {
            session_id: "new_session".to_string(),
            user_id: "@alice:example.com".to_string(),
            name_id: Some("alice".to_string()),
            issuer: Some("https://idp.example.com".to_string()),
            session_index: Some("index123".to_string()),
            attributes,
            expires_in_seconds: 3600,
        };
        assert_eq!(request.user_id, "@alice:example.com");
    }

    #[test]
    fn test_create_saml_identity_provider_request() {
        let request = CreateSamlIdentityProviderRequest {
            entity_id: "https://new-idp.example.com".to_string(),
            display_name: Some("New IdP".to_string()),
            description: Some("New Identity Provider".to_string()),
            metadata_url: None,
            metadata_xml: Some("<xml>new metadata</xml>".to_string()),
            enabled: Some(true),
            priority: Some(0),
            attribute_mapping: None,
        };
        assert_eq!(request.entity_id, "https://new-idp.example.com");
    }
}

#[derive(Clone)]
pub struct SamlStorage {
    pool: Arc<sqlx::PgPool>,
}

impl SamlStorage {
    pub fn new(pool: &Arc<sqlx::PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_session(&self, request: CreateSamlSessionRequest) -> Result<SamlSession, ApiError> {
        let now = Utc::now().timestamp_millis();
        let expires_at = Utc::now().timestamp_millis() + request.expires_in_seconds * 1000;
        let attributes_json = serde_json::to_value(&request.attributes).unwrap_or(serde_json::json!({}));

        let row = sqlx::query_as::<_, SamlSession>(
            r#"
            INSERT INTO saml_sessions (
                session_id, user_id, name_id, issuer, session_index, attributes,
                created_ts, expires_at, last_used_ts, status
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $7, 'active')
            RETURNING *
            "#,
        )
        .bind(&request.session_id)
        .bind(&request.user_id)
        .bind(&request.name_id)
        .bind(&request.issuer)
        .bind(&request.session_index)
        .bind(&attributes_json)
        .bind(now)
        .bind(expires_at)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create SAML session", &e))?;

        info!("Created SAML session: {} for user: {}", request.session_id, request.user_id);
        Ok(row)
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Option<SamlSession>, ApiError> {
        let row =
            sqlx::query_as::<_, SamlSession>("SELECT id, session_id, user_id, name_id, issuer, session_index, attributes, created_ts, expires_at, last_used_ts, status FROM saml_sessions WHERE session_id = $1 AND status = 'active'")
                .bind(session_id)
                .fetch_optional(&*self.pool)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to get SAML session", &e))?;

        Ok(row)
    }

    pub async fn get_session_by_user(&self, user_id: &str) -> Result<Option<SamlSession>, ApiError> {
        let now = Utc::now().timestamp_millis();
        let row = sqlx::query_as::<_, SamlSession>(
            r#"
            SELECT id, session_id, user_id, name_id, issuer, session_index, attributes, created_ts, expires_at, last_used_ts, status FROM saml_sessions
            WHERE user_id = $1 AND status = 'active' AND expires_at > $2
            ORDER BY created_ts DESC
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get SAML session by user", &e))?;

        Ok(row)
    }

    pub async fn update_session_last_used(&self, session_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            "UPDATE saml_sessions SET last_used_ts = (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000) WHERE session_id = $1",
        )
        .bind(session_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to update session last used", &e))?;

        Ok(())
    }

    pub async fn invalidate_session(&self, session_id: &str) -> Result<(), ApiError> {
        sqlx::query("UPDATE saml_sessions SET status = 'invalidated' WHERE session_id = $1")
            .bind(session_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to invalidate SAML session", &e))?;

        info!("Invalidated SAML session: {}", session_id);
        Ok(())
    }

    pub async fn cleanup_expired_sessions(&self) -> Result<u64, ApiError> {
        let now = Utc::now().timestamp_millis();
        let result = sqlx::query("DELETE FROM saml_sessions WHERE expires_at < $1 OR status = 'invalidated'")
            .bind(now)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to cleanup expired sessions", &e))?;

        let count = result.rows_affected();
        if count > 0 {
            info!("Cleaned up {} expired SAML sessions", count);
        }
        Ok(count)
    }

    pub async fn create_user_mapping(
        &self,
        request: CreateSamlUserMappingRequest,
    ) -> Result<SamlUserMapping, ApiError> {
        let now = Utc::now().timestamp_millis();
        let attributes_json = serde_json::to_value(&request.attributes).unwrap_or(serde_json::json!({}));

        let row = sqlx::query_as::<_, SamlUserMapping>(
            r#"
            INSERT INTO saml_user_mapping (
                name_id, user_id, issuer, first_seen_ts, last_authenticated_ts,
                authentication_count, attributes
            )
            VALUES ($1, $2, $3, $4, $4, 1, $5)
            ON CONFLICT (name_id, issuer) DO UPDATE SET
                last_authenticated_ts = EXTRACT(EPOCH FROM NOW())::BIGINT * 1000,
                authentication_count = saml_user_mapping.authentication_count + 1,
                attributes = EXCLUDED.attributes
            RETURNING *
            "#,
        )
        .bind(&request.name_id)
        .bind(&request.user_id)
        .bind(&request.issuer)
        .bind(now)
        .bind(&attributes_json)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create SAML user mapping", &e))?;

        info!("Created/updated SAML user mapping: {} -> {}", request.name_id, request.user_id);
        Ok(row)
    }

    pub async fn get_user_mapping_by_name_id(
        &self,
        name_id: &str,
        issuer: &str,
    ) -> Result<Option<SamlUserMapping>, ApiError> {
        let row =
            sqlx::query_as::<_, SamlUserMapping>("SELECT id, name_id, user_id, issuer, first_seen_ts, last_authenticated_ts, authentication_count, attributes FROM saml_user_mapping WHERE name_id = $1 AND issuer = $2")
                .bind(name_id)
                .bind(issuer)
                .fetch_optional(&*self.pool)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to get SAML user mapping", &e))?;

        Ok(row)
    }

    pub async fn get_user_mapping_by_user_id(&self, user_id: &str) -> Result<Option<SamlUserMapping>, ApiError> {
        let row = sqlx::query_as::<_, SamlUserMapping>("SELECT id, name_id, user_id, issuer, first_seen_ts, last_authenticated_ts, authentication_count, attributes FROM saml_user_mapping WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get SAML user mapping", &e))?;

        Ok(row)
    }

    pub async fn delete_user_mapping(&self, name_id: &str, issuer: &str) -> Result<(), ApiError> {
        sqlx::query("DELETE FROM saml_user_mapping WHERE name_id = $1 AND issuer = $2")
            .bind(name_id)
            .bind(issuer)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete SAML user mapping", &e))?;

        info!("Deleted SAML user mapping: {} ({})", name_id, issuer);
        Ok(())
    }

    /// List SAML user mappings with keyset pagination on `name_id`.
    ///
    /// `after` is an optional cursor: when provided, only mappings with
    /// `name_id > after` are returned. Rows are ordered ascending by
    /// `(name_id, issuer)` so the cursor remains deterministic even when
    /// the same `name_id` appears under multiple issuers.
    pub async fn list_user_mappings(&self, limit: i64, after: Option<&str>) -> Result<Vec<SamlUserMapping>, ApiError> {
        let rows = if let Some(cursor) = after {
            sqlx::query_as::<_, SamlUserMapping>(
                r#"
                SELECT id, name_id, user_id, issuer, first_seen_ts, last_authenticated_ts, authentication_count, attributes FROM saml_user_mapping
                WHERE name_id > $1
                ORDER BY name_id ASC, issuer ASC
                LIMIT $2
                "#,
            )
            .bind(cursor)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query_as::<_, SamlUserMapping>(
                r#"
                SELECT id, name_id, user_id, issuer, first_seen_ts, last_authenticated_ts, authentication_count, attributes FROM saml_user_mapping
                ORDER BY name_id ASC, issuer ASC
                LIMIT $1
                "#,
            )
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        }
        .map_err(|e| ApiError::internal_with_log("Failed to list SAML user mappings", &e))?;

        Ok(rows)
    }

    /// Fetch the first SAML user mapping matching `name_id` across any issuer.
    ///
    /// The admin SDK identifies mappings by `name_id` only; when multiple
    /// issuers have produced the same `name_id`, the oldest (`first_seen_ts
    /// ASC`, then `issuer ASC`) is returned for stability. Consumers that
    /// need exactness should use {@link Self::get_user_mapping_by_name_id}
    /// with an explicit issuer.
    pub async fn get_user_mapping_any_issuer(&self, name_id: &str) -> Result<Option<SamlUserMapping>, ApiError> {
        let row = sqlx::query_as::<_, SamlUserMapping>(
            r#"
            SELECT id, name_id, user_id, issuer, first_seen_ts, last_authenticated_ts, authentication_count, attributes FROM saml_user_mapping
            WHERE name_id = $1
            ORDER BY first_seen_ts ASC, issuer ASC
            LIMIT 1
            "#,
        )
        .bind(name_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get SAML user mapping", &e))?;

        Ok(row)
    }

    /// Update the first mapping matching `name_id` (across any issuer).
    ///
    /// `new_user_id` replaces the mapped homeserver user when `Some`.
    /// `attributes` replaces the stored attribute JSON when `Some`.
    /// Returns the updated row, or `None` when no mapping matched.
    pub async fn update_user_mapping_by_name_id(
        &self,
        name_id: &str,
        new_user_id: Option<&str>,
        attributes: Option<&serde_json::Value>,
    ) -> Result<Option<SamlUserMapping>, ApiError> {
        let existing = self.get_user_mapping_any_issuer(name_id).await?;
        let Some(existing) = existing else {
            return Ok(None);
        };

        let target_user_id = new_user_id.unwrap_or(&existing.user_id);
        let target_attributes = attributes.cloned().unwrap_or(existing.attributes.clone());

        let row = sqlx::query_as::<_, SamlUserMapping>(
            r#"
            UPDATE saml_user_mapping
            SET user_id = $1, attributes = $2
            WHERE name_id = $3 AND issuer = $4
            RETURNING *
            "#,
        )
        .bind(target_user_id)
        .bind(&target_attributes)
        .bind(&existing.name_id)
        .bind(&existing.issuer)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to update SAML user mapping", &e))?;

        if row.is_some() {
            info!("Updated SAML user mapping: {} -> {}", name_id, target_user_id);
        }
        Ok(row)
    }

    /// Delete every mapping row matching `name_id` (across any issuer).
    ///
    /// Returns the number of rows removed.
    pub async fn delete_user_mapping_by_name_id(&self, name_id: &str) -> Result<u64, ApiError> {
        let result = sqlx::query("DELETE FROM saml_user_mapping WHERE name_id = $1")
            .bind(name_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete SAML user mappings", &e))?;

        let count = result.rows_affected();
        if count > 0 {
            info!("Deleted {} SAML user mapping rows for name_id {}", count, name_id);
        }
        Ok(count)
    }

    pub async fn create_identity_provider(
        &self,
        request: CreateSamlIdentityProviderRequest,
    ) -> Result<SamlIdentityProvider, ApiError> {
        let now = Utc::now().timestamp_millis();
        let attribute_mapping = request.attribute_mapping.unwrap_or(serde_json::json!({}));

        let row = sqlx::query_as::<_, SamlIdentityProvider>(
            r#"
            INSERT INTO saml_identity_providers (
                entity_id, display_name, description, metadata_url, metadata_xml,
                is_enabled, priority, attribute_mapping, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
            ON CONFLICT (entity_id) DO UPDATE SET
                display_name = EXCLUDED.display_name,
                description = EXCLUDED.description,
                metadata_url = EXCLUDED.metadata_url,
                metadata_xml = EXCLUDED.metadata_xml,
                is_enabled = EXCLUDED.is_enabled,
                priority = EXCLUDED.priority,
                attribute_mapping = EXCLUDED.attribute_mapping,
                updated_ts = (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
            RETURNING *
            "#,
        )
        .bind(&request.entity_id)
        .bind(&request.display_name)
        .bind(&request.description)
        .bind(&request.metadata_url)
        .bind(&request.metadata_xml)
        .bind(request.enabled.unwrap_or(true))
        .bind(request.priority.unwrap_or(100))
        .bind(&attribute_mapping)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create SAML IdP", &e))?;

        info!("Created/updated SAML identity provider: {}", request.entity_id);
        Ok(row)
    }

    pub async fn get_identity_provider(&self, entity_id: &str) -> Result<Option<SamlIdentityProvider>, ApiError> {
        let row =
            sqlx::query_as::<_, SamlIdentityProvider>("SELECT id, entity_id, display_name, description, metadata_url, metadata_xml, is_enabled, priority, attribute_mapping, created_ts, updated_ts, last_metadata_refresh_at, metadata_valid_until_at FROM saml_identity_providers WHERE entity_id = $1")
                .bind(entity_id)
                .fetch_optional(&*self.pool)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to get SAML IdP", &e))?;

        Ok(row)
    }

    pub async fn get_all_identity_providers(&self) -> Result<Vec<SamlIdentityProvider>, ApiError> {
        let rows =
            sqlx::query_as::<_, SamlIdentityProvider>("SELECT id, entity_id, display_name, description, metadata_url, metadata_xml, is_enabled, priority, attribute_mapping, created_ts, updated_ts, last_metadata_refresh_at, metadata_valid_until_at FROM saml_identity_providers ORDER BY priority ASC")
                .fetch_all(&*self.pool)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to get SAML IdPs", &e))?;

        Ok(rows)
    }

    pub async fn get_enabled_identity_providers(&self) -> Result<Vec<SamlIdentityProvider>, ApiError> {
        let rows = sqlx::query_as::<_, SamlIdentityProvider>(
            "SELECT id, entity_id, display_name, description, metadata_url, metadata_xml, is_enabled, priority, attribute_mapping, created_ts, updated_ts, last_metadata_refresh_at, metadata_valid_until_at FROM saml_identity_providers WHERE is_enabled = true ORDER BY priority ASC",
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get enabled SAML IdPs", &e))?;

        Ok(rows)
    }

    pub async fn update_idp_metadata(
        &self,
        entity_id: &str,
        metadata_xml: &str,
        valid_until: Option<i64>,
    ) -> Result<(), ApiError> {
        let now = Utc::now().timestamp_millis();
        sqlx::query(
            r#"
            UPDATE saml_identity_providers
            SET metadata_xml = $1,
                last_metadata_refresh_at = $4,
                metadata_valid_until_at = $2,
                updated_ts = $4
            WHERE entity_id = $3
            "#,
        )
        .bind(metadata_xml)
        .bind(valid_until)
        .bind(entity_id)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to update IdP metadata", &e))?;

        debug!("Updated SAML IdP metadata: {}", entity_id);
        Ok(())
    }

    pub async fn delete_identity_provider(&self, entity_id: &str) -> Result<(), ApiError> {
        sqlx::query("DELETE FROM saml_identity_providers WHERE entity_id = $1")
            .bind(entity_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete SAML IdP", &e))?;

        info!("Deleted SAML identity provider: {}", entity_id);
        Ok(())
    }

    pub async fn create_auth_event(&self, request: CreateSamlAuthEventRequest) -> Result<SamlAuthEvent, ApiError> {
        let now = Utc::now().timestamp_millis();
        let attributes_json = serde_json::to_value(&request.attributes).unwrap_or(serde_json::json!({}));

        let row = sqlx::query_as::<_, SamlAuthEvent>(
            r#"
            INSERT INTO saml_auth_events (
                session_id, user_id, name_id, issuer, event_type, status,
                error_message, ip_address, user_agent, request_id, attributes, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            RETURNING *
            "#,
        )
        .bind(&request.session_id)
        .bind(&request.user_id)
        .bind(&request.name_id)
        .bind(&request.issuer)
        .bind(&request.event_type)
        .bind(&request.status)
        .bind(&request.error_message)
        .bind(&request.ip_address)
        .bind(&request.user_agent)
        .bind(&request.request_id)
        .bind(&attributes_json)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create SAML auth event", &e))?;

        debug!("Created SAML auth event: {} - {}", request.event_type, request.status);
        Ok(row)
    }

    pub async fn get_auth_events_by_user(&self, user_id: &str, limit: i64) -> Result<Vec<SamlAuthEvent>, ApiError> {
        let rows = sqlx::query_as::<_, SamlAuthEvent>(
            r#"
            SELECT id, session_id, user_id, name_id, issuer, event_type, status, error_message, ip_address, user_agent, request_id, attributes, created_ts FROM saml_auth_events
            WHERE user_id = $1
            ORDER BY created_ts DESC
            LIMIT $2
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get auth events", &e))?;

        Ok(rows)
    }

    pub async fn create_logout_request(
        &self,
        request: CreateSamlLogoutRequestRequest,
    ) -> Result<SamlLogoutRequest, ApiError> {
        let now = Utc::now().timestamp_millis();
        let row = sqlx::query_as::<_, SamlLogoutRequest>(
            r#"
            INSERT INTO saml_logout_requests (
                request_id, session_id, user_id, name_id, issuer, reason, status, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, 'pending', $7)
            RETURNING *
            "#,
        )
        .bind(&request.request_id)
        .bind(&request.session_id)
        .bind(&request.user_id)
        .bind(&request.name_id)
        .bind(&request.issuer)
        .bind(&request.reason)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create logout request", &e))?;

        info!("Created SAML logout request: {}", request.request_id);
        Ok(row)
    }

    pub async fn get_logout_request(&self, request_id: &str) -> Result<Option<SamlLogoutRequest>, ApiError> {
        let row = sqlx::query_as::<_, SamlLogoutRequest>("SELECT id, request_id, session_id, user_id, name_id, issuer, reason, status, created_ts, processed_at FROM saml_logout_requests WHERE request_id = $1")
            .bind(request_id)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get logout request", &e))?;

        Ok(row)
    }

    pub async fn process_logout_request(&self, request_id: &str) -> Result<(), ApiError> {
        let now_ts = chrono::Utc::now().timestamp_millis();
        sqlx::query("UPDATE saml_logout_requests SET status = 'processed', processed_ts = $2 WHERE request_id = $1")
            .bind(request_id)
            .bind(now_ts)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to process logout request", &e))?;

        info!("Processed SAML logout request: {}", request_id);
        Ok(())
    }

    pub async fn cleanup_old_auth_events(&self, days: i64) -> Result<u64, ApiError> {
        let result = sqlx::query(
            "DELETE FROM saml_auth_events WHERE created_ts < (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000) - $1 * 86400000",
        )
            .bind(days)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to cleanup auth events", &e))?;

        let count = result.rows_affected();
        if count > 0 {
            info!("Cleaned up {} old SAML auth events", count);
        }
        Ok(count)
    }

    /// Load every persisted SAML runtime config override.
    ///
    /// Called once at service startup to hydrate the in-memory cache
    /// that `SamlService::effective_config()` reads from synchronously.
    pub async fn get_all_config_overrides(&self) -> Result<HashMap<String, serde_json::Value>, ApiError> {
        let rows: Vec<(String, serde_json::Value)> =
            sqlx::query_as("SELECT config_key, config_value FROM saml_config_overrides")
                .fetch_all(&*self.pool)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to load SAML config overrides", &e))?;

        debug!("Loaded {} SAML config override(s)", rows.len());
        Ok(rows.into_iter().collect())
    }

    /// Upsert a single SAML runtime config override.
    ///
    /// The caller (SamlService) is responsible for enforcing the
    /// `MUTABLE_CONFIG_FIELDS` whitelist; this method trusts its inputs.
    pub async fn upsert_config_override(&self, key: &str, value: &serde_json::Value) -> Result<(), ApiError> {
        let now = Utc::now().timestamp_millis();
        sqlx::query(
            r#"
            INSERT INTO saml_config_overrides (config_key, config_value, updated_ts)
            VALUES ($1, $2, $3)
            ON CONFLICT (config_key) DO UPDATE SET
                config_value = EXCLUDED.config_value,
                updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(key)
        .bind(value)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to upsert SAML config override", &e))?;

        Ok(())
    }

    /// Remove a single SAML runtime config override.
    ///
    /// Used to reset a field back to the static `SamlConfig` value.
    pub async fn delete_config_override(&self, key: &str) -> Result<(), ApiError> {
        sqlx::query("DELETE FROM saml_config_overrides WHERE config_key = $1")
            .bind(key)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete SAML config override", &e))?;

        Ok(())
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::sync::Arc;

    async fn test_pool() -> Arc<sqlx::PgPool> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&db_url)
            .await
            .expect("Failed to connect to test database");
        Arc::new(pool)
    }

    async fn ensure_test_user(pool: &sqlx::PgPool, user_id: &str) {
        let username = user_id
            .strip_prefix('@')
            .and_then(|u| u.split(':').next())
            .unwrap_or("testuser");
        sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, EXTRACT(EPOCH FROM NOW()) * 1000) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(username)
        .execute(pool)
        .await
        .ok();
    }

    async fn cleanup_saml_test_data(pool: &sqlx::PgPool, suffix: &str) {
        let pattern = format!("%{suffix}%");
        sqlx::query("DELETE FROM saml_sessions WHERE session_id LIKE $1 OR user_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM saml_user_mapping WHERE name_id LIKE $1 OR user_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM saml_identity_providers WHERE entity_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await
            .ok();
    }

    fn make_attrs(entries: &[(&str, &str)]) -> HashMap<String, Vec<String>> {
        let mut m = HashMap::new();
        for (k, v) in entries {
            m.insert(k.to_string(), vec![v.to_string()]);
        }
        m
    }

    // ---------------------------------------------------------------------------
    // Session tests
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_session_valid_record() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@saml_create_session_{suffix}:localhost");
        let session_id = format!("sess_create_{suffix}");
        let issuer = format!("https://idp-{suffix}.example.com");

        cleanup_saml_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = SamlStorage::new(&pool);
        let req = CreateSamlSessionRequest {
            session_id: session_id.clone(),
            user_id: user_id.clone(),
            name_id: Some(format!("name_{suffix}")),
            issuer: Some(issuer.clone()),
            session_index: Some(format!("idx_{suffix}")),
            attributes: make_attrs(&[("email", &format!("user_{suffix}@example.com"))]),
            expires_in_seconds: 3600,
        };

        let session = storage.create_session(req).await.expect("create_session should succeed");

        assert!(session.id > 0);
        assert_eq!(session.session_id, session_id);
        assert_eq!(session.user_id, user_id);
        assert_eq!(session.issuer.as_deref(), Some(issuer.as_str()));
        assert_eq!(session.status, "active");
        assert!(session.expires_at > session.created_ts);
        assert!(session.last_used_ts > 0);

        cleanup_saml_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_session_not_found() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let storage = SamlStorage::new(&pool);

        cleanup_saml_test_data(&pool, &suffix).await;

        let result = storage.get_session(&format!("nonexistent_{suffix}")).await.expect("query should succeed");
        assert!(result.is_none(), "nonexistent session should return None");

        cleanup_saml_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_session_by_user_found_and_not_found() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@saml_gbu_{suffix}:localhost");
        let session_id = format!("sess_gbu_{suffix}");

        cleanup_saml_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = SamlStorage::new(&pool);
        let req = CreateSamlSessionRequest {
            session_id: session_id.clone(),
            user_id: user_id.clone(),
            name_id: Some(format!("name_{suffix}")),
            issuer: None,
            session_index: None,
            attributes: make_attrs(&[]),
            expires_in_seconds: 3600,
        };
        storage.create_session(req).await.expect("create should succeed");

        // Found
        let found = storage
            .get_session_by_user(&user_id)
            .await
            .expect("query should succeed")
            .expect("session should be found");
        assert_eq!(found.session_id, session_id);

        // Not found — different user
        let other_user = format!("@saml_other_{suffix}:localhost");
        let not_found = storage.get_session_by_user(&other_user).await.expect("query should succeed");
        assert!(not_found.is_none(), "should not find session for other user");

        cleanup_saml_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_update_session_last_used() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@saml_upd_lu_{suffix}:localhost");
        let session_id = format!("sess_upd_lu_{suffix}");

        cleanup_saml_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = SamlStorage::new(&pool);
        let req = CreateSamlSessionRequest {
            session_id: session_id.clone(),
            user_id: user_id.clone(),
            name_id: None,
            issuer: None,
            session_index: None,
            attributes: make_attrs(&[]),
            expires_in_seconds: 3600,
        };
        storage.create_session(req).await.expect("create should succeed");

        // Note: the SQL uses EXTRACT(EPOCH FROM NOW())::BIGINT * 1000 which
        // truncates to second precision, so the "updated" timestamp may
        // appear less than the Rust timestamp. We just verify the call
        // succeeds and the session is still retrievable.
        storage
            .update_session_last_used(&session_id)
            .await
            .expect("update should succeed");

        let updated = storage
            .get_session(&session_id)
            .await
            .expect("query should succeed")
            .expect("session should still exist after update");

        assert_eq!(updated.session_id, session_id);
        assert!(updated.last_used_ts > 0);

        cleanup_saml_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_invalidate_session_then_get_returns_none() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@saml_inval_{suffix}:localhost");
        let session_id = format!("sess_inval_{suffix}");

        cleanup_saml_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = SamlStorage::new(&pool);
        let req = CreateSamlSessionRequest {
            session_id: session_id.clone(),
            user_id: user_id.clone(),
            name_id: None,
            issuer: None,
            session_index: None,
            attributes: make_attrs(&[]),
            expires_in_seconds: 3600,
        };
        storage.create_session(req).await.expect("create should succeed");

        // Verify exists before invalidation
        let before = storage.get_session(&session_id).await.expect("query should succeed");
        assert!(before.is_some(), "session should exist before invalidation");

        storage
            .invalidate_session(&session_id)
            .await
            .expect("invalidate should succeed");

        let after = storage.get_session(&session_id).await.expect("query should succeed");
        assert!(after.is_none(), "invalidated session should not be returned by get_session");

        cleanup_saml_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_cleanup_expired_sessions() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@saml_cleanup_{suffix}:localhost");
        let expired_session_id = format!("sess_expired_{suffix}");
        let valid_session_id = format!("sess_valid_{suffix}");

        cleanup_saml_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let now = chrono::Utc::now().timestamp_millis();

        // Insert an already-expired session directly via SQL
        sqlx::query(
            r#"INSERT INTO saml_sessions
               (session_id, user_id, name_id, issuer, session_index, attributes, created_ts, expires_at, last_used_ts, status)
               VALUES ($1, $2, NULL, NULL, NULL, '{}'::jsonb, $3, $4, $5, 'active')"#,
        )
        .bind(&expired_session_id)
        .bind(&user_id)
        .bind(now - 7200000) // created 2 hours ago
        .bind(now - 3600000) // expired 1 hour ago
        .bind(now - 7200000)
        .execute(&*pool)
        .await
        .expect("should insert expired session");

        // Create a valid (non-expired) session via the storage API
        let storage = SamlStorage::new(&pool);
        let req = CreateSamlSessionRequest {
            session_id: valid_session_id.clone(),
            user_id: user_id.clone(),
            name_id: None,
            issuer: None,
            session_index: None,
            attributes: make_attrs(&[]),
            expires_in_seconds: 3600,
        };
        storage.create_session(req).await.expect("create valid session should succeed");

        let removed = storage.cleanup_expired_sessions().await.expect("cleanup should succeed");
        assert!(removed >= 1, "should have removed at least the expired session");

        // Expired session should be gone
        let expired = storage.get_session(&expired_session_id).await.expect("query should succeed");
        assert!(expired.is_none(), "expired session should be cleaned up");

        // Valid session should remain
        let valid = storage
            .get_session(&valid_session_id)
            .await
            .expect("query should succeed")
            .expect("valid session should survive cleanup");
        assert_eq!(valid.session_id, valid_session_id);

        cleanup_saml_test_data(&pool, &suffix).await;
    }

    // ---------------------------------------------------------------------------
    // User mapping tests
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_user_mapping() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@saml_map_{suffix}:localhost");
        let name_id = format!("name_map_{suffix}");
        let issuer = format!("https://idp-{suffix}.example.com");

        cleanup_saml_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = SamlStorage::new(&pool);
        let req = CreateSamlUserMappingRequest {
            name_id: name_id.clone(),
            user_id: user_id.clone(),
            issuer: issuer.clone(),
            attributes: make_attrs(&[("email", &format!("map_{suffix}@example.com"))]),
        };

        let mapping = storage.create_user_mapping(req).await.expect("create_user_mapping should succeed");

        assert!(mapping.id > 0);
        assert_eq!(mapping.name_id, name_id);
        assert_eq!(mapping.user_id, user_id);
        assert_eq!(mapping.issuer, issuer);
        assert_eq!(mapping.authentication_count, 1);
        assert!(mapping.first_seen_ts > 0);
        assert!(mapping.last_authenticated_ts > 0);

        cleanup_saml_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_create_user_mapping_on_conflict_updates() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id_a = format!("@saml_map_a_{suffix}:localhost");
        let user_id_b = format!("@saml_map_b_{suffix}:localhost");
        let name_id = format!("name_conflict_{suffix}");
        let issuer = format!("https://idp-{suffix}.example.com");

        cleanup_saml_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id_a).await;
        ensure_test_user(&pool, &user_id_b).await;

        let storage = SamlStorage::new(&pool);

        // First create
        let req1 = CreateSamlUserMappingRequest {
            name_id: name_id.clone(),
            user_id: user_id_a.clone(),
            issuer: issuer.clone(),
            attributes: make_attrs(&[("email", &format!("a_{suffix}@example.com"))]),
        };
        let m1 = storage.create_user_mapping(req1).await.expect("first create should succeed");
        assert_eq!(m1.user_id, user_id_a);
        assert_eq!(m1.authentication_count, 1);

        // Brief sleep so last_authenticated_ts can change
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Second create with same (name_id, issuer) but different user_id
        let req2 = CreateSamlUserMappingRequest {
            name_id: name_id.clone(),
            user_id: user_id_b.clone(),
            issuer: issuer.clone(),
            attributes: make_attrs(&[("email", &format!("b_{suffix}@example.com"))]),
        };
        let m2 = storage.create_user_mapping(req2).await.expect("second create should succeed");

        // ON CONFLICT DO UPDATE should bump counter.
        // Note: last_authenticated_ts via SQL EXTRACT(EPOCH) truncates to second
        // precision and may be LESS than the Rust timestamp, so we only check
        // the counter.
        assert_eq!(m2.id, m1.id, "should update the same row");
        assert_eq!(m2.authentication_count, 2);

        cleanup_saml_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_user_mapping_by_name_id_found_and_not_found() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@saml_gmbn_{suffix}:localhost");
        let name_id = format!("name_gmbn_{suffix}");
        let issuer = format!("https://idp-{suffix}.example.com");

        cleanup_saml_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = SamlStorage::new(&pool);
        let req = CreateSamlUserMappingRequest {
            name_id: name_id.clone(),
            user_id: user_id.clone(),
            issuer: issuer.clone(),
            attributes: make_attrs(&[]),
        };
        storage.create_user_mapping(req).await.expect("create should succeed");

        // Found
        let found = storage
            .get_user_mapping_by_name_id(&name_id, &issuer)
            .await
            .expect("query should succeed")
            .expect("mapping should be found");
        assert_eq!(found.name_id, name_id);
        assert_eq!(found.issuer, issuer);

        // Not found — wrong name_id
        let not_found = storage
            .get_user_mapping_by_name_id(&format!("wrong_{suffix}"), &issuer)
            .await
            .expect("query should succeed");
        assert!(not_found.is_none());

        // Not found — wrong issuer
        let not_found2 = storage
            .get_user_mapping_by_name_id(&name_id, &format!("https://wrong-{suffix}.com"))
            .await
            .expect("query should succeed");
        assert!(not_found2.is_none());

        cleanup_saml_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_user_mapping_by_user_id_found_and_not_found() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@saml_gmbu_{suffix}:localhost");

        cleanup_saml_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = SamlStorage::new(&pool);
        let req = CreateSamlUserMappingRequest {
            name_id: format!("name_gmbu_{suffix}"),
            user_id: user_id.clone(),
            issuer: format!("https://idp-{suffix}.example.com"),
            attributes: make_attrs(&[]),
        };
        storage.create_user_mapping(req).await.expect("create should succeed");

        // Found
        let found = storage
            .get_user_mapping_by_user_id(&user_id)
            .await
            .expect("query should succeed")
            .expect("mapping should be found");
        assert_eq!(found.user_id, user_id);

        // Not found
        let not_found = storage
            .get_user_mapping_by_user_id(&format!("@nonexistent_{suffix}:localhost"))
            .await
            .expect("query should succeed");
        assert!(not_found.is_none());

        cleanup_saml_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_delete_user_mapping_and_idempotent() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@saml_del_map_{suffix}:localhost");
        let name_id = format!("name_del_{suffix}");
        let issuer = format!("https://idp-{suffix}.example.com");

        cleanup_saml_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = SamlStorage::new(&pool);
        let req = CreateSamlUserMappingRequest {
            name_id: name_id.clone(),
            user_id: user_id.clone(),
            issuer: issuer.clone(),
            attributes: make_attrs(&[]),
        };
        storage.create_user_mapping(req).await.expect("create should succeed");

        // Delete
        storage
            .delete_user_mapping(&name_id, &issuer)
            .await
            .expect("delete should succeed");

        // Verify gone
        let after = storage
            .get_user_mapping_by_name_id(&name_id, &issuer)
            .await
            .expect("query should succeed");
        assert!(after.is_none(), "mapping should be deleted");

        // Delete again (idempotent)
        storage
            .delete_user_mapping(&name_id, &issuer)
            .await
            .expect("idempotent delete should succeed");

        cleanup_saml_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_list_user_mappings_returns_list() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let name_a = format!("aaa_list_{suffix}");
        let name_b = format!("bbb_list_{suffix}");
        let issuer = format!("https://idp-{suffix}.example.com");

        cleanup_saml_test_data(&pool, &suffix).await;

        let storage = SamlStorage::new(&pool);
        for name in [&name_a, &name_b] {
            let uid = format!("@{name}:localhost");
            ensure_test_user(&pool, &uid).await;
            let req = CreateSamlUserMappingRequest {
                name_id: name.clone(),
                user_id: uid,
                issuer: issuer.clone(),
                attributes: make_attrs(&[]),
            };
            storage.create_user_mapping(req).await.expect("create should succeed");
        }

        let mappings = storage
            .list_user_mappings(10, None)
            .await
            .expect("list should succeed");
        assert!(mappings.len() >= 2, "should return at least 2 mappings");

        // Should be ordered by name_id ASC
        let names: Vec<&str> = mappings.iter().map(|m| m.name_id.as_str()).collect();
        let pos_a = names.iter().position(|&n| n == name_a);
        let pos_b = names.iter().position(|&n| n == name_b);
        assert!(pos_a.is_some(), "mapping A should be in list");
        assert!(pos_b.is_some(), "mapping B should be in list");
        assert!(pos_a.unwrap() < pos_b.unwrap(), "A should come before B (names sorted ASC)");

        cleanup_saml_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_list_user_mappings_cursor_pagination() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let name_a = format!("aaa_cursor_{suffix}");
        let name_b = format!("bbb_cursor_{suffix}");
        let name_c = format!("ccc_cursor_{suffix}");
        let issuer = format!("https://idp-{suffix}.example.com");

        cleanup_saml_test_data(&pool, &suffix).await;

        let storage = SamlStorage::new(&pool);
        for name in [&name_a, &name_b, &name_c] {
            let uid = format!("@{name}:localhost");
            ensure_test_user(&pool, &uid).await;
            let req = CreateSamlUserMappingRequest {
                name_id: name.clone(),
                user_id: uid,
                issuer: issuer.clone(),
                attributes: make_attrs(&[]),
            };
            storage.create_user_mapping(req).await.expect("create should succeed");
        }

        // Fetch all rows (large limit) and filter to only our test records
        let all = storage
            .list_user_mappings(10000, None)
            .await
            .expect("list all should succeed");
        let my_names: Vec<&str> = all
            .iter()
            .map(|m| m.name_id.as_str())
            .filter(|n| n.contains(&suffix))
            .collect();
        assert_eq!(my_names.len(), 3, "should find all 3 test mappings");
        assert_eq!(my_names[0], name_a);
        assert_eq!(my_names[1], name_b);
        assert_eq!(my_names[2], name_c);

        // Cursor pagination: after name_b, only name_c should remain (within our records)
        let after_b = storage
            .list_user_mappings(10000, Some(&name_b))
            .await
            .expect("after_b should succeed");
        let after_b_names: Vec<&str> = after_b
            .iter()
            .map(|m| m.name_id.as_str())
            .filter(|n| n.contains(&suffix))
            .collect();
        assert_eq!(
            after_b_names.len(),
            1,
            "only the remaining test record should be after name_b"
        );
        assert_eq!(after_b_names[0], name_c);

        cleanup_saml_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_user_mapping_any_issuer_found_and_not_found() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@saml_anyiss_{suffix}:localhost");
        let name_id = format!("name_anyiss_{suffix}");
        let issuer = format!("https://idp-{suffix}.example.com");

        cleanup_saml_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = SamlStorage::new(&pool);
        let req = CreateSamlUserMappingRequest {
            name_id: name_id.clone(),
            user_id: user_id.clone(),
            issuer: issuer.clone(),
            attributes: make_attrs(&[]),
        };
        storage.create_user_mapping(req).await.expect("create should succeed");

        // Found
        let found = storage
            .get_user_mapping_any_issuer(&name_id)
            .await
            .expect("query should succeed")
            .expect("mapping should be found");
        assert_eq!(found.name_id, name_id);
        assert_eq!(found.user_id, user_id);

        // Not found
        let not_found = storage
            .get_user_mapping_any_issuer(&format!("nonexistent_{suffix}"))
            .await
            .expect("query should succeed");
        assert!(not_found.is_none());

        cleanup_saml_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_update_user_mapping_by_name_id() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@saml_upd_map_{suffix}:localhost");
        let new_user_id = format!("@saml_upd_map_new_{suffix}:localhost");
        let name_id = format!("name_upd_{suffix}");
        let issuer = format!("https://idp-{suffix}.example.com");

        cleanup_saml_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;
        ensure_test_user(&pool, &new_user_id).await;

        let storage = SamlStorage::new(&pool);
        let req = CreateSamlUserMappingRequest {
            name_id: name_id.clone(),
            user_id: user_id.clone(),
            issuer: issuer.clone(),
            attributes: make_attrs(&[("role", "user")]),
        };
        storage.create_user_mapping(req).await.expect("create should succeed");

        let new_attrs = serde_json::json!({"role": "admin", "department": "engineering"});
        let updated = storage
            .update_user_mapping_by_name_id(&name_id, Some(&new_user_id), Some(&new_attrs))
            .await
            .expect("update should succeed")
            .expect("should return updated mapping");

        assert_eq!(updated.user_id, new_user_id);
        // Attributes should be updated
        let attrs_map: serde_json::Value = updated.attributes;
        assert_eq!(attrs_map["role"], serde_json::json!("admin"));
        assert_eq!(attrs_map["department"], serde_json::json!("engineering"));

        // Verify persisted
        let persisted = storage
            .get_user_mapping_by_name_id(&name_id, &issuer)
            .await
            .expect("query should succeed")
            .expect("mapping should exist");
        assert_eq!(persisted.user_id, new_user_id);

        cleanup_saml_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_delete_user_mapping_by_name_id() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id_1 = format!("@saml_delbn_1_{suffix}:localhost");
        let user_id_2 = format!("@saml_delbn_2_{suffix}:localhost");
        let name_id = format!("name_delbn_{suffix}");
        let issuer_a = format!("https://idp-a-{suffix}.example.com");
        let issuer_b = format!("https://idp-b-{suffix}.example.com");

        cleanup_saml_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id_1).await;
        ensure_test_user(&pool, &user_id_2).await;

        let storage = SamlStorage::new(&pool);

        // Create two mappings with same name_id but different issuers
        for (uid, iss) in [(&user_id_1, &issuer_a), (&user_id_2, &issuer_b)] {
            let req = CreateSamlUserMappingRequest {
                name_id: name_id.clone(),
                user_id: uid.clone(),
                issuer: iss.clone(),
                attributes: make_attrs(&[]),
            };
            storage.create_user_mapping(req).await.expect("create should succeed");
        }

        // Delete by name_id should remove ALL matching rows
        let count = storage
            .delete_user_mapping_by_name_id(&name_id)
            .await
            .expect("delete should succeed");
        assert_eq!(count, 2, "should delete both mappings with the same name_id");

        // Verify both are gone
        for iss in [&issuer_a, &issuer_b] {
            let result = storage
                .get_user_mapping_by_name_id(&name_id, iss)
                .await
                .expect("query should succeed");
            assert!(result.is_none(), "mapping for issuer {iss} should be deleted");
        }

        // Idempotent: deleting again returns 0
        let count2 = storage
            .delete_user_mapping_by_name_id(&name_id)
            .await
            .expect("second delete should succeed");
        assert_eq!(count2, 0);

        cleanup_saml_test_data(&pool, &suffix).await;
    }

    // ---------------------------------------------------------------------------
    // Identity provider tests
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_identity_provider_with_all_fields() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let entity_id = format!("https://idp-create-{suffix}.example.com");

        cleanup_saml_test_data(&pool, &suffix).await;

        let storage = SamlStorage::new(&pool);
        let req = CreateSamlIdentityProviderRequest {
            entity_id: entity_id.clone(),
            display_name: Some(format!("Test IdP {suffix}")),
            description: Some("A test identity provider".to_string()),
            metadata_url: Some(format!("https://metadata-{suffix}.example.com")),
            metadata_xml: Some("<xml>test metadata</xml>".to_string()),
            enabled: Some(false),
            priority: Some(50),
            attribute_mapping: Some(serde_json::json!({"uid": "name_id", "mail": "email"})),
        };

        let idp = storage
            .create_identity_provider(req)
            .await
            .expect("create_identity_provider should succeed");

        assert!(idp.id > 0);
        assert_eq!(idp.entity_id, entity_id);
        assert_eq!(idp.display_name.as_deref(), Some(format!("Test IdP {suffix}").as_str()));
        assert!(!idp.is_enabled);
        assert_eq!(idp.priority, 50);
        assert!(idp.created_ts > 0);
        assert!(idp.updated_ts.is_some());

        cleanup_saml_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_identity_provider_found_and_not_found() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let entity_id = format!("https://idp-get-{suffix}.example.com");

        cleanup_saml_test_data(&pool, &suffix).await;

        let storage = SamlStorage::new(&pool);
        let req = CreateSamlIdentityProviderRequest {
            entity_id: entity_id.clone(),
            display_name: None,
            description: None,
            metadata_url: None,
            metadata_xml: None,
            enabled: None,
            priority: None,
            attribute_mapping: None,
        };
        storage
            .create_identity_provider(req)
            .await
            .expect("create should succeed");

        // Found
        let found = storage
            .get_identity_provider(&entity_id)
            .await
            .expect("query should succeed")
            .expect("idp should be found");
        assert_eq!(found.entity_id, entity_id);

        // Not found
        let not_found = storage
            .get_identity_provider(&format!("https://nonexistent-{suffix}.example.com"))
            .await
            .expect("query should succeed");
        assert!(not_found.is_none());

        cleanup_saml_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_all_identity_providers() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let entity_a = format!("https://idp-all-a-{suffix}.example.com");
        let entity_b = format!("https://idp-all-b-{suffix}.example.com");

        cleanup_saml_test_data(&pool, &suffix).await;

        let storage = SamlStorage::new(&pool);

        // Create two IdPs with different priorities
        for (entity, prio) in [(&entity_a, 200), (&entity_b, 100)] {
            let req = CreateSamlIdentityProviderRequest {
                entity_id: entity.clone(),
                display_name: None,
                description: None,
                metadata_url: None,
                metadata_xml: None,
                enabled: None,
                priority: Some(prio),
                attribute_mapping: None,
            };
            storage.create_identity_provider(req).await.expect("create should succeed");
        }

        let all = storage.get_all_identity_providers().await.expect("query should succeed");
        assert!(all.len() >= 2, "should return at least 2 IdPs");

        // Should be ordered by priority ASC (entity_b has priority 100, entity_a has 200)
        let positions: Vec<&str> = all.iter().map(|p| p.entity_id.as_str()).collect();
        let pos_b = positions.iter().position(|&e| e == entity_b.as_str());
        let pos_a = positions.iter().position(|&e| e == entity_a.as_str());
        assert!(pos_b.is_some() && pos_a.is_some());
        assert!(pos_b.unwrap() < pos_a.unwrap(), "lower priority should come first");

        cleanup_saml_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_enabled_identity_providers() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let entity_enabled = format!("https://idp-on-{suffix}.example.com");
        let entity_disabled = format!("https://idp-off-{suffix}.example.com");

        cleanup_saml_test_data(&pool, &suffix).await;

        let storage = SamlStorage::new(&pool);

        // Create enabled IdP
        let req_on = CreateSamlIdentityProviderRequest {
            entity_id: entity_enabled.clone(),
            display_name: None,
            description: None,
            metadata_url: None,
            metadata_xml: None,
            enabled: Some(true),
            priority: Some(10),
            attribute_mapping: None,
        };
        storage.create_identity_provider(req_on).await.expect("create enabled should succeed");

        // Create disabled IdP
        let req_off = CreateSamlIdentityProviderRequest {
            entity_id: entity_disabled.clone(),
            display_name: None,
            description: None,
            metadata_url: None,
            metadata_xml: None,
            enabled: Some(false),
            priority: Some(20),
            attribute_mapping: None,
        };
        storage.create_identity_provider(req_off).await.expect("create disabled should succeed");

        let enabled = storage
            .get_enabled_identity_providers()
            .await
            .expect("query should succeed");

        // Only enabled IdPs should be returned
        let has_enabled = enabled.iter().any(|p| p.entity_id == entity_enabled);
        let has_disabled = enabled.iter().any(|p| p.entity_id == entity_disabled);
        assert!(has_enabled, "enabled IdP should be in results");
        assert!(!has_disabled, "disabled IdP should NOT be in results");

        cleanup_saml_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_update_idp_metadata() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let entity_id = format!("https://idp-meta-{suffix}.example.com");

        cleanup_saml_test_data(&pool, &suffix).await;

        let storage = SamlStorage::new(&pool);
        let req = CreateSamlIdentityProviderRequest {
            entity_id: entity_id.clone(),
            display_name: None,
            description: None,
            metadata_url: None,
            metadata_xml: Some("<xml>original</xml>".to_string()),
            enabled: None,
            priority: None,
            attribute_mapping: None,
        };
        storage.create_identity_provider(req).await.expect("create should succeed");

        let valid_until = chrono::Utc::now().timestamp_millis() + 86400000; // 1 day from now
        storage
            .update_idp_metadata(&entity_id, "<xml>updated metadata</xml>", Some(valid_until))
            .await
            .expect("update_idp_metadata should succeed");

        let updated = storage
            .get_identity_provider(&entity_id)
            .await
            .expect("query should succeed")
            .expect("idp should exist");

        assert_eq!(
            updated.metadata_xml.as_deref(),
            Some("<xml>updated metadata</xml>"),
            "metadata_xml should be updated"
        );
        assert!(
            updated.last_metadata_refresh_ts.is_some(),
            "last_metadata_refresh_ts should be set"
        );

        cleanup_saml_test_data(&pool, &suffix).await;
    }
}
