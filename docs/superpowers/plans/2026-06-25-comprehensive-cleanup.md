# Comprehensive Codebase Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce technical debt across 5 workstreams: split the 10K-line `api_doc.rs`, decompose route files >1000 lines, eliminate 131 production `unwrap()` calls, split the 45-field `AdminServices` god struct, and clean up `src/storage/` wrapper files.

**Architecture:** Five independent workstreams (A-E), each producing its own commit. No two workstreams touch the same file. Each task is independently verifiable with `cargo check --workspace --all-features` + `cargo test --test unit --features test-utils`. Workstreams follow existing codebase patterns: thin facades, sub-module decomposition by resource, `Result<T, ApiError>` error propagation.

**Tech Stack:** Rust 2021 edition, axum, utoipa (OpenAPI), sqlx, tokio. No new dependencies.

## Global Constraints

- `cargo clippy --all-features --locked -- -D warnings` must pass after each task
- `SQLX_OFFLINE=true` required for all build/test commands (pre-existing sqlx offline cache issue)
- `cargo test --test unit --features test-utils` must pass after each task
- `unwrap_used = "deny"` in Cargo.toml lints — no new unwrap calls
- `panic = "abort"` in Cargo.toml — expect() must not be used in production paths
- Files split by resource follow existing `room/` sub-module pattern: `mod.rs` re-exports, child modules contain handlers

---

## Workstream A: api_doc.rs Decomposition

### File Structure

```
src/web/api_doc.rs (10,466 lines) → src/web/api_doc/ (7 files, ~6800 lines total)
├── mod.rs             (~80 lines)  — #[derive(OpenApi)] struct, swagger_ui_router(), module declarations
├── health.rs          (~200 lines) — health check, versions, capabilities, well-known schemas + paths
├── auth.rs            (~800 lines) — login, register, token, account, refresh schemas + paths
├── client_server.rs   (~5000 lines)— C-S API: rooms, sync, search, push, tags, profile, presence, etc.
├── admin.rs           (~2500 lines)— admin user, room, media, server, federation schemas + paths
├── federation.rs      (~1200 lines)— federation key, event, transaction, membership schemas + paths
└── schemas.rs         (~600 lines) — shared request/response types used across domains
```

Each file gets `#![cfg(feature = "openapi-docs")]` at the top so individual items don't need per-item annotation.

### Task A1: Create `api_doc/` directory structure and `mod.rs`

**Files:**
- Create: `src/web/api_doc/mod.rs`
- Create: `src/web/api_doc/health.rs` (empty placeholder)
- Create: `src/web/api_doc/auth.rs` (empty placeholder)
- Create: `src/web/api_doc/client_server.rs` (empty placeholder)
- Create: `src/web/api_doc/admin.rs` (empty placeholder)
- Create: `src/web/api_doc/federation.rs` (empty placeholder)
- Create: `src/web/api_doc/schemas.rs` (empty placeholder)
- Modify: `src/web/mod.rs` — change `pub mod api_doc;` to `pub mod api_doc;` (no change needed if module path stays same)
- Delete: `src/web/api_doc.rs` (only after all content migrated)

- [ ] **Step 1: Create placeholder module files**

Create `src/web/api_doc/mod.rs`:
```rust
//! OpenAPI / Swagger documentation for the Synapse-Rust Matrix homeserver.
//!
//! Enabled via the `openapi-docs` feature flag. When enabled, the Swagger UI
//! is served at `/_swagger` and the OpenAPI JSON schema at `/_api-doc/openapi.json`.
//!
//! Route annotation is progressive — health, versions, capabilities, and
//! well-known endpoints are annotated as canonical examples. Additional routes
//! should be annotated incrementally through follow-up patches.

#![cfg(feature = "openapi-docs")]

pub mod admin;
pub mod auth;
pub mod client_server;
pub mod federation;
pub mod health;
pub mod schemas;

use crate::web::routes::AppState;

/// Build the Swagger UI router for the given OpenAPI schema.
///
/// The UI is mounted at `/_swagger` with a redirect from `/_swagger/` for
/// convenience. The raw OpenAPI JSON is served at `/_api-doc/openapi.json`.
#[cfg(feature = "openapi-docs")]
pub fn swagger_ui_router(_state: AppState) -> axum::Router<AppState> {
    use utoipa::OpenApi;
    use utoipa_swagger_ui::SwaggerUi;

    #[derive(OpenApi)]
    #[openapi(
        info(
            title = "Synapse-Rust Matrix Homeserver API",
            version = env!("CARGO_PKG_VERSION"),
            description = "Matrix Client-Server API implementation in Rust. \
                Compliant with Matrix Spec v1.13."
        ),
        servers(
            (url = "/", description = "Local Synapse-Rust instance"),
        ),
        tags(
            (name = "Health", description = "Server health and version endpoints"),
            (name = "Authentication", description = "Login, registration, and token management"),
            (name = "Client-Server", description = "Matrix Client-Server API (v3)"),
            (name = "Admin", description = "Server administration endpoints"),
            (name = "Federation", description = "Server-to-server federation API"),
        ),
        paths(
            // Imported from sub-modules below
        ),
    )]
    struct ApiDoc;

    SwaggerUi::new("/_swagger")
        .url("/_api-doc/openapi.json", ApiDoc::openapi())
        .into_router()
}
```

Create placeholder files for each domain module:
```rust
// src/web/api_doc/health.rs
#![cfg(feature = "openapi-docs")]
```

```rust
// src/web/api_doc/auth.rs
#![cfg(feature = "openapi-docs")]
```

```rust
// src/web/api_doc/client_server.rs
#![cfg(feature = "openapi-docs")]
```

```rust
// src/web/api_doc/admin.rs
#![cfg(feature = "openapi-docs")]
```

```rust
// src/web/api_doc/federation.rs
#![cfg(feature = "openapi-docs")]
```

```rust
// src/web/api_doc/schemas.rs
#![cfg(feature = "openapi-docs")]
```

- [ ] **Step 2: Verify compilation with feature flag**

Run: `SQLX_OFFLINE=true cargo check --features openapi-docs 2>&1 | tail -5`
Expected: `Finished` — compiles with feature flag (no content yet, just module structure)

- [ ] **Step 3: Commit**

```bash
git add src/web/api_doc/ src/web/api_doc.rs
git commit -m "chore: create api_doc/ directory structure, move mod.rs scaffold"
```

### Task A2: Migrate health + schemas from api_doc.rs

**Files:**
- Modify: `src/web/api_doc/schemas.rs` — move all `#[derive(ToSchema)]` struct definitions
- Modify: `src/web/api_doc/health.rs` — move health/version/well-known path functions
- Modify: `src/web/api_doc/mod.rs` — add path imports, remove migrated items from old file
- Modify: `src/web/api_doc.rs` — delete migrated content

- [ ] **Step 1: Extract schemas from api_doc.rs into schemas.rs**

Read `src/web/api_doc.rs`, find all `#[derive(utoipa::ToSchema)]` struct definitions and move them to `src/web/api_doc/schemas.rs` with `pub` visibility. These include:

```rust
use crate::web::routes::AppState;

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct ApiHealthCheckResult {
    status: String,
    message: String,
    duration_ms: u64,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct ApiHealthStatus {
    status: String,
    version: String,
    timestamp: i64,
    checks: std::collections::HashMap<String, ApiHealthCheckResult>,
}

// ... all other ToSchema structs from api_doc.rs
```

- [ ] **Step 2: Extract health path functions into health.rs**

Move all health/version/capabilities/well-known path functions from `api_doc.rs` into `health.rs`:
```rust
// src/web/api_doc/health.rs
#![cfg(feature = "openapi-docs")]

use crate::web::routes::AppState;
use super::schemas::*;
use axum::response::IntoResponse;

#[utoipa::path(
    get,
    path = "/health",
    responses((status = 200, body = ApiHealthStatus)),
    tag = "Health",
)]
pub async fn health_check(_state: AppState) -> impl IntoResponse {
    unimplemented!()
}

#[utoipa::path(
    get,
    path = "/_matrix/client/versions",
    responses((status = 200, body = ApiVersionsResponse)),
    tag = "Health",
)]
pub async fn get_client_versions(_state: AppState) -> impl IntoResponse {
    unimplemented!()
}

// ... all other health/version/well-known path functions
```

- [ ] **Step 3: Update mod.rs to import health paths**

Update the `paths(...)` list in `mod.rs`:
```rust
paths(
    health::health_check,
    health::detailed_health_check,
    health::get_client_versions,
    health::get_server_version,
    health::get_capabilities,
    health::get_well_known_server,
    health::get_well_known_client,
    health::get_well_known_support,
)
```

- [ ] **Step 4: Delete migrated content from api_doc.rs**

Remove the schemas and health path functions from `src/web/api_doc.rs`.

- [ ] **Step 5: Verify compilation**

Run: `SQLX_OFFLINE=true cargo check --features openapi-docs 2>&1 | tail -5`
Expected: `Finished` — openapi-docs feature compiles

- [ ] **Step 6: Commit**

```bash
git add src/web/api_doc/
git commit -m "chore: extract health + schemas from api_doc.rs into sub-modules"
```

### Task A3: Migrate auth path functions

**Files:**
- Modify: `src/web/api_doc/auth.rs`
- Modify: `src/web/api_doc/mod.rs` — update paths(...)
- Modify: `src/web/api_doc.rs` — delete migrated content

- [ ] **Step 1: Move auth path functions**

Move from `api_doc.rs` → `auth.rs` all authentication-related paths: login, register, token refresh, logout, change password, deactivate account, 3PID management, whoami, etc.

- [ ] **Step 2: Update mod.rs paths(...)**

Replace auth-related path entries in the master list with `auth::*` imports.

- [ ] **Step 3: Delete from api_doc.rs**

- [ ] **Step 4: Verify**

Run: `SQLX_OFFLINE=true cargo check --features openapi-docs 2>&1 | tail -5`
Expected: `Finished`

- [ ] **Step 5: Commit**

```bash
git add src/web/api_doc/
git commit -m "chore: extract auth paths from api_doc.rs into api_doc/auth.rs"
```

### Task A4: Migrate admin path functions

**Files:**
- Modify: `src/web/api_doc/admin.rs`
- Modify: `src/web/api_doc/mod.rs`
- Modify: `src/web/api_doc.rs`

- [ ] **Step 1: Move admin path functions** (~2500 lines)
- [ ] **Step 2: Update mod.rs paths(...)**
- [ ] **Step 3: Delete from api_doc.rs**
- [ ] **Step 4: Verify** — `SQLX_OFFLINE=true cargo check --features openapi-docs`
- [ ] **Step 5: Commit**

```bash
git add src/web/api_doc/
git commit -m "chore: extract admin paths from api_doc.rs into api_doc/admin.rs"
```

### Task A5: Migrate federation path functions

**Files:**
- Modify: `src/web/api_doc/federation.rs`
- Modify: `src/web/api_doc/mod.rs`
- Modify: `src/web/api_doc.rs`

- [ ] **Step 1: Move federation path functions** (~1200 lines)
- [ ] **Step 2: Update mod.rs paths(...)**
- [ ] **Step 3: Delete from api_doc.rs**
- [ ] **Step 4: Verify** — `SQLX_OFFLINE=true cargo check --features openapi-docs`
- [ ] **Step 5: Commit**

```bash
git add src/web/api_doc/
git commit -m "chore: extract federation paths from api_doc.rs into api_doc/federation.rs"
```

### Task A6: Migrate client_server path functions and finalize

**Files:**
- Modify: `src/web/api_doc/client_server.rs`
- Modify: `src/web/api_doc/mod.rs`
- Modify: `src/web/api_doc.rs` — final deletion of old monolithic file

- [ ] **Step 1: Move client_server path functions** (~5000 lines)
- [ ] **Step 2: Update mod.rs paths(...)**
- [ ] **Step 3: Delete api_doc.rs — the old 10466-line monolithic file**
- [ ] **Step 4: Verify** — `SQLX_OFFLINE=true cargo check --features openapi-docs`
- [ ] **Step 5: Run full verification**

```bash
SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings
SQLX_OFFLINE=true cargo test --test unit --features test-utils
```

Expected: clippy clean, all unit tests pass.

- [ ] **Step 6: Commit**

```bash
git rm src/web/api_doc.rs
git add src/web/api_doc/
git commit -m "chore: complete api_doc.rs decomposition into 7 sub-modules"
```

---

## Workstream B: Route File Splitting

### Task B1: Split `e2ee_routes.rs` (1489 lines)

**Files:**
- Create: `src/web/routes/e2ee/mod.rs`
- Create: `src/web/routes/e2ee/keys.rs`
- Create: `src/web/routes/e2ee/backup.rs`
- Create: `src/web/routes/e2ee/devices.rs`
- Modify: `src/web/routes/mod.rs` — change `pub mod e2ee_routes;` to `pub mod e2ee;`
- Delete: `src/web/routes/e2ee_routes.rs`

**Interfaces:**
- Produces: `pub fn create_e2ee_compat_router() -> Router<AppState>`, `pub fn create_e2ee_v3_only_router() -> Router<AppState>` (in `mod.rs`)

- [ ] **Step 1: Create `e2ee/mod.rs` with re-exports from sub-modules**

Read the original `e2ee_routes.rs` to identify all public handlers and sub-routers. Create:

```rust
// src/web/routes/e2ee/mod.rs
mod backup;
mod devices;
pub mod keys;

use super::AppState;
use axum::Router;

pub fn create_e2ee_routes() -> Router<AppState> {
    Router::new()
        .merge(keys::create_keys_router())
        .merge(backup::create_backup_router())
        .merge(devices::create_devices_router())
}
```

- [ ] **Step 2: Move key-related handlers to `keys.rs`**

```rust
// src/web/routes/e2ee/keys.rs
use super::super::{AppState, AuthenticatedUser, MatrixJson};
use crate::ApiError;

pub fn create_keys_router() -> Router<AppState> {
    // upload_keys, query_keys, claim_keys, etc.
}
```

- [ ] **Step 3: Move backup-related handlers to `backup.rs`**

```rust
// src/web/routes/e2ee/backup.rs
use super::super::{AppState, AuthenticatedUser, MatrixJson};
use crate::ApiError;
use crate::e2ee::secure_backup::RestoreSecureBackupRequest;

pub fn create_backup_router() -> Router<AppState> {
    // secure backup endpoints
}
```

- [ ] **Step 4: Move device-related handlers to `devices.rs`**

```rust
// src/web/routes/e2ee/devices.rs
use super::super::{AppState, AuthenticatedUser, MatrixJson};
use crate::ApiError;

pub fn create_devices_router() -> Router<AppState> {
    // device list, device verification endpoints
}
```

- [ ] **Step 5: Update `src/web/routes/mod.rs`**

Change:
```rust
pub mod e2ee_routes;
```
To:
```rust
pub mod e2ee;
```

- [ ] **Step 6: Update `assembly.rs` reference**

Find `use ... e2ee_routes::...` in `src/web/routes/assembly.rs` and update to `e2ee::...`.

- [ ] **Step 7: Verify**

Run: `SQLX_OFFLINE=true cargo check --workspace --all-features 2>&1 | tail -5`
Expected: `Finished`

- [ ] **Step 8: Run unit tests**

Run: `SQLX_OFFLINE=true cargo test --test unit --features test-utils 2>&1 | tail -5`
Expected: all tests pass

- [ ] **Step 9: Delete old file and commit**

```bash
git rm src/web/routes/e2ee_routes.rs
git add src/web/routes/e2ee/ src/web/routes/mod.rs
git commit -m "refactor: split e2ee_routes.rs into e2ee/ sub-modules by resource"
```

### Task B2: Split `federation/membership.rs` (1192 lines)

**Files:**
- Create: `src/web/routes/federation/membership/mod.rs`
- Create: `src/web/routes/federation/membership/invite.rs`
- Create: `src/web/routes/federation/membership/join.rs`
- Create: `src/web/routes/federation/membership/leave.rs`
- Create: `src/web/routes/federation/membership/knock.rs`
- Delete: `src/web/routes/federation/membership.rs`

**Interfaces:**
- Produces: `pub fn create_membership_router() -> Router<AppState>` (in `mod.rs`)

- [ ] **Step 1: Create directory + mod.rs**
- [ ] **Step 2: Move invite handlers to invite.rs**
- [ ] **Step 3: Move join handlers to join.rs**
- [ ] **Step 4: Move leave handlers to leave.rs**
- [ ] **Step 5: Move knock handlers to knock.rs**
- [ ] **Step 6: Update references in `federation/mod.rs`**
- [ ] **Step 7: Verify** — `SQLX_OFFLINE=true cargo check --workspace --all-features`
- [ ] **Step 8: Run unit tests** — `SQLX_OFFLINE=true cargo test --test unit --features test-utils`
- [ ] **Step 9: Commit**

```bash
git rm src/web/routes/federation/membership.rs
git add src/web/routes/federation/membership/
git commit -m "refactor: split federation membership.rs into invite/join/leave/knock sub-modules"
```

### Task B3: Split `room/management.rs` (1159 lines)

**Files:**
- Create: `src/web/routes/handlers/room/management/mod.rs`
- Create: `src/web/routes/handlers/room/management/create.rs`
- Create: `src/web/routes/handlers/room/management/upgrade.rs`
- Create: `src/web/routes/handlers/room/management/delete.rs`
- Delete: `src/web/routes/handlers/room/management.rs`

**Interfaces:**
- Produces: `pub fn create_room_management_router() -> Router<AppState>` (in `mod.rs`)

- [ ] **Step 1: Create directory + mod.rs**
- [ ] **Step 2: Move create room handlers to create.rs**
- [ ] **Step 3: Move upgrade room handlers to upgrade.rs**
- [ ] **Step 4: Move delete/leave handlers to delete.rs**
- [ ] **Step 5: Update references in `room/mod.rs`**
- [ ] **Step 6: Verify** — `SQLX_OFFLINE=true cargo check --workspace --all-features`
- [ ] **Step 7: Run unit tests** — `SQLX_OFFLINE=true cargo test --test unit --features test-utils`
- [ ] **Step 8: Commit**

```bash
git rm src/web/routes/handlers/room/management.rs
git add src/web/routes/handlers/room/management/
git commit -m "refactor: split room management.rs into create/upgrade/delete sub-modules"
```

### Task B4: Split `media.rs` (1153 lines)

**Files:**
- Create: `src/web/routes/media/mod.rs`
- Create: `src/web/routes/media/download.rs`
- Create: `src/web/routes/media/upload.rs`
- Create: `src/web/routes/media/thumbnail.rs`
- Delete: `src/web/routes/media.rs`

**Interfaces:**
- Produces: `pub fn create_media_routes() -> Router<AppState>` (in `mod.rs`)

- [ ] **Step 1: Create directory + mod.rs**
- [ ] **Step 2: Move download handlers**
- [ ] **Step 3: Move upload handlers**
- [ ] **Step 4: Move thumbnail handlers**
- [ ] **Step 5: Update references**
- [ ] **Step 6: Verify**
- [ ] **Step 7: Commit**

```bash
git rm src/web/routes/media.rs
git add src/web/routes/media/
git commit -m "refactor: split media.rs into download/upload/thumbnail sub-modules"
```

### Task B5: Split `oidc.rs` (1124 lines)

**Files:**
- Create: `src/web/routes/oidc/mod.rs`
- Create: `src/web/routes/oidc/callback.rs`
- Create: `src/web/routes/oidc/discovery.rs`
- Delete: `src/web/routes/oidc.rs`

- [ ] **Step 1: Create directory + mod.rs**
- [ ] **Step 2: Move callback handlers to callback.rs**
- [ ] **Step 3: Move discovery/registration handlers to discovery.rs**
- [ ] **Step 4: Update references**
- [ ] **Step 5: Verify**
- [ ] **Step 6: Commit**

```bash
git rm src/web/routes/oidc.rs
git add src/web/routes/oidc/
git commit -m "refactor: split oidc.rs into callback/discovery sub-modules"
```

### Task B6: Split `handlers/search.rs` (1092 lines)

**Files:**
- Create: `src/web/routes/handlers/search/mod.rs`
- Create: `src/web/routes/handlers/search/user.rs`
- Create: `src/web/routes/handlers/search/room.rs`
- Delete: `src/web/routes/handlers/search.rs`

- [ ] **Step 1: Create directory + mod.rs**
- [ ] **Step 2: Move user search handlers to user.rs**
- [ ] **Step 3: Move room/space search handlers to room.rs**
- [ ] **Step 4: Update references**
- [ ] **Step 5: Verify**
- [ ] **Step 6: Commit**

```bash
git rm src/web/routes/handlers/search.rs
git add src/web/routes/handlers/search/
git commit -m "refactor: split search.rs into user/room sub-modules"
```

### Task B7: Split `friend_room.rs` (1054 lines)

**Files:**
- Create: `src/web/routes/friend_room/mod.rs`
- Create: `src/web/routes/friend_room/create.rs`
- Create: `src/web/routes/friend_room/manage.rs`
- Delete: `src/web/routes/friend_room.rs`

- [ ] **Step 1: Create directory + mod.rs**
- [ ] **Step 2: Move create/invite handlers to create.rs**
- [ ] **Step 3: Move manage/update/delete handlers to manage.rs**
- [ ] **Step 4: Update references**
- [ ] **Step 5: Verify**
- [ ] **Step 6: Commit**

```bash
git rm src/web/routes/friend_room.rs
git add src/web/routes/friend_room/
git commit -m "refactor: split friend_room.rs into create/manage sub-modules"
```

### Task B8: Split `handlers/room/events.rs` (990 lines)

**Files:**
- Create: `src/web/routes/handlers/room/events/mod.rs`
- Create: `src/web/routes/handlers/room/events/send.rs`
- Create: `src/web/routes/handlers/room/events/state.rs`
- Create: `src/web/routes/handlers/room/events/redact.rs`
- Delete: `src/web/routes/handlers/room/events.rs`

- [ ] **Step 1: Create directory + mod.rs**
- [ ] **Step 2: Move send event handlers to send.rs**
- [ ] **Step 3: Move state event handlers to state.rs**
- [ ] **Step 4: Move redact event handlers to redact.rs**
- [ ] **Step 5: Update references**
- [ ] **Step 6: Verify**
- [ ] **Step 7: Commit**

```bash
git rm src/web/routes/handlers/room/events.rs
git add src/web/routes/handlers/room/events/
git commit -m "refactor: split room events.rs into send/state/redact sub-modules"
```

---

## Workstream C: unwrap() Elimination

### Task C1: Eliminate unwrap() in `security/invite_signature.rs` (11 instances)

**Files:**
- Modify: `src/security/invite_signature.rs`

- [ ] **Step 1: Read the file and identify all unwrap() calls**

Run: `grep -n '\.unwrap()' src/security/invite_signature.rs`

- [ ] **Step 2: Replace each unwrap() with proper error handling**

Pattern:
```rust
// Before:
let key = hmac_key.as_ref().unwrap();

// After:
let key = hmac_key.as_ref().ok_or_else(|| ApiError::internal("HMAC key not initialized"))?;
```

For HMAC operations that can't fail at runtime but use `unwrap()`:
```rust
// Before:
let hmac = Hmac::<Sha256>::new_from_slice(key).unwrap();

// After:
let hmac = Hmac::<Sha256>::new_from_slice(key)
    .map_err(|e| ApiError::internal_with_log("HMAC init failed", &e))?;
```

- [ ] **Step 3: Verify compilation**

Run: `SQLX_OFFLINE=true cargo check --workspace 2>&1 | grep -E 'error|warning' | head -10`
Expected: no errors related to this file

- [ ] **Step 4: Run unit tests**

Run: `SQLX_OFFLINE=true cargo test --test unit --features test-utils 2>&1 | tail -5`
Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add src/security/invite_signature.rs
git commit -m "fix: eliminate unwrap() calls in invite_signature.rs"
```

### Task C2: Eliminate unwrap() in `security/device_binding.rs` (10 instances)

**Files:**
- Modify: `src/security/device_binding.rs`

- [ ] **Step 1: Identify unwrap() locations**
- [ ] **Step 2: Replace with Result<T, ApiError>**
- [ ] **Step 3: Verify**
- [ ] **Step 4: Commit**

```bash
git add src/security/device_binding.rs
git commit -m "fix: eliminate unwrap() calls in device_binding.rs"
```

### Task C3: Eliminate unwrap() in `web/utils/auth.rs` (11 instances)

**Files:**
- Modify: `src/web/utils/auth.rs`

- [ ] **Step 1: Identify unwrap() locations**
- [ ] **Step 2: Replace with Result<T, ApiError>**
- [ ] **Step 3: Verify**
- [ ] **Step 4: Commit**

```bash
git add src/web/utils/auth.rs
git commit -m "fix: eliminate unwrap() calls in auth.rs"
```

### Task C4: Eliminate unwrap() in `admin/register.rs` (13 instances)

**Files:**
- Modify: `src/web/routes/admin/register.rs`

- [ ] **Step 1: Identify unwrap() locations**
- [ ] **Step 2: Replace with Result<T, ApiError>**
- [ ] **Step 3: Verify**
- [ ] **Step 4: Commit**

```bash
git add src/web/routes/admin/register.rs
git commit -m "fix: eliminate unwrap() calls in admin/register.rs"
```

### Task C5: Eliminate unwrap() in `extractors/auth.rs` (8 instances)

**Files:**
- Modify: `src/web/routes/extractors/auth.rs`

- [ ] **Step 1: Replace each unwrap()**
- [ ] **Step 2: Verify**
- [ ] **Step 3: Commit**

```bash
git add src/web/routes/extractors/auth.rs
git commit -m "fix: eliminate unwrap() calls in extractors/auth.rs"
```

### Task C6: Eliminate unwrap() in remaining files (batch)

**Files:**
- Modify: `src/web/routes/room_summary.rs` (7)
- Modify: `src/web/routes/media.rs` (6)
- Modify: `src/web/routes/extractors/json.rs` (6)
- Modify: `src/web/routes/friend_room.rs` (5)
- Modify: `src/web/routes/dm.rs` (5)
- Modify: `src/web/streaming.rs` (6)
- Modify: `src/web/routes/push_rules.rs` (4)
- Modify: `src/web/routes/oidc.rs` (4)
- Modify: `src/web/routes/federation/keys.rs` (3)
- Modify: `src/web/filter.rs` (3)
- Modify: `src/web/routes/voip.rs` (2)
- Modify: `src/web/routes/ledger_export.rs` (2)
- Modify: `src/web/routes/space/types.rs` (1)
- Modify: `src/web/routes/space/children_hierarchy.rs` (1)

- [ ] **Step 1: Replace all remaining unwrap() calls**
- [ ] **Step 2: Verify**

```bash
SQLX_OFFLINE=true cargo check --workspace --all-features 2>&1 | tail -5
```

- [ ] **Step 3: Run clippy**

```bash
SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings 2>&1 | tail -5
```

Expected: `Finished` (no warnings)

- [ ] **Step 4: Run unit tests**

```bash
SQLX_OFFLINE=true cargo test --test unit --features test-utils 2>&1 | tail -5
```

Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add src/web/
git commit -m "fix: eliminate remaining production unwrap() calls across 14 route files"
```

---

## Workstream D: AdminServices Decomposition

### Task D1: Split AdminServices into sub-structs

**Files:**
- Modify: `synapse-services/src/container.rs` — split AdminServices into 5 sub-structs + update assembly
- Modify: all files referencing `state.services.admin.X` — update field paths

**Target structures:**

```rust
pub struct AdminUserServices {
    pub admin_registration_service: crate::admin_registration_service::AdminRegistrationService,
    pub admin_user_service: Arc<crate::admin_user_service::AdminUserService>,
    pub email_verification_storage: EmailVerificationStorage,
    pub email_verification_service: Arc<crate::email_verification_service::EmailVerificationService>,
}

pub struct AdminFederationServices {
    pub admin_federation_service: Arc<crate::admin_federation_service::AdminFederationService>,
    pub federation_blacklist_storage: synapse_storage::federation_blacklist::FederationBlacklistStorage,
    pub federation_blacklist_service: Arc<crate::federation_blacklist_service::FederationBlacklistService>,
}

pub struct AdminMediaServices {
    pub admin_media_service: Arc<crate::admin_media_service::AdminMediaService>,
    pub media_quota_storage: synapse_storage::media_quota::MediaQuotaStorage,
    pub media_quota_service: Arc<crate::media_quota_service::MediaQuotaService>,
}

pub struct AdminSecurityServices {
    pub admin_security_service: Arc<crate::admin_security_service::AdminSecurityService>,
    pub admin_server_service: Arc<crate::admin_server_service::AdminServerService>,
    pub telemetry_alert_service: Arc<crate::telemetry_service::TelemetryAlertService>,
    pub admin_audit_service: Arc<crate::admin_audit_service::AdminAuditService>,
    pub audit_storage: synapse_storage::audit::AuditEventStorage,
    pub captcha_storage: synapse_storage::captcha::CaptchaStorage,
    pub captcha_service: Arc<crate::captcha_service::CaptchaService>,
    pub admin_token_service: Arc<crate::admin_token_service::AdminTokenService>,
    pub refresh_token_storage: synapse_storage::refresh_token::RefreshTokenStorage,
    pub refresh_token_service: Arc<crate::refresh_token_service::RefreshTokenService>,
    pub registration_token_storage: synapse_storage::registration_token::RegistrationTokenStorage,
    pub registration_token_service: Arc<crate::registration_token_service::RegistrationTokenService>,
}

pub struct AdminModuleServices {
    pub feature_flag_storage: synapse_storage::feature_flags::FeatureFlagStorage,
    pub feature_flag_service: Arc<crate::feature_flag_service::FeatureFlagService>,
    pub event_report_storage: synapse_storage::event_report::EventReportStorage,
    pub event_report_service: Arc<crate::event_report_service::EventReportService>,
    pub background_update_storage: synapse_storage::background_update::BackgroundUpdateStorage,
    pub background_update_service: Arc<crate::background_update_service::BackgroundUpdateService>,
    pub module_storage: synapse_storage::module::ModuleStorage,
    pub module_service: Arc<crate::module_service::ModuleService>,
    pub account_validity_service: Arc<crate::module_service::AccountValidityService>,
    pub retention_storage: synapse_storage::retention::RetentionStorage,
    pub retention_service: Arc<crate::retention_service::RetentionService>,
    pub push_notification_storage: synapse_storage::push_notification::PushNotificationStorage,
    pub push_notification_service: Arc<crate::push_notification_service::PushNotificationService>,
    pub app_service_storage: ApplicationServiceStorage,
    pub app_service_manager: Arc<crate::application_service::ApplicationServiceManager>,
    pub app_service_scheduler: Arc<crate::application_service::ApplicationServiceScheduler>,
    #[cfg(feature = "external-services")]
    pub external_service_integration: Arc<crate::external_service_integration::ExternalServiceIntegration>,
    pub rendezvous_storage: synapse_storage::rendezvous::RendezvousStorage,
    pub rendezvous_message_storage: synapse_storage::rendezvous::RendezvousMessageStorage,
}
```

**Updated AdminServices as aggregation:**
```rust
pub struct AdminServices {
    pub user: AdminUserServices,
    pub federation: AdminFederationServices,
    pub media: AdminMediaServices,
    pub security: AdminSecurityServices,
    pub module: AdminModuleServices,
}
```

- [ ] **Step 1: Define the 5 sub-structs and update AdminServices in container.rs**

- [ ] **Step 2: Update `assemble_admin_support()` to return sub-structs**

- [ ] **Step 3: Update all references from `state.services.admin.X` to `state.services.admin.subgroup.X`**

Run to find all references:
```bash
grep -rn '\.admin\.' src/ --include='*.rs' | grep -v target/ | head -50
```

Update each reference:
- `admin.admin_registration_service` → `admin.user.admin_registration_service`
- `admin.admin_federation_service` → `admin.federation.admin_federation_service`
- `admin.admin_media_service` → `admin.media.admin_media_service`
- `admin.admin_security_service` → `admin.security.admin_security_service`
- `admin.module_storage` → `admin.module.module_storage`
- etc.

- [ ] **Step 4: Verify compilation**

```bash
SQLX_OFFLINE=true cargo check --workspace --all-features 2>&1 | tail -5
```

- [ ] **Step 5: Run unit tests**

```bash
SQLX_OFFLINE=true cargo test --test unit --features test-utils 2>&1 | tail -5
```

- [ ] **Step 6: Commit**

```bash
git add synapse-services/src/container.rs
git add src/web/routes/
git commit -m "refactor: split AdminServices (45 fields) into 5 domain sub-structs"
```

---

## Workstream E: Storage Wrapper Cleanup

### Task E1: Reduce multi-line storage wrappers to pure facades

**Files** (20 files at 4-8 lines):
- `src/storage/admin_media.rs` — 4 lines → 1 line
- `src/storage/application_service.rs` — 5 lines → reduce
- `src/storage/audit.rs` — 4 lines → 1 line
- `src/storage/background_update.rs` — 4 lines → 1 line
- `src/storage/beacon.rs` — 4 lines → 1 line
- `src/storage/captcha.rs` — 4 lines → 1 line
- `src/storage/cas.rs` — 5 lines → 1 line
- `src/storage/event_report.rs` — 4 lines → reduce
- `src/storage/feature_flags.rs` — 4 lines → 1 line
- `src/storage/federation_blacklist.rs` — 5 lines → reduce
- `src/storage/friend_room.rs` — 4 lines → reduce
- `src/storage/matrixrtc.rs` — 4 lines → 1 line
- `src/storage/media_quota.rs` — 6 lines → reduce
- `src/storage/moderation.rs` — 4 lines → 1 line
- `src/storage/module.rs` — 6 lines → reduce
- `src/storage/monitoring.rs` — 4 lines → 1 line
- `src/storage/openclaw.rs` — 5 lines → reduce
- `src/storage/refresh_token.rs` — 4 lines → 1 line
- `src/storage/registration_token.rs` — 8 lines → reduce
- `src/storage/relations.rs` — 5 lines → reduce
- `src/storage/retention.rs` — 5 lines → reduce
- `src/storage/saml.rs` — 5 lines → reduce
- `src/storage/server_notification.rs` — 8 lines → reduce
- `src/storage/sliding_sync.rs` — 5 lines → reduce
- `src/storage/space.rs` — 5 lines → reduce
- `src/storage/thread.rs` — 4 lines → 1 line
- `src/storage/user.rs` — 4 lines → 1 line

**Pattern:** For each file, if the content is only `pub use synapse_storage::module::Type1, Type2, ...;`, verify that consumers already import directly from `synapse_storage` and reduce to `pub use synapse_storage::module::*;`.

- [ ] **Step 1: For each file, check consumer imports**

For each file, e.g. `src/storage/admin_media.rs`:
```bash
grep -r 'crate::storage::admin_media::' src/ --include='*.rs'
```

If no direct references to `crate::storage::admin_media::Type`, the extra type re-exports are unused and the file can be reduced to:
```rust
pub use synapse_storage::admin_media::*;
```

- [ ] **Step 2: Reduce each file to 1-line facade**
- [ ] **Step 3: Verify compilation**

```bash
SQLX_OFFLINE=true cargo check --workspace --all-features 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add src/storage/
git commit -m "chore: reduce storage wrapper files to pure 1-line facades"
```

---

## Final Verification

### Task F1: Full workspace verification

- [ ] **Step 1: Clippy**

```bash
SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings 2>&1
```
Expected: `Finished` (no warnings, no errors)

- [ ] **Step 2: Unit tests**

```bash
SQLX_OFFLINE=true cargo test --test unit --features test-utils 2>&1 | tail -10
```
Expected: 862 passed; 0 failed

- [ ] **Step 3: Integration tests**

```bash
SQLX_OFFLINE=true cargo test --test integration --features test-utils -- --test-threads=4 2>&1 | tail -20
```
Expected: all passed

- [ ] **Step 4: Full build check**

```bash
SQLX_OFFLINE=true cargo check --workspace --all-features 2>&1 | tail -5
```
Expected: `Finished`

- [ ] **Step 5: Verify no remaining production unwrap()**

```bash
grep -rn '\.unwrap()' src/ --include='*.rs' | grep -v tests/ | grep -v '#\[cfg(test)\]' | grep -v 'test_utils' | wc -l
```
Expected: `0`

- [ ] **Step 6: Commit**

```bash
git commit --allow-empty -m "chore: comprehensive cleanup verification complete

- api_doc.rs: 10,466 lines → 7 files in api_doc/ (~6800 lines)
- Route files: 8 files >1000 lines split into domain sub-modules
- unwrap(): 131 production unwrap() calls eliminated
- AdminServices: 45 fields split into 5 domain sub-structs
- Storage wrappers: 20 files reduced to 1-line facades"
```
