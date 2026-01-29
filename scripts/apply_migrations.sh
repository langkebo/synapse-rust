#!/bin/bash

# Database migration script for synapse-rust
# This script applies all migration files in order

set -e

DATABASE_URL="postgres://synapse:synapse@localhost:5432/synapse"

echo "🚀 Starting database migrations..."
echo "📦 Database: synapse_rust"
echo ""

# Migration order (must be applied in this order to respect foreign keys)
MIGRATIONS=(
    "migrations/000_create_core_tables.sql"
    "migrations/001_create_device_keys.sql"
    "migrations/002_create_cross_signing_keys.sql"
    "migrations/004_create_key_backups.sql"
    "migrations/005_create_event_signatures.sql"
    "migrations/006_create_auth_and_room_tables.sql"
    "migrations/007_create_enhanced_tables.sql"
)

for migration in "${MIGRATIONS[@]}"; do
    if [ -f "$migration" ]; then
        echo "📄 Applying: $migration"
        psql "$DATABASE_URL" -f "$migration" > /dev/null 2>&1 && \
            echo "✅ Successfully applied: $migration" || \
            echo "⚠️  Migration may have warnings (table may already exist): $migration"
    else
        echo "❌ Migration file not found: $migration"
    fi
done

echo ""
echo "🎉 Database migration completed!"
echo ""
echo "To verify, you can check the tables with:"
echo "  psql '$DATABASE_URL' -c \"\\dt\""
