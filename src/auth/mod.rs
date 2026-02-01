use crate::cache::*;
use crate::common::*;
use crate::storage::*;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub user_id: String,
    pub admin: bool,
    pub exp: i64,
    pub iat: i64,
    pub device_id: Option<String>,
}

#[derive(Clone)]
pub struct AuthService {
    pub user_storage: UserStorage,
    pub device_storage: DeviceStorage,
    pub token_storage: AccessTokenStorage,
    pub refresh_token_storage: RefreshTokenStorage,
    pub cache: Arc<CacheManager>,
    pub jwt_secret: Vec<u8>,
    pub token_expiry: i64,
    pub refresh_token_expiry: i64,
    pub server_name: String,
}

impl AuthService {
    pub fn new(
        pool: &Arc<sqlx::PgPool>,
        cache: Arc<CacheManager>,
        jwt_secret: &str,
        server_name: &str,
    ) -> Self {
        Self {
            user_storage: UserStorage::new(pool),
            device_storage: DeviceStorage::new(pool),
            token_storage: AccessTokenStorage::new(pool),
            refresh_token_storage: RefreshTokenStorage::new(pool),
            cache,
            jwt_secret: jwt_secret.as_bytes().to_vec(),
            token_expiry: 24 * 60 * 60,
            refresh_token_expiry: 7 * 24 * 60 * 60,
            server_name: server_name.to_string(),
        }
    }

    pub async fn register(
        &self,
        username: &str,
        password: &str,
        admin: bool,
        _displayname: Option<&str>,
    ) -> ApiResult<(User, String, String, String)> {
        if username.is_empty() || password.is_empty() {
            return Err(ApiError::bad_request(
                "Username and password are required".to_string(),
            ));
        }

        let existing_user = self
            .user_storage
            .get_user_by_username(username)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if existing_user.is_some() {
            return Err(ApiError::conflict("Username already taken".to_string()));
        }

        let user_id = format!("@{}:{}", username, self.server_name);
        let password_hash = self.hash_password(password)?;
        let user = self
            .user_storage
            .create_user(&user_id, username, Some(&password_hash), admin)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create user: {}", e)))?;

        let device_id = generate_token(16);
        self.device_storage
            .create_device(&device_id, &user_id, None)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create device: {}", e)))?;

        let access_token = self
            .generate_access_token(&user_id, &device_id, user.is_admin.unwrap_or(false))
            .await?;
        let refresh_token = self.generate_refresh_token(&user_id, &device_id).await?;

        Ok((user, access_token, refresh_token, device_id))
    }

    pub async fn login(
        &self,
        username: &str,
        password: &str,
        device_id: Option<&str>,
        _initial_display_name: Option<&str>,
    ) -> ApiResult<(User, String, String, String)> {
        let user = self
            .user_storage
            .get_user_by_identifier(username)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let user = match user {
            Some(u) => u,
            _ => return Err(ApiError::unauthorized("Invalid credentials".to_string())),
        };

        let password_hash = match &user.password_hash {
            Some(h) => h,
            _ => return Err(ApiError::unauthorized("Invalid credentials".to_string())),
        };

        if !self.verify_password(password, password_hash)? {
            return Err(ApiError::unauthorized("Invalid credentials".to_string()));
        }

        let device_id = match device_id {
            Some(d) => d.to_string(),
            _ => auth_generate_token(16),
        };

        if !self
            .device_storage
            .device_exists(&device_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        {
            self.device_storage
                .create_device(&device_id, &user.user_id, None)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to create device: {}", e)))?;
        }

        let access_token = self
            .generate_access_token(&user.user_id, &device_id, user.is_admin.unwrap_or(false))
            .await?;
        let refresh_token = self
            .generate_refresh_token(&user.user_id, &device_id)
            .await?;

        Ok((user, access_token, refresh_token, device_id))
    }

    pub async fn logout(&self, access_token: &str, device_id: Option<&str>) -> ApiResult<()> {
        self.token_storage
            .delete_token(access_token)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete token: {}", e)))?;

        if let Some(d_id) = device_id {
            self.token_storage
                .delete_device_tokens(d_id)
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to delete device tokens: {}", e))
                })?;
        }

        Ok(())
    }

    pub async fn logout_all(&self, user_id: &str) -> ApiResult<()> {
        self.token_storage
            .delete_user_tokens(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete tokens: {}", e)))?;

        self.device_storage
            .delete_user_devices(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete devices: {}", e)))?;

        Ok(())
    }

    pub async fn refresh_token(&self, refresh_token: &str) -> ApiResult<(String, String, String)> {
        let token_data = self
            .refresh_token_storage
            .get_refresh_token(refresh_token)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        match token_data {
            Some(t) => {
                if t.expires_ts > 0 && t.expires_ts < Utc::now().timestamp() {
                    return Err(ApiError::unauthorized("Refresh token expired".to_string()));
                }

                let user = self
                    .user_storage
                    .get_user_by_id(&t.user_id)
                    .await
                    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

                match user {
                    Some(u) => {
                        let new_access_token = self
                            .generate_access_token(
                                &u.user_id,
                                &t.device_id,
                                u.is_admin.unwrap_or(false),
                            )
                            .await?;
                        let new_refresh_token = self
                            .generate_refresh_token(&u.user_id, &t.device_id)
                            .await?;

                        self.refresh_token_storage
                            .delete_refresh_token(refresh_token)
                            .await
                            .map_err(|e| {
                                ApiError::internal(format!(
                                    "Failed to delete old refresh token: {}",
                                    e
                                ))
                            })?;

                        Ok((new_access_token, new_refresh_token, t.device_id))
                    }
                    _ => Err(ApiError::unauthorized("User not found".to_string())),
                }
            }
            _ => Err(ApiError::unauthorized("Invalid refresh token".to_string())),
        }
    }

    pub async fn validate_token(&self, token: &str) -> ApiResult<(String, Option<String>, bool)> {
        let cached_token = self.cache.get_token(token).await;

        if let Some(claims) = cached_token {
            return Ok((claims.user_id, None, claims.admin));
        }

        match self.decode_token(token) {
            Ok(claims) => {
                if claims.exp < Utc::now().timestamp() {
                    return Err(ApiError::unauthorized("Token expired".to_string()));
                }

                let user_exists = self
                    .user_storage
                    .user_exists(&claims.sub)
                    .await
                    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

                if user_exists {
                    self.cache.set_token(token, &claims, 3600).await;
                    Ok((claims.user_id, None, claims.admin))
                } else {
                    Err(ApiError::unauthorized("User not found".to_string()))
                }
            }
            Err(e) => Err(ApiError::unauthorized(format!("Invalid token: {}", e))),
        }
    }

    pub async fn change_password(&self, user_id: &str, new_password: &str) -> ApiResult<()> {
        let password_hash = self.hash_password(new_password)?;
        self.user_storage
            .update_password(user_id, &password_hash)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update password: {}", e)))?;

        self.token_storage
            .delete_user_tokens(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to invalidate tokens: {}", e)))?;

        Ok(())
    }

    pub async fn deactivate_user(&self, user_id: &str) -> ApiResult<()> {
        self.user_storage
            .deactivate_user(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to deactivate user: {}", e)))?;

        self.token_storage
            .delete_user_tokens(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete tokens: {}", e)))?;

        self.device_storage
            .delete_user_devices(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete devices: {}", e)))?;

        Ok(())
    }

    async fn generate_access_token(
        &self,
        user_id: &str,
        device_id: &str,
        admin: bool,
    ) -> ApiResult<String> {
        let now = Utc::now();
        let claims = Claims {
            sub: user_id.to_string(),
            user_id: user_id.to_string(),
            admin,
            exp: (now + Duration::seconds(self.token_expiry)).timestamp(),
            iat: now.timestamp(),
            device_id: Some(device_id.to_string()),
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&self.jwt_secret),
        )
        .map_err(|e| ApiError::internal(format!("Failed to generate token: {}", e)))
    }

    async fn generate_refresh_token(&self, user_id: &str, device_id: &str) -> ApiResult<String> {
        let token = generate_token(32);
        let expiry_ts = Utc::now() + Duration::seconds(self.refresh_token_expiry);
        let expiry_timestamp = expiry_ts.timestamp();

        self.refresh_token_storage
            .create_refresh_token(&token, user_id, device_id, Some(expiry_timestamp))
            .await
            .map_err(|e| ApiError::internal(format!("Failed to store refresh token: {}", e)))?;

        Ok(token)
    }

    fn decode_token(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        jsonwebtoken::decode(
            token,
            &DecodingKey::from_secret(&self.jwt_secret),
            &Validation::default(),
        )
        .map(|e| e.claims)
    }

    fn hash_password(&self, password: &str) -> Result<String, ApiError> {
        let salt = auth_generate_token(16);
        let mut output = [0u8; 32];
        let mut input = password.as_bytes().to_vec();
        input.extend_from_slice(salt.as_bytes());

        for _ in 0..10000 {
            let mut hasher = Sha256::new();
            hasher.update(&input);
            let result = hasher.finalize();
            output.copy_from_slice(&result);
            input = output.to_vec();
        }

        let hash = STANDARD.encode(output);
        Ok(format!("$sha256$v=1$m=32,p=1${}${}${}", salt, 10000, hash))
    }

    fn verify_password(&self, password: &str, password_hash: &str) -> Result<bool, ApiError> {
        if password_hash.starts_with("$sha256$") {
            self.verify_sha256_password(password, password_hash)
        } else {
            Ok(password_hash.starts_with("$argon2"))
        }
    }

    fn verify_sha256_password(
        &self,
        password: &str,
        password_hash: &str,
    ) -> Result<bool, ApiError> {
        let parts: Vec<&str> = password_hash.split('$').collect();

        if parts.len() >= 7 {
            let salt = parts[4];
            let iterations = parts[5].parse::<u32>().unwrap_or(10000);

            let mut hash = [0u8; 32];
            let mut input = password.as_bytes().to_vec();
            input.extend_from_slice(salt.as_bytes());

            for _ in 0..iterations {
                let mut hasher = Sha256::new();
                hasher.update(&input);
                let result = hasher.finalize();
                hash.copy_from_slice(&result);
                input = hash.to_vec();
            }

            let expected_hash = STANDARD.encode(hash);
            let stored_hash = parts[6];

            Ok(secure_compare(&expected_hash, stored_hash))
        } else {
            Ok(false)
        }
    }
}

fn auth_generate_token(length: usize) -> String {
    static CHARSET: [u8; 62] = *b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    let mut token = String::with_capacity(length);
    for _ in 0..length {
        let idx = (rng.next_u32() as usize) % CHARSET.len();
        token.push(CHARSET[idx] as char);
    }
    token
}

fn secure_compare(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (byte_a, byte_b) in a.bytes().zip(b.bytes()) {
        result |= byte_a ^ byte_b;
    }

    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claims_struct() {
        let claims = Claims {
            sub: "@test:example.com".to_string(),
            user_id: "@test:example.com".to_string(),
            admin: false,
            exp: 1234567890,
            iat: 1234567889,
            device_id: Some("DEVICE123".to_string()),
        };
        assert_eq!(claims.sub, "@test:example.com");
        assert_eq!(claims.user_id, "@test:example.com");
        assert!(!claims.admin);
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn test_claims_with_admin() {
        let claims = Claims {
            sub: "@admin:example.com".to_string(),
            user_id: "@admin:example.com".to_string(),
            admin: true,
            exp: 1234567890,
            iat: 1234567890,
            device_id: None,
        };
        assert!(claims.admin);
        assert!(claims.device_id.is_none());
    }

    #[test]
    fn test_generate_token_length() {
        for len in [8, 16, 32, 64] {
            let token = auth_generate_token(len);
            assert_eq!(token.len(), len);
        }
    }

    #[test]
    fn test_generate_token_chars() {
        let token = auth_generate_token(100);
        for c in token.chars() {
            assert!(c.is_ascii_alphanumeric());
        }
    }

    #[test]
    fn test_claims_serialization() {
        let claims = Claims {
            sub: "@test:example.com".to_string(),
            user_id: "@test:example.com".to_string(),
            admin: false,
            exp: 1234567890,
            iat: 1234567890,
            device_id: Some("DEVICE123".to_string()),
        };
        let json = serde_json::to_string(&claims).unwrap();
        let deserialized: Claims = serde_json::from_str(&json).unwrap();
        assert_eq!(claims.sub, deserialized.sub);
        assert_eq!(claims.user_id, deserialized.user_id);
        assert_eq!(claims.admin, deserialized.admin);
    }
}
