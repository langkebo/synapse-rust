pub use synapse_services::cas_service::*;

#[cfg(test)]
mod tests {
    use super::{CasService, CasValidationResponse, RegisterServiceRequest};
    use crate::storage::cas::CasStorage;
    use std::sync::Arc;

    #[test]
    fn root_cas_service_reexport_keeps_constructor_shape() {
        let _ctor: fn(Arc<CasStorage>, String) -> CasService = CasService::new;
    }

    #[test]
    fn root_cas_service_reexports_keep_request_types() {
        let request = RegisterServiceRequest {
            service_id: "svc-001".to_string(),
            name: "My App".to_string(),
            description: Some("A test application".to_string()),
            service_url_pattern: "^https://myapp\\.example\\.com/.*$".to_string(),
            allowed_attributes: Some(vec!["email".to_string()]),
            allowed_proxy_callbacks: Some(vec!["https://callback.example.com".to_string()]),
            is_require_secure: Some(true),
            is_single_logout: Some(false),
        };

        assert_eq!(request.service_id, "svc-001");
        assert_eq!(request.name, "My App");
        assert_eq!(request.allowed_proxy_callbacks.as_ref().map(Vec::len), Some(1));
    }

    #[test]
    fn root_cas_validation_response_remains_accessible() {
        let response = CasValidationResponse::Failure {
            code: "INVALID_TICKET".to_string(),
            description: "Ticket is expired or invalid".to_string(),
        };
        let xml = response.to_xml();
        assert!(xml.contains("<cas:authenticationFailure"));
        assert!(xml.contains("code=\"INVALID_TICKET\""));
        assert!(xml.contains("Ticket is expired or invalid"));
        assert!(xml.contains("xmlns:cas=\"https://cas.example.org/cas\""));
    }
}
