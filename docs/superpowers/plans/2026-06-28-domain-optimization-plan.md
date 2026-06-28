# Domain Architecture Optimization Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the root/canonical dual-track architecture (storage + services), update protocol version declarations, and break up the monolithic server assembly.

**Architecture:** Mechanical refactoring — replace all `use crate::storage::*` imports with direct `use synapse_storage::*` paths, and all `use crate::services::*` imports with direct `use synapse_services::*` paths. Then collapse the now-empty `src/storage/mod.rs` and `src/services/mod.rs` facade files to retain only locally-defined modules. No behavior changes — all gate checks (fmt, clippy, unit tests, doc tests) must pass at every commit.

**Tech Stack:** Rust, sqlx, axum, tokio — no new dependencies.

## Global Constraints

- All gate checks must pass at every commit: `cargo fmt --all -- --check`, `SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings`, `cargo test --doc --locked`, `SQLX_OFFLINE=true cargo test --test unit --features test-utils --locked`
- No behavior changes — this is a pure import-path refactoring
- Each task is independently revertible
- Commit messages follow the existing `conventional: description` pattern
- Feature-gated modules must remain gated behind the same `#[cfg(feature = "...")]` attributes

---

### Task 1: Audit storage module ownership — which files are facades vs. locals

**Files:**
- Read: `src/storage/mod.rs`
- Read: Each `src/storage/<module>.rs` referenced by `mod.rs:4-87`

**Interfaces:**
- Produces: `STORAGE_MIGRATION_MAP.md` — a ledger file listing every `src/storage/<module>.rs` file, whether it's a pure facade (can be deleted) or contains local code (must be preserved), and its canonical path in `synapse-storage`

- [ ] **Step 1: Generate the facade audit**

Run this audit script to classify every storage module:

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust
for f in src/storage/*.rs; do
  name=$(basename "$f" .rs)
  if [ "$name" = "mod" ]; then continue; fi
  
  # Count non-comment, non-blank, non-use lines
  body_lines=$(grep -v '^\s*//' "$f" | grep -v '^\s*$' | grep -v '^\s*pub use' | grep -v '^use ' | wc -l | tr -d ' ')
  
  canon="synapse-storage/src/${name}.rs"
  if [ -f "$canon" ]; then
    canon_exists="yes"
    canon_size=$(wc -l < "$canon" | tr -d ' ')
  else
    canon_exists="no"
    canon_size="0"
  fi
  
  echo "$name | local_body_lines=$body_lines | canon=$canon_exists | canon_size=$canon_size"
done
```

Expected: A table listing all 55+ modules with their classification.

- [ ] **Step 2: Classify each module and write the ledger**

For each module in the output, classify into one of three categories:

1. **Local-only** (canon does not exist, or local has substantial body): Keep in `src/storage/`. Example: `schema_health_check`, `schema_validator`, `performance`.
2. **Pure facade** (canon exists, local body_lines < 5): Delete the `src/storage/<name>.rs` file, remove `pub mod <name>;` from `mod.rs`, replace `pub use self::<name>::{...}` with `pub use synapse_storage::<name>::{...}`.
3. **Mixed** (canon exists, local body_lines >= 5): Needs manual inspection before migrating.

Write `STORAGE_MIGRATION_MAP.md` with the full classification.

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/STORAGE_MIGRATION_MAP.md
git commit -m "docs: audit storage module ownership — facade vs local classification ledger"
```

---

### Task 2: Eliminate pure-facade storage modules

**Files:**
- Modify: `src/storage/mod.rs` (remove `pub mod X;` for pure facades, replace `pub use self::X` with `pub use synapse_storage::X`)
- Delete: Each `src/storage/<name>.rs` classified as "pure facade" in Task 1

**Interfaces:**
- Consumes: `STORAGE_MIGRATION_MAP.md` from Task 1
- Produces: Cleaned-up `src/storage/mod.rs` with zero `pub mod X;` lines for pure facades; all `pub use` lines now point to `synapse_storage` directly

- [ ] **Step 1: Pick the first pure-facade module from the ledger**

Example with `admin_media` (if classified as pure facade):

```bash
# Check that synapse-storage/src/admin_media.rs exists
ls -la synapse-storage/src/admin_media.rs
```

- [ ] **Step 2: Remove the local facade file and update mod.rs**

Delete the facade file and update the `pub use` path:

```bash
rm src/storage/admin_media.rs
```

In `src/storage/mod.rs`, remove `pub mod admin_media;` and change:
```rust
pub use self::admin_media::{...};
```
to:
```rust
pub use synapse_storage::admin_media::{...};
```

- [ ] **Step 3: Verify compilation**

```bash
SQLX_OFFLINE=true cargo check --all-features 2>&1 | head -5
```

Expected: No errors related to the module change. If errors appear, check that all re-exported types exist in `synapse-storage`.

- [ ] **Step 4: Commit this module**

```bash
git add src/storage/admin_media.rs src/storage/mod.rs
git commit -m "refactor: eliminate admin_media storage facade — use synapse_storage directly"
```

- [ ] **Step 5: Repeat Steps 1-4 for each pure-facade module**

Process one module per commit. Expected: ~40 modules are pure facades.

---

### Task 3: Migrate storage consumers to direct `synapse_storage` imports

**Files:**
- Modify: All 24 files that import from `crate::storage::*` (listed in `CONSUMER_LIST`)
- Modify: `src/storage/mod.rs` (strip remaining `pub use` lines after consumers migrated)

**Interfaces:**
- Consumes: Cleaned-up `src/storage/mod.rs` from Task 2
- Produces: Zero files importing via `use crate::storage::*`; all consumers import directly from `synapse_storage`

**Consumer list** (24 files):

| File | Current import |
|------|---------------|
| `src/server.rs` | `use crate::storage::*` (line 18) + specific items |
| `src/tasks/mod.rs` | `use crate::storage::maintenance::*` + `use crate::storage::{...}` |
| `src/web/middleware/auth.rs` | `use crate::storage::CreateAuditEventRequest` |
| `src/web/utils/admin_auth.rs` | `use crate::storage::{CreateAuditEventRequest, User}` |
| `src/web/routes/app_service.rs` | `use crate::storage::application_service::*` |
| `src/web/routes/feature_flags.rs` | `use crate::storage::*` |
| `src/web/routes/event_report.rs` | `use crate::storage::event_report::*` |
| `src/web/routes/rendezvous.rs` | `use crate::storage::*` |
| `src/web/routes/module.rs` | `use crate::storage::module::*` |
| `src/web/routes/openclaw.rs` | `use crate::storage::openclaw::*` |
| `src/web/routes/push_notification.rs` | `use crate::storage::*` |
| `src/web/routes/sliding_sync.rs` | `use crate::storage::sliding_sync::*` |
| `src/web/routes/admin/audit.rs` | `use crate::storage::audit::*` + `use crate::storage::*` |
| `src/web/routes/admin/federation.rs` | `use crate::storage::federation_blacklist::*` |
| `src/web/routes/admin/notification.rs` | `use crate::storage::*` (×2) |
| `src/web/routes/admin/report.rs` | `use crate::storage::event_report::*` |
| `src/web/routes/admin/token.rs` | `use crate::storage::registration_token::*` |
| `src/web/routes/admin/user.rs` | `use crate::storage::User as AdminUserRecord` |
| `src/web/routes/admin/room/mod.rs` | `use crate::storage::*` (×2) |
| `src/web/routes/admin/retention.rs` | `use crate::storage::retention::*` |
| `src/web/routes/space/types.rs` | `use crate::storage::space::*` |
| `src/web/routes/extractors/auth.rs` | (check current state) |
| `src/web/routes/handlers/room/events.rs` | (check current state) |
| `src/web/routes/handlers/room/state.rs` | `use crate::storage::CreateBeaconInfoParams` + `use crate::storage::CreateEventParams` |

- [ ] **Step 1: Migrate one file — example `src/web/routes/admin/user.rs`**

Current:
```rust
use crate::storage::User as AdminUserRecord;
```
Replace with:
```rust
use synapse_storage::user::User as AdminUserRecord;
```

- [ ] **Step 2: Verify compilation after each file change**

```bash
SQLX_OFFLINE=true cargo check --all-features 2>&1 | grep -E "^error" | head -10
```

Expected: No errors.

- [ ] **Step 3: Commit each file migration**

```bash
git add <modified_file>
git commit -m "refactor: migrate <file> storage imports to synapse_storage direct path"
```

- [ ] **Step 4: Repeat Steps 1-3 for all 24 files**

Process in batches of 3-5 files per commit where changes are mechanical. Files with `use crate::storage::*` wildcard imports need to expand to explicit type imports from `synapse_storage::*`.

- [ ] **Step 5: Final clean-up of `src/storage/mod.rs`**

After all 24 consumers are migrated, strip `src/storage/mod.rs` down to only what remains:
- `pub mod` declarations for local-only modules (schema_health_check, schema_validator, performance, etc.)
- No `pub use` re-exports (they're now unused)

```bash
# Verify no remaining consumers before cleanup
grep -rn "use crate::storage::" src/ --include="*.rs"
```

Expected: zero matches.

```bash
git add src/storage/mod.rs
git commit -m "refactor: strip storage facade re-exports — all consumers use synapse_storage directly"
```

---

### Task 4: Migrate service consumers to direct `synapse_services` imports

**Files:**
- Modify: All 32 files importing from `crate::services::*`
- Modify: `src/services/mod.rs` (strip `pub use` re-exports, keep `pub mod container` and `ServiceContainer`)

**Interfaces:**
- Consumes: None (independent of Tasks 1-3)
- Produces: Zero files importing via `use crate::services::*`; `src/services/mod.rs` contains only `pub mod container;` and `pub use container::ServiceContainer;`

- [ ] **Step 1: Audit current service imports**

Run to list exact imports per file:

```bash
grep -rn "use crate::services::" src/ --include="*.rs" | grep -v "use crate::services::ServiceContainer"
```

- [ ] **Step 2: Migrate one file at a time**

Example — `src/web/routes/handlers/versions.rs` line 33:

Current:
```rust
use crate::services::CapabilityGovernance;
```
Replace with:
```rust
use synapse_services::capability_governance::CapabilityGovernance;
```

- [ ] **Step 3: Verify compilation after each file change**

```bash
SQLX_OFFLINE=true cargo check --all-features 2>&1 | grep -E "^error" | head -10
```

- [ ] **Step 4: Commit each batch**

```bash
git add <modified_files>
git commit -m "refactor: migrate service imports in <files> to synapse_services direct path"
```

- [ ] **Step 5: Strip `src/services/mod.rs`**

After all consumers are migrated, `src/services/mod.rs` should shrink to:

```rust
pub mod container;
#[cfg(feature = "test-utils")]
pub mod test_config;
pub use container::ServiceContainer;
```

No other `pub use` lines remain.

- [ ] **Step 6: Final verification**

```bash
grep -rn "use crate::services::" src/ --include="*.rs" | grep -v ServiceContainer
```

Expected: zero matches for anything other than `ServiceContainer`.

```bash
git add src/services/mod.rs
git commit -m "refactor: strip service facade re-exports — all consumers use synapse_services directly"
```

---

### Task 5: Update Matrix protocol version declarations (v1.13 → v1.18)

**Files:**
- Modify: `src/web/routes/handlers/versions.rs` — update `CLIENT_API_VERSION_SUPPORT`
- Modify: `docs/synapse-rust/SUPPORTED_MATRIX_SURFACE.md` — update declared versions and evidence matrix
- Create: `docs/synapse-rust/VERSION_GAP_ANALYSIS.md` — per-version audit of v1.14-v1.18 changes

**Interfaces:**
- Consumes: None
- Produces: Updated `/versions` response; documented version gap analysis

- [ ] **Step 1: Research v1.14-v1.18 spec changes**

Fetch the Matrix spec changelog for each version:

```bash
# Manual research step — check each version's changelog
# v1.14: https://spec.matrix.org/v1.14/changelog/
# v1.15: https://spec.matrix.org/v1.15/changelog/
# v1.16: https://spec.matrix.org/v1.16/changelog/
# v1.17: https://spec.matrix.org/v1.17/changelog/
# v1.18: https://spec.matrix.org/v1.18/changelog/
```

For each version, list the new/changed Client-Server API endpoints, error codes, and capability changes that synapse-rust would need to implement.

- [ ] **Step 2: Audit synapse-rust support for each version**

For each endpoint/capability change from Step 1, check whether synapse-rust already supports it:

```bash
# Example: check if a specific route exists
grep -r "<route_path>" src/web/routes/ --include="*.rs"
```

Classify each spec change as: `already supported`, `missing (trivial)`, `missing (significant)`, or `not applicable`.

- [ ] **Step 3: Write VERSION_GAP_ANALYSIS.md**

Document each version's changes and synapse-rust's support status. For items classified as `already supported`, record the evidence (route ledger, test, or implementation file). This document justifies which versions can be safely declared.

- [ ] **Step 4: Update CLIENT_API_VERSION_SUPPORT**

In `src/web/routes/handlers/versions.rs`, find the `CLIENT_API_VERSION_SUPPORT` constant (likely near line 40-60). Add only the versions that have full audit evidence:

```rust
// Before (likely around line 42-55):
const CLIENT_API_VERSION_SUPPORT: &[&str] = &[
    "r0.5.0", "r0.6.0", "r0.6.1",
    "v1.1", "v1.2", "v1.3", "v1.4", "v1.5",
    "v1.6", "v1.7", "v1.8", "v1.9", "v1.10",
    "v1.11", "v1.12", "v1.13",
];

// After (add only versions confirmed by audit):
const CLIENT_API_VERSION_SUPPORT: &[&str] = &[
    "r0.5.0", "r0.6.0", "r0.6.1",
    "v1.1", "v1.2", "v1.3", "v1.4", "v1.5",
    "v1.6", "v1.7", "v1.8", "v1.9", "v1.10",
    "v1.11", "v1.12", "v1.13",
    // v1.14+ added per VERSION_GAP_ANALYSIS.md audit
    "v1.14", "v1.15", "v1.16", "v1.17", "v1.18",
];
```

> **Note:** Only add versions where the audit confirms support. If the audit finds gaps for certain versions, only declare up to the highest fully-supported version.

- [ ] **Step 5: Update SUPPORTED_MATRIX_SURFACE.md**

Update the "暂不声明" section to reflect the newly declared versions. Document the audit evidence for each newly declared version.

- [ ] **Step 6: Verify and commit**

```bash
# Verify /versions response doesn't crash
SQLX_OFFLINE=true cargo check --all-features

# Run the version-related integration test
SQLX_OFFLINE=true cargo test --test integration --features test-utils \
  api_auth_routes_tests -- --exact --nocapture 2>&1 | tail -10

git add src/web/routes/handlers/versions.rs \
        docs/synapse-rust/SUPPORTED_MATRIX_SURFACE.md \
        docs/synapse-rust/VERSION_GAP_ANALYSIS.md
git commit -m "docs: update Matrix protocol version declarations v1.13→v1.18 with audit evidence"
```

---

### Task 6: Extract `build_database` from `server.rs`

**Files:**
- Create: `src/server/database.rs`
- Modify: `src/server.rs` (lines relevant to database initialization)

**Interfaces:**
- Produces: `pub async fn build_database_pool(config: &Config) -> Result<PgPool, StartupError>` in `src/server/database.rs`
- Consumes: None (extraction of existing code)

**Context:** `src/server.rs` (1237 lines) is a monolith that builds every subsystem inline. This task extracts the database initialization block into a focused module. The database block includes: Postgres pool creation, schema health checks, runtime DB init guards, and pool health verification.

- [ ] **Step 1: Identify the database initialization block in server.rs**

Read `src/server.rs` and locate the database pool creation code. It starts around the `PgPoolOptions::new()` call and includes schema health checks. The exact line range depends on the current file structure — use `grep -n` to find the boundaries.

```bash
grep -n "PgPool\|schema_health\|build_database\|SYNAPSE_ENABLE_RUNTIME" src/server.rs | head -20
```

- [ ] **Step 2: Create `src/server/database.rs`**

Create the new module with a focused function signature:

```rust
// src/server/database.rs
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use crate::common::config::Config;
use crate::storage::schema_health_check::run_schema_health_check;

pub async fn build_database_pool(config: &Config) -> Result<PgPool, Box<dyn std::error::Error>> {
    let pool = PgPoolOptions::new()
        .max_connections(config.database.max_connections)
        .connect(&config.database.url)
        .await?;

    // Schema health check
    run_schema_health_check(&pool).await?;

    // Runtime DB init guard (if enabled)
    if config.database.enable_runtime_init {
        // ... existing runtime init code
    }

    Ok(pool)
}
```

> **Important:** Copy the EXACT code from `server.rs` — do not rewrite logic. This task is extraction only.

- [ ] **Step 3: Create `src/server/mod.rs`**

Since `src/server.rs` becomes a directory module:

```bash
# Move server.rs to server/mod.rs
mkdir -p src/server
git mv src/server.rs src/server/mod.rs
```

- [ ] **Step 4: Wire the new module in `src/server/mod.rs`**

Add `mod database;` at the top, and replace the inline database code with:
```rust
let pool = database::build_database_pool(&config).await.map_err(|e| {
    tracing::error!("Failed to initialize database: {e}");
    std::process::exit(1);
})?;
```

- [ ] **Step 5: Verify compilation**

```bash
SQLX_OFFLINE=true cargo check --all-features 2>&1 | grep -E "^error" | head -10
```

- [ ] **Step 6: Commit**

```bash
git add src/server/
git commit -m "refactor: extract database initialization from server.rs into server/database.rs"
```

---

### Task 7: Extract `build_services` from `server.rs`

**Files:**
- Create: `src/server/services.rs`
- Modify: `src/server/mod.rs`

**Interfaces:**
- Consumes: `src/server/database.rs` from Task 6
- Produces: `pub fn build_service_container(pool: PgPool, config: &Config) -> ServiceContainer` in `src/server/services.rs`

- [ ] **Step 1: Identify the ServiceContainer construction block**

```bash
grep -n "ServiceContainer\|build_services\|let services" src/server/mod.rs | head -20
```

- [ ] **Step 2: Create `src/server/services.rs`**

Extract the `ServiceContainer::new(...)` call and all its dependencies (Redis client creation, cache setup, etc.) into a single function.

- [ ] **Step 3: Wire in `src/server/mod.rs`**

Replace the inline code with:
```rust
let services = services::build_service_container(pool.clone(), &config);
```

- [ ] **Step 4: Verify and commit**

```bash
SQLX_OFFLINE=true cargo check --all-features
git add src/server/
git commit -m "refactor: extract service container construction from server.rs"
```

---

### Task 8: Extract `build_router` from `server.rs`

**Files:**
- Create: `src/server/router.rs`
- Modify: `src/server/mod.rs`

**Interfaces:**
- Consumes: `src/server/services.rs` from Task 7
- Produces: `pub fn build_router(services: &ServiceContainer, config: &Config) -> Router` in `src/server/router.rs`

- [ ] **Step 1: Identify the router assembly block**

```bash
grep -n "Router::new\|axum::Router\|assembly::" src/server/mod.rs | head -20
```

- [ ] **Step 2: Create `src/server/router.rs`**

Extract all middleware layering, route merging, and listener binding into `build_router()`. This includes:
- CORS layer
- Security headers middleware
- Compression layer
- CSRF middleware
- Rate limiting setup
- Route assembly via `assembly::create_router()`

- [ ] **Step 3: Wire in `src/server/mod.rs`**

Replace the inline code with:
```rust
let router = router::build_router(&services, &config);
```

- [ ] **Step 4: Verify and commit**

```bash
SQLX_OFFLINE=true cargo check --all-features
git add src/server/
git commit -m "refactor: extract router assembly from server.rs"
```

---

### Task 9: Extract `configure_telemetry` from `server.rs`

**Files:**
- Create: `src/server/telemetry.rs`
- Modify: `src/server/mod.rs`

**Interfaces:**
- Consumes: None
- Produces: `pub fn init_telemetry(config: &Config) -> TracingGuard` where `TracingGuard` drops on shutdown

- [ ] **Step 1: Identify the tracing/logging setup block**

```bash
grep -n "tracing\|telemetry\|opentelemetry\|tracer" src/server/mod.rs | head -20
```

- [ ] **Step 2: Create `src/server/telemetry.rs`**

Extract the tracing subscriber initialization and any OpenTelemetry setup.

- [ ] **Step 3: Wire and commit**

```bash
SQLX_OFFLINE=true cargo check --all-features
git add src/server/
git commit -m "refactor: extract telemetry initialization from server.rs"
```

---

### Task 10: Final cleanup — `src/server/mod.rs` is now a thin orchestrator

**Files:**
- Modify: `src/server/mod.rs`

**Goal:** After Tasks 6-9, `src/server/mod.rs` should be a ~50-line main function that calls the extracted sub-modules in order.

- [ ] **Step 1: Verify the current state**

```bash
wc -l src/server/mod.rs
cat src/server/mod.rs
```

Expected: The file should now be mostly function calls — `init_telemetry()`, `build_database_pool()`, `build_service_container()`, `build_router()` — with the `main` function reduced to orchestration.

- [ ] **Step 2: Run the full gate suite**

```bash
cargo fmt --all -- --check
SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings
cargo test --doc --locked
SQLX_OFFLINE=true cargo test --test unit --features test-utils --locked
```

- [ ] **Step 3: Commit**

```bash
git add src/server/
git commit -m "refactor: server.rs now thin orchestrator — database, services, router, telemetry extracted to sub-modules"
```

---

## Verification Strategy

After each task:

```bash
# Quick gate (every commit)
cargo fmt --all -- --check
SQLX_OFFLINE=true cargo check --all-features

# Full gate (at task boundaries)
SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings
cargo test --doc --locked
SQLX_OFFLINE=true cargo test --test unit --features test-utils --locked
```

After the final task:
```bash
# Integration test compilation check (requires PostgreSQL to actually run)
SQLX_OFFLINE=true cargo test --test integration --features test-utils --no-run
```

## Rollback Strategy

Each task is an independent commit. If any task introduces a regression:
1. `git revert <commit>` to undo the single task
2. Re-run the full gate suite
3. Fix and re-commit

## Deferred to Separate Specs

The following domain review findings are out of scope for this plan and need their own design specs:

| Issue | Reason deferred |
|-------|----------------|
| Worker/replication architecture (like upstream Synapse multi-process workers) | Multi-week feature; needs architecture design, Redis pub/sub protocol spec, and staged rollout |
| Integration test infrastructure (test DB template management, CI speed) | Already partially addressed (B2); remaining work needs CI pipeline analysis |
| API coverage gap closure (80-97% → 100%) | Per-endpoint implementation work; needs per-endpoint spec |

---

## Deliverables

| Phase | Task | Output |
|-------|------|--------|
| Phase 1 | Task 1 | `STORAGE_MIGRATION_MAP.md` — full facade audit ledger |
| Phase 1 | Task 2 | ~40 pure-facade files deleted from `src/storage/` |
| Phase 1 | Task 3 | 24 consumer files migrated to direct `synapse_storage` imports |
| Phase 2 | Task 4 | 32 consumer files migrated to direct `synapse_services` imports |
| Phase 3 | Task 5 | Protocol versions v1.13→v1.18 with audit evidence |
| Phase 4 | Tasks 6-9 | `src/server/` directory with focused sub-modules |
| Phase 4 | Task 10 | `src/server/mod.rs` as thin ~50-line orchestrator |
