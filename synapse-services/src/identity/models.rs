use serde::{Deserialize, Serialize};
use synapse_common::current_timestamp_millis;

/// The `ThirdPartyId` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThirdPartyId {
    /// The `address` field.
    pub address: String,
    /// The `medium` field.
    pub medium: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `validated_ts` field.
    pub validated_ts: i64,
    /// The `added_ts` field.
    pub added_ts: i64,
}

/// The `ThirdPartyIdValidation` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThirdPartyIdValidation {
    /// The `sid` field.
    pub sid: String,
    /// The `client_secret` field.
    pub client_secret: String,
    /// The `medium` field.
    pub medium: String,
    /// The `address` field.
    pub address: String,
}

/// The `IdentityServerInfo` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityServerInfo {
    /// The `trusted_servers` field.
    pub trusted_servers: Vec<String>,
    /// The `api_endpoint` field.
    pub api_endpoint: String,
}

/// The `BindingRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingRequest {
    /// The `sid` field.
    pub sid: String,
    /// The `client_secret` field.
    pub client_secret: String,
    /// The `id_server` field.
    pub id_server: String,
    /// The `id_access_token` field.
    pub id_access_token: String,
}

/// The `BindingResponse` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingResponse {
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: Option<String>,
}

/// The `UnbindingRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnbindingRequest {
    /// The `id_server` field.
    pub id_server: String,
    /// The `id_access_token` field.
    pub id_access_token: String,
}

/// The `Invitation` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invitation {
    /// The `room_id` field.
    pub room_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `medium` field.
    pub medium: String,
    /// The `address` field.
    pub address: String,
    /// The `id_server` field.
    pub id_server: String,
    /// The `display_name` field.
    pub display_name: Option<String>,
}

/// The `InvitationResponse` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvitationResponse {
    /// The `user_id` field.
    pub user_id: Option<String>,
    /// The `signed` field.
    pub signed: Option<serde_json::Value>,
}

/// The `Invite3pid` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invite3pid {
    /// The `id_server` field.
    pub id_server: String,
    /// The `id_access_token` field.
    pub id_access_token: String,
    /// The `medium` field.
    pub medium: String,
    /// The `address` field.
    pub address: String,
    /// The `signer` field.
    pub signer: String,
    /// The `signature` field.
    pub signature: Option<serde_json::Value>,
}

/// The `LookupRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupRequest {
    /// The `medium` field.
    pub medium: String,
    /// The `address` field.
    pub address: String,
}

/// The `LookupResponse` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupResponse {
    /// The `user_id` field.
    pub user_id: String,
    /// The `medium` field.
    pub medium: String,
    /// The `address` field.
    pub address: String,
    /// The `not_before` field.
    pub not_before: Option<i64>,
    /// The `not_after` field.
    pub not_after: Option<i64>,
    /// The `devices` field.
    pub devices: Option<Vec<serde_json::Value>>,
}

/// The `HashLookupRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashLookupRequest {
    /// The `algorithm` field.
    pub algorithm: String,
    /// The `addresses` field.
    pub addresses: Vec<String>,
    /// The `mediums` field.
    pub mediums: Vec<String>,
}

/// The `HashLookupResponse` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashLookupResponse {
    /// The `chunk` field.
    pub chunk: Vec<serde_json::Value>,
}

impl ThirdPartyId {
    /// See [`new`].
    pub fn new(address: &str, medium: &str, user_id: &str) -> Self {
        let now = current_timestamp_millis();
        Self {
            address: address.to_string(),
            medium: medium.to_string(),
            user_id: user_id.to_string(),
            validated_ts: now,
            added_ts: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_third_party_id_new() {
        let id = ThirdPartyId::new("user@example.com", "email", "@user:example.com");
        assert_eq!(id.address, "user@example.com");
        assert_eq!(id.medium, "email");
        assert_eq!(id.user_id, "@user:example.com");
        assert!(id.validated_ts > 0);
        assert!(id.added_ts > 0);
        assert_eq!(id.validated_ts, id.added_ts);
    }

    #[test]
    fn test_third_party_id_serialization() {
        let id = ThirdPartyId {
            address: "user@example.com".to_string(),
            medium: "email".to_string(),
            user_id: "@user:example.com".to_string(),
            validated_ts: 1700000000000,
            added_ts: 1700000000000,
        };
        let json = serde_json::to_string(&id).unwrap();
        assert!(json.contains("user@example.com"));
        assert!(json.contains("email"));
        assert!(json.contains("@user:example.com"));
    }

    #[test]
    fn test_third_party_id_validation() {
        let validation = ThirdPartyIdValidation {
            sid: "session_123".to_string(),
            client_secret: "secret_abc".to_string(),
            medium: "email".to_string(),
            address: "user@example.com".to_string(),
        };
        assert_eq!(validation.sid, "session_123");
        assert_eq!(validation.medium, "email");
    }

    #[test]
    fn test_identity_server_info() {
        let info = IdentityServerInfo {
            trusted_servers: vec!["id.example.com".to_string()],
            api_endpoint: "https://id.example.com".to_string(),
        };
        assert_eq!(info.trusted_servers.len(), 1);
        assert_eq!(info.api_endpoint, "https://id.example.com");
    }

    #[test]
    fn test_binding_request() {
        let request = BindingRequest {
            sid: "session_123".to_string(),
            client_secret: "secret_abc".to_string(),
            id_server: "id.example.com".to_string(),
            id_access_token: "access_token_xyz".to_string(),
        };
        assert_eq!(request.sid, "session_123");
        assert_eq!(request.id_server, "id.example.com");
    }

    #[test]
    fn test_binding_response() {
        let response =
            BindingResponse { user_id: "@user:example.com".to_string(), device_id: Some("device_1".to_string()) };
        assert_eq!(response.user_id, "@user:example.com");
        assert_eq!(response.device_id.as_deref(), Some("device_1"));
    }

    #[test]
    fn test_binding_response_no_device() {
        let response = BindingResponse { user_id: "@user:example.com".to_string(), device_id: None };
        assert_eq!(response.user_id, "@user:example.com");
        assert!(response.device_id.is_none());
    }

    #[test]
    fn test_unbinding_request() {
        let request = UnbindingRequest {
            id_server: "id.example.com".to_string(),
            id_access_token: "access_token_xyz".to_string(),
        };
        assert_eq!(request.id_server, "id.example.com");
    }

    #[test]
    fn test_invitation() {
        let invitation = Invitation {
            room_id: "!room:example.com".to_string(),
            sender: "@inviter:example.com".to_string(),
            medium: "email".to_string(),
            address: "invitee@example.com".to_string(),
            id_server: "id.example.com".to_string(),
            display_name: Some("Inviter Name".to_string()),
        };
        assert_eq!(invitation.room_id, "!room:example.com");
        assert_eq!(invitation.medium, "email");
        assert_eq!(invitation.display_name.as_deref(), Some("Inviter Name"));
    }

    #[test]
    fn test_invitation_response() {
        let response = InvitationResponse {
            user_id: Some("@user:example.com".to_string()),
            signed: Some(serde_json::json!({"signatures": {}})),
        };
        assert_eq!(response.user_id.as_deref(), Some("@user:example.com"));
        assert!(response.signed.is_some());
    }

    #[test]
    fn test_invite_3pid() {
        let invite = Invite3pid {
            id_server: "id.example.com".to_string(),
            id_access_token: "token".to_string(),
            medium: "email".to_string(),
            address: "user@example.com".to_string(),
            signer: "@signer:example.com".to_string(),
            signature: None,
        };
        assert_eq!(invite.medium, "email");
        assert_eq!(invite.address, "user@example.com");
        assert_eq!(invite.signer, "@signer:example.com");
    }

    #[test]
    fn test_lookup_request() {
        let request = LookupRequest { medium: "email".to_string(), address: "user@example.com".to_string() };
        assert_eq!(request.medium, "email");
        assert_eq!(request.address, "user@example.com");
    }

    #[test]
    fn test_lookup_response() {
        let response = LookupResponse {
            user_id: "@user:example.com".to_string(),
            medium: "email".to_string(),
            address: "user@example.com".to_string(),
            not_before: Some(1700000000000),
            not_after: None,
            devices: None,
        };
        assert_eq!(response.user_id, "@user:example.com");
        assert_eq!(response.not_before, Some(1700000000000));
    }

    #[test]
    fn test_hash_lookup_request() {
        let request = HashLookupRequest {
            algorithm: "sha256".to_string(),
            addresses: vec!["hash1".to_string(), "hash2".to_string()],
            mediums: vec!["email".to_string()],
        };
        assert_eq!(request.algorithm, "sha256");
        assert_eq!(request.addresses.len(), 2);
        assert_eq!(request.mediums.len(), 1);
    }
}
