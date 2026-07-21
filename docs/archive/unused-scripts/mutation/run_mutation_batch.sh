#!/usr/bin/env bash
# Mutation Testing Batch Runner
# Generated: 2026-06-12
#
# Runs cargo-mutants on a specified batch of files and generates baseline JSON.
#
# Usage:
#   bash scripts/mutation/run_mutation_batch.sh <batch_id>          # Run single batch
#   bash scripts/mutation/run_mutation_batch.sh --all                # Run all batches
#   bash scripts/mutation/run_mutation_batch.sh --list               # List available batches
#   bash scripts/mutation/run_mutation_batch.sh --from <batch_id>    # Run from batch N onwards
#
# Environment variables:
#   TEST_THREADS        Number of test threads (default: 2)
#   MUTANT_TIMEOUT      Timeout per mutant in seconds (default: 30)
#   BASELINE_DIR        Output directory for baseline JSON (default: target/mutation-baseline)
#   DATABASE_URL        PostgreSQL connection string
#   TEST_REDIS_URL      Redis connection string (optional)

set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Source batch configuration (disable nounset for associative array compatibility)
set +u
source "$SCRIPT_DIR/batch_config.sh"
set -u

# ── Configuration ──
TEST_THREADS="${TEST_THREADS:-2}"
MUTANT_TIMEOUT="${MUTANT_TIMEOUT:-30}"
BASELINE_DIR="${BASELINE_DIR:-$PROJECT_DIR/target/mutation-baseline}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# ── Color output ──
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info()  { echo -e "${BLUE}[INFO]${NC}  $*"; }
log_ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }

# ── Resolve batch files by ID ──
get_batch_files() {
    local batch_id="$1"
    case "$batch_id" in
        batch_01_cache)              echo "${BATCH_01_CACHE[@]}" ;;
        batch_02_auth)               echo "${BATCH_02_AUTH[@]}" ;;
        batch_03_worker)             echo "${BATCH_03_WORKER[@]}" ;;
        batch_04_federation)         echo "${BATCH_04_FEDERATION[@]}" ;;
        batch_05_common_config)      echo "${BATCH_05_COMMON_CONFIG[@]}" ;;
        batch_06_common)             echo "${BATCH_06_COMMON_FILES[@]}" ;;
        batch_07_e2ee_backup_cross)  echo "${BATCH_07_E2EE_BACKUP_CROSS[@]}" ;;
        batch_08_e2ee_device_keys)   echo "${BATCH_08_E2EE_DEVICE_KEYS[@]}" ;;
        batch_09_e2ee_key_megolm)    echo "${BATCH_09_E2EE_KEY_MEGOLM[@]}" ;;
        batch_10_e2ee_remaining)     echo "${BATCH_10_E2EE_REMAINING[@]}" ;;
        batch_11_storage_event)      echo "${BATCH_11_STORAGE_EVENT[@]}" ;;
        batch_12_storage_media)      echo "${BATCH_12_STORAGE_MEDIA[@]}" ;;
        batch_13_storage_room)       echo "${BATCH_13_STORAGE_ROOM[@]}" ;;
        batch_14_storage_top_a)      echo "${BATCH_14_STORAGE_TOP_A[@]}" ;;
        batch_15_storage_top_b)      echo "${BATCH_15_STORAGE_TOP_B[@]}" ;;
        batch_16_services_auth_assemble) echo "${BATCH_16_SERVICES_AUTH_ASSEMBLE[@]}" ;;
        batch_17_services_room)      echo "${BATCH_17_SERVICES_ROOM[@]}" ;;
        batch_18_services_sync)      echo "${BATCH_18_SERVICES_SYNC[@]}" ;;
        batch_19_services_media_id)  echo "${BATCH_19_SERVICES_MEDIA_ID[@]}" ;;
        batch_20_services_push)      echo "${BATCH_20_SERVICES_PUSH[@]}" ;;
        batch_21_web_middleware)     echo "${BATCH_21_WEB_MIDDLEWARE[@]}" ;;
        batch_22_web_extractors)     echo "${BATCH_22_WEB_EXTRACTORS[@]}" ;;
        batch_23_web_routes_a)       echo "${BATCH_23_WEB_ROUTES_A[@]}" ;;
        batch_24_web_routes_b)       echo "${BATCH_24_WEB_ROUTES_B[@]}" ;;
        *)
            log_error "Unknown batch ID: $batch_id"
            log_info "Run with --list to see available batches"
            return 1
            ;;
    esac
}

# ── List all batches ──
list_batches() {
    echo "Available mutation testing batches:"
    echo "===================================="
    for batch_id in "${ALL_BATCH_IDS[@]}"; do
        local desc
        desc=$(get_batch_desc "$batch_id")
        local files
        files=($(get_batch_files "$batch_id" 2>/dev/null))
        local count=${#files[@]}
        printf "  ${GREEN}%-35s${NC} %s (%d files)\n" "$batch_id" "$desc" "$count"
    done
}

# ── Run mutation tests for a single batch ──
run_batch() {
    local batch_id="$1"
    local desc
    desc=$(get_batch_desc "$batch_id")
    local files
    files=($(get_batch_files "$batch_id"))

    if [ ${#files[@]} -eq 0 ]; then
        log_error "No files found for batch: $batch_id"
        return 1
    fi

    log_info "Batch: $batch_id — $desc"
    log_info "Files: ${#files[@]}"
    log_info "Threads: $TEST_THREADS, Timeout: ${MUTANT_TIMEOUT}s"
    echo ""

    local batch_dir="$BASELINE_DIR/$batch_id/$TIMESTAMP"
    mkdir -p "$batch_dir"

    local total_caught=0
    local total_missed=0
    local total_unviable=0
    local total_timeout=0
    local total_mutants=0
    local failed_files=()

    for file in "${files[@]}"; do
        if [ ! -f "$PROJECT_DIR/$file" ]; then
            log_warn "File not found, skipping: $file"
            continue
        fi

        local file_basename
        file_basename=$(basename "$file" .rs)
        local file_output_dir="$batch_dir/$file_basename"
        mkdir -p "$file_output_dir"

        log_info "  ==> $file"

        # Run cargo mutants
        local exit_code=0
        cd "$PROJECT_DIR"

        set +e
        cargo mutants --package synapse-rust \
            --file "$file" \
            --timeout "$MUTANT_TIMEOUT" \
            --baseline skip \
            -- --test-threads="$TEST_THREADS" \
            > "$file_output_dir/mutants_output.txt" 2>&1
        exit_code=$?
        set -e

        # Parse results from output
        if [ -f "$file_output_dir/mutants_output.txt" ]; then
            local output
            output=$(cat "$file_output_dir/mutants_output.txt")

            local caught missed unviable timeout
            caught=$(echo "$output" | grep -oP 'caught:\s*\K\d+' || echo "0")
            missed=$(echo "$output" | grep -oP 'missed:\s*\K\d+' || echo "0")
            unviable=$(echo "$output" | grep -oP 'unviable:\s*\K\d+' || echo "0")
            timeout=$(echo "$output" | grep -oP 'timeout:\s*\K\d+' || echo "0")
            # Fallback: try to parse from summary line
            if [ "$caught" = "0" ] && [ "$missed" = "0" ]; then
                caught=$(echo "$output" | grep -oP '(\d+)\s+caught' | grep -oP '\d+' || echo "0")
                missed=$(echo "$output" | grep -oP '(\d+)\s+missed' | grep -oP '\d+' || echo "0")
            fi

            total_caught=$((total_caught + caught))
            total_missed=$((total_missed + missed))
            total_unviable=$((total_unviable + unviable))
            total_timeout=$((total_timeout + timeout))

            local file_mutants=$((caught + missed + unviable + timeout))
            total_mutants=$((total_mutants + file_mutants))

            if [ "$exit_code" -ne 0 ]; then
                failed_files+=("$file (exit=$exit_code)")
                log_warn "    exit=$exit_code, caught=$caught, missed=$missed, unviable=$unviable, timeout=$timeout"
            else
                log_ok "    caught=$caught, missed=$missed, unviable=$unviable, timeout=$timeout"
            fi
        else
            log_error "    No output file generated for $file"
            failed_files+=("$file (no output)")
        fi
    done

    # ── Generate batch summary JSON ──
    local caught_pct=0
    if [ "$total_mutants" -gt 0 ]; then
        caught_pct=$(echo "scale=1; $total_caught * 100 / $total_mutants" | bc 2>/dev/null || echo "0")
    fi

    local summary_json="$batch_dir/summary.json"
    cat > "$summary_json" << JSONEOF
{
  "batch_id": "$batch_id",
  "description": "$desc",
  "timestamp": "$TIMESTAMP",
  "files_count": ${#files[@]},
  "failed_files": $(printf '%s\n' "${failed_files[@]}" | jq -R . | jq -s . 2>/dev/null || echo "[]"),
  "totals": {
    "mutants": $total_mutants,
    "caught": $total_caught,
    "missed": $total_missed,
    "unviable": $total_unviable,
    "timeout": $total_timeout,
    "caught_pct": $caught_pct
  },
  "files": $(printf '%s\n' "${files[@]}" | jq -R . | jq -s .)
}
JSONEOF

    echo ""
    log_info "Batch summary:"
    log_info "  Mutants:    $total_mutants"
    log_ok   "  Caught:     $total_caught ($caught_pct%)"
    log_warn "  Missed:     $total_missed"
    log_info "  Unviable:   $total_unviable"
    log_info "  Timeout:    $total_timeout"
    log_info "  Report:     $summary_json"

    if [ ${#failed_files[@]} -gt 0 ]; then
        echo ""
        log_warn "Failed files (${#failed_files[@]}):"
        for f in "${failed_files[@]}"; do
            log_warn "  - $f"
        done
    fi

    echo "$summary_json"
}

# ── Generate aggregate baseline ──
generate_aggregate_baseline() {
    local aggregate_file="$BASELINE_DIR/mutation-baseline.json"
    log_info "Generating aggregate baseline: $aggregate_file"

    # Collect all batch summaries
    local summaries=()
    while IFS= read -r -d '' f; do
        summaries+=("$f")
    done < <(find "$BASELINE_DIR" -name "summary.json" -print0 | sort -z)

    if [ ${#summaries[@]} -eq 0 ]; then
        log_warn "No batch summaries found to aggregate"
        return 0
    fi

    # Build aggregate JSON
    {
        echo '{'
        echo '  "generated": "'"$TIMESTAMP"'",'
        echo '  "total_batches": '"${#summaries[@]}"','
        echo '  "batches": ['
        local first=true
        for s in "${summaries[@]}"; do
            if [ "$first" = true ]; then
                first=false
            else
                echo ','
            fi
            echo -n '    '
            cat "$s"
        done
        echo ''
        echo '  ]'
        echo '}'
    } > "$aggregate_file"

    # Compute overall totals
    local total_m=0 total_c=0 total_ms=0
    for s in "${summaries[@]}"; do
        local m c ms
        m=$(jq '.totals.mutants // 0' "$s")
        c=$(jq '.totals.caught // 0' "$s")
        ms=$(jq '.totals.missed // 0' "$s")
        total_m=$((total_m + m))
        total_c=$((total_c + c))
        total_ms=$((total_ms + ms))
    done

    log_info "Aggregate totals:"
    log_info "  Total mutants:  $total_m"
    log_ok   "  Total caught:   $total_c"
    log_warn "  Total missed:   $total_ms"

    if [ "$total_m" -gt 0 ]; then
        local pct
        pct=$(echo "scale=1; $total_c * 100 / $total_m" | bc 2>/dev/null || echo "0")
        log_info "  Overall caught: ${pct}%"
    fi
}

# ── Main ──
main() {
    if [ $# -eq 0 ]; then
        echo "Usage: $0 <batch_id> | --all | --list | --from <batch_id> | --aggregate"
        echo ""
        list_batches
        exit 1
    fi

    case "$1" in
        --list|-l)
            list_batches
            ;;
        --all|-a)
            log_info "Running all ${#ALL_BATCH_IDS[@]} batches..."
            for bid in "${ALL_BATCH_IDS[@]}"; do
                run_batch "$bid" || log_error "Batch $bid failed, continuing..."
                echo ""
            done
            generate_aggregate_baseline
            ;;
        --from)
            if [ -z "${2:-}" ]; then
                log_error "--from requires a batch_id argument"
                exit 1
            fi
            local start_id="$2"
            local started=false
            for bid in "${ALL_BATCH_IDS[@]}"; do
                if [ "$started" = false ]; then
                    if [ "$bid" = "$start_id" ]; then
                        started=true
                    else
                        continue
                    fi
                fi
                run_batch "$bid" || log_error "Batch $bid failed, continuing..."
                echo ""
            done
            generate_aggregate_baseline
            ;;
        --aggregate)
            generate_aggregate_baseline
            ;;
        *)
            run_batch "$1"
            ;;
    esac
}

main "$@"