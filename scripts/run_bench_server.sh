#!/bin/bash
# Launch synapse-rust release server for performance benchmarking.
#
# Usage:
#   bash scripts/run_bench_server.sh [start|stop|status]
#
# Environment overrides:
#   BENCH_PORT          Server listen port (default: 8008)
#   BENCH_DB_HOST       PostgreSQL host (default: localhost)
#   BENCH_DB_PORT       PostgreSQL port (default: 15432)
#   BENCH_DB_USER       PostgreSQL user (default: synapse)
#   BENCH_DB_PASSWORD   PostgreSQL password (default: synapse)
#   BENCH_DB_NAME       PostgreSQL database (default: synapse)
#   BENCH_REDIS_ENABLE  Enable Redis (default: false, so baseline is Redis-off)
#   BENCH_REDIS_URL     Redis URL (default: redis://localhost:16379)
#
# The default configuration targets a DB seeded by scripts/seed_test_db.sh
# against the docker-compose.dev-host-access.yml port mapping.
#
# Output:
#   Running server on :<BENCH_PORT> with the bench admin token printed to stdout.
#   A .gstack/bench_homeserver.yaml config file is generated from the template.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- Configuration -----------------------------------------------------------
BENCH_PORT="${BENCH_PORT:-8008}"
BENCH_DB_HOST="${BENCH_DB_HOST:-localhost}"
BENCH_DB_PORT="${BENCH_DB_PORT:-15432}"
BENCH_DB_USER="${BENCH_DB_USER:-synapse}"
BENCH_DB_PASSWORD="${BENCH_DB_PASSWORD:-synapse}"
BENCH_DB_NAME="${BENCH_DB_NAME:-synapse}"
BENCH_REDIS_ENABLE="${BENCH_REDIS_ENABLE:-false}"
BENCH_REDIS_URL="${BENCH_REDIS_URL:-redis://localhost:16379}"
BENCH_CONFIG="${BENCH_CONFIG:-$PROJECT_ROOT/.gstack/bench_homeserver.yaml}"
BENCH_DATA_DIR="${BENCH_DATA_DIR:-$PROJECT_ROOT/.gstack/bench_data}"
BENCH_LOG_DIR="${BENCH_LOG_DIR:-$PROJECT_ROOT/.gstack/bench_logs}"
BENCH_PID_FILE="$PROJECT_ROOT/.gstack/bench_server.pid"
BENCH_SIGNING_KEY="${BENCH_SIGNING_KEY:-$BENCH_DATA_DIR/signing.key}"
BENCH_BINARY="${BENCH_BINARY:-$PROJECT_ROOT/target/release/synapse-rust}"
BENCH_TOKEN_HASH_SECRET="${BENCH_TOKEN_HASH_SECRET:-bench_token_hash_secret_bench_bench_01}"
BENCH_PASSWORD="${BENCH_PASSWORD:-benchmark123}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log() { echo -e "${GREEN}[bench-server]${NC} $*"; }
warn() { echo -e "${YELLOW}[bench-server]${NC} $*"; }
err() { echo -e "${RED}[bench-server]${NC} $*" >&2; }

# --- Generate homeserver config ----------------------------------------------
generate_config() {
    mkdir -p "$(dirname "$BENCH_CONFIG")" "$BENCH_DATA_DIR" "$BENCH_LOG_DIR"

    # Generate signing key if missing
    if [ ! -f "$BENCH_SIGNING_KEY" ]; then
        log "Generating signing key: $BENCH_SIGNING_KEY"
        python3 -c "
import base64
key = base64.urlsafe_b64encode(b'\x00' * 32).decode().rstrip('=')
with open('$BENCH_SIGNING_KEY', 'w') as f:
    f.write(f'ed25519 abc {key}')
" 2>/dev/null || {
            # Fallback: use openssl
            KEY=$(openssl rand -base64 32 | tr -d '\n/+' | head -c 43)
            echo "ed25519 abc $KEY" >"$BENCH_SIGNING_KEY"
        }
        log "Signing key generated (ed25519 placeholder — bench only, do not use in production)"
    fi

    log "Writing bench config: $BENCH_CONFIG"

    if [ "$BENCH_REDIS_ENABLE" = "true" ]; then
        REDIS_YAML=$(
            cat <<REDISYAML
redis:
  enabled: true
  host: "localhost"
  port: 6379
  password: ~
  key_prefix: "bench_"
  pool_size: 16
  connection_timeout_ms: 5000
  command_timeout_ms: 3000
  circuit_breaker: {}
REDISYAML
        )
        REDIS_MODE="redis-enabled"
    else
        REDIS_YAML=$(
            cat <<REDISYAML
redis:
  enabled: false
  host: "localhost"
  port: 6379
  password: ~
  key_prefix: "bench_"
  pool_size: 8
  connection_timeout_ms: 5000
  command_timeout_ms: 3000
  circuit_breaker: {}
REDISYAML
        )
        REDIS_MODE="redis-disabled"
    fi

    cat >"$BENCH_CONFIG" <<YAMLEOF
# Auto-generated benchmark homeserver config
# Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)
# Mode: $REDIS_MODE

server:
  name: "bench.localhost"
  host: "0.0.0.0"
  port: $BENCH_PORT
  public_baseurl: "http://localhost:$BENCH_PORT"
  signing_key_path: "$BENCH_SIGNING_KEY"
  macaroon_secret_key: "bench_macaroon_secret_bench_macaroon_secret_01"
  form_secret: "bench_form_secret_bench_form_secret_bench_01"
  server_name: "localhost"
  registration_shared_secret: "bench_registration_secret_bench_reg_secret_01"
  max_upload_size: 10485760
  max_image_resolution: 8000000
  enable_registration: true
  enable_registration_captcha: false
  background_tasks_interval: 300
  expire_access_token: false
  expire_access_token_lifetime: 86400
  refresh_token_lifetime: 604800
  refresh_token_sliding_window_size: 3600
  session_duration: 86400000
  warmup_pool: true

cors:
  allowed_origins:
    - "*"
  allow_credentials: false

rate_limit:
  enabled: false

database:
  host: "$BENCH_DB_HOST"
  port: $BENCH_DB_PORT
  username: "$BENCH_DB_USER"
  password: "$BENCH_DB_PASSWORD"
  name: "$BENCH_DB_NAME"
  pool_size: 20
  max_size: 50
  min_idle: 5
  connection_timeout: 30

$REDIS_YAML

media:
  max_upload_size: 10485760
  storage_backend: "local"
  local_media_path: "$BENCH_DATA_DIR/media"
  max_image_pixels: 32000000

search:
  enabled: false
  elasticsearch_url: ""

logging:
  level: "warn"
  format: "json"
  log_path: "$BENCH_LOG_DIR"

federation:
  enabled: false
  allow_ingress: false
  server_name: "localhost"
  federation_port: 8448
  connection_pool_size: 4
  max_transaction_payload: 10485760

security:
  secret: "bench_security_secret_bench_sec_00000000000000001"
  expiry_time: 86400
  refresh_token_expiry: 604800

YAMLEOF

    log "Config written: $BENCH_CONFIG"
}

# --- Build release binary ----------------------------------------------------
build_binary() {
    if [ -f "$BENCH_BINARY" ]; then
        # Check if binary is stale (source newer than binary)
        local newest_src
        newest_src=$(find "$PROJECT_ROOT/src" "$PROJECT_ROOT/synapse-services" "$PROJECT_ROOT/synapse-storage" "$PROJECT_ROOT/synapse-federation" "$PROJECT_ROOT/synapse-common" "$PROJECT_ROOT/synapse-e2ee" -name "*.rs" -newer "$BENCH_BINARY" 2>/dev/null | head -1)
        if [ -z "$newest_src" ] && [ "${FORCE_BUILD:-0}" != "1" ]; then
            log "Binary up to date: $BENCH_BINARY"
            return 0
        fi
        log "Source changes detected, rebuilding..."
    fi

    log "Building release binary (this may take a few minutes)..."
    cd "$PROJECT_ROOT"
    SQLX_OFFLINE=true cargo build --release --features "server,core-private-chat" --no-default-features 2>&1 | tail -5
    if [ ! -f "$BENCH_BINARY" ]; then
        err "Build failed — binary not found: $BENCH_BINARY"
        exit 1
    fi
    log "Build complete: $BENCH_BINARY"
}

# --- Start server ------------------------------------------------------------
start_server() {
    if [ -f "$BENCH_PID_FILE" ]; then
        local existing_pid
        existing_pid=$(cat "$BENCH_PID_FILE" 2>/dev/null || echo "")
        if [ -n "$existing_pid" ] && kill -0 "$existing_pid" 2>/dev/null; then
            warn "Server already running (PID $existing_pid). Use 'stop' first."
            return 1
        fi
        rm -f "$BENCH_PID_FILE"
    fi

    generate_config
    build_binary

    log "Starting benchmark server on port $BENCH_PORT..."
    log "Config: $BENCH_CONFIG"
    log "Logs: $BENCH_LOG_DIR/server.log"

    SYNAPSE_CONFIG_PATH="$BENCH_CONFIG" \
        RUST_ENV=development \
        TOKEN_HASH_SECRET="$BENCH_TOKEN_HASH_SECRET" \
        RUST_LOG="${RUST_LOG:-warn,synapse=info}" \
        nohup "$BENCH_BINARY" >"$BENCH_LOG_DIR/server.log" 2>&1 &

    local pid=$!
    echo "$pid" >"$BENCH_PID_FILE"
    log "Server started (PID $pid)"

    # Wait for health
    log "Waiting for server to become healthy..."
    local max_attempts=60
    local attempt=1
    while [ $attempt -le $max_attempts ]; do
        if ! kill -0 "$pid" 2>/dev/null; then
            err "Server died during startup. Check $BENCH_LOG_DIR/server.log"
            return 1
        fi
        local status
        status=$(curl -s -o /dev/null -w "%{http_code}" "http://localhost:$BENCH_PORT/_matrix/client/versions" 2>/dev/null || echo "000")
        if [ "$status" = "200" ]; then
            log "Server healthy (attempt $attempt, HTTP $status)"
            break
        fi
        if [ $attempt -eq $max_attempts ]; then
            err "Server did not become healthy within $max_attempts attempts."
            err "Last status: $status. Check $BENCH_LOG_DIR/server.log"
            return 1
        fi
        sleep 1
        attempt=$((attempt + 1))
    done

    # Register admin user and get access token
    log "Registering bench admin user..."
    local register_resp
    register_resp=$(curl -s -X POST "http://localhost:$BENCH_PORT/_matrix/client/r0/register" \
        -H 'Content-Type: application/json' \
        -d "{
            \"type\": \"m.login.password\",
            \"username\": \"bench_admin_00\",
            \"password\": \"$BENCH_PASSWORD\",
            \"admin\": true
        }" 2>/dev/null || echo '{"error":"register failed"}')

    local access_token
    access_token=$(echo "$register_resp" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('access_token',''))" 2>/dev/null || echo "")

    if [ -z "$access_token" ]; then
        # Try login instead (user might already exist from seed)
        log "Register failed, trying login..."
        local login_resp
        login_resp=$(curl -s -X POST "http://localhost:$BENCH_PORT/_matrix/client/r0/login" \
            -H 'Content-Type: application/json' \
            -d "{
                \"type\": \"m.login.password\",
                \"user\": \"bench_admin_00\",
                \"password\": \"$BENCH_PASSWORD\"
            }" 2>/dev/null || echo '{"error":"login failed"}')
        access_token=$(echo "$login_resp" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('access_token',''))" 2>/dev/null || echo "")
    fi

    if [ -z "$access_token" ]; then
        warn "Could not obtain access token automatically."
        warn "Try manually: curl -X POST http://localhost:$BENCH_PORT/_matrix/client/r0/login -H 'Content-Type: application/json' -d '{\"type\":\"m.login.password\",\"user\":\"bench_admin_00\",\"password\":\"$BENCH_PASSWORD\"}'"
    else
        log "Access token obtained: ${access_token:0:20}..."
    fi

    echo ""
    echo "============================================"
    echo " Benchmark Server Ready"
    echo "============================================"
    echo " URL:          http://localhost:$BENCH_PORT"
    echo " Config:       $BENCH_CONFIG"
    echo " PID:          $pid"
    echo " Redis:        $BENCH_REDIS_ENABLE"
    echo " Access Token: $access_token"
    echo ""
    echo " Export for bench harness:"
    echo "   export BENCH_BASE_URL=http://localhost:$BENCH_PORT"
    echo "   export BENCH_ADMIN_TOKEN=$access_token"
    echo ""
    echo " Server log: tail -f $BENCH_LOG_DIR/server.log"
    echo "============================================"
}

# --- Stop server -------------------------------------------------------------
stop_server() {
    if [ ! -f "$BENCH_PID_FILE" ]; then
        warn "No PID file found. Server may not be running."
        return 0
    fi
    local pid
    pid=$(cat "$BENCH_PID_FILE" 2>/dev/null || echo "")
    if [ -z "$pid" ] || ! kill -0 "$pid" 2>/dev/null; then
        log "Server not running (stale PID file)."
        rm -f "$BENCH_PID_FILE"
        return 0
    fi
    log "Stopping server (PID $pid)..."
    kill "$pid" 2>/dev/null || true
    sleep 2
    if kill -0 "$pid" 2>/dev/null; then
        warn "Server did not stop gracefully, force-killing..."
        kill -9 "$pid" 2>/dev/null || true
    fi
    rm -f "$BENCH_PID_FILE"
    log "Server stopped."
}

# --- Status ------------------------------------------------------------------
status_server() {
    if [ ! -f "$BENCH_PID_FILE" ]; then
        echo "Server: NOT RUNNING (no PID file)"
        return 1
    fi
    local pid
    pid=$(cat "$BENCH_PID_FILE" 2>/dev/null || echo "")
    if [ -z "$pid" ] || ! kill -0 "$pid" 2>/dev/null; then
        echo "Server: NOT RUNNING (stale PID file)"
        return 1
    fi
    local health
    health=$(curl -s -o /dev/null -w "%{http_code}" "http://localhost:$BENCH_PORT/_matrix/client/versions" 2>/dev/null || echo "unreachable")
    echo "Server: RUNNING (PID $pid, HTTP $health)"
}

# --- Main --------------------------------------------------------------------
case "${1:-start}" in
    start)
        start_server
        ;;
    stop)
        stop_server
        ;;
    restart)
        stop_server
        sleep 1
        start_server
        ;;
    status)
        status_server
        ;;
    config)
        generate_config
        log "Config written: $BENCH_CONFIG"
        ;;
    build)
        build_binary
        ;;
    *)
        echo "Usage: $0 {start|stop|restart|status|config|build}"
        echo ""
        echo "Environment:"
        echo "  BENCH_PORT=$BENCH_PORT"
        echo "  BENCH_DB_HOST=$BENCH_DB_HOST"
        echo "  BENCH_REDIS_ENABLE=$BENCH_REDIS_ENABLE"
        exit 1
        ;;
esac
