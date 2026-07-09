use std::sync::Arc;

use chrono::Utc;
use sqlx::PgPool;
use synapse_common::ApiError;

use super::models::*;

#[derive(Clone)]
pub struct CasStorage {
    pool: PgPool,
}

impl CasStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: (**pool).clone() }
    }

    pub async fn create_ticket(&self, request: CreateTicketRequest) -> Result<CasTicket, ApiError> {
        let now = Utc::now().timestamp_millis();
        let expires_at = Utc::now().timestamp_millis() + request.expires_in_seconds * 1000;

        let ticket = sqlx::query_as::<_, CasTicketRow>(
            r"
            INSERT INTO cas_tickets (ticket_id, user_id, service_url, created_ts, expires_at)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, ticket_id, user_id, service_url, created_ts, expires_at, consumed_at, consumed_by, is_valid
            ",
        )
        .bind(&request.ticket_id)
        .bind(&request.user_id)
        .bind(&request.service_url)
        .bind(now)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create CAS ticket", &e))?;

        Ok(ticket.into())
    }

    pub async fn validate_ticket(&self, ticket_id: &str, service_url: &str) -> Result<Option<CasTicket>, ApiError> {
        let now = Utc::now().timestamp_millis();

        let ticket = sqlx::query_as::<_, CasTicketRow>(
            r"
            UPDATE cas_tickets
            SET consumed_at = $1, is_valid = FALSE
            WHERE ticket_id = $2 AND service_url = $3 AND is_valid = TRUE AND expires_at > $1
            RETURNING id, ticket_id, user_id, service_url, created_ts, expires_at, consumed_at, consumed_by, is_valid
            ",
        )
        .bind(now)
        .bind(ticket_id)
        .bind(service_url)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to validate CAS ticket", &e))?;

        Ok(ticket.map(CasTicket::from))
    }

    pub async fn get_ticket(&self, ticket_id: &str) -> Result<Option<CasTicket>, ApiError> {
        let ticket = sqlx::query_as::<_, CasTicketRow>(
            r"
            SELECT id, ticket_id, user_id, service_url, created_ts, expires_at, consumed_at, consumed_by, is_valid
            FROM cas_tickets
            WHERE ticket_id = $1
            ",
        )
        .bind(ticket_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get CAS ticket", &e))?;

        Ok(ticket.map(CasTicket::from))
    }

    pub async fn delete_ticket(&self, ticket_id: &str) -> Result<bool, ApiError> {
        let result = sqlx::query(
            r"
            DELETE FROM cas_tickets
            WHERE ticket_id = $1
            ",
        )
        .bind(ticket_id)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to delete CAS ticket", &e))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn cleanup_expired_tickets(&self) -> Result<u64, ApiError> {
        let now = Utc::now().timestamp_millis();
        let result = sqlx::query(
            r"
            DELETE FROM cas_tickets
            WHERE expires_at < $1
            ",
        )
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to cleanup expired tickets", &e))?;

        Ok(result.rows_affected())
    }

    pub async fn create_proxy_ticket(&self, request: CreateProxyTicketRequest) -> Result<CasProxyTicket, ApiError> {
        let now = Utc::now().timestamp_millis();
        let expires_at = Utc::now().timestamp_millis() + request.expires_in_seconds * 1000;

        let ticket = sqlx::query_as::<_, CasProxyTicketRow>(
            r"
            INSERT INTO cas_proxy_tickets (proxy_ticket_id, user_id, service_url, pgt_url, created_ts, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, proxy_ticket_id, user_id, service_url, pgt_url, created_ts, expires_at, consumed_at, is_valid
            ",
        )
        .bind(&request.proxy_ticket_id)
        .bind(&request.user_id)
        .bind(&request.service_url)
        .bind(&request.pgt_url)
        .bind(now)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create CAS proxy ticket", &e))?;

        Ok(ticket.into())
    }

    pub async fn validate_proxy_ticket(
        &self,
        proxy_ticket_id: &str,
        service_url: &str,
    ) -> Result<Option<CasProxyTicket>, ApiError> {
        let now = Utc::now().timestamp_millis();

        let ticket = sqlx::query_as::<_, CasProxyTicketRow>(
            r"
            UPDATE cas_proxy_tickets
            SET consumed_at = $1, is_valid = FALSE
            WHERE proxy_ticket_id = $2 AND service_url = $3 AND is_valid = TRUE AND expires_at > $1
            RETURNING id, proxy_ticket_id, user_id, service_url, pgt_url, created_ts, expires_at, consumed_at, is_valid
            ",
        )
        .bind(now)
        .bind(proxy_ticket_id)
        .bind(service_url)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to validate CAS proxy ticket", &e))?;

        Ok(ticket.map(CasProxyTicket::from))
    }

    pub async fn create_pgt(&self, request: CreatePgtRequest) -> Result<CasProxyGrantingTicket, ApiError> {
        let now = Utc::now().timestamp_millis();
        let expires_at = Utc::now().timestamp_millis() + request.expires_in_seconds * 1000;

        let pgt = sqlx::query_as::<_, CasProxyGrantingTicket>(
            r"
            INSERT INTO cas_proxy_granting_tickets (pgt_id, user_id, service_url, iou, created_ts, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, pgt_id, user_id, service_url, iou, created_ts, expires_at, is_valid
            ",
        )
        .bind(&request.pgt_id)
        .bind(&request.user_id)
        .bind(&request.service_url)
        .bind(&request.iou)
        .bind(now)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create CAS PGT", &e))?;

        Ok(pgt)
    }

    pub async fn get_pgt(&self, pgt_id: &str) -> Result<Option<CasProxyGrantingTicket>, ApiError> {
        let pgt = sqlx::query_as::<_, CasProxyGrantingTicket>(
            r"
            SELECT id, pgt_id, user_id, service_url, iou, created_ts, expires_at, is_valid
            FROM cas_proxy_granting_tickets
            WHERE pgt_id = $1 AND is_valid = TRUE
            ",
        )
        .bind(pgt_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get CAS PGT", &e))?;

        Ok(pgt)
    }

    pub async fn get_pgt_by_iou(&self, iou: &str) -> Result<Option<CasProxyGrantingTicket>, ApiError> {
        let pgt = sqlx::query_as::<_, CasProxyGrantingTicket>(
            r"
            SELECT id, pgt_id, user_id, service_url, iou, created_ts, expires_at, is_valid
            FROM cas_proxy_granting_tickets
            WHERE iou = $1 AND is_valid = TRUE
            ",
        )
        .bind(iou)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get CAS PGT by IOU", &e))?;

        Ok(pgt)
    }

    pub async fn register_service(&self, request: RegisterServiceRequest) -> Result<CasRegisteredService, ApiError> {
        let allowed_attributes =
            serde_json::to_value(request.allowed_attributes.unwrap_or_default()).unwrap_or(serde_json::json!([]));
        let allowed_proxy_callbacks =
            serde_json::to_value(request.allowed_proxy_callbacks.unwrap_or_default()).unwrap_or(serde_json::json!([]));
        let now = Utc::now().timestamp_millis();

        let service = sqlx::query_as::<_, CasRegisteredServiceRow>(
            r"
            INSERT INTO cas_services (
                service_id, name, description, service_url_pattern,
                allowed_attributes, allowed_proxy_callbacks,
                is_require_secure, is_single_logout, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
            RETURNING id, service_id, name, description, service_url_pattern,
                      allowed_attributes, allowed_proxy_callbacks,
                      is_enabled, is_require_secure, is_single_logout, created_ts, updated_ts
            ",
        )
        .bind(&request.service_id)
        .bind(&request.name)
        .bind(&request.description)
        .bind(&request.service_url_pattern)
        .bind(&allowed_attributes)
        .bind(&allowed_proxy_callbacks)
        .bind(request.is_require_secure.unwrap_or(true))
        .bind(request.is_single_logout.unwrap_or(false))
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to register CAS service", &e))?;

        Ok(service.into())
    }

    pub async fn get_service(&self, service_id: &str) -> Result<Option<CasRegisteredService>, ApiError> {
        let service = sqlx::query_as::<_, CasRegisteredServiceRow>(
            r"
            SELECT id, service_id, name, description, service_url_pattern,
                   allowed_attributes, allowed_proxy_callbacks,
                   is_enabled, is_require_secure, is_single_logout, created_ts, updated_ts
            FROM cas_services
            WHERE service_id = $1
            ",
        )
        .bind(service_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get CAS service", &e))?;

        Ok(service.map(CasRegisteredService::from))
    }

    pub async fn get_service_by_url(&self, service_url: &str) -> Result<Option<CasRegisteredService>, ApiError> {
        let service = sqlx::query_as::<_, CasRegisteredServiceRow>(
            r"
            SELECT id, service_id, name, description, service_url_pattern,
                   allowed_attributes, allowed_proxy_callbacks,
                   is_enabled, is_require_secure, is_single_logout, created_ts, updated_ts
            FROM cas_services
            WHERE $1 ~ service_url_pattern AND is_enabled = TRUE
            ",
        )
        .bind(service_url)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get CAS service by URL", &e))?;

        Ok(service.map(CasRegisteredService::from))
    }

    pub async fn list_services(&self) -> Result<Vec<CasRegisteredService>, ApiError> {
        let services = sqlx::query_as::<_, CasRegisteredServiceRow>(
            r"
            SELECT id, service_id, name, description, service_url_pattern,
                   allowed_attributes, allowed_proxy_callbacks,
                   is_enabled, is_require_secure, is_single_logout, created_ts, updated_ts
            FROM cas_services
            ORDER BY created_ts DESC
            ",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to list CAS services", &e))?;

        Ok(services.into_iter().map(CasRegisteredService::from).collect())
    }

    pub async fn delete_service(&self, service_id: &str) -> Result<bool, ApiError> {
        let result = sqlx::query(
            r"
            DELETE FROM cas_services
            WHERE service_id = $1
            ",
        )
        .bind(service_id)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to delete CAS service", &e))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn set_user_attribute(
        &self,
        user_id: &str,
        attribute_name: &str,
        attribute_value: &str,
    ) -> Result<CasUserAttribute, ApiError> {
        let now = Utc::now().timestamp_millis();

        let attr = sqlx::query_as::<_, CasUserAttributeRow>(
            r"
            INSERT INTO cas_user_attributes (user_id, attribute_name, attribute_value, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $4)
            ON CONFLICT (user_id, attribute_name)
            DO UPDATE SET attribute_value = $3, updated_ts = $4
            RETURNING id, user_id, attribute_name, attribute_value, created_ts, updated_ts
            ",
        )
        .bind(user_id)
        .bind(attribute_name)
        .bind(attribute_value)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to set CAS user attribute", &e))?;

        Ok(attr.into())
    }

    pub async fn get_user_attributes(&self, user_id: &str) -> Result<Vec<CasUserAttribute>, ApiError> {
        let attrs = sqlx::query_as::<_, CasUserAttributeRow>(
            r"
            SELECT id, user_id, attribute_name, attribute_value, created_ts, updated_ts
            FROM cas_user_attributes
            WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get CAS user attributes", &e))?;

        Ok(attrs.into_iter().map(CasUserAttribute::from).collect())
    }

    pub async fn create_slo_session(
        &self,
        session_id: &str,
        user_id: &str,
        service_url: &str,
        ticket_id: Option<&str>,
    ) -> Result<CasSloSession, ApiError> {
        let now = Utc::now().timestamp_millis();
        let session = sqlx::query_as::<_, CasSloSessionRow>(
            r"
            INSERT INTO cas_slo_sessions (session_id, user_id, service_url, ticket_id, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, session_id, user_id, service_url, ticket_id, created_ts, logout_sent_at
            ",
        )
        .bind(session_id)
        .bind(user_id)
        .bind(service_url)
        .bind(ticket_id)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create CAS SLO session", &e))?;

        Ok(session.into())
    }

    pub async fn mark_slo_sent(&self, session_id: &str) -> Result<bool, ApiError> {
        let now = Utc::now().timestamp_millis();
        let result = sqlx::query(
            r"
            UPDATE cas_slo_sessions
            SET logout_sent_at = $1
            WHERE session_id = $2 AND logout_sent_at IS NULL
            ",
        )
        .bind(now)
        .bind(session_id)
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to mark SLO sent", &e))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn get_active_slo_sessions(&self, user_id: &str) -> Result<Vec<CasSloSession>, ApiError> {
        let sessions = sqlx::query_as::<_, CasSloSessionRow>(
            r"
            SELECT id, session_id, user_id, service_url, ticket_id, created_ts, logout_sent_at
            FROM cas_slo_sessions
            WHERE user_id = $1 AND logout_sent_at IS NULL
            ",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get active SLO sessions", &e))?;

        Ok(sessions.into_iter().map(CasSloSession::from).collect())
    }
}
