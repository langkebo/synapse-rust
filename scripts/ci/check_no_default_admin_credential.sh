#!/usr/bin/env bash
#
# CI gate: no default administrator credential in shipped scripts/configs (SEC-02).
#
# Several helper scripts used to fall back to a built-in default password when
# `ADMIN_PASS` was unset. The fallback made every one of them "work" out of the
# box against a server whose admin password was never changed — i.e. it shipped
# a working credential, and it silently turned an unset variable into a
# successful privileged login instead of a configuration error.
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
# Red-proved 2026-10-22: reinstating that fallback in a helper script → exit 1;
# removing it → exit 0.
#
# This file is inside its own scan surface, so **no comment here may spell the
# literal out** — see the PATTERN note below for the spelling to use instead.

set -eu

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

# Written as `Admin@12[3]` so this file does not match its own pattern.
#
# The bracket is load-bearing: as an ERE, `Admin@12[3]` matches the three-digit
# password, while the *text* `Admin@12[3]` does not match it. Anywhere this file
# needs to name the old default — including prose in comments — it must use this
# spelling, otherwise the gate fails on itself (that is what happened between
# W5's commit and this fix: two comments spelled it out and `ci.yml` was red
# from the day it landed).
#
# Comments in *other* scanned files are **not** exempt on purpose: a commented
# default in a `.env.example` is one uncomment away from shipping.
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
