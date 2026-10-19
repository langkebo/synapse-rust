//! Best-effort mirroring of room moderation transitions into friend DMs.
//!
//! Every ban / kick / unban — whether it arrives through the client-server
//! endpoints or the admin endpoints — has to push the same membership
//! transition into any friend DM that links the target user to the room. The
//! mirroring is **best-effort**: the moderation itself has already been
//! persisted, so a friend-DM failure must never turn a successful moderation
//! into an error response.
//!
//! The trigger lives in [`MembershipService`](super::service::MembershipService)
//! so the HTTP layer no longer has to remember to call it. The concrete
//! implementation lives in `friend_room_service`, which depends on
//! [`RoomService`](crate::room::service::RoomService) — installing it as a
//! direct strong dependency would form an `Arc` cycle
//! (`FriendRoomService → RoomService → MembershipService → FriendRoomService`).
//! Instead [`MembershipService`](super::service::MembershipService) holds a
//! [`Weak`](std::sync::Weak) reference installed after construction.

use async_trait::async_trait;

/// The membership transitions that are mirrored into a friend DM.
///
/// The wire vocabulary (`banned` / `kicked` / `unbanned`) is fixed by
/// `FriendRoomService::sync_dm_room_membership_change`; spelling the states as
/// an enum keeps a typo from silently writing an unknown state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModerationState {
    /// The target is now `ban`ned from the room.
    Banned,
    /// The target was kicked (`leave`), but not banned.
    Kicked,
    /// A previous ban was lifted (`leave`).
    Unbanned,
}

impl ModerationState {
    /// The friend-DM state string this transition maps to.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Banned => "banned",
            Self::Kicked => "kicked",
            Self::Unbanned => "unbanned",
        }
    }
}

/// The friend-DM side of a moderation transition.
///
/// Implemented by `FriendRoomService`; modelled as a trait so the membership
/// layer does not depend on the concrete friend service (and therefore does
/// not close an `Arc` cycle).
#[async_trait]
pub trait FriendDmMembershipSync: Send + Sync {
    /// Mirror `state` into the friend DM linking `affected_user_id` to
    /// `room_id`. Returns the number of friend-list links updated.
    async fn sync_membership_state(
        &self,
        room_id: &str,
        affected_user_id: &str,
        state: ModerationState,
        changed_by: Option<&str>,
        reason: Option<&str>,
    ) -> Result<usize, String>;
}
