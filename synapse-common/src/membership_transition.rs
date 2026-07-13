//! Room membership transition legality — the single rulebook for whether an
//! `m.room.member` state change is authorized.
//!
//! Deep module: a tiny pure interface (`is_legal`) over the full Matrix
//! membership authorization rules. Both the client membership handlers
//! (synapse-services) and the federation inbound event-auth path
//! (synapse-federation) resolve authority (power levels, join rule, creator,
//! ban state) their own async way, then hand the resolved facts here for a
//! pure, synchronous, exhaustively-testable verdict.
//!
//! Scope: room versions v1–v11 share one rule set (current behaviour). A
//! `room_version` dimension is intentionally out of scope until v12 auth
//! differences are evaluated; add it to [`TransitionCtx`] then.

use crate::error::ApiError;
use crate::types::Membership;
use std::fmt;
use std::str::FromStr;

/// Room join rule, resolved from `m.room.join_rules`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JoinRule {
    Public,
    Invite,
    Knock,
    Restricted,
    KnockRestricted,
    Private,
}

impl fmt::Display for JoinRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Public => write!(f, "public"),
            Self::Invite => write!(f, "invite"),
            Self::Knock => write!(f, "knock"),
            Self::Restricted => write!(f, "restricted"),
            Self::KnockRestricted => write!(f, "knock_restricted"),
            Self::Private => write!(f, "private"),
        }
    }
}

impl FromStr for JoinRule {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "public" => Ok(Self::Public),
            "invite" => Ok(Self::Invite),
            "knock" => Ok(Self::Knock),
            "restricted" => Ok(Self::Restricted),
            "knock_restricted" => Ok(Self::KnockRestricted),
            "private" => Ok(Self::Private),
            _ => Err(()),
        }
    }
}

/// Authority facts a caller must resolve before asking for a verdict. Plain
/// data only — no storage handles, no async. The caller (client or federation
/// adapter) owns the async resolution; this struct is the seam.
#[derive(Debug, Clone, Copy)]
pub struct TransitionCtx {
    /// Power level of the sender making the change.
    pub actor_pl: i64,
    /// Power level of the target of the change (`state_key`).
    pub target_pl: i64,
    /// `ban` threshold from `m.room.power_levels`.
    pub ban_level: i64,
    /// `kick` threshold from `m.room.power_levels`.
    pub kick_level: i64,
    /// `invite` threshold from `m.room.power_levels`.
    pub invite_level: i64,
    /// Resolved room join rule.
    pub join_rule: JoinRule,
    /// True when sender == target (self-leave / self-knock / accept-invite),
    /// false when acting on another user (kick / ban / invite-on-behalf).
    pub actor_is_target: bool,
    /// True when the target is currently banned (blocks invite of a banned user).
    pub target_is_banned: bool,
    /// True when the target is the room creator (protected from kick/ban).
    pub target_is_creator: bool,
    /// For restricted / knock_restricted join rules: whether the joiner has
    /// been resolved as satisfying an allow condition. Ignored for other rules.
    pub restricted_join_authorized: bool,
}

impl TransitionCtx {
    /// Build a ctx that validates ONLY the state-machine dimension of a
    /// transition (from→to legality, ban state, join rule, self-vs-other),
    /// delegating power-level and creator-protection authorization to the
    /// caller. Power thresholds are set to always-satisfied sentinels and
    /// `target_is_creator` is `false`.
    ///
    /// Use this from the client membership handlers, where a separate authority
    /// (`AuthService::can_ban_user` / `can_kick_user` / `can_invite_user`) has
    /// already enforced power levels and creator protection with room-specific
    /// audit logging. The federation inbound path, which has no such prior
    /// gate, builds a full [`TransitionCtx`] with real power facts instead.
    pub fn state_only(
        join_rule: JoinRule,
        actor_is_target: bool,
        target_is_banned: bool,
        restricted_join_authorized: bool,
    ) -> Self {
        Self {
            actor_pl: i64::MAX,
            target_pl: i64::MIN,
            ban_level: i64::MIN,
            kick_level: i64::MIN,
            invite_level: i64::MIN,
            join_rule,
            actor_is_target,
            target_is_banned,
            target_is_creator: false,
            restricted_join_authorized,
        }
    }
}

/// Why a membership transition is rejected. Domain semantics, not HTTP —
/// callers map this to their own error surface (`ApiError` for clients,
/// event rejection for federation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionError {
    /// Target is banned and must be unbanned before this transition.
    Banned,
    /// Cannot invite a user who is currently banned.
    TargetBanned,
    /// Sender lacks the power level required for this action.
    InsufficientPower { needed: i64, have: i64 },
    /// Room is invite-only (or knock/restricted) and the joiner was not invited/authorized.
    NotInvited,
    /// Target is the room creator and cannot be kicked or banned.
    TargetIsCreator,
    /// The state transition is not permitted by the membership state machine.
    InvalidTransition,
}

impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Banned => write!(f, "You are banned from this room"),
            Self::TargetBanned => write!(f, "Cannot invite a banned user"),
            Self::InsufficientPower { needed, have } => {
                write!(f, "Insufficient power level (need {needed}, have {have})")
            }
            Self::NotInvited => write!(f, "You are not invited to this room"),
            Self::TargetIsCreator => write!(f, "Cannot act on the room creator"),
            Self::InvalidTransition => write!(f, "Illegal membership transition"),
        }
    }
}

impl From<TransitionError> for ApiError {
    fn from(err: TransitionError) -> Self {
        ApiError::forbidden(err.to_string())
    }
}

/// Decide whether a room membership transition `from -> to` is authorized.
///
/// `from == None` means the target has no current membership (never joined).
/// Idempotent transitions (e.g. `Join -> Join` profile updates) are legal;
/// callers may short-circuit no-ops themselves.
pub fn is_legal(from: Option<Membership>, to: Membership, ctx: &TransitionCtx) -> Result<(), TransitionError> {
    use Membership::*;
    match to {
        Join => check_join(from, ctx),
        Invite => check_invite(from, ctx),
        Leave => check_leave(from, ctx),
        Ban => check_ban(from, ctx),
        Knock => check_knock(from, ctx),
    }
}

fn check_join(from: Option<Membership>, ctx: &TransitionCtx) -> Result<(), TransitionError> {
    // Only the user themselves can join (state_key == sender).
    if !ctx.actor_is_target {
        return Err(TransitionError::InvalidTransition);
    }
    match from {
        Some(Membership::Ban) => Err(TransitionError::Banned),
        Some(Membership::Join) => Ok(()),   // idempotent / profile update
        Some(Membership::Invite) => Ok(()), // accept invite (valid under any join rule)
        from => match ctx.join_rule {
            JoinRule::Public => Ok(()),
            JoinRule::Restricted | JoinRule::KnockRestricted if ctx.restricted_join_authorized => Ok(()),
            // invite / knock / restricted-unauthorized / private: needs an invite,
            // which was handled by the Invite arm above.
            _ => {
                let _ = from;
                Err(TransitionError::NotInvited)
            }
        },
    }
}

fn check_invite(from: Option<Membership>, ctx: &TransitionCtx) -> Result<(), TransitionError> {
    if ctx.actor_pl < ctx.invite_level {
        return Err(TransitionError::InsufficientPower { needed: ctx.invite_level, have: ctx.actor_pl });
    }
    if ctx.target_is_banned {
        return Err(TransitionError::TargetBanned);
    }
    match from {
        Some(Membership::Ban) => Err(TransitionError::TargetBanned),
        Some(Membership::Join) => Err(TransitionError::InvalidTransition), // already joined
        Some(Membership::Invite) => Ok(()),                                // idempotent re-invite
        // None / Leave / Knock: invite is valid (knock -> invite == accept knock)
        _ => Ok(()),
    }
}

fn check_leave(from: Option<Membership>, ctx: &TransitionCtx) -> Result<(), TransitionError> {
    if ctx.actor_is_target {
        // Self-leave: leaving, rejecting an invite, or retracting a knock.
        return match from {
            // Cannot self-unban; a banned user cannot leave their own ban.
            Some(Membership::Ban) => Err(TransitionError::InvalidTransition),
            _ => Ok(()),
        };
    }
    // Acting on another user: either a kick or an unban.
    match from {
        Some(Membership::Ban) => {
            // Unban requires ban-level power.
            if ctx.actor_pl < ctx.ban_level {
                Err(TransitionError::InsufficientPower { needed: ctx.ban_level, have: ctx.actor_pl })
            } else {
                Ok(())
            }
        }
        Some(Membership::Join) | Some(Membership::Invite) | Some(Membership::Knock) => {
            // Kick.
            if ctx.target_is_creator {
                return Err(TransitionError::TargetIsCreator);
            }
            if ctx.actor_pl < ctx.kick_level || ctx.actor_pl <= ctx.target_pl {
                return Err(TransitionError::InsufficientPower { needed: ctx.kick_level, have: ctx.actor_pl });
            }
            Ok(())
        }
        // Kicking/unbanning someone with no membership or already left: no-op.
        None | Some(Membership::Leave) => Err(TransitionError::InvalidTransition),
    }
}

fn check_ban(_from: Option<Membership>, ctx: &TransitionCtx) -> Result<(), TransitionError> {
    // Nobody bans themselves.
    if ctx.actor_is_target {
        return Err(TransitionError::InvalidTransition);
    }
    if ctx.target_is_creator {
        return Err(TransitionError::TargetIsCreator);
    }
    // Ban is legal from any prior membership, subject to power.
    if ctx.actor_pl < ctx.ban_level || ctx.actor_pl <= ctx.target_pl {
        return Err(TransitionError::InsufficientPower { needed: ctx.ban_level, have: ctx.actor_pl });
    }
    Ok(())
}

fn check_knock(from: Option<Membership>, ctx: &TransitionCtx) -> Result<(), TransitionError> {
    // Only the user themselves can knock.
    if !ctx.actor_is_target {
        return Err(TransitionError::InvalidTransition);
    }
    // Knocking is only meaningful under knock join rules.
    if !matches!(ctx.join_rule, JoinRule::Knock | JoinRule::KnockRestricted) {
        return Err(TransitionError::InvalidTransition);
    }
    match from {
        Some(Membership::Ban) => Err(TransitionError::Banned),
        Some(Membership::Knock) => Ok(()),        // idempotent
        None | Some(Membership::Leave) => Ok(()), // may knock
        // Already invited or joined: knocking is meaningless.
        Some(Membership::Invite) | Some(Membership::Join) => Err(TransitionError::InvalidTransition),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A permissive baseline ctx: actor outranks target, high power, self-action,
    /// public room, target clean. Individual tests tweak one axis.
    fn ctx() -> TransitionCtx {
        TransitionCtx {
            actor_pl: 100,
            target_pl: 0,
            ban_level: 50,
            kick_level: 50,
            invite_level: 0,
            join_rule: JoinRule::Public,
            actor_is_target: true,
            target_is_banned: false,
            target_is_creator: false,
            restricted_join_authorized: false,
        }
    }

    // ---- join ----
    #[test]
    fn self_join_public_is_legal() {
        assert_eq!(is_legal(None, Membership::Join, &ctx()), Ok(()));
    }

    #[test]
    fn join_while_banned_is_rejected() {
        assert_eq!(is_legal(Some(Membership::Ban), Membership::Join, &ctx()), Err(TransitionError::Banned));
    }

    #[test]
    fn accept_invite_join_is_legal_in_invite_only() {
        let c = TransitionCtx { join_rule: JoinRule::Invite, ..ctx() };
        assert_eq!(is_legal(Some(Membership::Invite), Membership::Join, &c), Ok(()));
    }

    #[test]
    fn join_invite_only_without_invite_is_rejected() {
        let c = TransitionCtx { join_rule: JoinRule::Invite, ..ctx() };
        assert_eq!(is_legal(None, Membership::Join, &c), Err(TransitionError::NotInvited));
    }

    #[test]
    fn join_restricted_authorized_is_legal() {
        let c = TransitionCtx { join_rule: JoinRule::Restricted, restricted_join_authorized: true, ..ctx() };
        assert_eq!(is_legal(None, Membership::Join, &c), Ok(()));
    }

    #[test]
    fn join_restricted_unauthorized_is_rejected() {
        let c = TransitionCtx { join_rule: JoinRule::Restricted, restricted_join_authorized: false, ..ctx() };
        assert_eq!(is_legal(None, Membership::Join, &c), Err(TransitionError::NotInvited));
    }

    #[test]
    fn join_on_behalf_of_another_is_illegal() {
        let c = TransitionCtx { actor_is_target: false, ..ctx() };
        assert_eq!(is_legal(None, Membership::Join, &c), Err(TransitionError::InvalidTransition));
    }

    // ---- invite ----
    #[test]
    fn invite_clean_target_is_legal() {
        let c = TransitionCtx { actor_is_target: false, ..ctx() };
        assert_eq!(is_legal(None, Membership::Invite, &c), Ok(()));
    }

    #[test]
    fn invite_banned_target_is_rejected() {
        let c = TransitionCtx { actor_is_target: false, target_is_banned: true, ..ctx() };
        assert_eq!(is_legal(None, Membership::Invite, &c), Err(TransitionError::TargetBanned));
    }

    #[test]
    fn invite_of_banned_from_state_is_rejected() {
        let c = TransitionCtx { actor_is_target: false, ..ctx() };
        assert_eq!(is_legal(Some(Membership::Ban), Membership::Invite, &c), Err(TransitionError::TargetBanned));
    }

    #[test]
    fn invite_without_power_is_rejected() {
        let c = TransitionCtx { actor_is_target: false, actor_pl: 0, invite_level: 50, ..ctx() };
        assert_eq!(
            is_legal(None, Membership::Invite, &c),
            Err(TransitionError::InsufficientPower { needed: 50, have: 0 })
        );
    }

    #[test]
    fn invite_already_joined_is_illegal() {
        let c = TransitionCtx { actor_is_target: false, ..ctx() };
        assert_eq!(is_legal(Some(Membership::Join), Membership::Invite, &c), Err(TransitionError::InvalidTransition));
    }

    #[test]
    fn accept_knock_via_invite_is_legal() {
        let c = TransitionCtx { actor_is_target: false, ..ctx() };
        assert_eq!(is_legal(Some(Membership::Knock), Membership::Invite, &c), Ok(()));
    }

    // ---- leave ----
    #[test]
    fn self_leave_from_join_is_legal() {
        assert_eq!(is_legal(Some(Membership::Join), Membership::Leave, &ctx()), Ok(()));
    }

    #[test]
    fn reject_invite_via_self_leave_is_legal() {
        assert_eq!(is_legal(Some(Membership::Invite), Membership::Leave, &ctx()), Ok(()));
    }

    #[test]
    fn retract_knock_via_self_leave_is_legal() {
        assert_eq!(is_legal(Some(Membership::Knock), Membership::Leave, &ctx()), Ok(()));
    }

    #[test]
    fn self_unban_via_leave_is_illegal() {
        assert_eq!(is_legal(Some(Membership::Ban), Membership::Leave, &ctx()), Err(TransitionError::InvalidTransition));
    }

    #[test]
    fn kick_with_power_is_legal() {
        let c = TransitionCtx { actor_is_target: false, ..ctx() };
        assert_eq!(is_legal(Some(Membership::Join), Membership::Leave, &c), Ok(()));
    }

    #[test]
    fn kick_without_power_is_rejected() {
        let c = TransitionCtx { actor_is_target: false, actor_pl: 10, kick_level: 50, ..ctx() };
        assert_eq!(
            is_legal(Some(Membership::Join), Membership::Leave, &c),
            Err(TransitionError::InsufficientPower { needed: 50, have: 10 })
        );
    }

    #[test]
    fn kick_equal_or_higher_target_is_rejected() {
        let c = TransitionCtx { actor_is_target: false, actor_pl: 50, target_pl: 50, ..ctx() };
        assert_eq!(
            is_legal(Some(Membership::Join), Membership::Leave, &c),
            Err(TransitionError::InsufficientPower { needed: 50, have: 50 })
        );
    }

    #[test]
    fn kick_creator_is_rejected() {
        let c = TransitionCtx { actor_is_target: false, target_is_creator: true, ..ctx() };
        assert_eq!(is_legal(Some(Membership::Join), Membership::Leave, &c), Err(TransitionError::TargetIsCreator));
    }

    #[test]
    fn unban_with_power_is_legal() {
        let c = TransitionCtx { actor_is_target: false, ..ctx() };
        assert_eq!(is_legal(Some(Membership::Ban), Membership::Leave, &c), Ok(()));
    }

    #[test]
    fn unban_without_power_is_rejected() {
        let c = TransitionCtx { actor_is_target: false, actor_pl: 10, ban_level: 50, ..ctx() };
        assert_eq!(
            is_legal(Some(Membership::Ban), Membership::Leave, &c),
            Err(TransitionError::InsufficientPower { needed: 50, have: 10 })
        );
    }

    #[test]
    fn kick_absent_user_is_illegal() {
        let c = TransitionCtx { actor_is_target: false, ..ctx() };
        assert_eq!(is_legal(None, Membership::Leave, &c), Err(TransitionError::InvalidTransition));
        assert_eq!(is_legal(Some(Membership::Leave), Membership::Leave, &c), Err(TransitionError::InvalidTransition));
    }

    // ---- ban ----
    #[test]
    fn ban_with_power_is_legal_from_any_state() {
        let c = TransitionCtx { actor_is_target: false, ..ctx() };
        for from in
            [None, Some(Membership::Join), Some(Membership::Invite), Some(Membership::Knock), Some(Membership::Leave)]
        {
            assert_eq!(is_legal(from, Membership::Ban, &c), Ok(()), "ban from {from:?}");
        }
    }

    #[test]
    fn self_ban_is_illegal() {
        assert_eq!(is_legal(Some(Membership::Join), Membership::Ban, &ctx()), Err(TransitionError::InvalidTransition));
    }

    #[test]
    fn ban_creator_is_rejected() {
        let c = TransitionCtx { actor_is_target: false, target_is_creator: true, ..ctx() };
        assert_eq!(is_legal(Some(Membership::Join), Membership::Ban, &c), Err(TransitionError::TargetIsCreator));
    }

    #[test]
    fn ban_without_power_is_rejected() {
        let c = TransitionCtx { actor_is_target: false, actor_pl: 10, ban_level: 50, ..ctx() };
        assert_eq!(
            is_legal(Some(Membership::Join), Membership::Ban, &c),
            Err(TransitionError::InsufficientPower { needed: 50, have: 10 })
        );
    }

    // ---- knock ----
    #[test]
    fn knock_under_knock_rule_is_legal() {
        let c = TransitionCtx { join_rule: JoinRule::Knock, ..ctx() };
        assert_eq!(is_legal(None, Membership::Knock, &c), Ok(()));
        assert_eq!(is_legal(Some(Membership::Leave), Membership::Knock, &c), Ok(()));
    }

    #[test]
    fn knock_under_public_rule_is_illegal() {
        assert_eq!(is_legal(None, Membership::Knock, &ctx()), Err(TransitionError::InvalidTransition));
    }

    #[test]
    fn knock_while_banned_is_rejected() {
        let c = TransitionCtx { join_rule: JoinRule::Knock, ..ctx() };
        assert_eq!(is_legal(Some(Membership::Ban), Membership::Knock, &c), Err(TransitionError::Banned));
    }

    #[test]
    fn knock_while_joined_or_invited_is_illegal() {
        let c = TransitionCtx { join_rule: JoinRule::Knock, ..ctx() };
        assert_eq!(is_legal(Some(Membership::Join), Membership::Knock, &c), Err(TransitionError::InvalidTransition));
        assert_eq!(is_legal(Some(Membership::Invite), Membership::Knock, &c), Err(TransitionError::InvalidTransition));
    }

    #[test]
    fn knock_on_behalf_of_another_is_illegal() {
        let c = TransitionCtx { join_rule: JoinRule::Knock, actor_is_target: false, ..ctx() };
        assert_eq!(is_legal(None, Membership::Knock, &c), Err(TransitionError::InvalidTransition));
    }

    // ---- error mapping ----
    #[test]
    fn transition_error_maps_to_forbidden_api_error() {
        let api: ApiError = TransitionError::Banned.into();
        assert_eq!(api.http_status(), axum::http::StatusCode::FORBIDDEN);
    }

    // ---- state_only ctx ----
    #[test]
    fn state_only_lets_power_pass_but_enforces_state_machine() {
        // Ban with sentinel power succeeds regardless of thresholds...
        let c = TransitionCtx::state_only(JoinRule::Invite, false, false, false);
        assert_eq!(is_legal(Some(Membership::Join), Membership::Ban, &c), Ok(()));
        // ...but the state machine still rejects a self-ban.
        let self_c = TransitionCtx::state_only(JoinRule::Invite, true, false, false);
        assert_eq!(is_legal(Some(Membership::Join), Membership::Ban, &self_c), Err(TransitionError::InvalidTransition));
    }

    #[test]
    fn state_only_invite_rejects_banned_target() {
        let c = TransitionCtx::state_only(JoinRule::Invite, false, true, false);
        assert_eq!(is_legal(None, Membership::Invite, &c), Err(TransitionError::TargetBanned));
    }

    #[test]
    fn state_only_join_honors_join_rule() {
        let public = TransitionCtx::state_only(JoinRule::Public, true, false, false);
        assert_eq!(is_legal(None, Membership::Join, &public), Ok(()));
        let invite_only = TransitionCtx::state_only(JoinRule::Invite, true, false, false);
        assert_eq!(is_legal(None, Membership::Join, &invite_only), Err(TransitionError::NotInvited));
    }
}
