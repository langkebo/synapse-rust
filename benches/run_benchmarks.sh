#!/bin/bash
# Benchmark runner script for synapse-rust
#
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
    echo "  1) Run validation benchmarks"
    echo "  2) Run string operation benchmarks"
    echo "  3) Run data structure benchmarks"
    echo "  4) Run serialization benchmarks"
    echo "  5) Run all benchmarks"
    echo "  6) Compare results (baseline vs optimized)"
    echo "  7) Quick benchmark run (fast mode)"
    echo "  8) Full benchmark run (accurate mode)"
    echo "  q) Quit"
    echo ""
}

# Quick benchmark run (fast mode, less accurate)
run_quick() {
    echo -e "${YELLOW}Running quick benchmarks...${NC}"
    cargo bench --bench database_bench -- --sample-size 10 --warm-up-time 1 --measurement-time 1
}

# Full benchmark run (accurate mode)
run_full() {
    echo -e "${YELLOW}Running full benchmarks...${NC}"
    cargo bench --bench database_bench -- --sample-size 100 --warm-up-time 3 --measurement-time 5
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
        "validation"|"string"|"data"|"serialization"|"all")
            run_benchmark "database_bench" "$RESULTS_DIR/benchmark_results.json"
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [quick|full|compare|validation|string|data|serialization|all]"
            exit 1
            ;;
    esac
    exit 0
fi

# Interactive mode
show_menu
read -p "Enter choice [1-8, q]: " choice

case $choice in
    1|2|3|4|5)
        run_benchmark "database_bench" "$RESULTS_DIR/benchmark_results.json"
        ;;
    6)
        echo "Enter path to baseline results:"
        read -r baseline
        echo "Enter path to optimized results:"
        read -r optimized
        generate_report "$baseline" "$optimized"
        ;;
    7)
        run_quick
        ;;
    8)
        run_full
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
