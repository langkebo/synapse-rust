# Zombie Code Cleanup & Crate Migration Completion

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Delete ~53 zombie source files that are never compiled, convert 8 byte-identical duplicated files to thin facades, and migrate `edu.rs` to `synapse-federation`.

**Architecture:** The codebase is mid-migration from monolithic `src/` to workspace sub-crates (`synapse-cache`, `synapse-e2ee`, `synapse-common`, `synapse-federation`, `synapse-services`, `synapse-storage`). Many files in `src/` are zombie leftovers (on disk but never compiled because `mod.rs` re-exports from sub-crates without `pub mod` declarations). Other files are byte-identical duplicates between `src/common/` and `synapse-common/src/`. This plan removes zombies, converts duplicates to facades, and completes the remaining federation migration.

**Tech Stack:** Rust, cargo, git

## Global Constraints

- `cargo build --locked` must succeed after each task
- `cargo clippy --all-features --locked -- -D warnings` must pass after each task
- `cargo fmt --all -- --check` must pass after each task
- All existing tests must continue to pass: `cargo test --all-features --locked -- --test-threads=4`
- No behavioral changes — this is purely structural cleanup
- Each task ends with a commit

---

### Task 1: Delete zombie files in `src/cache/`

**Files:**
- Delete: `src/cache/circuit_breaker.rs`
- Delete: `src/cache/federation_signature_cache.rs`
- Delete: `src/cache/invalidation.rs`
- Delete: `src/cache/query_cache.rs`
- Delete: `src/cache/strategy.rs`

**Why:** `src/cache/mod.rs` uses `pub use synapse_cache::*;` with no `pub mod` declarations — these 5 files exist on disk but are never compiled. The real code lives in `synapse-cache/src/`.

- [ ] **Step 1: Verify files are zombie (no mod declaration references them)**

Run:
```bash
grep -n 'pub mod' src/cache/mod.rs
```
Expected: No output (only `pub use synapse_cache::xxx;` lines exist).

- [ ] **Step 2: Delete the zombie files**

```bash
rm src/cache/circuit_breaker.rs
rm src/cache/federation_signature_cache.rs
rm src/cache/invalidation.rs
rm src/cache/query_cache.rs
rm src/cache/strategy.rs
```

- [ ] **Step 3: Verify build still succeeds**

Run:
```bash
cargo build --locked 2>&1
```
Expected: Build succeeds with no errors.

- [ ] **Step 4: Verify directory structure**

Run:
```bash
ls src/cache/
```
Expected: Only `mod.rs` remains.

- [ ] **Step 5: Commit**

```bash
git add src/cache/
git commit -m "$(cat <<'EOF'
chore: remove 5 zombie files from src/cache/

These files were never compiled — src/cache/mod.rs re-exports from
synapse-cache without any `pub mod` declarations. The real code
lives in synapse-cache/src/.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: Delete zombie directories in `src/e2ee/` (part 1 — directories not declared in e2ee/mod.rs)

**Files:**
- Delete: `src/e2ee/backup/` (mod.rs, models.rs, service.rs, storage.rs)
- Delete: `src/e2ee/crypto/` (mod.rs, aes.rs, ed25519.rs)
- Delete: `src/e2ee/device_trust/` (mod.rs, models.rs, service.rs, storage.rs)
- Delete: `src/e2ee/key_request/` (mod.rs, models.rs, service.rs, storage.rs)
- Delete: `src/e2ee/olm/` (mod.rs, models.rs, service.rs, session.rs, storage.rs)

**Why:** `src/e2ee/mod.rs` imports these via `pub use synapse_e2ee::backup;` etc., NOT `pub mod backup;`. The local directories are never compiled. The real code lives in `synapse-e2ee/src/`.

- [ ] **Step 1: Confirm these modules use `pub use` not `pub mod`**

Run:
```bash
grep -E 'pub (use|mod)' src/e2ee/mod.rs | grep -E 'backup|crypto|device_trust|key_request|olm'
```
Expected:
```
pub use synapse_e2ee::backup;
pub use synapse_e2ee::crypto;
pub use synapse_e2ee::device_trust;
pub use synapse_e2ee::key_request;
pub use synapse_e2ee::olm;
```
No `pub mod backup;` etc.

- [ ] **Step 2: Delete the zombie directories**

```bash
rm -rf src/e2ee/backup
rm -rf src/e2ee/crypto
rm -rf src/e2ee/device_trust
rm -rf src/e2ee/key_request
rm -rf src/e2ee/olm
```

- [ ] **Step 3: Verify build still succeeds**

Run:
```bash
cargo build --locked 2>&1
```
Expected: Build succeeds with no errors.

- [ ] **Step 4: Commit**

```bash
git add src/e2ee/
git commit -m "$(cat <<'EOF'
chore: remove 5 zombie directories from src/e2ee/ (part 1)

backup, crypto, device_trust, key_request, olm — imported via
pub use synapse_e2ee::*, not pub mod. Directories were never compiled.
Real code lives in synapse-e2ee/src/.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: Delete zombie directories in `src/e2ee/` (part 2)

**Files:**
- Delete: `src/e2ee/secure_backup/` (mod.rs, models.rs, service.rs)
- Delete: `src/e2ee/signature/` (mod.rs, models.rs, service.rs, storage.rs)
- Delete: `src/e2ee/to_device/` (mod.rs, service.rs, storage.rs)
- Delete: `src/e2ee/verification/` (mod.rs, models.rs, service.rs, storage.rs)

**Why:** Same as Task 2 — imported via `pub use synapse_e2ee::*`, not `pub mod`.

- [ ] **Step 1: Confirm these modules use `pub use` not `pub mod`**

Run:
```bash
grep -E 'pub (use|mod)' src/e2ee/mod.rs | grep -E 'secure_backup|signature|to_device|verification'
```
Expected:
```
pub use synapse_e2ee::secure_backup;
pub use synapse_e2ee::signature;
pub use synapse_e2ee::to_device;
pub use synapse_e2ee::verification;
```

- [ ] **Step 2: Delete the zombie directories**

```bash
rm -rf src/e2ee/secure_backup
rm -rf src/e2ee/signature
rm -rf src/e2ee/to_device
rm -rf src/e2ee/verification
```

- [ ] **Step 3: Verify build and clippy**

Run:
```bash
cargo build --locked 2>&1 && cargo clippy --all-features --locked -- -D warnings 2>&1
```
Expected: Both succeed with no errors.

- [ ] **Step 4: Commit**

```bash
git add src/e2ee/
git commit -m "$(cat <<'EOF'
chore: remove 4 zombie directories from src/e2ee/ (part 2)

secure_backup, signature, to_device, verification — imported via
pub use synapse_e2ee::*, not pub mod. Directories were never compiled.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: Delete zombie sub-files in `src/e2ee/cross_signing/` and `src/e2ee/ssss/`

**Files:**
- Delete: `src/e2ee/cross_signing/models.rs`
- Delete: `src/e2ee/cross_signing/service.rs`
- Delete: `src/e2ee/cross_signing/storage.rs`
- Delete: `src/e2ee/ssss/models.rs`
- Delete: `src/e2ee/ssss/service.rs`
- Delete: `src/e2ee/ssss/storage.rs`

**Why:** `src/e2ee/mod.rs` declares `pub mod cross_signing;` and `pub mod ssss;`, so `cross_signing/mod.rs` and `ssss/mod.rs` ARE compiled. But those `mod.rs` files contain only `pub use synapse_e2ee::cross_signing::*;` and `pub use synapse_e2ee::ssss::*;` without declaring `pub mod models;` etc. So `models.rs`, `service.rs`, `storage.rs` in each dir are zombies.

- [ ] **Step 1: Confirm sub-modules are not declared**

Run:
```bash
echo "=== cross_signing/mod.rs ===" && cat src/e2ee/cross_signing/mod.rs
echo "=== ssss/mod.rs ===" && cat src/e2ee/ssss/mod.rs
```
Expected: Each file contains only `pub use synapse_e2ee::<name>::*;` with no `pub mod`.

- [ ] **Step 2: Delete the zombie sub-files**

```bash
rm src/e2ee/cross_signing/models.rs
rm src/e2ee/cross_signing/service.rs
rm src/e2ee/cross_signing/storage.rs
rm src/e2ee/ssss/models.rs
rm src/e2ee/ssss/service.rs
rm src/e2ee/ssss/storage.rs
```

- [ ] **Step 3: Verify directory contents after deletion**

Run:
```bash
echo "=== cross_signing/ ===" && ls src/e2ee/cross_signing/
echo "=== ssss/ ===" && ls src/e2ee/ssss/
```
Expected: Only `mod.rs` remains in each directory.

- [ ] **Step 4: Verify build still succeeds**

Run:
```bash
cargo build --locked 2>&1
```
Expected: Build succeeds with no errors.

- [ ] **Step 5: Commit**

```bash
git add src/e2ee/cross_signing/ src/e2ee/ssss/
git commit -m "$(cat <<'EOF'
chore: remove 6 zombie sub-files from src/e2ee/cross_signing/ and ssss/

mod.rs files re-export from synapse_e2ee without declaring sub-modules.
models.rs, service.rs, storage.rs in each dir were never compiled.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: Delete zombie standalone files in `src/e2ee/`

**Files:**
- Delete: `src/e2ee/signed_json.rs`
- Delete: `src/e2ee/vodozemac_megolm.rs`

**Why:** `src/e2ee/mod.rs` imports these via `pub use synapse_e2ee::signed_json;` and `pub use synapse_e2ee::vodozemac_megolm;` — NOT via `pub mod`. The local files are never compiled.

- [ ] **Step 1: Confirm files are not declared as mods**

Run:
```bash
grep 'mod signed_json\|mod vodozemac_megolm' src/e2ee/mod.rs
```
Expected: No output (only `pub use synapse_e2ee::signed_json;` and `pub use synapse_e2ee::vodozemac_megolm;` exist).

- [ ] **Step 2: Delete the zombie files**

```bash
rm src/e2ee/signed_json.rs
rm src/e2ee/vodozemac_megolm.rs
```

- [ ] **Step 3: Run full test suite to verify no breakage**

Run:
```bash
cargo test --all-features --locked -- --test-threads=4 2>&1 | tail -20
```
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/e2ee/
git commit -m "$(cat <<'EOF'
chore: remove 2 zombie standalone files from src/e2ee/

signed_json.rs and vodozemac_megolm.rs — imported via
pub use synapse_e2ee::*, not pub mod. Never compiled.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: Convert `src/common/collections.rs` to facade

**Files:**
- Modify: `src/common/collections.rs`

**Why:** This file is byte-for-byte identical to `synapse-common/src/collections.rs`. Replace with a thin re-export to eliminate duplication.

- [ ] **Step 1: Verify byte-level identity**

Run:
```bash
diff src/common/collections.rs synapse-common/src/collections.rs && echo "IDENTICAL" || echo "DIFFER"
```
Expected: `IDENTICAL`

- [ ] **Step 2: Verify synapse-common exports the needed items**

Run:
```bash
grep 'pub mod collections' synapse-common/src/lib.rs
grep 'pub use.*collections' synapse-common/src/lib.rs
```
Check that `synapse-common` makes `collections` publicly accessible.

- [ ] **Step 3: Replace file content with thin facade**

Write to `src/common/collections.rs`:
```rust
pub use synapse_common::collections::*;
```

- [ ] **Step 4: Verify build succeeds**

Run:
```bash
cargo build --locked 2>&1
```
Expected: Build succeeds with no errors.

- [ ] **Step 5: Run clippy**

Run:
```bash
cargo clippy --all-features --locked -- -D warnings 2>&1
```
Expected: No warnings or errors.

- [ ] **Step 6: Commit**

```bash
git add src/common/collections.rs
git commit -m "$(cat <<'EOF'
refactor: convert src/common/collections.rs to thin facade

Byte-identical with synapse-common/src/collections.rs.
Replaced 164 lines with single re-export.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: Convert remaining byte-identical files (batch 1)

**Files:**
- Modify: `src/common/concurrency.rs` (197 lines → 1 line)
- Modify: `src/common/constants.rs` (173 lines → 1 line)
- Modify: `src/common/media_locator.rs` (80 lines → 1 line)
- Modify: `src/common/regex_cache.rs` (125 lines → 1 line)

**Why:** All four files are byte-for-byte identical with their `synapse-common/src/` counterparts.

- [ ] **Step 1: Verify byte-level identity for all four**

Run:
```bash
for f in concurrency constants media_locator regex_cache; do
  result=$(diff src/common/$f.rs synapse-common/src/$f.rs && echo "IDENTICAL" || echo "DIFFER")
  echo "$f: $result"
done
```
Expected: All four show `IDENTICAL`.

- [ ] **Step 2: Replace each file with thin facade**

Write to `src/common/concurrency.rs`:
```rust
pub use synapse_common::concurrency::*;
```

Write to `src/common/constants.rs`:
```rust
pub use synapse_common::constants::*;
```

Write to `src/common/media_locator.rs`:
```rust
pub use synapse_common::media_locator::*;
```

Write to `src/common/regex_cache.rs`:
```rust
pub use synapse_common::regex_cache::*;
```

- [ ] **Step 3: Verify build and tests**

Run:
```bash
cargo build --locked 2>&1 && cargo test --all-features --locked -- --test-threads=4 2>&1 | tail -20
```
Expected: Build succeeds, all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/common/concurrency.rs src/common/constants.rs src/common/media_locator.rs src/common/regex_cache.rs
git commit -m "$(cat <<'EOF'
refactor: convert 4 more byte-identical files to thin facades

concurrency.rs, constants.rs, media_locator.rs, regex_cache.rs —
all byte-identical with synapse-common/src/ counterparts.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 8: Convert remaining byte-identical files (batch 2)

**Files:**
- Modify: `src/common/traits.rs` (104 lines → 1 line)
- Modify: `src/common/federation_test_keys.rs` (222 lines → 1 line)
- Modify: `src/common/feature_flags.rs` (180 lines → 1 line)

**Why:** These three files are byte-for-byte identical with their `synapse-common/src/` counterparts. `feature_flags.rs` and `federation_test_keys.rs` are larger files, so verification is especially important.

- [ ] **Step 1: Verify byte-level identity**

Run:
```bash
for f in traits federation_test_keys feature_flags; do
  result=$(diff src/common/$f.rs synapse-common/src/$f.rs && echo "IDENTICAL" || echo "DIFFER")
  echo "$f: $result"
done
```
Expected: All three show `IDENTICAL`.

- [ ] **Step 2: Replace each file with thin facade**

Write to `src/common/traits.rs`:
```rust
pub use synapse_common::traits::*;
```

Write to `src/common/federation_test_keys.rs`:
```rust
// Federation test keys are only used in test contexts.
pub use synapse_common::federation_test_keys::*;
```

Write to `src/common/feature_flags.rs`:
```rust
pub use synapse_common::feature_flags::*;
```

- [ ] **Step 3: Verify `#[cfg(any(test, feature = "test-utils"))]` gate on federation_test_keys**

Check `src/common/mod.rs` line 61-63 ensures `federation_test_keys` is only used with the feature gate. The facade must work with this gating.

Run:
```bash
cargo build --locked 2>&1
```
Expected: Build succeeds. The `#[cfg]` gate in mod.rs handles the conditional compilation.

- [ ] **Step 4: Verify full build with all features**

Run:
```bash
cargo build --all-features --locked 2>&1
```
Expected: Build succeeds.

- [ ] **Step 5: Commit**

```bash
git add src/common/traits.rs src/common/federation_test_keys.rs src/common/feature_flags.rs
git commit -m "$(cat <<'EOF'
refactor: convert final 3 byte-identical files to thin facades

traits.rs, federation_test_keys.rs, feature_flags.rs — all
byte-identical with synapse-common/src/ counterparts.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 9: Migrate `src/federation/edu.rs` to `synapse-federation`

**Files:**
- Create: `synapse-federation/src/edu.rs`
- Modify: `synapse-federation/src/lib.rs` (add `pub mod edu;`)
- Modify: `synapse-federation/Cargo.toml` (add any needed dependencies)
- Modify: `src/federation/edu.rs` (replace with thin facade)
- Modify: `src/federation/mod.rs` (update import path)

**Why:** `edu.rs` (571 lines) is the only remaining business logic in `src/federation/` that has not been migrated to `synapse-federation`. It contains `EduDispatcher`, `EduProcessResult`, and `EduType` types used for federation EDU (Ephemeral Data Unit) processing.

- [ ] **Step 1: Check dependencies needed by edu.rs**

Run:
```bash
grep -E '^use ' src/federation/edu.rs | grep -v 'crate::' | sort -u
```
Identify external crate dependencies used by edu.rs that may not be in `synapse-federation/Cargo.toml`.

- [ ] **Step 2: Read synapse-federation/Cargo.toml and compare**

Run:
```bash
cat synapse-federation/Cargo.toml
```
Note which required dependencies are already present and which need to be added.

- [ ] **Step 3: Read edu.rs to identify internal dependencies**

Run:
```bash
grep -E 'use (crate|super|synapse_)' src/federation/edu.rs | sort -u
```
Identify all internal crate dependencies (`synapse_common::`, `synapse_cache::`, `synapse_storage::`, etc.).

- [ ] **Step 4: Move edu.rs to synapse-federation**

```bash
cp src/federation/edu.rs synapse-federation/src/edu.rs
```

- [ ] **Step 5: Update internal imports in migrated edu.rs**

In `synapse-federation/src/edu.rs`, update import paths:
- `crate::federation::` → `crate::`
- `crate::common::` → `synapse_common::`
- `crate::cache::` → `synapse_cache::`
- `crate::storage::` → `synapse_storage::`
- Any `super::` references need adjustment

The exact replacements depend on Step 3 findings. Common patterns:
```rust
// Before (in src/federation/edu.rs):
use crate::common::error::ApiError;
use crate::storage::Database;

// After (in synapse-federation/src/edu.rs):
use synapse_common::error::ApiError;
use synapse_storage::Database;
```

- [ ] **Step 6: Add `pub mod edu;` to synapse-federation/src/lib.rs**

Add this line to `synapse-federation/src/lib.rs`:
```rust
pub mod edu;
```

- [ ] **Step 7: Add any missing dependencies to synapse-federation/Cargo.toml**

If edu.rs uses crates not yet in `synapse-federation/Cargo.toml`, add them.

- [ ] **Step 8: Build synapse-federation in isolation**

Run:
```bash
cargo build -p synapse-federation --locked 2>&1
```
Expected: Build succeeds.

- [ ] **Step 9: Replace src/federation/edu.rs with thin facade**

Write to `src/federation/edu.rs`:
```rust
pub use synapse_federation::edu::*;
```

- [ ] **Step 10: Verify src/federation/mod.rs needs no changes**

No changes needed to `src/federation/mod.rs`:
- Line 3 `pub mod edu;` compiles the facade file (`src/federation/edu.rs`)
- Line 15 `pub use edu::{EduDispatcher, EduProcessResult, EduType};` re-exports from the facade
- The facade does `pub use synapse_federation::edu::*;` — so all types flow through correctly

- [ ] **Step 11: Full build and test verification**

Run:
```bash
cargo build --all-features --locked 2>&1
cargo test --all-features --locked -- --test-threads=4 2>&1 | tail -20
```
Expected: Build succeeds, all tests pass.

- [ ] **Step 12: Commit**

```bash
git add synapse-federation/src/edu.rs synapse-federation/src/lib.rs synapse-federation/Cargo.toml src/federation/edu.rs
git commit -m "$(cat <<'EOF'
refactor: migrate edu.rs to synapse-federation crate

Move EduDispatcher, EduProcessResult, and EduType to synapse-federation,
replacing src/federation/edu.rs with a thin facade. This completes the
federation module migration — all federation business logic now lives in
synapse-federation.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 10: Final verification

**Purpose:** Ensure the full workspace builds, passes clippy and tests after all cleanup.

- [ ] **Step 1: Clean build from scratch**

Run:
```bash
cargo clean && cargo build --all-features --locked 2>&1 | tail -10
```
Expected: Build succeeds.

- [ ] **Step 2: Run clippy**

Run:
```bash
cargo clippy --all-features --locked -- -D warnings 2>&1 | tail -10
```
Expected: No warnings or errors.

- [ ] **Step 3: Run format check**

Run:
```bash
cargo fmt --all -- --check
```
Expected: No formatting issues.

- [ ] **Step 4: Run full test suite**

Run:
```bash
cargo test --all-features --locked -- --test-threads=4 2>&1 | tail -20
```
Expected: All tests pass.

- [ ] **Step 5: Verify no remaining zombie files**

Run:
```bash
# Check src/cache/
echo "src/cache/ files:" && ls src/cache/
# Check src/e2ee/ directories
echo "src/e2ee/ dirs:" && ls -d src/e2ee/*/
# Check src/e2ee/ files
echo "src/e2ee/ files:" && ls src/e2ee/*.rs
# Count remaining real-code files in src/common/ (not 1-3 line facades)
find src/common/ -name '*.rs' -exec sh -c 'test $(wc -l < "$1") -gt 4 && echo "$1"' _ {} \;
```
Expected: No unexpected large files remain in facade directories.

- [ ] **Step 6: Commit any remaining cleanup**

```bash
git status
```

If clean, no commit needed. If there are additional cleanup artifacts, review and commit them.
