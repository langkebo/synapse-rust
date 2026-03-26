#!/bin/bash
# PostgreSQL initialization script
# This script runs during database initialization

set -e

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<-EOSQL
    -- Create synapse user with CREATEDB privilege
    CREATE USER synapse WITH PASSWORD 'synapse' CREATEDB;
    
    -- Create synapse_test database
    CREATE DATABASE synapse_test OWNER synapse;
    
    -- Grant privileges
    GRANT ALL PRIVILEGES ON DATABASE synapse_test TO synapse;
    
    -- Create schema_migrations table
    CREATE TABLE IF NOT EXISTS schema_migrations (
        version VARCHAR(255) PRIMARY KEY,
        applied_at BIGINT NOT NULL
    );
    
    -- Grant on schema
    GRANT ALL ON SCHEMA public TO synapse;
EOSQL

echo "PostgreSQL initialization completed"
