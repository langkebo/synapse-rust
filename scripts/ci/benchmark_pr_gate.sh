#!/usr/bin/env bash
#
# PR Benchmark Gate: compares pure-Rust Criterion benchmarks against a baseline
# and fails the PR when a benchmark regresses beyond the threshold.
#
# ── What was broken (fixed 2026-09-11) ───────────────────────────────────────
#
# This script previously could not detect a regression, in three independent
# ways — audit: docs/audit/P4_ci_gate_integrity_2026-09-11.md.
#
#  1. `local` was used inside two top-level `while` loops. Bash rejects that
#     (`local: can only be used in a function`, exit 1), so whenever a baseline
#     *was* present the step died instead of reporting a verdict.
#
#  2. The downloaded baseline (`benchmark.txt`) is produced with
#     `--output-format bencher` (`test NAME ... bench: N ns/iter`), but the
#     parser keys on Criterion's text form (`time: [...]`). The two never
#     intersect, so zero benchmarks were ever compared.
#
#  3. When no baseline file existed the script printed
#     "PASSED (baseline comparison skipped)". Combined with the workflow's
#     `continue-on-error: true` on the download, a missing baseline was
#     indistinguishable from "no regressions" — permanent false green.
#
# It also benchmarked in the **default** profile while the baseline is recorded
# with `--profile release-perf`, making even correct plumbing incomparable.
#
# ── Environment ─────────────────────────────────────────────────────────────
#   BENCH_THRESHOLD_PERCENT   regression threshold (default: 15)
#   BENCH_BASELINE_PATH       baseline file in Criterion TEXT format (required)
#   BENCH_PROFILE             cargo profile (default: release-perf — must match
#                             the profile the baseline was recorded with)
#   BENCH_PR_GATE_SKIP_BENCH  when 1, do not run `cargo bench`; read current
#                             results from artifacts/pr_benchmark_current.txt
#                             (used by tests and for re-analysing a saved run)
#
# Exits 0 on no regression, 1 on regression or unusable baseline.

set -eo pipefail

THRESHOLD="${BENCH_THRESHOLD_PERCENT:-15}"
PROFILE="${BENCH_PROFILE:-release-perf}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

mkdir -p artifacts

CURRENT_RESULTS="${BENCH_PR_GATE_CURRENT_PATH:-artifacts/pr_benchmark_current.txt}"
# Human-readable log of what we actually parsed (kept as a CI artifact).
PARSED_REPORT="${BENCH_PR_GATE_PARSED_PATH:-artifacts/pr_benchmark_parsed.txt}"

if [ -n "${BENCH_PR_GATE_CURRENT_PATH:-}" ]; then
    mkdir -p "$(dirname "${CURRENT_RESULTS}")"
fi

echo "==> PR Benchmark Gate (threshold: ${THRESHOLD}%, profile: ${PROFILE})"

# ---------------------------------------------------------------------------
# Parse Criterion TEXT output into "name median_value unit" lines.
#
# Criterion prints long benchmark ids on their own line, then the timing on the
# next:
#     state_resolution_chain_10
#                             time:   [273.27 ns 274.52 ns 275.91 ns]
# so we join those pairs before extracting the median (second value).
# ---------------------------------------------------------------------------
extract_benchmarks() {
    local file="$1"
    [ -f "$file" ] || return 0
    awk '
        # A bare benchmark-id line: remember it.
        /^[A-Za-z_][A-Za-z0-9_\/:-]*$/ { pending = $0; next }
        # A timing line: pair with the pending id, or take the inline name.
        /time: *\[/ {
            name = (pending != "" ? pending : $1)
            # median = second of the three bracketed values, unit follows it
            if (match($0, /\[[^]]*\]/)) {
                bracket = substr($0, RSTART + 1, RLENGTH - 2)
                split(bracket, parts, " ")
                if (length(parts) >= 5) {
                    print name "\t" parts[3] "\t" parts[4]
                }
            }
            pending = ""
            next
        }
        { pending = "" }
    ' "$file"
}

# Convert a value+unit to nanoseconds.
# Echoes an empty string when the unit is unknown, so the caller can reject the
# baseline rather than compare fabricated numbers.
normalize_to_ns() {
    local value="$1"
    local unit="$2"
    value="${value//,/}"
    case "$unit" in
        ns) printf '%.4f' "$value" ;;
        us | µs) awk -v v="$value" 'BEGIN { printf "%.4f", v * 1000 }' ;;
        ms) awk -v v="$value" 'BEGIN { printf "%.4f", v * 1000000 }' ;;
        s) awk -v v="$value" 'BEGIN { printf "%.4f", v * 1000000000 }' ;;
        *) return 1 ;;
    esac
}

# ---------------------------------------------------------------------------
# 1) Current results
# ---------------------------------------------------------------------------
if [ "${BENCH_PR_GATE_SKIP_BENCH:-0}" = "1" ]; then
    echo "Skipping benchmark run (BENCH_PR_GATE_SKIP_BENCH=1); using ${CURRENT_RESULTS}"
    if [ ! -s "${CURRENT_RESULTS}" ]; then
        echo "ERROR: ${CURRENT_RESULTS} 不存在或为空，无法比较" >&2
        exit 1
    fi
else
    mkdir -p artifacts
    echo "Running performance_federation_benchmarks (--profile ${PROFILE})..."
    cargo bench --locked --profile "${PROFILE}" --bench performance_federation_benchmarks \
        -- --noplot 2>&1 | tee artifacts/bench_federation.log

    echo "Running performance_membership_benchmarks (--profile ${PROFILE})..."
    cargo bench --locked --profile "${PROFILE}" --bench performance_membership_benchmarks \
        -- --noplot 2>&1 | tee artifacts/bench_membership.log

    : >"${CURRENT_RESULTS}"
    extract_benchmarks artifacts/bench_federation.log >>"${CURRENT_RESULTS}"
    extract_benchmarks artifacts/bench_membership.log >>"${CURRENT_RESULTS}"
fi

if [ ! -s "${CURRENT_RESULTS}" ]; then
    echo "ERROR: 未能从本次基准运行中解析出任何测量值；门禁无法比较。" >&2
    echo "       若基准目标名或 Criterion 输出格式变化，请同步更新 extract_benchmarks。" >&2
    exit 1
fi

sort -o "${CURRENT_RESULTS}" "${CURRENT_RESULTS}"
echo ""
echo "Current benchmark results (${CURRENT_RESULTS}):"
cat "${CURRENT_RESULTS}"
echo ""

# ---------------------------------------------------------------------------
# 2) Baseline — required, and must actually be parseable
# ---------------------------------------------------------------------------
if [ -z "${BENCH_BASELINE_PATH:-}" ]; then
    echo "ERROR: BENCH_BASELINE_PATH 未设置。" >&2
    echo "       门禁必须与基线比较；「没有基线」不等于「没有回归」。" >&2
    exit 1
fi
if [ ! -f "${BENCH_BASELINE_PATH}" ]; then
    echo "ERROR: 基线文件不存在: ${BENCH_BASELINE_PATH}" >&2
    echo "       （历史缺陷：此处曾打印 PASSED，使缺失基线伪装成通过）" >&2
    exit 1
fi

extract_benchmarks "${BENCH_BASELINE_PATH}" >"${PARSED_REPORT}"

if [ ! -s "${PARSED_REPORT}" ]; then
    echo "ERROR: 基线格式无法解析（解析出 0 个基准）: ${BENCH_BASELINE_PATH}" >&2
    echo "       本脚本只接受 Criterion **文本**格式（形如 \`time: [...]\`）。" >&2
    echo "       注意 benchmark.yml 的 benchmark.txt 使用 --output-format bencher，" >&2
    echo "       其行形如 \`test NAME ... bench: N ns/iter\`，与此不兼容；" >&2
    echo "       请使用 benchmark_standard.txt（Criterion 文本输出）。" >&2
    head -5 "${BENCH_BASELINE_PATH}" >&2 || true
    exit 1
fi

sort -o "${PARSED_REPORT}" "${PARSED_REPORT}"
echo "Baseline results (parsed from ${BENCH_BASELINE_PATH}):"
cat "${PARSED_REPORT}"
echo ""

# ---------------------------------------------------------------------------
# 3) Compare
# ---------------------------------------------------------------------------
REGRESSIONS=0
COMPARED=0
MISSING_BASELINE=0

while IFS=$'\t' read -r bench_name bench_value bench_unit; do
    [ -n "${bench_name}" ] || continue

    baseline_line="$(grep -P "^\\Q${bench_name}\\E\t" "${PARSED_REPORT}" 2>/dev/null || grep -F "${bench_name}	" "${PARSED_REPORT}" || true)"
    if [ -z "${baseline_line}" ]; then
        echo "INFO: ${bench_name} — 基线中无此项（新基准），跳过比较"
        MISSING_BASELINE=$((MISSING_BASELINE + 1))
        continue
    fi

    base_value="$(printf '%s' "${baseline_line}" | cut -f2)"
    base_unit="$(printf '%s' "${baseline_line}" | cut -f3)"

    if ! current_ns="$(normalize_to_ns "${bench_value}" "${bench_unit}")"; then
        echo "ERROR: 未知单位 '${bench_unit}'（当前结果 ${bench_name}）；拒绝用不可比数字出结论" >&2
        exit 1
    fi
    if ! base_ns="$(normalize_to_ns "${base_value}" "${base_unit}")"; then
        echo "ERROR: 未知单位 '${base_unit}'（基线 ${bench_name}）；拒绝用不可比数字出结论" >&2
        exit 1
    fi
    if awk -v b="${base_ns}" 'BEGIN { exit !(b == 0) }'; then
        echo "INFO: ${bench_name} — 基线值为 0，无法计算变化率，跳过"
        continue
    fi

    COMPARED=$((COMPARED + 1))
    pct_change="$(awk -v c="${current_ns}" -v b="${base_ns}" 'BEGIN { printf "%.2f", ((c - b) / b) * 100 }')"
    abs_change="${pct_change#-}"

    if awk -v a="${abs_change}" -v t="${THRESHOLD}" 'BEGIN { exit !(a > t) }'; then
        echo "REGRESSION: ${bench_name} changed by ${pct_change}% (threshold: ${THRESHOLD}%)"
        REGRESSIONS=$((REGRESSIONS + 1))
    else
        echo "OK: ${bench_name} changed by ${pct_change}% (within threshold)"
    fi
done <"${CURRENT_RESULTS}"

echo ""
echo "Compared ${COMPARED} benchmark(s); ${MISSING_BASELINE} had no baseline entry."

if [ "${COMPARED}" -eq 0 ]; then
    echo "ERROR: 没有任何基准被实际比较（基线内容与当前基准名不匹配）。" >&2
    echo "       门禁不能在没有比较的情况下声称通过。" >&2
    exit 1
fi

if [ "${REGRESSIONS}" -gt 0 ]; then
    echo "PR Benchmark Gate: FAILED (${REGRESSIONS} regression(s) detected)"
    GATE_FAILED=1
else
    echo "PR Benchmark Gate: PASSED (no regressions beyond ${THRESHOLD}%)"
    GATE_FAILED=0
fi

# ---------------------------------------------------------------------------
# 4) JSON report for artifacts
# ---------------------------------------------------------------------------
JSON_REPORT="artifacts/pr_benchmark_results.json"
{
    echo "{"
    echo "  \"threshold_percent\": ${THRESHOLD},"
    echo "  \"profile\": \"${PROFILE}\","
    echo "  \"compared\": ${COMPARED},"
    echo "  \"regressions\": ${REGRESSIONS},"
    echo "  \"benchmarks\": ["
    FIRST=true
    while IFS=$'\t' read -r bench_name bench_value bench_unit; do
        [ -n "${bench_name}" ] || continue
        if [ "${FIRST}" = "true" ]; then
            FIRST=false
        else
            echo ","
        fi
        printf '    {"name": "%s", "value": "%s", "unit": "%s"}' "${bench_name}" "${bench_value}" "${bench_unit}"
    done <"${CURRENT_RESULTS}"
    echo ""
    echo "  ]"
    echo "}"
} >"${JSON_REPORT}"

echo ""
echo "Benchmark report generated: ${JSON_REPORT}"

exit "${GATE_FAILED}"
