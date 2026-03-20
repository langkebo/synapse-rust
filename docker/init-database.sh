#!/bin/bash
# ============================================================================
# Database Initialization Script
# Purpose: Initialize database with all required tables and schema
# ============================================================================

set -e

echo "=========================================="
echo "Database Initialization Script"
echo "=========================================="

# Configuration
DB_HOST="${DB_HOST:-db}"
DB_PORT="${DB_PORT:-5432}"
DB_NAME="${DB_NAME:-synapse_test}"
DB_USER="${DB_USER:-synapse}"
DB_PASSWORD="${DB_PASSWORD:-synapse}"
MIGRATIONS_DIR="${MIGRATIONS_DIR:-/app/migrations}"

# Wait for database to be ready
echo ""
echo "Waiting for database to be ready..."
until PGPASSWORD=$DB_PASSWORD psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -c '\q' 2>/dev/null; do
    echo "Database is unavailable - sleeping"
    sleep 2
done
echo "Database is ready!"

# Create migrations table if not exists
echo ""
echo "Creating migrations table..."
PGPASSWORD=$DB_PASSWORD psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" << 'EOSQL'
CREATE TABLE IF NOT EXISTS migrations (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) UNIQUE NOT NULL,
    applied_at TIMESTAMP DEFAULT NOW()
);
EOSQL

# Run all migrations in order
echo ""
echo "Running database migrations..."
echo "Migrations directory: $MIGRATIONS_DIR"

# Get list of migration files sorted by name
MIGRATION_FILES=$(ls -1 "$MIGRATIONS_DIR"/*.sql 2>/dev/null | sort)

for migration_file in $MIGRATION_FILES; do
    migration_name=$(basename "$migration_file" .sql)
    
    # Check if migration already applied
    applied=$(PGPASSWORD=$DB_PASSWORD psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -t -c \
        "SELECT COUNT(*) FROM migrations WHERE name = '$migration_name';" | tr -d ' ')
    
    if [ "$applied" -eq 0 ]; then
        echo ""
        echo "Applying migration: $migration_name"
        PGPASSWORD=$DB_PASSWORD psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -f "$migration_file"
        
        # Record migration
        PGPASSWORD=$DB_PASSWORD psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -c \
            "INSERT INTO migrations (name) VALUES ('$migration_name') ON CONFLICT DO NOTHING;"
        
        echo "Migration $migration_name applied successfully"
    else
        echo "Migration $migration_name already applied, skipping"
    fi
done

echo ""
echo "=========================================="
echo "Database initialization completed!"
echo "=========================================="

# Verify tables
echo ""
echo "Verifying database tables..."
PGPASSWORD=$DB_PASSWORD psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -c "\dt"

echo ""
echo "Database initialization script finished."
