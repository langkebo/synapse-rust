#!/usr/bin/env bash
#
# CI gate: no default administrator credential in shipped scripts/configs (SEC-02).
#
# Several helper scripts used to carry `ADMIN_PASS="${ADMIN_PASS:-Admin@123}"`.
# The fallback made every one of them "work" out of the box against a server
# whose admin password was never changed — i.e. it shipped a working
# credential, and it silently turned an unset variable into a successful
# privileged login instead of a configuration error.
#
# The rule that replaced them: the credential is *required*, never defaulted
# (`${ADMIN_PASS:?…}`, or an explicit empty-check that exits non-zero). This
# gate keeps it that way.
#
# Usage:
#   bash scripts/ci/check_no_default_admin_credential.sh
#
# Scan surface: every git-tracked file under `scripts/` and `docker/`, minus
# `*.md` (historical reports quote credentials as evidence, they do not log in).
# Exits 0 when there is no match, 1 on any violation.
#
# Red-proved 2026-10-22: restoring `ADMIN_PASS="${ADMIN_PASS:-Admin@123}"` in a
# helper script → exit 1; removing it → exit 0.

set -eu

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

# Written as `Admin@12[3]` so this file does not match its own pattern.
PATTERN='Admin@12[3]'

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "::error::check_no_default_admin_credential: not inside a git work tree" >&2
    exit 1
fi

files=$(git ls-files 'scripts/**' 'docker/**' | grep -vE '\.md$' || true)
if [[ -z "${files}" ]]; then
    # An empty scan surface would pass silently, which is the failure mode this
    # check exists to prevent. Fail loudly instead.
    echo "::error::check_no_default_admin_credential: no git-tracked files under scripts/ or docker/" >&2
    exit 1
fi

# `grep` returns 1 when nothing matches; do not let `set -e` turn that into a
# failure of the script.
matches=$(printf '%s\n' "${files}" | xargs grep -nE "${PATTERN}" 2>/dev/null || true)

if [[ -z "${matches}" ]]; then
    echo "check_no_default_admin_credential: OK (no default admin credential under scripts/ or docker/)"
    exit 0
fi

echo "check_no_default_admin_credential: FAIL" >&2
echo "A default administrator password is back in a shipped script or config." >&2
echo "Require the credential instead of defaulting it:" >&2
echo "  : \"\${ADMIN_PASS:?ADMIN_PASS must be set (no default credential is shipped)}\"" >&2
echo >&2
echo "Violations:" >&2
printf '%s\n' "${matches}" >&2
exit 1
