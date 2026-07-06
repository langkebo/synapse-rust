use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::Utc;
use rand::RngCore;
use std::sync::Arc;
use synapse_common::ApiError;
use synapse_storage::cas::{
    CasProxyGrantingTicket, CasProxyTicket, CasSloSession, CasStoreApi, CasTicket, CasUserAttribute, CreatePgtRequest,
    CreateProxyTicketRequest, CreateTicketRequest,
};
pub use synapse_storage::cas::{CasRegisteredService, RegisterServiceRequest};
use tracing::{info, instrument};

pub struct CasService {
    storage: Arc<dyn CasStoreApi>,
    server_name: String,
    ticket_prefix: String,
    ticket_validity_seconds: i64,
}

impl CasService {
    pub fn new(storage: Arc<dyn CasStoreApi>, server_name: String) -> Self {
        Self { storage, server_name, ticket_prefix: "ST".to_string(), ticket_validity_seconds: 300 }
    }

    /// 检查 CAS 服务是否已正确配置和初始化
    pub async fn is_configured(&self) -> bool {
        // 尝试查询一个简单的操作来检查数据库表是否存在
        // 如果表不存在，查询会失败
        match self.storage.list_services().await {
            Ok(_) => true,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    server_name = %self.server_name,
                    ticket_prefix = %self.ticket_prefix,
                    "CAS service configuration check failed; database tables may not exist"
                );
                false
            }
        }
    }

    fn generate_ticket_id(&self, prefix: &str) -> String {
        let mut random_bytes = [0u8; 16];
        rand::rng().fill_bytes(&mut random_bytes);
        let random_str = URL_SAFE_NO_PAD.encode(random_bytes);
        format!("{}-{}-{}", prefix, self.server_name, random_str)
    }

    #[instrument(skip(self))]
    pub async fn create_service_ticket(&self, user_id: &str, service_url: &str) -> Result<CasTicket, ApiError> {
        info!(user_id = %user_id, has_service_url = !service_url.is_empty(), "Creating service ticket");

        let ticket_id = self.generate_ticket_id(&self.ticket_prefix);

        let request = CreateTicketRequest {
            ticket_id,
            user_id: user_id.to_string(),
            service_url: service_url.to_string(),
            expires_in_seconds: self.ticket_validity_seconds,
        };

        self.storage.create_ticket(request).await
    }

    #[instrument(skip(self))]
    pub async fn validate_service_ticket(
        &self,
        ticket_id: &str,
        service_url: &str,
    ) -> Result<Option<CasTicket>, ApiError> {
        info!(ticket_id = %ticket_id, has_service_url = !service_url.is_empty(), "Validating service ticket");

        let ticket = self.storage.validate_ticket(ticket_id, service_url).await?;

        if let Some(ref t) = ticket {
            info!(
                ticket_id = %ticket_id,
                user_id = %t.user_id,
                has_service_url = !service_url.is_empty(),
                "Service ticket validated"
            );
        }

        Ok(ticket)
    }

    #[instrument(skip(self))]
    pub async fn validate_service_ticket_v3(
        &self,
        ticket_id: &str,
        service_url: &str,
        pgt_url: Option<&str>,
        renew: bool,
    ) -> Result<CasValidationResponse, ApiError> {
        info!(
            ticket_id = %ticket_id,
            has_service_url = !service_url.is_empty(),
            pgt_callback_requested = pgt_url.is_some(),
            renew,
            "Validating service ticket with CAS v3"
        );

        let ticket = self.storage.get_ticket(ticket_id).await?;

        match ticket {
            Some(t) if t.is_valid && t.expires_at > Utc::now().timestamp_millis() => {
                if renew && t.consumed_ts.is_some() {
                    return Ok(CasValidationResponse::Failure {
                        code: "INVALID_TICKET".to_string(),
                        description: "Ticket was already used".to_string(),
                    });
                }

                let attributes = self.storage.get_user_attributes(&t.user_id).await?;

                let mut pgt_iou = None;
                if let Some(pgt_url) = pgt_url {
                    let pgt = self.create_proxy_granting_ticket(&t.user_id, service_url, Some(pgt_url)).await?;
                    pgt_iou = pgt.iou;
                }

                self.storage.validate_ticket(ticket_id, service_url).await?;

                Ok(CasValidationResponse::Success {
                    user: t.user_id.clone(),
                    attributes: attributes.into_iter().map(|a| (a.attribute_name, a.attribute_value)).collect(),
                    proxy_granting_ticket: pgt_iou,
                })
            }
            Some(_) => Ok(CasValidationResponse::Failure {
                code: "INVALID_TICKET".to_string(),
                description: "Ticket is expired or invalid".to_string(),
            }),
            None => Ok(CasValidationResponse::Failure {
                code: "INVALID_TICKET".to_string(),
                description: "Ticket not found".to_string(),
            }),
        }
    }

    #[instrument(skip(self))]
    pub async fn create_proxy_granting_ticket(
        &self,
        user_id: &str,
        service_url: &str,
        pgt_url: Option<&str>,
    ) -> Result<CasProxyGrantingTicket, ApiError> {
        info!(
            user_id = %user_id,
            has_service_url = !service_url.is_empty(),
            pgt_callback_requested = pgt_url.is_some(),
            "Creating proxy granting ticket"
        );

        let pgt_id = self.generate_ticket_id("PGT");
        let iou = Some(self.generate_ticket_id("PGTIOU"));

        let request = CreatePgtRequest {
            pgt_id,
            user_id: user_id.to_string(),
            service_url: service_url.to_string(),
            iou: iou.clone(),
            expires_in_seconds: 3600,
        };

        let pgt = self.storage.create_pgt(request).await?;

        if let Some(url) = pgt_url {
            info!(
                has_pgt_callback_url = !url.is_empty(),
                pgt_iou = ?iou,
                "CAS PGT callback prepared"
            );
        }

        Ok(pgt)
    }

    #[instrument(skip(self))]
    pub async fn create_proxy_ticket(&self, pgt_id: &str, target_service: &str) -> Result<CasProxyTicket, ApiError> {
        info!(pgt_id = %pgt_id, has_target_service = !target_service.is_empty(), "Creating proxy ticket");

        let pgt = self
            .storage
            .get_pgt(pgt_id)
            .await?
            .ok_or_else(|| ApiError::bad_request("Invalid proxy granting ticket"))?;

        if pgt.expires_at < Utc::now().timestamp_millis() {
            return Err(ApiError::bad_request("Proxy granting ticket has expired"));
        }

        let proxy_ticket_id = self.generate_ticket_id("PT");

        let request = CreateProxyTicketRequest {
            proxy_ticket_id,
            user_id: pgt.user_id.clone(),
            service_url: target_service.to_string(),
            pgt_url: None,
            expires_in_seconds: self.ticket_validity_seconds,
        };

        self.storage.create_proxy_ticket(request).await
    }

    #[instrument(skip(self))]
    pub async fn validate_proxy_ticket(
        &self,
        proxy_ticket_id: &str,
        service_url: &str,
    ) -> Result<Option<CasProxyTicket>, ApiError> {
        info!(
            proxy_ticket_id = %proxy_ticket_id,
            has_service_url = !service_url.is_empty(),
            "Validating proxy ticket"
        );
        self.storage.validate_proxy_ticket(proxy_ticket_id, service_url).await
    }

    #[instrument(skip(self))]
    pub async fn register_service(&self, request: RegisterServiceRequest) -> Result<CasRegisteredService, ApiError> {
        info!(
            service_id = %request.service_id,
            has_service_url = !request.service_url_pattern.is_empty(),
            "Registering CAS service"
        );
        self.storage.register_service(request).await
    }

    #[instrument(skip(self))]
    pub async fn get_service(&self, service_id: &str) -> Result<Option<CasRegisteredService>, ApiError> {
        self.storage.get_service(service_id).await
    }

    #[instrument(skip(self))]
    pub async fn get_service_by_url(&self, service_url: &str) -> Result<Option<CasRegisteredService>, ApiError> {
        self.storage.get_service_by_url(service_url).await
    }

    #[instrument(skip(self))]
    pub async fn list_services(&self) -> Result<Vec<CasRegisteredService>, ApiError> {
        self.storage.list_services().await
    }

    #[instrument(skip(self))]
    pub async fn delete_service(&self, service_id: &str) -> Result<bool, ApiError> {
        info!(service_id = %service_id, "Deleting CAS service");
        self.storage.delete_service(service_id).await
    }

    #[instrument(skip(self))]
    pub async fn set_user_attribute(
        &self,
        user_id: &str,
        attribute_name: &str,
        attribute_value: &str,
    ) -> Result<CasUserAttribute, ApiError> {
        self.storage.set_user_attribute(user_id, attribute_name, attribute_value).await
    }

    #[instrument(skip(self))]
    pub async fn get_user_attributes(&self, user_id: &str) -> Result<Vec<CasUserAttribute>, ApiError> {
        self.storage.get_user_attributes(user_id).await
    }

    #[instrument(skip(self))]
    pub async fn initiate_single_logout(&self, user_id: &str) -> Result<Vec<CasSloSession>, ApiError> {
        info!(user_id = %user_id, "Initiating CAS single logout");
        let sessions = self.storage.get_active_slo_sessions(user_id).await?;
        Ok(sessions)
    }

    #[instrument(skip(self))]
    pub async fn cleanup_expired_tickets(&self) -> Result<u64, ApiError> {
        info!("Cleaning up expired CAS tickets");
        self.storage.cleanup_expired_tickets().await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CasValidationResponse {
    Success {
        user: String,
        attributes: std::collections::HashMap<String, String>,
        proxy_granting_ticket: Option<String>,
    },
    Failure {
        code: String,
        description: String,
    },
}

impl CasValidationResponse {
    pub fn to_xml(&self) -> String {
        match self {
            CasValidationResponse::Success { user, attributes, proxy_granting_ticket } => {
                let attrs_xml = if attributes.is_empty() {
                    String::new()
                } else {
                    let attrs: String =
                        attributes.iter().map(|(k, v)| format!("<cas:{}>{}</cas:{}>", k, v, k)).collect();
                    format!("<cas:attributes>{}</cas:attributes>", attrs)
                };

                let pgt_xml = match proxy_granting_ticket {
                    Some(pgt) => {
                        format!("<cas:proxyGrantingTicket>{}</cas:proxyGrantingTicket>", pgt)
                    }
                    None => String::new(),
                };

                format!(
                    r#"<cas:serviceResponse xmlns:cas="https://cas.example.org/cas">
    <cas:authenticationSuccess>
        <cas:user>{}</cas:user>
        {}{}
    </cas:authenticationSuccess>
</cas:serviceResponse>"#,
                    user, attrs_xml, pgt_xml
                )
            }
            CasValidationResponse::Failure { code, description } => {
                format!(
                    r#"<cas:serviceResponse xmlns:cas="https://cas.example.org/cas">
    <cas:authenticationFailure code="{}">
        {}
    </cas:authenticationFailure>
</cas:serviceResponse>"#,
                    code, description
                )
            }
        }
    }
}

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_storage::test_mocks::InMemoryCasStore;

    fn test_service() -> CasService {
        CasService::new(Arc::new(InMemoryCasStore::new()), "example.com".to_string())
    }

    // ── to_xml (CasValidationResponse) ─────────────────────────────

    #[test]
    fn xml_success_with_user_only() {
        let resp = CasValidationResponse::Success {
            user: "@alice:example.com".to_string(),
            attributes: std::collections::HashMap::new(),
            proxy_granting_ticket: None,
        };
        let xml = resp.to_xml();
        assert!(xml.contains("<cas:authenticationSuccess>"));
        assert!(xml.contains("<cas:user>@alice:example.com</cas:user>"));
        assert!(!xml.contains("<cas:attributes>"));
        assert!(!xml.contains("<cas:proxyGrantingTicket>"));
    }

    #[test]
    fn xml_success_with_attributes() {
        let mut attrs = std::collections::HashMap::new();
        attrs.insert("email".to_string(), "alice@example.com".to_string());
        let resp = CasValidationResponse::Success {
            user: "@alice:example.com".to_string(),
            attributes: attrs,
            proxy_granting_ticket: None,
        };
        let xml = resp.to_xml();
        assert!(xml.contains("<cas:attributes>"));
        assert!(xml.contains("<cas:email>alice@example.com</cas:email>"));
    }

    #[test]
    fn xml_success_with_pgt() {
        let resp = CasValidationResponse::Success {
            user: "@alice:example.com".to_string(),
            attributes: std::collections::HashMap::new(),
            proxy_granting_ticket: Some("PGTIOU-123".to_string()),
        };
        let xml = resp.to_xml();
        assert!(xml.contains("<cas:proxyGrantingTicket>PGTIOU-123</cas:proxyGrantingTicket>"));
    }

    #[test]
    fn xml_failure_contains_code_and_description() {
        let resp = CasValidationResponse::Failure {
            code: "INVALID_TICKET".to_string(),
            description: "Ticket expired".to_string(),
        };
        let xml = resp.to_xml();
        assert!(xml.contains("<cas:authenticationFailure"));
        assert!(xml.contains("INVALID_TICKET"));
        assert!(xml.contains("Ticket expired"));
    }

    // ── create_service_ticket ───────────────────────────────────────

    #[tokio::test]
    async fn create_service_ticket_returns_valid_ticket() {
        let svc = test_service();
        let ticket = svc.create_service_ticket("@alice:example.com", "https://app.example.com").await.unwrap();
        assert_eq!(ticket.user_id, "@alice:example.com");
        assert_eq!(ticket.service_url, "https://app.example.com");
        assert!(ticket.is_valid);
        assert!(ticket.ticket_id.starts_with("ST-"));
    }

    // ── validate_service_ticket_v3 ──────────────────────────────────

    #[tokio::test]
    async fn validate_v3_success_for_valid_ticket() {
        let svc = test_service();
        let ticket = svc.create_service_ticket("@alice:example.com", "https://app.example.com").await.unwrap();
        let result =
            svc.validate_service_ticket_v3(&ticket.ticket_id, "https://app.example.com", None, false).await.unwrap();
        match result {
            CasValidationResponse::Success { user, .. } => assert_eq!(user, "@alice:example.com"),
            _ => panic!("Expected success"),
        }
    }

    #[tokio::test]
    async fn validate_v3_fails_for_nonexistent_ticket() {
        let svc = test_service();
        let result =
            svc.validate_service_ticket_v3("ST-nonexistent", "https://app.example.com", None, false).await.unwrap();
        match result {
            CasValidationResponse::Failure { code, .. } => assert_eq!(code, "INVALID_TICKET"),
            _ => panic!("Expected failure"),
        }
    }

    // ── create_proxy_ticket ─────────────────────────────────────────

    #[tokio::test]
    async fn create_proxy_ticket_requires_valid_pgt() {
        let svc = test_service();
        let err = svc.create_proxy_ticket("PGT-invalid", "https://target.example.com").await.unwrap_err();
        assert!(err.to_string().contains("Invalid proxy granting ticket"));
    }

    #[tokio::test]
    async fn create_proxy_ticket_succeeds_with_valid_pgt() {
        let svc = test_service();
        // Create a PGT first
        let pgt =
            svc.create_proxy_granting_ticket("@alice:example.com", "https://app.example.com", None).await.unwrap();
        let pt = svc.create_proxy_ticket(&pgt.pgt_id, "https://target.example.com").await.unwrap();
        assert_eq!(pt.user_id, "@alice:example.com");
        assert_eq!(pt.service_url, "https://target.example.com");
        assert!(pt.is_valid);
    }

    // ── is_configured ───────────────────────────────────────────────

    #[tokio::test]
    async fn is_configured_returns_true_when_tables_exist() {
        let svc = test_service();
        // The InMemoryCasStore always succeeds on list_services
        assert!(svc.is_configured().await);
    }
}
