use serde::{Deserialize, Serialize};
use sqlx::FromRow;

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
pub(super) struct CasTicketRow {
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
pub(super) struct CasProxyTicketRow {
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
pub(super) struct CasRegisteredServiceRow {
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
pub(super) struct CasSloSessionRow {
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
pub(super) struct CasUserAttributeRow {
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
