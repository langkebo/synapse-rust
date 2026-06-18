#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DOC_DIR="$ROOT_DIR/docs/synapse-rust"
PR_TEMPLATE="$ROOT_DIR/.github/pull_request_template.md"
STRICT_WARNINGS="${STRICT_WARNINGS:-0}"

usage() {
    cat <<'EOF'
Usage:
  bash scripts/ci/check_release_doc_spotcheck.sh
  bash scripts/ci/check_release_doc_spotcheck.sh --strict
  STRICT_WARNINGS=1 bash scripts/ci/check_release_doc_spotcheck.sh

Options:
  --strict   Treat advisory warnings as failures.
  --help     Show this help message.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --strict)
            STRICT_WARNINGS=1
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            printf 'Unknown argument: %s\n\n' "$1" >&2
            usage >&2
            exit 2
            ;;
    esac
    shift
done

PASS_COUNT=0
FAIL_COUNT=0
WARN_COUNT=0

pass() {
    printf 'PASS %s\n' "$1"
    PASS_COUNT=$((PASS_COUNT + 1))
}

fail() {
    printf 'FAIL %s\n' "$1"
    FAIL_COUNT=$((FAIL_COUNT + 1))
}

warn() {
    printf 'WARN %s\n' "$1"
    WARN_COUNT=$((WARN_COUNT + 1))
}

require_file() {
    local file_path="$1"
    if [[ -f "$file_path" ]]; then
        pass "exists: ${file_path#$ROOT_DIR/}"
        return 0
    fi

    fail "missing: ${file_path#$ROOT_DIR/}"
    return 1
}

require_pattern() {
    local file_path="$1"
    local pattern="$2"
    local description="$3"
    if grep -Eq "$pattern" "$file_path"; then
        pass "${description}: ${file_path#$ROOT_DIR/}"
        return 0
    fi

    fail "${description}: ${file_path#$ROOT_DIR/}"
    return 1
}

check_doc_metadata() {
    local file_path="$1"
    local freshness_pattern="$2"
    local evidence_pattern="$3"

    require_file "$file_path" || return 0
    require_pattern "$file_path" "$freshness_pattern" "freshness marker present"
    require_pattern "$file_path" "$evidence_pattern" "baseline/evidence marker present"
}

advisory_pattern() {
    local file_path="$1"
    local pattern="$2"
    local description="$3"
    if grep -Eq "$pattern" "$file_path"; then
        warn "${description}: ${file_path#$ROOT_DIR/}"
    else
        pass "${description}: ${file_path#$ROOT_DIR/}"
    fi
}

printf '=== release doc spot-check ===\n'
printf 'mode: STRICT_WARNINGS=%s\n' "$STRICT_WARNINGS"

check_doc_metadata \
    "$DOC_DIR/COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md" \
    '最后验证时间' \
    '证据来源'

check_doc_metadata \
    "$DOC_DIR/SUPPORTED_MATRIX_SURFACE.md" \
    '审查日期|最后更新|日期' \
    '基线:|治理规则:|当前 .* 生成'

check_doc_metadata \
    "$DOC_DIR/TECHNICAL_DEBT_OPTIMIZATION_PLAN_2026-06-11.md" \
    '日期:|最后更新' \
    '基于:|参考基准:'

check_doc_metadata \
    "$DOC_DIR/LAYER_MIGRATION_OPTIMIZATION_PLAN_2026-06-12.md" \
    '日期:|最后更新' \
    '审查范围:|参考基准:|状态:'

check_doc_metadata \
    "$DOC_DIR/WORKER_TOPOLOGY_BASELINE_2026-06-14.md" \
    '日期:|最后更新' \
    '对应代码:|当前仓库已提供'

check_doc_metadata \
    "$DOC_DIR/M3_PROGRESS.md" \
    '最后更新:|日期:' \
    '当前策略|CI 门禁'

advisory_pattern \
    "$DOC_DIR/TECHNICAL_DEBT_OPTIMIZATION_PLAN_2026-06-11.md" \
    '仅 4 行|已退化为对 .*route_ledger.*re-export' \
    'advisory: technical debt doc should not retain known-stale route_ledger wording'

advisory_pattern \
    "$DOC_DIR/TECHNICAL_DEBT_OPTIMIZATION_PLAN_2026-06-11.md" \
    'thin re-export.*route_ledger 保持一致' \
    'advisory: technical debt doc should be reviewed for outdated facade comparisons'

require_file "$PR_TEMPLATE" || true
require_pattern "$PR_TEMPLATE" '### 文档状态同步' 'pr template includes doc sync checklist'
require_pattern \
    "$PR_TEMPLATE" \
    'check_release_doc_spotcheck\.sh|文档状态同步' \
    'pr template references release doc spot-check flow'

printf '\nSummary: PASS=%d WARN=%d FAIL=%d\n' "$PASS_COUNT" "$WARN_COUNT" "$FAIL_COUNT"

if [[ "$FAIL_COUNT" -gt 0 ]]; then
    printf 'Next step: fix missing freshness/baseline markers or broken PR checklist references, then rerun the spot-check.\n'
fi

if [[ "$WARN_COUNT" -gt 0 ]]; then
    printf 'Next step: review advisory wording drift in the flagged current docs; use --strict (or STRICT_WARNINGS=1) when warnings must block release.\n'
fi

if [[ "$STRICT_WARNINGS" = "1" && "$WARN_COUNT" -gt 0 ]]; then
    printf 'Strict mode enabled: treating warnings as failures.\n'
    exit 1
fi

if [[ "$FAIL_COUNT" -gt 0 ]]; then
    exit 1
fi
