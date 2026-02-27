use crate::common::ApiError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CasTicket {
    pub id: i32,
    pub ticket_id: String,
    pub user_id: String,
    pub service_url: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
    pub consumed_by: Option<String>,
    pub is_valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CasProxyTicket {
    pub id: i32,
    pub proxy_ticket_id: String,
    pub user_id: String,
    pub service_url: String,
    pub pgt_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
    pub is_valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CasProxyGrantingTicket {
    pub id: i32,
    pub pgt_id: String,
    pub user_id: String,
    pub service_url: String,
    pub iou: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub is_valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CasService {
    pub id: i32,
    pub service_id: String,
    pub name: String,
    pub description: Option<String>,
    pub service_url_pattern: String,
    pub allowed_attributes: serde_json::Value,
    pub allowed_proxy_callbacks: serde_json::Value,
    pub is_enabled: bool,
    pub require_secure: bool,
    pub single_logout: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CasSloSession {
    pub id: i32,
    pub session_id: String,
    pub user_id: String,
    pub service_url: String,
    pub ticket_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub logout_sent_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CasUserAttribute {
    pub id: i32,
    pub user_id: String,
    pub attribute_name: String,
    pub attribute_value: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
    pub require_secure: Option<bool>,
    pub single_logout: Option<bool>,
}

#[derive(Clone)]
pub struct CasStorage {
    pool: PgPool,
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
            created_at: chrono::DateTime::from_timestamp(1234567800, 0).unwrap(),
            expires_at: chrono::DateTime::from_timestamp(1234567890, 0).unwrap(),
            consumed_at: None,
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
            service_url: "https://backend.example.com".to_string(),
            pgt_url: Some("https://proxy.example.com".to_string()),
            created_at: chrono::DateTime::from_timestamp(1234567800, 0).unwrap(),
            expires_at: chrono::DateTime::from_timestamp(1234567890, 0).unwrap(),
            consumed_at: None,
            is_valid: true,
        };
        assert_eq!(ticket.proxy_ticket_id, "PT-12345678");
        assert!(ticket.is_valid);
    }

    #[test]
    fn test_cas_proxy_granting_ticket_creation() {
        let ticket = CasProxyGrantingTicket {
            id: 1,
            pgt_id: "PGT-12345678".to_string(),
            user_id: "@alice:example.com".to_string(),
            service_url: "https://proxy.example.com".to_string(),
            iou: Some("iou123".to_string()),
            created_at: chrono::DateTime::from_timestamp(1234567800, 0).unwrap(),
            expires_at: chrono::DateTime::from_timestamp(1234567890, 0).unwrap(),
            is_valid: true,
        };
        assert_eq!(ticket.pgt_id, "PGT-12345678");
    }

    #[test]
    fn test_cas_service_creation() {
        let service = CasService {
            id: 1,
            service_id: "app1".to_string(),
            name: "Example App".to_string(),
            description: Some("Test app".to_string()),
            service_url_pattern: "https://app.example.com/*".to_string(),
            allowed_attributes: serde_json::json!(["email"]),
            allowed_proxy_callbacks: serde_json::json!([]),
            is_enabled: true,
            require_secure: true,
            single_logout: false,
            created_at: chrono::DateTime::from_timestamp(1234567800, 0).unwrap(),
            updated_at: chrono::DateTime::from_timestamp(1234567890, 0).unwrap(),
        };
        assert!(service.is_enabled);
    }

    #[test]
    fn test_cas_user_attribute_creation() {
        let attr = CasUserAttribute {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            attribute_name: "email".to_string(),
            attribute_value: "alice@example.com".to_string(),
            created_at: chrono::DateTime::from_timestamp(1234567800, 0).unwrap(),
            updated_at: chrono::DateTime::from_timestamp(1234567890, 0).unwrap(),
        };
        assert_eq!(attr.attribute_name, "email");
    }

    #[test]
    fn test_create_ticket_request() {
        let request = CreateTicketRequest {
            ticket_id: "ST-12345678".to_string(),
            user_id: "@alice:example.com".to_string(),
            service_url: "https://app.example.com".to_string(),
            expires_in_seconds: 300,
        };
        assert_eq!(request.user_id, "@alice:example.com");
    }

    #[test]
    fn test_validate_ticket_request() {
        let request = ValidateTicketRequest {
            ticket_id: "ST-12345678".to_string(),
            service_url: "https://app.example.com".to_string(),
        };
        assert_eq!(request.ticket_id, "ST-12345678");
    }

    #[test]
    fn test_register_service_request() {
        let request = RegisterServiceRequest {
            service_id: "new-app".to_string(),
            name: "New App".to_string(),
            description: Some("New App".to_string()),
            service_url_pattern: "https://new-app.example.com/*".to_string(),
            allowed_attributes: Some(vec![]),
            allowed_proxy_callbacks: Some(vec![]),
            require_secure: Some(true),
            single_logout: Some(false),
        };
        assert_eq!(request.service_id, "new-app");
    }
}

impl CasStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self {
            pool: (**pool).clone(),
        }
    }

    pub async fn create_ticket(&self, request: CreateTicketRequest) -> Result<CasTicket, ApiError> {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(request.expires_in_seconds);

        let ticket = sqlx::query_as::<_, CasTicket>(
            r#"
            INSERT INTO cas_tickets (ticket_id, user_id, service_url, created_at, expires_at)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
        )
        .bind(&request.ticket_id)
        .bind(&request.user_id)
        .bind(&request.service_url)
        .bind(now)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create CAS ticket: {}", e)))?;

        Ok(ticket)
    }

    pub async fn validate_ticket(
        &self,
        ticket_id: &str,
        service_url: &str,
    ) -> Result<Option<CasTicket>, ApiError> {
        let now = Utc::now();

        let ticket = sqlx::query_as::<_, CasTicket>(
            r#"
            UPDATE cas_tickets
            SET consumed_at = $1, is_valid = FALSE
            WHERE ticket_id = $2 AND service_url = $3 AND is_valid = TRUE AND expires_at > $1
            RETURNING *
            "#,
        )
        .bind(now)
        .bind(ticket_id)
        .bind(service_url)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to validate CAS ticket: {}", e)))?;

        Ok(ticket)
    }

    pub async fn get_ticket(&self, ticket_id: &str) -> Result<Option<CasTicket>, ApiError> {
        let ticket =
            sqlx::query_as::<_, CasTicket>(r#"SELECT * FROM cas_tickets WHERE ticket_id = $1"#)
                .bind(ticket_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get CAS ticket: {}", e)))?;

        Ok(ticket)
    }

    pub async fn delete_ticket(&self, ticket_id: &str) -> Result<bool, ApiError> {
        let result = sqlx::query(r#"DELETE FROM cas_tickets WHERE ticket_id = $1"#)
            .bind(ticket_id)
            .execute(&self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete CAS ticket: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn cleanup_expired_tickets(&self) -> Result<u64, ApiError> {
        let now = Utc::now();
        let result = sqlx::query(r#"DELETE FROM cas_tickets WHERE expires_at < $1"#)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to cleanup expired tickets: {}", e)))?;

        Ok(result.rows_affected())
    }

    pub async fn create_proxy_ticket(
        &self,
        request: CreateProxyTicketRequest,
    ) -> Result<CasProxyTicket, ApiError> {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(request.expires_in_seconds);

        let ticket = sqlx::query_as::<_, CasProxyTicket>(
            r#"
            INSERT INTO cas_proxy_tickets (proxy_ticket_id, user_id, service_url, pgt_url, created_at, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(&request.proxy_ticket_id)
        .bind(&request.user_id)
        .bind(&request.service_url)
        .bind(&request.pgt_url)
        .bind(now)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create CAS proxy ticket: {}", e)))?;

        Ok(ticket)
    }

    pub async fn validate_proxy_ticket(
        &self,
        proxy_ticket_id: &str,
        service_url: &str,
    ) -> Result<Option<CasProxyTicket>, ApiError> {
        let now = Utc::now();

        let ticket = sqlx::query_as::<_, CasProxyTicket>(
            r#"
            UPDATE cas_proxy_tickets
            SET consumed_at = $1, is_valid = FALSE
            WHERE proxy_ticket_id = $2 AND service_url = $3 AND is_valid = TRUE AND expires_at > $1
            RETURNING *
            "#,
        )
        .bind(now)
        .bind(proxy_ticket_id)
        .bind(service_url)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to validate CAS proxy ticket: {}", e)))?;

        Ok(ticket)
    }

    pub async fn create_pgt(
        &self,
        request: CreatePgtRequest,
    ) -> Result<CasProxyGrantingTicket, ApiError> {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(request.expires_in_seconds);

        let pgt = sqlx::query_as::<_, CasProxyGrantingTicket>(
            r#"
            INSERT INTO cas_proxy_granting_tickets (pgt_id, user_id, service_url, iou, created_at, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(&request.pgt_id)
        .bind(&request.user_id)
        .bind(&request.service_url)
        .bind(&request.iou)
        .bind(now)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create CAS PGT: {}", e)))?;

        Ok(pgt)
    }

    pub async fn get_pgt(&self, pgt_id: &str) -> Result<Option<CasProxyGrantingTicket>, ApiError> {
        let pgt = sqlx::query_as::<_, CasProxyGrantingTicket>(
            r#"SELECT * FROM cas_proxy_granting_tickets WHERE pgt_id = $1 AND is_valid = TRUE"#,
        )
        .bind(pgt_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get CAS PGT: {}", e)))?;

        Ok(pgt)
    }

    pub async fn get_pgt_by_iou(
        &self,
        iou: &str,
    ) -> Result<Option<CasProxyGrantingTicket>, ApiError> {
        let pgt = sqlx::query_as::<_, CasProxyGrantingTicket>(
            r#"SELECT * FROM cas_proxy_granting_tickets WHERE iou = $1 AND is_valid = TRUE"#,
        )
        .bind(iou)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get CAS PGT by IOU: {}", e)))?;

        Ok(pgt)
    }

    pub async fn register_service(
        &self,
        request: RegisterServiceRequest,
    ) -> Result<CasService, ApiError> {
        let allowed_attributes =
            serde_json::to_value(request.allowed_attributes.unwrap_or_default())
                .unwrap_or(serde_json::json!([]));
        let allowed_proxy_callbacks =
            serde_json::to_value(request.allowed_proxy_callbacks.unwrap_or_default())
                .unwrap_or(serde_json::json!([]));

        let service = sqlx::query_as::<_, CasService>(
            r#"
            INSERT INTO cas_services (
                service_id, name, description, service_url_pattern,
                allowed_attributes, allowed_proxy_callbacks,
                require_secure, single_logout
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            "#,
        )
        .bind(&request.service_id)
        .bind(&request.name)
        .bind(&request.description)
        .bind(&request.service_url_pattern)
        .bind(&allowed_attributes)
        .bind(&allowed_proxy_callbacks)
        .bind(request.require_secure.unwrap_or(true))
        .bind(request.single_logout.unwrap_or(false))
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to register CAS service: {}", e)))?;

        Ok(service)
    }

    pub async fn get_service(&self, service_id: &str) -> Result<Option<CasService>, ApiError> {
        let service =
            sqlx::query_as::<_, CasService>(r#"SELECT * FROM cas_services WHERE service_id = $1"#)
                .bind(service_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get CAS service: {}", e)))?;

        Ok(service)
    }

    pub async fn get_service_by_url(
        &self,
        service_url: &str,
    ) -> Result<Option<CasService>, ApiError> {
        let service = sqlx::query_as::<_, CasService>(
            r#"SELECT * FROM cas_services WHERE $1 ~ service_url_pattern AND is_enabled = TRUE"#,
        )
        .bind(service_url)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get CAS service by URL: {}", e)))?;

        Ok(service)
    }

    pub async fn list_services(&self) -> Result<Vec<CasService>, ApiError> {
        let services = sqlx::query_as::<_, CasService>(
            r#"SELECT * FROM cas_services ORDER BY created_at DESC"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list CAS services: {}", e)))?;

        Ok(services)
    }

    pub async fn delete_service(&self, service_id: &str) -> Result<bool, ApiError> {
        let result = sqlx::query(r#"DELETE FROM cas_services WHERE service_id = $1"#)
            .bind(service_id)
            .execute(&self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete CAS service: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn set_user_attribute(
        &self,
        user_id: &str,
        attribute_name: &str,
        attribute_value: &str,
    ) -> Result<CasUserAttribute, ApiError> {
        let now = Utc::now();

        let attr = sqlx::query_as::<_, CasUserAttribute>(
            r#"
            INSERT INTO cas_user_attributes (user_id, attribute_name, attribute_value, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $4)
            ON CONFLICT (user_id, attribute_name)
            DO UPDATE SET attribute_value = $3, updated_at = $4
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(attribute_name)
        .bind(attribute_value)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to set CAS user attribute: {}", e)))?;

        Ok(attr)
    }

    pub async fn get_user_attributes(
        &self,
        user_id: &str,
    ) -> Result<Vec<CasUserAttribute>, ApiError> {
        let attrs = sqlx::query_as::<_, CasUserAttribute>(
            r#"SELECT * FROM cas_user_attributes WHERE user_id = $1"#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get CAS user attributes: {}", e)))?;

        Ok(attrs)
    }

    pub async fn create_slo_session(
        &self,
        session_id: &str,
        user_id: &str,
        service_url: &str,
        ticket_id: Option<&str>,
    ) -> Result<CasSloSession, ApiError> {
        let session = sqlx::query_as::<_, CasSloSession>(
            r#"
            INSERT INTO cas_slo_sessions (session_id, user_id, service_url, ticket_id)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#,
        )
        .bind(session_id)
        .bind(user_id)
        .bind(service_url)
        .bind(ticket_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create CAS SLO session: {}", e)))?;

        Ok(session)
    }

    pub async fn mark_slo_sent(&self, session_id: &str) -> Result<bool, ApiError> {
        let now = Utc::now();
        let result =
            sqlx::query(r#"UPDATE cas_slo_sessions SET logout_sent_at = $1 WHERE session_id = $2"#)
                .bind(now)
                .bind(session_id)
                .execute(&self.pool)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to mark SLO sent: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn get_active_slo_sessions(
        &self,
        user_id: &str,
    ) -> Result<Vec<CasSloSession>, ApiError> {
        let sessions = sqlx::query_as::<_, CasSloSession>(
            r#"SELECT * FROM cas_slo_sessions WHERE user_id = $1 AND logout_sent_at IS NULL"#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get active SLO sessions: {}", e)))?;

        Ok(sessions)
    }
}
