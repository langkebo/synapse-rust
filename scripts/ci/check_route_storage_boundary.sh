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
#
# `SYNAPSE_WEB_ROUTES_DIR` overrides the scan surface (mirrors
# `SYNAPSE_WEB_CRATE_DIR` in scripts/quality/check_route_layering.sh) so the
# gate can be self-tested against an empty tree.

set -eu

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ROUTES_DIR="${SYNAPSE_WEB_ROUTES_DIR:-${ROOT_DIR}/synapse-web/src/routes}"
ALLOWLIST="${ROOT_DIR}/scripts/ci/route_storage_exceptions.txt"

# A gate whose scan surface can silently disappear is not a gate.
#
# This branch used to `exit 0`, which meant renaming or moving the route
# directory (exactly what B4-5b did when `src/web` became `synapse-web`) would
# report OK while inspecting nothing — the "move code out of the gate's scan
# surface" escape from AGENTS.md 铁律 8. Fail loudly and say why, so the fix is
# to re-point the gate rather than to delete it.
if [[ ! -d "${ROUTES_DIR}" ]]; then
    echo "::error::check_route_storage_boundary: scan surface missing: ${ROUTES_DIR}" >&2
    echo "  The route layer moved or was renamed. Re-point ROUTES_DIR (or set" >&2
    echo "  SYNAPSE_WEB_ROUTES_DIR) at its new location — do NOT delete this check," >&2
    echo "  which exists because 137 route files once imported crate::storage directly." >&2
    exit 1
fi

# Guard the scan surface itself: a directory that exists but contains no Rust
# files would also make the gate vacuous. `synapse-web/src/routes` is a large,
# always-populated tree, so an empty result means the path is wrong.
if ! find "${ROUTES_DIR}" -name '*.rs' -print -quit | grep -q .; then
    echo "::error::check_route_storage_boundary: no .rs files under ${ROUTES_DIR}" >&2
    echo "  The gate would inspect nothing and pass. Check the path." >&2
    exit 1
fi

# Build a newline-separated list of allowlisted repo-relative paths.
allowlist_text=""
if [[ -f "${ALLOWLIST}" ]]; then
    allowlist_text=$(awk 'NF && $1 !~ /^#/' "${ALLOWLIST}" || true)
fi

# Find every line in synapse-web/src/routes that pulls a type out of
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
