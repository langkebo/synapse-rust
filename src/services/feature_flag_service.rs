pub use synapse_services::feature_flag_service::*;

#[cfg(test)]
mod tests {
    use super::FeatureFlagService;
    use crate::services::admin_audit_service::AdminAuditService;
    use crate::storage::FeatureFlagStorage;
    use std::sync::Arc;

    #[test]
    fn root_feature_flag_service_reexport_keeps_constructor_shape() {
        let _ctor: fn(Arc<FeatureFlagStorage>, Arc<AdminAuditService>) -> FeatureFlagService = FeatureFlagService::new;
    }
}
