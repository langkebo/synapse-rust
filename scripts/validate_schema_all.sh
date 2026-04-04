#!/bin/bash
# Comprehensive Schema Validation Runner
# Runs all schema validation checks in sequence
# Updated: 2026-04-04

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
REPORTS_DIR="$PROJECT_ROOT/artifacts/schema_validation"

# Create reports directory
mkdir -p "$REPORTS_DIR"

TIMESTAMP=$(date +%Y%m%d_%H%M%S)
SUMMARY_REPORT="$REPORTS_DIR/validation_summary_$TIMESTAMP.md"

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  Schema Validation Suite${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# Initialize summary report
cat > "$SUMMARY_REPORT" <<EOF
# Schema Validation Summary

> Generated: $(date +%Y-%m-%d\ %H:%M:%S)
> Database: ${DATABASE_URL:-postgresql://synapse:synapse@localhost:5432/synapse_test}

## Validation Results

EOF

# Track overall status
OVERALL_STATUS=0

# Function to run a validation check
run_check() {
    local check_name=$1
    local check_command=$2
    local check_description=$3

    echo -e "${YELLOW}Running: $check_name${NC}"
    echo "  $check_description"
    echo ""

    if eval "$check_command"; then
        echo -e "${GREEN}✓ $check_name passed${NC}"
        echo "- ✅ **$check_name**: PASSED" >> "$SUMMARY_REPORT"
        echo "" >> "$SUMMARY_REPORT"
        return 0
    else
        echo -e "${RED}✗ $check_name failed${NC}"
        echo "- ❌ **$check_name**: FAILED" >> "$SUMMARY_REPORT"
        echo "" >> "$SUMMARY_REPORT"
        OVERALL_STATUS=1
        return 1
    fi
}

# 1. Table Coverage Check
run_check \
    "Table Coverage" \
    "python3 '$SCRIPT_DIR/check_schema_table_coverage.py'" \
    "Verify all expected tables are defined in migrations"

# 2. Contract Coverage Check
COVERAGE_REPORT="$REPORTS_DIR/contract_coverage_$TIMESTAMP.md"
run_check \
    "Contract Coverage" \
    "python3 '$SCRIPT_DIR/check_schema_contract_coverage.py' --threshold 90 --report '$COVERAGE_REPORT'" \
    "Verify schema contracts meet 90% coverage threshold"

# 3. Migration Layout Audit
run_check \
    "Migration Layout" \
    "python3 '$SCRIPT_DIR/audit_migration_layout.py'" \
    "Check for duplicate definitions and migration conflicts"

# 4. Migration Manifest Verification (if manifest exists)
if [ -f "$PROJECT_ROOT/artifacts/MANIFEST-ci.txt" ]; then
    run_check \
        "Migration Manifest" \
        "python3 '$SCRIPT_DIR/verify_migration_manifest.py' '$PROJECT_ROOT/artifacts/MANIFEST-ci.txt'" \
        "Verify migration manifest integrity"
else
    echo -e "${YELLOW}⚠ Migration manifest not found, skipping verification${NC}"
    echo "- ⚠️ **Migration Manifest**: SKIPPED (no manifest file)" >> "$SUMMARY_REPORT"
    echo "" >> "$SUMMARY_REPORT"
fi

# 5. Database Connection Check (if DATABASE_URL is set)
if [ -n "$DATABASE_URL" ]; then
    run_check \
        "Database Connection" \
        "psql '$DATABASE_URL' -c 'SELECT 1' > /dev/null 2>&1" \
        "Verify database connectivity"

    # 6. pg_amcheck (if database is available)
    if command -v pg_amcheck &> /dev/null; then
        run_check \
            "Physical Integrity (pg_amcheck)" \
            "python3 '$SCRIPT_DIR/run_pg_amcheck.py'" \
            "Run PostgreSQL physical integrity checks"
    else
        echo -e "${YELLOW}⚠ pg_amcheck not available, skipping${NC}"
        echo "- ⚠️ **Physical Integrity**: SKIPPED (pg_amcheck not installed)" >> "$SUMMARY_REPORT"
        echo "" >> "$SUMMARY_REPORT"
    fi

    # 7. Logical Checksum
    CHECKSUM_REPORT="$REPORTS_DIR/logical_checksum_$TIMESTAMP.md"
    run_check \
        "Logical Checksum" \
        "python3 '$SCRIPT_DIR/generate_logical_checksum_report.py' --output '$CHECKSUM_REPORT'" \
        "Generate logical schema checksum"
else
    echo -e "${YELLOW}⚠ DATABASE_URL not set, skipping database checks${NC}"
    echo "- ⚠️ **Database Checks**: SKIPPED (DATABASE_URL not set)" >> "$SUMMARY_REPORT"
    echo "" >> "$SUMMARY_REPORT"
fi

# Finalize summary report
cat >> "$SUMMARY_REPORT" <<EOF

## Summary

- **Overall Status**: $([ $OVERALL_STATUS -eq 0 ] && echo "✅ PASSED" || echo "❌ FAILED")
- **Reports Directory**: $REPORTS_DIR

## Generated Reports

EOF

# List all generated reports
for report in "$REPORTS_DIR"/*_$TIMESTAMP.md; do
    if [ -f "$report" ]; then
        echo "- $(basename "$report")" >> "$SUMMARY_REPORT"
    fi
done

cat >> "$SUMMARY_REPORT" <<EOF

---

**Validation completed**: $(date +%Y-%m-%d\ %H:%M:%S)
EOF

echo ""
echo -e "${BLUE}========================================${NC}"
if [ $OVERALL_STATUS -eq 0 ]; then
    echo -e "${GREEN}✓ All validation checks passed${NC}"
else
    echo -e "${RED}✗ Some validation checks failed${NC}"
fi
echo -e "${BLUE}========================================${NC}"
echo ""
echo -e "Summary report: ${BLUE}$SUMMARY_REPORT${NC}"
echo ""

exit $OVERALL_STATUS
