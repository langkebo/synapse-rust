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
DB_USER=""
DB_PASSWORD=""
DB_NAME=""
DB_HAS_RELATIONS=""

db_resolve_docker_creds() {
    if [ -n "$DB_USER" ] && [ -n "$DB_NAME" ]; then
        return 0
    fi
    local db_creds
    db_creds="$(
        python3 - <<'PY'
import os
from urllib.parse import urlparse
u = urlparse(os.environ["DATABASE_URL"])
print(u.username or "postgres")
print(u.password or "")
print(u.path.lstrip("/") or "postgres")
PY
    )"
    DB_USER="$(printf '%s\n' "$db_creds" | sed -n '1p')"
    DB_PASSWORD="$(printf '%s\n' "$db_creds" | sed -n '2p')"
    DB_NAME="$(printf '%s\n' "$db_creds" | sed -n '3p')"
}

db_has_public_relations() {
    local sql="SELECT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public' AND c.relkind IN ('r','m','p','v') AND c.relname NOT LIKE 'pg_%');"
    if command -v psql &> /dev/null; then
        psql "$DATABASE_URL" -tAc "$sql" 2>/dev/null | tr -d '[:space:]' | grep -q '^t$'
        return $?
    fi
    if [ -n "${PSQL_CONTAINER:-}" ] && command -v docker &> /dev/null; then
        db_resolve_docker_creds
        if [ -n "$DB_PASSWORD" ]; then
            docker exec -i -e PGPASSWORD="$DB_PASSWORD" "$PSQL_CONTAINER" \
                psql -U "$DB_USER" -d "$DB_NAME" -tAc "$sql" 2>/dev/null | tr -d '[:space:]' | grep -q '^t$'
            return $?
        fi
        docker exec -i "$PSQL_CONTAINER" \
            psql -U "$DB_USER" -d "$DB_NAME" -tAc "$sql" 2>/dev/null | tr -d '[:space:]' | grep -q '^t$'
        return $?
    fi
    return 2
}

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
TABLE_COVERAGE_JSON="$REPORTS_DIR/table_coverage_$TIMESTAMP.json"
run_check \
    "Table Coverage" \
    "python3 '$SCRIPT_DIR/check_schema_table_coverage.py' --json-report '$TABLE_COVERAGE_JSON'" \
    "Verify all expected tables are defined in migrations"

# 2. Contract Coverage Check
COVERAGE_REPORT="$REPORTS_DIR/contract_coverage_$TIMESTAMP.md"
COVERAGE_JSON="$REPORTS_DIR/contract_coverage_$TIMESTAMP.json"
run_check \
    "Contract Coverage" \
    "python3 '$SCRIPT_DIR/check_schema_contract_coverage.py' --threshold 90 --report '$COVERAGE_REPORT' --json-report '$COVERAGE_JSON'" \
    "Verify schema contracts meet 90% coverage threshold"

# 3. Migration Layout Audit
LAYOUT_AUDIT_JSON="$REPORTS_DIR/migration_layout_audit_$TIMESTAMP.json"
run_check \
    "Migration Layout" \
    "python3 '$SCRIPT_DIR/audit_migration_layout.py' --report '$LAYOUT_AUDIT_JSON'" \
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
if [ -n "${DATABASE_URL:-}" ]; then
    if command -v psql &> /dev/null; then
        run_check \
            "Database Connection" \
            "psql '$DATABASE_URL' -c 'SELECT 1' > /dev/null 2>&1" \
            "Verify database connectivity"
    elif [ -n "${PSQL_CONTAINER:-}" ] && command -v docker &> /dev/null; then
        db_resolve_docker_creds
        if [ -n "$DB_PASSWORD" ]; then
            run_check \
                "Database Connection" \
                "docker exec -i -e PGPASSWORD='$DB_PASSWORD' '$PSQL_CONTAINER' psql -U '$DB_USER' -d '$DB_NAME' -v ON_ERROR_STOP=1 -c 'SELECT 1' > /dev/null 2>&1" \
                "Verify database connectivity (via docker exec)"
        else
            run_check \
                "Database Connection" \
                "docker exec -i '$PSQL_CONTAINER' psql -U '$DB_USER' -d '$DB_NAME' -v ON_ERROR_STOP=1 -c 'SELECT 1' > /dev/null 2>&1" \
                "Verify database connectivity (via docker exec)"
        fi
    else
        echo -e "${YELLOW}⚠ psql not available and PSQL_CONTAINER not set, skipping database checks${NC}"
        echo "- ⚠️ **Database Checks**: SKIPPED (psql not installed and PSQL_CONTAINER not set)" >> "$SUMMARY_REPORT"
        echo "" >> "$SUMMARY_REPORT"
    fi

    if db_has_public_relations; then
        DB_HAS_RELATIONS="1"
    else
        DB_HAS_RELATIONS="0"
        echo -e "${YELLOW}⚠ public schema has no relations, skipping amcheck/logical checksum${NC}"
        echo "- ⚠️ **DB Content**: SKIPPED (no relations in public schema; run migrations first)" >> "$SUMMARY_REPORT"
        echo "" >> "$SUMMARY_REPORT"
    fi

    # 6. pg_amcheck (if database is available)
    if [ "$DB_HAS_RELATIONS" = "1" ] && command -v pg_amcheck &> /dev/null; then
        run_check \
            "Physical Integrity (pg_amcheck)" \
            "python3 '$SCRIPT_DIR/run_pg_amcheck.py'" \
            "Run PostgreSQL physical integrity checks"
    elif [ "$DB_HAS_RELATIONS" = "1" ] && [ -n "${PSQL_CONTAINER:-}" ] && command -v docker &> /dev/null; then
        run_check \
            "Physical Integrity (pg_amcheck)" \
            "PG_AMCHECK_CONTAINER='$PSQL_CONTAINER' python3 '$SCRIPT_DIR/run_pg_amcheck.py'" \
            "Run PostgreSQL physical integrity checks (via docker exec)"
    else
        if [ "$DB_HAS_RELATIONS" != "1" ]; then
            :
        else
            echo -e "${YELLOW}⚠ pg_amcheck not available and PSQL_CONTAINER not set, skipping${NC}"
            echo "- ⚠️ **Physical Integrity**: SKIPPED (pg_amcheck not installed and PSQL_CONTAINER not set)" >> "$SUMMARY_REPORT"
            echo "" >> "$SUMMARY_REPORT"
        fi
    fi

    # 7. Logical Checksum
    CHECKSUM_REPORT="$REPORTS_DIR/logical_checksum_$TIMESTAMP.json"
    if [ "$DB_HAS_RELATIONS" = "1" ] && command -v psql &> /dev/null; then
        run_check \
            "Logical Checksum" \
            "python3 '$SCRIPT_DIR/generate_logical_checksum_report.py' --output '$CHECKSUM_REPORT'" \
            "Generate logical schema checksum"
    elif [ "$DB_HAS_RELATIONS" = "1" ] && [ -n "${PSQL_CONTAINER:-}" ] && command -v docker &> /dev/null; then
        run_check \
            "Logical Checksum" \
            "PSQL_CONTAINER='$PSQL_CONTAINER' python3 '$SCRIPT_DIR/generate_logical_checksum_report.py' --output '$CHECKSUM_REPORT'" \
            "Generate logical schema checksum (via docker exec)"
    else
        if [ "$DB_HAS_RELATIONS" != "1" ]; then
            :
        else
            echo -e "${YELLOW}⚠ psql not available and PSQL_CONTAINER not set, skipping logical checksum${NC}"
            echo "- ⚠️ **Logical Checksum**: SKIPPED (psql not installed and PSQL_CONTAINER not set)" >> "$SUMMARY_REPORT"
            echo "" >> "$SUMMARY_REPORT"
        fi
    fi
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
for report in "$REPORTS_DIR"/*_"$TIMESTAMP".md "$REPORTS_DIR"/*_"$TIMESTAMP".json; do
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
