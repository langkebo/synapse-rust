use crate::common::ApiError;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;

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
// These row types bridge the two for sqlx::query_as! without changing the
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

        let ticket = sqlx::query_as!(
            CasTicketRow,
            r#"
            INSERT INTO cas_tickets (ticket_id, user_id, service_url, created_ts, expires_at)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING
                id AS "id!",
                ticket_id AS "ticket_id!",
                user_id AS "user_id!",
                service_url AS "service_url!",
                created_ts AS "created_ts!",
                expires_at AS "expires_at!",
                consumed_at AS "consumed_at?",
                consumed_by AS "consumed_by?",
                is_valid AS "is_valid!"
            "#,
            request.ticket_id,
            request.user_id,
            request.service_url,
            now,
            expires_at,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create CAS ticket", &e))?;

        Ok(ticket.into())
    }

    pub async fn validate_ticket(&self, ticket_id: &str, service_url: &str) -> Result<Option<CasTicket>, ApiError> {
        let now = Utc::now().timestamp_millis();

        let ticket = sqlx::query_as!(
            CasTicketRow,
            r#"
            UPDATE cas_tickets
            SET consumed_at = $1, is_valid = FALSE
            WHERE ticket_id = $2 AND service_url = $3 AND is_valid = TRUE AND expires_at > $1
            RETURNING
                id AS "id!",
                ticket_id AS "ticket_id!",
                user_id AS "user_id!",
                service_url AS "service_url!",
                created_ts AS "created_ts!",
                expires_at AS "expires_at!",
                consumed_at AS "consumed_at?",
                consumed_by AS "consumed_by?",
                is_valid AS "is_valid!"
            "#,
            now,
            ticket_id,
            service_url,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to validate CAS ticket", &e))?;

        Ok(ticket.map(CasTicket::from))
    }

    pub async fn get_ticket(&self, ticket_id: &str) -> Result<Option<CasTicket>, ApiError> {
        let ticket = sqlx::query_as!(
            CasTicketRow,
            r#"
            SELECT
                id AS "id!",
                ticket_id AS "ticket_id!",
                user_id AS "user_id!",
                service_url AS "service_url!",
                created_ts AS "created_ts!",
                expires_at AS "expires_at!",
                consumed_at AS "consumed_at?",
                consumed_by AS "consumed_by?",
                is_valid AS "is_valid!"
            FROM cas_tickets
            WHERE ticket_id = $1
            "#,
            ticket_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get CAS ticket", &e))?;

        Ok(ticket.map(CasTicket::from))
    }

    pub async fn delete_ticket(&self, ticket_id: &str) -> Result<bool, ApiError> {
        let result = sqlx::query!(
            r#"
            DELETE FROM cas_tickets
            WHERE ticket_id = $1
            "#,
            ticket_id,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to delete CAS ticket", &e))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn cleanup_expired_tickets(&self) -> Result<u64, ApiError> {
        let now = Utc::now().timestamp_millis();
        let result = sqlx::query!(
            r#"
            DELETE FROM cas_tickets
            WHERE expires_at < $1
            "#,
            now,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to cleanup expired tickets", &e))?;

        Ok(result.rows_affected())
    }

    pub async fn create_proxy_ticket(&self, request: CreateProxyTicketRequest) -> Result<CasProxyTicket, ApiError> {
        let now = Utc::now().timestamp_millis();
        let expires_at = Utc::now().timestamp_millis() + request.expires_in_seconds * 1000;

        let ticket = sqlx::query_as!(
            CasProxyTicketRow,
            r#"
            INSERT INTO cas_proxy_tickets (proxy_ticket_id, user_id, service_url, pgt_url, created_ts, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING
                id AS "id!",
                proxy_ticket_id AS "proxy_ticket_id!",
                user_id AS "user_id!",
                service_url AS "service_url!",
                pgt_url AS "pgt_url?",
                created_ts AS "created_ts!",
                expires_at AS "expires_at!",
                consumed_at AS "consumed_at?",
                is_valid AS "is_valid!"
            "#,
            request.proxy_ticket_id,
            request.user_id,
            request.service_url,
            request.pgt_url,
            now,
            expires_at,
        )
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

        let ticket = sqlx::query_as!(
            CasProxyTicketRow,
            r#"
            UPDATE cas_proxy_tickets
            SET consumed_at = $1, is_valid = FALSE
            WHERE proxy_ticket_id = $2 AND service_url = $3 AND is_valid = TRUE AND expires_at > $1
            RETURNING
                id AS "id!",
                proxy_ticket_id AS "proxy_ticket_id!",
                user_id AS "user_id!",
                service_url AS "service_url!",
                pgt_url AS "pgt_url?",
                created_ts AS "created_ts!",
                expires_at AS "expires_at!",
                consumed_at AS "consumed_at?",
                is_valid AS "is_valid!"
            "#,
            now,
            proxy_ticket_id,
            service_url,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to validate CAS proxy ticket", &e))?;

        Ok(ticket.map(CasProxyTicket::from))
    }

    pub async fn create_pgt(&self, request: CreatePgtRequest) -> Result<CasProxyGrantingTicket, ApiError> {
        let now = Utc::now().timestamp_millis();
        let expires_at = Utc::now().timestamp_millis() + request.expires_in_seconds * 1000;

        let pgt = sqlx::query_as!(
            CasProxyGrantingTicket,
            r#"
            INSERT INTO cas_proxy_granting_tickets (pgt_id, user_id, service_url, iou, created_ts, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING
                id AS "id!",
                pgt_id AS "pgt_id!",
                user_id AS "user_id!",
                service_url AS "service_url!",
                iou AS "iou?",
                created_ts AS "created_ts!",
                expires_at AS "expires_at!",
                is_valid AS "is_valid!"
            "#,
            request.pgt_id,
            request.user_id,
            request.service_url,
            request.iou,
            now,
            expires_at,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create CAS PGT", &e))?;

        Ok(pgt)
    }

    pub async fn get_pgt(&self, pgt_id: &str) -> Result<Option<CasProxyGrantingTicket>, ApiError> {
        let pgt = sqlx::query_as!(
            CasProxyGrantingTicket,
            r#"
            SELECT
                id AS "id!",
                pgt_id AS "pgt_id!",
                user_id AS "user_id!",
                service_url AS "service_url!",
                iou AS "iou?",
                created_ts AS "created_ts!",
                expires_at AS "expires_at!",
                is_valid AS "is_valid!"
            FROM cas_proxy_granting_tickets
            WHERE pgt_id = $1 AND is_valid = TRUE
            "#,
            pgt_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get CAS PGT", &e))?;

        Ok(pgt)
    }

    pub async fn get_pgt_by_iou(&self, iou: &str) -> Result<Option<CasProxyGrantingTicket>, ApiError> {
        let pgt = sqlx::query_as!(
            CasProxyGrantingTicket,
            r#"
            SELECT
                id AS "id!",
                pgt_id AS "pgt_id!",
                user_id AS "user_id!",
                service_url AS "service_url!",
                iou AS "iou?",
                created_ts AS "created_ts!",
                expires_at AS "expires_at!",
                is_valid AS "is_valid!"
            FROM cas_proxy_granting_tickets
            WHERE iou = $1 AND is_valid = TRUE
            "#,
            iou,
        )
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

        let service = sqlx::query_as!(
            CasRegisteredServiceRow,
            r#"
            INSERT INTO cas_services (
                service_id, name, description, service_url_pattern,
                allowed_attributes, allowed_proxy_callbacks,
                is_require_secure, is_single_logout, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
            RETURNING
                id AS "id!",
                service_id AS "service_id!",
                name AS "name!",
                description AS "description?",
                service_url_pattern AS "service_url_pattern!",
                allowed_attributes AS "allowed_attributes!",
                allowed_proxy_callbacks AS "allowed_proxy_callbacks!",
                COALESCE(is_enabled, false) AS "is_enabled!",
                COALESCE(is_require_secure, true) AS "is_require_secure!",
                COALESCE(is_single_logout, false) AS "is_single_logout!",
                created_ts AS "created_ts!",
                updated_ts AS "updated_ts?"
            "#,
            request.service_id,
            request.name,
            request.description,
            request.service_url_pattern,
            allowed_attributes,
            allowed_proxy_callbacks,
            request.is_require_secure.unwrap_or(true),
            request.is_single_logout.unwrap_or(false),
            now,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to register CAS service", &e))?;

        Ok(service.into())
    }

    pub async fn get_service(&self, service_id: &str) -> Result<Option<CasRegisteredService>, ApiError> {
        let service = sqlx::query_as!(
            CasRegisteredServiceRow,
            r#"
            SELECT
                id AS "id!",
                service_id AS "service_id!",
                name AS "name!",
                description AS "description?",
                service_url_pattern AS "service_url_pattern!",
                allowed_attributes AS "allowed_attributes!",
                allowed_proxy_callbacks AS "allowed_proxy_callbacks!",
                COALESCE(is_enabled, false) AS "is_enabled!",
                COALESCE(is_require_secure, true) AS "is_require_secure!",
                COALESCE(is_single_logout, false) AS "is_single_logout!",
                created_ts AS "created_ts!",
                updated_ts AS "updated_ts?"
            FROM cas_services
            WHERE service_id = $1
            "#,
            service_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get CAS service", &e))?;

        Ok(service.map(CasRegisteredService::from))
    }

    pub async fn get_service_by_url(&self, service_url: &str) -> Result<Option<CasRegisteredService>, ApiError> {
        let service = sqlx::query_as!(
            CasRegisteredServiceRow,
            r#"
            SELECT
                id AS "id!",
                service_id AS "service_id!",
                name AS "name!",
                description AS "description?",
                service_url_pattern AS "service_url_pattern!",
                allowed_attributes AS "allowed_attributes!",
                allowed_proxy_callbacks AS "allowed_proxy_callbacks!",
                COALESCE(is_enabled, false) AS "is_enabled!",
                COALESCE(is_require_secure, true) AS "is_require_secure!",
                COALESCE(is_single_logout, false) AS "is_single_logout!",
                created_ts AS "created_ts!",
                updated_ts AS "updated_ts?"
            FROM cas_services
            WHERE $1 ~ service_url_pattern AND is_enabled = TRUE
            "#,
            service_url,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get CAS service by URL", &e))?;

        Ok(service.map(CasRegisteredService::from))
    }

    pub async fn list_services(&self) -> Result<Vec<CasRegisteredService>, ApiError> {
        let services = sqlx::query_as!(
            CasRegisteredServiceRow,
            r#"
            SELECT
                id AS "id!",
                service_id AS "service_id!",
                name AS "name!",
                description AS "description?",
                service_url_pattern AS "service_url_pattern!",
                allowed_attributes AS "allowed_attributes!",
                allowed_proxy_callbacks AS "allowed_proxy_callbacks!",
                COALESCE(is_enabled, false) AS "is_enabled!",
                COALESCE(is_require_secure, true) AS "is_require_secure!",
                COALESCE(is_single_logout, false) AS "is_single_logout!",
                created_ts AS "created_ts!",
                updated_ts AS "updated_ts?"
            FROM cas_services
            ORDER BY created_ts DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to list CAS services", &e))?;

        Ok(services.into_iter().map(CasRegisteredService::from).collect())
    }

    pub async fn delete_service(&self, service_id: &str) -> Result<bool, ApiError> {
        let result = sqlx::query!(
            r#"
            DELETE FROM cas_services
            WHERE service_id = $1
            "#,
            service_id,
        )
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

        let attr = sqlx::query_as!(
            CasUserAttributeRow,
            r#"
            INSERT INTO cas_user_attributes (user_id, attribute_name, attribute_value, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $4)
            ON CONFLICT (user_id, attribute_name)
            DO UPDATE SET attribute_value = $3, updated_ts = $4
            RETURNING
                id AS "id!",
                user_id AS "user_id!",
                attribute_name AS "attribute_name!",
                attribute_value AS "attribute_value!",
                created_ts AS "created_ts!",
                updated_ts AS "updated_ts?"
            "#,
            user_id,
            attribute_name,
            attribute_value,
            now,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to set CAS user attribute", &e))?;

        Ok(attr.into())
    }

    pub async fn get_user_attributes(&self, user_id: &str) -> Result<Vec<CasUserAttribute>, ApiError> {
        let attrs = sqlx::query_as!(
            CasUserAttributeRow,
            r#"
            SELECT
                id AS "id!",
                user_id AS "user_id!",
                attribute_name AS "attribute_name!",
                attribute_value AS "attribute_value!",
                created_ts AS "created_ts!",
                updated_ts AS "updated_ts?"
            FROM cas_user_attributes
            WHERE user_id = $1
            "#,
            user_id,
        )
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
        let session = sqlx::query_as!(
            CasSloSessionRow,
            r#"
            INSERT INTO cas_slo_sessions (session_id, user_id, service_url, ticket_id, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING
                id AS "id!",
                session_id AS "session_id!",
                user_id AS "user_id!",
                service_url AS "service_url!",
                ticket_id AS "ticket_id?",
                created_ts AS "created_ts!",
                logout_sent_at AS "logout_sent_at?"
            "#,
            session_id,
            user_id,
            service_url,
            ticket_id,
            now,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create CAS SLO session", &e))?;

        Ok(session.into())
    }

    pub async fn mark_slo_sent(&self, session_id: &str) -> Result<bool, ApiError> {
        let now = Utc::now().timestamp_millis();
        let result = sqlx::query!(
            r#"
            UPDATE cas_slo_sessions
            SET logout_sent_at = $1
            WHERE session_id = $2
            "#,
            now,
            session_id,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to mark SLO sent", &e))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn get_active_slo_sessions(&self, user_id: &str) -> Result<Vec<CasSloSession>, ApiError> {
        let sessions = sqlx::query_as!(
            CasSloSessionRow,
            r#"
            SELECT
                id AS "id!",
                session_id AS "session_id!",
                user_id AS "user_id!",
                service_url AS "service_url!",
                ticket_id AS "ticket_id?",
                created_ts AS "created_ts!",
                logout_sent_at AS "logout_sent_at?"
            FROM cas_slo_sessions
            WHERE user_id = $1 AND logout_sent_at IS NULL
            "#,
            user_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get active SLO sessions", &e))?;

        Ok(sessions.into_iter().map(CasSloSession::from).collect())
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
