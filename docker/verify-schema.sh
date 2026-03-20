#!/bin/bash
set -e

echo "=== Schema Verification ==="

DATABASE_URL="${DATABASE_URL:-postgres://synapse:synapse@db:5432/synapse_test}"

# 等待数据库就绪
for i in $(seq 1 $DB_WAIT_ATTEMPTS); do
    if pg_isready -h db -U synapse -d synapse_test > /dev/null 2>&1; then
        echo "Database is ready"
        break
    fi
    echo "Waiting for database... ($i/$DB_WAIT_ATTEMPTS)"
    sleep $DB_WAIT_INTERVAL
done

# 验证关键表存在
echo "Verifying database schema..."

TABLES=$(psql "$DATABASE_URL" -t -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_name IN ('users', 'devices', 'rooms', 'events', 'access_tokens', 'refresh_tokens')")

if [ "$TABLES" -lt 6 ]; then
    echo "ERROR: Missing required tables"
    exit 1
fi

echo "Schema verification completed successfully"
exit 0
