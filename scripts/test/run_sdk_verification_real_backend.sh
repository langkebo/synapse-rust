#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
WORKSPACE_ROOT="$(cd "$BACKEND_ROOT/.." && pwd)"
DOCKER_DIR="$BACKEND_ROOT/docker"
SDK_ROOT="${SDK_ROOT:-$WORKSPACE_ROOT/matrix-js-sdk}"
SDK_REPOSITORY="${SDK_REPOSITORY:-langkebo/matrix-js-sdk}"
SDK_REF="${SDK_REF:-}"
COMPOSE_FILE="${COMPOSE_FILE:-$DOCKER_DIR/docker-compose.yml}"
DOCKER_ENV_FILE="${DOCKER_ENV_FILE:-$DOCKER_DIR/.env}"
BASE_URL="${BASE_URL:-http://localhost:8008}"
WAIT_SECONDS="${WAIT_SECONDS:-180}"
KEEP_STACK_RUNNING="${KEEP_STACK_RUNNING:-0}"
TEMP_ENV_CREATED=false
SDK_CLONED=false
STACK_STARTED=false
SDK_TEST_SCRIPT="${SDK_TEST_SCRIPT:-test:real-backend:verification}"

log() {
    printf '[e2ee-sdk-interop] %s\n' "$*"
}

cleanup() {
    if [ "$KEEP_STACK_RUNNING" != "1" ] && [ "$STACK_STARTED" = true ]; then
        (
            cd "$DOCKER_DIR"
            docker compose --env-file "$DOCKER_ENV_FILE" -f "$COMPOSE_FILE" down -v --remove-orphans
        ) >/dev/null 2>&1 || true
    fi

    if [ "$TEMP_ENV_CREATED" = true ]; then
        rm -f "$DOCKER_ENV_FILE"
    fi

    if [ "$SDK_CLONED" = true ]; then
        rm -rf "$SDK_ROOT"
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
FEDERATION_PORT=28448
DB_EXPOSE_PORT=5432
REDIS_EXPOSE_PORT=6379
SERVER_NAME=localhost
PUBLIC_BASEURL=http://localhost:8008
ALLOWED_ORIGINS=http://localhost:8008,http://127.0.0.1:8008
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

ensure_sdk_checkout() {
    if [ -d "$SDK_ROOT" ]; then
        return 0
    fi

    ensure_command git
    log "SDK directory not found, cloning ${SDK_REPOSITORY}"
    local clone_args=(clone --depth 1 "https://github.com/${SDK_REPOSITORY}.git" "$SDK_ROOT")
    if [ -n "$SDK_REF" ]; then
        clone_args=(clone --depth 1 --branch "$SDK_REF" "https://github.com/${SDK_REPOSITORY}.git" "$SDK_ROOT")
    fi
    git "${clone_args[@]}"
    SDK_CLONED=true
}

wait_for_backend() {
    local deadline=$((SECONDS + WAIT_SECONDS))
    until curl -fsS "$BASE_URL/_matrix/client/versions" >/dev/null; do
        if [ "$SECONDS" -ge "$deadline" ]; then
            echo "Backend did not become ready within ${WAIT_SECONDS}s" >&2
            return 1
        fi
        sleep 2
    done
}

ensure_command docker
ensure_command pnpm
ensure_command curl

if [ ! -f "$COMPOSE_FILE" ]; then
    echo "Compose file not found: $COMPOSE_FILE" >&2
    exit 1
fi

ensure_docker_env
ensure_sdk_checkout

log "Installing matrix-js-sdk dependencies"
pnpm --dir "$SDK_ROOT" install --frozen-lockfile

log "Building synapse-rust docker image"
(
    cd "$DOCKER_DIR"
    docker compose --env-file "$DOCKER_ENV_FILE" -f "$COMPOSE_FILE" build synapse-rust
)

log "Starting synapse-rust docker stack"
(
    cd "$DOCKER_DIR"
    docker compose --env-file "$DOCKER_ENV_FILE" -f "$COMPOSE_FILE" up -d db redis synapse-rust
)
STACK_STARTED=true

log "Waiting for backend readiness at $BASE_URL"
wait_for_backend

log "Running matrix-js-sdk real-backend verification: $SDK_TEST_SCRIPT"
pnpm --dir "$SDK_ROOT" "$SDK_TEST_SCRIPT"
