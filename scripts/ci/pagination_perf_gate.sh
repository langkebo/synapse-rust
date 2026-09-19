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
#       3. the keyset deep page plan still uses an index (`index_scan=1`);
#       4. the real deep-page gain is at least `PAGINATION_MIN_GAIN`
#          (default 2.0x). Measured healthy: ~5-9x. Degraded (index dropped, or
#          the keyset predicate replaced by OFFSET): ~1-1.5x → fails.
#
# ## Environment
#   BENCHMARK_DATABASE_URL     Postgres URL (required; the bench reads it and
#                              skips when unreachable — which fails the gate)
#   PAGINATION_MIN_GAIN        minimum real speedup, default 2.0
#   PAGINATION_MIN_ROWS        minimum seeded fixture rows, default 150000
#
# Exit codes: 0 = pass; 1 = real breach (regression / wrong page / lost index /
# too-slow gain); 2 = the gate cannot prove its premise (missing measurement,
# unparseable numbers) — never exit 0 in that case.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

MIN_GAIN="${PAGINATION_MIN_GAIN:-2.0}"
MIN_ROWS="${PAGINATION_MIN_ROWS:-150000}"
BENCH_LOG="artifacts/pagination_perf_gate.log"

mkdir -p artifacts

echo "==> DB-backed pagination perf gate (E4)"
echo "    min gain: ${MIN_GAIN}x   min fixture rows: ${MIN_ROWS}"

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
OFFSET_DEEP="$(field offset_deep_us)"
INDEX_SCAN="$(field index_scan)"
CORRECT="$(field correct)"

[ -n "${ROWS}" ] || premise_fail "measurement line has no rows= field: ${PERF_LINE}"
[ -n "${KEYSET_DEEP}" ] || premise_fail "measurement line has no keyset_deep_us= field: ${PERF_LINE}"
[ -n "${OFFSET_DEEP}" ] || premise_fail "measurement line has no offset_deep_us= field: ${PERF_LINE}"

# Reject anything that is not a plain positive number before comparing.
is_positive_number() {
    awk -v v="$1" 'BEGIN { exit !(v ~ /^[0-9]+(\.[0-9]+)?$/ && v > 0) }'
}
is_positive_number "${KEYSET_DEEP}" || premise_fail "keyset_deep_us is not a positive number: '${KEYSET_DEEP}'"
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

# 3) A lost index is the canonical keyset regression; the plan reports it.
if [ "${INDEX_SCAN}" != "1" ]; then
    echo "BREACH: keyset deep-page plan contains a Seq Scan (index_scan=${INDEX_SCAN:-missing})"
    FAILURES=$((FAILURES + 1))
else
    echo "OK: keyset deep-page plan uses an index"
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

echo ""
if [ "${FAILURES}" -gt 0 ]; then
    echo "==> Pagination Perf Gate: FAILED (${FAILURES} breach(es)); log: ${BENCH_LOG}"
    exit 1
fi
echo "==> Pagination Perf Gate: PASSED"
exit 0
