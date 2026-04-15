#[derive(Debug, Clone)]
pub struct AuthorizationContext {
    pub user_id: String,
    pub is_admin: bool,
    pub device_id: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ResourceType {
    User,
    Room,
    Device,
    Media,
    Event,
    AccountData,
}

#[derive(Debug, Clone)]
pub enum Action {
    Read,
    Write,
    Delete,
    Admin,
    Invite,
    Ban,
    Kick,
    Redact,
    ModifyPowerLevels,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_authorization_context_creation() {
        let ctx = AuthorizationContext {
            user_id: "@user:example.com".to_string(),
            is_admin: false,
            device_id: Some("DEVICE123".to_string()),
        };

        assert_eq!(ctx.user_id, "@user:example.com");
        assert!(!ctx.is_admin);
        assert!(ctx.device_id.is_some());
    }

    #[test]
    fn test_authorization_context_admin() {
        let ctx = AuthorizationContext {
            user_id: "@admin:example.com".to_string(),
            is_admin: true,
            device_id: None,
        };

        assert!(ctx.is_admin);
        assert!(ctx.device_id.is_none());
    }

    #[test]
    fn test_resource_type_variants() {
        let types = [
            ResourceType::User,
            ResourceType::Room,
            ResourceType::Device,
            ResourceType::Media,
            ResourceType::Event,
            ResourceType::AccountData,
        ];

        assert_eq!(types.len(), 6);
    }

    #[test]
    fn test_action_variants() {
        let actions = vec![
            Action::Read,
            Action::Write,
            Action::Delete,
            Action::Admin,
            Action::Invite,
            Action::Ban,
            Action::Kick,
            Action::Redact,
            Action::ModifyPowerLevels,
        ];

        assert_eq!(actions.len(), 9);
    }
}
