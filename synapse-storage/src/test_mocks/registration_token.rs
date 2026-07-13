use super::*;

#[derive(Clone, Default)]
pub struct InMemoryRegistrationTokenStore {
    tokens: Arc<RwLock<HashMap<String, crate::registration_token::RegistrationToken>>>,
    token_by_id: Arc<RwLock<HashMap<i64, crate::registration_token::RegistrationToken>>>,
    usage: Arc<RwLock<HashMap<i64, Vec<crate::registration_token::RegistrationTokenUsage>>>>,
    invites: Arc<RwLock<HashMap<String, crate::registration_token::RoomInvite>>>,
    batches: Arc<RwLock<HashMap<String, crate::registration_token::RegistrationTokenBatch>>>,
    next_id: Arc<std::sync::atomic::AtomicI64>,
}

impl InMemoryRegistrationTokenStore {
    pub fn new() -> Self {
        Self { next_id: Arc::new(std::sync::atomic::AtomicI64::new(1)), ..Default::default() }
    }
}

#[async_trait::async_trait]
impl crate::registration_token::RegistrationTokenStoreApi for InMemoryRegistrationTokenStore {
    async fn create_token(
        &self,
        request: crate::registration_token::CreateRegistrationTokenRequest,
    ) -> Result<crate::registration_token::RegistrationToken, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = chrono::Utc::now().timestamp_millis();
        let token_str = request.token.unwrap_or_else(|| {
            use rand::Rng;
            let mut rng = rand::rng();
            let chars: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789";
            (0..32).map(|_| chars[rng.random_range(0..chars.len())] as char).collect()
        });
        let token_type = request.token_type.unwrap_or_else(|| "single_use".to_string());
        let t = crate::registration_token::RegistrationToken {
            id,
            token: token_str.clone(),
            token_type,
            description: request.description,
            max_uses: request.max_uses.unwrap_or(1),
            uses_count: 0,
            is_used: false,
            is_enabled: true,
            expires_at: request.expires_at,
            created_by: request.created_by,
            created_ts: now,
            updated_ts: Some(now),
            last_used_ts: None,
            allowed_email_domains: request.allowed_email_domains,
            allowed_user_ids: request.allowed_user_ids,
            auto_join_rooms: request.auto_join_rooms,
            display_name: request.display_name,
            email: request.email,
        };
        self.tokens.write().await.insert(token_str, t.clone());
        self.token_by_id.write().await.insert(id, t.clone());
        Ok(t)
    }

    async fn get_token(
        &self,
        token: &str,
    ) -> Result<Option<crate::registration_token::RegistrationToken>, sqlx::Error> {
        Ok(self.tokens.read().await.get(token).cloned())
    }

    async fn get_token_by_id(
        &self,
        id: i64,
    ) -> Result<Option<crate::registration_token::RegistrationToken>, sqlx::Error> {
        Ok(self.token_by_id.read().await.get(&id).cloned())
    }

    async fn update_token(
        &self,
        id: i64,
        request: crate::registration_token::UpdateRegistrationTokenRequest,
    ) -> Result<crate::registration_token::RegistrationToken, sqlx::Error> {
        let mut by_id = self.token_by_id.write().await;
        let t = by_id.get_mut(&id).ok_or_else(|| sqlx::Error::RowNotFound)?;
        if let Some(desc) = request.description {
            t.description = Some(desc);
        }
        if let Some(mu) = request.max_uses {
            t.max_uses = mu;
        }
        if let Some(enabled) = request.is_enabled {
            t.is_enabled = enabled;
        }
        if let Some(exp) = request.expires_at {
            t.expires_at = Some(exp);
        }
        t.updated_ts = Some(chrono::Utc::now().timestamp_millis());
        let result = t.clone();
        // Also update in tokens map
        self.tokens.write().await.insert(t.token.clone(), t.clone());
        Ok(result)
    }

    async fn delete_token(&self, id: i64) -> Result<(), sqlx::Error> {
        let t = { self.token_by_id.read().await.get(&id).cloned() };
        if let Some(t) = t {
            self.tokens.write().await.remove(&t.token);
            self.token_by_id.write().await.remove(&id);
        }
        Ok(())
    }

    async fn validate_token(
        &self,
        token: &str,
    ) -> Result<crate::registration_token::TokenValidationResult, sqlx::Error> {
        let t = self.tokens.read().await.get(token).cloned();
        match t {
            None => Ok(crate::registration_token::TokenValidationResult {
                is_valid: false,
                token_id: None,
                error_message: Some("Token not found".to_string()),
            }),
            Some(t) => {
                if !t.is_enabled {
                    return Ok(crate::registration_token::TokenValidationResult {
                        is_valid: false,
                        token_id: Some(t.id),
                        error_message: Some("Token is not active".to_string()),
                    });
                }
                if t.is_used && t.token_type == "single_use" {
                    return Ok(crate::registration_token::TokenValidationResult {
                        is_valid: false,
                        token_id: Some(t.id),
                        error_message: Some("Token has already been used".to_string()),
                    });
                }
                if t.max_uses > 0 && t.uses_count >= t.max_uses {
                    return Ok(crate::registration_token::TokenValidationResult {
                        is_valid: false,
                        token_id: Some(t.id),
                        error_message: Some("Token has reached maximum uses".to_string()),
                    });
                }
                if let Some(exp) = t.expires_at {
                    if exp < chrono::Utc::now().timestamp_millis() {
                        return Ok(crate::registration_token::TokenValidationResult {
                            is_valid: false,
                            token_id: Some(t.id),
                            error_message: Some("Token has expired".to_string()),
                        });
                    }
                }
                Ok(crate::registration_token::TokenValidationResult {
                    is_valid: true,
                    token_id: Some(t.id),
                    error_message: None,
                })
            }
        }
    }

    async fn use_token(
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
        let token_id = validation.token_id.unwrap_or(0);
        let mut by_id = self.token_by_id.write().await;
        if let Some(t) = by_id.get_mut(&token_id) {
            t.uses_count += 1;
            if t.token_type == "single_use" {
                t.is_used = true;
            }
            t.last_used_ts = Some(chrono::Utc::now().timestamp_millis());
            self.tokens.write().await.insert(t.token.clone(), t.clone());
        }
        let usage = crate::registration_token::RegistrationTokenUsage {
            id: self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            token_id: Some(token_id),
            token: token.to_string(),
            user_id: user_id.to_string(),
            username: username.map(|s| s.to_string()),
            email: email.map(|s| s.to_string()),
            ip_address: ip_address.map(|s| s.to_string()),
            user_agent: user_agent.map(|s| s.to_string()),
            used_ts: chrono::Utc::now().timestamp_millis(),
            is_success: true,
            error_message: None,
        };
        self.usage.write().await.entry(token_id).or_default().push(usage);
        Ok(true)
    }

    async fn get_all_tokens(
        &self,
        limit: i64,
        from: Option<crate::registration_token::RegistrationTokenCursor>,
    ) -> Result<(Vec<crate::registration_token::RegistrationToken>, Option<String>), sqlx::Error> {
        let tokens = self.token_by_id.read().await;
        let mut all: Vec<_> = tokens.values().cloned().collect();
        all.sort_by_key(|t| std::cmp::Reverse((t.created_ts, t.id)));
        let mut iter = all.into_iter();
        if let Some(cursor) = from {
            iter = iter
                .skip_while(|t| {
                    t.created_ts > cursor.created_ts || (t.created_ts == cursor.created_ts && t.id >= cursor.id)
                })
                .collect::<Vec<_>>()
                .into_iter();
        }
        let rows: Vec<_> = iter.take((limit + 1) as usize).collect();
        let cursor = if rows.len() > limit as usize {
            rows.get(limit as usize).map(|t| {
                crate::registration_token::encode_registration_token_cursor(
                    &crate::registration_token::RegistrationTokenCursor { created_ts: t.created_ts, id: t.id },
                )
            })
        } else {
            None
        };
        Ok((rows.into_iter().take(limit as usize).collect(), cursor))
    }

    async fn get_active_tokens(&self) -> Result<Vec<crate::registration_token::RegistrationToken>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        Ok(self
            .tokens
            .read()
            .await
            .values()
            .filter(|t| {
                t.is_enabled && t.expires_at.is_none_or(|e| e > now) && (t.max_uses == 0 || t.uses_count < t.max_uses)
            })
            .cloned()
            .collect())
    }

    async fn get_token_usage(
        &self,
        token_id: i64,
    ) -> Result<Vec<crate::registration_token::RegistrationTokenUsage>, sqlx::Error> {
        Ok(self.usage.read().await.get(&token_id).cloned().unwrap_or_default())
    }

    async fn deactivate_token(&self, id: i64) -> Result<(), sqlx::Error> {
        if let Some(t) = self.token_by_id.write().await.get_mut(&id) {
            t.is_enabled = false;
            self.tokens.write().await.insert(t.token.clone(), t.clone());
        }
        Ok(())
    }

    async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut count = 0i64;
        for (_, t) in self.token_by_id.write().await.iter_mut() {
            if t.is_enabled && t.expires_at.is_some_and(|e| e < now) {
                t.is_enabled = false;
                count += 1;
            }
        }
        Ok(count)
    }

    async fn create_room_invite(
        &self,
        request: crate::registration_token::CreateRoomInviteRequest,
    ) -> Result<crate::registration_token::RoomInvite, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = chrono::Utc::now().timestamp_millis();
        let invite_code: String = {
            use rand::Rng;
            let mut rng = rand::rng();
            let chars: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789";
            (0..32).map(|_| chars[rng.random_range(0..chars.len())] as char).collect()
        };
        let invite = crate::registration_token::RoomInvite {
            id,
            invite_code: invite_code.clone(),
            room_id: request.room_id,
            inviter_user_id: request.inviter_user_id,
            invitee_email: request.invitee_email,
            invitee_user_id: None,
            is_used: false,
            is_revoked: false,
            expires_at: request.expires_at,
            created_ts: now,
            used_ts: None,
            revoked_at: None,
            revoked_reason: None,
        };
        self.invites.write().await.insert(invite_code, invite.clone());
        Ok(invite)
    }

    async fn get_room_invite(
        &self,
        invite_code: &str,
    ) -> Result<Option<crate::registration_token::RoomInvite>, sqlx::Error> {
        Ok(self.invites.read().await.get(invite_code).cloned())
    }

    async fn use_room_invite(&self, invite_code: &str, invitee_user_id: &str) -> Result<bool, sqlx::Error> {
        let mut invites = self.invites.write().await;
        if let Some(i) = invites.get_mut(invite_code) {
            if i.is_used || i.is_revoked {
                return Ok(false);
            }
            if let Some(exp) = i.expires_at {
                if exp < chrono::Utc::now().timestamp_millis() {
                    return Ok(false);
                }
            }
            i.is_used = true;
            i.invitee_user_id = Some(invitee_user_id.to_string());
            i.used_ts = Some(chrono::Utc::now().timestamp_millis());
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn revoke_room_invite(&self, invite_code: &str, reason: &str) -> Result<(), sqlx::Error> {
        if let Some(i) = self.invites.write().await.get_mut(invite_code) {
            i.is_revoked = true;
            i.revoked_at = Some(chrono::Utc::now().timestamp_millis());
            i.revoked_reason = Some(reason.to_string());
        }
        Ok(())
    }

    async fn create_batch(
        &self,
        batch: &crate::registration_token::RegistrationTokenBatch,
        tokens: &[String],
    ) -> Result<i64, sqlx::Error> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let b = crate::registration_token::RegistrationTokenBatch {
            id,
            batch_id: batch.batch_id.clone(),
            description: batch.description.clone(),
            token_count: batch.token_count,
            tokens_used: 0,
            created_by: batch.created_by.clone(),
            created_ts: chrono::Utc::now().timestamp_millis(),
            expires_at: batch.expires_at,
            is_enabled: true,
            allowed_email_domains: batch.allowed_email_domains.clone(),
            auto_join_rooms: batch.auto_join_rooms.clone(),
        };
        for token_str in tokens {
            let tid = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let now = chrono::Utc::now().timestamp_millis();
            let t = crate::registration_token::RegistrationToken {
                id: tid,
                token: token_str.clone(),
                token_type: "single_use".to_string(),
                description: batch.description.clone(),
                max_uses: 1,
                uses_count: 0,
                is_used: false,
                is_enabled: true,
                expires_at: batch.expires_at,
                created_by: batch.created_by.clone(),
                created_ts: now,
                updated_ts: Some(now),
                last_used_ts: None,
                allowed_email_domains: None,
                allowed_user_ids: None,
                auto_join_rooms: None,
                display_name: None,
                email: None,
            };
            self.tokens.write().await.insert(token_str.clone(), t.clone());
            self.token_by_id.write().await.insert(tid, t);
        }
        self.batches.write().await.insert(batch.batch_id.clone(), b);
        Ok(id)
    }

    async fn get_batch(
        &self,
        batch_id: &str,
    ) -> Result<Option<crate::registration_token::RegistrationTokenBatch>, sqlx::Error> {
        Ok(self.batches.read().await.get(batch_id).cloned())
    }
}
