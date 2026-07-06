use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;
use synapse_common::ApiError;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CasTicket {
    pub id: i64,
    pub ticket_id: String,
    pub user_id: String,
    pub service_url: String,
    pub created_ts: i64,
    pub expires_at: i64,
    pub consumed_ts: Option<i64>,
    pub consumed_by: Option<String>,
    pub is_valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CasProxyTicket {
    pub id: i64,
    pub proxy_ticket_id: String,
    pub user_id: String,
    pub service_url: String,
    pub pgt_url: Option<String>,
    pub created_ts: i64,
    pub expires_at: i64,
    pub consumed_ts: Option<i64>,
    pub is_valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CasProxyGrantingTicket {
    pub id: i64,
    pub pgt_id: String,
    pub user_id: String,
    pub service_url: String,
    pub iou: Option<String>,
    pub created_ts: i64,
    pub expires_at: i64,
    pub is_valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CasRegisteredService {
    pub id: i64,
    pub service_id: String,
    pub name: String,
    pub description: Option<String>,
    pub service_url_pattern: String,
    pub allowed_attributes: serde_json::Value,
    pub allowed_proxy_callbacks: serde_json::Value,
    pub is_enabled: bool,
    pub is_require_secure: bool,
    pub is_single_logout: bool,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CasSloSession {
    pub id: i64,
    pub session_id: String,
    pub user_id: String,
    pub service_url: String,
    pub ticket_id: Option<String>,
    pub created_ts: i64,
    pub logout_sent_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CasUserAttribute {
    pub id: i64,
    pub user_id: String,
    pub attribute_name: String,
    pub attribute_value: String,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTicketRequest {
    pub ticket_id: String,
    pub user_id: String,
    pub service_url: String,
    pub expires_in_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateTicketRequest {
    pub ticket_id: String,
    pub service_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProxyTicketRequest {
    pub proxy_ticket_id: String,
    pub user_id: String,
    pub service_url: String,
    pub pgt_url: Option<String>,
    pub expires_in_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePgtRequest {
    pub pgt_id: String,
    pub user_id: String,
    pub service_url: String,
    pub iou: Option<String>,
    pub expires_in_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterServiceRequest {
    pub service_id: String,
    pub name: String,
    pub description: Option<String>,
    pub service_url_pattern: String,
    pub allowed_attributes: Option<Vec<String>>,
    pub allowed_proxy_callbacks: Option<Vec<String>>,
    pub is_require_secure: Option<bool>,
    pub is_single_logout: Option<bool>,
}

// ---- Row wrappers --------------------------------------------------------
//
// v8 schema uses `consumed_at` / `logout_sent_at` / `updated_ts` (nullable)
// but the public models keep `_ts` suffixes and `updated_ts: i64` (non-null).
// These row types bridge the two for sqlx::query_as without changing the
// public API. The drift itself is tracked in `M3-ISSUE-3`.

#[derive(Debug, Clone, FromRow)]
struct CasTicketRow {
    pub id: i64,
    pub ticket_id: String,
    pub user_id: String,
    pub service_url: String,
    pub created_ts: i64,
    pub expires_at: i64,
    pub consumed_at: Option<i64>,
    pub consumed_by: Option<String>,
    pub is_valid: bool,
}

impl From<CasTicketRow> for CasTicket {
    fn from(row: CasTicketRow) -> Self {
        CasTicket {
            id: row.id,
            ticket_id: row.ticket_id,
            user_id: row.user_id,
            service_url: row.service_url,
            created_ts: row.created_ts,
            expires_at: row.expires_at,
            consumed_ts: row.consumed_at,
            consumed_by: row.consumed_by,
            is_valid: row.is_valid,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
struct CasProxyTicketRow {
    pub id: i64,
    pub proxy_ticket_id: String,
    pub user_id: String,
    pub service_url: String,
    pub pgt_url: Option<String>,
    pub created_ts: i64,
    pub expires_at: i64,
    pub consumed_at: Option<i64>,
    pub is_valid: bool,
}

impl From<CasProxyTicketRow> for CasProxyTicket {
    fn from(row: CasProxyTicketRow) -> Self {
        CasProxyTicket {
            id: row.id,
            proxy_ticket_id: row.proxy_ticket_id,
            user_id: row.user_id,
            service_url: row.service_url,
            pgt_url: row.pgt_url,
            created_ts: row.created_ts,
            expires_at: row.expires_at,
            consumed_ts: row.consumed_at,
            is_valid: row.is_valid,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
struct CasRegisteredServiceRow {
    pub id: i64,
    pub service_id: String,
    pub name: String,
    pub description: Option<String>,
    pub service_url_pattern: String,
    pub allowed_attributes: serde_json::Value,
    pub allowed_proxy_callbacks: serde_json::Value,
    pub is_enabled: bool,
    pub is_require_secure: bool,
    pub is_single_logout: bool,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

impl From<CasRegisteredServiceRow> for CasRegisteredService {
    fn from(row: CasRegisteredServiceRow) -> Self {
        CasRegisteredService {
            id: row.id,
            service_id: row.service_id,
            name: row.name,
            description: row.description,
            service_url_pattern: row.service_url_pattern,
            allowed_attributes: row.allowed_attributes,
            allowed_proxy_callbacks: row.allowed_proxy_callbacks,
            is_enabled: row.is_enabled,
            is_require_secure: row.is_require_secure,
            is_single_logout: row.is_single_logout,
            created_ts: row.created_ts,
            updated_ts: row.updated_ts.unwrap_or(0),
        }
    }
}

#[derive(Debug, Clone, FromRow)]
struct CasSloSessionRow {
    pub id: i64,
    pub session_id: String,
    pub user_id: String,
    pub service_url: String,
    pub ticket_id: Option<String>,
    pub created_ts: i64,
    pub logout_sent_at: Option<i64>,
}

impl From<CasSloSessionRow> for CasSloSession {
    fn from(row: CasSloSessionRow) -> Self {
        CasSloSession {
            id: row.id,
            session_id: row.session_id,
            user_id: row.user_id,
            service_url: row.service_url,
            ticket_id: row.ticket_id,
            created_ts: row.created_ts,
            logout_sent_ts: row.logout_sent_at,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
struct CasUserAttributeRow {
    pub id: i64,
    pub user_id: String,
    pub attribute_name: String,
    pub attribute_value: String,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

impl From<CasUserAttributeRow> for CasUserAttribute {
    fn from(row: CasUserAttributeRow) -> Self {
        CasUserAttribute {
            id: row.id,
            user_id: row.user_id,
            attribute_name: row.attribute_name,
            attribute_value: row.attribute_value,
            created_ts: row.created_ts,
            updated_ts: row.updated_ts.unwrap_or(0),
        }
    }
}

#[async_trait]
pub trait CasStoreApi: Send + Sync {
    async fn create_ticket(&self, request: CreateTicketRequest) -> Result<CasTicket, ApiError>;
    async fn validate_ticket(&self, ticket_id: &str, service_url: &str) -> Result<Option<CasTicket>, ApiError>;
    async fn get_ticket(&self, ticket_id: &str) -> Result<Option<CasTicket>, ApiError>;
    async fn get_user_attributes(&self, user_id: &str) -> Result<Vec<CasUserAttribute>, ApiError>;
    async fn create_pgt(&self, request: CreatePgtRequest) -> Result<CasProxyGrantingTicket, ApiError>;
    async fn get_pgt(&self, pgt_id: &str) -> Result<Option<CasProxyGrantingTicket>, ApiError>;
    async fn create_proxy_ticket(&self, request: CreateProxyTicketRequest) -> Result<CasProxyTicket, ApiError>;
    async fn validate_proxy_ticket(
        &self,
        proxy_ticket_id: &str,
        service_url: &str,
    ) -> Result<Option<CasProxyTicket>, ApiError>;
    async fn register_service(&self, request: RegisterServiceRequest) -> Result<CasRegisteredService, ApiError>;
    async fn get_service(&self, service_id: &str) -> Result<Option<CasRegisteredService>, ApiError>;
    async fn get_service_by_url(&self, service_url: &str) -> Result<Option<CasRegisteredService>, ApiError>;
    async fn list_services(&self) -> Result<Vec<CasRegisteredService>, ApiError>;
    async fn delete_service(&self, service_id: &str) -> Result<bool, ApiError>;
    async fn set_user_attribute(
        &self,
        user_id: &str,
        attribute_name: &str,
        attribute_value: &str,
    ) -> Result<CasUserAttribute, ApiError>;
    async fn get_active_slo_sessions(&self, user_id: &str) -> Result<Vec<CasSloSession>, ApiError>;
    async fn cleanup_expired_tickets(&self) -> Result<u64, ApiError>;
}

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

#[async_trait]
impl CasStoreApi for CasStorage {
    async fn create_ticket(&self, request: CreateTicketRequest) -> Result<CasTicket, ApiError> {
        self.create_ticket(request).await
    }
    async fn validate_ticket(&self, ticket_id: &str, service_url: &str) -> Result<Option<CasTicket>, ApiError> {
        self.validate_ticket(ticket_id, service_url).await
    }
    async fn get_ticket(&self, ticket_id: &str) -> Result<Option<CasTicket>, ApiError> {
        self.get_ticket(ticket_id).await
    }
    async fn get_user_attributes(&self, user_id: &str) -> Result<Vec<CasUserAttribute>, ApiError> {
        self.get_user_attributes(user_id).await
    }
    async fn create_pgt(&self, request: CreatePgtRequest) -> Result<CasProxyGrantingTicket, ApiError> {
        self.create_pgt(request).await
    }
    async fn get_pgt(&self, pgt_id: &str) -> Result<Option<CasProxyGrantingTicket>, ApiError> {
        self.get_pgt(pgt_id).await
    }
    async fn create_proxy_ticket(&self, request: CreateProxyTicketRequest) -> Result<CasProxyTicket, ApiError> {
        self.create_proxy_ticket(request).await
    }
    async fn validate_proxy_ticket(
        &self,
        proxy_ticket_id: &str,
        service_url: &str,
    ) -> Result<Option<CasProxyTicket>, ApiError> {
        self.validate_proxy_ticket(proxy_ticket_id, service_url).await
    }
    async fn register_service(&self, request: RegisterServiceRequest) -> Result<CasRegisteredService, ApiError> {
        self.register_service(request).await
    }
    async fn get_service(&self, service_id: &str) -> Result<Option<CasRegisteredService>, ApiError> {
        self.get_service(service_id).await
    }
    async fn get_service_by_url(&self, service_url: &str) -> Result<Option<CasRegisteredService>, ApiError> {
        self.get_service_by_url(service_url).await
    }
    async fn list_services(&self) -> Result<Vec<CasRegisteredService>, ApiError> {
        self.list_services().await
    }
    async fn delete_service(&self, service_id: &str) -> Result<bool, ApiError> {
        self.delete_service(service_id).await
    }
    async fn set_user_attribute(
        &self,
        user_id: &str,
        attribute_name: &str,
        attribute_value: &str,
    ) -> Result<CasUserAttribute, ApiError> {
        self.set_user_attribute(user_id, attribute_name, attribute_value).await
    }
    async fn get_active_slo_sessions(&self, user_id: &str) -> Result<Vec<CasSloSession>, ApiError> {
        self.get_active_slo_sessions(user_id).await
    }
    async fn cleanup_expired_tickets(&self) -> Result<u64, ApiError> {
        self.cleanup_expired_tickets().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cas_ticket_creation() {
        let ticket = CasTicket {
            id: 1,
            ticket_id: "ST-12345678".to_string(),
            user_id: "@alice:example.com".to_string(),
            service_url: "https://app.example.com".to_string(),
            created_ts: 1234567800000,
            expires_at: 1234567890000,
            consumed_ts: None,
            consumed_by: None,
            is_valid: true,
        };
        assert_eq!(ticket.ticket_id, "ST-12345678");
        assert!(ticket.is_valid);
    }

    #[test]
    fn test_cas_proxy_ticket_creation() {
        let ticket = CasProxyTicket {
            id: 1,
            proxy_ticket_id: "PT-12345678".to_string(),
            user_id: "@alice:example.com".to_string(),
            service_url: "https://app.example.com".to_string(),
            pgt_url: Some("https://pgt.example.com".to_string()),
            created_ts: 1234567800000,
            expires_at: 1234567890000,
            consumed_ts: None,
            is_valid: true,
        };
        assert_eq!(ticket.proxy_ticket_id, "PT-12345678");
        assert!(ticket.is_valid);
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::env;
    use std::sync::Arc;

    // ---- test infrastructure ----

    async fn test_pool() -> Arc<sqlx::PgPool> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    fn storage(pool: &Arc<sqlx::PgPool>) -> CasStorage {
        CasStorage::new(pool)
    }

    async fn cleanup_with_suffix(pool: &sqlx::PgPool, suffix: &str) {
        let pattern = format!("%{suffix}%");
        let _ = sqlx::query("DELETE FROM cas_tickets WHERE ticket_id LIKE $1").bind(&pattern).execute(pool).await;
        let _ = sqlx::query("DELETE FROM cas_proxy_tickets WHERE proxy_ticket_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM cas_proxy_granting_tickets WHERE pgt_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM cas_services WHERE service_id LIKE $1").bind(&pattern).execute(pool).await;
        let _ = sqlx::query("DELETE FROM cas_user_attributes WHERE user_id LIKE $1").bind(&pattern).execute(pool).await;
        let _ = sqlx::query("DELETE FROM cas_slo_sessions WHERE session_id LIKE $1").bind(&pattern).execute(pool).await;
    }

    async fn ensure_test_user(pool: &sqlx::PgPool, user_id: &str) {
        let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
        let _ = sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, EXTRACT(EPOCH FROM NOW()) * 1000) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(username)
        .execute(pool)
        .await;
    }

    // ---- CAS ticket tests ----

    #[tokio::test]
    async fn test_create_ticket() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@testuser-{suffix}:example.com");
        let ticket_id = format!("cas_ticket_{suffix}");
        let service_url = format!("https://svc-{suffix}.example.com");

        cleanup_with_suffix(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let cas = storage(&pool);
        let ticket = cas
            .create_ticket(CreateTicketRequest {
                ticket_id: ticket_id.clone(),
                user_id: user_id.clone(),
                service_url: service_url.clone(),
                expires_in_seconds: 3600,
            })
            .await
            .expect("should succeed");

        assert_eq!(ticket.ticket_id, ticket_id);
        assert_eq!(ticket.user_id, user_id);
        assert_eq!(ticket.service_url, service_url);
        assert!(ticket.is_valid);
        assert!(ticket.expires_at > ticket.created_ts);

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_create_duplicate_ticket() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@testuser-{suffix}:example.com");
        let ticket_id = format!("cas_ticket_{suffix}");
        let service_url = format!("https://svc-{suffix}.example.com");

        cleanup_with_suffix(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let cas = storage(&pool);
        let req = CreateTicketRequest {
            ticket_id: ticket_id.clone(),
            user_id: user_id.clone(),
            service_url: service_url.clone(),
            expires_in_seconds: 3600,
        };

        // First creation should succeed.
        cas.create_ticket(req.clone()).await.expect("should succeed");

        // Second creation with same ticket_id should fail (unique constraint).
        let result = cas.create_ticket(req).await;
        assert!(result.is_err(), "duplicate ticket_id should return error");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_validate_ticket_valid() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@testuser-{suffix}:example.com");
        let ticket_id = format!("cas_ticket_{suffix}");
        let service_url = format!("https://svc-{suffix}.example.com");

        cleanup_with_suffix(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let cas = storage(&pool);
        cas.create_ticket(CreateTicketRequest {
            ticket_id: ticket_id.clone(),
            user_id: user_id.clone(),
            service_url: service_url.clone(),
            expires_in_seconds: 3600,
        })
        .await
        .expect("should succeed");

        // Validate the ticket — should succeed and mark consumed.
        let validated =
            cas.validate_ticket(&ticket_id, &service_url).await.expect("should succeed").expect("should return Some");

        assert_eq!(validated.ticket_id, ticket_id);
        assert_eq!(validated.user_id, user_id);
        assert!(!validated.is_valid, "should be marked invalid after consumption");

        // Validating the same ticket again should return None.
        let second = cas.validate_ticket(&ticket_id, &service_url).await.expect("should succeed");
        assert!(second.is_none(), "already-consumed ticket should return None");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_validate_ticket_expired() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@testuser-{suffix}:example.com");
        let ticket_id = format!("cas_ticket_{suffix}");
        let service_url = format!("https://svc-{suffix}.example.com");

        cleanup_with_suffix(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let cas = storage(&pool);
        // expires_in_seconds negative = ticket is already expired.
        cas.create_ticket(CreateTicketRequest {
            ticket_id: ticket_id.clone(),
            user_id: user_id.clone(),
            service_url: service_url.clone(),
            expires_in_seconds: -3600,
        })
        .await
        .expect("should succeed");

        let result = cas.validate_ticket(&ticket_id, &service_url).await.expect("should succeed");
        assert!(result.is_none(), "expired ticket should return None");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_validate_ticket_wrong_url() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@testuser-{suffix}:example.com");
        let ticket_id = format!("cas_ticket_{suffix}");
        let service_url = format!("https://svc-{suffix}.example.com");

        cleanup_with_suffix(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let cas = storage(&pool);
        cas.create_ticket(CreateTicketRequest {
            ticket_id: ticket_id.clone(),
            user_id: user_id.clone(),
            service_url: service_url.clone(),
            expires_in_seconds: 3600,
        })
        .await
        .expect("should succeed");

        // Validate with wrong service_url.
        let result = cas.validate_ticket(&ticket_id, "https://wrong.example.com").await.expect("should succeed");
        assert!(result.is_none(), "wrong service_url should return None");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_ticket() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@testuser-{suffix}:example.com");
        let ticket_id = format!("cas_ticket_{suffix}");
        let service_url = format!("https://svc-{suffix}.example.com");

        cleanup_with_suffix(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let cas = storage(&pool);
        cas.create_ticket(CreateTicketRequest {
            ticket_id: ticket_id.clone(),
            user_id: user_id.clone(),
            service_url: service_url.clone(),
            expires_in_seconds: 3600,
        })
        .await
        .expect("should succeed");

        // Get existing ticket.
        let found = cas.get_ticket(&ticket_id).await.expect("should succeed").expect("should find ticket");
        assert_eq!(found.ticket_id, ticket_id);

        // Get non-existent ticket.
        let not_found = cas.get_ticket("nonexistent_ticket_id").await.expect("should succeed");
        assert!(not_found.is_none());

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_delete_ticket() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@testuser-{suffix}:example.com");
        let ticket_id = format!("cas_ticket_{suffix}");
        let service_url = format!("https://svc-{suffix}.example.com");

        cleanup_with_suffix(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let cas = storage(&pool);
        cas.create_ticket(CreateTicketRequest {
            ticket_id: ticket_id.clone(),
            user_id: user_id.clone(),
            service_url: service_url.clone(),
            expires_in_seconds: 3600,
        })
        .await
        .expect("should succeed");

        // Delete existing ticket.
        let deleted = cas.delete_ticket(&ticket_id).await.expect("should succeed");
        assert!(deleted, "should return true when ticket is deleted");

        // Verify it's gone.
        let after = cas.get_ticket(&ticket_id).await.expect("should succeed");
        assert!(after.is_none(), "ticket should be gone after delete");

        // Delete already-deleted ticket — should return false.
        let again = cas.delete_ticket(&ticket_id).await.expect("should succeed");
        assert!(!again, "should return false for already-deleted ticket");

        // Delete non-existent ticket.
        let never_existed = cas.delete_ticket("nonexistent_ticket_id").await.expect("should succeed");
        assert!(!never_existed);

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_cleanup_expired_tickets() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@testuser-{suffix}:example.com");
        let expired_id = format!("expired_ticket_{suffix}");
        let valid_id = format!("valid_ticket_{suffix}");
        let service_url = format!("https://svc-{suffix}.example.com");

        cleanup_with_suffix(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let cas = storage(&pool);

        // Create an already-expired ticket.
        cas.create_ticket(CreateTicketRequest {
            ticket_id: expired_id.clone(),
            user_id: user_id.clone(),
            service_url: service_url.clone(),
            expires_in_seconds: -3600,
        })
        .await
        .expect("should succeed");

        // Create a valid (not-expired) ticket.
        cas.create_ticket(CreateTicketRequest {
            ticket_id: valid_id.clone(),
            user_id: user_id.clone(),
            service_url: service_url.clone(),
            expires_in_seconds: 3600,
        })
        .await
        .expect("should succeed");

        // Cleanup should remove the expired one.
        let removed = cas.cleanup_expired_tickets().await.expect("should succeed");
        assert!(removed >= 1, "should remove at least 1 expired ticket");

        // Expired ticket should be gone.
        let expired_after = cas.get_ticket(&expired_id).await.expect("should succeed");
        assert!(expired_after.is_none(), "expired ticket should be removed");

        // Valid ticket should still exist.
        let valid_after =
            cas.get_ticket(&valid_id).await.expect("should succeed").expect("valid ticket should still exist");
        assert_eq!(valid_after.ticket_id, valid_id);

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- CAS proxy ticket tests ----

    #[tokio::test]
    async fn test_create_proxy_ticket() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@testuser-{suffix}:example.com");
        let proxy_ticket_id = format!("proxy_ticket_{suffix}");
        let service_url = format!("https://svc-{suffix}.example.com");

        cleanup_with_suffix(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let cas = storage(&pool);
        let ticket = cas
            .create_proxy_ticket(CreateProxyTicketRequest {
                proxy_ticket_id: proxy_ticket_id.clone(),
                user_id: user_id.clone(),
                service_url: service_url.clone(),
                pgt_url: Some(format!("https://pgt-{suffix}.example.com")),
                expires_in_seconds: 3600,
            })
            .await
            .expect("should succeed");

        assert_eq!(ticket.proxy_ticket_id, proxy_ticket_id);
        assert_eq!(ticket.user_id, user_id);
        assert_eq!(ticket.service_url, service_url);
        assert!(ticket.is_valid);
        assert!(ticket.expires_at > ticket.created_ts);

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_validate_proxy_ticket() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@testuser-{suffix}:example.com");
        let valid_pt_id = format!("valid_pt_{suffix}");
        let expired_pt_id = format!("expired_pt_{suffix}");
        let service_url = format!("https://svc-{suffix}.example.com");

        cleanup_with_suffix(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let cas = storage(&pool);

        // Create a valid proxy ticket.
        cas.create_proxy_ticket(CreateProxyTicketRequest {
            proxy_ticket_id: valid_pt_id.clone(),
            user_id: user_id.clone(),
            service_url: service_url.clone(),
            pgt_url: None,
            expires_in_seconds: 3600,
        })
        .await
        .expect("should succeed");

        // Create an already-expired proxy ticket.
        cas.create_proxy_ticket(CreateProxyTicketRequest {
            proxy_ticket_id: expired_pt_id.clone(),
            user_id: user_id.clone(),
            service_url: service_url.clone(),
            pgt_url: None,
            expires_in_seconds: -3600,
        })
        .await
        .expect("should succeed");

        // Validate valid ticket.
        let validated = cas
            .validate_proxy_ticket(&valid_pt_id, &service_url)
            .await
            .expect("should succeed")
            .expect("should return Some");
        assert_eq!(validated.proxy_ticket_id, valid_pt_id);
        assert!(!validated.is_valid, "should be consumed after validation");

        // Validate expired ticket.
        let expired = cas.validate_proxy_ticket(&expired_pt_id, &service_url).await.expect("should succeed");
        assert!(expired.is_none(), "expired proxy ticket should return None");

        // Validate with wrong service_url.
        let wrong_url =
            cas.validate_proxy_ticket(&valid_pt_id, "https://wrong.example.com").await.expect("should succeed");
        assert!(wrong_url.is_none(), "wrong service_url should return None");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- CAS PGT tests ----

    #[tokio::test]
    async fn test_pgt_create_and_retrieve() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@testuser-{suffix}:example.com");
        let pgt_id = format!("pgt_{suffix}");
        let service_url = format!("https://svc-{suffix}.example.com");

        cleanup_with_suffix(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let cas = storage(&pool);
        let pgt = cas
            .create_pgt(CreatePgtRequest {
                pgt_id: pgt_id.clone(),
                user_id: user_id.clone(),
                service_url: service_url.clone(),
                iou: Some(format!("iou_{suffix}")),
                expires_in_seconds: 3600,
            })
            .await
            .expect("should succeed");

        assert_eq!(pgt.pgt_id, pgt_id);
        assert_eq!(pgt.user_id, user_id);
        assert!(pgt.is_valid);

        // Retrieve by pgt_id.
        let found = cas.get_pgt(&pgt_id).await.expect("should succeed").expect("should find PGT");
        assert_eq!(found.pgt_id, pgt_id);

        // Non-existent PGT should return None.
        let not_found = cas.get_pgt("nonexistent_pgt_id").await.expect("should succeed");
        assert!(not_found.is_none());

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_pgt_by_iou() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@testuser-{suffix}:example.com");
        let pgt_id = format!("pgt_{suffix}");
        let iou = format!("iou_{suffix}");
        let service_url = format!("https://svc-{suffix}.example.com");

        cleanup_with_suffix(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let cas = storage(&pool);
        cas.create_pgt(CreatePgtRequest {
            pgt_id: pgt_id.clone(),
            user_id: user_id.clone(),
            service_url: service_url.clone(),
            iou: Some(iou.clone()),
            expires_in_seconds: 3600,
        })
        .await
        .expect("should succeed");

        // Retrieve by IOU.
        let found = cas.get_pgt_by_iou(&iou).await.expect("should succeed").expect("should find PGT by IOU");
        assert_eq!(found.pgt_id, pgt_id);
        assert_eq!(found.iou.unwrap(), iou);

        // Non-existent IOU should return None.
        let not_found = cas.get_pgt_by_iou("nonexistent_iou").await.expect("should succeed");
        assert!(not_found.is_none());

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- CAS registered service tests ----

    #[tokio::test]
    async fn test_register_service() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let service_id = format!("svc_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        let cas = storage(&pool);
        let service = cas
            .register_service(RegisterServiceRequest {
                service_id: service_id.clone(),
                name: format!("Test Service {suffix}"),
                description: Some("A test service".to_string()),
                service_url_pattern: format!(".*svc-{suffix}.*"),
                allowed_attributes: Some(vec!["email".to_string(), "name".to_string()]),
                allowed_proxy_callbacks: Some(vec!["https://cb-{suffix}.example.com".to_string()]),
                is_require_secure: Some(false),
                is_single_logout: Some(true),
            })
            .await
            .expect("should succeed");

        assert_eq!(service.service_id, service_id);
        assert!(service.name.contains(&suffix));
        assert!(service.is_single_logout);
        assert!(!service.is_require_secure);
        assert!(service.is_enabled);

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_service() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let service_id = format!("svc_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        let cas = storage(&pool);
        cas.register_service(RegisterServiceRequest {
            service_id: service_id.clone(),
            name: format!("Test Service {suffix}"),
            description: None,
            service_url_pattern: format!(".*svc-{suffix}.*"),
            allowed_attributes: None,
            allowed_proxy_callbacks: None,
            is_require_secure: None,
            is_single_logout: None,
        })
        .await
        .expect("should succeed");

        // Get by service_id.
        let found = cas.get_service(&service_id).await.expect("should succeed").expect("should find service");
        assert_eq!(found.service_id, service_id);

        // Non-existent service.
        let not_found = cas.get_service("nonexistent_svc_id").await.expect("should succeed");
        assert!(not_found.is_none());

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_service_by_url() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let service_id = format!("svc_{suffix}");
        let service_url = format!("https://svc-{suffix}.example.com");

        cleanup_with_suffix(&pool, &suffix).await;

        let cas = storage(&pool);
        // Register with a regex pattern that matches the test URL.
        cas.register_service(RegisterServiceRequest {
            service_id: service_id.clone(),
            name: format!("Test Service {suffix}"),
            description: None,
            service_url_pattern: format!(".*svc-{suffix}.*"),
            allowed_attributes: None,
            allowed_proxy_callbacks: None,
            is_require_secure: None,
            is_single_logout: None,
        })
        .await
        .expect("should succeed");

        // Find by matching URL.
        let found = cas
            .get_service_by_url(&service_url)
            .await
            .expect("should succeed")
            .expect("should find service by URL pattern");
        assert_eq!(found.service_id, service_id);

        // Non-matching URL.
        let not_found = cas.get_service_by_url("https://unrelated.example.com").await.expect("should succeed");
        assert!(not_found.is_none());

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_list_services() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let service_id = format!("svc_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        let cas = storage(&pool);
        cas.register_service(RegisterServiceRequest {
            service_id: service_id.clone(),
            name: format!("Test Service {suffix}"),
            description: None,
            service_url_pattern: format!(".*svc-{suffix}.*"),
            allowed_attributes: None,
            allowed_proxy_callbacks: None,
            is_require_secure: None,
            is_single_logout: None,
        })
        .await
        .expect("should succeed");

        let services = cas.list_services().await.expect("should succeed");
        // The list contains all services (including pre-existing); verify ours is present.
        assert!(services.iter().any(|s| s.service_id == service_id), "registered service should appear in list");

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_delete_service() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let service_id = format!("svc_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        let cas = storage(&pool);
        cas.register_service(RegisterServiceRequest {
            service_id: service_id.clone(),
            name: format!("Test Service {suffix}"),
            description: None,
            service_url_pattern: format!(".*svc-{suffix}.*"),
            allowed_attributes: None,
            allowed_proxy_callbacks: None,
            is_require_secure: None,
            is_single_logout: None,
        })
        .await
        .expect("should succeed");

        // Delete it.
        let deleted = cas.delete_service(&service_id).await.expect("should succeed");
        assert!(deleted, "should return true when service is deleted");

        // Verify it's gone.
        let after = cas.get_service(&service_id).await.expect("should succeed");
        assert!(after.is_none(), "service should be gone after delete");

        // Delete again — should return false.
        let again = cas.delete_service(&service_id).await.expect("should succeed");
        assert!(!again, "should return false for already-deleted service");

        // Delete non-existent.
        let never = cas.delete_service("nonexistent_svc").await.expect("should succeed");
        assert!(!never);

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- CAS user attributes tests ----

    #[tokio::test]
    async fn test_user_attributes() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@testuser-{suffix}:example.com");

        cleanup_with_suffix(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let cas = storage(&pool);

        // Set an attribute.
        let attr = cas.set_user_attribute(&user_id, "email", "test@example.com").await.expect("should succeed");
        assert_eq!(attr.user_id, user_id);
        assert_eq!(attr.attribute_name, "email");
        assert_eq!(attr.attribute_value, "test@example.com");

        // Set another attribute.
        cas.set_user_attribute(&user_id, "display_name", "Test User").await.expect("should succeed");

        // Get all attributes.
        let attrs = cas.get_user_attributes(&user_id).await.expect("should succeed");
        assert_eq!(attrs.len(), 2);
        assert!(attrs.iter().any(|a| a.attribute_name == "email"));
        assert!(attrs.iter().any(|a| a.attribute_name == "display_name"));

        // Upsert existing attribute (update value).
        let updated = cas.set_user_attribute(&user_id, "email", "updated@example.com").await.expect("should succeed");
        assert_eq!(updated.attribute_value, "updated@example.com");

        // Verify update persisted.
        let attrs_after = cas.get_user_attributes(&user_id).await.expect("should succeed");
        let email_attr =
            attrs_after.iter().find(|a| a.attribute_name == "email").expect("email attribute should exist");
        assert_eq!(email_attr.attribute_value, "updated@example.com");

        // Still only 2 attributes after upsert (not a new row).
        assert_eq!(attrs_after.len(), 2);

        // Get attributes for user with no attributes.
        let empty = cas.get_user_attributes("@noattr:example.com").await.expect("should succeed");
        assert!(empty.is_empty());

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- CAS SLO session tests ----

    #[tokio::test]
    async fn test_slo_session_create_and_mark() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@testuser-{suffix}:example.com");
        let session_id = format!("slo_session_{suffix}");
        let service_url = format!("https://svc-{suffix}.example.com");

        cleanup_with_suffix(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let cas = storage(&pool);

        // Create SLO session.
        let session = cas
            .create_slo_session(&session_id, &user_id, &service_url, Some("ticket_ref"))
            .await
            .expect("should succeed");
        assert_eq!(session.session_id, session_id);
        assert_eq!(session.user_id, user_id);
        assert_eq!(session.service_url, service_url);
        assert!(session.logout_sent_ts.is_none());

        // Mark as sent.
        let marked = cas.mark_slo_sent(&session_id).await.expect("should succeed");
        assert!(marked, "should return true for first mark_slo_sent");

        // Marking again should return false (logout_sent_at is already set).
        let again = cas.mark_slo_sent(&session_id).await.expect("should succeed");
        assert!(!again, "second mark_slo_sent should return false");

        // Mark non-existent session.
        let never = cas.mark_slo_sent("nonexistent_session").await.expect("should succeed");
        assert!(!never);

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_active_slo_sessions() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@testuser-{suffix}:example.com");
        let session_id_1 = format!("active_slo_{suffix}_1");
        let session_id_2 = format!("active_slo_{suffix}_2");
        let service_url = format!("https://svc-{suffix}.example.com");

        cleanup_with_suffix(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let cas = storage(&pool);

        // Create two SLO sessions.
        cas.create_slo_session(&session_id_1, &user_id, &service_url, None).await.expect("should succeed");
        cas.create_slo_session(&session_id_2, &user_id, &service_url, None).await.expect("should succeed");

        // Both should appear as active.
        let active = cas.get_active_slo_sessions(&user_id).await.expect("should succeed");
        assert_eq!(active.len(), 2);
        assert!(active.iter().any(|s| s.session_id == session_id_1));
        assert!(active.iter().any(|s| s.session_id == session_id_2));

        // Mark one as sent — it should no longer be active.
        cas.mark_slo_sent(&session_id_1).await.expect("should succeed");

        let active_after = cas.get_active_slo_sessions(&user_id).await.expect("should succeed");
        assert_eq!(active_after.len(), 1);
        assert_eq!(active_after[0].session_id, session_id_2);

        // User with no sessions.
        let empty = cas.get_active_slo_sessions("@nosessions:example.com").await.expect("should succeed");
        assert!(empty.is_empty());

        cleanup_with_suffix(&pool, &suffix).await;
    }

    // ---- End-to-end lifecycle tests ----

    #[tokio::test]
    async fn test_full_ticket_lifecycle() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@testuser-{suffix}:example.com");
        let ticket_id = format!("lifecycle_ticket_{suffix}");
        let service_url = format!("https://svc-{suffix}.example.com");

        cleanup_with_suffix(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let cas = storage(&pool);

        // Create.
        let created = cas
            .create_ticket(CreateTicketRequest {
                ticket_id: ticket_id.clone(),
                user_id: user_id.clone(),
                service_url: service_url.clone(),
                expires_in_seconds: 3600,
            })
            .await
            .expect("should succeed");
        assert!(created.is_valid);

        // Get — verify it exists.
        cas.get_ticket(&ticket_id).await.expect("should succeed").expect("should exist");

        // Validate — consume it.
        let validated =
            cas.validate_ticket(&ticket_id, &service_url).await.expect("should succeed").expect("should validate");
        assert!(!validated.is_valid);

        // Get — still exists in DB but is_valid = false.
        let after_validate =
            cas.get_ticket(&ticket_id).await.expect("should succeed").expect("should still exist in DB");
        assert!(!after_validate.is_valid);

        // Delete — remove it.
        let deleted = cas.delete_ticket(&ticket_id).await.expect("should succeed");
        assert!(deleted);

        // Get — should be gone.
        let after_delete = cas.get_ticket(&ticket_id).await.expect("should succeed");
        assert!(after_delete.is_none());

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_full_service_lifecycle() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let service_id = format!("lifecycle_svc_{suffix}");

        cleanup_with_suffix(&pool, &suffix).await;

        let cas = storage(&pool);

        // Register.
        let registered = cas
            .register_service(RegisterServiceRequest {
                service_id: service_id.clone(),
                name: format!("Lifecycle Test {suffix}"),
                description: Some("Lifecycle test service".to_string()),
                service_url_pattern: format!(".*lifecycle-{suffix}.*"),
                allowed_attributes: Some(vec!["email".to_string()]),
                allowed_proxy_callbacks: None,
                is_require_secure: Some(false),
                is_single_logout: Some(false),
            })
            .await
            .expect("should succeed");
        assert_eq!(registered.service_id, service_id);

        // Get by id.
        cas.get_service(&service_id).await.expect("should succeed").expect("should find by id");

        // Get by URL.
        let service_url = format!("https://lifecycle-{suffix}.example.com");
        cas.get_service_by_url(&service_url).await.expect("should succeed").expect("should find by URL");

        // List — should include this service.
        let list = cas.list_services().await.expect("should succeed");
        assert!(list.iter().any(|s| s.service_id == service_id));

        // Delete.
        let deleted = cas.delete_service(&service_id).await.expect("should succeed");
        assert!(deleted);

        // Verify gone.
        assert!(cas.get_service(&service_id).await.expect("should succeed").is_none());

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_slo_session_lifecycle() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@testuser-{suffix}:example.com");
        let session_id = format!("slo_lifecycle_{suffix}");
        let service_url = format!("https://svc-{suffix}.example.com");

        cleanup_with_suffix(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let cas = storage(&pool);

        // Create without optional ticket_id.
        let created = cas.create_slo_session(&session_id, &user_id, &service_url, None).await.expect("should succeed");
        assert_eq!(created.session_id, session_id);
        assert!(created.logout_sent_ts.is_none());
        assert!(created.ticket_id.is_none());

        // Should appear in active sessions.
        let active = cas.get_active_slo_sessions(&user_id).await.expect("should succeed");
        assert!(active.iter().any(|s| s.session_id == session_id));

        // Mark as sent.
        let marked = cas.mark_slo_sent(&session_id).await.expect("should succeed");
        assert!(marked);

        // Should no longer be active.
        let active_after = cas.get_active_slo_sessions(&user_id).await.expect("should succeed");
        assert!(!active_after.iter().any(|s| s.session_id == session_id));

        cleanup_with_suffix(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_pgt_full_lifecycle() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@testuser-{suffix}:example.com");
        let pgt_id = format!("pgt_lifecycle_{suffix}");
        let iou = format!("iou_lifecycle_{suffix}");
        let service_url = format!("https://svc-{suffix}.example.com");

        cleanup_with_suffix(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let cas = storage(&pool);

        // Create PGT with IOU.
        let created = cas
            .create_pgt(CreatePgtRequest {
                pgt_id: pgt_id.clone(),
                user_id: user_id.clone(),
                service_url: service_url.clone(),
                iou: Some(iou.clone()),
                expires_in_seconds: 3600,
            })
            .await
            .expect("should succeed");
        assert_eq!(created.pgt_id, pgt_id);
        assert_eq!(created.iou.as_deref(), Some(iou.as_str()));
        assert!(created.is_valid);

        // Retrieve by pgt_id.
        let by_id = cas.get_pgt(&pgt_id).await.expect("should succeed").expect("should find by pgt_id");
        assert_eq!(by_id.pgt_id, pgt_id);

        // Retrieve by IOU.
        let by_iou = cas.get_pgt_by_iou(&iou).await.expect("should succeed").expect("should find by IOU");
        assert_eq!(by_iou.pgt_id, pgt_id);

        // Create a proxy ticket using the PGT (common CAS flow).
        let proxy_ticket_id = format!("pt_from_pgt_{suffix}");
        cas.create_proxy_ticket(CreateProxyTicketRequest {
            proxy_ticket_id: proxy_ticket_id.clone(),
            user_id: user_id.clone(),
            service_url: service_url.clone(),
            pgt_url: Some(pgt_id.clone()),
            expires_in_seconds: 3600,
        })
        .await
        .expect("should succeed");

        // Validate the proxy ticket.
        let validated = cas
            .validate_proxy_ticket(&proxy_ticket_id, &service_url)
            .await
            .expect("should succeed")
            .expect("should validate proxy ticket");
        assert_eq!(validated.user_id, user_id);

        cleanup_with_suffix(&pool, &suffix).await;
    }
}
