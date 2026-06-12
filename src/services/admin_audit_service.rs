pub use synapse_services::admin_audit_service::*;

#[cfg(test)]
mod tests {
    use super::AdminAuditService;
    use crate::storage::AuditEventStorage;
    use std::sync::Arc;

    #[test]
    fn root_admin_audit_service_reexport_keeps_constructor_shape() {
        let _ctor: fn(Arc<AuditEventStorage>) -> AdminAuditService = AdminAuditService::new;
    }
}
