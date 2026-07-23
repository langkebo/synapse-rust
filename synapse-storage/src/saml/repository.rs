use std::collections::HashMap;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

use synapse_common::error::ApiError;
use tracing::{debug, info};

use super::models::*;

pub struct SamlStorage {
    pool: Arc<sqlx::PgPool>,
}

impl SamlStorage {
    pub fn new(pool: &Arc<sqlx::PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_session(&self, request: CreateSamlSessionRequest) -> Result<SamlSession, ApiError> {
        let now = current_timestamp_millis();
        let expires_at = current_timestamp_millis() + request.expires_in_seconds * 1000;
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
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();
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
        let now = current_timestamp_millis();
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
        let now_ts = current_timestamp_millis();
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
        let now = current_timestamp_millis();
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
