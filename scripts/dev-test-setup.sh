#!/bin/bash
set -euo pipefail
CONTAINER_NAME="synapse-test-db"
# 宿主发布端口。**不要用 5432**：dev/deploy 栈的 `synapse-postgres` 已占用
# 127.0.0.1:5432，测试库若也硬编码 5432，迁移会打到应用库上（见
# BACKEND_OPEN_ISSUES_HANDOFF_2026-10-18.md §1.3）。
TEST_DB_PORT="${TEST_DB_PORT:-55432}"

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
            # 已有容器可能由更早的手工命令发布在别的端口上：以**实际映射**为准，
            # 否则下面会把迁移打到 `localhost:$TEST_DB_PORT` 上另一个实例。
            mapped_port="$(docker port "$CONTAINER_NAME" 5432/tcp 2>/dev/null | head -n 1 | awk -F: '{print $NF}')"
            if [ -n "$mapped_port" ]; then
                TEST_DB_PORT="$mapped_port"
                echo "Using existing container port mapping: $TEST_DB_PORT"
            fi
        else
            docker rm -f "$CONTAINER_NAME" 2>/dev/null || true
            docker run -d --name "$CONTAINER_NAME" \
                -e POSTGRES_USER=synapse \
                -e POSTGRES_PASSWORD=synapse \
                -e POSTGRES_DB=synapse_test \
                -p "127.0.0.1:${TEST_DB_PORT}:5432" \
                postgres:16
            echo "Waiting for PostgreSQL..."
            until docker exec "$CONTAINER_NAME" pg_isready -U synapse >/dev/null 2>&1; do
                sleep 1
            done
            echo "PostgreSQL ready."
        fi

        TEST_DATABASE_URL="postgres://synapse:synapse@localhost:${TEST_DB_PORT}/synapse_test"

        echo "Running migrations..."
        # 显式给出目标：容器声明的是 POSTGRES_DB=synapse_test，宿主端口见
        # TEST_DB_PORT。不显式指定就会落到 `docker/.env` 兜底的 DB_NAME=`synapse`
        # （并在容器里凭空建库）。这也让 H-14 护栏
        # （docker/db_migrate.sh：不显式给 DATABASE_URL 就拒绝宿主 psql 打 loopback）
        # 不会误伤这条正常的开发路径。
        #
        # 口令用 `synapse` 是**对的**：本脚本面向 **dev 栈**（docker/docker-compose.yml），
        # 它的口令来源是 `docker/.env` 的 `DB_PASSWORD`。**deploy 栈**的口令不同
        # （部署时随机生成），要连 deploy 栈请用 scripts/lib/db_credentials.sh。
        DATABASE_URL="$TEST_DATABASE_URL" \
            bash docker/db_migrate.sh migrate

        # 迁移后校验：确认**这个容器**的 synapse_test.public 真的有表。
        # 只信 db_migrate.sh 的退出码不够 —— 它打到别的实例时同样是 0。
        table_count="$(docker exec "$CONTAINER_NAME" \
            psql -U synapse -d synapse_test -tAc \
            "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'public'" \
            2>/dev/null | tr -d '[:space:]')"
        if ! [[ "${table_count:-}" =~ ^[0-9]+$ ]] || [ "$table_count" -lt 1 ]; then
            echo "ERROR: ${CONTAINER_NAME}:synapse_test.public has no tables (count=${table_count:-<unknown>})." >&2
            echo "       Migrations did not reach the test container; check DATABASE_URL ($TEST_DATABASE_URL)." >&2
            exit 1
        fi
        echo "Verified: ${table_count} tables in ${CONTAINER_NAME}:synapse_test.public"

        echo ""
        echo "=== Test environment ready ==="
        echo "Run:"
        echo "  export TEST_DATABASE_URL=$TEST_DATABASE_URL"
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
