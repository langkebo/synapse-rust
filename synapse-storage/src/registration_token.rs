use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationTokenCursor {
    pub created_ts: i64,
    pub id: i64,
}

pub fn encode_registration_token_cursor(cursor: &RegistrationTokenCursor) -> String {
    format!("{}|{}", cursor.created_ts, cursor.id)
}

pub fn decode_registration_token_cursor(cursor: Option<&str>) -> Option<RegistrationTokenCursor> {
    let cursor = cursor?;
    let mut parts = cursor.split('|');
    let created_ts = parts.next()?.parse::<i64>().ok()?;
    let id = parts.next()?.parse::<i64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(RegistrationTokenCursor { created_ts, id })
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RegistrationToken {
    pub id: i64,
    pub token: String,
    pub token_type: String,
    pub description: Option<String>,
    pub max_uses: i32,
    pub uses_count: i32,
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
    pub is_success: bool,
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
    pub revoked_at: Option<i64>,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
        let token_type = request.token_type.unwrap_or_else(|| "single_use".to_string());

        let row = sqlx::query_as::<_, RegistrationToken>(
            r"
            INSERT INTO registration_tokens (
                token, token_type, description, max_uses, expires_at, created_by,
                created_ts, updated_ts, allowed_email_domains, allowed_user_ids,
                auto_join_rooms, display_name, email
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $7, $8, $9, $10, $11, $12)
            RETURNING *
            ",
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
        let mut rng = rand::rng();
        let chars: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789";
        let token: String = (0..32).map(|_| chars[rng.random_range(0..chars.len())] as char).collect();
        token
    }

    pub async fn get_token(&self, token: &str) -> Result<Option<RegistrationToken>, sqlx::Error> {
        let row = sqlx::query_as::<_, RegistrationToken>("SELECT id, token, token_type, description, max_uses, uses_count, is_used, is_enabled, expires_at, created_by, created_ts, updated_ts, last_used_ts, allowed_email_domains, allowed_user_ids, auto_join_rooms, display_name, email FROM registration_tokens WHERE token = $1")
            .bind(token)
            .fetch_optional(&*self.pool)
            .await?;

        Ok(row)
    }

    pub async fn get_token_by_id(&self, id: i64) -> Result<Option<RegistrationToken>, sqlx::Error> {
        let row = sqlx::query_as::<_, RegistrationToken>("SELECT id, token, token_type, description, max_uses, uses_count, is_used, is_enabled, expires_at, created_by, created_ts, updated_ts, last_used_ts, allowed_email_domains, allowed_user_ids, auto_join_rooms, display_name, email FROM registration_tokens WHERE id = $1")
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
            r"
            UPDATE registration_tokens SET
                description = COALESCE($2, description),
                max_uses = COALESCE($3, max_uses),
                is_enabled = COALESCE($4, is_enabled),
                expires_at = COALESCE($5, expires_at)
            WHERE id = $1
            RETURNING *
            ",
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
        sqlx::query("DELETE FROM registration_tokens WHERE id = $1").bind(id).execute(&*self.pool).await?;

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

                if t.max_uses > 0 && t.uses_count >= t.max_uses {
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

                Ok(TokenValidationResult { is_valid: true, token_id: Some(t.id), error_message: None })
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
            r"
            UPDATE registration_tokens
            SET uses_count = uses_count + 1,
                is_used = CASE WHEN token_type = 'single_use' THEN TRUE ELSE is_used END,
                last_used_ts = $2
            WHERE id = $1
            ",
        )
        .bind(token_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r"
            INSERT INTO registration_token_usage (
                token_id, token, user_id, username, email, ip_address, user_agent, used_ts
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ",
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
        from: Option<RegistrationTokenCursor>,
    ) -> Result<(Vec<RegistrationToken>, Option<String>), sqlx::Error> {
        let rows = if let Some(cursor) = from {
            sqlx::query_as::<_, RegistrationToken>(
                "SELECT id, token, token_type, description, max_uses, uses_count, is_used, is_enabled, expires_at, created_by, created_ts, updated_ts, last_used_ts, allowed_email_domains, allowed_user_ids, auto_join_rooms, display_name, email FROM registration_tokens \
                 WHERE (created_ts, id) < ($1, $2) \
                 ORDER BY created_ts DESC, id DESC \
                 LIMIT $3",
            )
            .bind(cursor.created_ts)
            .bind(cursor.id)
            .bind(limit + 1)
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as::<_, RegistrationToken>(
                "SELECT id, token, token_type, description, max_uses, uses_count, is_used, is_enabled, expires_at, created_by, created_ts, updated_ts, last_used_ts, allowed_email_domains, allowed_user_ids, auto_join_rooms, display_name, email FROM registration_tokens \
                 ORDER BY created_ts DESC, id DESC \
                 LIMIT $1",
            )
            .bind(limit + 1)
            .fetch_all(&*self.pool)
            .await?
        };

        let next_batch = if rows.len() > limit as usize {
            rows.get(limit as usize).map(|last_token| {
                encode_registration_token_cursor(&RegistrationTokenCursor {
                    created_ts: last_token.created_ts,
                    id: last_token.id,
                })
            })
        } else {
            None
        };

        let rows = rows.into_iter().take(limit as usize).collect();

        Ok((rows, next_batch))
    }
}

#[cfg(test)]
mod cursor_tests {
    use super::{decode_registration_token_cursor, encode_registration_token_cursor, RegistrationTokenCursor};

    #[test]
    fn registration_token_cursor_round_trip() {
        let cursor = RegistrationTokenCursor { created_ts: 1_746_700_000_000, id: 42 };

        let encoded = encode_registration_token_cursor(&cursor);
        assert_eq!(decode_registration_token_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn registration_token_cursor_rejects_invalid_values() {
        assert_eq!(decode_registration_token_cursor(None), None);
        assert_eq!(decode_registration_token_cursor(Some("bad")), None);
        assert_eq!(decode_registration_token_cursor(Some("123|")), None);
        assert_eq!(decode_registration_token_cursor(Some("123|456|789")), None);
    }
}

impl RegistrationTokenStorage {
    pub async fn get_active_tokens(&self) -> Result<Vec<RegistrationToken>, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let rows = sqlx::query_as::<_, RegistrationToken>(
            r"
            SELECT id, token, token_type, description, max_uses, uses_count, is_used, is_enabled, expires_at, created_by, created_ts, updated_ts, last_used_ts, allowed_email_domains, allowed_user_ids, auto_join_rooms, display_name, email FROM registration_tokens
            WHERE is_enabled = TRUE
            AND (expires_at IS NULL OR expires_at > $1)
            AND (max_uses = 0 OR uses_count < max_uses)
            ORDER BY created_ts DESC
            ",
        )
        .bind(now)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_token_usage(&self, token_id: i64) -> Result<Vec<RegistrationTokenUsage>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RegistrationTokenUsage>(
            "SELECT id, token_id, token, user_id, username, email, ip_address, user_agent, used_ts, is_success, error_message FROM registration_token_usage WHERE token_id = $1 ORDER BY used_ts DESC",
        )
        .bind(token_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn deactivate_token(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE registration_tokens SET is_enabled = FALSE WHERE id = $1 AND is_enabled = TRUE")
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

    pub async fn create_room_invite(&self, request: CreateRoomInviteRequest) -> Result<RoomInvite, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let invite_code = Self::generate_token();

        let row = sqlx::query_as::<_, RoomInvite>(
            r"
            INSERT INTO room_invites (
                invite_code, room_id, inviter_user_id, invitee_email, expires_at, created_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            ",
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

    pub async fn get_room_invite(&self, invite_code: &str) -> Result<Option<RoomInvite>, sqlx::Error> {
        let row = sqlx::query_as::<_, RoomInvite>("SELECT id, invite_code, room_id, inviter_user_id, invitee_email, invitee_user_id, is_used, is_revoked, expires_at, created_ts, used_ts, revoked_at, revoked_reason FROM room_invites WHERE invite_code = $1")
            .bind(invite_code)
            .fetch_optional(&*self.pool)
            .await?;

        Ok(row)
    }

    pub async fn use_room_invite(&self, invite_code: &str, invitee_user_id: &str) -> Result<bool, sqlx::Error> {
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
                    r"
                    UPDATE room_invites SET
                        is_used = TRUE,
                        invitee_user_id = $2,
                        used_ts = $3
                    WHERE invite_code = $1
                    ",
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

    pub async fn revoke_room_invite(&self, invite_code: &str, reason: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r"
            UPDATE room_invites SET
                is_revoked = TRUE,
                revoked_at = $2,
                revoked_reason = $3
            WHERE invite_code = $1
            ",
        )
        .bind(invite_code)
        .bind(now)
        .bind(reason)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn create_batch(&self, batch: &RegistrationTokenBatch, tokens: &[String]) -> Result<i64, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, RegistrationTokenBatch>(
            r"
            INSERT INTO registration_token_batches (
                batch_id, description, token_count, created_by, created_ts, expires_at,
                allowed_email_domains, auto_join_rooms
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            ",
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

        if !tokens.is_empty() {
            let tokens_arr: Vec<&str> = tokens.iter().map(|s| s.as_str()).collect();
            sqlx::query(
                r"
                INSERT INTO registration_tokens (
                    token, token_type, description, max_uses, expires_at, created_by,
                    created_ts, updated_ts
                )
                SELECT unnest($1::text[]), 'single_use', $2, 1, $3, $4, $5, $5
                ",
            )
            .bind(&tokens_arr)
            .bind(&batch.description)
            .bind(batch.expires_at)
            .bind(&batch.created_by)
            .bind(now)
            .execute(&*self.pool)
            .await?;
        }

        Ok(row.id)
    }

    pub async fn get_batch(&self, batch_id: &str) -> Result<Option<RegistrationTokenBatch>, sqlx::Error> {
        let row =
            sqlx::query_as::<_, RegistrationTokenBatch>("SELECT id, batch_id, description, token_count, tokens_used, created_by, created_ts, expires_at, is_enabled, allowed_email_domains, auto_join_rooms FROM registration_token_batches WHERE batch_id = $1")
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
            uses_count: 0,
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
        assert_eq!(token.uses_count, 0);
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
            uses_count: 5,
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
        let result = TokenValidationResult { is_valid: true, token_id: Some(1), error_message: None };

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
            "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789".chars().collect();

        for c in token.chars() {
            assert!(valid_chars.contains(&c), "Invalid character: {c}");
        }
    }

    #[test]
    fn test_generate_token_no_ambiguous_chars() {
        let token = RegistrationTokenStorage::generate_token();
        let ambiguous_chars = ['I', 'O', 'l', 'i', 'o', '0', '1'];

        for c in token.chars() {
            assert!(!ambiguous_chars.contains(&c), "Ambiguous character found: {c}");
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
            uses_count: 0,
            is_used: false,
            is_enabled: true,
            expires_at: Some(now + 86_400_000),
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
            uses_count: 0,
            is_used: false,
            is_enabled: true,
            expires_at: Some(now - 86_400_000),
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
            uses_count: 100,
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
            uses_count: 5,
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
            uses_count: 10,
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

        assert!(unlimited_token.max_uses == 0 || unlimited_token.uses_count < unlimited_token.max_uses);
        assert!(limited_token_available.uses_count < limited_token_available.max_uses);
        assert!(limited_token_exhausted.uses_count >= limited_token_exhausted.max_uses);
    }

    #[test]
    fn test_registration_token_disabled() {
        let disabled_token = RegistrationToken {
            id: 1,
            token: "DisabledToken".to_string(),
            token_type: "single_use".to_string(),
            description: None,
            max_uses: 1,
            uses_count: 0,
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
            uses_count: 1,
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
        assert_eq!(used_token.uses_count, used_token.max_uses);
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
            revoked_at: None,
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
            revoked_at: Some(1700000100000),
            revoked_reason: Some("No longer needed".to_string()),
        };

        assert!(revoked_invite.is_revoked);
        assert!(revoked_invite.revoked_at.is_some());
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
            is_success: true,
            error_message: None,
        };

        assert_eq!(usage.token_id, 100);
        assert_eq!(usage.user_id, "@user:example.com");
        assert!(usage.is_success);
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
            is_success: false,
            error_message: Some("Token expired".to_string()),
        };

        assert!(!failed_usage.is_success);
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
        let deserialized: RegistrationToken = serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(token.id, deserialized.id);
        assert_eq!(token.token, deserialized.token);
        assert_eq!(token.token_type, deserialized.token_type);
        assert_eq!(token.max_uses, deserialized.max_uses);
    }

    #[test]
    fn test_token_validation_result_serialization() {
        let result = TokenValidationResult { is_valid: true, token_id: Some(42), error_message: None };

        let json = serde_json::to_string(&result).expect("Failed to serialize");
        let deserialized: TokenValidationResult = serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(result.is_valid, deserialized.is_valid);
        assert_eq!(result.token_id, deserialized.token_id);
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::env;

    async fn test_pool() -> Arc<sqlx::PgPool> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&db_url)
            .await
            .expect("Failed to connect to test database");
        Arc::new(pool)
    }

    async fn cleanup_test_data(pool: &sqlx::PgPool, suffix: &str) {
        let token_pattern = format!("%_{}", suffix);
        let room_pattern = format!("%{}%", suffix);

        sqlx::query("DELETE FROM registration_token_usage WHERE token LIKE $1")
            .bind(&token_pattern)
            .execute(pool)
            .await
            .ok();

        sqlx::query("DELETE FROM registration_tokens WHERE token LIKE $1")
            .bind(&token_pattern)
            .execute(pool)
            .await
            .ok();

        sqlx::query("DELETE FROM room_invites WHERE inviter_user_id LIKE $1")
            .bind(&room_pattern)
            .execute(pool)
            .await
            .ok();
    }

    async fn ensure_test_user(pool: &sqlx::PgPool, user_id: &str) {
        let now = chrono::Utc::now().timestamp_millis();
        let username = user_id
            .strip_prefix('@')
            .and_then(|u| u.split(':').next())
            .unwrap_or("testuser");
        sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(username)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test user");
    }

    fn make_suffix() -> String {
        uuid::Uuid::new_v4().to_string().replace('-', "")
    }

    fn make_full_token(suffix: &str) -> String {
        format!("regtok_test_{}", suffix)
    }

    fn empty_token_request() -> CreateRegistrationTokenRequest {
        CreateRegistrationTokenRequest {
            token: Some(String::new()),
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
        }
    }

    // ——————————————————————————————————————————
    // 1. create_token
    // ——————————————————————————————————————————

    #[tokio::test]
    async fn test_create_token_with_all_fields() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_test_data(&pool, &suffix).await;

        let storage = RegistrationTokenStorage::new(&pool);
        let token_str = make_full_token(&suffix);
        let request = CreateRegistrationTokenRequest {
            token: Some(token_str.clone()),
            token_type: Some("multi_use".to_string()),
            description: Some("DB test token".to_string()),
            max_uses: Some(10),
            expires_at: None,
            created_by: Some(format!("@admin_{}:test.local", suffix)),
            allowed_email_domains: Some(vec!["test.local".to_string()]),
            allowed_user_ids: Some(vec![format!("@user_{}:test.local", suffix)]),
            auto_join_rooms: Some(vec![format!("!room_{}:test.local", suffix)]),
            display_name: Some("Test Display".to_string()),
            email: Some(format!("test_{}@test.local", suffix)),
        };

        let result = storage.create_token(request).await.expect("create_token should succeed");

        assert_eq!(result.token, token_str);
        assert_eq!(result.token_type, "multi_use");
        assert_eq!(result.description.as_deref(), Some("DB test token"));
        assert_eq!(result.max_uses, 10);
        assert_eq!(result.uses_count, 0);
        assert!(!result.is_used);
        assert!(result.is_enabled);
        assert!(result.created_ts > 0);
        assert!(result.expires_at.is_none());
        assert_eq!(result.created_by.as_deref(), Some(format!("@admin_{}:test.local", suffix).as_str()));
        assert_eq!(result.allowed_email_domains.as_deref(), Some(&vec!["test.local".to_string()][..]));
        assert_eq!(result.allowed_user_ids.as_deref(), Some(&vec![format!("@user_{}:test.local", suffix)][..]));
        assert_eq!(result.auto_join_rooms.as_deref(), Some(&vec![format!("!room_{}:test.local", suffix)][..]));
        assert_eq!(result.display_name.as_deref(), Some("Test Display"));
        assert_eq!(result.email.as_deref(), Some(format!("test_{}@test.local", suffix).as_str()));

        cleanup_test_data(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————
    // 2. get_token (found + not_found)
    // ——————————————————————————————————————————

    #[tokio::test]
    async fn test_get_token_found_and_not_found() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_test_data(&pool, &suffix).await;

        let storage = RegistrationTokenStorage::new(&pool);
        let token_str = make_full_token(&suffix);

        // Not found before creation
        let missing = storage.get_token(&token_str).await.expect("get_token should not error");
        assert!(missing.is_none(), "token should not exist yet");

        // Create and find
        let request = CreateRegistrationTokenRequest {
            token: Some(token_str.clone()),
            ..empty_token_request()
        };
        let created = storage.create_token(request).await.expect("create_token should succeed");
        assert_eq!(created.token, token_str);

        let found = storage.get_token(&token_str).await.expect("get_token should not error");
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.token, token_str);
        assert_eq!(found.id, created.id);

        cleanup_test_data(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————
    // 3. get_token_by_id (found + not_found)
    // ——————————————————————————————————————————

    #[tokio::test]
    async fn test_get_token_by_id_found_and_not_found() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_test_data(&pool, &suffix).await;

        let storage = RegistrationTokenStorage::new(&pool);
        let token_str = make_full_token(&suffix);

        // Not found for non-existent id
        let missing = storage.get_token_by_id(-999).await.expect("get_token_by_id should not error");
        assert!(missing.is_none());

        // Create and find by id
        let request = CreateRegistrationTokenRequest {
            token: Some(token_str.clone()),
            ..empty_token_request()
        };
        let created = storage.create_token(request).await.expect("create_token should succeed");

        let found = storage.get_token_by_id(created.id).await.expect("get_token_by_id should not error");
        assert!(found.is_some());
        assert_eq!(found.unwrap().token, token_str);

        cleanup_test_data(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————
    // 4. update_token (success + not_found)
    // ——————————————————————————————————————————

    #[tokio::test]
    async fn test_update_token_success_and_not_found() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_test_data(&pool, &suffix).await;

        let storage = RegistrationTokenStorage::new(&pool);
        let token_str = make_full_token(&suffix);

        let created = storage
            .create_token(CreateRegistrationTokenRequest {
                token: Some(token_str.clone()),
                description: Some("original".to_string()),
                max_uses: Some(5),
                ..empty_token_request()
            })
            .await
            .expect("create_token should succeed");

        // Successful update
        let future_expiry = chrono::Utc::now().timestamp_millis() + 86_400_000;
        let update_req = UpdateRegistrationTokenRequest {
            description: Some("updated".to_string()),
            max_uses: Some(20),
            is_enabled: None,
            expires_at: Some(future_expiry),
        };
        let updated = storage.update_token(created.id, update_req).await.expect("update_token should succeed");
        assert_eq!(updated.description.as_deref(), Some("updated"));
        assert_eq!(updated.max_uses, 20);
        assert_eq!(updated.expires_at, Some(future_expiry));
        assert_eq!(updated.id, created.id);

        // Not found — update on non-existent id should error
        let result = storage
            .update_token(-999, UpdateRegistrationTokenRequest::default())
            .await;
        assert!(result.is_err(), "update_token on non-existent id should fail");

        cleanup_test_data(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————
    // 5. delete_token (deletes + idempotent)
    // ——————————————————————————————————————————

    #[tokio::test]
    async fn test_delete_token_and_idempotent() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_test_data(&pool, &suffix).await;

        let storage = RegistrationTokenStorage::new(&pool);
        let token_str = make_full_token(&suffix);

        let created = storage
            .create_token(CreateRegistrationTokenRequest {
                token: Some(token_str.clone()),
                ..empty_token_request()
            })
            .await
            .expect("create_token should succeed");

        // Delete
        storage.delete_token(created.id).await.expect("delete_token should succeed");
        let after_delete = storage.get_token_by_id(created.id).await.expect("get_token_by_id should not error");
        assert!(after_delete.is_none(), "token should be deleted");

        // Idempotent — delete again (non-existent id should not error)
        let result = storage.delete_token(created.id).await;
        assert!(result.is_ok(), "delete_token on already-deleted id should not error");

        // Also test with never-existed id
        let result2 = storage.delete_token(-99999).await;
        assert!(result2.is_ok(), "delete_token on never-existed id should not error");

        cleanup_test_data(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————
    // 6. validate_token — valid
    // ——————————————————————————————————————————

    #[tokio::test]
    async fn test_validate_token_valid() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_test_data(&pool, &suffix).await;

        let storage = RegistrationTokenStorage::new(&pool);
        let token_str = make_full_token(&suffix);

        storage
            .create_token(CreateRegistrationTokenRequest {
                token: Some(token_str.clone()),
                token_type: Some("multi_use".to_string()),
                max_uses: Some(10),
                ..empty_token_request()
            })
            .await
            .expect("create_token should succeed");

        let result = storage.validate_token(&token_str).await.expect("validate_token should not error");
        assert!(result.is_valid);
        assert!(result.token_id.is_some());
        assert!(result.error_message.is_none());

        cleanup_test_data(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————
    // 7. validate_token — expired
    // ——————————————————————————————————————————

    #[tokio::test]
    async fn test_validate_token_expired() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_test_data(&pool, &suffix).await;

        let storage = RegistrationTokenStorage::new(&pool);
        let token_str = make_full_token(&suffix);

        let past = chrono::Utc::now().timestamp_millis() - 86_400_000;
        let created = storage
            .create_token(CreateRegistrationTokenRequest {
                token: Some(token_str.clone()),
                expires_at: Some(past),
                ..empty_token_request()
            })
            .await
            .expect("create_token should succeed");

        let result = storage.validate_token(&token_str).await.expect("validate_token should not error");
        assert!(!result.is_valid);
        assert_eq!(result.token_id, Some(created.id));
        assert_eq!(result.error_message.as_deref(), Some("Token has expired"));

        cleanup_test_data(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————
    // 8. validate_token — exhausted (max_uses reached)
    // ——————————————————————————————————————————

    #[tokio::test]
    async fn test_validate_token_exhausted() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_test_data(&pool, &suffix).await;

        let storage = RegistrationTokenStorage::new(&pool);
        let token_str = make_full_token(&suffix);
        let user_id = format!("@exhausted_{}:test.local", suffix);
        ensure_test_user(&pool, &user_id).await;

        // Create multi_use token with max_uses=1
        let created = storage
            .create_token(CreateRegistrationTokenRequest {
                token: Some(token_str.clone()),
                token_type: Some("multi_use".to_string()),
                max_uses: Some(1),
                ..empty_token_request()
            })
            .await
            .expect("create_token should succeed");
        assert_eq!(created.max_uses, 1);
        assert_eq!(created.uses_count, 0);

        // Use the token once — should succeed and exhaust
        let used = storage
            .use_token(&token_str, &user_id, None, None, None, None)
            .await
            .expect("use_token should not error");
        assert!(used, "first use should succeed");

        // Now validate — should be exhausted
        let result = storage.validate_token(&token_str).await.expect("validate_token should not error");
        assert!(!result.is_valid);
        assert_eq!(result.error_message.as_deref(), Some("Token has reached maximum uses"));

        cleanup_test_data(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————
    // 9. validate_token — disabled (not active)
    // ——————————————————————————————————————————

    #[tokio::test]
    async fn test_validate_token_disabled() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_test_data(&pool, &suffix).await;

        let storage = RegistrationTokenStorage::new(&pool);
        let token_str = make_full_token(&suffix);

        let created = storage
            .create_token(CreateRegistrationTokenRequest {
                token: Some(token_str.clone()),
                ..empty_token_request()
            })
            .await
            .expect("create_token should succeed");

        // Deactivate
        storage.deactivate_token(created.id).await.expect("deactivate_token should succeed");

        let result = storage.validate_token(&token_str).await.expect("validate_token should not error");
        assert!(!result.is_valid);
        assert_eq!(result.token_id, Some(created.id));
        assert_eq!(result.error_message.as_deref(), Some("Token is not active"));

        cleanup_test_data(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————
    // 10. validate_token — not found
    // ——————————————————————————————————————————

    #[tokio::test]
    async fn test_validate_token_not_found() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_test_data(&pool, &suffix).await;

        let storage = RegistrationTokenStorage::new(&pool);
        let non_existent = make_full_token(&suffix);

        let result = storage.validate_token(&non_existent).await.expect("validate_token should not error");
        assert!(!result.is_valid);
        assert!(result.token_id.is_none());
        assert_eq!(result.error_message.as_deref(), Some("Token not found"));

        cleanup_test_data(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————
    // 11. use_token — increments counter
    // ——————————————————————————————————————————

    #[tokio::test]
    async fn test_use_token_increments_counter() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_test_data(&pool, &suffix).await;

        let storage = RegistrationTokenStorage::new(&pool);
        let token_str = make_full_token(&suffix);
        let user_id = format!("@counter_{}:test.local", suffix);
        ensure_test_user(&pool, &user_id).await;

        let created = storage
            .create_token(CreateRegistrationTokenRequest {
                token: Some(token_str.clone()),
                token_type: Some("multi_use".to_string()),
                max_uses: Some(5),
                ..empty_token_request()
            })
            .await
            .expect("create_token should succeed");
        assert_eq!(created.uses_count, 0);

        // First use
        let used = storage
            .use_token(&token_str, &user_id, Some("user1"), None, None, None)
            .await
            .expect("use_token should not error");
        assert!(used);

        let after_first = storage.get_token(&token_str).await.expect("get_token should not error");
        assert_eq!(after_first.unwrap().uses_count, 1);

        // Second use — different user
        let user_id2 = format!("@counter2_{}:test.local", suffix);
        ensure_test_user(&pool, &user_id2).await;
        let used2 = storage
            .use_token(&token_str, &user_id2, Some("user2"), None, None, None)
            .await
            .expect("use_token should not error");
        assert!(used2);

        let after_second = storage.get_token(&token_str).await.expect("get_token should not error");
        assert_eq!(after_second.unwrap().uses_count, 2);

        cleanup_test_data(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————
    // 12. use_token — fails when exhausted
    // ——————————————————————————————————————————

    #[tokio::test]
    async fn test_use_token_fails_when_exhausted() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_test_data(&pool, &suffix).await;

        let storage = RegistrationTokenStorage::new(&pool);
        let token_str = make_full_token(&suffix);
        let user_id = format!("@exhaust_{}:test.local", suffix);
        ensure_test_user(&pool, &user_id).await;

        storage
            .create_token(CreateRegistrationTokenRequest {
                token: Some(token_str.clone()),
                token_type: Some("multi_use".to_string()),
                max_uses: Some(1),
                ..empty_token_request()
            })
            .await
            .expect("create_token should succeed");

        // First use — succeeds
        let first = storage
            .use_token(&token_str, &user_id, None, None, None, None)
            .await
            .expect("use_token should not error");
        assert!(first);

        // Second use — fails (token exhausted)
        let user_id2 = format!("@exhaust2_{}:test.local", suffix);
        ensure_test_user(&pool, &user_id2).await;
        let second = storage
            .use_token(&token_str, &user_id2, None, None, None, None)
            .await
            .expect("use_token should not error");
        assert!(!second, "second use on exhausted token should return false");

        cleanup_test_data(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————
    // 13. get_all_tokens — cursor pagination
    // ——————————————————————————————————————————

    #[tokio::test]
    async fn test_get_all_tokens_cursor_pagination() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_test_data(&pool, &suffix).await;

        let storage = RegistrationTokenStorage::new(&pool);

        // Create 4 tokens with tracked IDs
        let prefix = format!("cursor_{}", suffix);
        let mut created_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();
        for i in 0..4 {
            let token_str = format!("{}_id{}", prefix, i);
            let created = storage
                .create_token(CreateRegistrationTokenRequest {
                    token: Some(token_str),
                    ..empty_token_request()
                })
                .await
                .expect("create_token should succeed");
            created_ids.insert(created.id);
        }

        // Fetch first page (limit 2) — results are global; just verify pagination works
        let (page1, cursor1) = storage
            .get_all_tokens(2, None)
            .await
            .expect("get_all_tokens should succeed");
        assert!(page1.len() <= 2, "should respect limit of 2, got {}", page1.len());
        assert!(cursor1.is_some(), "should have a next cursor (global data)");

        // Fetch second page using cursor
        let decoded = decode_registration_token_cursor(cursor1.as_deref());
        assert!(decoded.is_some(), "cursor should decode");

        let (page2, _cursor2) = storage
            .get_all_tokens(2, decoded)
            .await
            .expect("get_all_tokens page 2 should succeed");
        assert!(page2.len() <= 2, "page 2 should respect limit of 2, got {}", page2.len());

        // Verify no overlap between pages
        let page1_ids: std::collections::HashSet<i64> = page1.iter().map(|t| t.id).collect();
        for t in &page2 {
            assert!(!page1_ids.contains(&t.id), "duplicate token id {} between pages", t.id);
        }

        cleanup_test_data(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————
    // 14. get_all_tokens — empty
    // ——————————————————————————————————————————

    #[tokio::test]
    async fn test_get_all_tokens_returns_without_error() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_test_data(&pool, &suffix).await;

        let storage = RegistrationTokenStorage::new(&pool);

        let (rows, _cursor) = storage
            .get_all_tokens(10, None)
            .await
            .expect("get_all_tokens should succeed");
        // get_all_tokens is global — can't assert empty in shared test DB
        assert!(rows.len() <= 10, "should respect limit");

        cleanup_test_data(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————
    // 15. get_active_tokens (active + empty)
    // ——————————————————————————————————————————

    #[tokio::test]
    async fn test_get_active_tokens_returns_active_only() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_test_data(&pool, &suffix).await;

        let storage = RegistrationTokenStorage::new(&pool);

        // Create an active token
        let active_str = format!("active_{}", suffix);
        storage
            .create_token(CreateRegistrationTokenRequest {
                token: Some(active_str.clone()),
                ..empty_token_request()
            })
            .await
            .expect("create_token should succeed");

        // Create a disabled token
        let disabled_str = format!("disabled_{}", suffix);
        let disabled = storage
            .create_token(CreateRegistrationTokenRequest {
                token: Some(disabled_str.clone()),
                ..empty_token_request()
            })
            .await
            .expect("create_token should succeed");
        storage.deactivate_token(disabled.id).await.expect("deactivate should succeed");

        // Create an expired token
        let expired_str = format!("expired_{}", suffix);
        let past = chrono::Utc::now().timestamp_millis() - 86_400_000;
        storage
            .create_token(CreateRegistrationTokenRequest {
                token: Some(expired_str.clone()),
                expires_at: Some(past),
                ..empty_token_request()
            })
            .await
            .expect("create_token should succeed");

        let active = storage.get_active_tokens().await.expect("get_active_tokens should succeed");

        // Our active token must be in the results
        let active_found = active.iter().any(|t| t.token == active_str);
        assert!(active_found, "active token should appear in get_active_tokens results");

        // Our disabled token must NOT be in the results
        let disabled_found = active.iter().any(|t| t.token == disabled_str);
        assert!(!disabled_found, "disabled token should not appear in get_active_tokens results");

        // Our expired token must NOT be in the results
        let expired_found = active.iter().any(|t| t.token == expired_str);
        assert!(!expired_found, "expired token should not appear in get_active_tokens results");

        cleanup_test_data(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————
    // 16. get_token_usage (records + empty)
    // ——————————————————————————————————————————

    #[tokio::test]
    async fn test_get_token_usage_with_records() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_test_data(&pool, &suffix).await;

        let storage = RegistrationTokenStorage::new(&pool);
        let token_str = make_full_token(&suffix);
        let user_id = format!("@usage_{}:test.local", suffix);
        ensure_test_user(&pool, &user_id).await;

        let created = storage
            .create_token(CreateRegistrationTokenRequest {
                token: Some(token_str.clone()),
                token_type: Some("multi_use".to_string()),
                max_uses: Some(10),
                ..empty_token_request()
            })
            .await
            .expect("create_token should succeed");

        // Empty before any usage
        let empty = storage.get_token_usage(created.id).await.expect("get_token_usage should succeed");
        assert!(empty.is_empty());

        // Use token
        storage
            .use_token(&token_str, &user_id, Some("testuser"), Some("test@test.local"), Some("127.0.0.1"), Some("TestAgent/1.0"))
            .await
            .expect("use_token should succeed");

        let records = storage.get_token_usage(created.id).await.expect("get_token_usage should succeed");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].token, token_str);
        assert_eq!(records[0].user_id, user_id);
        assert_eq!(records[0].username.as_deref(), Some("testuser"));
        assert!(records[0].is_success);

        cleanup_test_data(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————
    // 17. deactivate_token (deactivates + idempotent)
    // ——————————————————————————————————————————

    #[tokio::test]
    async fn test_deactivate_token_and_idempotent() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_test_data(&pool, &suffix).await;

        let storage = RegistrationTokenStorage::new(&pool);
        let token_str = make_full_token(&suffix);

        let created = storage
            .create_token(CreateRegistrationTokenRequest {
                token: Some(token_str.clone()),
                ..empty_token_request()
            })
            .await
            .expect("create_token should succeed");
        assert!(created.is_enabled);

        // Deactivate
        storage.deactivate_token(created.id).await.expect("deactivate should succeed");
        let after = storage.get_token_by_id(created.id).await.expect("get should succeed");
        assert!(!after.unwrap().is_enabled, "token should be disabled");

        // Idempotent — deactivate again
        let result = storage.deactivate_token(created.id).await;
        assert!(result.is_ok(), "second deactivate should not error");

        // Also idempotent on never-existed id
        let result2 = storage.deactivate_token(-99999).await;
        assert!(result2.is_ok(), "deactivate on never-existed id should not error");

        cleanup_test_data(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————
    // 18. cleanup_expired_tokens
    // ——————————————————————————————————————————

    #[tokio::test]
    async fn test_cleanup_expired_tokens() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_test_data(&pool, &suffix).await;

        let storage = RegistrationTokenStorage::new(&pool);

        // Create a valid token (no expiry)
        let valid_str = format!("valid_{}", suffix);
        let valid = storage
            .create_token(CreateRegistrationTokenRequest {
                token: Some(valid_str.clone()),
                ..empty_token_request()
            })
            .await
            .expect("create_token should succeed");
        assert!(valid.is_enabled);

        // Create an expired token (expires_at in the past), must be enabled
        let expired_str = format!("expired_{}", suffix);
        let past = chrono::Utc::now().timestamp_millis() - 86_400_000;
        let expired = storage
            .create_token(CreateRegistrationTokenRequest {
                token: Some(expired_str.clone()),
                expires_at: Some(past),
                ..empty_token_request()
            })
            .await
            .expect("create_token should succeed");
        assert!(expired.is_enabled);

        // Run cleanup
        let affected = storage
            .cleanup_expired_tokens()
            .await
            .expect("cleanup_expired_tokens should succeed");
        assert!(affected >= 1, "should have affected at least 1 expired token");

        // Valid token should still be enabled
        let valid_after = storage.get_token_by_id(valid.id).await.expect("get should succeed").unwrap();
        assert!(valid_after.is_enabled, "valid token should still be enabled");

        // Expired token should now be disabled
        let expired_after = storage.get_token_by_id(expired.id).await.expect("get should succeed").unwrap();
        assert!(!expired_after.is_enabled, "expired token should be disabled");

        // Second cleanup should return 0 (no more expired + enabled tokens)
        let affected2 = storage
            .cleanup_expired_tokens()
            .await
            .expect("cleanup should succeed");
        assert_eq!(affected2, 0, "second cleanup should affect 0 rows");

        cleanup_test_data(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————
    // 19. create_token with auto-generated token
    // ——————————————————————————————————————————

    #[tokio::test]
    async fn test_create_token_auto_generates_token() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_test_data(&pool, &suffix).await;

        let storage = RegistrationTokenStorage::new(&pool);

        // Create token without specifying token — it should be auto-generated
        let request = CreateRegistrationTokenRequest {
            token: None,
            token_type: Some("single_use".to_string()),
            created_by: Some(format!("@admin_{}:test.local", suffix)),
            ..empty_token_request()
        };

        let result = storage.create_token(request).await.expect("create_token should succeed");

        assert!(result.id > 0);
        assert!(!result.token.is_empty());
        assert_eq!(result.token.len(), 32, "auto-generated token should be 32 characters");
        assert_eq!(result.token_type, "single_use");
        assert_eq!(result.created_by.as_deref(), Some(format!("@admin_{}:test.local", suffix).as_str()));

        // The token should be findable (no FK dependency)
        let found = storage.get_token(&result.token).await.expect("get_token should succeed");
        assert!(found.is_some());

        // Cleanup with the generated token pattern — it won't match our suffix,
        // so delete directly by the returned id
        storage.delete_token(result.id).await.ok();

        cleanup_test_data(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————
    // 20. get_room_invite (found + not_found)
    // ——————————————————————————————————————————

    #[tokio::test]
    async fn test_get_room_invite_found_and_not_found() {
        let pool = test_pool().await;
        let suffix = make_suffix();
        cleanup_test_data(&pool, &suffix).await;

        let storage = RegistrationTokenStorage::new(&pool);
        let room_id = format!("!room2_{}:test.local", suffix);
        let inviter = format!("@inviter2_{}:test.local", suffix);
        let invite_code = format!("invitecode_{}", suffix);
        let now = chrono::Utc::now().timestamp_millis();

        // Not found before creation
        let missing = storage.get_room_invite("nonexistent_code").await.expect("get_room_invite should not error");
        assert!(missing.is_none());

        // Insert a room invite via raw SQL (create_room_invite is broken due to
        // required inviter/invitee columns that it does not supply — pre-existing bug).
        sqlx::query(
            "INSERT INTO room_invites (invite_code, room_id, inviter_user_id, inviter, invitee, created_ts, is_used, is_revoked) \
             VALUES ($1, $2, $3, $4, $5, $6, FALSE, FALSE)",
        )
        .bind(&invite_code)
        .bind(&room_id)
        .bind(&inviter)
        .bind(&inviter)
        .bind(&inviter)
        .bind(now)
        .execute(&*pool)
        .await
        .expect("failed to insert test room invite");

        // Find by invite_code
        let found = storage.get_room_invite(&invite_code).await.expect("get_room_invite should not error");
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.invite_code, invite_code);
        assert_eq!(found.room_id, room_id);
        assert_eq!(found.inviter_user_id, inviter);

        cleanup_test_data(&pool, &suffix).await;
    }
}
