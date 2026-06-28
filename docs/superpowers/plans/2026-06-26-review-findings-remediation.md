# Review Findings Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Address 17 actionable INFORMATIONAL findings from the 2026-06-26 `/review`: fix 4 security issues, 3 performance anti-patterns, 3 test coverage gaps, and 7 maintainability problems across the codebase.

**Architecture:** Five independent workstreams (A-E), each producing its own commit series. Workstream A (Quick Wins) is lowest risk and builds momentum. Workstream B (Performance) reuses the HashSet pattern already established in the D1 refactoring. Workstream C (Security) requires careful config validation. Workstreams D and E add tests and eliminate duplication. No workstream touches files modified by another.

**Tech Stack:** Rust 2021 edition, axum, tokio, sqlx. No new dependencies.

## Global Constraints

- `cargo build --locked` must succeed after each task
- `SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings` must pass after each task
- `cargo fmt --all -- --check` must pass after each task
- `cargo test --test unit --features test-utils` must pass after each task
- No behavioral changes — only cleanup, refactoring, test additions, and security hardening
- Each task ends with a commit

---

## Workstream A: Quick Wins (5 tasks, ~30 min)

Low-risk maintainability fixes with immediate readability improvement.

### Task A1: Translate Chinese comments to English in `src/main.rs`

**Files:**
- Modify: `src/main.rs:21-28`

**Why:** Mixed-language comments create a barrier for non-Chinese-reading contributors. The comment blocks at the top of main() describe the bootstrap sequence in Chinese.

- [ ] **Step 1: Read the current comments and replace with English**

Read `src/main.rs` lines 21-28 to verify current content, then apply:

```rust
// Before (lines ~21-28):
// ========== 1. 预加载配置 ==========
// ========== 2. 初始化遥测服务 ==========
// ========== 3. 初始化全局日志与追踪 ==========
// ========== 4. 优雅停机 ==========

// After:
// ========== 1. Load configuration ==========
// ========== 2. Initialize telemetry ==========
// ========== 3. Initialize global logging and tracing ==========
// ========== 4. Graceful shutdown ==========
```

- [ ] **Step 2: Verify build and commit**

Run: `cargo build --locked 2>&1`
Expected: Build succeeds (comment-only change).

```bash
git add src/main.rs
git commit -m "$(cat <<'EOF'
chore: translate Chinese bootstrap comments to English in main.rs

Improves accessibility for non-Chinese-reading contributors.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task A2: Delete dead code in `benches/common.rs`

**Files:**
- Delete: `benches/common.rs`

**Why:** The 196-line file defines 9 public items (`BenchmarkConfig`, `configure_criterion`, etc.) that are never imported by any benchmark binary. Grep confirmed zero references.

- [ ] **Step 1: Verify the file is dead**

Run:
```bash
grep -rn 'benches::common\|common::BenchmarkConfig\|common::configure_criterion' benches/ --include='*.rs'
```
Expected: No output (no benchmark imports any item from common.rs).

- [ ] **Step 2: Delete the file**

```bash
rm benches/common.rs
```

- [ ] **Step 3: Verify build**

Run: `cargo build --locked 2>&1`
Expected: Build succeeds.

- [ ] **Step 4: Commit**

```bash
git add benches/common.rs
git commit -m "$(cat <<'EOF'
chore: delete dead benchmark common.rs (never imported)

All 9 public items (BenchmarkConfig, configure_criterion, etc.) are
never used by any benchmark binary.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task A3: Extract magic numbers to named constants in `src/server.rs` (Part 1: time durations)

**Files:**
- Modify: `src/server.rs` — add const block near top of file, replace literals

**Why:** Bare numeric literals (`5`, `60`, `30`, `86400`) scattered through server.rs reduce readability and make tuning error-prone. Extract to named consts with doc comments.

- [ ] **Step 1: Add const block after the `use` statements in server.rs**

Find the last `use` / `const` block before `pub struct SynapseServer`, insert after it:

```rust
// --- Tuning constants ---

/// Minimum idle database connections maintained in the connection pool.
const DB_MIN_IDLE_CONNECTIONS: u32 = 5;

/// Interval (seconds) between background maintenance task ticks.
const BACKGROUND_TASK_INTERVAL_SECS: u64 = 60;

/// Minimum interval (seconds) between background task executions to prevent
/// tight loops when a task completes quickly.
const MIN_BACKGROUND_INTERVAL_SECS: u64 = 10;

/// Capacity of the tokio broadcast channel used for graceful shutdown signaling.
const SHUTDOWN_BROADCAST_CAPACITY: usize = 3;

/// Maximum retries for federation destination connection attempts
/// before marking a destination as unreachable.
const FEDERATION_RETRY_MAX_COUNT: u32 = 5;

/// Timeout (seconds) for draining in-flight requests during graceful shutdown.
const DRAIN_TIMEOUT_SECS: u64 = 30;

/// Interval (seconds) between megolm session key cleanup runs.
const MEGOLM_CLEANUP_INTERVAL_SECS: u64 = 6 * 3600;

/// Interval (seconds) between event pruning runs.
const PRUNING_INTERVAL_SECS: u64 = 86400;

/// Database acquire timeout string for sqlx pool configuration.
const DB_ACQUIRE_TIMEOUT: &str = "30s";

/// Database idle timeout string for sqlx pool configuration.
const DB_IDLE_TIMEOUT: &str = "10s";

/// Database max lifetime string for sqlx pool configuration.
const DB_MAX_LIFETIME: &str = "60s";
```

- [ ] **Step 2: Replace each literal with its constant**

Replace each bare literal throughout `src/server.rs` with the corresponding const name. Search for each value and verify the context matches before replacing.

| Literal | Replace with | Location context |
|---------|-------------|------------------|
| `.min_connections(5)` | `.min_connections(DB_MIN_IDLE_CONNECTIONS)` | Pool builder |
| `interval(60)` background | `interval(BACKGROUND_TASK_INTERVAL_SECS)` | Background task spawn |
| `.max(10)` min interval | `.max(MIN_BACKGROUND_INTERVAL_SECS)` | Duration clamp |
| `broadcast::channel(3)` | `broadcast::channel(SHUTDOWN_BROADCAST_CAPACITY)` | Shutdown signal |
| `retries: 5` federation | `retries: FEDERATION_RETRY_MAX_COUNT` | Federation config |
| `Duration::from_secs(30)` drain | `Duration::from_secs(DRAIN_TIMEOUT_SECS)` | Shutdown drain |
| `6 * 3600` megolm | `MEGOLM_CLEANUP_INTERVAL_SECS` | Megolm cleanup |
| `86400` pruning | `PRUNING_INTERVAL_SECS` | Pruning interval |
| `"30s"` acquire | `DB_ACQUIRE_TIMEOUT` | Pool config |
| `"10s"` idle | `DB_IDLE_TIMEOUT` | Pool config |
| `"60s"` max lifetime | `DB_MAX_LIFETIME` | Pool config |

- [ ] **Step 3: Verify build and clippy**

Run:
```bash
cargo build --locked 2>&1
SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings 2>&1 | tail -5
```
Expected: Build succeeds, clippy clean.

- [ ] **Step 4: Commit**

```bash
git add src/server.rs
git commit -m "$(cat <<'EOF'
refactor: extract magic numbers to named constants in server.rs

Replace 12 bare numeric/string literals with descriptively-named consts:
DB timeouts, background intervals, federation retry cap, shutdown drain
timeout, pruning/megolm cleanup intervals.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task A4: Replace stale task tracking codes in `src/server.rs`

**Files:**
- Modify: `src/server.rs` — comment-only changes

**Why:** Internal codes like `P1-12`, `P3-11`, `OPT-07` and version claims like `v1.153 alignment` will rot over time and mislead maintainers. Replace with plain-language rationale.

- [ ] **Step 1: Find all stale codes**

Run:
```bash
grep -n 'P[0-9]-[0-9]\|OPT-[0-9]\|v1\.15[0-9]' src/server.rs
```

- [ ] **Step 2: Replace each with plain-language comment**

For each match, replace the code with a short rationale comment. Example transformations:

```
// P1-12: room summary background recalculation
→ // Background room summary recalculation

// P3-11: device list change pruning
→ // Device list change pruning — keeps device_lists table bounded

// OPT-07: federation queue dedup
→ // Federation queue deduplication — prevents duplicate event processing

// v1.153 alignment: rate limit config path
→ // Rate limit config file path
```

- [ ] **Step 3: Verify build**

Run: `cargo build --locked 2>&1`
Expected: Build succeeds (comment-only changes).

- [ ] **Step 4: Commit**

```bash
git add src/server.rs
git commit -m "$(cat <<'EOF'
chore: replace stale task tracking codes with plain-language comments

P1-12, P3-11, OPT-07, and v1.153 version references replaced with
descriptive rationale that won't rot over time.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task A5: Extract DRY rate-limit config helper in `src/server.rs`

**Files:**
- Modify: `src/server.rs:289-300`

**Why:** An identical 2-line rate-limit config creation block appears in both the `Err` match arm and the `else` branch. Extract to a private helper function.

- [ ] **Step 1: Read the duplicated block**

Read `src/server.rs` around lines 285-305 to see both occurrences:

```rust
// Pattern (appears twice):
let rate_limit_config_manager = Arc::new(crate::common::rate_limit::RateLimitConfigManager::new(
    config_path.to_path_buf(),
));
```

- [ ] **Step 2: Add the helper function**

Add near the other helper functions in `src/server.rs` (e.g., after `dehydrated_device_cleanup_interval`):

```rust
fn create_rate_limit_manager(config_path: &std::path::Path) -> Arc<crate::common::rate_limit::RateLimitConfigManager> {
    Arc::new(crate::common::rate_limit::RateLimitConfigManager::new(
        config_path.to_path_buf(),
    ))
}
```

- [ ] **Step 3: Replace both occurrences**

Replace each 2-line block with:
```rust
let rate_limit_config_manager = create_rate_limit_manager(&config_path);
```

- [ ] **Step 4: Verify build and clippy**

Run:
```bash
cargo build --locked 2>&1
SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings 2>&1 | tail -5
```
Expected: Build succeeds, clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src/server.rs
git commit -m "$(cat <<'EOF'
refactor: extract create_rate_limit_manager helper in server.rs

The identical 2-line RateLimitConfigManager construction appears in
both the config-not-found Err arm and the else branch. DRY it up.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Workstream B: Performance Fixes (3 tasks, ~45 min)

Replace O(n²) and O(n*m) patterns with HashSet-based O(1) lookups. Fix blocking call in async context.

### Task B1: Replace `Vec<String>` with `HashSet<String>` in `filter_users_with_shared_rooms`

**Files:**
- Modify: `src/web/routes/response_helpers.rs:44-71` — return type and internal collection
- Modify: `src/web/routes/e2ee/keys.rs:240` — caller `.contains()` already O(1) once HashSet

**Why:** `filter_users_with_shared_rooms` returns `Vec<String>` and callers use `.contains()` in O(n) loops, creating O(n*m) behavior. Returning `HashSet<String>` makes lookups O(1). The function was already fixed for N+1 (CRITICAL-001), but still returns Vec.

- [ ] **Step 1: Change return type**

In `src/web/routes/response_helpers.rs`, change the function signature and body:

```rust
// Before:
pub(crate) async fn filter_users_with_shared_rooms(
    state: &AppState,
    current_user_id: &str,
    requested_users: &[String],
) -> Vec<String> {
    let mut allowed = vec![current_user_id.to_string()];
    // ...
    allowed.extend(shared);
    allowed
}

// After:
use std::collections::HashSet;

pub(crate) async fn filter_users_with_shared_rooms(
    state: &AppState,
    current_user_id: &str,
    requested_users: &[String],
) -> HashSet<String> {
    let mut allowed = HashSet::new();
    allowed.insert(current_user_id.to_string());
    // ...
    for uid in shared {
        allowed.insert(uid);
    }
    allowed
}
```

Also update the test at line ~48 to use `HashSet` assertions.

- [ ] **Step 2: Update callers**

In `src/web/routes/e2ee/keys.rs`, find the call site (near line 240). The `.contains()` calls already work with both `Vec` and `HashSet`. Verify no callers iterate over the return value.

Run:
```bash
grep -rn 'filter_users_with_shared_rooms' src/ --include='*.rs'
```
Expected: All callers use `.contains()` — no index-based access.

- [ ] **Step 3: Verify build and tests**

Run:
```bash
cargo build --locked 2>&1
cargo test --test unit --features test-utils 2>&1 | tail -10
```
Expected: Build succeeds, all unit tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/web/routes/response_helpers.rs src/web/routes/e2ee/keys.rs
git commit -m "$(cat <<'EOF'
perf: return HashSet from filter_users_with_shared_rooms

O(1) lookups replace O(n) Vec::contains() scans in e2ee key query loops,
eliminating O(n*m) behavior when many device keys are requested.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task B2: Replace `Vec` linear scan with `HashSet` in presence handler

**Files:**
- Modify: `src/web/routes/handlers/presence.rs:193`

**Why:** For each `target_id` in `subscriptions` (Vec), the code calls `presences.iter().any(|p| p["user_id"] == *target_id)` — a linear scan over presences. Build a HashSet of already-present user_ids before the loop.

- [ ] **Step 1: Read the problematic loop**

Read `src/web/routes/handlers/presence.rs` lines 185-210 to understand the full context.

- [ ] **Step 2: Build a HashSet before the loop**

```rust
// Before (conceptual):
for target_id in &subscriptions {
    if presences.iter().any(|p| p["user_id"] == *target_id) {
        continue;
    }
    // ... fetch presence for target_id, push to presences
}

// After:
use std::collections::HashSet;
let present_user_ids: HashSet<&str> = presences
    .iter()
    .filter_map(|p| p["user_id"].as_str())
    .collect();

for target_id in &subscriptions {
    if present_user_ids.contains(target_id.as_str()) {
        continue;
    }
    // ... fetch presence for target_id, push to presences
}
```

- [ ] **Step 3: Verify build and clippy**

Run:
```bash
cargo build --locked 2>&1
SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings 2>&1 | tail -5
```
Expected: Build succeeds, clippy clean.

- [ ] **Step 4: Commit**

```bash
git add src/web/routes/handlers/presence.rs
git commit -m "$(cat <<'EOF'
perf: use HashSet for O(1) presence lookup in subscription loop

Replaces O(n) Vec::iter().any() scan with O(1) HashSet::contains(),
eliminating quadratic behavior when many presence subscriptions exist.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task B3: Replace blocking `std::process::Command` with `tokio::process::Command`

**Files:**
- Modify: `src/bin/run_migrations.rs:16`

**Why:** `std::process::Command::new("bash").status()` is called synchronously inside `#[tokio::main] async fn main()`, blocking the tokio runtime for the duration of the migration script.

- [ ] **Step 1: Read the current code**

Read `src/bin/run_migrations.rs` to see the exact call site.

- [ ] **Step 2: Replace with tokio**

```rust
// Before:
use std::process::Command;
let status = Command::new("bash")
    .arg(&script_path)
    .status()
    .expect("failed to execute migration script");

// After:
use tokio::process::Command;
let status = Command::new("bash")
    .arg(&script_path)
    .status()
    .await
    .expect("failed to execute migration script");
```

- [ ] **Step 3: Verify build**

Run:
```bash
cargo build --locked --bin synapse_worker 2>&1
```
Expected: Build succeeds (this file is part of the worker binary).

- [ ] **Step 4: Commit**

```bash
git add src/bin/run_migrations.rs
git commit -m "$(cat <<'EOF'
perf: use tokio::process::Command in run_migrations.rs

Replaces std::process::Command (blocking) with tokio::process::Command
(async), so the migration script no longer blocks the tokio runtime.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Workstream C: Security Hardening (3 tasks, ~1 hr)

Fix CSRF secret derivation, add SSO redirect validation, and warn on non-HTTPS base URLs.

### Task C1: Use random HMAC secret for CSRF tokens instead of server name

**Files:**
- Modify: `src/web/middleware/csrf.rs` — CsrfTokenManager construction
- Modify: `synapse-common/src/config/security.rs` or equivalent — add csrf_secret field
- Modify: `src/server.rs` — pass secret to CsrfTokenManager

**Why:** `CsrfTokenManager::new(state.services.core.server_name.clone())` uses the publicly-known server name as the HMAC secret. A random secret provides defense-in-depth against session ID leakage.

- [ ] **Step 1: Check if SecurityConfig already has a suitable secret field**

Run:
```bash
grep -rn 'csrf\|hmac_secret\|signing_key' synapse-common/src/config/ --include='*.rs'
```

- [ ] **Step 2: If no secret exists, add one to the config, falling back to a random value**

In the `SecurityConfig` or equivalent config struct, add:

```rust
/// HMAC secret for CSRF token signing. If not explicitly configured,
/// a cryptographically random 32-byte secret is generated at startup.
/// This secret is ephemeral (not persisted), so CSRF tokens from
/// previous server runs will be invalid after restart.
#[serde(default = "default_csrf_secret")]
pub csrf_secret: String,

fn default_csrf_secret() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
```

- [ ] **Step 3: Update CsrfTokenManager::new to accept the secret**

Change the constructor to accept `&str` instead of deriving from server name.

- [ ] **Step 4: Update the call site in server.rs**

```rust
// Before:
CsrfTokenManager::new(state.services.core.server_name.clone())

// After:
CsrfTokenManager::new(config.security.csrf_secret.clone())
```

- [ ] **Step 5: Verify build and tests**

Run:
```bash
cargo build --locked 2>&1
cargo test --test unit --features test-utils 2>&1 | tail -10
```
Expected: Build succeeds, all unit tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/web/middleware/csrf.rs synapse-common/src/config/security.rs src/server.rs
git commit -m "$(cat <<'EOF'
security: use random HMAC secret for CSRF tokens

Replaces the publicly-known server name as CSRF HMAC secret with a
cryptographically random 32-byte value, generated at startup if not
explicitly configured. Provides defense-in-depth against forged
CSRF tokens in case of session ID leakage.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task C2: Add WARN log when public_baseurl falls back to HTTP

**Files:**
- Modify: `synapse-common/src/config/server.rs` — `get_public_baseurl()` method

**Why:** `get_public_baseurl()` defaults to `http://` when `public_baseurl` is unset, silently downgrading security for OIDC/SAML SSO callbacks. A WARN-level log alerts operators to misconfiguration.

- [ ] **Step 1: Find get_public_baseurl()**

Run:
```bash
grep -n 'fn get_public_baseurl\|public_baseurl' synapse-common/src/config/server.rs
```

- [ ] **Step 2: Add WARN log in the HTTP fallback branch**

```rust
pub fn get_public_baseurl(&self) -> String {
    if let Some(ref url) = self.public_baseurl {
        return url.clone();
    }
    // Fallback: construct from server_name
    let fallback = format!("http://{}", self.server_name);
    #[cfg(not(debug_assertions))]
    {
        tracing::warn!(
            server_name = %self.server_name,
            "public_baseurl is not configured — falling back to HTTP (http://{}). \
             Set public_baseurl to an HTTPS URL in production.",
            self.server_name
        );
    }
    fallback
}
```

Use `#[cfg(not(debug_assertions))]` to avoid noise during local development.

- [ ] **Step 3: Verify build**

Run: `cargo build --locked 2>&1`
Expected: Build succeeds.

- [ ] **Step 4: Commit**

```bash
git add synapse-common/src/config/server.rs
git commit -m "$(cat <<'EOF'
security: warn when public_baseurl falls back to HTTP

Silent HTTP fallback for unconfigured public_baseurl can expose
OIDC/SAML callback URLs. Now emits WARN in non-debug builds.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task C3: Validate SSO redirect URLs against config allowlist

**Files:**
- Modify: `src/web/routes/auth/sso_redirect.rs` — `is_safe_redirect_url()`
- Modify: `synapse-common/src/config/` — add `sso_redirect_allowlist` config field

**Why:** `is_safe_redirect_url()` allows any hostname-based URL as a valid SSO post-login redirect target, only blocking IP addresses and dangerous schemes. An attacker can craft a login URL redirecting to an external phishing site.

- [ ] **Step 1: Read is_safe_redirect_url()**

Run:
```bash
grep -n 'is_safe_redirect_url' src/web/routes/auth/sso_redirect.rs
```
Read the full function to understand the current validation logic.

- [ ] **Step 2: Add the allowlist config field**

In the appropriate config struct (check which config SSO routes have access to):

```rust
/// Allowed redirect URL prefixes for SSO post-login redirects.
/// If empty, only same-origin paths (starting with `/`) are permitted.
/// Example: `["https://app.example.com/"]`
#[serde(default)]
pub sso_redirect_allowlist: Vec<String>,
```

- [ ] **Step 3: Update is_safe_redirect_url() to check the allowlist**

Add after existing scheme/host checks:

```rust
fn is_safe_redirect_url(url: &str, allowlist: &[String]) -> bool {
    // Existing checks: block IPs, localhost, dangerous schemes...

    // If the URL is an absolute URL with a host, validate against allowlist
    if let Ok(parsed) = url::Url::parse(url) {
        if parsed.has_host() {
            let url_str = parsed.as_str();
            if allowlist.is_empty() {
                // No allowlist configured — only permit same-origin (relative) paths
                return false;
            }
            return allowlist.iter().any(|allowed| url_str.starts_with(allowed));
        }
    }

    // Same-origin path (starts with /) — always safe
    url.starts_with('/')
}
```

- [ ] **Step 4: Update the call site to pass the allowlist**

Find where `is_safe_redirect_url` is called and thread the config through.

- [ ] **Step 5: Verify build and clippy**

Run:
```bash
cargo build --locked 2>&1
SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings 2>&1 | tail -5
```
Expected: Build succeeds, clippy clean.

- [ ] **Step 6: Commit**

```bash
git add src/web/routes/auth/sso_redirect.rs synapse-common/src/config/
git commit -m "$(cat <<'EOF'
security: validate SSO redirect URLs against config allowlist

is_safe_redirect_url() previously allowed any hostname-based URL as an
SSO post-login redirect target. Now requires allowlist configuration
for cross-origin redirects; same-origin paths (/) always permitted.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Workstream D: Test Coverage (3 tasks, ~1 hr)

Add unit tests for recently-extracted e2ee sub-modules and middleware edge cases.

### Task D1: Add unit tests for `src/web/routes/e2ee/devices.rs`

**Files:**
- Modify: `src/web/routes/e2ee/devices.rs` — add `#[cfg(test)] mod tests` at bottom

**Why:** This 584-line sub-module (19 handler functions) extracted from `e2ee_routes.rs` has zero inline unit tests. Test the cursor encode/decode round-trip and edge cases.

- [ ] **Step 1: Find testable pure functions in devices.rs**

Key candidates:
- `encode_key_request_cursor` / `decode_key_request_cursor` — pure encode/decode
- Any validation or parsing helpers

- [ ] **Step 2: Add tests module at end of file**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_request_cursor_round_trip() {
        let original = "s12345_token";
        let encoded = encode_key_request_cursor(original);
        let decoded = decode_key_request_cursor(&encoded);
        assert_eq!(decoded, Some(original.to_string()));
    }

    #[test]
    fn test_key_request_cursor_empty_string() {
        let decoded = decode_key_request_cursor("");
        assert_eq!(decoded, None);
    }

    #[test]
    fn test_key_request_cursor_invalid_base64() {
        let decoded = decode_key_request_cursor("!!!not-valid-base64!!!");
        assert_eq!(decoded, None);
    }

    #[test]
    fn test_key_request_cursor_with_special_chars() {
        let original = "s12345:a/b+c=";
        let encoded = encode_key_request_cursor(original);
        let decoded = decode_key_request_cursor(&encoded);
        assert_eq!(decoded, Some(original.to_string()));
    }

    #[test]
    fn test_key_request_cursor_with_newline() {
        let original = "s12345\ntest";
        let encoded = encode_key_request_cursor(original);
        let decoded = decode_key_request_cursor(&encoded);
        assert_eq!(decoded, Some(original.to_string()));
    }
}
```

- [ ] **Step 3: Run the new tests**

Run:
```bash
cargo test --test unit --features test-utils devices::tests 2>&1 | tail -10
```
Expected: All 5 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/web/routes/e2ee/devices.rs
git commit -m "$(cat <<'EOF'
test: add unit tests for e2ee devices.rs key_request_cursor round-trip

Covers encode/decode round-trip, empty string, invalid base64,
special characters, and embedded newlines.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task D2: Add unit tests for `src/web/routes/e2ee/keys.rs`

**Files:**
- Modify: `src/web/routes/e2ee/keys.rs` — add `#[cfg(test)] mod tests` at bottom

**Why:** This 582-line sub-module has zero inline unit tests. Contains `parse_stream_id` and router factory functions.

- [ ] **Step 1: Find testable pure functions in keys.rs**

Key candidates:
- `parse_stream_id(s: &str) -> Option<i64>` — pure parsing function

- [ ] **Step 2: Add tests module at end of file**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_stream_id_with_s_prefix() {
        assert_eq!(parse_stream_id("s12345"), Some(12345));
    }

    #[test]
    fn test_parse_stream_id_zero() {
        assert_eq!(parse_stream_id("s0"), Some(0));
    }

    #[test]
    fn test_parse_stream_id_max_i64() {
        assert_eq!(parse_stream_id("s9223372036854775807"), Some(9223372036854775807));
    }

    #[test]
    fn test_parse_stream_id_empty_string() {
        assert_eq!(parse_stream_id(""), None);
    }

    #[test]
    fn test_parse_stream_id_no_s_prefix() {
        assert_eq!(parse_stream_id("12345"), None);
    }

    #[test]
    fn test_parse_stream_id_non_numeric() {
        assert_eq!(parse_stream_id("sabc"), None);
    }

    #[test]
    fn test_parse_stream_id_negative() {
        assert_eq!(parse_stream_id("s-1"), None);
    }

    #[test]
    fn test_parse_stream_id_overflow() {
        assert_eq!(parse_stream_id("s9223372036854775808"), None);
    }
}
```

- [ ] **Step 3: Run the new tests**

Run:
```bash
cargo test --test unit --features test-utils keys::tests 2>&1 | tail -10
```
Expected: All 8 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/web/routes/e2ee/keys.rs
git commit -m "$(cat <<'EOF'
test: add unit tests for e2ee keys.rs parse_stream_id

Covers prefix parsing, zero, max i64, empty, missing prefix,
non-numeric, negative, and overflow edge cases.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task D3: Add unit test for `method_not_allowed_middleware` pass-through

**Files:**
- Modify: `src/web/middleware/security.rs` — add test in existing `#[cfg(test)]` module

**Why:** Integration tests cover the Matrix-compliant JSON injection path, but no unit test validates the non-empty-body pass-through behavior (the middleware should not modify responses that already have a body).

- [ ] **Step 1: Find existing test module and the middleware logic**

Read lines 199-220 of `src/web/middleware/security.rs` to see the pass-through logic.

- [ ] **Step 2: Add test to existing `#[cfg(test)] mod tests`**

```rust
#[tokio::test]
async fn test_method_not_allowed_passthrough_with_body() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware,
        response::Response,
        routing::get,
        Router,
    };
    use tower::ServiceExt;

    // A handler that returns a custom 405 with a body
    async fn custom_405() -> Response {
        Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Body::from(r#"{"error":"custom"}"#))
            .unwrap()
    }

    let app = Router::new()
        .route("/test", get(custom_405))
        .layer(middleware::from_fn(method_not_allowed_middleware));

    // Send a POST (method not allowed for this GET-only route)
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert_eq!(body_str, r#"{"error":"custom"}"#);
}
```

- [ ] **Step 3: Run the test**

Run:
```bash
cargo test --test unit --features test-utils security::tests::test_method_not_allowed_passthrough 2>&1
```
Expected: Test passes.

- [ ] **Step 4: Commit**

```bash
git add src/web/middleware/security.rs
git commit -m "$(cat <<'EOF'
test: add unit test for method_not_allowed_middleware body pass-through

Verifies the middleware does not modify 405 responses that already
have a body (e.g., custom error responses from route handlers).

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Workstream E: DRY Refactoring (1 task, ~20 min)

### Task E1: Extract `prune_step` macro for 5 identical match blocks in `src/server.rs`

**Files:**
- Modify: `src/server.rs:655-697`

**Why:** Five nearly identical match blocks for pruning operations (device list changes, presence, one-time keys, to-device transactions, token blacklist, federation queue) share the same Ok/Err handling pattern.

- [ ] **Step 1: Read the five match blocks**

Read `src/server.rs` lines 655-697 to see the exact pattern.

- [ ] **Step 2: Define the macro**

Add near the top of `src/server.rs` (before any fn that uses it):

```rust
/// Helper macro for pruning background tasks.
/// Each pruning operation follows the same pattern: call an async function,
/// log success with a count, or log a warning on failure.
macro_rules! prune_step {
    ($label:expr, $prune_fn:expr) => {{
        match $prune_fn.await {
            Ok(count) => {
                if count > 0 {
                    tracing::info!(
                        count = count,
                        "{}: pruned {count} expired entries",
                        $label
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "{}: prune operation failed",
                    $label
                );
            }
        }
    }};
}
```

- [ ] **Step 3: Replace the 5 match blocks**

Replace each individual match block with a `prune_step!` invocation. Example:

```rust
// Before:
match device_service.prune_expired_device_list_changes().await {
    Ok(count) => {
        if count > 0 {
            tracing::info!(count = count, "Pruned {count} expired device list changes");
        }
    }
    Err(e) => {
        tracing::warn!(error = %e, "Failed to prune device list changes");
    }
}

// After:
prune_step!("device list changes", device_service.prune_expired_device_list_changes());
```

Repeat for the remaining 4 blocks (presence, one-time keys, to-device, token blacklist, federation queue).

- [ ] **Step 4: Verify build and clippy**

Run:
```bash
cargo build --locked 2>&1
SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings 2>&1 | tail -5
```
Expected: Build succeeds, clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src/server.rs
git commit -m "$(cat <<'EOF'
refactor: extract prune_step macro for 5 identical pruning blocks

Five pruning operations (device list, presence, one-time keys, to-device,
token blacklist, federation queue) shared the same Ok/Err logging pattern.
DRY them up with a macro.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Execution Order

Workstreams are independent and can run in parallel if using subagents.

**Sequential (single agent):** A → B → D → E → C
- A first (lowest risk, builds momentum)
- C last (security changes need most review)

**Parallel (subagents):** A + B + D + E together, then C last

## Verification Checklist

After all tasks complete:

- [ ] `cargo build --locked` — succeeds
- [ ] `SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings` — clean
- [ ] `cargo fmt --all -- --check` — clean
- [ ] `cargo test --test unit --features test-utils` — all pass
- [ ] `SQLX_OFFLINE=true cargo check --test integration --features test-utils` — compiles (full run takes hours)
- [ ] `git diff --stat origin/main` — review the complete change set
