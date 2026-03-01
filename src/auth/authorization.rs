use crate::common::{ApiError, ApiResult};
use std::sync::Arc;

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

pub struct AuthorizationService {
    room_member_storage: Arc<crate::storage::RoomMemberStorage>,
    room_storage: Arc<crate::storage::RoomStorage>,
}

impl AuthorizationService {
    pub fn new(
        room_member_storage: Arc<crate::storage::RoomMemberStorage>,
        room_storage: Arc<crate::storage::RoomStorage>,
    ) -> Self {
        Self {
            room_member_storage,
            room_storage,
        }
    }

    pub async fn check_resource_access(
        &self,
        ctx: &AuthorizationContext,
        resource_type: ResourceType,
        resource_owner_id: &str,
        action: Action,
    ) -> ApiResult<()> {
        match resource_type {
            ResourceType::User => {
                self.check_user_access(ctx, resource_owner_id, &action).await?;
            }
            ResourceType::Room => {
                self.check_room_access(ctx, resource_owner_id, &action).await?;
            }
            ResourceType::Device => {
                self.check_device_access(ctx, resource_owner_id, &action).await?;
            }
            ResourceType::Media => {
                self.check_media_access(ctx, resource_owner_id, &action).await?;
            }
            _ => {}
        }
        Ok(())
    }

    async fn check_user_access(
        &self,
        ctx: &AuthorizationContext,
        target_user_id: &str,
        action: &Action,
    ) -> ApiResult<()> {
        match action {
            Action::Read => {
                // Anyone can read public user profiles
                Ok(())
            }
            Action::Write | Action::Delete => {
                // Only the user themselves or admin can modify/delete
                if ctx.user_id == target_user_id || ctx.is_admin {
                    Ok(())
                } else {
                    ::tracing::warn!(
                        target: "security_audit",
                        event = "unauthorized_access_attempt",
                        user_id = ctx.user_id,
                        target_user_id = target_user_id,
                        action = ?action,
                        "User attempted to modify another user's resource"
                    );
                    Err(ApiError::forbidden(
                        "You do not have permission to modify this resource".to_string(),
                    ))
                }
            }
            _ => Ok(()),
        }
    }

    async fn check_room_access(
        &self,
        ctx: &AuthorizationContext,
        room_id: &str,
        action: &Action,
    ) -> ApiResult<()> {
        match action {
            Action::Read => {
                // Check if user is member or room is public
                let is_member = self
                    .room_member_storage
                    .is_member(room_id, &ctx.user_id)
                    .await
                    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

                if is_member {
                    return Ok(());
                }

                // Check if room is public
                let room = self
                    .room_storage
                    .get_room(room_id)
                    .await
                    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

                if let Some(r) = room {
                    if r.join_rule == "public" {
                        return Ok(());
                    }
                }

                Err(ApiError::forbidden(
                    "You do not have access to this room".to_string(),
                ))
            }
            Action::Write | Action::Delete => {
                let is_member = self
                    .room_member_storage
                    .is_member(room_id, &ctx.user_id)
                    .await
                    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

                if !is_member && !ctx.is_admin {
                    return Err(ApiError::forbidden(
                        "You must be a room member to perform this action".to_string(),
                    ));
                }

                Ok(())
            }
            Action::ModifyPowerLevels => {
                // Only users with appropriate power level can modify power levels
                let user_power = self
                    .get_user_power_level(room_id, &ctx.user_id)
                    .await?;

                // Require at least power level 50 (moderator) to modify power levels
                if user_power < 50 && !ctx.is_admin {
                    ::tracing::warn!(
                        target: "security_audit",
                        event = "unauthorized_power_level_change",
                        user_id = ctx.user_id,
                        room_id = room_id,
                        user_power = user_power,
                        "User attempted to modify power levels without sufficient permission"
                    );
                    return Err(ApiError::forbidden(
                        "You do not have permission to modify power levels".to_string(),
                    ));
                }

                Ok(())
            }
            Action::Invite => {
                let user_power = self
                    .get_user_power_level(room_id, &ctx.user_id)
                    .await?;

                // Require at least power level 0 (member) to invite
                if user_power < 0 {
                    return Err(ApiError::forbidden(
                        "You do not have permission to invite users".to_string(),
                    ));
                }

                Ok(())
            }
            Action::Ban | Action::Kick => {
                let user_power = self
                    .get_user_power_level(room_id, &ctx.user_id)
                    .await?;

                // Require at least power level 50 (moderator) to ban/kick
                if user_power < 50 && !ctx.is_admin {
                    return Err(ApiError::forbidden(
                        "You do not have permission to ban/kick users".to_string(),
                    ));
                }

                Ok(())
            }
            Action::Redact => {
                let user_power = self
                    .get_user_power_level(room_id, &ctx.user_id)
                    .await?;

                // Require at least power level 50 (moderator) to redact
                if user_power < 50 && !ctx.is_admin {
                    return Err(ApiError::forbidden(
                        "You do not have permission to redact messages".to_string(),
                    ));
                }

                Ok(())
            }
            Action::Admin => {
                if !ctx.is_admin {
                    return Err(ApiError::forbidden(
                        "Admin access required".to_string(),
                    ));
                }
                Ok(())
            }
        }
    }

    async fn check_device_access(
        &self,
        _ctx: &AuthorizationContext,
        _device_id: &str,
        action: &Action,
    ) -> ApiResult<()> {
        match action {
            Action::Read => Ok(()),
            Action::Write | Action::Delete => {
                // Device ownership should be verified
                // This is a placeholder - actual implementation would check device ownership
                Ok(())
            }
            _ => Ok(()),
        }
    }

    async fn check_media_access(
        &self,
        _ctx: &AuthorizationContext,
        _media_id: &str,
        action: &Action,
    ) -> ApiResult<()> {
        match action {
            Action::Read => Ok(()),
            Action::Write | Action::Delete => {
                // Media ownership should be verified
                // This is a placeholder - actual implementation would check media ownership
                Ok(())
            }
            _ => Ok(()),
        }
    }

    async fn get_user_power_level(&self, room_id: &str, user_id: &str) -> ApiResult<i64> {
        // Get user's power level from room state
        // Default power levels: admin=100, moderator=50, user=0
        
        let member = self
            .room_member_storage
            .get_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if let Some(_m) = member {
            // Check if user is room creator (implicit admin)
            let room = self
                .room_storage
                .get_room(room_id)
                .await
                .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

            if let Some(r) = room {
                if r.creator.as_deref() == Some(user_id) {
                    return Ok(100);
                }
            }

            // Get power level from member data
            // This is simplified - actual implementation would read from power_levels event
            Ok(0)
        } else {
            Ok(-1) // Not a member
        }
    }

    pub async fn verify_profile_ownership(
        &self,
        actor_user_id: &str,
        target_user_id: &str,
    ) -> ApiResult<()> {
        if actor_user_id != target_user_id {
            ::tracing::warn!(
                target: "security_audit",
                event = "unauthorized_profile_modification",
                actor_user_id = actor_user_id,
                target_user_id = target_user_id,
                "User attempted to modify another user's profile"
            );
            return Err(ApiError::forbidden(
                "You can only modify your own profile".to_string(),
            ));
        }
        Ok(())
    }

    pub async fn verify_room_admin(
        &self,
        room_id: &str,
        user_id: &str,
        is_server_admin: bool,
    ) -> ApiResult<()> {
        if is_server_admin {
            return Ok(());
        }

        let power_level = self.get_user_power_level(room_id, user_id).await?;
        
        if power_level < 100 {
            ::tracing::warn!(
                target: "security_audit",
                event = "unauthorized_room_admin_action",
                user_id = user_id,
                room_id = room_id,
                power_level = power_level,
                "User attempted admin action without sufficient permission"
            );
            return Err(ApiError::forbidden(
                "Room admin permission required".to_string(),
            ));
        }

        Ok(())
    }
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
        let types = vec![
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
