# Architecture Deepening — Storage Traits + Service Collapse + Governance Extraction

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Introduce storage traits to unlock unit testing, collapse pass-through services that fail the deletion test, extract domain logic from HTTP handlers, and deduplicate SQL patterns.

**Architecture:** A pattern-setting `UserStore` trait with Postgres + in-memory adapters enables service unit tests without PostgreSQL. Pass-through services (PresenceService, RoomTagService) are collapsed — callers use storage directly since no business logic interposes. Capability governance moves from the 947-line `handlers/versions.rs` into a dedicated service module behind a 2-method interface. Trigram ranking SQL is extracted into a shared `TrigramRanking` helper consumed by `user.rs` and `space.rs`.

**Tech Stack:** Rust, sqlx, tokio, PostgreSQL (pg_trgm extension)

## Global Constraints

- Storage traits use `async_trait` (already a workspace dependency via `synapse-common`)
- All new code passes `cargo fmt --all -- --check` and `SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings`
- Tests run with `cargo test --test unit <test_name> -- --exact --nocapture`
- Trait error type is `sqlx::Error` (matches existing storage return types)
- In-memory fakes use `std::collections::HashMap` behind `tokio::sync::RwLock`
- Deletion test: if a service has zero business logic beyond `self.storage.x().await.map_err(...)`, collapse it
- No `pub pool` fields on new code; existing `pub pool` access is not worsened

---

### Task 1: Define UserStore trait and implement for UserStorage

**Files:**
- Modify: `synapse-storage/src/user.rs` — add `UserStore` trait, impl for `UserStorage`
- Modify: `synapse-storage/src/lib.rs` — re-export trait

**Interfaces:**
- Consumes: nothing (first task)
- Produces:
  - `pub trait UserStore`: 5 lock-related methods (focused subset, pattern-setter)
    ```rust
    async fn lock_user(&self, user_id: &str, reason: Option<&str>, locked_by: &str, now_ts: i64) -> Result<LockedUser, sqlx::Error>;
    async fn unlock_user(&self, user_id: &str, now_ts: i64) -> Result<(), sqlx::Error>;
    async fn is_user_locked(&self, user_id: &str) -> Result<bool, sqlx::Error>;
    async fn get_active_user_lock(&self, user_id: &str) -> Result<Option<LockedUser>, sqlx::Error>;
    async fn get_locked_users(&self, limit: i64, offset: i64) -> Result<Vec<LockedUser>, sqlx::Error>;
    ```
  - `impl UserStore for UserStorage { ... }` — delegates to existing methods
  - Re-export: `pub use user::UserStore;` in `synapse-storage/src/lib.rs`

- [ ] **Step 1: Add `async_trait` import and define `UserStore` trait**

In `synapse-storage/src/user.rs`, after the existing imports, add:

```rust
use async_trait::async_trait;
```

Then, immediately before the `pub struct UserStorage {` line, add the trait definition:

```rust
/// Storage trait for user lock operations.
/// Two adapters justify the seam: Postgres (prod) and in-memory (test).
#[async_trait]
pub trait UserStore: Send + Sync {
    async fn lock_user(
        &self,
        user_id: &str,
        reason: Option<&str>,
        locked_by: &str,
        now_ts: i64,
    ) -> Result<LockedUser, sqlx::Error>;

    async fn unlock_user(&self, user_id: &str, now_ts: i64) -> Result<(), sqlx::Error>;

    async fn is_user_locked(&self, user_id: &str) -> Result<bool, sqlx::Error>;

    async fn get_active_user_lock(&self, user_id: &str) -> Result<Option<LockedUser>, sqlx::Error>;

    async fn get_locked_users(&self, limit: i64, offset: i64) -> Result<Vec<LockedUser>, sqlx::Error>;
}
```

- [ ] **Step 2: Implement `UserStore` for `UserStorage`**

In `synapse-storage/src/user.rs`, after the existing `impl UserStorage { ... }` block, add:

```rust
#[async_trait]
impl UserStore for UserStorage {
    async fn lock_user(
        &self,
        user_id: &str,
        reason: Option<&str>,
        locked_by: &str,
        now_ts: i64,
    ) -> Result<LockedUser, sqlx::Error> {
        self.lock_user(user_id, reason, locked_by, now_ts).await
    }

    async fn unlock_user(&self, user_id: &str, now_ts: i64) -> Result<(), sqlx::Error> {
        self.unlock_user(user_id, now_ts).await
    }

    async fn is_user_locked(&self, user_id: &str) -> Result<bool, sqlx::Error> {
        self.is_user_locked(user_id).await
    }

    async fn get_active_user_lock(&self, user_id: &str) -> Result<Option<LockedUser>, sqlx::Error> {
        self.get_active_user_lock(user_id).await
    }

    async fn get_locked_users(&self, limit: i64, offset: i64) -> Result<Vec<LockedUser>, sqlx::Error> {
        self.get_locked_users(limit, offset).await
    }
}
```

- [ ] **Step 3: Re-export trait from `synapse-storage/src/lib.rs`**

Find the glob re-export line `pub use self::user::*;` and verify it already covers `UserStore`. If not, add an explicit re-export after it:

```rust
pub use user::UserStore;
```

Note: The existing `pub use self::user::*;` glob should already cover it if it's before the trait definition. Verify the glob re-export is declared after the trait is defined (Rust glob re-exports capture all `pub` items in the module, including traits).

- [ ] **Step 4: Verify compilation**

Run: `cargo build --locked 2>&1 | head -40`
Expected: Compiles successfully with the new trait.

- [ ] **Step 5: Commit**

```bash
git add synapse-storage/src/user.rs synapse-storage/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: define UserStore trait for user lock operations

Pattern-setting storage trait with 5 lock-related methods.
UserStorage (Postgres) is the first adapter; an in-memory
FakeUserStore adapter will be added in the next task.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: Create FakeUserStore and unit-test UserLockService

**Files:**
- Create: `synapse-storage/src/user_store_fake.rs` — in-memory adapter
- Modify: `synapse-storage/src/lib.rs` — declare + re-export fake module
- Modify: `synapse-services/src/user_lock_service.rs` — accept `Arc<dyn UserStore>`, add unit tests

**Interfaces:**
- Consumes: `UserStore` trait, `LockedUser` struct from Task 1
- Produces:
  - `pub struct FakeUserStore { locked_users: RwLock<Vec<LockedUser>> }`
  - `impl UserStore for FakeUserStore` — in-memory implementation
  - Updated `UserLockService::new(store: Arc<dyn UserStore>)` — accepts trait object
  - Unit tests for `UserLockService` using `FakeUserStore`

- [ ] **Step 1: Create `synapse-storage/src/user_store_fake.rs`**

```rust
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::user::{LockedUser, UserStore};

/// In-memory adapter for UserStore — used in unit tests.
/// Stores locked users in a Vec behind RwLock.
#[derive(Clone, Default)]
pub struct FakeUserStore {
    locked_users: Arc<RwLock<Vec<LockedUser>>>,
}

impl FakeUserStore {
    pub fn new() -> Self {
        Self { locked_users: Arc::new(RwLock::new(Vec::new())) }
    }
}

#[async_trait]
impl UserStore for FakeUserStore {
    async fn lock_user(
        &self,
        user_id: &str,
        reason: Option<&str>,
        locked_by: &str,
        now_ts: i64,
    ) -> Result<LockedUser, sqlx::Error> {
        let mut users = self.locked_users.write().await;
        // Deactivate any existing active lock for this user
        for u in users.iter_mut() {
            if u.user_id == user_id {
                u.is_active = false;
            }
        }
        let locked = LockedUser {
            id: users.len() as i64 + 1,
            user_id: user_id.to_string(),
            reason: reason.map(|s| s.to_string()),
            locked_by: locked_by.to_string(),
            created_ts: now_ts,
            unlocked_ts: None,
            is_active: true,
        };
        users.push(locked.clone());
        Ok(locked)
    }

    async fn unlock_user(&self, user_id: &str, now_ts: i64) -> Result<(), sqlx::Error> {
        let mut users = self.locked_users.write().await;
        for u in users.iter_mut() {
            if u.user_id == user_id && u.is_active {
                u.is_active = false;
                u.unlocked_ts = Some(now_ts);
            }
        }
        Ok(())
    }

    async fn is_user_locked(&self, user_id: &str) -> Result<bool, sqlx::Error> {
        let users = self.locked_users.read().await;
        Ok(users.iter().any(|u| u.user_id == user_id && u.is_active))
    }

    async fn get_active_user_lock(&self, user_id: &str) -> Result<Option<LockedUser>, sqlx::Error> {
        let users = self.locked_users.read().await;
        Ok(users.iter().find(|u| u.user_id == user_id && u.is_active).cloned())
    }

    async fn get_locked_users(&self, limit: i64, offset: i64) -> Result<Vec<LockedUser>, sqlx::Error> {
        let users = self.locked_users.read().await;
        let active: Vec<_> = users.iter().filter(|u| u.is_active).cloned().collect();
        let start = offset as usize;
        let end = (offset + limit).min(active.len() as i64) as usize;
        Ok(active[start..end.min(active.len())].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_lock_and_check_user() {
        let store = FakeUserStore::new();
        let now = 1000;

        assert!(!store.is_user_locked("@alice:example.com").await.unwrap());

        let locked = store.lock_user("@alice:example.com", Some("spam"), "admin", now).await.unwrap();
        assert!(locked.is_active);
        assert_eq!(locked.user_id, "@alice:example.com");
        assert_eq!(locked.reason, Some("spam".to_string()));

        assert!(store.is_user_locked("@alice:example.com").await.unwrap());
    }

    #[tokio::test]
    async fn test_unlock_user() {
        let store = FakeUserStore::new();
        let now = 1000;

        store.lock_user("@alice:example.com", None, "admin", now).await.unwrap();
        assert!(store.is_user_locked("@alice:example.com").await.unwrap());

        store.unlock_user("@alice:example.com", now + 100).await.unwrap();
        assert!(!store.is_user_locked("@alice:example.com").await.unwrap());
    }

    #[tokio::test]
    async fn test_get_active_user_lock_returns_none_after_unlock() {
        let store = FakeUserStore::new();
        let now = 1000;

        store.lock_user("@bob:example.com", None, "admin", now).await.unwrap();
        let lock = store.get_active_user_lock("@bob:example.com").await.unwrap();
        assert!(lock.is_some());

        store.unlock_user("@bob:example.com", now + 1).await.unwrap();
        let lock = store.get_active_user_lock("@bob:example.com").await.unwrap();
        assert!(lock.is_none());
    }

    #[tokio::test]
    async fn test_get_locked_users_pagination() {
        let store = FakeUserStore::new();
        let now = 1000;

        store.lock_user("@a:example.com", None, "admin", now).await.unwrap();
        store.lock_user("@b:example.com", None, "admin", now).await.unwrap();
        store.lock_user("@c:example.com", None, "admin", now).await.unwrap();

        let page = store.get_locked_users(2, 0).await.unwrap();
        assert_eq!(page.len(), 2);

        let page2 = store.get_locked_users(2, 2).await.unwrap();
        assert_eq!(page2.len(), 1);
    }
}
```

- [ ] **Step 2: Register module in `synapse-storage/src/lib.rs`**

Add after the existing `pub mod user;` line:

```rust
pub mod user_store_fake;
```

And add to the glob re-exports section:

```rust
pub use user_store_fake::FakeUserStore;
```

- [ ] **Step 3: Run FakeUserStore unit tests**

Run: `cargo test --test unit user_store_fake -- --exact --nocapture`
Expected: 4 tests PASS

- [ ] **Step 4: Update `UserLockService` to accept `Arc<dyn UserStore>`**

In `synapse-services/src/user_lock_service.rs`, change the struct and impl:

```rust
use std::sync::Arc;
use synapse_storage::user::{LockedUser, UserStore};

#[derive(Clone)]
pub struct UserLockService {
    user_store: Arc<dyn UserStore>,
}

impl UserLockService {
    pub fn new(user_store: Arc<dyn UserStore>) -> Self {
        Self { user_store }
    }

    pub async fn lock_user(
        &self,
        user_id: &str,
        reason: Option<&str>,
        locked_by: &str,
        now_ts: i64,
    ) -> Result<LockedUser, ApiError> {
        self.user_store
            .lock_user(user_id, reason, locked_by, now_ts)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to lock user", &e))
    }

    pub async fn unlock_user(&self, user_id: &str, now_ts: i64) -> Result<(), ApiError> {
        self.user_store
            .unlock_user(user_id, now_ts)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to unlock user", &e))
    }

    pub async fn is_user_locked(&self, user_id: &str) -> Result<bool, ApiError> {
        self.user_store
            .is_user_locked(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check user lock status", &e))
    }

    pub async fn get_active_user_lock(&self, user_id: &str) -> Result<Option<LockedUser>, ApiError> {
        self.user_store
            .get_active_user_lock(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get active user lock", &e))
    }

    pub async fn get_locked_users(&self, limit: i64, offset: i64) -> Result<Vec<LockedUser>, ApiError> {
        self.user_store
            .get_locked_users(limit, offset)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get locked users", &e))
    }
}
```

- [ ] **Step 5: Add unit tests for `UserLockService`**

Append to `synapse-services/src/user_lock_service.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use synapse_storage::FakeUserStore;

    #[tokio::test]
    async fn test_lock_user_service() {
        let store = Arc::new(FakeUserStore::new());
        let service = UserLockService::new(store);

        let locked = service.lock_user("@alice:example.com", Some("spam"), "admin", 1000).await.unwrap();
        assert_eq!(locked.user_id, "@alice:example.com");
        assert!(locked.is_active);

        assert!(service.is_user_locked("@alice:example.com").await.unwrap());
    }

    #[tokio::test]
    async fn test_unlock_user_service() {
        let store = Arc::new(FakeUserStore::new());
        let service = UserLockService::new(store);

        service.lock_user("@alice:example.com", None, "admin", 1000).await.unwrap();
        service.unlock_user("@alice:example.com", 1001).await.unwrap();
        assert!(!service.is_user_locked("@alice:example.com").await.unwrap());
    }

    #[tokio::test]
    async fn test_get_locked_users_service() {
        let store = Arc::new(FakeUserStore::new());
        let service = UserLockService::new(store);

        service.lock_user("@a:example.com", None, "admin", 1000).await.unwrap();
        service.lock_user("@b:example.com", None, "admin", 1000).await.unwrap();

        let users = service.get_locked_users(10, 0).await.unwrap();
        assert_eq!(users.len(), 2);
    }
}
```

- [ ] **Step 6: Run the UserLockService unit tests**

Run: `cargo test --test unit user_lock_service -- --exact --nocapture`
Expected: 3 tests PASS (no PostgreSQL required)

- [ ] **Step 7: Verify compilation of all crates**

Run: `cargo build --locked 2>&1 | tail -5`
Expected: Compiles successfully. If `container.rs` constructs `UserLockService::new(Arc::new(UserStorage { ... }))`, update it to wrap in `Arc::new(...)`.

- [ ] **Step 8: Commit**

```bash
git add synapse-storage/src/user_store_fake.rs synapse-storage/src/lib.rs synapse-services/src/user_lock_service.rs
git commit -m "$(cat <<'EOF'
feat: add FakeUserStore and unit-test UserLockService without PostgreSQL

Introduces in-memory FakeUserStore adapter implementing UserStore trait.
UserLockService now accepts Arc<dyn UserStore> instead of concrete
Arc<UserStorage>, enabling isolated unit tests with the fake adapter.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: Collapse PresenceService — route handlers call storage directly

**Files:**
- Modify: `synapse-services/src/presence_service.rs` — remove file (or deprecate to empty)
- Modify: `src/web/routes/handlers/presence.rs` — use `PresenceStorage` directly
- Modify: `synapse-services/src/lib.rs` — remove presence_service module
- Modify: `src/services/mod.rs` — remove presence_service re-export

**Interfaces:**
- Consumes: `PresenceStorage` from `synapse-storage`
- Produces: No new types. `PresenceStorage` becomes the direct dependency of presence handlers.

- [ ] **Step 1: Read the presence handler to identify PresenceService call sites**

Read `src/web/routes/handlers/presence.rs` and identify every call to `PresenceService`. Each call follows the pattern `state.services.presence_service.get_presence(...)` — we will replace with `state.services.presence_storage.get_presence(...)`.

The handler already has access to `AppState` which carries the service container. We'll need to add `PresenceStorage` to the container or access it through the existing path.

- [ ] **Step 2: Make PresenceStorage available in the route handler context**

Read `src/services/container.rs` and find where `PresenceStorage` is constructed. Add a direct field or accessor. The simplest path: check if `presence_storage` already exists in the container or one of its sub-containers. If it's nested inside `PresenceService`, extract it to a direct field.

Actually, the simplest approach: check whether `PresenceStorage` is already accessible as `state.services.presence_storage` or similar. If `PresenceService` wraps it, we need to either:
a. Expose `presence_storage` as a public field on `PresenceService` (temporary), or
b. Add `PresenceStorage` directly to the container struct.

Check `container.rs` for the current wiring.

- [ ] **Step 3: Update presence handler — replace PresenceService calls with PresenceStorage calls**

In `src/web/routes/handlers/presence.rs`, replace each method call. Example transformation:

Before:
```rust
state.services.presence_service.get_presence_with_meta(user_id).await
```

After:
```rust
state.services.presence_storage.get_presence_with_meta(user_id)
    .await
    .map_err(|e| ApiError::internal_with_log("Failed to get presence", &e))?
```

Apply this pattern to all PresenceService calls in the handler.

- [ ] **Step 4: Remove `synapse-services/src/presence_service.rs`**

Delete the file.

- [ ] **Step 5: Update `synapse-services/src/lib.rs`**

Remove the line `pub mod presence_service;`

- [ ] **Step 6: Update `src/services/mod.rs`**

Remove any presence_service re-exports.

- [ ] **Step 7: Verify compilation**

Run: `cargo build --locked 2>&1 | tail -10`
Expected: Compiles successfully, no references to `PresenceService`.

- [ ] **Step 8: Run existing presence-related tests**

Run: `cargo test --test integration presence -- --exact --nocapture 2>&1 | tail -20`
Expected: Existing presence integration tests pass.

- [ ] **Step 9: Commit**

```bash
git add synapse-services/src/presence_service.rs synapse-services/src/lib.rs src/services/mod.rs src/web/routes/handlers/presence.rs src/services/container.rs
git commit -m "$(cat <<'EOF'
refactor: collapse PresenceService — callers use PresenceStorage directly

PresenceService was maximally shallow — every method was a one-line
delegation to PresenceStorage with map_err. Fails the deletion test:
removing it concentrates zero complexity.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: Collapse RoomTagService — callers use RoomTagStorage directly

**Files:**
- Modify: `synapse-services/src/room_tag_service.rs` — remove file
- Modify: `synapse-services/src/lib.rs` — remove room_tag_service module
- Modify: `src/services/mod.rs` — remove room_tag_service re-export
- Modify: callers that use `RoomTagService` — route handlers that call tag methods

**Interfaces:**
- Consumes: `RoomTagStorage` from `synapse-storage::room_tag`
- Produces: No new types. Callers use `RoomTagStorage` directly.

- [ ] **Step 1: Find all RoomTagService call sites**

Run: `grep -rn "room_tag_service\|RoomTagService" src/ --include="*.rs"`
Identify every file that uses `RoomTagService`. Typically these are in route handlers under `src/web/routes/`.

- [ ] **Step 2: Replace each call site with direct RoomTagStorage usage**

For each call site, replace:
```rust
state.services.room_tag_service.get_all_user_tags(user_id).await
```
with:
```rust
state.services.room_tag_storage.get_all_tags(user_id)
    .await
    .map_err(|e| ApiError::internal_with_log("Failed to get tags", &e))?
```

- [ ] **Step 3: Ensure RoomTagStorage is accessible in the handler context**

Check `container.rs` — if `RoomTagStorage` is only constructed inside `RoomTagService::new()`, extract it to a direct field in the container.

- [ ] **Step 4: Delete `synapse-services/src/room_tag_service.rs`**

- [ ] **Step 5: Update module declarations**

Remove `pub mod room_tag_service;` from `synapse-services/src/lib.rs`
Remove room_tag_service re-exports from `src/services/mod.rs`

- [ ] **Step 6: Verify compilation**

Run: `cargo build --locked 2>&1 | tail -10`
Expected: Compiles with no references to `RoomTagService`.

- [ ] **Step 7: Run existing tag-related tests**

Run: `cargo test --test integration tag -- --exact --nocapture 2>&1 | tail -20`
Expected: Tests pass.

- [ ] **Step 8: Commit**

```bash
git add synapse-services/src/room_tag_service.rs synapse-services/src/lib.rs src/services/mod.rs src/services/container.rs
# Add any modified caller files
git commit -m "$(cat <<'EOF'
refactor: collapse RoomTagService — callers use RoomTagStorage directly

RoomTagService was a pure delegation layer with 4 methods, each a
one-line storage call with map_err. Zero business logic, zero tests.
Fails the deletion test.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: Extract capability governance from handlers/versions.rs

**Files:**
- Create: `synapse-services/src/capability_governance.rs` — deep module
- Modify: `src/web/routes/handlers/versions.rs` — thin HTTP adapter
- Modify: `synapse-services/src/lib.rs` — declare module
- Modify: `src/services/mod.rs` — re-export

**Interfaces:**
- Consumes: `Config` from `synapse_common::config::Config`, `RouteLedger` from crate
- Produces:
  - `pub struct CapabilityGovernance` with:
    - `pub fn new(config: &Config, route_ledger: &RouteLedger) -> Self`
    - `pub fn build_capabilities_response(&self, authenticated: bool) -> serde_json::Value`
    - `pub fn sso_providers(&self) -> Vec<&'static str>`

- [ ] **Step 1: Create `synapse-services/src/capability_governance.rs`**

The module contains:
- `CapabilityFlag` struct and `CapabilityGovernance` enum (moved from versions.rs)
- All 20+ capability builder functions (moved from versions.rs, made private)
- `build_capabilities_response()` (moved)
- `sso_providers()` (moved)
- `build_capabilities_unstable_features()` (moved)
- `CLIENT_API_VERSION_SUPPORT` constant (moved)

Public interface:
```rust
use synapse_common::config::Config;

// ... CapabilityFlag, CapabilityGovernance, all capability functions (private) ...

pub struct CapabilityGovernance {
    config: Config,
    // route_ledger: RouteLedger,  // if needed for route-surface governance
}

impl CapabilityGovernance {
    pub fn new(config: &Config) -> Self {
        Self { config: config.clone() }
    }

    pub fn build_capabilities_response(&self, authenticated: bool) -> serde_json::Value {
        // ... current build_capabilities_response body, using self.config ...
    }

    pub fn sso_providers(&self) -> Vec<&'static str> {
        // ... current sso_providers body, using self.config ...
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sso_providers_none_by_default() {
        // Create a minimal config with saml/oidc disabled
        // Assert providers list is empty
    }

    #[test]
    fn test_build_capabilities_includes_room_versions() {
        // Create config, build response
        // Assert response contains "m.room_versions"
    }
}
```

- [ ] **Step 2: Move all capability code from `handlers/versions.rs`**

Copy the following items from `handlers/versions.rs` into `capability_governance.rs`:
- `CapabilityGovernance` enum (lines 96-100)
- `CapabilityFlag` struct + impl (lines 102-120)
- All `fn *_capability()` functions (~20 functions)
- `fn build_capabilities_response()` (lines 502-556)
- `fn build_capabilities_unstable_features()`
- `fn sso_providers()` (lines 224-240)
- `fn insert_enabled_capability()`
- `CLIENT_API_VERSION_SUPPORT` constant (lines 65-82)
- `ClientApiVersionSupport` struct if defined locally
- Any helper types used by the above

- [ ] **Step 3: Thin out `handlers/versions.rs`**

Replace the moved code with a thin adapter:

```rust
use crate::services::CapabilityGovernance;

pub async fn get_versions(state: ...) -> ... {
    let governance = CapabilityGovernance::new(&state.config);
    let body = governance.build_capabilities_response(authenticated);
    // ... HTTP response formatting (unchanged) ...
}
```

The handler keeps: HTTP request parsing, authentication check, response formatting, cache headers. All domain logic delegates to `CapabilityGovernance`.

- [ ] **Step 4: Register module and re-export**

In `synapse-services/src/lib.rs`:
```rust
pub mod capability_governance;
```

In `src/services/mod.rs`:
```rust
pub use synapse_services::capability_governance::CapabilityGovernance;
```

- [ ] **Step 5: Run capability governance tests**

Run: `cargo test --test unit capability_governance -- --exact --nocapture`
Expected: Unit tests pass (testing business logic without HTTP).

- [ ] **Step 6: Run existing versions endpoint tests**

Run: `cargo test --test integration versions -- --exact --nocapture 2>&1 | tail -20`
Expected: Existing integration tests pass (HTTP surface unchanged).

- [ ] **Step 7: Commit**

```bash
git add synapse-services/src/capability_governance.rs synapse-services/src/lib.rs src/services/mod.rs src/web/routes/handlers/versions.rs
git commit -m "$(cat <<'EOF'
refactor: extract CapabilityGovernance module from HTTP handlers

Moves 400+ lines of domain logic (capability flags, SSO discovery,
feature gating) from handlers/versions.rs into a dedicated service
module. The handler becomes a thin HTTP adapter. Interface shrinks
from 20+ public functions to 2: build_capabilities_response and
sso_providers.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: Deduplicate trigram search SQL with TrigramRanking helper

**Files:**
- Create: `synapse-storage/src/trigram_ranking.rs` — shared SQL builder
- Modify: `synapse-storage/src/user.rs` — use `TrigramRanking` in search methods
- Modify: `synapse-storage/src/space.rs` — use `TrigramRanking` in search_spaces
- Modify: `synapse-storage/src/lib.rs` — declare module

**Interfaces:**
- Consumes: nothing (internal to storage crate)
- Produces:
  - `pub struct TrigramRanking` with:
    - `pub fn new(column: &str, table: &str) -> Self`
    - `pub fn match_priority_case(&self) -> String` — returns the CASE expression
    - `pub fn where_clause(&self) -> String` — returns the WHERE clause
    - `pub fn candidate_cte(&self, additional_fields: &[&str]) -> String` — returns full CTE

- [ ] **Step 1: Create `synapse-storage/src/trigram_ranking.rs`**

```rust
/// Builds pg_trgm search CTEs with consistent ranking across storage modules.
/// Internal to the storage crate — not a new seam, just shared implementation.
pub struct TrigramRanking {
    column: String,
    table: String,
}

impl TrigramRanking {
    pub fn new(column: &str, table: &str) -> Self {
        Self {
            column: column.to_string(),
            table: table.to_string(),
        }
    }

    /// Returns the CASE expression for match priority:
    /// 0 = exact, 1 = prefix, 2 = contains, 3 = fuzzy (trigram)
    pub fn match_priority_case(&self) -> String {
        let col = &self.column;
        format!(
            "CASE
                WHEN {col} ILIKE $1 ESCAPE '\\' THEN 0
                WHEN {col} ILIKE $2 ESCAPE '\\' THEN 1
                WHEN {col} ILIKE $3 ESCAPE '\\' THEN 2
                ELSE 3
            END AS match_priority"
        )
    }

    /// Returns the similarity expression for this column.
    pub fn similarity_expr(&self) -> String {
        let col = &self.column;
        format!("similarity({col}, $4) AS match_similarity")
    }

    /// Returns the WHERE clause for matching against this column.
    pub fn where_clause(&self) -> String {
        let col = &self.column;
        format!(
            "{col} ILIKE $1 ESCAPE '\\'
             OR {col} ILIKE $2 ESCAPE '\\'
             OR {col} ILIKE $3 ESCAPE '\\'
             OR (char_length($4) >= 3 AND {col} % $4)"
        )
    }

    /// Returns the full subquery for matching a single column.
    pub fn column_match_subquery(&self, select_fields: &str, extra_where: Option<&str>, null_check: bool) -> String {
        let col = &self.column;
        let table = &self.table;
        let null_guard = if null_check {
            format!("AND {col} IS NOT NULL")
        } else {
            String::new()
        };
        let extra = extra_where.map(|w| format!("AND {w}")).unwrap_or_default();
        format!(
            "SELECT {select_fields},
                    {priority_case},
                    COALESCE({similarity}, 0.0) AS match_similarity
             FROM {table}
             WHERE ({where_clause})
             {null_guard}
             {extra}",
            select_fields = select_fields,
            priority_case = self.match_priority_case(),
            similarity = self.similarity_expr(),
            table = table,
            where_clause = self.where_clause(),
            null_guard = null_guard,
            extra = extra,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_priority_case_references_column() {
        let ranking = TrigramRanking::new("username", "users");
        let case_sql = ranking.match_priority_case();
        assert!(case_sql.contains("username ILIKE $1"));
        assert!(case_sql.contains("WHEN username ILIKE $2"));
    }

    #[test]
    fn test_where_clause_includes_trigram() {
        let ranking = TrigramRanking::new("name", "rooms");
        let where_sql = ranking.where_clause();
        assert!(where_sql.contains("name % $4"));
        assert!(where_sql.contains("char_length($4) >= 3"));
    }
}
```

- [ ] **Step 2: Register module in `synapse-storage/src/lib.rs`**

Add: `pub mod trigram_ranking;`

- [ ] **Step 3: Run TrigramRanking tests**

Run: `cargo test --test unit trigram_ranking -- --exact --nocapture`
Expected: 2 tests PASS

- [ ] **Step 4: Update `user.rs` to use TrigramRanking**

In `search_users` (around line 626), replace the inline CASE/WHERE/SQL with calls to `TrigramRanking`. The first subquery (matching `username`) becomes:

```rust
use crate::trigram_ranking::TrigramRanking;

// Inside search_users:
let username_rank = TrigramRanking::new("username", "users");
let user_id_rank = TrigramRanking::new("user_id", "users");
let displayname_rank = TrigramRanking::new("displayname", "users");

let sql = format!(
    r"
    WITH candidate_matches AS (
        SELECT user_id, MIN(match_priority) AS match_priority, MAX(match_similarity) AS match_similarity
        FROM (
            {username_subquery}
            UNION ALL
            {user_id_subquery}
            UNION ALL
            {displayname_subquery}
        ) AS matches
        GROUP BY user_id
    )
    SELECT u.user_id, u.username, COALESCE(u.displayname, u.username) AS displayname, u.avatar_url, u.created_ts
    FROM candidate_matches cm
    JOIN users u ON u.user_id = cm.user_id
    WHERE COALESCE(u.is_deactivated, FALSE) = FALSE
    ORDER BY cm.match_priority ASC, cm.match_similarity DESC, u.created_ts DESC
    LIMIT $5
    ",
    username_subquery = username_rank.column_match_subquery("user_id", Some("COALESCE(is_deactivated, FALSE) = FALSE"), false),
    user_id_subquery = user_id_rank.column_match_subquery("user_id", Some("COALESCE(is_deactivated, FALSE) = FALSE"), false),
    displayname_subquery = displayname_rank.column_match_subquery("user_id", Some("COALESCE(is_deactivated, FALSE) = FALSE"), true),
);
```

Replace the old `sqlx::query_as` SQL string, keeping the same bound parameters.

- [ ] **Step 5: Update `space.rs` to use TrigramRanking**

In `search_spaces`, replace the inline trigram SQL with `TrigramRanking` calls:

```rust
let name_rank = TrigramRanking::new("s.name", "spaces s");
let topic_rank = TrigramRanking::new("s.topic", "spaces s");
```

Same pattern — build subqueries with `column_match_subquery()` and interpolate into the CTE.

- [ ] **Step 6: Verify compilation and run search tests**

Run: `cargo build --locked 2>&1 | tail -10`
Expected: Compiles.

Run: `cargo test --test integration search -- --exact --nocapture 2>&1 | tail -20`
Expected: Existing search tests pass (behaviour unchanged).

- [ ] **Step 7: Commit**

```bash
git add synapse-storage/src/trigram_ranking.rs synapse-storage/src/lib.rs synapse-storage/src/user.rs synapse-storage/src/space.rs
git commit -m "$(cat <<'EOF'
refactor: extract TrigramRanking helper for shared search SQL

Deduplicates ~118 lines of near-identical pg_trgm ranking SQL between
user.rs and space.rs. The TrigramRanking struct builds match_priority
CASE expressions, similarity calls, and WHERE clauses from column/table
parameters — fix once, fixed everywhere.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---
