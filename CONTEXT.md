# CONTEXT — Domain Glossary

This file names the load-bearing concepts (good seams) in synapse-rust so that
future architecture reviews and AI navigation share one vocabulary. It is a
glossary, not documentation of every type — add a term only when a module is
named after a concept that isn't obvious from the code.

## Membership transitions

- **MembershipTransition** — the single rulebook for whether an `m.room.member`
  state change (`invite`, `join`, `leave`, `ban`, `knock`) is authorized. Lives
  in `synapse-common/src/membership_transition.rs` as a *deep module*: a tiny
  pure interface over the full Matrix membership authorization rules. It sits in
  `synapse-common` because both the client membership handlers
  (`synapse-services`) and the federation inbound event-auth path
  (`synapse-federation` / root crate) must reach it, and those crates are
  siblings that must not depend on each other.

- **`is_legal(from, to, ctx)`** — the pure, synchronous, exhaustively-tested
  verdict function at the heart of MembershipTransition. `from` is the target's
  current membership (`None` = never joined); `to` is the requested membership.
  Returns `Result<(), TransitionError>`. Callers resolve authority facts their
  own async way, then hand the resolved facts here.

- **TransitionCtx** — the resolved authority facts a caller must supply before
  asking for a verdict: actor/target power levels, `ban`/`kick`/`invite`
  thresholds, join rule, and the self-vs-other / banned / creator / restricted
  flags. Plain data only — no storage handles, no async. This struct *is* the
  seam between the async resolution and the pure rulebook.
  - **`TransitionCtx::state_only(...)`** — a constructor for callers that have
    *already* enforced power level and creator protection through a separate
    authority (e.g. `AuthService::can_ban_user`, with its own audit logging).
    Power thresholds are set to always-satisfied sentinels; `is_legal` then only
    enforces the state-machine dimension (transition legality, ban state, join
    rule). Used by the client path and the federation inbound path.

- **TransitionError** — the domain rejection reason (`Banned`, `TargetBanned`,
  `InsufficientPower`, `NotInvited`, `TargetIsCreator`, `InvalidTransition`).
  Domain semantics, not HTTP; clients map it to `ApiError` (`403 Forbidden`),
  federation maps it to event rejection.

- **JoinRule** — the resolved `m.room.join_rules` value (`public`, `invite`,
  `knock`, `restricted`, `knock_restricted`, `private`) as a typed enum, so the
  rulebook branches on a variant instead of a raw string.
