use crate::common::config::VoipConfig;
use crate::common::error::ApiError;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha1 = Hmac<Sha1>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnCredentials {
    pub username: String,
    pub password: String,
    pub uris: Vec<String>,
    pub ttl: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoipSettings {
    pub turn_uris: Vec<String>,
    pub turn_username: Option<String>,
    pub turn_password: Option<String>,
    pub stun_uris: Vec<String>,
}

pub struct VoipService {
    config: Arc<VoipConfig>,
}

impl VoipService {
    pub fn new(config: Arc<VoipConfig>) -> Self {
        Self { config }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.is_enabled()
    }

    pub fn get_settings(&self) -> VoipSettings {
        VoipSettings {
            turn_uris: self.config.turn_uris.clone(),
            turn_username: self.config.turn_username.clone(),
            turn_password: self.config.turn_password.clone(),
            stun_uris: self.config.stun_uris.clone(),
        }
    }

    pub fn generate_turn_credentials(&self, user_id: &str) -> Result<TurnCredentials, ApiError> {
        if !self.is_enabled() {
            return Err(ApiError::bad_request("VoIP/TURN is not configured"));
        }

        if self.config.turn_uris.is_empty() {
            return Err(ApiError::bad_request("No TURN URIs configured"));
        }

        let lifetime = self.config.lifetime_seconds();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| ApiError::internal(format!("Time error: {}", e)))?
            .as_secs() as i64;

        let expiry = now + lifetime;

        let (username, password) = if let Some(ref secret) = self.config.turn_shared_secret {
            let username = format!("{}:{}", expiry, user_id);
            let password = self.generate_turn_password(&username, secret)?;
            (username, password)
        } else if let (Some(ref username), Some(ref password)) =
            (&self.config.turn_username, &self.config.turn_password)
        {
            (username.clone(), password.clone())
        } else {
            return Err(ApiError::internal(
                "TURN credentials not configured. Set turn_shared_secret or turn_username/turn_password",
            ));
        };

        Ok(TurnCredentials {
            username,
            password,
            uris: self.config.turn_uris.clone(),
            ttl: lifetime,
        })
    }

    fn generate_turn_password(&self, username: &str, secret: &str) -> Result<String, ApiError> {
        let mut mac = HmacSha1::new_from_slice(secret.as_bytes())
            .map_err(|e| ApiError::internal(format!("HMAC error: {}", e)))?;
        mac.update(username.as_bytes());
        let result = mac.finalize();
        Ok(BASE64.encode(result.into_bytes()))
    }

    pub fn can_guest_use_turn(&self) -> bool {
        self.config.turn_allow_guests
    }

    pub fn get_turn_uris(&self) -> Vec<String> {
        self.config.turn_uris.clone()
    }

    pub fn get_stun_uris(&self) -> Vec<String> {
        self.config.stun_uris.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> VoipConfig {
        VoipConfig {
            turn_uris: vec![
                "turn:turn.example.com:3478?transport=udp".to_string(),
                "turn:turn.example.com:3478?transport=tcp".to_string(),
            ],
            turn_shared_secret: Some("test_secret_key".to_string()),
            turn_shared_secret_path: None,
            turn_username: None,
            turn_password: None,
            turn_user_lifetime: "1h".to_string(),
            turn_allow_guests: true,
            stun_uris: vec!["stun:stun.example.com:3478".to_string()],
        }
    }

    #[test]
    fn test_voip_config_enabled() {
        let config = create_test_config();
        assert!(config.is_enabled());
    }

    #[test]
    fn test_voip_config_disabled() {
        let config = VoipConfig::default();
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_lifetime_seconds() {
        let config = create_test_config();
        assert_eq!(config.lifetime_seconds(), 3600);
    }

    #[test]
    fn test_lifetime_seconds_custom() {
        let config = VoipConfig {
            turn_user_lifetime: "30m".to_string(),
            ..Default::default()
        };
        assert_eq!(config.lifetime_seconds(), 1800);
    }

    #[test]
    fn test_generate_turn_credentials() {
        let config = Arc::new(create_test_config());
        let service = VoipService::new(config);

        let creds = service
            .generate_turn_credentials("@alice:example.com")
            .unwrap();

        assert!(!creds.username.is_empty());
        assert!(!creds.password.is_empty());
        assert_eq!(creds.uris.len(), 2);
        assert_eq!(creds.ttl, 3600);
        assert!(creds.username.contains(':'));
    }

    #[test]
    fn test_generate_turn_credentials_static() {
        let config = Arc::new(VoipConfig {
            turn_uris: vec!["turn:turn.example.com:3478".to_string()],
            turn_shared_secret: None,
            turn_username: Some("static_user".to_string()),
            turn_password: Some("static_pass".to_string()),
            ..Default::default()
        });
        let service = VoipService::new(config);

        let creds = service
            .generate_turn_credentials("@alice:example.com")
            .unwrap();

        assert_eq!(creds.username, "static_user");
        assert_eq!(creds.password, "static_pass");
    }

    #[test]
    fn test_turn_password_generation() {
        let config = Arc::new(create_test_config());
        let service = VoipService::new(config);

        let password1 = service
            .generate_turn_password("test_user", "secret")
            .unwrap();
        let password2 = service
            .generate_turn_password("test_user", "secret")
            .unwrap();

        assert_eq!(password1, password2);
        assert!(!password1.is_empty());
    }

    #[test]
    fn test_guest_access() {
        let config = Arc::new(create_test_config());
        let service = VoipService::new(config);
        assert!(service.can_guest_use_turn());
    }

    #[test]
    fn test_get_uris() {
        let config = Arc::new(create_test_config());
        let service = VoipService::new(config);

        assert_eq!(service.get_turn_uris().len(), 2);
        assert_eq!(service.get_stun_uris().len(), 1);
    }
}
