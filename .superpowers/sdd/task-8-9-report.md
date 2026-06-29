# Task 8+9 Report: Migrate Handlers from State<AppState> to Typed Context Structs

## Summary

Migrated 5 handler files from `State<AppState>` to typed context structs (`RoomContext`, `SyncContext`, `AuthContext`, `DeviceContext`). Fixed 16 files total (including pre-existing partially-migrated files) to achieve clean compilation.

## Files Migrated

| File | Old State Extract | New State Extract |
|---|---|---|
| `src/web/routes/handlers/sync.rs` | `State(state): State<AppState>` | `State(ctx): State<SyncContext>` |
| `src/web/routes/handlers/room/state.rs` | `State(state): State<AppState>` | `State(ctx): State<RoomContext>` |
| `src/web/routes/handlers/room/members.rs` | `State(state): State<AppState>` | `State(ctx): State<RoomContext>` |
| `src/web/routes/handlers/room/management.rs` | `State(state): State<AppState>` | `State(ctx): State<RoomContext>` |
| `src/web/routes/handlers/room/events.rs` | `State(state): State<AppState>` | `State(ctx): State<RoomContext>` |

## Supporting Changes

- **`context.rs`**: Added `config` field to `AuthContext` (needed by auth handlers that access config values like server URL, federation port, OIDC settings)
- **`room_access.rs`**: Added `RoomContext`-based helpers (`ensure_room_member_ctx`, `ensure_room_member_strict_ctx`, `is_member_ctx`, `is_member_or_creator_ctx`)
- **`auth_discovery.rs`**: Inlined OIDC helpers (`oidc_available`, `build_oidc_discovery`) to avoid `AppState` dependency
- **`versions.rs`**, **`client_config.rs`**: Migrated to `State<AuthContext>`
- **`e2ee.rs`**, **`receipts.rs`**: Fixed state variable references (pre-existing partial migration)
- **`routes/mod.rs`**, **`routes/state.rs`**: Updated re-exports and state routing

## Key Technical Details

- Uses Axum's `State` extractor + `FromRef<AppState>` pattern for type-safe handler dependency injection
- Rate limit config: `resolve_rate_limit_override` returns individual `u32` fields instead of structs to avoid type mismatch between `SyncRateLimitConfigFile` and `SyncRateLimitConfig`
- Auth handlers (versions, client_config, auth_discovery) use `AuthContext` which now carries a `config` field for accessing server configuration
- Build verified: `cargo build --locked` — 0 errors, warnings only for unused functions

## Files Modified (16 total)

- `src/web/routes/context.rs` — Added config field to AuthContext
- `src/web/routes/room_access.rs` — RoomContext-based helpers
- `src/web/routes/mod.rs` — Updated re-exports
- `src/web/routes/state.rs` — Updated state routing
- `src/web/routes/extractors/auth.rs` — FromRequestParts impls
- `src/web/routes/handlers/room/mod.rs` — Updated helpers
- `src/web/routes/handlers/room/state.rs` — Migrated to RoomContext
- `src/web/routes/handlers/room/members.rs` — Migrated to RoomContext
- `src/web/routes/handlers/room/management.rs` — Migrated to RoomContext
- `src/web/routes/handlers/room/events.rs` — Migrated to RoomContext
- `src/web/routes/handlers/room/e2ee.rs` — Fixed state references
- `src/web/routes/handlers/room/receipts.rs` — Fixed state references
- `src/web/routes/handlers/sync.rs` — Migrated to SyncContext
- `src/web/routes/handlers/versions.rs` — Migrated to AuthContext
- `src/web/routes/handlers/client_config.rs` — Migrated to AuthContext
- `src/web/routes/handlers/auth_discovery.rs` — Migrated to AuthContext
