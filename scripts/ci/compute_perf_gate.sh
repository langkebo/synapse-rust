#!/usr/bin/env bash
#
# Compute Performance Regression Gate
#
# Runs the **pure-compute** Criterion benchmarks (no server, no database) and
# fails when a benchmark's mean exceeds a calibrated ceiling.
#
# ── Why this exists ──────────────────────────────────────────────────────────
#
# TESTING.md used to declare per-endpoint P95 targets (500/1000/100 ms, and
# per-benchmark targets like "whoami ≤20 ms"). Nothing executed them: no test,
# no CI step read them. They were documentation, and measured reality was
# 15–30× *better* than the numbers — so even had they run they could not have
# caught a regression of any realistic size.
#
# Meanwhile the only threshold that existed in code was
# `sliding_sync_perf_gate.sh` (5000 ms), which needs a database and runs in a
# separate job.
#
# This gate is the executable replacement for the pure-compute half: it measures
# benchmarks that need nothing but CPU, so it always runs, and it fails loudly on
# order-of-magnitude regressions.
#
# ── On the ceilings ──────────────────────────────────────────────────────────
#
# These are intentionally GENEROUS (~10× the recorded baseline). CI hardware is
# not comparable to the machine the baseline was taken on, so a tight threshold
# would be a flaky gate — and a flaky gate gets disabled, which is how we got
# here. The target is *order-of-magnitude* regressions (lost vectorisation, an
# accidental allocation or lock on a hot path, a wrong data structure), not
# 10–20% drift. Tightening is a deliberate act: lower a ceiling only after
# recording a new baseline on the same runner class.
#
# Baselines recorded 2026-09-11 on an Apple-silicon dev machine,
# `--warm-up-time 1 --measurement-time 3 --sample-size 30`:
#
#   state_resolution_chain_10    274.52 ns
#   state_resolution_chain_100   295.60 ns
#   auth_chain_build_10            5.36 µs
#   membership_transitions/*     1.07–1.81 ns   (ceiling: 5 µs flat)
#
# Environment:
#   COMPUTE_PERF_GATE_STRICT=1   fail if a benchmark produced no measurement
#                                (default) — a missing benchmark must not pass
#   COMPUTE_PERF_GATE_SKIP_BUILD=1  skip `cargo bench --no-run` pre-check
#
# Exits 0 when every benchmark is within its ceiling, 1 otherwise.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

STRICT="${COMPUTE_PERF_GATE_STRICT:-1}"
mkdir -p artifacts

echo "==> Compute Performance Regression Gate"

# ---------------------------------------------------------------------------
# 1) Ceilings: "benchmark_name<TAB>ceiling_in_ns"
# ---------------------------------------------------------------------------
CEILINGS="$(
    cat <<'EOF'
state_resolution_chain_10	3000
state_resolution_chain_100	3000
auth_chain_build_10	60000
EOF
)"

# Membership transitions are all single-digit nanoseconds — dominated by the
# benchmark loop itself. A flat ceiling catches a pathological change (an
# allocation, lock, or hashmap lookup creeping onto this hot path) without
# pretending to resolve sub-nanosecond differences.
MEMBERSHIP_CEILING_NS=5000

TOTAL=0
FAILURES=0
MISSING=0

# ---------------------------------------------------------------------------
# 2) Run a Criterion target and parse "name ... time: [lo mean hi]"
# ---------------------------------------------------------------------------
# Criterion prints long benchmark ids on their own line followed by the time
# line, so we join those pairs before parsing.
run_target() {
    local target="$1"
    shift
    local filter="${1:-}"
    local log="artifacts/compute_perf_${target}.log"

    echo "    running ${target}${filter:+ (filter: ${filter})}..."
    # shellcheck disable=SC2086
    if ! cargo bench --locked --bench "${target}" \
        ${filter:+$filter} \
        -- --warm-up-time 1 --measurement-time 3 --sample-size 30 >"${log}" 2>&1; then
        echo "ERROR: ${target} failed to run; see ${log}"
        tail -20 "${log}"
        FAILURES=$((FAILURES + 1))
        return
    fi

    # Join "<name>\n   time: [...]" into single lines.
    awk '
        /^[A-Za-z_][A-Za-z0-9_\/:-]*$/ { pending = $0; next }
        /time: *\[/ {
            if (pending != "") { print pending "\t" $0; pending = "" }
            else { print $1 "\t" $0 }
            next
        }
        { pending = "" }
    ' "${log}" >>"${MEASUREMENTS_FILE}"
}

# Reset once, before the first target: `run_target` only appends.
MEASUREMENTS_FILE="artifacts/compute_perf_measurements.txt"
: >"${MEASUREMENTS_FILE}"
run_target performance_federation_benchmarks
run_target performance_membership_benchmarks

if [[ "${STRICT}" != "1" ]]; then
    echo "    (non-strict mode: missing measurements will warn only)"
fi

# ---------------------------------------------------------------------------
# 3) Compare
# ---------------------------------------------------------------------------
echo "    measurements:"
while IFS=$'\t' read -r name time_line; do
    [[ -z "${name}" ]] && continue
    TOTAL=$((TOTAL + 1))

    # "time:   [273.27 ns 274.52 ns 275.91 ns]" -> mean token
    mean_raw="$(echo "${time_line}" | sed -n 's/.*\[\([^]]*\)\].*/\1/p' | awk '{print $3, $4}')"
    value="$(echo "${mean_raw}" | awk '{print $1}')"
    unit="$(echo "${mean_raw}" | awk '{print $2}')"

    case "${unit}" in
        ns) factor=1 ;;
        µs | us) factor=1000 ;;
        ms) factor=1000000 ;;
        s) factor=1000000000 ;;
        *)
            echo "      WARNING: unknown unit '${unit}' for ${name}"
            MISSING=$((MISSING + 1))
            continue
            ;;
    esac

    value_ns="$(awk -v v="${value}" -v f="${factor}" 'BEGIN { printf "%.0f", v * f }')"

    # Look up the ceiling: exact match first, then the membership flat ceiling.
    ceiling=""
    while IFS=$'\t' read -r c_name c_ns; do
        [[ -z "${c_name}" ]] && continue
        if [[ "${name}" == "${c_name}" ]]; then
            ceiling="${c_ns}"
            break
        fi
    done <<<"${CEILINGS}"
    if [[ -z "${ceiling}" && "${name}" == membership_transitions/* ]]; then
        ceiling="${MEMBERSHIP_CEILING_NS}"
    fi

    if [[ -z "${ceiling}" ]]; then
        echo "      ${name}: ${value} ${unit}  (未设阈值，仅记录)"
        continue
    fi

    if [[ "${value_ns}" -gt "${ceiling}" ]]; then
        echo "      BREACH: ${name}: ${value} ${unit} (${value_ns} ns) > ceiling ${ceiling} ns"
        FAILURES=$((FAILURES + 1))
    else
        echo "      OK: ${name}: ${value} ${unit} (${value_ns} ns <= ${ceiling} ns)"
    fi
done <"${MEASUREMENTS_FILE}"

# ---------------------------------------------------------------------------
# 4) Assert we actually measured the benchmarks we think we did
# ---------------------------------------------------------------------------
EXPECTED=4 # 3 federation + at least 1 membership
if [[ "${TOTAL}" -lt "${EXPECTED}" ]]; then
    echo "ERROR: 只测得 ${TOTAL} 个基准（期望 >= ${EXPECTED}）—— 基准可能被静默跳过"
    if [[ "${STRICT}" = "1" ]]; then
        MISSING=$((MISSING + 1))
    fi
fi

echo ""
echo "==> Summary: measured=${TOTAL} breaches=${FAILURES} missing=${MISSING}"

if [[ "${FAILURES}" -gt 0 || "${MISSING}" -gt 0 ]]; then
    echo "==> Compute Performance Gate: FAILED"
    echo "    日志: artifacts/compute_perf_*.log"
    echo "    若确认是环境差异（换了 runner 规格），请在同规格机器上重录基线后调整阈值；"
    echo "    不要为了让门禁变绿而放宽到失去意义。"
    exit 1
fi

echo "==> Compute Performance Gate: PASSED"
exit 0
