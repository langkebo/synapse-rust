#!/bin/bash

# Database migration script for synapse-rust
# This script applies all migration files in order

set -e

DATABASE_URL="${DATABASE_URL:-postgres://synapse:synapse@localhost:5432/synapse}"
MIGRATIONS_DIR="$(dirname "$0")/../migrations"

echo "🚀 Starting database migrations..."
echo "📦 Database: synapse_rust"
echo "🔗 Connection: $DATABASE_URL"
echo ""

# Check if psql is available
if ! command -v psql &> /dev/null; then
    echo "❌ Error: psql command not found"
    echo ""
    echo "Please install PostgreSQL client tools:"
    echo "  - macOS: brew install postgresql"
    echo "  - Ubuntu: sudo apt-get install postgresql-client"
    echo "  - Or use: docker exec -i <postgres_container> psql..."
    exit 1
fi

# Function to apply a single migration
apply_migration() {
    local migration_file=$1
    local filename=$(basename "$migration_file")

    echo "📄 Applying: $filename"
    if psql "$DATABASE_URL" -f "$migration_file" > /tmp/migration_output.log 2>&1; then
        echo "✅ Successfully applied: $filename"
        return 0
    else
        # Check if it's just a warning (table already exists)
        if grep -qi "already exists" /tmp/migration_output.log; then
            echo "⚠️  Migration applied (some objects already exist): $filename"
            return 0
        else
            echo "❌ Failed to apply: $filename"
            cat /tmp/migration_output.log
            return 1
        fi
    fi
}

echo "📋 Migration files found:"
ls -1 "$MIGRATIONS_DIR"/*.sql 2>/dev/null | sort | while read -r file; do
    echo "   - $(basename "$file")"
done
echo ""

# Apply migrations in sorted order (by filename which includes timestamp)
SUCCESS_COUNT=0
TOTAL_COUNT=0

for migration_file in $(ls -1 "$MIGRATIONS_DIR"/*.sql 2>/dev/null | sort); do
    TOTAL_COUNT=$((TOTAL_COUNT + 1))

    if apply_migration "$migration_file"; then
        SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
    else
        echo "⚠️  Continuing with remaining migrations..."
    fi
done

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "📊 Migration Summary"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Total migrations: $TOTAL_COUNT"
echo "Successfully applied: $SUCCESS_COUNT"
echo "Failed: $((TOTAL_COUNT - SUCCESS_COUNT))"
echo ""

if [ $SUCCESS_COUNT -eq $TOTAL_COUNT ]; then
    echo "🎉 All migrations applied successfully!"
    echo ""
    echo "To verify, you can check the tables with:"
    echo "  psql '$DATABASE_URL' -c \"\\dt\""
    echo ""
    echo "Or check indexes with:"
    echo "  psql '$DATABASE_URL' -c \"\\di\""
    exit 0
else
    echo "⚠️  Some migrations had issues. Please review the errors above."
    exit 1
fi
