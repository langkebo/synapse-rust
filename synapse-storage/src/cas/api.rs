use async_trait::async_trait;
use synapse_common::ApiError;

use super::models::*;
use super::repository::CasStorage;

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
