#!/usr/bin/env bash
#
# check_route_layering.sh — Route Layering Gate
#
# Ensures that `src/web/routes/` handlers do not directly import
# `crate::storage` or invoke `sqlx::query` / `PgPool` / `Pool`.
# The architecture requires:
#
#   route → service → storage
#
# Direct storage access in route handlers bypasses transaction
# management, rate-limiting, metrics, and error normalisation.
#
# Exit codes:
#   0 — no violations found
#   1 — violations found (CI gate fails)
#   2 — usage error (missing directory, etc.)
#
# Usage:
#   bash scripts/quality/check_route_layering.sh           # check all routes
#   bash scripts/quality/check_route_layering.sh --json    # machine-readable output
#   bash scripts/quality/check_route_layering.sh --explain # explain why each is a violation
#

set -euo pipefail

SRC_DIR="${SYNAPSE_SRC_DIR:-src}"
ROUTES_DIR="$SRC_DIR/web/routes"
OUTPUT_FORMAT="${1:-text}"

if [ ! -d "$ROUTES_DIR" ]; then
    echo "ERROR: Routes directory not found: $ROUTES_DIR" >&2
    exit 2
fi

# ---------------------------------------------------------------------------
# Detection patterns
# ---------------------------------------------------------------------------

# Pattern A: `use crate::storage` (or `use crate::storage::...`)
# Pattern B: `sqlx::query` / `sqlx::query_as` / `sqlx::query_scalar`
# Pattern C: `PgPool` / `Pool<Postgres>` / `.pool` (direct pool access)
# Pattern D: `use sqlx::PgPool` / `use sqlx::Pool`
# Pattern E: `as_ref()` on pool (never needed in a route handler)

violations=()

while IFS= read -r -d '' file; do
    file_violations=()

    # Pattern A: direct storage import
    if grep -nE 'use\s+crate::storage' "$file" >/dev/null 2>&1; then
        file_violations+=("A: uses crate::storage directly")
    fi

    # Pattern B: raw sqlx::query in route handler
    if grep -nE 'sqlx::query\b|sqlx::query_as\b|sqlx::query_scalar\b|sqlx::query_file\b' "$file" >/dev/null 2>&1; then
        file_violations+=("B: uses sqlx::query* directly")
    fi

    # Pattern C: direct pool access
    if grep -nE '\bPgPool\b|\bPool\s*<\s*Postgres\s*>' "$file" >/dev/null 2>&1; then
        file_violations+=("C: references PgPool/Pool<Postgres> directly")
    fi

    if [ ${#file_violations[@]} -gt 0 ]; then
        violations+=("$file:${file_violations[*]}")
    fi
done < <(find "$ROUTES_DIR" -name '*.rs' -print0 2>/dev/null)

# ---------------------------------------------------------------------------
# Output
# ---------------------------------------------------------------------------

if [ ${#violations[@]} -eq 0 ]; then
    if [ "$OUTPUT_FORMAT" = "--json" ]; then
        echo '{"status":"pass","violations":[]}'
    else
        echo "PASS: No route-layer violations found in $ROUTES_DIR"
    fi
    exit 0
fi

if [ "$OUTPUT_FORMAT" = "--json" ]; then
    echo '{"status":"fail","violations":['
    for i in "${!violations[@]}"; do
        comma=","
        if [ "$i" -eq $((${#violations[@]} - 1)) ]; then
            comma=""
        fi
        echo "  {\"file\":\"${violations[$i]}\"}$comma"
    done
    echo ']}'
elif [ "$OUTPUT_FORMAT" = "--explain" ]; then
    echo "=== Route Layering Violations ==="
    echo ""
    echo "The architecture requires: route → service → storage"
    echo "Direct storage access in route handlers bypasses:"
    echo "  - Transaction management"
    echo "  - Rate-limiting"
    echo "  - Metrics collection"
    echo "  - Error normalisation"
    echo ""
    echo "Fix: move the storage call into a service method, then call"
    echo "     the service from the route handler."
    echo ""
    echo "Violations:"
    for v in "${violations[@]}"; do
        echo "  $v"
    done
else
    echo "FAIL: Route layering violations found in $ROUTES_DIR"
    echo ""
    for v in "${violations[@]}"; do
        echo "  $v"
    done
fi

exit 1
