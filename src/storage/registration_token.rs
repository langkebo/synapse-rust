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

    pub async fn create_token(&self, request: CreateRegistrationTokenRequest) -> Result<RegistrationToken, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let token = request.token.unwrap_or_else(Self::generate_token);
        let token_type = request.token_type.unwrap_or_else(|| "single_use".to_string());

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

    pub async fn update_token(&self, id: i64, request: UpdateRegistrationTokenRequest) -> Result<RegistrationToken, sqlx::Error> {
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

    pub async fn use_token(&self, token: &str, user_id: &str, username: Option<&str>, email: Option<&str>, ip_address: Option<&str>, user_agent: Option<&str>) -> Result<bool, sqlx::Error> {
        let validation = self.validate_token(token).await?;

        if !validation.is_valid {
            return Ok(false);
        }

        let token_id = validation.token_id.unwrap();
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

    pub async fn get_all_tokens(&self, limit: i64, offset: i64) -> Result<Vec<RegistrationToken>, sqlx::Error> {
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

    pub async fn get_token_usage(&self, token_id: i64) -> Result<Vec<RegistrationTokenUsage>, sqlx::Error> {
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

    pub async fn create_room_invite(&self, request: CreateRoomInviteRequest) -> Result<RoomInvite, sqlx::Error> {
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

    pub async fn get_room_invite(&self, invite_code: &str) -> Result<Option<RoomInvite>, sqlx::Error> {
        let row = sqlx::query_as::<_, RoomInvite>(
            "SELECT * FROM room_invites WHERE invite_code = $1",
        )
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

    pub async fn revoke_room_invite(&self, invite_code: &str, reason: &str) -> Result<(), sqlx::Error> {
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

    pub async fn create_batch(&self, batch: &RegistrationTokenBatch, tokens: &[String]) -> Result<i64, sqlx::Error> {
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

    pub async fn get_batch(&self, batch_id: &str) -> Result<Option<RegistrationTokenBatch>, sqlx::Error> {
        let row = sqlx::query_as::<_, RegistrationTokenBatch>(
            "SELECT * FROM registration_token_batches WHERE batch_id = $1",
        )
        .bind(batch_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }
}
