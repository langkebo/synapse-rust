#!/bin/bash
# Benchmark runner script for synapse-rust
# Updated: 2026-04-04
# This script runs benchmarks and generates comparison reports.

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
BENCH_DIR="$PROJECT_ROOT/benches"
RESULTS_DIR="$BENCH_DIR/results"
BASELINE_FILE="$RESULTS_DIR/baseline_$(date +%Y%m%d).json"

# Create results directory if it doesn't exist
mkdir -p "$RESULTS_DIR"

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  Synapse Rust Benchmark Runner${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# Check if Criterion is installed
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}Error: cargo not found. Please install Rust/Cargo first.${NC}"
    exit 1
fi

# Check if benchmark data exists
check_benchmark_data() {
    echo -e "${YELLOW}Checking benchmark data...${NC}"

    if ! bash "$PROJECT_ROOT/scripts/generate_benchmark_data.sh" stats 2>/dev/null | grep -q "bench_user"; then
        echo -e "${YELLOW}No benchmark data found. Generating small dataset...${NC}"
        bash "$PROJECT_ROOT/scripts/generate_benchmark_data.sh" preset small
    else
        echo -e "${GREEN}✓ Benchmark data exists${NC}"
    fi
}

# Function to run a specific benchmark
run_benchmark() {
    local bench_name=$1
    local output_file=$2

    echo -e "${YELLOW}Running benchmark: $bench_name${NC}"
    echo ""

    cd "$PROJECT_ROOT"

    # Run the benchmark and save output
    cargo bench --bench "$bench_name" -- --save-baseline main \
        --output-format bencher | tee "$RESULTS_DIR/${bench_name}_output.txt"

    # Copy the Criterion JSON output if it exists
    if [ -f "target/criterion/$bench_name" ]; then
        cp -r "target/criterion/$bench_name" "$RESULTS_DIR/"
    fi

    echo -e "${GREEN}✓ Benchmark complete: $bench_name${NC}"
    echo ""
}

# Generate baseline report
generate_baseline_report() {
    local output="$RESULTS_DIR/BASELINE_REPORT_$(date +%Y%m%d_%H%M%S).md"

    echo -e "${YELLOW}Generating baseline report...${NC}"

    cat > "$output" <<EOF
# Performance Baseline Report

> Generated: $(date +%Y-%m-%d\ %H:%M:%S)
> Database: synapse_test
> Benchmark Data: $(bash "$PROJECT_ROOT/scripts/generate_benchmark_data.sh" stats 2>/dev/null | grep -c "bench_" || echo "0") records

## Summary

This report captures the current performance baseline for synapse-rust.

## Benchmark Results

EOF

    # Append benchmark outputs
    for file in "$RESULTS_DIR"/*_output.txt; do
        if [ -f "$file" ]; then
            echo "### $(basename "$file" _output.txt)" >> "$output"
            echo "" >> "$output"
            echo '```' >> "$output"
            tail -20 "$file" >> "$output"
            echo '```' >> "$output"
            echo "" >> "$output"
        fi
    done

    cat >> "$output" <<EOF

## Environment

- Rust Version: $(rustc --version)
- Cargo Version: $(cargo --version)
- OS: $(uname -s)
- Architecture: $(uname -m)

## Next Steps

1. Save this baseline for future comparisons
2. Run benchmarks after optimizations
3. Compare results to detect regressions

---

**Report saved to**: $output
EOF

    echo -e "${GREEN}✓ Baseline report generated: $output${NC}"
    echo ""
}

# Function to generate comparison report
generate_report() {
    local baseline=$1
    local optimized=$2
    local output=${3:-"$RESULTS_DIR/BENCHMARK_REPORT.md"}

    echo -e "${YELLOW}Generating comparison report...${NC}"

    if [ -f "$baseline" ] && [ -f "$optimized" ]; then
        python3 "$SCRIPT_DIR/results/compare_results.py" "$baseline" "$optimized" "$output"
    else
        echo -e "${RED}Error: Cannot find baseline or optimized results${NC}"
        echo "  Baseline: $baseline"
        echo "  Optimized: $optimized"
        return 1
    fi
}

# Main menu
show_menu() {
    echo ""
    echo -e "${BLUE}Select an option:${NC}"
    echo "  1) Run API benchmarks"
    echo "  2) Run federation benchmarks"
    echo "  3) Run database benchmarks"
    echo "  4) Run all Criterion benchmarks"
    echo "  5) Run manual performance tests"
    echo "  6) Run all performance suites"
    echo "  7) Generate baseline report"
    echo "  8) Compare results (baseline vs optimized)"
    echo "  9) Quick benchmark run (fast mode)"
    echo "  10) Full benchmark run (accurate mode)"
    echo "  11) Setup benchmark data"
    echo "  q) Quit"
    echo ""
}

# Quick benchmark run (fast mode, less accurate)
run_quick() {
    echo -e "${YELLOW}Running quick benchmarks...${NC}"
    check_benchmark_data
    cargo bench --bench performance_api_benchmarks -- --sample-size 10 --warm-up-time 1 --measurement-time 1
    cargo bench --bench performance_federation_benchmarks -- --sample-size 10 --warm-up-time 1 --measurement-time 1
    cargo bench --bench performance_database_benchmarks -- --sample-size 10 --warm-up-time 1 --measurement-time 1
}

# Full benchmark run (accurate mode)
run_full() {
    echo -e "${YELLOW}Running full benchmarks...${NC}"
    check_benchmark_data
    cargo bench --bench performance_api_benchmarks -- --sample-size 100 --warm-up-time 3 --measurement-time 5
    cargo bench --bench performance_federation_benchmarks -- --sample-size 100 --warm-up-time 3 --measurement-time 5
    cargo bench --bench performance_database_benchmarks -- --sample-size 100 --warm-up-time 3 --measurement-time 5
    generate_baseline_report
}

# Setup benchmark data
setup_benchmark_data() {
    echo -e "${YELLOW}Setting up benchmark data...${NC}"
    echo ""
    echo "Select dataset size:"
    echo "  1) Small (1K users, 100 rooms, 10K events)"
    echo "  2) Medium (10K users, 1K rooms, 100K events)"
    echo "  3) Large (100K users, 10K rooms, 1M events)"
    echo ""
    read -p "Enter choice [1-3]: " choice

    case $choice in
        1)
            bash "$PROJECT_ROOT/scripts/generate_benchmark_data.sh" preset small
            ;;
        2)
            bash "$PROJECT_ROOT/scripts/generate_benchmark_data.sh" preset medium
            ;;
        3)
            bash "$PROJECT_ROOT/scripts/generate_benchmark_data.sh" preset large
            ;;
        *)
            echo -e "${RED}Invalid choice${NC}"
            return 1
            ;;
    esac
}
    echo -e "${YELLOW}Running full benchmarks...${NC}"
    cargo bench --bench performance_api_benchmarks -- --sample-size 100 --warm-up-time 3 --measurement-time 5
    cargo bench --bench performance_federation_benchmarks -- --sample-size 100 --warm-up-time 3 --measurement-time 5
}

# Parse command line arguments
if [ $# -gt 0 ]; then
    case "$1" in
        "quick")
            run_quick
            ;;
        "full")
            run_full
            ;;
        "compare")
            if [ $# -lt 3 ]; then
                echo "Usage: $0 compare <baseline.json> <optimized.json> [output.md]"
                exit 1
            fi
            generate_report "$2" "$3" "$4"
            ;;
        "api")
            check_benchmark_data
            run_benchmark "performance_api_benchmarks" "$RESULTS_DIR/benchmark_results.json"
            ;;
        "federation")
            check_benchmark_data
            run_benchmark "performance_federation_benchmarks" "$RESULTS_DIR/benchmark_results.json"
            ;;
        "database")
            check_benchmark_data
            run_benchmark "performance_database_benchmarks" "$RESULTS_DIR/benchmark_results.json"
            ;;
        "manual")
            cargo test --features performance-tests --test performance_manual -- --nocapture
            ;;
        "all")
            check_benchmark_data
            run_benchmark "performance_api_benchmarks" "$RESULTS_DIR/benchmark_results.json"
            run_benchmark "performance_federation_benchmarks" "$RESULTS_DIR/benchmark_results.json"
            run_benchmark "performance_database_benchmarks" "$RESULTS_DIR/benchmark_results.json"
            generate_baseline_report
            ;;
        "setup")
            setup_benchmark_data
            ;;
        "baseline")
            generate_baseline_report
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [quick|full|compare|api|federation|database|manual|all|setup|baseline]"
            exit 1
            ;;
    esac
    exit 0
fi

# Interactive mode
show_menu
read -p "Enter choice [1-11, q]: " choice

case $choice in
    1)
        check_benchmark_data
        run_benchmark "performance_api_benchmarks" "$RESULTS_DIR/benchmark_results.json"
        ;;
    2)
        check_benchmark_data
        run_benchmark "performance_federation_benchmarks" "$RESULTS_DIR/benchmark_results.json"
        ;;
    3)
        check_benchmark_data
        run_benchmark "performance_database_benchmarks" "$RESULTS_DIR/benchmark_results.json"
        ;;
    4)
        check_benchmark_data
        run_benchmark "performance_api_benchmarks" "$RESULTS_DIR/benchmark_results.json"
        run_benchmark "performance_federation_benchmarks" "$RESULTS_DIR/benchmark_results.json"
        run_benchmark "performance_database_benchmarks" "$RESULTS_DIR/benchmark_results.json"
        ;;
    5)
        cargo test --features performance-tests --test performance_manual -- --nocapture
        ;;
    6)
        check_benchmark_data
        run_benchmark "performance_api_benchmarks" "$RESULTS_DIR/benchmark_results.json"
        run_benchmark "performance_federation_benchmarks" "$RESULTS_DIR/benchmark_results.json"
        run_benchmark "performance_database_benchmarks" "$RESULTS_DIR/benchmark_results.json"
        cargo test --features performance-tests --test performance_manual -- --nocapture
        generate_baseline_report
        ;;
    7)
        generate_baseline_report
        ;;
    8)
        echo "Enter path to baseline results:"
        read -r baseline
        echo "Enter path to optimized results:"
        read -r optimized
        generate_report "$baseline" "$optimized"
        ;;
    9)
        run_quick
        ;;
    10)
        run_full
        ;;
    11)
        setup_benchmark_data
        ;;
    q|Q)
        echo "Goodbye!"
        exit 0
        ;;
    *)
        echo -e "${RED}Invalid choice${NC}"
        exit 1
        ;;
esac

echo ""
echo -e "${GREEN}Benchmark run complete!${NC}"
echo -e "Results saved to: ${BLUE}$RESULTS_DIR${NC}"
