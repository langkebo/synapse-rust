#!/usr/bin/env bash
#
# CI gate: no silent cache invalidation.
#
# `CacheManager::delete` returns `()`. Its outcome is handled *inside* the
# manager (a failed L2 DEL tombstones the key, so the next read goes back to
# the database), so the only thing `let _ = ...cache.delete(...)` documents is
# that the author believed a result was being discarded. There is no result to
# discard — and while the calls looked like "best effort", a failed L2 delete
# used to resurrect the stale value on the next read (CODE-02).
#
# The rule that replaced them: every deleted cache key has exactly one delete
# site, exposed as a small helper next to the key's definition
# (`room_state_cache::invalidate_room_state`,
# `account_data_service::invalidate_account_data`, …). This gate keeps it that
# way by rejecting the `let _ =` form outright.
#
# Usage:
#   bash scripts/ci/check_no_silent_cache_delete.sh
#
# Exits 0 when there is no match, 1 on any violation. The scan surface is
# every git-tracked `.rs` file (tests included), so `target/`, build output and
# untracked scratch files are out of scope by construction.

set -eu

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

# `let _ = <something>.cache.delete(...)` / `...cache.delete_batch(...)`.
# Case-insensitive on `cache` so `CacheManager` locals are caught too, but the
# line must contain `cache` and a `.delete` call — this is not a generic
# "let _ = anything" ban.
PATTERN='let[[:space:]]+_[[:space:]]*=[^;]*[Cc]ache[^;]*\.delete(_batch)?\('

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "::error::check_no_silent_cache_delete: not inside a git work tree" >&2
    exit 1
fi

files=$(git ls-files '*.rs')
if [[ -z "${files}" ]]; then
    # An empty scan surface would pass silently, which is the failure mode this
    # check exists to prevent. Fail loudly instead.
    echo "::error::check_no_silent_cache_delete: no git-tracked .rs files found" >&2
    exit 1
fi

# `grep` returns 1 when nothing matches; do not let `set -e` turn that into a
# failure of the script.
matches=$(printf '%s\n' "${files}" | xargs grep -nE "${PATTERN}" 2>/dev/null || true)
# Ignore matches that are inside a line comment.
matches=$(printf '%s\n' "${matches}" | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' || true)

if [[ -z "${matches}" ]]; then
    echo "check_no_silent_cache_delete: OK (no \`let _ = ...cache.delete(...)\`)"
    exit 0
fi

echo "check_no_silent_cache_delete: FAIL" >&2
echo "\`CacheManager::delete\` returns \`()\`; wrapping it in \`let _ =\` hides nothing" >&2
echo "and invites a second, divergent delete site for the same key." >&2
echo "Call the key's invalidation helper instead, e.g." >&2
echo "  crate::room_state_cache::invalidate_room_state(&self.cache, room_id).await;" >&2
echo >&2
echo "Violations:" >&2
printf '%s\n' "${matches}" >&2
exit 1
