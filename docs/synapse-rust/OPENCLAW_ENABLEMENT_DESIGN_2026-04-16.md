# OpenClaw Enablement Design

## Goal

This document records the OpenClaw route-family enablement design and the remaining gaps after the first controlled rollout.

The current repository state is:

- `openclaw` has storage, migrations, and route handlers.
- The route module is feature-gated behind `openclaw-routes`.
- The route module is merged into the top-level router only when both the compile-time feature and runtime flag are enabled.
- OpenClaw webhook handling remains available through `/_synapse/external/openclaw/{service_id}/webhook`.

This remains the correct default posture because the route family stays opt-in and isolated under an unstable Matrix namespace.

## Current State

### Existing user-scoped route surface

`src/web/routes/openclaw.rs` currently defines the following route family:

- `GET/POST /_matrix/client/unstable/org.synapse_rust.openclaw/connections`
- `GET/PUT/DELETE /_matrix/client/unstable/org.synapse_rust.openclaw/connections/{id}`
- `POST /_matrix/client/unstable/org.synapse_rust.openclaw/connections/{id}/test`
- `GET/POST /_matrix/client/unstable/org.synapse_rust.openclaw/conversations`
- `GET/PUT/DELETE /_matrix/client/unstable/org.synapse_rust.openclaw/conversations/{id}`
- `GET/POST /_matrix/client/unstable/org.synapse_rust.openclaw/conversations/{id}/messages`
- `DELETE /_matrix/client/unstable/org.synapse_rust.openclaw/messages/{id}`
- `GET/POST /_matrix/client/unstable/org.synapse_rust.openclaw/generations`
- `GET/DELETE /_matrix/client/unstable/org.synapse_rust.openclaw/generations/{id}`
- `GET/POST /_matrix/client/unstable/org.synapse_rust.openclaw/roles`
- `GET/PUT/DELETE /_matrix/client/unstable/org.synapse_rust.openclaw/roles/{id}`

### Existing auth and authorization behavior

The route handlers use user-scoped auth, not `AdminUser`.

Current handler behavior is mostly owner-scoped:

- connections: owner-only read/write/delete/test
- conversations: owner-only read/write/delete
- messages: conversation owner-only read/write/delete
- generations: owner-only read/delete
- chat roles:
  - create: authenticated owner creates own role
  - read: allowed for owner or any authenticated user if `is_public = true`
  - update/delete: owner-only

Additional enforced policy in the current implementation:

- guest users are explicitly rejected
- connection `base_url` rejects localhost and private or local IP ranges
- owner checks intentionally return `404` for private resources owned by other users

### Existing non-route integration surface

OpenClaw also exists as an external integration through:

- `/_synapse/external/openclaw/{service_id}/webhook`

That webhook path is separate from the user-scoped route family and should remain independent of the user API enablement decision.

## Main Risks

### Risk 1: unstable namespace remains product-internal

The route family now uses the unstable Matrix namespace, which is the correct short-term shape, but it is still not a supported stable client API.

Remaining implications:

- clients must treat it as experimental
- operators should not assume long-term path stability
- any future stable exposure still needs a deliberate compatibility decision

### Risk 2: visibility model must stay documented and tested

`GET /roles/{id}` allows reading public roles, and the list endpoint currently returns both the caller's roles and public roles from other users.

That behavior is coherent today, but it should remain explicit because future refactors could accidentally collapse it back to owner-only listing.

- direct read succeeds for public roles
- list returns both own roles and globally public roles

### Risk 3: low-trust token policy is only partially explicit

Generic authenticated extractors accept any valid access token, including guest tokens and shadow-banned users.

OpenClaw routes already reject guest tokens, which is necessary for user-owned AI connection metadata and API-key backed resources. Shadow-ban behavior is still inherited from broader middleware and has not been made OpenClaw-specific.

### Risk 4: enablement contract must remain constrained

The current top-level wiring is constrained, but the contract still needs to stay documented for operators and contributors:

- whether the capability is enabled
- which users may access it
- which route prefix is stable
- whether the API is experimental or supported

## Recommended Enablement Model

### 1. Keep feature gating

Retain the compile-time feature:

- `openclaw-routes`

Do not expose the route family in default builds.

### 2. Keep runtime gating before top-level merge

If the feature is enabled, still require a runtime flag before wiring the router into `assembly.rs`.

Recommended shape:

- compile-time: `openclaw-routes`
- runtime: `config.experimental.openclaw_routes_enabled`

This gives safe rollout semantics:

- local development can test the feature
- production can build with support but leave it disabled
- rollout can be staged per environment

### 3. Keep the unstable route prefix until a stable contract exists

Do not expose a stable Matrix or ad hoc `/api/...` path until the API contract is intentionally versioned.

Recommended user-scoped prefix:

- `/_matrix/client/unstable/org.synapse_rust.openclaw/...`

Recommended examples:

- `/_matrix/client/unstable/org.synapse_rust.openclaw/connections`
- `/_matrix/client/unstable/org.synapse_rust.openclaw/connections/{id}`
- `/_matrix/client/unstable/org.synapse_rust.openclaw/conversations`
- `/_matrix/client/unstable/org.synapse_rust.openclaw/conversations/{id}/messages`
- `/_matrix/client/unstable/org.synapse_rust.openclaw/roles`

Rationale:

- keeps non-standard API out of stable Matrix namespaces
- makes experimentation explicit
- avoids implying Matrix spec compatibility

The external integration webhook should remain on:

- `/_synapse/external/openclaw/{service_id}/webhook`

because it is server integration traffic rather than a Matrix client API.

### 4. Adopt an explicit auth model

Use `AuthenticatedUser` for all user-scoped OpenClaw routes, but add explicit policy checks:

- deny guest users
- deny shadow-banned users if product policy requires this for outbound AI/integration actions
- never use `AdminUser` for user data access

Recommended helper contract:

- `ensure_openclaw_user_allowed(auth: &AuthenticatedUser) -> Result<(), ApiError>`

Minimum behavior:

- reject `is_guest`
- optionally reject `is_shadow_banned`

### 5. Normalize object visibility rules

The object model should be:

- connections: private, owner-only
- conversations: private, owner-only
- messages: private, owner-only
- generations: private, owner-only
- chat roles:
  - private roles: owner-only
  - public roles: readable by authenticated non-guest users
  - public roles remain editable/deletable only by owner

The route behavior must match that model consistently.

Required adjustment before enablement:

- add a dedicated public-role listing endpoint or update the listing semantics

Recommended option:

- `GET /roles` returns:
  - caller's own roles
  - plus public roles from other users
  - with stable filtering rules

Alternative option:

- `GET /roles` stays owner-only
- add `GET /roles/public`

The first option is usually better for product simplicity.

### 6. Keep ownership checks at the handler or service boundary

Current route handlers already enforce ownership for most mutable operations.

Before enablement, keep or strengthen the following invariant:

- every object fetch that returns non-public data must be validated against the authenticated user

Longer term, consider moving this from route handlers into service-level helper methods to reduce repetition and prevent future missed checks.

## Top-Level Wiring Plan

The route should not be merged unconditionally.

Recommended top-level logic in `assembly.rs`:

1. compile only when `openclaw-routes` is enabled
2. merge only when runtime config says the capability is enabled
3. add a dedicated unstable namespace prefix
4. document the capability in server/client config output only when enabled

Pseudo-structure:

```rust
#[cfg(feature = "openclaw-routes")]
if state.services.config.experimental.openclaw_routes_enabled {
    router = router.merge(create_openclaw_router(state.clone()));
}
```

That merge already exists in the current codebase behind feature and runtime gates, and `create_openclaw_router()` already exposes the unstable Matrix prefix.

## Regression Test Plan

### Route structure tests

Integration coverage should keep asserting that the route family uses the unstable Matrix namespace instead of `/api/...`.

Minimum checks:

- no `/_matrix/client/v3/...` stable path is used
- all OpenClaw user routes live under `/_matrix/client/unstable/org.synapse_rust.openclaw/...`

### Authentication tests

Keep integration tests for:

- unauthenticated request returns `401`
- guest token returns `403`
- regular authenticated user succeeds on own objects

### Authorization tests

Keep integration tests for:

- user A cannot read user B's private connection
- user A cannot update or delete user B's conversation
- user A cannot read user B's generation
- user A can read user B's public role
- user A cannot modify user B's public role

### Listing consistency tests

Keep tests covering the chosen visibility contract:

- verify `GET /roles` keeps returning both own and public roles
- verify private roles never leak to other authenticated users

### Webhook non-regression tests

Keep webhook coverage independent:

- OpenClaw webhook auth still works
- route-family enablement does not change `/_synapse/external/openclaw/{service_id}/webhook`

### Feature and config gating tests

Keep tests or compile checks for:

- default build without `openclaw-routes`
- build with `--all-features`
- runtime disabled: route not mounted
- runtime enabled: route mounted

## Acceptance Criteria Before Exposure

The following criteria should remain true for continued exposure:

- route prefix stays in unstable Matrix namespace
- guest policy remains explicitly enforced
- public role listing semantics stay intentional and tested
- runtime enablement flag remains required
- integration tests remain green for auth, authorization, and visibility
- webhook regression remains independent and green
- docs stay aligned with the actual mounted behavior

## Recommended Decision

Current decision:

- keep `openclaw` isolated under feature and runtime gates
- keep webhook integration independent
- keep the route family on the unstable Matrix namespace only

Next implementation step:

- decide whether shadow-banned users need an explicit OpenClaw-specific deny policy
- preserve and expand the regression suite as behavior evolves
- only after a stable product contract exists evaluate whether any stable namespace should be introduced
