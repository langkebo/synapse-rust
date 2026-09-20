#!/usr/bin/env bash
#
# DB-backed pagination performance gate (E4).
#
# ## Why this exists
#
# `scripts/check_pagination_benchmark.py` compares two **in-memory simulations**
# (`benches/performance_api_benchmarks.rs`). Their margin is ~1500x, so no real
# `synapse-storage` SQL regression can trip its 30% threshold — it is a
# compute-path smoke check, not a pagination guard
# (`docs/audit/GATE_INTEGRITY_FOLLOWUP_2026-09-19.md` §9, row E4).
#
# This gate measures the real thing:
#
#   * `benches/performance_pagination_benchmarks.rs` seeds a real migrated
#     `events` table, runs the production keyset query
#     (`EventStorage::get_room_events_paginated_cursor`, the SQL `/messages`
#     uses) at a deep cursor, and runs the naive `LIMIT/OFFSET` shape that
#     ISSUE-06 replaced, for the same page;
#   * the bench emits `[perf] pagination ...` on stderr;
#   * this script enforces:
#       1. the fixture really was seeded (`rows >= PAGINATION_MIN_ROWS`);
#       2. the keyset page and the offset page are the **same rows**
#          (`correct=1`) — a "fast" query that returns the wrong page is not a
#          win;
#       3. the keyset deep page plan still uses an ordered index (`index_scan=1`
#          now means `Index Scan using idx_events_room_ts_stream` with no `Sort`
#          node, not merely "no Seq Scan");
#       4. the real deep-page gain is at least `PAGINATION_MIN_GAIN`
#          (default 2.0x). Measured healthy: ~5-9x. Degraded (index dropped, or
#          the keyset predicate replaced by OFFSET): ~1-1.5x → fails.
#       5. the shallowest production page (`keyset_shallow_us`, the first
#          `/messages` call) is no more than `PAGINATION_MAX_SHALLOW_RATIO`
#          (default 4.0x) slower than the deep page, unless it is below the
#          absolute `PAGINATION_SHALLOW_BREACH_FLOOR_US` (default 5000 us, i.e.
#          measurement noise).
#
# ## Gated: `keyset_shallow_us` (the ORDER BY alias-shadowing regression)
#
# `keyset_shallow_us` is the first (`from = None`) `/messages` page, measured
# through the production query. It used to be *higher* than the deep page
# (measured 22672.2 us vs 2932.5 us on a 150k-row fixture) because
# `ROOM_EVENT_COLS` aliases `COALESCE(origin_server_ts, 0) AS
# origin_server_ts` and the keyset SQL wrote `ORDER BY origin_server_ts`
# unqualified, so the sort key bound to that output alias (EXPLAIN:
# `Sort Key: (COALESCE(origin_server_ts, '0'::bigint))`) and
# `idx_events_room_ts_stream` could not supply the order. The planner then
# sorted every row above the cursor, so shallower pages cost more and the
# no-cursor first page was the worst case.
#
# `synapse-storage/src/event/pagination.rs` now qualifies the sort keys
# (`ORDER BY events.origin_server_ts, events.stream_ordering`), so every depth
# is a plain index scan (measured 0.044-0.052 ms, shallow/deep ~1x). Reverting
# the qualification measured ~10-15x (18.2 ms vs 1.2 ms) → this check reddens.
# The bench samples the two shapes interleaved and the check carries an absolute
# floor (see `PAGINATION_SHALLOW_BREACH_FLOOR_US`) so transient load cannot
# fake the ratio.
#
# ## Environment
#   BENCHMARK_DATABASE_URL        Postgres URL (required; the bench reads it and
#                                 skips when unreachable — which fails the gate)
#   PAGINATION_MIN_GAIN           minimum real speedup, default 2.0
#   PAGINATION_MIN_ROWS           minimum seeded fixture rows, default 150000
#   PAGINATION_MAX_SHALLOW_RATIO  max shallow/deep keyset ratio, default 4.0
#   PAGINATION_SHALLOW_BREACH_FLOOR_US
#                                 a shallow page faster than this many us is
#                                 never a shallow-page breach, default 5000.
#                                 The ratio alone is noisy: a transient load
#                                 spike that lands on the shallow samples once
#                                 produced 3.2 ms vs 0.32 ms (9.9x) on a healthy
#                                 tree. The buggy shallow page measured
#                                 10.9-22.7 ms, healthy 0.18-0.57 ms, so the
#                                 floor sits between the two.
#
# Exit codes: 0 = pass; 1 = real breach (regression / wrong page / lost index /
# too-slow gain / shallow page slower than the deep page); 2 = the gate cannot
# prove its premise (missing measurement, unparseable numbers) — never exit 0 in
# that case.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

MIN_GAIN="${PAGINATION_MIN_GAIN:-2.0}"
MIN_ROWS="${PAGINATION_MIN_ROWS:-150000}"
MAX_SHALLOW_RATIO="${PAGINATION_MAX_SHALLOW_RATIO:-4.0}"
SHALLOW_FLOOR_US="${PAGINATION_SHALLOW_BREACH_FLOOR_US:-5000}"
BENCH_LOG="artifacts/pagination_perf_gate.log"

mkdir -p artifacts

echo "==> DB-backed pagination perf gate (E4)"
echo "    min gain: ${MIN_GAIN}x   min fixture rows: ${MIN_ROWS}   max shallow/deep ratio: ${MAX_SHALLOW_RATIO}x (floor ${SHALLOW_FLOOR_US}us)"

# Fail closed: a premise the gate cannot establish is not a pass.
premise_fail() {
    echo "ERROR: pagination_perf_gate: $1" >&2
    exit 2
}

# `PAGINATION_GATE_ONLY=1` makes the bench emit only the `[perf]` line and skip
# criterion; `BENCH_REQUIRE=pagination_db` makes a silently skipped DB
# measurement a non-zero exit instead of an empty log.
if ! PAGINATION_GATE_ONLY=1 BENCH_REQUIRE="pagination_db" \
    cargo bench --locked --bench performance_pagination_benchmarks >"${BENCH_LOG}" 2>&1; then
    echo "ERROR: DB-backed pagination benchmark failed to run" >&2
    cat "${BENCH_LOG}" >&2
    exit 1
fi

# `tail -1`: a re-run inside one cargo invocation cannot happen today, but if it
# ever did, comparing against the first sample would silently pick winners.
PERF_LINE="$(grep -E '^\[perf\] pagination ' "${BENCH_LOG}" | tail -n 1 || true)"
if [ -z "${PERF_LINE}" ]; then
    echo "ERROR: no '[perf] pagination' line in ${BENCH_LOG}" >&2
    cat "${BENCH_LOG}" >&2
    exit 1
fi
echo "    ${PERF_LINE}"

# Extract a `key=value` field from the single perf line. Values are numeric or
# 0/1, so the first whitespace-delimited occurrence is unambiguous.
field() {
    printf '%s\n' "${PERF_LINE}" | sed -n "s/.*[[:space:]]$1=\([^[:space:]]*\).*/\1/p"
}

ROWS="$(field rows)"
KEYSET_DEEP="$(field keyset_deep_us)"
KEYSET_SHALLOW="$(field keyset_shallow_us)"
OFFSET_DEEP="$(field offset_deep_us)"
INDEX_SCAN="$(field index_scan)"
CORRECT="$(field correct)"

[ -n "${ROWS}" ] || premise_fail "measurement line has no rows= field: ${PERF_LINE}"
[ -n "${KEYSET_DEEP}" ] || premise_fail "measurement line has no keyset_deep_us= field: ${PERF_LINE}"
[ -n "${KEYSET_SHALLOW}" ] || premise_fail "measurement line has no keyset_shallow_us= field: ${PERF_LINE}"
[ -n "${OFFSET_DEEP}" ] || premise_fail "measurement line has no offset_deep_us= field: ${PERF_LINE}"

# Reject anything that is not a plain positive number before comparing.
is_positive_number() {
    awk -v v="$1" 'BEGIN { exit !(v ~ /^[0-9]+(\.[0-9]+)?$/ && v > 0) }'
}
is_positive_number "${KEYSET_DEEP}" || premise_fail "keyset_deep_us is not a positive number: '${KEYSET_DEEP}'"
is_positive_number "${KEYSET_SHALLOW}" || premise_fail "keyset_shallow_us is not a positive number: '${KEYSET_SHALLOW}'"
is_positive_number "${OFFSET_DEEP}" || premise_fail "offset_deep_us is not a positive number: '${OFFSET_DEEP}'"
[[ "${ROWS}" =~ ^[0-9]+$ ]] || premise_fail "rows is not an integer: '${ROWS}'"

FAILURES=0

# 1) The fixture has to be real. A tiny/partial seed would make the ratio noise.
if [ "${ROWS}" -lt "${MIN_ROWS}" ]; then
    echo "BREACH: fixture has only ${ROWS} rows (< ${MIN_ROWS}); the deep-page measurement is too small to trust"
    FAILURES=$((FAILURES + 1))
else
    echo "OK: fixture rows=${ROWS}"
fi

# 2) Same page or it is not a comparison.
if [ "${CORRECT}" != "1" ]; then
    echo "BREACH: keyset page != offset page (correct=${CORRECT:-missing}); the timing comparison is meaningless"
    FAILURES=$((FAILURES + 1))
else
    echo "OK: keyset and offset return the same deep page"
fi

# 3) A lost ordered index is the canonical keyset regression: `index_scan=1`
#    means the plan is an ordered `Index Scan` on `idx_events_room_ts_stream`
#    with no `Sort` node, i.e. the index supplies the sort. A `Bitmap Heap Scan
#    + Sort` (the alias-shadowing plan) or a `Seq Scan` both breach.
if [ "${INDEX_SCAN}" != "1" ]; then
    echo "BREACH: keyset deep-page plan is not an ordered Index Scan (index_scan=${INDEX_SCAN:-missing})"
    FAILURES=$((FAILURES + 1))
else
    echo "OK: keyset deep-page plan uses the ordered index (no Sort node)"
fi

# 4) The real gain. This is the threshold that replaces the 30% / ~1500x
#    in-memory comparison.
GAIN_OK=0
if awk -v k="${KEYSET_DEEP}" -v o="${OFFSET_DEEP}" -v g="${MIN_GAIN}" 'BEGIN { exit !(k * g <= o) }'; then
    GAIN_OK=1
fi
GAIN="$(awk -v k="${KEYSET_DEEP}" -v o="${OFFSET_DEEP}" 'BEGIN { printf "%.2f", o / k }')"
if [ "${GAIN_OK}" = "1" ]; then
    echo "OK: keyset deep page is ${GAIN}x faster than offset (>= ${MIN_GAIN}x required)"
else
    echo "BREACH: keyset deep page is only ${GAIN}x faster than offset (< ${MIN_GAIN}x required)"
    FAILURES=$((FAILURES + 1))
fi

# 5) The first `/messages` page must not be the worst case. This is the check
#    that catches the ORDER BY alias-shadowing regression if the qualified sort
#    key is reverted: the shallow page then sorts every row above its cursor.
#    Breach only when the page is BOTH off-ratio and above the absolute floor;
#    a sub-floor page is measurement noise, not a plan regression.
SHALLOW_RATIO="$(awk -v s="${KEYSET_SHALLOW}" -v d="${KEYSET_DEEP}" 'BEGIN { printf "%.2f", s / d }')"
SHALLOW_LIMIT="$(awk -v d="${KEYSET_DEEP}" -v r="${MAX_SHALLOW_RATIO}" -v f="${SHALLOW_FLOOR_US}" \
    'BEGIN { printf "%.1f", (d * r > f) ? d * r : f }')"
if awk -v s="${KEYSET_SHALLOW}" -v l="${SHALLOW_LIMIT}" 'BEGIN { exit !(s <= l) }'; then
    echo "OK: shallow keyset page is ${SHALLOW_RATIO}x the deep page (limit ${SHALLOW_LIMIT}us: ${MAX_SHALLOW_RATIO}x deep or ${SHALLOW_FLOOR_US}us floor)"
else
    echo "BREACH: shallow keyset page is ${SHALLOW_RATIO}x the deep page and ${KEYSET_SHALLOW}us (> ${SHALLOW_LIMIT}us = ${MAX_SHALLOW_RATIO}x deep or ${SHALLOW_FLOOR_US}us floor); the first /messages page is the worst case (ORDER BY no longer uses the ordered index?)"
    FAILURES=$((FAILURES + 1))
fi

echo ""
if [ "${FAILURES}" -gt 0 ]; then
    echo "==> Pagination Perf Gate: FAILED (${FAILURES} breach(es)); log: ${BENCH_LOG}"
    exit 1
fi
echo "==> Pagination Perf Gate: PASSED"
exit 0
