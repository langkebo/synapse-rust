use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct OlmAccount {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub identity_key: String,
    pub serialized_account: String,
    pub is_one_time_keys_published: bool,
    pub is_fallback_key_published: bool,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_olm_account_struct() {
        let account = OlmAccount {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            identity_key: "identity_key_base64".to_string(),
            serialized_account: "serialized_olm_account".to_string(),
            is_one_time_keys_published: false,
            is_fallback_key_published: false,
            created_ts: 1234567890000,
            updated_ts: 1234567890000,
        };

        assert_eq!(account.user_id, "@alice:example.com");
        assert_eq!(account.device_id, "DEVICE123");
        assert!(!account.is_one_time_keys_published);
        assert!(!account.is_fallback_key_published);
    }

    #[test]
    fn test_olm_account_with_published_keys() {
        let account = OlmAccount {
            id: 2,
            user_id: "@bob:example.com".to_string(),
            device_id: "DEVICE456".to_string(),
            identity_key: "another_identity_key".to_string(),
            serialized_account: "another_serialized_account".to_string(),
            is_one_time_keys_published: true,
            is_fallback_key_published: true,
            created_ts: 1234567890000,
            updated_ts: 1234567900000,
        };

        assert!(account.is_one_time_keys_published);
        assert!(account.is_fallback_key_published);
        assert!(account.updated_ts > account.created_ts);
    }
}
