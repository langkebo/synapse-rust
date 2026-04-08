#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

MODE="${1:-check}"

echo "=========================================="
echo "Pre-Refactor Schema Guard"
echo "=========================================="
echo "Mode: $MODE"
echo ""

if [[ "$MODE" == "repair" ]]; then
    echo "[1/3] Apply managed migrations"
    bash docker/db_migrate.sh migrate
else
    echo "[1/3] Skip migration apply (check mode)"
fi

echo "[2/3] Validate core/critical schema"
bash docker/db_migrate.sh validate

echo "[3/3] Show latest applied migrations"
bash docker/db_migrate.sh status | head -n 40

echo ""
echo "Schema guard passed."
