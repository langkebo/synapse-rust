#[cfg(feature = "cas-sso")]
use super::*;

#[cfg(feature = "cas-sso")]
#[derive(Clone, Default)]
#[allow(clippy::type_complexity)]
pub struct InMemoryCasStore {
    tickets: Arc<tokio::sync::RwLock<HashMap<String, CasTicket>>>,
    proxy_tickets: Arc<tokio::sync::RwLock<HashMap<String, CasProxyTicket>>>,
    pgts: Arc<tokio::sync::RwLock<HashMap<String, CasProxyGrantingTicket>>>,
    services: Arc<tokio::sync::RwLock<HashMap<String, CasRegisteredService>>>,
    user_attributes: Arc<tokio::sync::RwLock<HashMap<String, HashMap<String, String>>>>,
    slo_sessions: Arc<tokio::sync::RwLock<Vec<CasSloSession>>>,
    next_id: Arc<tokio::sync::RwLock<i64>>,
}

#[cfg(feature = "cas-sso")]
impl InMemoryCasStore {
    pub fn new() -> Self {
        Self {
            tickets: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            proxy_tickets: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            pgts: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            services: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            user_attributes: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            slo_sessions: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            next_id: Arc::new(tokio::sync::RwLock::new(1)),
        }
    }

    async fn next_id(&self) -> i64 {
        let mut id = self.next_id.write().await;
        let current = *id;
        *id += 1;
        current
    }
}

#[cfg(feature = "cas-sso")]
#[async_trait::async_trait]
impl CasStoreApi for InMemoryCasStore {
    async fn create_ticket(&self, request: CreateTicketRequest) -> Result<CasTicket, ApiError> {
        let now = Utc::now().timestamp_millis();
        let ticket = CasTicket {
            id: self.next_id().await,
            ticket_id: request.ticket_id.clone(),
            user_id: request.user_id.clone(),
            service_url: request.service_url.clone(),
            created_ts: now,
            expires_at: now + (request.expires_in_seconds) * 1000,
            consumed_ts: None,
            consumed_by: None,
            is_valid: true,
        };
        self.tickets.write().await.insert(ticket.ticket_id.clone(), ticket.clone());
        Ok(ticket)
    }

    async fn validate_ticket(&self, ticket_id: &str, service_url: &str) -> Result<Option<CasTicket>, ApiError> {
        let mut tickets = self.tickets.write().await;
        if let Some(ticket) = tickets.get_mut(ticket_id) {
            if ticket.is_valid {
                let now = Utc::now().timestamp_millis();
                ticket.consumed_ts = Some(now);
                ticket.consumed_by = Some(service_url.to_string());
                return Ok(Some(ticket.clone()));
            }
        }
        Ok(None)
    }

    async fn get_ticket(&self, ticket_id: &str) -> Result<Option<CasTicket>, ApiError> {
        Ok(self.tickets.read().await.get(ticket_id).cloned())
    }

    async fn get_user_attributes(&self, user_id: &str) -> Result<Vec<CasUserAttribute>, ApiError> {
        let attrs = self.user_attributes.read().await;
        let user_attrs = attrs.get(user_id);
        Ok(user_attrs.map_or(Vec::new(), |map| {
            map.iter()
                .map(|(name, value)| CasUserAttribute {
                    id: 0,
                    user_id: user_id.to_string(),
                    attribute_name: name.clone(),
                    attribute_value: value.clone(),
                    created_ts: 0,
                    updated_ts: 0,
                })
                .collect()
        }))
    }

    async fn create_pgt(&self, request: CreatePgtRequest) -> Result<CasProxyGrantingTicket, ApiError> {
        let now = Utc::now().timestamp_millis();
        let pgt = CasProxyGrantingTicket {
            id: self.next_id().await,
            pgt_id: request.pgt_id.clone(),
            user_id: request.user_id.clone(),
            service_url: request.service_url.clone(),
            iou: request.iou.clone(),
            created_ts: now,
            expires_at: now + (request.expires_in_seconds) * 1000,
            is_valid: true,
        };
        self.pgts.write().await.insert(pgt.pgt_id.clone(), pgt.clone());
        Ok(pgt)
    }

    async fn get_pgt(&self, pgt_id: &str) -> Result<Option<CasProxyGrantingTicket>, ApiError> {
        Ok(self.pgts.read().await.get(pgt_id).cloned())
    }

    async fn create_proxy_ticket(&self, request: CreateProxyTicketRequest) -> Result<CasProxyTicket, ApiError> {
        let now = Utc::now().timestamp_millis();
        let ticket = CasProxyTicket {
            id: self.next_id().await,
            proxy_ticket_id: request.proxy_ticket_id.clone(),
            user_id: request.user_id.clone(),
            service_url: request.service_url.clone(),
            pgt_url: request.pgt_url.clone(),
            created_ts: now,
            expires_at: now + (request.expires_in_seconds) * 1000,
            consumed_ts: None,
            is_valid: true,
        };
        self.proxy_tickets.write().await.insert(ticket.proxy_ticket_id.clone(), ticket.clone());
        Ok(ticket)
    }

    async fn validate_proxy_ticket(
        &self,
        proxy_ticket_id: &str,
        _service_url: &str,
    ) -> Result<Option<CasProxyTicket>, ApiError> {
        let mut tickets = self.proxy_tickets.write().await;
        if let Some(ticket) = tickets.get_mut(proxy_ticket_id) {
            if ticket.is_valid {
                let now = Utc::now().timestamp_millis();
                ticket.consumed_ts = Some(now);
                return Ok(Some(ticket.clone()));
            }
        }
        Ok(None)
    }

    async fn register_service(&self, request: RegisterServiceRequest) -> Result<CasRegisteredService, ApiError> {
        let now = Utc::now().timestamp_millis();
        let service = CasRegisteredService {
            id: self.next_id().await,
            service_id: request.service_id.clone(),
            name: request.name.clone(),
            description: request.description.clone(),
            service_url_pattern: request.service_url_pattern.clone(),
            allowed_attributes: serde_json::Value::Null,
            allowed_proxy_callbacks: serde_json::Value::Null,
            is_enabled: true,
            is_require_secure: request.is_require_secure.unwrap_or(false),
            is_single_logout: request.is_single_logout.unwrap_or(false),
            created_ts: now,
            updated_ts: now,
        };
        self.services.write().await.insert(service.service_id.clone(), service.clone());
        Ok(service)
    }

    async fn get_service(&self, service_id: &str) -> Result<Option<CasRegisteredService>, ApiError> {
        Ok(self.services.read().await.get(service_id).cloned())
    }

    async fn get_service_by_url(&self, service_url: &str) -> Result<Option<CasRegisteredService>, ApiError> {
        Ok(self.services.read().await.values().find(|s| s.service_url_pattern == service_url).cloned())
    }

    async fn list_services(&self) -> Result<Vec<CasRegisteredService>, ApiError> {
        Ok(self.services.read().await.values().cloned().collect())
    }

    async fn delete_service(&self, service_id: &str) -> Result<bool, ApiError> {
        Ok(self.services.write().await.remove(service_id).is_some())
    }

    async fn set_user_attribute(
        &self,
        user_id: &str,
        attribute_name: &str,
        attribute_value: &str,
    ) -> Result<CasUserAttribute, ApiError> {
        let mut attrs = self.user_attributes.write().await;
        let user_attrs = attrs.entry(user_id.to_string()).or_default();
        user_attrs.insert(attribute_name.to_string(), attribute_value.to_string());
        Ok(CasUserAttribute {
            id: 0,
            user_id: user_id.to_string(),
            attribute_name: attribute_name.to_string(),
            attribute_value: attribute_value.to_string(),
            created_ts: 0,
            updated_ts: 0,
        })
    }

    async fn get_active_slo_sessions(&self, user_id: &str) -> Result<Vec<CasSloSession>, ApiError> {
        Ok(self.slo_sessions.read().await.iter().filter(|s| s.user_id == user_id).cloned().collect())
    }

    async fn cleanup_expired_tickets(&self) -> Result<u64, ApiError> {
        let now = Utc::now().timestamp_millis();
        let mut count = 0u64;
        let mut tickets = self.tickets.write().await;
        tickets.retain(|_, t| {
            if t.expires_at < now {
                count += 1;
                false
            } else {
                true
            }
        });
        Ok(count)
    }
}
