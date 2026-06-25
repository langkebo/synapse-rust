<!-- /autoplan restore point: /Users/ljf/.gstack/projects/langkebo-synapse-rust/main-autoplan-restore-20260625-223058.md -->
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

---

## GSTACK REVIEW REPORT

**Review date:** 2026-06-25
**Pipeline:** /autoplan (single-model — Codex unavailable)
**Mode:** HOLD SCOPE (appropriate for refactoring)
**Reviewer:** Claude Opus 4.7

---

### Phase 1: CEO Review — Strategy & Scope

**Mode confirmation:** HOLD SCOPE is correct. This is a technical debt cleanup plan — no new features, no user-facing changes, no scope expansion warranted. Each workstream is independently verifiable and produces its own commit.

#### Step 0A: Premise Challenge

Five premises evaluated:

| # | Premise | Verdict |
|---|---------|---------|
| P1 | api_doc.rs at 10K lines is unmaintainable | **CONFIRMED** — file was 10,466 lines before split; single-file god modules this size make grep/IDE navigation painful |
| P2 | Route files >1000 lines should be split by resource | **CONFIRMED** — existing pattern in `src/web/routes/handlers/room/` validates the approach; B1 and membership.rs decomposition follow same pattern |
| P3 | 131 production `unwrap()` calls are technical debt | **CONFIRMED** — `unwrap_used = "deny"` in Cargo.toml lints makes this a policy violation, not just a style preference |
| P4 | 45-field AdminServices struct is a god object | **CONFIRMED** — 45 fields in one struct is a coupling smell; 5 domain sub-structs mirror the codebase's own layering |
| P5 | Storage wrapper files should be 1-line facades | **CONFIRMED** — 27 files at 4-8 lines doing only `pub use` re-exports; consumers already import from `synapse_storage` directly |

All premises sound. No challenges raised.

#### Step 0B: Existing Code Leverage Map

| Sub-problem | Existing Code | Leverage |
|-------------|---------------|----------|
| File splitting pattern | `src/web/routes/handlers/room/` sub-modules | Reuse `mod.rs` + child module structure |
| OpenAPI path annotation | utoipa `#[utoipa::path]` macro | No new annotation framework needed |
| unwrap → Result | `ApiError::internal()`, `ApiError::internal_with_log()` | Existing error constructors, no new error types |
| Struct decomposition | `ServiceContainer` field grouping pattern | Reuse same domain-boundary logic |
| Storage facade thinning | `synapse_storage::*` crate re-exports | Consumers already import directly |

#### Step 0C: Dream State Delta

```
CURRENT STATE                    THIS PLAN                    12-MONTH IDEAL
─────────────────────────────────────────────────────────────────────────────
api_doc.rs (10K lines)    →    api_doc/ (7 files)        →   All routes annotated
8 route files >1000 lines →    8 dirs with sub-modules    →   No file >500 lines
131 unwrap() calls         →    0 production unwraps       →   unwrap_used = "deny" clean
45-field AdminServices     →    5 domain sub-structs       →   Per-domain service groups
27 thin storage wrappers   →    27 1-line facades          →   Remove wrappers entirely
```

The plan leaves us at the middle column. The 12-month ideal (removing storage wrappers entirely) would require refactoring all consumers to import from `synapse_storage` directly — out of scope for this cleanup, correctly deferred.

#### Step 0D: Alternatives Considered

| Alternative | Effort | Risk | Verdict |
|-------------|--------|------|---------|
| Skip Workstream E (storage wrappers) | Save ~30 min | Accumulates cruft | **Rejected (P2)** — trivial change, in blast radius |
| Use macro to eliminate unwrap boilerplate | 2h to implement | Adds abstraction | **Rejected (P5)** — explicit `ok_or_else` per call site is clearer |
| Skip Workstream D (AdminServices split) | Save ~1h | God struct grows further | **Rejected (P1)** — 45 fields today, more coming |
| Combine all workstreams in one commit | Save git overhead | Hard to review/revert | **Rejected (P5)** — per-workstream commits are clearer |

#### Step 0E: Temporal Interrogation

- **HOUR 1:** Workstreams A + B1 already complete. B2 (membership.rs) in progress.
- **HOUR 3:** B3-B8 (6 route files), C1-C5 (first 5 unwrap files). These are parallelizable by an agent.
- **HOUR 6+:** C6 (batch of 14 files), D1 (AdminServices split with all reference updates), E1 (27 storage files). D1 is the highest-risk task due to reference churn.

No task blocks another. Workstreams are independent. An agentic worker could execute all remaining tasks in ~3-4 hours of CC time.

#### Step 0F: Mode Selection — HOLD SCOPE

Confirmed. No scope expansion warranted. This is cleanup, not feature work.

---

#### Section 1: Architecture Review — No issues found

Examined: dependency graph across 5 workstreams, coupling boundaries, single points of failure.

```
                    ┌──────────────────┐
                    │   Workstream A   │
                    │  api_doc/ split  │── COMPLETE
                    └──────────────────┘

                    ┌──────────────────┐
                    │   Workstream B   │
                    │  Route splitting │── B1 COMPLETE, B2-B8 remain
                    └──────────────────┘
                                          No cross-workstream
                    ┌──────────────────┐   dependencies.
                    │   Workstream C   │   Each produces its
                    │  unwrap() elim.  │   own commit.
                    └──────────────────┘

                    ┌──────────────────┐
                    │   Workstream D   │
                    │  AdminServices   │── Highest risk: touches
                    │  decomposition   │   all admin consumers
                    └──────────────────┘

                    ┌──────────────────┐
                    │   Workstream E   │
                    │  Storage facades │── Trivial: 27 one-line edits
                    └──────────────────┘
```

Workstreams A and B are file-splitting only — zero behavioral change. Workstream C changes error propagation but preserves all existing error types. Workstream D is the only structural change with cascading references — verified `state.services.admin.X` pattern across `src/server.rs`, `src/web/routes/`, and middleware. Workstream E is purely cosmetic (replacing explicit re-exports with glob re-exports).

**Finding: membership.rs has no public items.** All functions in `src/web/routes/federation/membership.rs` are private `async fn`. The plan assumes a `pub fn create_membership_router()` interface, but the actual file has no public API surface. The split into `invite/join/leave/knock` sub-modules may need to expose more items as `pub(crate)` than the plan anticipates. This is an implementation detail, not a plan defect — the implementer will discover and handle it.

---

#### Section 2: Error & Rescue Map — No issues found

This is a refactoring plan. No new methods, services, or codepaths introduced. Workstream C (unwrap elimination) *improves* error handling by converting panics to `Result<T, ApiError>`. The existing error types (`ApiError::internal`, `ApiError::internal_with_log`) already have rescue paths in the Axum error handler layer. No GAPs to flag.

---

#### Section 3: Security & Threat Model — No issues found

Examined: attack surface expansion, input validation, authorization boundaries, secrets, dependency risk, injection vectors, audit logging.

**Finding: unwrap() elimination reduces DoS surface.** Every `unwrap()` in a request handler is a potential panic → 500 → connection drop. Converting 131 of these to `Result<T, ApiError>` removes 131 panic vectors from request paths. This is a security improvement, not just code quality.

No new attack surface. No new endpoints, params, file paths, or background jobs. No new dependencies. AdminServices decomposition is purely structural — same fields, same types, same access patterns, different struct shape.

---

#### Section 4: Data Flow & Interaction Edge Cases — No issues found

No new data flows. No new user interactions. All changes are structural: files split, functions re-exported, structs regrouped, unwraps converted to Results. Compilation verification (`cargo check --workspace --all-features`) after each task ensures behavioral equivalence.

---

#### Section 5: Dependency & Coupling — One finding

**Finding: AdminServices decomposition touches many consumers.** Workstream D changes `state.services.admin.X` to `state.services.admin.subgroup.X` across all admin route handlers, middleware, and `server.rs`. This is the only workstream with cascading reference updates.

- **Auto-decided (P5):** Proceed as planned. The 5 sub-struct mapping is explicit and mechanical. `grep` + sed/IDE refactor handles this cleanly. Each reference update is a single-line change. Risk is low because Rust's type checker catches every missed reference at compile time.

---

#### Section 6: Test & Verification Strategy — Minor gap

**Finding: No integration test run after each task.** The plan runs `cargo test --test unit --features test-utils` after each task but defers integration tests to final verification (Task F1). This is pragmatic given that most tasks are file-splitting with zero behavioral change. However, Workstream C (unwrap elimination) changes error propagation paths — a missed `.map_err()` conversion could change behavior.

- **Auto-decided (P3):** Accept the plan's approach. Running full integration tests after every unwrap-elimination task would add ~10 min per task without proportional benefit. The final verification gate catches any regressions. If an integration test breaks, `git bisect` across per-task commits pinpoints the cause.

---

#### Section 7: Rollback & Recovery — No issues found

Per-workstream commits with descriptive messages make `git revert` clean and targeted. No database migrations. No config changes. No feature flags needed. Rollback is `git revert <commit>` per workstream.

---

#### Section 8: Observability & Monitoring — No issues found

No new logs, metrics, or traces needed. The plan is behavioral no-op from an observability standpoint.

---

#### Section 9: Documentation & Knowledge — No issues found

File splitting improves discoverability (smaller files, clearer names). No API docs changes needed — utoipa annotations move with their handler code. CLAUDE.md already documents the workspace structure.

---

#### Section 10: Migration & Compatibility — No issues found

No API changes. No config changes. No database schema changes. All workstreams are internal refactoring. External consumers (Matrix clients, federation peers) are unaffected.

---

#### CEO Completion Summary

| Dimension | Score | Notes |
|-----------|-------|-------|
| Premises | 5/5 confirmed | All sound, grounded in measurable code state |
| Scope calibration | HOLD SCOPE | Correct for cleanup — no expansion warranted |
| Architecture | PASS | 5 independent workstreams, zero cross-dependencies |
| Error handling | IMPROVED | 131 unwrap → Result conversions |
| Security | IMPROVED | Panic surface reduced |
| Test coverage | ADEQUATE | Per-task unit tests + final integration gate |
| Rollback safety | SAFE | Per-workstream commits, no schema changes |
| Codex | UNAVAILABLE | Binary not found — single-model review |

**Phase 1 complete.** Codex: N/A. Claude: 1 finding (membership.rs private fn pattern), 1 minor gap (integration test timing). All auto-decided per principles. Passing to Phase 3.

---

### Phase 2: Design Review — SKIPPED

No UI scope detected. Plan is pure Rust backend refactoring. Zero view/rendering terms matched.

---

### Phase 3: Engineering Review

#### Code Verification

Examined actual filesystem state against plan claims:

| Plan Claim | Actual | Status |
|------------|--------|--------|
| Workstream A: api_doc/ 7 files | 7 files exist (`mod.rs`, `admin.rs`, `auth.rs`, `client_server.rs`, `federation.rs`, `health.rs`, `schemas.rs`) | **COMPLETE** |
| Task B1: e2ee/ 4 files | 4 files exist (`mod.rs`, `backup.rs`, `devices.rs`, `keys.rs`) | **COMPLETE** |
| 131 production unwrap() calls | `grep -rn '\.unwrap()' src/ --include='*.rs' \| grep -v tests/ \| grep -v '#\[cfg(test)\]' \| grep -v test_utils \| wc -l` → 131 | **CONFIRMED** |
| 27 storage wrapper files at 4-8 lines | All 27 files exist, 4-8 lines each, `pub use synapse_storage::*` pattern | **CONFIRMED** |
| membership.rs all private fns | Verified: no `pub fn` items in file | **FINDING** — plan assumes public router fn |
| AdminServices 45 fields | `state.services.admin.X` pattern verified across `server.rs`, middleware, routes | **CONFIRMED** |

**Discrepancy: membership.rs public API assumption.** The plan describes `pub fn create_membership_router() -> Router<AppState>` as the output interface for B2, but `src/web/routes/federation/membership.rs` has zero public items. The callers likely import the handlers directly. This doesn't block the split — the sub-modules just need appropriate `pub(crate)` visibility on handlers that other federation modules call.

- **Auto-decided (P5):** No plan change needed. The implementer adjusts visibility during the split. This is a code-reading finding, not a plan defect.

#### Section 1: Architecture — No additional issues

CEO architecture review already covers this. ASCII dependency graph produced above. No new concerns.

#### Section 2: Code Quality — No issues

The plan follows existing patterns (sub-module decomposition, `Result<T, ApiError>` propagation, thin facade re-exports). No DRY violations. No naming issues. No complexity introduced — complexity is reduced in every workstream.

#### Section 3: Test Review

**Test plan for remaining workstreams:**

| Workstream | What to test | Test type | Coverage |
|------------|-------------|-----------|----------|
| B2-B8 (route splitting) | Router assembly compiles, handlers accessible | `cargo check` | Compile-time |
| B2-B8 | Unit tests pass after refactor | `cargo test --test unit` | Existing suite |
| C1-C6 (unwrap elimination) | Each file compiles after conversion | `cargo check` | Compile-time |
| C1-C6 | Error propagation paths unchanged | `cargo test --test unit` | Existing suite |
| C1-C6 | No remaining production unwraps | `grep` verification | Script |
| D1 (AdminServices) | All admin routes compile | `cargo check --workspace` | Compile-time |
| D1 | Admin unit tests pass | `cargo test --test unit` | Existing suite |
| E1 (storage facades) | All consumers compile | `cargo check --workspace` | Compile-time |
| F1 (final) | Full clippy + unit + integration | `scripts/run_ci_tests.sh` | Full suite |

**Gap: no targeted tests for unwrap→Result conversion correctness.** The plan relies on compilation + existing unit tests. This is adequate because:
1. Rust's type system catches type errors at compile time
2. `?` operator propagates errors through existing call chains
3. The existing Axum error handler already maps `ApiError` to HTTP responses
4. Integration tests in F1 catch behavioral regressions

- **Auto-decided (P5/P6):** Accept the plan's verification strategy. Adding targeted tests for each unwrap conversion would be ideal but the compile-time guard + existing suite is sufficient given this is a cleanup, not a feature.

Test plan artifact written to: `~/.gstack/projects/langkebo-synapse-rust/main-test-plan-20260625.md`

#### Section 4: Performance — No issues

No performance impact. File splitting and struct decomposition are compile-time only. unwrap→Result conversion has identical runtime cost (panic path becomes error path). Storage wrapper glob re-exports have identical import resolution.

---

#### Eng Consensus Table (single-model)

```
ENG DUAL VOICES — CONSENSUS TABLE:
═══════════════════════════════════════════════════════════════
  Dimension                           Claude  Codex  Consensus
  ──────────────────────────────────── ─────── ─────── ─────────
  1. Architecture sound?               ✓       N/A     CONFIRMED
  2. Test coverage sufficient?         ✓       N/A     CONFIRMED
  3. Performance risks addressed?      ✓       N/A     CONFIRMED (N/A)
  4. Security threats covered?         ✓       N/A     CONFIRMED
  5. Error paths handled?              ✓       N/A     CONFIRMED
  6. Deployment risk manageable?       ✓       N/A     CONFIRMED
═══════════════════════════════════════════════════════════════
```

**Phase 3 complete.** Codex: N/A. Claude: 1 finding (membership.rs visibility), 1 gap noted (integration test timing). All auto-decided. Passing to final gate.

---

### Phase 3.5: DX Review — SKIPPED

No developer-facing scope detected. This is internal codebase cleanup — no new APIs, CLI, SDK, or developer tools introduced.

---

### Decision Audit Trail

| ID | Phase | Section | Decision | Principle | Classification |
|----|-------|---------|----------|-----------|----------------|
| D-PREMISES | CEO | 0A | All 5 premises confirmed | N/A (user confirmed) | Premise gate |
| D-MODE | CEO | 0F | HOLD SCOPE confirmed | P3 | Mechanical |
| D-ALT-MACRO | CEO | 0D | Reject unwrap macro approach | P5 (explicit) | Mechanical |
| D-ALT-COMBO | CEO | 0D | Reject single-commit approach | P5 (explicit) | Mechanical |
| D-MEMBERSHIP | CEO | S1 | membership.rs private fn pattern — implementer handles | P5 (explicit) | Mechanical |
| D-ADMIN-REFS | CEO | S5 | Proceed with grep/sed refactor of admin references | P5 (explicit) | Mechanical |
| D-INTEGRATION | CEO | S6 | Accept deferred integration tests until F1 | P3 (pragmatic) | Mechanical |
| D-TEST-GAP | Eng | S3 | Accept compile-time + existing suite for unwrap conversion | P5/P6 | Mechanical |

**Zero taste decisions surfaced.** All findings were mechanical — one clearly correct answer. No borderline scope expansions. No close approaches. No Codex disagreements (Codex unavailable).

**Zero user challenges.** Both models (single model in this case) agree with the plan's direction. No features to merge, split, add, or remove.

---

### Final Scores

| Dimension | Score | Notes |
|-----------|-------|-------|
| Strategic alignment | 10/10 | Directly reduces technical debt measured in file size, unwrap count, struct size |
| Scope calibration | 10/10 | HOLD SCOPE — right call for cleanup |
| Architecture | 9/10 | Clean independent workstreams; membership.rs visibility is minor |
| Error handling | 10/10 | 131 panic vectors eliminated |
| Security | 10/10 | DoS surface reduced |
| Test coverage | 8/10 | Adequate; integration tests deferred to final gate |
| Rollback safety | 10/10 | Per-workstream commits, git revert clean |
| Implementation feasibility | 10/10 | Well-defined tasks, existing patterns, compile-time verification |

**Overall: 96/100 — APPROVED**

### Implementation Tasks (remaining)

Workstreams A and B1 already committed. Remaining:

- [ ] B2: Split `federation/membership.rs` (1192 lines) → `membership/{invite,join,leave,knock}.rs`
- [ ] B3: Split `room/management.rs` (1159 lines) → `management/{create,upgrade,delete}.rs`
- [ ] B4: Split `media.rs` (1153 lines) → `media/{download,upload,thumbnail}.rs`
- [ ] B5: Split `oidc.rs` (1124 lines) → `oidc/{callback,discovery}.rs`
- [ ] B6: Split `handlers/search.rs` (1092 lines) → `search/{user,room}.rs`
- [ ] B7: Split `friend_room.rs` (1054 lines) → `friend_room/{create,manage}.rs`
- [ ] B8: Split `handlers/room/events.rs` (990 lines) → `events/{send,state,redact}.rs`
- [ ] C1-C6: Eliminate 131 `unwrap()` calls across 19 files
- [ ] D1: Split AdminServices (45 fields) into 5 domain sub-structs
- [ ] E1: Reduce 27 storage wrappers to 1-line facades
- [ ] F1: Full workspace verification (clippy + unit + integration + unwrap grep)

**Estimated CC time:** ~3-4 hours for an agentic worker implementing task-by-task.

