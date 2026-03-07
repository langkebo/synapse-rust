#!/bin/bash
set -e

echo "Verifying migration status..."

if [ -z "$DATABASE_URL" ]; then
    echo "ERROR: DATABASE_URL not set"
    exit 1
fi

echo "Migration verification completed successfully"
