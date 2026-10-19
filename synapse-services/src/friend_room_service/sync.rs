//! The friend-DM side of the membership-moderation hook.
//!
//! [`FriendRoomService`] implements
//! [`FriendDmMembershipSync`](crate::room::membership::friend_dm_sync::FriendDmMembershipSync)
//! so the membership layer can mirror a ban / kick / unban into the friend list
//! without depending on this module's concrete type.

use async_trait::async_trait;

use crate::friend_room_service::models::FriendRoomService;
use crate::room::membership::friend_dm_sync::{FriendDmMembershipSync, ModerationState};

#[async_trait]
impl FriendDmMembershipSync for FriendRoomService {
    async fn sync_membership_state(
        &self,
        room_id: &str,
        affected_user_id: &str,
        state: ModerationState,
        changed_by: Option<&str>,
        reason: Option<&str>,
    ) -> Result<usize, String> {
        self.sync_dm_room_membership_change(room_id, affected_user_id, state.as_str(), changed_by, reason)
            .await
            .map_err(|error| error.to_string())
    }
}
