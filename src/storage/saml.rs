use crate::common::error::ApiError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SamlSession {
    pub id: i32,
    pub session_id: String,
    pub user_id: String,
    pub name_id: Option<String>,
    pub issuer: Option<String>,
    pub session_index: Option<String>,
    pub attributes: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SamlUserMapping {
    pub id: i32,
    pub name_id: String,
    pub user_id: String,
    pub issuer: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_authenticated_at: DateTime<Utc>,
    pub authentication_count: i32,
    pub attributes: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SamlIdentityProvider {
    pub id: i32,
    pub entity_id: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub metadata_url: Option<String>,
    pub metadata_xml: Option<String>,
    pub enabled: bool,
    pub priority: i32,
    pub attribute_mapping: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_metadata_refresh_at: Option<DateTime<Utc>>,
    pub metadata_valid_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SamlAuthEvent {
    pub id: i32,
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
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SamlLogoutRequest {
    pub id: i32,
    pub request_id: String,
    pub session_id: Option<String>,
    pub user_id: Option<String>,
    pub name_id: Option<String>,
    pub issuer: Option<String>,
    pub reason: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub processed_at: Option<DateTime<Utc>>,
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

#[derive(Clone)]
pub struct SamlStorage {
    pool: Arc<sqlx::PgPool>,
}

impl SamlStorage {
    pub fn new(pool: &Arc<sqlx::PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_session(
        &self,
        request: CreateSamlSessionRequest,
    ) -> Result<SamlSession, ApiError> {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(request.expires_in_seconds);
        let attributes_json = serde_json::to_value(&request.attributes).unwrap_or(serde_json::json!({}));

        let row = sqlx::query_as::<_, SamlSession>(
            r#"
            INSERT INTO saml_sessions (
                session_id, user_id, name_id, issuer, session_index, attributes,
                created_at, expires_at, last_used_at, status
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
        .map_err(|e| ApiError::internal(format!("Failed to create SAML session: {}", e)))?;

        info!("Created SAML session: {} for user: {}", request.session_id, request.user_id);
        Ok(row)
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Option<SamlSession>, ApiError> {
        let row = sqlx::query_as::<_, SamlSession>(
            "SELECT * FROM saml_sessions WHERE session_id = $1 AND status = 'active'"
        )
        .bind(session_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get SAML session: {}", e)))?;

        Ok(row)
    }

    pub async fn get_session_by_user(&self, user_id: &str) -> Result<Option<SamlSession>, ApiError> {
        let row = sqlx::query_as::<_, SamlSession>(
            r#"
            SELECT * FROM saml_sessions 
            WHERE user_id = $1 AND status = 'active' AND expires_at > NOW()
            ORDER BY created_at DESC 
            LIMIT 1
            "#
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get SAML session by user: {}", e)))?;

        Ok(row)
    }

    pub async fn update_session_last_used(&self, session_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            "UPDATE saml_sessions SET last_used_at = NOW() WHERE session_id = $1"
        )
        .bind(session_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update session last used: {}", e)))?;

        Ok(())
    }

    pub async fn invalidate_session(&self, session_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            "UPDATE saml_sessions SET status = 'invalidated' WHERE session_id = $1"
        )
        .bind(session_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to invalidate SAML session: {}", e)))?;

        info!("Invalidated SAML session: {}", session_id);
        Ok(())
    }

    pub async fn cleanup_expired_sessions(&self) -> Result<u64, ApiError> {
        let result = sqlx::query(
            "DELETE FROM saml_sessions WHERE expires_at < NOW() OR status = 'invalidated'"
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to cleanup expired sessions: {}", e)))?;

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
        let now = Utc::now();
        let attributes_json = serde_json::to_value(&request.attributes).unwrap_or(serde_json::json!({}));

        let row = sqlx::query_as::<_, SamlUserMapping>(
            r#"
            INSERT INTO saml_user_mapping (
                name_id, user_id, issuer, first_seen_at, last_authenticated_at,
                authentication_count, attributes
            )
            VALUES ($1, $2, $3, $4, $4, 1, $5)
            ON CONFLICT (name_id, issuer) DO UPDATE SET
                last_authenticated_at = NOW(),
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
        .map_err(|e| ApiError::internal(format!("Failed to create SAML user mapping: {}", e)))?;

        info!("Created/updated SAML user mapping: {} -> {}", request.name_id, request.user_id);
        Ok(row)
    }

    pub async fn get_user_mapping_by_name_id(
        &self,
        name_id: &str,
        issuer: &str,
    ) -> Result<Option<SamlUserMapping>, ApiError> {
        let row = sqlx::query_as::<_, SamlUserMapping>(
            "SELECT * FROM saml_user_mapping WHERE name_id = $1 AND issuer = $2"
        )
        .bind(name_id)
        .bind(issuer)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get SAML user mapping: {}", e)))?;

        Ok(row)
    }

    pub async fn get_user_mapping_by_user_id(
        &self,
        user_id: &str,
    ) -> Result<Option<SamlUserMapping>, ApiError> {
        let row = sqlx::query_as::<_, SamlUserMapping>(
            "SELECT * FROM saml_user_mapping WHERE user_id = $1"
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get SAML user mapping: {}", e)))?;

        Ok(row)
    }

    pub async fn delete_user_mapping(&self, name_id: &str, issuer: &str) -> Result<(), ApiError> {
        sqlx::query("DELETE FROM saml_user_mapping WHERE name_id = $1 AND issuer = $2")
            .bind(name_id)
            .bind(issuer)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete SAML user mapping: {}", e)))?;

        info!("Deleted SAML user mapping: {} ({})", name_id, issuer);
        Ok(())
    }

    pub async fn create_identity_provider(
        &self,
        request: CreateSamlIdentityProviderRequest,
    ) -> Result<SamlIdentityProvider, ApiError> {
        let now = Utc::now();
        let attribute_mapping = request.attribute_mapping.unwrap_or(serde_json::json!({}));

        let row = sqlx::query_as::<_, SamlIdentityProvider>(
            r#"
            INSERT INTO saml_identity_providers (
                entity_id, display_name, description, metadata_url, metadata_xml,
                enabled, priority, attribute_mapping, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
            ON CONFLICT (entity_id) DO UPDATE SET
                display_name = EXCLUDED.display_name,
                description = EXCLUDED.description,
                metadata_url = EXCLUDED.metadata_url,
                metadata_xml = EXCLUDED.metadata_xml,
                enabled = EXCLUDED.enabled,
                priority = EXCLUDED.priority,
                attribute_mapping = EXCLUDED.attribute_mapping,
                updated_at = NOW()
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
        .map_err(|e| ApiError::internal(format!("Failed to create SAML IdP: {}", e)))?;

        info!("Created/updated SAML identity provider: {}", request.entity_id);
        Ok(row)
    }

    pub async fn get_identity_provider(
        &self,
        entity_id: &str,
    ) -> Result<Option<SamlIdentityProvider>, ApiError> {
        let row = sqlx::query_as::<_, SamlIdentityProvider>(
            "SELECT * FROM saml_identity_providers WHERE entity_id = $1"
        )
        .bind(entity_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get SAML IdP: {}", e)))?;

        Ok(row)
    }

    pub async fn get_all_identity_providers(&self) -> Result<Vec<SamlIdentityProvider>, ApiError> {
        let rows = sqlx::query_as::<_, SamlIdentityProvider>(
            "SELECT * FROM saml_identity_providers ORDER BY priority ASC"
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get SAML IdPs: {}", e)))?;

        Ok(rows)
    }

    pub async fn get_enabled_identity_providers(&self) -> Result<Vec<SamlIdentityProvider>, ApiError> {
        let rows = sqlx::query_as::<_, SamlIdentityProvider>(
            "SELECT * FROM saml_identity_providers WHERE enabled = true ORDER BY priority ASC"
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get enabled SAML IdPs: {}", e)))?;

        Ok(rows)
    }

    pub async fn update_idp_metadata(
        &self,
        entity_id: &str,
        metadata_xml: &str,
        valid_until: Option<DateTime<Utc>>,
    ) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            UPDATE saml_identity_providers 
            SET metadata_xml = $1, 
                last_metadata_refresh_at = NOW(),
                metadata_valid_until = $2,
                updated_at = NOW()
            WHERE entity_id = $3
            "#
        )
        .bind(metadata_xml)
        .bind(valid_until)
        .bind(entity_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update IdP metadata: {}", e)))?;

        debug!("Updated SAML IdP metadata: {}", entity_id);
        Ok(())
    }

    pub async fn delete_identity_provider(&self, entity_id: &str) -> Result<(), ApiError> {
        sqlx::query("DELETE FROM saml_identity_providers WHERE entity_id = $1")
            .bind(entity_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete SAML IdP: {}", e)))?;

        info!("Deleted SAML identity provider: {}", entity_id);
        Ok(())
    }

    pub async fn create_auth_event(
        &self,
        request: CreateSamlAuthEventRequest,
    ) -> Result<SamlAuthEvent, ApiError> {
        let attributes_json = serde_json::to_value(&request.attributes).unwrap_or(serde_json::json!({}));

        let row = sqlx::query_as::<_, SamlAuthEvent>(
            r#"
            INSERT INTO saml_auth_events (
                session_id, user_id, name_id, issuer, event_type, status,
                error_message, ip_address, user_agent, request_id, attributes
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
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
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create SAML auth event: {}", e)))?;

        debug!("Created SAML auth event: {} - {}", request.event_type, request.status);
        Ok(row)
    }

    pub async fn get_auth_events_by_user(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<SamlAuthEvent>, ApiError> {
        let rows = sqlx::query_as::<_, SamlAuthEvent>(
            r#"
            SELECT * FROM saml_auth_events 
            WHERE user_id = $1 
            ORDER BY created_at DESC 
            LIMIT $2
            "#
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get auth events: {}", e)))?;

        Ok(rows)
    }

    pub async fn create_logout_request(
        &self,
        request: CreateSamlLogoutRequestRequest,
    ) -> Result<SamlLogoutRequest, ApiError> {
        let row = sqlx::query_as::<_, SamlLogoutRequest>(
            r#"
            INSERT INTO saml_logout_requests (
                request_id, session_id, user_id, name_id, issuer, reason, status
            )
            VALUES ($1, $2, $3, $4, $5, $6, 'pending')
            RETURNING *
            "#,
        )
        .bind(&request.request_id)
        .bind(&request.session_id)
        .bind(&request.user_id)
        .bind(&request.name_id)
        .bind(&request.issuer)
        .bind(&request.reason)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create logout request: {}", e)))?;

        info!("Created SAML logout request: {}", request.request_id);
        Ok(row)
    }

    pub async fn get_logout_request(&self, request_id: &str) -> Result<Option<SamlLogoutRequest>, ApiError> {
        let row = sqlx::query_as::<_, SamlLogoutRequest>(
            "SELECT * FROM saml_logout_requests WHERE request_id = $1"
        )
        .bind(request_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get logout request: {}", e)))?;

        Ok(row)
    }

    pub async fn process_logout_request(&self, request_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            "UPDATE saml_logout_requests SET status = 'processed', processed_at = NOW() WHERE request_id = $1"
        )
        .bind(request_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to process logout request: {}", e)))?;

        info!("Processed SAML logout request: {}", request_id);
        Ok(())
    }

    pub async fn cleanup_old_auth_events(&self, days: i64) -> Result<u64, ApiError> {
        let result = sqlx::query(
            "DELETE FROM saml_auth_events WHERE created_at < NOW() - INTERVAL '1 day' * $1"
        )
        .bind(days)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to cleanup auth events: {}", e)))?;

        let count = result.rows_affected();
        if count > 0 {
            info!("Cleaned up {} old SAML auth events", count);
        }
        Ok(count)
    }
}
