#!/usr/bin/env bash
#
# CI gate: enforce the `route → service → storage` boundary.
#
# Routes are not allowed to import types directly from `crate::storage`.
# They must depend on the `service` layer so that transactions, rate
# limits, metrics, audit logging and error normalisation cannot be
# accidentally bypassed. This is the single biggest architectural
# regression vector in the project (137 occurrences before this gate
# was introduced) and one of the items called out in the 2026-06-03
# comprehensive audit.
#
# Usage:
#   bash scripts/ci/check_route_storage_boundary.sh
#
# Exits 0 on success, 1 on any violation. The allowlist lives next
# to this script (`route_storage_exceptions.txt`) and lists file
# paths (one per line) that still contain legacy storage imports.
# Every entry is a technical-debt marker and should be removed as
# the call sites are migrated to a service.

set -eu

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ROUTES_DIR="${ROOT_DIR}/src/web/routes"
ALLOWLIST="${ROOT_DIR}/scripts/ci/route_storage_exceptions.txt"

if [[ ! -d "${ROUTES_DIR}" ]]; then
    echo "check_route_storage_boundary: routes directory not found at ${ROUTES_DIR}" >&2
    exit 0
fi

# Build a newline-separated list of allowlisted repo-relative paths.
allowlist_text=""
if [[ -f "${ALLOWLIST}" ]]; then
    allowlist_text=$(awk 'NF && $1 !~ /^#/' "${ALLOWLIST}" || true)
fi

# Find every line in src/web/routes that pulls a type out of
# `crate::storage::*`. grep returns 1 when nothing matches, so we
# intentionally do not propagate that exit status through `set -e`.
matches=$(grep -RIn --include='*.rs' -E 'use[[:space:]]+crate::storage' "${ROUTES_DIR}" 2>/dev/null || true)

if [[ -z "${matches}" ]]; then
    echo "check_route_storage_boundary: OK (no route imports from crate::storage)"
    exit 0
fi

# The allowlist is keyed on file paths, not on individual import
# lines, because the same file typically violates the rule from
# several call sites. A file should be removed from the allowlist
# only when every offending import in it has been migrated to a
# service.
violations=""
while IFS= read -r line; do
    [[ -z "${line}" ]] && continue
    rel="${line#${ROOT_DIR}/}"
    file="${rel%%:*}" # strip the `:line:content` suffix
    if printf '%s\n' "${allowlist_text}" | grep -F -x -q -- "${file}"; then
        echo "check_route_storage_boundary: allowlisted: ${file}"
        continue
    fi
    violations+="${line}"$'\n'
done <<<"${matches}"

if [[ -n "${violations}" ]]; then
    echo "check_route_storage_boundary: FAIL" >&2
    echo "Routes must not import types from \`crate::storage\` directly." >&2
    echo "Wrap storage access in a service and depend on the service instead." >&2
    echo "If migration is not yet feasible, add the file path to" >&2
    echo "  ${ALLOWLIST##*/}" >&2
    echo "and remove the entry once the migration is complete." >&2
    echo >&2
    echo "Violations:" >&2
    printf '%s' "${violations}" >&2
    exit 1
fi

echo "check_route_storage_boundary: OK (only allowlisted imports remain)"
exit 0
