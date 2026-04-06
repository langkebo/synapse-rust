#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SQLX_SOURCE_DIR="${SQLX_SOURCE_DIR:-$PROJECT_ROOT/artifacts/sqlx-migrations-local}"

python3 "$PROJECT_ROOT/scripts/build_sqlx_migration_source.py" "$SQLX_SOURCE_DIR"

exec sqlx migrate "$@" --source "$SQLX_SOURCE_DIR"
