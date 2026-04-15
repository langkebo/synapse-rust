#!/bin/bash

set -eEuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DOCKER_DIR="$ROOT_DIR/docker"
MODE="${1:-all}"
TEMP_ENV_CREATED=false
IMAGE_BUILT=false
SERVICES_STARTED=false
COMPOSE_FILES=(-f docker-compose.yml -f docker-compose.dev-host-access.yml)

log() {
    printf '[ci] %s\n' "$*"
}

docker_compose() {
    (
        cd "$DOCKER_DIR"
        docker compose "${COMPOSE_FILES[@]}" "$@"
    )
}

ensure_docker_env() {
    if [ -f "$DOCKER_DIR/.env" ]; then
        return 0
    fi

    cat >"$DOCKER_DIR/.env" <<'EOF'
COMPOSE_PROJECT_NAME=synapse-ci
SYNAPSE_IMAGE=synapse-rust
SYNAPSE_IMAGE_TAG=latest
POSTGRES_VERSION=16
REDIS_VERSION=7
TZ=UTC
RUST_LOG=info
SYNAPSE_PORT=28008
FEDERATION_PORT=28448
DB_EXPOSE_PORT=5432
REDIS_EXPOSE_PORT=6379
SERVER_NAME=localhost
PUBLIC_BASEURL=http://localhost:28008
ALLOWED_ORIGINS=http://localhost:28008,http://127.0.0.1:28008
DB_USER=synapse
DB_PASSWORD=synapse
DB_NAME=synapse
DB_POOL_SIZE=20
DB_MAX_SIZE=50
DB_MIN_IDLE=10
DB_CONNECTION_TIMEOUT=60
REDIS_KEY_PREFIX=synapse:
REDIS_PASSWORD=synapse-redis
SECRET_KEY=ci-secret-key
MACAROON_SECRET=ci-macaroon-secret
FORM_SECRET=ci-form-secret
REGISTRATION_SECRET=ci-registration-secret
ADMIN_SECRET=ci-admin-secret
FEDERATION_SIGNING_KEY=ci-federation-signing-key
FEDERATION_KEY_ID=ed25519:ci
RUN_MIGRATIONS=true
VERIFY_SCHEMA=true
STOP_ON_MIGRATION_FAILURE=true
DB_WAIT_ATTEMPTS=30
DB_WAIT_INTERVAL=2
MIGRATION_TIMEOUT=300
EOF
    TEMP_ENV_CREATED=true
}

load_docker_env() {
    set -a
    . "$DOCKER_DIR/.env"
    set +a
}

cleanup() {
    if [ "$SERVICES_STARTED" = true ]; then
        docker_compose down -v --remove-orphans >/dev/null 2>&1 || true
    fi
    if [ "$TEMP_ENV_CREATED" = true ]; then
        rm -f "$DOCKER_DIR/.env"
    fi
}

dump_docker_logs() {
    if [ ! -d "$DOCKER_DIR" ]; then
        return 0
    fi
    if [ "$SERVICES_STARTED" != true ]; then
        return 0
    fi
    (
        docker_compose ps || true
        docker_compose logs --no-color --tail=300 || true
    ) >&2
}

on_error() {
    dump_docker_logs
}

wait_for_health() {
    local service_name="$1"
    local attempts="${2:-40}"
    local sleep_seconds="${3:-3}"
    for ((i=1; i<=attempts; i++)); do
        local container_id
        container_id="$(docker_compose ps -q "$service_name" 2>/dev/null || true)"
        if [ -z "$container_id" ]; then
            sleep "$sleep_seconds"
            continue
        fi

        local health_status
        health_status="$(docker inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "$container_id" 2>/dev/null || true)"
        if [ "$health_status" = "healthy" ] || [ "$health_status" = "running" ]; then
            return 0
        fi
        sleep "$sleep_seconds"
    done

    log "服务健康检查超时: $service_name"
    dump_docker_logs
    return 1
}

wait_for_http() {
    local url="$1"
    local attempts="${2:-20}"
    local sleep_seconds="${3:-3}"

    for ((i=1; i<=attempts; i++)); do
        if curl -fsS "$url" >/dev/null 2>&1; then
            return 0
        fi
        sleep "$sleep_seconds"
    done

    return 1
}

ensure_clean_stack() {
    docker_compose down -v --remove-orphans >/dev/null 2>&1 || true
    SERVICES_STARTED=false
}

reset_stack() {
    ensure_clean_stack
    ensure_dependencies
}

ensure_dependencies() {
    if [ "$SERVICES_STARTED" = true ]; then
        return 0
    fi

    docker_compose up -d db redis
    SERVICES_STARTED=true
    wait_for_health db 40 3
    wait_for_health redis 30 2
}

ensure_image() {
    if [ "$IMAGE_BUILT" = true ]; then
        return 0
    fi

    docker_compose build synapse-rust
    IMAGE_BUILT=true
}

run_rust_checks() {
    ensure_dependencies

    export DATABASE_URL="postgres://${DB_USER}:${DB_PASSWORD}@localhost:${DB_EXPOSE_PORT}/${DB_NAME}"
    export TEST_DATABASE_URL="$DATABASE_URL"
    export REDIS_URL="redis://:${REDIS_PASSWORD}@localhost:${REDIS_EXPOSE_PORT}"
    export RUST_BACKTRACE=1
    export RUST_LOG=info

    cd "$ROOT_DIR"
    cargo fmt --all -- --check
    cargo check --locked
    TEST_THREADS="${TEST_THREADS:-4}" TEST_RETRIES="${TEST_RETRIES:-2}" bash scripts/run_ci_tests.sh
    emit_skip_report || true
}

emit_skip_report() {
    local skipped_file="$ROOT_DIR/test-results/api-integration.skipped.txt"
    local report_file="$ROOT_DIR/test-results/api-integration.skipped-analysis.txt"
    if [ ! -f "$skipped_file" ]; then
        log "未发现 skipped 结果文件，跳过分类报告: $skipped_file"
        return 0
    fi
    log "输出 skipped 分类报告: $report_file"
    local -a analyzer_args
    analyzer_args=(
        --input "$skipped_file"
        --output "$report_file"
    )
    if [ "${FAIL_ON_SKIP_BACKEND_GAP:-1}" = "1" ]; then
        analyzer_args+=(--fail-on-backend-gap)
    fi
    python3 "$ROOT_DIR/scripts/quality/analyze_skipped_tests.py" "${analyzer_args[@]}"
}

run_migration_checks() {
    ensure_dependencies
    ensure_image

    docker_compose run --rm --no-deps --entrypoint /app/scripts/db_migrate.sh synapse-rust migrate
    docker_compose run --rm --no-deps --entrypoint /app/scripts/db_migrate.sh synapse-rust validate

    cd "$ROOT_DIR"
    # Default guard for refactor-era schema drift: fail fast if critical tables are missing.
    bash scripts/db/pre_refactor_schema_guard.sh check
}

run_docker_smoke() {
    ensure_dependencies
    ensure_image

    docker_compose up -d synapse-rust
    wait_for_http "http://localhost:${SYNAPSE_PORT}/_matrix/client/versions" 20 3
    wait_for_http "http://localhost:${FEDERATION_PORT}/_matrix/federation/v1/version" 20 3
    curl -fsS "http://localhost:${SYNAPSE_PORT}/_matrix/client/versions" >/dev/null
    curl -fsS "http://localhost:${FEDERATION_PORT}/_matrix/federation/v1/version" >/dev/null
}

trap cleanup EXIT
trap on_error ERR

ensure_docker_env
load_docker_env
ensure_clean_stack

case "$MODE" in
    rust)
        log "运行 Rust 全量测试"
        run_rust_checks
        ;;
    migration)
        log "运行迁移校验"
        run_migration_checks
        ;;
    docker)
        log "运行 Docker 启动验证"
        run_docker_smoke
        ;;
    all)
        log "运行完整后端 CI 验证"
        reset_stack
        run_migration_checks
        ensure_clean_stack
        reset_stack
        run_rust_checks
        ensure_clean_stack
        reset_stack
        run_docker_smoke
        ;;
    *)
        printf '用法: %s [rust|migration|docker|all]\n' "$0" >&2
        exit 1
        ;;
esac
