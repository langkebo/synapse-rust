use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// The `CasTicket` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CasTicket {
    /// The `id` field.
    pub id: i64,
    /// The `ticket_id` field.
    pub ticket_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `service_url` field.
    pub service_url: String,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `expires_at` field.
    pub expires_at: i64,
    /// The `consumed_ts` field.
    pub consumed_ts: Option<i64>,
    /// The `consumed_by` field.
    pub consumed_by: Option<String>,
    /// The `is_valid` field.
    pub is_valid: bool,
}

/// The `CasProxyTicket` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CasProxyTicket {
    /// The `id` field.
    pub id: i64,
    /// The `proxy_ticket_id` field.
    pub proxy_ticket_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `service_url` field.
    pub service_url: String,
    /// The `pgt_url` field.
    pub pgt_url: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `expires_at` field.
    pub expires_at: i64,
    /// The `consumed_ts` field.
    pub consumed_ts: Option<i64>,
    /// The `is_valid` field.
    pub is_valid: bool,
}

/// The `CasProxyGrantingTicket` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CasProxyGrantingTicket {
    /// The `id` field.
    pub id: i64,
    /// The `pgt_id` field.
    pub pgt_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `service_url` field.
    pub service_url: String,
    /// The `iou` field.
    pub iou: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `expires_at` field.
    pub expires_at: i64,
    /// The `is_valid` field.
    pub is_valid: bool,
}

/// The `CasRegisteredService` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CasRegisteredService {
    /// The `id` field.
    pub id: i64,
    /// The `service_id` field.
    pub service_id: String,
    /// The `name` field.
    pub name: String,
    /// The `description` field.
    pub description: Option<String>,
    /// The `service_url_pattern` field.
    pub service_url_pattern: String,
    /// The `allowed_attributes` field.
    pub allowed_attributes: serde_json::Value,
    /// The `allowed_proxy_callbacks` field.
    pub allowed_proxy_callbacks: serde_json::Value,
    /// The `is_enabled` field.
    pub is_enabled: bool,
    /// The `is_require_secure` field.
    pub is_require_secure: bool,
    /// The `is_single_logout` field.
    pub is_single_logout: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// The `CasSloSession` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CasSloSession {
    /// The `id` field.
    pub id: i64,
    /// The `session_id` field.
    pub session_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `service_url` field.
    pub service_url: String,
    /// The `ticket_id` field.
    pub ticket_id: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `logout_sent_ts` field.
    pub logout_sent_ts: Option<i64>,
}

/// The `CasUserAttribute` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CasUserAttribute {
    /// The `id` field.
    pub id: i64,
    /// The `user_id` field.
    pub user_id: String,
    /// The `attribute_name` field.
    pub attribute_name: String,
    /// The `attribute_value` field.
    pub attribute_value: String,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// The `CreateTicketRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTicketRequest {
    /// The `ticket_id` field.
    pub ticket_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `service_url` field.
    pub service_url: String,
    /// The `expires_in_seconds` field.
    pub expires_in_seconds: i64,
}

/// The `ValidateTicketRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateTicketRequest {
    /// The `ticket_id` field.
    pub ticket_id: String,
    /// The `service_url` field.
    pub service_url: String,
}

/// The `CreateProxyTicketRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProxyTicketRequest {
    /// The `proxy_ticket_id` field.
    pub proxy_ticket_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `service_url` field.
    pub service_url: String,
    /// The `pgt_url` field.
    pub pgt_url: Option<String>,
    /// The `expires_in_seconds` field.
    pub expires_in_seconds: i64,
}

/// The `CreatePgtRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePgtRequest {
    /// The `pgt_id` field.
    pub pgt_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `service_url` field.
    pub service_url: String,
    /// The `iou` field.
    pub iou: Option<String>,
    /// The `expires_in_seconds` field.
    pub expires_in_seconds: i64,
}

/// The `RegisterServiceRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterServiceRequest {
    /// The `service_id` field.
    pub service_id: String,
    /// The `name` field.
    pub name: String,
    /// The `description` field.
    pub description: Option<String>,
    /// The `service_url_pattern` field.
    pub service_url_pattern: String,
    /// The `allowed_attributes` field.
    pub allowed_attributes: Option<Vec<String>>,
    /// The `allowed_proxy_callbacks` field.
    pub allowed_proxy_callbacks: Option<Vec<String>>,
    /// The `is_require_secure` field.
    pub is_require_secure: Option<bool>,
    /// The `is_single_logout` field.
    pub is_single_logout: Option<bool>,
}

// ---- Row wrappers --------------------------------------------------------
//
// v8 schema uses `consumed_at` / `logout_sent_at` / `updated_ts` (nullable)
// but the public models keep `_ts` suffixes and `updated_ts: i64` (non-null).
// These row types bridge the two for sqlx::query_as without changing the
// public API. The drift itself is tracked in `M3-ISSUE-3`.

/// The `CasTicketRow` struct.
#[derive(Debug, Clone, FromRow)]
pub(super) struct CasTicketRow {
    /// The `id` field.
    pub id: i64,
    /// The `ticket_id` field.
    pub ticket_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `service_url` field.
    pub service_url: String,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `expires_at` field.
    pub expires_at: i64,
    /// The `consumed_at` field.
    pub consumed_at: Option<i64>,
    /// The `consumed_by` field.
    pub consumed_by: Option<String>,
    /// The `is_valid` field.
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

/// The `CasProxyTicketRow` struct.
#[derive(Debug, Clone, FromRow)]
pub(super) struct CasProxyTicketRow {
    /// The `id` field.
    pub id: i64,
    /// The `proxy_ticket_id` field.
    pub proxy_ticket_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `service_url` field.
    pub service_url: String,
    /// The `pgt_url` field.
    pub pgt_url: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `expires_at` field.
    pub expires_at: i64,
    /// The `consumed_at` field.
    pub consumed_at: Option<i64>,
    /// The `is_valid` field.
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

/// The `CasRegisteredServiceRow` struct.
#[derive(Debug, Clone, FromRow)]
pub(super) struct CasRegisteredServiceRow {
    /// The `id` field.
    pub id: i64,
    /// The `service_id` field.
    pub service_id: String,
    /// The `name` field.
    pub name: String,
    /// The `description` field.
    pub description: Option<String>,
    /// The `service_url_pattern` field.
    pub service_url_pattern: String,
    /// The `allowed_attributes` field.
    pub allowed_attributes: serde_json::Value,
    /// The `allowed_proxy_callbacks` field.
    pub allowed_proxy_callbacks: serde_json::Value,
    /// The `is_enabled` field.
    pub is_enabled: bool,
    /// The `is_require_secure` field.
    pub is_require_secure: bool,
    /// The `is_single_logout` field.
    pub is_single_logout: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
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

/// The `CasSloSessionRow` struct.
#[derive(Debug, Clone, FromRow)]
pub(super) struct CasSloSessionRow {
    /// The `id` field.
    pub id: i64,
    /// The `session_id` field.
    pub session_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `service_url` field.
    pub service_url: String,
    /// The `ticket_id` field.
    pub ticket_id: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `logout_sent_at` field.
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

/// The `CasUserAttributeRow` struct.
#[derive(Debug, Clone, FromRow)]
pub(super) struct CasUserAttributeRow {
    /// The `id` field.
    pub id: i64,
    /// The `user_id` field.
    pub user_id: String,
    /// The `attribute_name` field.
    pub attribute_name: String,
    /// The `attribute_value` field.
    pub attribute_value: String,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
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
