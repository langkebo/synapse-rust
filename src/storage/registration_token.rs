use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RegistrationToken {
    pub id: i64,
    pub token: String,
    pub token_type: String,
    pub description: Option<String>,
    pub max_uses: i32,
    pub current_uses: i32,
    pub is_used: bool,
    pub is_enabled: bool,
    pub expires_at: Option<i64>,
    pub created_by: Option<String>,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub last_used_ts: Option<i64>,
    pub allowed_email_domains: Option<Vec<String>>,
    pub allowed_user_ids: Option<Vec<String>>,
    pub auto_join_rooms: Option<Vec<String>>,
    pub display_name: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RegistrationTokenUsage {
    pub id: i64,
    pub token_id: i64,
    pub token: String,
    pub user_id: String,
    pub username: Option<String>,
    pub email: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub used_ts: i64,
    pub success: bool,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoomInvite {
    pub id: i64,
    pub invite_code: String,
    pub room_id: String,
    pub inviter_user_id: String,
    pub invitee_email: Option<String>,
    pub invitee_user_id: Option<String>,
    pub is_used: bool,
    pub is_revoked: bool,
    pub expires_at: Option<i64>,
    pub created_ts: i64,
    pub used_ts: Option<i64>,
    pub revoked_ts: Option<i64>,
    pub revoked_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RegistrationTokenBatch {
    pub id: i64,
    pub batch_id: String,
    pub description: Option<String>,
    pub token_count: i32,
    pub tokens_used: i32,
    pub created_by: Option<String>,
    pub created_ts: i64,
    pub expires_at: Option<i64>,
    pub is_enabled: bool,
    pub allowed_email_domains: Option<Vec<String>>,
    pub auto_join_rooms: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRegistrationTokenRequest {
    pub token: Option<String>,
    pub token_type: Option<String>,
    pub description: Option<String>,
    pub max_uses: Option<i32>,
    pub expires_at: Option<i64>,
    pub created_by: Option<String>,
    pub allowed_email_domains: Option<Vec<String>>,
    pub allowed_user_ids: Option<Vec<String>>,
    pub auto_join_rooms: Option<Vec<String>>,
    pub display_name: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateRegistrationTokenRequest {
    pub description: Option<String>,
    pub max_uses: Option<i32>,
    pub is_enabled: Option<bool>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoomInviteRequest {
    pub room_id: String,
    pub inviter_user_id: String,
    pub invitee_email: Option<String>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenValidationResult {
    pub is_valid: bool,
    pub token_id: Option<i64>,
    pub error_message: Option<String>,
}

#[derive(Clone)]
pub struct RegistrationTokenStorage {
    pool: Arc<PgPool>,
}

impl RegistrationTokenStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_token(
        &self,
        request: CreateRegistrationTokenRequest,
    ) -> Result<RegistrationToken, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let token = request.token.unwrap_or_else(Self::generate_token);
        let token_type = request
            .token_type
            .unwrap_or_else(|| "single_use".to_string());

        let row = sqlx::query_as::<_, RegistrationToken>(
            r#"
            INSERT INTO registration_tokens (
                token, token_type, description, max_uses, expires_at, created_by,
                created_ts, updated_ts, allowed_email_domains, allowed_user_ids,
                auto_join_rooms, display_name, email
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $7, $8, $9, $10, $11, $12)
            RETURNING *
            "#,
        )
        .bind(&token)
        .bind(&token_type)
        .bind(&request.description)
        .bind(request.max_uses.unwrap_or(1))
        .bind(request.expires_at)
        .bind(&request.created_by)
        .bind(now)
        .bind(&request.allowed_email_domains)
        .bind(&request.allowed_user_ids)
        .bind(&request.auto_join_rooms)
        .bind(&request.display_name)
        .bind(&request.email)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub fn generate_token() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let chars: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789";
        let token: String = (0..32)
            .map(|_| chars[rng.gen_range(0..chars.len())] as char)
            .collect();
        token
    }

    pub async fn get_token(&self, token: &str) -> Result<Option<RegistrationToken>, sqlx::Error> {
        let row = sqlx::query_as::<_, RegistrationToken>(
            "SELECT * FROM registration_tokens WHERE token = $1",
        )
        .bind(token)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_token_by_id(&self, id: i64) -> Result<Option<RegistrationToken>, sqlx::Error> {
        let row = sqlx::query_as::<_, RegistrationToken>(
            "SELECT * FROM registration_tokens WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn update_token(
        &self,
        id: i64,
        request: UpdateRegistrationTokenRequest,
    ) -> Result<RegistrationToken, sqlx::Error> {
        let row = sqlx::query_as::<_, RegistrationToken>(
            r#"
            UPDATE registration_tokens SET
                description = COALESCE($2, description),
                max_uses = COALESCE($3, max_uses),
                is_enabled = COALESCE($4, is_enabled),
                expires_at = COALESCE($5, expires_at)
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(&request.description)
        .bind(request.max_uses)
        .bind(request.is_enabled)
        .bind(request.expires_at)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn delete_token(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM registration_tokens WHERE id = $1")
            .bind(id)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    pub async fn validate_token(&self, token: &str) -> Result<TokenValidationResult, sqlx::Error> {
        let token_record = self.get_token(token).await?;

        match token_record {
            None => Ok(TokenValidationResult {
                is_valid: false,
                token_id: None,
                error_message: Some("Token not found".to_string()),
            }),
            Some(t) => {
                if !t.is_enabled {
                    return Ok(TokenValidationResult {
                        is_valid: false,
                        token_id: Some(t.id),
                        error_message: Some("Token is not active".to_string()),
                    });
                }

                if t.is_used && t.token_type == "single_use" {
                    return Ok(TokenValidationResult {
                        is_valid: false,
                        token_id: Some(t.id),
                        error_message: Some("Token has already been used".to_string()),
                    });
                }

                if t.max_uses > 0 && t.current_uses >= t.max_uses {
                    return Ok(TokenValidationResult {
                        is_valid: false,
                        token_id: Some(t.id),
                        error_message: Some("Token has reached maximum uses".to_string()),
                    });
                }

                if let Some(expires_at) = t.expires_at {
                    let now = Utc::now().timestamp_millis();
                    if expires_at < now {
                        return Ok(TokenValidationResult {
                            is_valid: false,
                            token_id: Some(t.id),
                            error_message: Some("Token has expired".to_string()),
                        });
                    }
                }

                Ok(TokenValidationResult {
                    is_valid: true,
                    token_id: Some(t.id),
                    error_message: None,
                })
            }
        }
    }

    pub async fn use_token(
        &self,
        token: &str,
        user_id: &str,
        username: Option<&str>,
        email: Option<&str>,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let validation = self.validate_token(token).await?;

        if !validation.is_valid {
            return Ok(false);
        }

        let token_id = match validation.token_id {
            Some(id) => id,
            None => {
                tracing::error!("Token validation succeeded but token_id is None");
                return Ok(false);
            }
        };
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE registration_tokens
            SET current_uses = current_uses + 1,
                is_used = CASE WHEN token_type = 'single_use' THEN TRUE ELSE is_used END,
                last_used_ts = $2
            WHERE id = $1
            "#,
        )
        .bind(token_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO registration_token_usage (
                token_id, token, user_id, username, email, ip_address, user_agent, used_ts
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(token_id)
        .bind(token)
        .bind(user_id)
        .bind(username)
        .bind(email)
        .bind(ip_address)
        .bind(user_agent)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(true)
    }

    pub async fn get_all_tokens(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<RegistrationToken>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RegistrationToken>(
            "SELECT * FROM registration_tokens ORDER BY created_ts DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_active_tokens(&self) -> Result<Vec<RegistrationToken>, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let rows = sqlx::query_as::<_, RegistrationToken>(
            r#"
            SELECT * FROM registration_tokens 
            WHERE is_enabled = TRUE 
            AND (expires_at IS NULL OR expires_at > $1)
            AND (max_uses = 0 OR current_uses < max_uses)
            ORDER BY created_ts DESC
            "#,
        )
        .bind(now)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_token_usage(
        &self,
        token_id: i64,
    ) -> Result<Vec<RegistrationTokenUsage>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RegistrationTokenUsage>(
            "SELECT * FROM registration_token_usage WHERE token_id = $1 ORDER BY used_ts DESC",
        )
        .bind(token_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn deactivate_token(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE registration_tokens SET is_enabled = FALSE WHERE id = $1")
            .bind(id)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    pub async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let result = sqlx::query(
            "UPDATE registration_tokens SET is_enabled = FALSE WHERE expires_at IS NOT NULL AND expires_at < $1 AND is_enabled = TRUE",
        )
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }

    pub async fn create_room_invite(
        &self,
        request: CreateRoomInviteRequest,
    ) -> Result<RoomInvite, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let invite_code = Self::generate_token();

        let row = sqlx::query_as::<_, RoomInvite>(
            r#"
            INSERT INTO room_invites (
                invite_code, room_id, inviter_user_id, invitee_email, expires_at, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(&invite_code)
        .bind(&request.room_id)
        .bind(&request.inviter_user_id)
        .bind(&request.invitee_email)
        .bind(request.expires_at)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_room_invite(
        &self,
        invite_code: &str,
    ) -> Result<Option<RoomInvite>, sqlx::Error> {
        let row =
            sqlx::query_as::<_, RoomInvite>("SELECT * FROM room_invites WHERE invite_code = $1")
                .bind(invite_code)
                .fetch_optional(&*self.pool)
                .await?;

        Ok(row)
    }

    pub async fn use_room_invite(
        &self,
        invite_code: &str,
        invitee_user_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let invite = self.get_room_invite(invite_code).await?;

        match invite {
            None => Ok(false),
            Some(i) => {
                if i.is_used || i.is_revoked {
                    return Ok(false);
                }

                if let Some(expires_at) = i.expires_at {
                    let now = Utc::now().timestamp_millis();
                    if expires_at < now {
                        return Ok(false);
                    }
                }

                let now = Utc::now().timestamp_millis();

                sqlx::query(
                    r#"
                    UPDATE room_invites SET
                        is_used = TRUE,
                        invitee_user_id = $2,
                        used_ts = $3
                    WHERE invite_code = $1
                    "#,
                )
                .bind(invite_code)
                .bind(invitee_user_id)
                .bind(now)
                .execute(&*self.pool)
                .await?;

                Ok(true)
            }
        }
    }

    pub async fn revoke_room_invite(
        &self,
        invite_code: &str,
        reason: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE room_invites SET
                is_revoked = TRUE,
                revoked_ts = $2,
                revoked_reason = $3
            WHERE invite_code = $1
            "#,
        )
        .bind(invite_code)
        .bind(now)
        .bind(reason)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn create_batch(
        &self,
        batch: &RegistrationTokenBatch,
        tokens: &[String],
    ) -> Result<i64, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, RegistrationTokenBatch>(
            r#"
            INSERT INTO registration_token_batches (
                batch_id, description, token_count, created_by, created_ts, expires_at,
                allowed_email_domains, auto_join_rooms
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            "#,
        )
        .bind(&batch.batch_id)
        .bind(&batch.description)
        .bind(batch.token_count)
        .bind(&batch.created_by)
        .bind(now)
        .bind(batch.expires_at)
        .bind(&batch.allowed_email_domains)
        .bind(&batch.auto_join_rooms)
        .fetch_one(&*self.pool)
        .await?;

        for token in tokens {
            sqlx::query(
                r#"
                INSERT INTO registration_tokens (
                    token, token_type, description, max_uses, expires_at, created_by,
                    created_ts, updated_ts
                )
                VALUES ($1, 'single_use', $2, 1, $3, $4, $5, $5)
                "#,
            )
            .bind(token)
            .bind(&batch.description)
            .bind(batch.expires_at)
            .bind(&batch.created_by)
            .bind(now)
            .execute(&*self.pool)
            .await?;
        }

        Ok(row.id)
    }

    pub async fn get_batch(
        &self,
        batch_id: &str,
    ) -> Result<Option<RegistrationTokenBatch>, sqlx::Error> {
        let row = sqlx::query_as::<_, RegistrationTokenBatch>(
            "SELECT * FROM registration_token_batches WHERE batch_id = $1",
        )
        .bind(batch_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_token() -> RegistrationToken {
        RegistrationToken {
            id: 1,
            token: "TestToken123456789".to_string(),
            token_type: "single_use".to_string(),
            description: Some("Test token for unit tests".to_string()),
            max_uses: 1,
            current_uses: 0,
            is_used: false,
            is_enabled: true,
            expires_at: None,
            created_by: Some("@admin:example.com".to_string()),
            created_ts: 1700000000000,
            updated_ts: 1700000000000,
            last_used_ts: None,
            allowed_email_domains: Some(vec!["example.com".to_string()]),
            allowed_user_ids: Some(vec!["@user1:example.com".to_string()]),
            auto_join_rooms: Some(vec!["!room1:example.com".to_string()]),
            display_name: Some("Test User".to_string()),
            email: Some("test@example.com".to_string()),
        }
    }

    #[test]
    fn test_registration_token_creation() {
        let token = create_test_token();

        assert_eq!(token.id, 1);
        assert_eq!(token.token, "TestToken123456789");
        assert_eq!(token.token_type, "single_use");
        assert_eq!(token.description, Some("Test token for unit tests".to_string()));
        assert_eq!(token.max_uses, 1);
        assert_eq!(token.current_uses, 0);
        assert!(!token.is_used);
        assert!(token.is_enabled);
        assert!(token.expires_at.is_none());
        assert_eq!(token.created_by, Some("@admin:example.com".to_string()));
    }

    #[test]
    fn test_registration_token_optional_fields() {
        let token = RegistrationToken {
            id: 2,
            token: "MinimalToken".to_string(),
            token_type: "multi_use".to_string(),
            description: None,
            max_uses: 10,
            current_uses: 5,
            is_used: false,
            is_enabled: true,
            expires_at: Some(1800000000000),
            created_by: None,
            created_ts: 1700000000000,
            updated_ts: 1700000000000,
            last_used_ts: Some(1750000000000),
            allowed_email_domains: None,
            allowed_user_ids: None,
            auto_join_rooms: None,
            display_name: None,
            email: None,
        };

        assert!(token.description.is_none());
        assert!(token.created_by.is_none());
        assert!(token.allowed_email_domains.is_none());
        assert!(token.allowed_user_ids.is_none());
        assert!(token.auto_join_rooms.is_none());
        assert!(token.display_name.is_none());
        assert!(token.email.is_none());
        assert!(token.expires_at.is_some());
        assert!(token.last_used_ts.is_some());
    }

    #[test]
    fn test_token_validation_result_valid() {
        let result = TokenValidationResult {
            is_valid: true,
            token_id: Some(1),
            error_message: None,
        };

        assert!(result.is_valid);
        assert_eq!(result.token_id, Some(1));
        assert!(result.error_message.is_none());
    }

    #[test]
    fn test_token_validation_result_invalid() {
        let result = TokenValidationResult {
            is_valid: false,
            token_id: Some(2),
            error_message: Some("Token has expired".to_string()),
        };

        assert!(!result.is_valid);
        assert_eq!(result.token_id, Some(2));
        assert_eq!(result.error_message, Some("Token has expired".to_string()));
    }

    #[test]
    fn test_token_validation_result_not_found() {
        let result = TokenValidationResult {
            is_valid: false,
            token_id: None,
            error_message: Some("Token not found".to_string()),
        };

        assert!(!result.is_valid);
        assert!(result.token_id.is_none());
        assert_eq!(result.error_message, Some("Token not found".to_string()));
    }

    #[test]
    fn test_create_registration_token_request() {
        let request = CreateRegistrationTokenRequest {
            token: Some("CustomToken123".to_string()),
            token_type: Some("multi_use".to_string()),
            description: Some("Custom token".to_string()),
            max_uses: Some(5),
            expires_at: Some(1800000000000),
            created_by: Some("@admin:example.com".to_string()),
            allowed_email_domains: Some(vec!["test.com".to_string()]),
            allowed_user_ids: Some(vec!["@user:test.com".to_string()]),
            auto_join_rooms: Some(vec!["!room:test.com".to_string()]),
            display_name: Some("Display Name".to_string()),
            email: Some("email@test.com".to_string()),
        };

        assert_eq!(request.token, Some("CustomToken123".to_string()));
        assert_eq!(request.token_type, Some("multi_use".to_string()));
        assert_eq!(request.max_uses, Some(5));
        assert!(request.expires_at.is_some());
    }

    #[test]
    fn test_create_registration_token_request_minimal() {
        let request = CreateRegistrationTokenRequest {
            token: None,
            token_type: None,
            description: None,
            max_uses: None,
            expires_at: None,
            created_by: None,
            allowed_email_domains: None,
            allowed_user_ids: None,
            auto_join_rooms: None,
            display_name: None,
            email: None,
        };

        assert!(request.token.is_none());
        assert!(request.token_type.is_none());
        assert!(request.max_uses.is_none());
        assert!(request.expires_at.is_none());
    }

    #[test]
    fn test_update_registration_token_request() {
        let request = UpdateRegistrationTokenRequest {
            description: Some("Updated description".to_string()),
            max_uses: Some(10),
            is_enabled: Some(false),
            expires_at: Some(1900000000000),
        };

        assert_eq!(request.description, Some("Updated description".to_string()));
        assert_eq!(request.max_uses, Some(10));
        assert_eq!(request.is_enabled, Some(false));
        assert!(request.expires_at.is_some());
    }

    #[test]
    fn test_update_registration_token_request_default() {
        let request = UpdateRegistrationTokenRequest::default();

        assert!(request.description.is_none());
        assert!(request.max_uses.is_none());
        assert!(request.is_enabled.is_none());
        assert!(request.expires_at.is_none());
    }

    #[test]
    fn test_generate_token_length() {
        let token = RegistrationTokenStorage::generate_token();
        assert_eq!(token.len(), 32);
    }

    #[test]
    fn test_generate_token_uniqueness() {
        let token1 = RegistrationTokenStorage::generate_token();
        let token2 = RegistrationTokenStorage::generate_token();
        let token3 = RegistrationTokenStorage::generate_token();

        assert_ne!(token1, token2);
        assert_ne!(token2, token3);
        assert_ne!(token1, token3);
    }

    #[test]
    fn test_generate_token_valid_characters() {
        let token = RegistrationTokenStorage::generate_token();
        let valid_chars: std::collections::HashSet<char> = 
            "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789"
                .chars()
                .collect();

        for c in token.chars() {
            assert!(valid_chars.contains(&c), "Invalid character: {}", c);
        }
    }

    #[test]
    fn test_generate_token_no_ambiguous_chars() {
        let token = RegistrationTokenStorage::generate_token();
        let ambiguous_chars = ['I', 'O', 'l', 'i', 'o', '0', '1'];

        for c in token.chars() {
            assert!(!ambiguous_chars.contains(&c), "Ambiguous character found: {}", c);
        }
    }

    #[test]
    fn test_registration_token_expiry_logic() {
        let now = chrono::Utc::now().timestamp_millis();

        let valid_token = RegistrationToken {
            id: 1,
            token: "ValidToken".to_string(),
            token_type: "single_use".to_string(),
            description: None,
            max_uses: 1,
            current_uses: 0,
            is_used: false,
            is_enabled: true,
            expires_at: Some(now + 86400000),
            created_by: None,
            created_ts: now,
            updated_ts: now,
            last_used_ts: None,
            allowed_email_domains: None,
            allowed_user_ids: None,
            auto_join_rooms: None,
            display_name: None,
            email: None,
        };

        let expired_token = RegistrationToken {
            id: 2,
            token: "ExpiredToken".to_string(),
            token_type: "single_use".to_string(),
            description: None,
            max_uses: 1,
            current_uses: 0,
            is_used: false,
            is_enabled: true,
            expires_at: Some(now - 86400000),
            created_by: None,
            created_ts: now - 172800000,
            updated_ts: now - 172800000,
            last_used_ts: None,
            allowed_email_domains: None,
            allowed_user_ids: None,
            auto_join_rooms: None,
            display_name: None,
            email: None,
        };

        assert!(valid_token.expires_at.unwrap() > now);
        assert!(expired_token.expires_at.unwrap() < now);
    }

    #[test]
    fn test_registration_token_usage_limit() {
        let unlimited_token = RegistrationToken {
            id: 1,
            token: "UnlimitedToken".to_string(),
            token_type: "multi_use".to_string(),
            description: None,
            max_uses: 0,
            current_uses: 100,
            is_used: false,
            is_enabled: true,
            expires_at: None,
            created_by: None,
            created_ts: 1700000000000,
            updated_ts: 1700000000000,
            last_used_ts: None,
            allowed_email_domains: None,
            allowed_user_ids: None,
            auto_join_rooms: None,
            display_name: None,
            email: None,
        };

        let limited_token_available = RegistrationToken {
            id: 2,
            token: "LimitedTokenAvailable".to_string(),
            token_type: "multi_use".to_string(),
            description: None,
            max_uses: 10,
            current_uses: 5,
            is_used: false,
            is_enabled: true,
            expires_at: None,
            created_by: None,
            created_ts: 1700000000000,
            updated_ts: 1700000000000,
            last_used_ts: None,
            allowed_email_domains: None,
            allowed_user_ids: None,
            auto_join_rooms: None,
            display_name: None,
            email: None,
        };

        let limited_token_exhausted = RegistrationToken {
            id: 3,
            token: "LimitedTokenExhausted".to_string(),
            token_type: "multi_use".to_string(),
            description: None,
            max_uses: 10,
            current_uses: 10,
            is_used: false,
            is_enabled: true,
            expires_at: None,
            created_by: None,
            created_ts: 1700000000000,
            updated_ts: 1700000000000,
            last_used_ts: None,
            allowed_email_domains: None,
            allowed_user_ids: None,
            auto_join_rooms: None,
            display_name: None,
            email: None,
        };

        assert!(unlimited_token.max_uses == 0 || unlimited_token.current_uses < unlimited_token.max_uses);
        assert!(limited_token_available.current_uses < limited_token_available.max_uses);
        assert!(limited_token_exhausted.current_uses >= limited_token_exhausted.max_uses);
    }

    #[test]
    fn test_registration_token_disabled() {
        let disabled_token = RegistrationToken {
            id: 1,
            token: "DisabledToken".to_string(),
            token_type: "single_use".to_string(),
            description: None,
            max_uses: 1,
            current_uses: 0,
            is_used: false,
            is_enabled: false,
            expires_at: None,
            created_by: None,
            created_ts: 1700000000000,
            updated_ts: 1700000000000,
            last_used_ts: None,
            allowed_email_domains: None,
            allowed_user_ids: None,
            auto_join_rooms: None,
            display_name: None,
            email: None,
        };

        assert!(!disabled_token.is_enabled);
        assert!(!disabled_token.is_used);
    }

    #[test]
    fn test_registration_token_single_use_used() {
        let used_token = RegistrationToken {
            id: 1,
            token: "UsedToken".to_string(),
            token_type: "single_use".to_string(),
            description: None,
            max_uses: 1,
            current_uses: 1,
            is_used: true,
            is_enabled: true,
            expires_at: None,
            created_by: None,
            created_ts: 1700000000000,
            updated_ts: 1700000000000,
            last_used_ts: Some(1700000100000),
            allowed_email_domains: None,
            allowed_user_ids: None,
            auto_join_rooms: None,
            display_name: None,
            email: None,
        };

        assert!(used_token.is_used);
        assert_eq!(used_token.token_type, "single_use");
        assert_eq!(used_token.current_uses, used_token.max_uses);
    }

    #[test]
    fn test_room_invite_creation() {
        let invite = RoomInvite {
            id: 1,
            invite_code: "InviteCode123".to_string(),
            room_id: "!room:example.com".to_string(),
            inviter_user_id: "@admin:example.com".to_string(),
            invitee_email: Some("guest@example.com".to_string()),
            invitee_user_id: None,
            is_used: false,
            is_revoked: false,
            expires_at: Some(1800000000000),
            created_ts: 1700000000000,
            used_ts: None,
            revoked_ts: None,
            revoked_reason: None,
        };

        assert_eq!(invite.invite_code, "InviteCode123");
        assert_eq!(invite.room_id, "!room:example.com");
        assert!(!invite.is_used);
        assert!(!invite.is_revoked);
        assert!(invite.invitee_user_id.is_none());
    }

    #[test]
    fn test_room_invite_revoked() {
        let revoked_invite = RoomInvite {
            id: 1,
            invite_code: "RevokedCode".to_string(),
            room_id: "!room:example.com".to_string(),
            inviter_user_id: "@admin:example.com".to_string(),
            invitee_email: None,
            invitee_user_id: None,
            is_used: false,
            is_revoked: true,
            expires_at: None,
            created_ts: 1700000000000,
            used_ts: None,
            revoked_ts: Some(1700000100000),
            revoked_reason: Some("No longer needed".to_string()),
        };

        assert!(revoked_invite.is_revoked);
        assert!(revoked_invite.revoked_ts.is_some());
        assert_eq!(revoked_invite.revoked_reason, Some("No longer needed".to_string()));
    }

    #[test]
    fn test_registration_token_batch() {
        let batch = RegistrationTokenBatch {
            id: 1,
            batch_id: "batch-uuid-123".to_string(),
            description: Some("Batch for testing".to_string()),
            token_count: 10,
            tokens_used: 3,
            created_by: Some("@admin:example.com".to_string()),
            created_ts: 1700000000000,
            expires_at: Some(1800000000000),
            is_enabled: true,
            allowed_email_domains: Some(vec!["test.com".to_string()]),
            auto_join_rooms: Some(vec!["!room:test.com".to_string()]),
        };

        assert_eq!(batch.batch_id, "batch-uuid-123");
        assert_eq!(batch.token_count, 10);
        assert_eq!(batch.tokens_used, 3);
        assert!(batch.is_enabled);
    }

    #[test]
    fn test_registration_token_usage() {
        let usage = RegistrationTokenUsage {
            id: 1,
            token_id: 100,
            token: "UsedToken123".to_string(),
            user_id: "@user:example.com".to_string(),
            username: Some("testuser".to_string()),
            email: Some("user@example.com".to_string()),
            ip_address: Some("192.168.1.1".to_string()),
            user_agent: Some("Mozilla/5.0".to_string()),
            used_ts: 1700000000000,
            success: true,
            error_message: None,
        };

        assert_eq!(usage.token_id, 100);
        assert_eq!(usage.user_id, "@user:example.com");
        assert!(usage.success);
        assert!(usage.error_message.is_none());
    }

    #[test]
    fn test_registration_token_usage_failed() {
        let failed_usage = RegistrationTokenUsage {
            id: 2,
            token_id: 100,
            token: "FailedToken".to_string(),
            user_id: "@user:example.com".to_string(),
            username: None,
            email: None,
            ip_address: Some("192.168.1.2".to_string()),
            user_agent: None,
            used_ts: 1700000000000,
            success: false,
            error_message: Some("Token expired".to_string()),
        };

        assert!(!failed_usage.success);
        assert_eq!(failed_usage.error_message, Some("Token expired".to_string()));
    }

    #[test]
    fn test_create_room_invite_request() {
        let request = CreateRoomInviteRequest {
            room_id: "!room:example.com".to_string(),
            inviter_user_id: "@admin:example.com".to_string(),
            invitee_email: Some("guest@example.com".to_string()),
            expires_at: Some(1800000000000),
        };

        assert_eq!(request.room_id, "!room:example.com");
        assert_eq!(request.inviter_user_id, "@admin:example.com");
        assert!(request.invitee_email.is_some());
    }

    #[test]
    fn test_token_serialization() {
        let token = create_test_token();
        let json = serde_json::to_string(&token).expect("Failed to serialize");
        let deserialized: RegistrationToken = 
            serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(token.id, deserialized.id);
        assert_eq!(token.token, deserialized.token);
        assert_eq!(token.token_type, deserialized.token_type);
        assert_eq!(token.max_uses, deserialized.max_uses);
    }

    #[test]
    fn test_token_validation_result_serialization() {
        let result = TokenValidationResult {
            is_valid: true,
            token_id: Some(42),
            error_message: None,
        };

        let json = serde_json::to_string(&result).expect("Failed to serialize");
        let deserialized: TokenValidationResult = 
            serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(result.is_valid, deserialized.is_valid);
        assert_eq!(result.token_id, deserialized.token_id);
    }
}
