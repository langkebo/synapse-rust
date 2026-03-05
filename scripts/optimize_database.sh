#!/bin/bash
# Database Performance Optimization Script
# This script applies performance indexes and optimizations

set -e

DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-5432}"
DB_NAME="${DB_NAME:-synapse}"
DB_USER="${DB_USER:-synapse}"

echo "========================================"
echo "Database Performance Optimization"
echo "========================================"
echo "Database: $DB_NAME on $DB_HOST:$DB_PORT"
echo ""

MIGRATION_FILE="/home/tzd/synapse-rust/migrations/20260227_security_enhancements.sql"

if [ ! -f "$MIGRATION_FILE" ]; then
    echo "ERROR: Migration file not found: $MIGRATION_FILE"
    exit 1
fi

echo "Applying database optimizations..."
echo ""

# Apply migration
PGPASSWORD="${DB_PASSWORD:-synapse_password}" psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -f "$MIGRATION_FILE"

echo ""
echo "========================================"
echo "Optimization Complete"
echo "========================================"
echo ""
echo "Applied optimizations:"
echo "  - Token blacklist table created"
echo "  - Access token validity tracking"
echo "  - Performance indexes added"
echo "  - Cleanup functions installed"
echo ""
echo "To verify indexes, run:"
echo "  psql -h $DB_HOST -U $DB_USER -d $DB_NAME -c \"\\di\""
