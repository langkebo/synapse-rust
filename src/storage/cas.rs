pub use synapse_storage::cas::*;

#[cfg(test)]
mod tests {
    use super::{CasStorage, CasTicket, CreateTicketRequest, RegisterServiceRequest};
    use sqlx::PgPool;
    use std::sync::Arc;

    #[test]
    fn root_cas_storage_reexport_keeps_constructor_shape() {
        let _ctor: fn(&Arc<PgPool>) -> CasStorage = CasStorage::new;
    }

    #[test]
    fn root_cas_storage_request_types_remain_accessible() {
        let ticket_request = CreateTicketRequest {
            ticket_id: "ST-test".to_string(),
            user_id: "@alice:example.com".to_string(),
            service_url: "https://app.example.com".to_string(),
            expires_in_seconds: 300,
        };
        let service_request = RegisterServiceRequest {
            service_id: "svc-001".to_string(),
            name: "My App".to_string(),
            description: Some("A test application".to_string()),
            service_url_pattern: "^https://myapp\\.example\\.com/.*$".to_string(),
            allowed_attributes: Some(vec!["email".to_string()]),
            allowed_proxy_callbacks: Some(vec!["https://callback.example.com".to_string()]),
            is_require_secure: Some(true),
            is_single_logout: Some(false),
        };

        assert_eq!(ticket_request.ticket_id, "ST-test");
        assert_eq!(service_request.service_id, "svc-001");
        assert_eq!(service_request.allowed_attributes.as_ref().map(Vec::len), Some(1));
    }

    #[test]
    fn root_cas_storage_types_remain_accessible() {
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
}
