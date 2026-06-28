#!/bin/bash
set -euo pipefail
CONTAINER_NAME="synapse-test-db"

usage() {
  echo "Usage: $0 {up|down}"
  echo "  up    Start PostgreSQL test DB, migrate, print connection env"
  echo "  down  Stop and remove the test DB container"
  exit 1
}

case "${1:-up}" in
  up)
    if docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
      echo "Container $CONTAINER_NAME is already running."
    else
      docker rm -f "$CONTAINER_NAME" 2>/dev/null || true
      docker run -d --name "$CONTAINER_NAME" \
        -e POSTGRES_USER=synapse \
        -e POSTGRES_PASSWORD=synapse \
        -e POSTGRES_DB=synapse_test \
        -p 5432:5432 \
        postgres:16
      echo "Waiting for PostgreSQL..."
      until docker exec "$CONTAINER_NAME" pg_isready -U synapse >/dev/null 2>&1; do
        sleep 1
      done
      echo "PostgreSQL ready."
    fi

    echo "Running migrations..."
    bash docker/db_migrate.sh migrate

    echo ""
    echo "=== Test environment ready ==="
    echo "Run:"
    echo "  export TEST_DB_TEMPLATE_SCHEMA=public"
    echo "  SQLX_OFFLINE=true cargo test --features test-utils --test integration -- --test-threads=2"
    echo ""
    echo "For a single test:"
    echo "  SQLX_OFFLINE=true cargo test --features test-utils --test integration <test_name> -- --exact --nocapture"
    ;;
  down)
    docker rm -f "$CONTAINER_NAME" 2>/dev/null || true
    echo "Test DB container removed."
    ;;
  *)
    usage
    ;;
esac
