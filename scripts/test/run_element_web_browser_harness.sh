#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DOCKER_DIR="$BACKEND_ROOT/docker"
HARNESS_DIR="${HARNESS_DIR:-$BACKEND_ROOT/tests/element-web-harness}"
BASE_COMPOSE_FILE="${BASE_COMPOSE_FILE:-$DOCKER_DIR/docker-compose.yml}"
WEB_COMPOSE_FILE="${WEB_COMPOSE_FILE:-$DOCKER_DIR/docker-compose.web.yml}"
DOCKER_ENV_FILE="${DOCKER_ENV_FILE:-$DOCKER_DIR/.env}"
MATRIX_BASE_URL="${MATRIX_BASE_URL:-https://matrix.test}"
ELEMENT_BASE_URL="${ELEMENT_BASE_URL:-https://element.test}"
WAIT_SECONDS="${WAIT_SECONDS:-240}"
KEEP_STACK_RUNNING="${KEEP_STACK_RUNNING:-0}"
BROWSER_ONLY_OVERLAY="${BROWSER_ONLY_OVERLAY:-0}"
SKIP_NODE_INSTALL="${SKIP_NODE_INSTALL:-0}"
ARTIFACT_DIR="${ELEMENT_HARNESS_ARTIFACT_DIR:-$BACKEND_ROOT/artifacts/e2ee-interop}"
TEST_USERNAME="${ELEMENT_TEST_USERNAME:-elementweb_$(date +%s)_$RANDOM}"
TEST_PASSWORD="${ELEMENT_TEST_PASSWORD:-Test@123456}"
TEST_DISPLAY_NAME="${ELEMENT_TEST_DISPLAY_NAME:-Element Web Smoke}"
TEST_SCRIPT="${TEST_SCRIPT:-smoke:login}" # 新增：可以选择测试脚本
DOCKER_CARGO_FEATURE_ARGS="${DOCKER_CARGO_FEATURE_ARGS:---features server --no-default-features}"
DOCKER_CARGO_BUILD_JOBS="${DOCKER_CARGO_BUILD_JOBS:-1}"
TEMP_ENV_CREATED=false
STACK_STARTED=false

log() {
    printf '[element-web-harness] %s\n' "$*"
}

compose() {
    (
        cd "$DOCKER_DIR"
        docker compose \
            --env-file "$DOCKER_ENV_FILE" \
            -f "$BASE_COMPOSE_FILE" \
            -f "$WEB_COMPOSE_FILE" \
            "$@"
    )
}

cleanup() {
    if [ "$KEEP_STACK_RUNNING" != "1" ] && [ "$STACK_STARTED" = true ]; then
        compose down -v --remove-orphans >/dev/null 2>&1 || true
    fi

    if [ "$TEMP_ENV_CREATED" = true ]; then
        rm -f "$DOCKER_ENV_FILE"
    fi
}

trap cleanup EXIT

ensure_command() {
    local command_name="$1"
    if ! command -v "$command_name" >/dev/null 2>&1; then
        echo "$command_name is required but was not found in PATH" >&2
        exit 1
    fi
}

ensure_docker_env() {
    if [ -f "$DOCKER_ENV_FILE" ]; then
        return 0
    fi

    mkdir -p "$(dirname "$DOCKER_ENV_FILE")"
    cat >"$DOCKER_ENV_FILE" <<'EOF'
COMPOSE_PROJECT_NAME=synapse-e2ee-interop
SYNAPSE_IMAGE=synapse-rust
SYNAPSE_IMAGE_TAG=latest
POSTGRES_VERSION=16
REDIS_VERSION=7
TZ=UTC
RUST_LOG=info
SYNAPSE_PORT=8008
FEDERATION_PORT=8448
DB_EXPOSE_PORT=5432
REDIS_EXPOSE_PORT=6379
SERVER_NAME=matrix.test
PUBLIC_BASEURL=https://matrix.test
ALLOWED_ORIGINS=https://matrix.test,https://element.test,http://localhost:8008,http://127.0.0.1:8008
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
WORKER_REPLICATION_SECRET=ci-worker-replication-secret
FEDERATION_SIGNING_KEY=ci-federation-signing-key
FEDERATION_KEY_ID=ed25519:ci
RUN_MIGRATIONS=true
VERIFY_SCHEMA=true
STOP_ON_MIGRATION_FAILURE=true
DB_WAIT_ATTEMPTS=30
DB_WAIT_INTERVAL=2
MIGRATION_TIMEOUT=300
SYNAPSE_ENABLE_RUNTIME_DB_INIT=true
SYNAPSE_SKIP_SCHEMA_CHECK=true
EOF
    TEMP_ENV_CREATED=true
}

wait_for_url() {
    local description="$1"
    local url="$2"
    shift 2

    local deadline=$((SECONDS + WAIT_SECONDS))
    until curl -fsS "$@" "$url" >/dev/null; do
        if [ "$SECONDS" -ge "$deadline" ]; then
            echo "$description did not become ready within ${WAIT_SECONDS}s: $url" >&2
            return 1
        fi
        sleep 2
    done
}

register_test_user() {
    local payload response
    payload="$(
        python3 - "$TEST_USERNAME" "$TEST_PASSWORD" "$TEST_DISPLAY_NAME" <<'PY'
import json
import sys

print(json.dumps({
    "username": sys.argv[1],
    "password": sys.argv[2],
    "displayname": sys.argv[3],
    "auth": {"type": "m.login.dummy"},
}))
PY
    )"

    response="$(
        curl -fsS \
            --resolve matrix.test:443:127.0.0.1 \
            -k \
            -H 'Content-Type: application/json' \
            -X POST \
            "$MATRIX_BASE_URL/_matrix/client/v3/register" \
            -d "$payload"
    )"

    TEST_USER_ID="$(printf '%s' "$response" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("user_id", ""))')"
    if [ -z "$TEST_USER_ID" ]; then
        echo "Registration succeeded but user_id was missing" >&2
        echo "$response" >&2
        return 1
    fi

    log "Registered test user ${TEST_USER_ID}"
}

install_browser_dependencies() {
    local playwright_bin

    if [ "$SKIP_NODE_INSTALL" = "1" ]; then
        return 0
    fi

    log "Installing browser harness dependencies"
    npm --prefix "$HARNESS_DIR" install

    playwright_bin="$HARNESS_DIR/node_modules/.bin/playwright"
    log "Installing Chromium for Playwright"
    if [ "$(uname -s)" = "Linux" ]; then
        "$playwright_bin" install --with-deps chromium
    else
        "$playwright_bin" install chromium
    fi
}

ensure_command docker
ensure_command node
ensure_command npm
ensure_command curl
ensure_command python3

if [ ! -f "$BASE_COMPOSE_FILE" ]; then
    echo "Base compose file not found: $BASE_COMPOSE_FILE" >&2
    exit 1
fi

if [ ! -f "$WEB_COMPOSE_FILE" ]; then
    echo "Web compose file not found: $WEB_COMPOSE_FILE" >&2
    exit 1
fi

mkdir -p "$ARTIFACT_DIR"
ensure_docker_env
install_browser_dependencies

if [ "$BROWSER_ONLY_OVERLAY" != "1" ]; then
    log "Building synapse-rust docker image"
    (
        cd "$DOCKER_DIR"
        DOCKER_CARGO_FEATURE_ARGS="$DOCKER_CARGO_FEATURE_ARGS" \
            DOCKER_CARGO_BUILD_JOBS="$DOCKER_CARGO_BUILD_JOBS" \
            docker compose --env-file "$DOCKER_ENV_FILE" -f "$BASE_COMPOSE_FILE" build synapse-rust
    )

    log "Starting browser interop stack"
    compose up -d db redis synapse-rust element-web nginx
else
    log "Starting Element Web overlay on top of existing synapse-rust stack"
    compose up -d element-web nginx
fi
STACK_STARTED=true

log "Waiting for Matrix HTTPS endpoint"
wait_for_url \
    "Matrix HTTPS endpoint" \
    "$MATRIX_BASE_URL/_matrix/client/versions" \
    --resolve matrix.test:443:127.0.0.1 \
    -k

log "Waiting for Element Web"
wait_for_url \
    "Element Web" \
    "$ELEMENT_BASE_URL/" \
    --resolve element.test:443:127.0.0.1 \
    -k

register_test_user

log "Running Element Web test: ${TEST_SCRIPT}"
ELEMENT_BASE_URL="$ELEMENT_BASE_URL" \
    ELEMENT_TEST_USERNAME="$TEST_USERNAME" \
    ELEMENT_TEST_PASSWORD="$TEST_PASSWORD" \
    ELEMENT_HARNESS_ARTIFACT_DIR="$ARTIFACT_DIR" \
    npm --prefix "$HARNESS_DIR" run "$TEST_SCRIPT"
