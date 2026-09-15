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
        # 显式给出目标：容器声明的是 POSTGRES_DB=synapse_test 并发布了 5432，
        # 而 docker/.env 兜底的 DB_NAME 是 `synapse` —— 不显式指定就会把迁移打到
        # 另一个库名上（并在容器里凭空建库）。这也让 H-14 护栏
        # （docker/db_migrate.sh：不显式给 DATABASE_URL 就拒绝宿主 psql 打 loopback）
        # 不会误伤这条正常的开发路径。
        DATABASE_URL="postgres://synapse:synapse@localhost:5432/synapse_test" \
            bash docker/db_migrate.sh migrate

        echo ""
        echo "=== Test environment ready ==="
        echo "Run:"
        echo "  export TEST_DATABASE_URL=postgres://synapse:synapse@localhost:5432/synapse_test"
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
