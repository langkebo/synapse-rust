pub use synapse_storage::registration_token::*;

#[cfg(test)]
mod tests {
    use super::{
        decode_registration_token_cursor, encode_registration_token_cursor, CreateRegistrationTokenRequest,
        RegistrationTokenCursor, RegistrationTokenStorage, UpdateRegistrationTokenRequest,
    };
    use sqlx::PgPool;
    use std::sync::Arc;

    #[test]
    fn root_registration_token_cursor_round_trip() {
        let cursor = RegistrationTokenCursor { created_ts: 1_746_700_000_000, id: 42 };

        let encoded = encode_registration_token_cursor(&cursor);
        assert_eq!(decode_registration_token_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn root_registration_token_cursor_rejects_invalid_values() {
        assert_eq!(decode_registration_token_cursor(None), None);
        assert_eq!(decode_registration_token_cursor(Some("bad")), None);
        assert_eq!(decode_registration_token_cursor(Some("123|")), None);
        assert_eq!(decode_registration_token_cursor(Some("123|456|789")), None);
    }

    #[test]
    fn root_registration_token_storage_reexport_keeps_constructor_shape() {
        let _ctor: fn(&Arc<PgPool>) -> RegistrationTokenStorage = RegistrationTokenStorage::new;
    }

    #[test]
    fn root_registration_token_request_types_remain_accessible() {
        let create = CreateRegistrationTokenRequest {
            token: Some("CustomToken123".to_string()),
            token_type: Some("multi_use".to_string()),
            description: Some("Custom token".to_string()),
            max_uses: Some(5),
            expires_at: Some(1_800_000_000_000),
            created_by: Some("@admin:example.com".to_string()),
            allowed_email_domains: Some(vec!["test.com".to_string()]),
            allowed_user_ids: Some(vec!["@user:test.com".to_string()]),
            auto_join_rooms: Some(vec!["!room:test.com".to_string()]),
            display_name: Some("Display Name".to_string()),
            email: Some("email@test.com".to_string()),
        };
        let update = UpdateRegistrationTokenRequest::default();

        assert_eq!(create.max_uses, Some(5));
        assert!(update.description.is_none());
    }
}
