use async_trait::async_trait;
use synapse_common::ApiResult;

/// Room-level authorization: power-level checks, moderator/admin verification,
/// and membership-action permissions (kick, ban, invite, redact).
#[async_trait]
pub trait RoomAuth: Send + Sync {
    async fn verify_message_event_write(&self, room_id: &str, user_id: &str, event_type: &str) -> ApiResult<()>;

    async fn verify_state_event_write(&self, room_id: &str, user_id: &str, event_type: &str) -> ApiResult<()>;

    async fn verify_power_levels_change(
        &self,
        room_id: &str,
        user_id: &str,
        new_content: &serde_json::Value,
    ) -> ApiResult<()>;

    async fn verify_room_moderator(&self, room_id: &str, user_id: &str) -> ApiResult<()>;

    async fn verify_room_admin(&self, room_id: &str, user_id: &str) -> ApiResult<()>;

    async fn can_kick_user(&self, room_id: &str, actor_user_id: &str, target_user_id: &str) -> ApiResult<()>;

    async fn can_ban_user(&self, room_id: &str, actor_user_id: &str, target_user_id: &str) -> ApiResult<()>;

    async fn can_unban_user(&self, room_id: &str, actor_user_id: &str, target_user_id: &str) -> ApiResult<()>;

    async fn can_invite_user(&self, room_id: &str, actor_user_id: &str) -> ApiResult<()>;

    async fn can_redact_event(&self, room_id: &str, actor_user_id: &str, event_sender_id: &str) -> ApiResult<()>;
}
