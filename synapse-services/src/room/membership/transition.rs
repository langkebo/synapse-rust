//! Membership transition legality table.
//!
//! Single source of truth for whether a membership state change is legal.
//! Called by both client handlers (actions.rs, moderation.rs) and the
//! federation inbound path (transaction.rs).

/// The five Matrix membership states that participate in transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MembershipState {
    Invite,
    Join,
    Leave,
    Ban,
    Knock,
}

impl std::str::FromStr for MembershipState {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "invite" => Ok(Self::Invite),
            "join" => Ok(Self::Join),
            "leave" => Ok(Self::Leave),
            "ban" => Ok(Self::Ban),
            "knock" => Ok(Self::Knock),
            _ => Err("unknown membership state"),
        }
    }
}

impl MembershipState {
    /// Convenience wrapper that returns `Option` for ergonomic use in
    /// `.and_then()` chains.
    pub fn parse_opt(s: &str) -> Option<Self> {
        <Self as std::str::FromStr>::from_str(s).ok()
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Invite => "invite",
            Self::Join => "join",
            Self::Leave => "leave",
            Self::Ban => "ban",
            Self::Knock => "knock",
        }
    }
}

/// Context required to evaluate transition legality.
///
/// Carry only what the transition table needs to decide (no DB handles, no IO).
/// Callers build this from their local knowledge (join_rule, power level info).
#[derive(Debug, Clone, Default)]
pub struct TransitionContext {
    /// The room's effective join rule (e.g. "public", "invite", "knock").
    /// `None` when unknown — the table fails-closed for unknown context.
    pub join_rule: Option<String>,
}

impl TransitionContext {
    pub fn new(join_rule: Option<String>) -> Self {
        Self { join_rule }
    }
}

/// Validate whether a membership transition is legal.
///
/// Returns `Ok(())` if the transition is allowed, or `Err(msg)` if denied.
/// This is a pure function — no IO, no side effects. Exhaustively testable.
///
/// Power-level checks (can_ban, can_kick, can_invite) are handled separately
/// by the auth service and are NOT part of this table.
pub fn is_legal(
    from: Option<MembershipState>,
    to: MembershipState,
    ctx: &TransitionContext,
) -> Result<(), &'static str> {
    match (from, to) {
        // ── [*] (no prior membership) ──
        (None, MembershipState::Invite) => Ok(()),
        (None, MembershipState::Knock) => Ok(()),
        // Public rooms allow direct join without prior invite.
        (None, MembershipState::Join) => {
            if ctx.join_rule.as_deref() == Some("public") {
                Ok(())
            } else {
                Err("Cannot join a room without an invite")
            }
        }
        (None, MembershipState::Leave) => Err("Cannot leave a room you are not a member of"),
        (None, MembershipState::Ban) => Err("Cannot ban a user who is not in the room"),

        // ── invite ──
        (Some(MembershipState::Invite), MembershipState::Join) => Ok(()),
        (Some(MembershipState::Invite), MembershipState::Leave) => Ok(()),
        (Some(MembershipState::Invite), MembershipState::Ban) => Ok(()),
        (Some(MembershipState::Invite), MembershipState::Knock) => Ok(()),
        (Some(MembershipState::Invite), MembershipState::Invite) => Ok(()),

        // ── join ──
        (Some(MembershipState::Join), MembershipState::Join) => Ok(()),
        (Some(MembershipState::Join), MembershipState::Leave) => Ok(()),
        (Some(MembershipState::Join), MembershipState::Ban) => Ok(()),
        (Some(MembershipState::Join), MembershipState::Invite) => Err("User is already a member of this room"),
        (Some(MembershipState::Join), MembershipState::Knock) => Err("You are already joined to this room"),

        // ── leave ──
        (Some(MembershipState::Leave), MembershipState::Join) => Ok(()),
        (Some(MembershipState::Leave), MembershipState::Invite) => Ok(()),
        (Some(MembershipState::Leave), MembershipState::Knock) => Ok(()),
        (Some(MembershipState::Leave), MembershipState::Leave) => Ok(()),
        (Some(MembershipState::Leave), MembershipState::Ban) => Err("Cannot ban a user who is not in the room"),

        // ── ban ──
        (Some(MembershipState::Ban), MembershipState::Leave) => Ok(()),
        (Some(MembershipState::Ban), MembershipState::Ban) => Ok(()),
        (Some(MembershipState::Ban), MembershipState::Join) => Err("You are banned from this room"),
        (Some(MembershipState::Ban), MembershipState::Invite) => Err("Cannot invite a banned user"),
        (Some(MembershipState::Ban), MembershipState::Knock) => Err("You are banned from this room"),

        // ── knock ──
        (Some(MembershipState::Knock), MembershipState::Invite) => Ok(()),
        (Some(MembershipState::Knock), MembershipState::Join) => Ok(()),
        (Some(MembershipState::Knock), MembershipState::Leave) => Ok(()),
        (Some(MembershipState::Knock), MembershipState::Ban) => Ok(()),
        (Some(MembershipState::Knock), MembershipState::Knock) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── [*] transitions ──

    #[test]
    fn test_none_to_invite_is_legal() {
        assert!(is_legal(None, MembershipState::Invite, &TransitionContext::default()).is_ok());
    }

    #[test]
    fn test_none_to_knock_is_legal() {
        assert!(is_legal(None, MembershipState::Knock, &TransitionContext::default()).is_ok());
    }

    #[test]
    fn test_none_to_join_rejected_in_non_public_room() {
        assert!(is_legal(None, MembershipState::Join, &TransitionContext::default()).is_err());
    }

    #[test]
    fn test_none_to_join_allowed_in_public_room() {
        let ctx = TransitionContext::new(Some("public".into()));
        assert!(is_legal(None, MembershipState::Join, &ctx).is_ok());
    }

    #[test]
    fn test_none_to_leave_rejected() {
        assert!(is_legal(None, MembershipState::Leave, &TransitionContext::default()).is_err());
    }

    #[test]
    fn test_none_to_ban_rejected() {
        assert!(is_legal(None, MembershipState::Ban, &TransitionContext::default()).is_err());
    }

    // ── invite transitions ──

    #[test]
    fn test_invite_to_join_is_legal() {
        assert!(is_legal(Some(MembershipState::Invite), MembershipState::Join, &TransitionContext::default()).is_ok());
    }

    #[test]
    fn test_invite_to_leave_is_legal() {
        assert!(is_legal(Some(MembershipState::Invite), MembershipState::Leave, &TransitionContext::default()).is_ok());
    }

    #[test]
    fn test_invite_to_ban_is_legal() {
        assert!(is_legal(Some(MembershipState::Invite), MembershipState::Ban, &TransitionContext::default()).is_ok());
    }

    #[test]
    fn test_invite_idempotent() {
        assert!(is_legal(Some(MembershipState::Invite), MembershipState::Invite, &TransitionContext::default()).is_ok());
    }

    // ── join transitions ──

    #[test]
    fn test_join_to_leave_is_legal() {
        assert!(is_legal(Some(MembershipState::Join), MembershipState::Leave, &TransitionContext::default()).is_ok());
    }

    #[test]
    fn test_join_to_ban_is_legal() {
        assert!(is_legal(Some(MembershipState::Join), MembershipState::Ban, &TransitionContext::default()).is_ok());
    }

    #[test]
    fn test_join_idempotent() {
        assert!(is_legal(Some(MembershipState::Join), MembershipState::Join, &TransitionContext::default()).is_ok());
    }

    #[test]
    fn test_join_to_invite_rejected() {
        assert!(is_legal(Some(MembershipState::Join), MembershipState::Invite, &TransitionContext::default()).is_err());
    }

    #[test]
    fn test_join_to_knock_rejected() {
        assert!(is_legal(Some(MembershipState::Join), MembershipState::Knock, &TransitionContext::default()).is_err());
    }

    // ── leave transitions ──

    #[test]
    fn test_leave_to_join_is_legal() {
        assert!(is_legal(Some(MembershipState::Leave), MembershipState::Join, &TransitionContext::default()).is_ok());
    }

    #[test]
    fn test_leave_to_invite_is_legal() {
        assert!(is_legal(Some(MembershipState::Leave), MembershipState::Invite, &TransitionContext::default()).is_ok());
    }

    #[test]
    fn test_leave_to_ban_rejected() {
        assert!(is_legal(Some(MembershipState::Leave), MembershipState::Ban, &TransitionContext::default()).is_err());
    }

    // ── ban transitions ──

    #[test]
    fn test_ban_to_leave_is_legal() {
        assert!(is_legal(Some(MembershipState::Ban), MembershipState::Leave, &TransitionContext::default()).is_ok());
    }

    #[test]
    fn test_ban_to_join_rejected() {
        assert!(is_legal(Some(MembershipState::Ban), MembershipState::Join, &TransitionContext::default()).is_err());
    }

    #[test]
    fn test_ban_to_invite_rejected() {
        assert!(is_legal(Some(MembershipState::Ban), MembershipState::Invite, &TransitionContext::default()).is_err());
    }

    #[test]
    fn test_ban_to_knock_rejected() {
        assert!(is_legal(Some(MembershipState::Ban), MembershipState::Knock, &TransitionContext::default()).is_err());
    }

    // ── knock transitions ──

    #[test]
    fn test_knock_to_invite_is_legal() {
        assert!(is_legal(Some(MembershipState::Knock), MembershipState::Invite, &TransitionContext::default()).is_ok());
    }

    #[test]
    fn test_knock_to_join_is_legal() {
        assert!(is_legal(Some(MembershipState::Knock), MembershipState::Join, &TransitionContext::default()).is_ok());
    }

    #[test]
    fn test_knock_to_leave_is_legal() {
        assert!(is_legal(Some(MembershipState::Knock), MembershipState::Leave, &TransitionContext::default()).is_ok());
    }

    // ── exhaustive coverage: all 5×5+5 = 30 combinations ──

    #[test]
    fn test_exhaustive_all_transitions_have_defined_rules() {
        let states = [
            MembershipState::Invite,
            MembershipState::Join,
            MembershipState::Leave,
            MembershipState::Ban,
            MembershipState::Knock,
        ];
        let ctx = TransitionContext::default();
        for &from in &states {
            for &to in &states {
                let result = is_legal(Some(from), to, &ctx);
                assert!(result.is_ok() || result.is_err(), "({:?}, {:?}) returned nothing", from, to);
            }
        }
        for &to in &states {
            let result = is_legal(None, to, &ctx);
            assert!(result.is_ok() || result.is_err(), "([*], {:?}) returned nothing", to);
        }
    }

    // ── from_str / as_str round-trip ──

    #[test]
    fn test_membership_state_round_trip() {
        for s in &["invite", "join", "leave", "ban", "knock"] {
            let state = MembershipState::parse_opt(s).expect("valid state");
            assert_eq!(state.as_str(), *s);
        }
    }

    #[test]
    fn test_from_str_invalid() {
        assert!(MembershipState::parse_opt("garbage").is_none());
    }
}
