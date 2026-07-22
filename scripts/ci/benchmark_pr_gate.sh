#!/usr/bin/env bash
#
# PR Benchmark Gate: Compares pure-Rust benchmarks against a baseline.
# Runs on every PR to block performance regressions.
#
# Environment:
#   BENCH_THRESHOLD_PERCENT — regression threshold (default: 15)
#   BENCH_BASELINE_PATH     — path to baseline benchmark output (optional)
#
# Exits 0 if no regression, 1 if any benchmark regresses beyond threshold.

set -eu

THRESHOLD="${BENCH_THRESHOLD_PERCENT:-15}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

mkdir -p artifacts

echo "==> PR Benchmark Gate (threshold: ${THRESHOLD}%)"

# ---------------------------------------------------------------------------
# 1) Run pure-Rust benchmarks
# ---------------------------------------------------------------------------
echo "Running performance_federation_benchmarks..."
cargo bench --locked --bench performance_federation_benchmarks -- --noplot 2>&1 | tee artifacts/bench_federation.log

echo "Running performance_membership_benchmarks..."
cargo bench --locked --bench performance_membership_benchmarks -- --noplot 2>&1 | tee artifacts/bench_membership.log

# ---------------------------------------------------------------------------
# 2) Parse Criterion output to extract timings
# ---------------------------------------------------------------------------
# Criterion output format (text mode):
#   test_name               time:   [value low value high value]
# We extract the median (middle value in the bracket).

parse_bench_line() {
    local line="$1"
    # Match: "  bench_name              time: [1.2345 ms 1.3456 ms 1.4567 ms]"
    # Extract the median (second number)
    echo "$line" | sed -n 's/.*time: \[[^ ]* \([^ ]*\) [^ ]* [^ ]*\].*/\1/p'
}

parse_bench_unit() {
    local line="$1"
    echo "$line" | sed -n 's/.*time: \[[^ ]* [^ ]* \([^ ]*\) [^ ]*\].*/\1/p'
}

# Extract benchmark results from a log file
# Returns lines in format: "bench_name value unit"
extract_benchmarks() {
    local file="$1"
    while IFS= read -r line; do
        # Look for lines containing "time: [" which indicate benchmark results
        if echo "$line" | grep -q 'time: \['; then
            # Extract benchmark name (first word before time:)
            local name
            name=$(echo "$line" | sed 's/^[[:space:]]*//' | sed 's/[[:space:]]*time:.*//')
            # Extract median value
            local value
            value=$(echo "$line" | sed 's/.*time: \[[^ ]* \([^ ]*\) [^ ]* [^ ]*\].*/\1/')
            # Extract unit (ns, us, ms, s)
            local unit
            unit=$(echo "$line" | sed 's/.*time: \[[^ ]* [^ ]* \([^ ]*\) [^ ]*\].*/\1/')
            if [ -n "$name" ] && [ -n "$value" ] && [ -n "$unit" ]; then
                echo "$name $value $unit"
            fi
        fi
    done < "$file"
}

# Convert value+unit to nanoseconds for comparison
normalize_to_ns() {
    local value="$1"
    local unit="$2"
    # Remove commas from value
    value=$(echo "$value" | tr -d ',')

    case "$unit" in
        ns) echo "$value" ;;
        us|µs) awk "BEGIN {print $value * 1000}" ;;
        ms) awk "BEGIN {print $value * 1000000}" ;;
        s)  awk "BEGIN {print $value * 1000000000}" ;;
        *)  echo "$value" ;; # unknown unit, pass through
    esac
}

# ---------------------------------------------------------------------------
# 3) Extract current results
# ---------------------------------------------------------------------------
CURRENT_RESULTS="artifacts/pr_benchmark_results.txt"
: > "$CURRENT_RESULTS"

for log in artifacts/bench_federation.log artifacts/bench_membership.log; do
    if [ -f "$log" ]; then
        extract_benchmarks "$log" >> "$CURRENT_RESULTS"
    fi
done

# Sort for consistent comparison
sort "$CURRENT_RESULTS" > "${CURRENT_RESULTS}.sorted"
mv "${CURRENT_RESULTS}.sorted" "$CURRENT_RESULTS"

echo ""
echo "Current benchmark results:"
cat "$CURRENT_RESULTS"
echo ""

# ---------------------------------------------------------------------------
# 4) Compare against baseline if available
# ---------------------------------------------------------------------------
if [ -n "${BENCH_BASELINE_PATH:-}" ] && [ -f "$BENCH_BASELINE_PATH" ]; then
    echo "Comparing against baseline: $BENCH_BASELINE_PATH"

    # Parse baseline results (same format)
    BASELINE_RESULTS="artifacts/baseline_results.txt"
    : > "$BASELINE_RESULTS"

    while IFS= read -r line; do
        if echo "$line" | grep -q 'time: \['; then
            local name
            name=$(echo "$line" | sed 's/^[[:space:]]*//' | sed 's/[[:space:]]*time:.*//')
            local value
            value=$(echo "$line" | sed 's/.*time: \[[^ ]* \([^ ]*\) [^ ]* [^ ]*\].*/\1/')
            local unit
            unit=$(echo "$line" | sed 's/.*time: \[[^ ]* [^ ]* \([^ ]*\) [^ ]*\].*/\1/')
            if [ -n "$name" ] && [ -n "$value" ] && [ -n "$unit" ]; then
                echo "$name $value $unit" >> "$BASELINE_RESULTS"
            fi
        fi
    done < "$BENCH_BASELINE_PATH"

    sort "$BASELINE_RESULTS" > "${BASELINE_RESULTS}.sorted"
    mv "${BASELINE_RESULTS}.sorted" "$BASELINE_RESULTS"

    echo "Baseline results:"
    cat "$BASELINE_RESULTS"
    echo ""

    # Compare each benchmark
    REGRESSIONS=0
    while IFS= read -r current_line; do
        local bench_name bench_value bench_unit
        bench_name=$(echo "$current_line" | awk '{print $1}')
        bench_value=$(echo "$current_line" | awk '{print $2}')
        bench_unit=$(echo "$current_line" | awk '{print $3}')

        # Find matching baseline
        local baseline_line
        baseline_line=$(grep "^$bench_name " "$BASELINE_RESULTS" || true)

        if [ -n "$baseline_line" ]; then
            local base_value base_unit
            base_value=$(echo "$baseline_line" | awk '{print $2}')
            base_unit=$(echo "$baseline_line" | awk '{print $3}')

            # Normalize to ns
            local current_ns base_ns
            current_ns=$(normalize_to_ns "$bench_value" "$bench_unit")
            base_ns=$(normalize_to_ns "$base_value" "$base_unit")

            # Calculate percentage change
            local pct_change
            pct_change=$(awk "BEGIN {printf \"%.2f\", (($current_ns - $base_ns) / $base_ns) * 100}")

            # Check if regression exceeds threshold
            local abs_change
            abs_change=$(echo "$pct_change" | tr -d '-')

            if awk "BEGIN {exit !($abs_change > $THRESHOLD)}"; then
                echo "REGRESSION: $bench_name changed by ${pct_change}% (threshold: ${THRESHOLD}%)"
                REGRESSIONS=$((REGRESSIONS + 1))
            else
                echo "OK: $bench_name changed by ${pct_change}% (within threshold)"
            fi
        else
            echo "INFO: $bench_name — no baseline found (new benchmark)"
        fi
    done < "$CURRENT_RESULTS"

    echo ""
    if [ "$REGRESSIONS" -gt 0 ]; then
        echo "PR Benchmark Gate: FAILED ($REGRESSIONS regression(s) detected)"
        exit 1
    else
        echo "PR Benchmark Gate: PASSED (no regressions beyond ${THRESHOLD}%)"
    fi
else
    echo "No baseline found. Recording results for future comparison."
    echo "PR Benchmark Gate: PASSED (baseline comparison skipped)"
fi

# ---------------------------------------------------------------------------
# 5) Generate JSON report for artifacts
# ---------------------------------------------------------------------------
JSON_REPORT="artifacts/pr_benchmark_results.json"
echo "{" > "$JSON_REPORT"
echo "  \"threshold_percent\": $THRESHOLD," >> "$JSON_REPORT"
echo "  \"benchmarks\": [" >> "$JSON_REPORT"

FIRST=true
while IFS= read -r line; do
    local bench_name bench_value bench_unit
    bench_name=$(echo "$line" | awk '{print $1}')
    bench_value=$(echo "$line" | awk '{print $2}')
    bench_unit=$(echo "$line" | awk '{print $3}')

    if [ "$FIRST" = "true" ]; then
        FIRST=false
    else
        echo "," >> "$JSON_REPORT"
    fi

    echo "    {\"name\": \"$bench_name\", \"value\": \"$bench_value\", \"unit\": \"$bench_unit\"}" >> "$JSON_REPORT"
done < "$CURRENT_RESULTS"

echo "" >> "$JSON_REPORT"
echo "  ]" >> "$JSON_REPORT"
echo "}" >> "$JSON_REPORT"

echo ""
echo "Benchmark report generated: $JSON_REPORT"
