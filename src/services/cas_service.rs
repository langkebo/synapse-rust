use crate::common::ApiError;
use crate::storage::cas::{
    CasStorage, CasTicket, CasProxyTicket, CasProxyGrantingTicket, CasSloSession,
    CasUserAttribute, CreateTicketRequest, CreateProxyTicketRequest, CreatePgtRequest,
    RegisterServiceRequest,
};
pub use crate::storage::cas::CasService as CasServiceModel;
use std::sync::Arc;
use tracing::{info, instrument};
use rand::RngCore;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

pub struct CasService {
    storage: Arc<CasStorage>,
    server_name: String,
    ticket_prefix: String,
    ticket_validity_seconds: i64,
}

impl CasService {
    pub fn new(storage: Arc<CasStorage>, server_name: String) -> Self {
        Self {
            storage,
            server_name,
            ticket_prefix: "ST".to_string(),
            ticket_validity_seconds: 300,
        }
    }

    fn generate_ticket_id(&self, prefix: &str) -> String {
        let mut random_bytes = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut random_bytes);
        let random_str = URL_SAFE_NO_PAD.encode(random_bytes);
        format!("{}-{}-{}", prefix, self.server_name, random_str)
    }

    #[instrument(skip(self))]
    pub async fn create_service_ticket(
        &self,
        user_id: &str,
        service_url: &str,
    ) -> Result<CasTicket, ApiError> {
        info!("Creating service ticket for user: {}", user_id);

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
        info!("Validating service ticket: {}", ticket_id);

        let ticket = self.storage.validate_ticket(ticket_id, service_url).await?;

        if let Some(ref t) = ticket {
            info!("Service ticket validated for user: {}", t.user_id);
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
        info!("V3 validating service ticket: {}", ticket_id);

        let ticket = self.storage.get_ticket(ticket_id).await?;

        match ticket {
            Some(t) if t.is_valid && t.expires_at > chrono::Utc::now() => {
                if renew && t.consumed_at.is_some() {
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
        info!("Creating proxy granting ticket for user: {}", user_id);

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
            info!("PGT callback URL: {}, IOU: {:?}", url, iou);
        }

        Ok(pgt)
    }

    #[instrument(skip(self))]
    pub async fn create_proxy_ticket(
        &self,
        pgt_id: &str,
        target_service: &str,
    ) -> Result<CasProxyTicket, ApiError> {
        info!("Creating proxy ticket for PGT: {}", pgt_id);

        let pgt = self.storage.get_pgt(pgt_id).await?
            .ok_or_else(|| ApiError::bad_request("Invalid proxy granting ticket"))?;

        if pgt.expires_at < chrono::Utc::now() {
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
        info!("Validating proxy ticket: {}", proxy_ticket_id);
        self.storage.validate_proxy_ticket(proxy_ticket_id, service_url).await
    }

    #[instrument(skip(self))]
    pub async fn register_service(
        &self,
        request: RegisterServiceRequest,
    ) -> Result<CasServiceModel, ApiError> {
        info!("Registering CAS service: {}", request.service_id);
        self.storage.register_service(request).await
    }

    #[instrument(skip(self))]
    pub async fn get_service(&self, service_id: &str) -> Result<Option<CasServiceModel>, ApiError> {
        self.storage.get_service(service_id).await
    }

    #[instrument(skip(self))]
    pub async fn get_service_by_url(&self, service_url: &str) -> Result<Option<CasServiceModel>, ApiError> {
        self.storage.get_service_by_url(service_url).await
    }

    #[instrument(skip(self))]
    pub async fn list_services(&self) -> Result<Vec<CasServiceModel>, ApiError> {
        self.storage.list_services().await
    }

    #[instrument(skip(self))]
    pub async fn delete_service(&self, service_id: &str) -> Result<bool, ApiError> {
        info!("Deleting CAS service: {}", service_id);
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
        info!("Initiating single logout for user: {}", user_id);
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
                    let attrs: String = attributes.iter()
                        .map(|(k, v)| format!("<cas:{}>{}</cas:{}>", k, v, k))
                        .collect();
                    format!("<cas:attributes>{}</cas:attributes>", attrs)
                };

                let pgt_xml = match proxy_granting_ticket {
                    Some(pgt) => format!("<cas:proxyGrantingTicket>{}</cas:proxyGrantingTicket>", pgt),
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
