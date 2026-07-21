#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
COMPOSE_FILE="${SPLIT_MINIMAL_COMPOSE_FILE:-$SCRIPT_DIR/docker-compose.split-minimal.yml}"
ENV_FILE="${SPLIT_MINIMAL_ENV_FILE:-$SCRIPT_DIR/.env}"
ENV_TEMPLATE_FILE="${SPLIT_MINIMAL_ENV_TEMPLATE_FILE:-$SCRIPT_DIR/config/.env.split-minimal.example}"
FALLBACK_ENV_FILE="${SPLIT_MINIMAL_FALLBACK_ENV_FILE:-$SCRIPT_DIR/deploy/.env}"
SMOKE_ENV_FILE="${SPLIT_MINIMAL_SMOKE_ENV_FILE:-$SCRIPT_DIR/config/split-minimal.smoke.env}"
PROJECT_NAME="${SPLIT_MINIMAL_PROJECT_NAME:-split_minimal_smoke_$(date +%s)}"
RUNTIME_ENV_FILE="${SPLIT_MINIMAL_RUNTIME_ENV_FILE:-}"
WAIT_TIMEOUT_SECONDS="${SPLIT_MINIMAL_WAIT_TIMEOUT_SECONDS:-240}"
WAIT_INTERVAL_SECONDS="${SPLIT_MINIMAL_WAIT_INTERVAL_SECONDS:-5}"
ADMIN_TOKEN_MAX_ATTEMPTS="${SPLIT_MINIMAL_ADMIN_TOKEN_MAX_ATTEMPTS:-12}"
ADMIN_TOKEN_RETRY_INTERVAL_SECONDS="${SPLIT_MINIMAL_ADMIN_TOKEN_RETRY_INTERVAL_SECONDS:-10}"
KEEP_RUNNING="${KEEP_RUNNING:-0}"
DOWN_VOLUMES="${DOWN_VOLUMES:-0}"
SKIP_BUILD="${SKIP_BUILD:-0}"
SKIP_DOWN="${SKIP_DOWN:-0}"
ADMIN_USERNAME="${ADMIN_USERNAME:-split_smoke_admin}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-Admin@123}"
RUN_APPSERVICE_P0_D2="${RUN_APPSERVICE_P0_D2:-0}"
APPSERVICE_P0_D2_LABEL="${APPSERVICE_P0_D2_LABEL:-baseline}"
APPSERVICE_D2_RESOURCE_SUMMARY="${APPSERVICE_D2_RESOURCE_SUMMARY:-split_minimal smoke run; 待补 CPU/RSS/连接池/慢查询摘要}"

if [ ! -f "$COMPOSE_FILE" ]; then
    echo "Missing compose file: $COMPOSE_FILE" >&2
    exit 1
fi

if [ ! -f "$ENV_FILE" ]; then
    echo "Missing env file: $ENV_FILE" >&2
    if [ -f "$ENV_TEMPLATE_FILE" ]; then
        echo "Create it first, for example:" >&2
        echo "  cp \"$ENV_TEMPLATE_FILE\" \"$ENV_FILE\"" >&2
    else
        echo "Create it first, for example by copying docker/config/.env.example or docker/deploy/.env.example." >&2
    fi
    exit 1
fi

if [ ! -f "$SMOKE_ENV_FILE" ]; then
    echo "Missing smoke env file: $SMOKE_ENV_FILE" >&2
    exit 1
fi

set -a
# shellcheck disable=SC1090
source "$ENV_FILE"
# shellcheck disable=SC1090
source "$SMOKE_ENV_FILE"
set +a

import_env_value_if_missing() {
    local var_name="$1"
    local env_file="$2"
    if [ -n "${!var_name:-}" ] || [ ! -f "$env_file" ]; then
        return 0
    fi

    local line
    line="$(grep -E "^${var_name}=" "$env_file" | head -n1 || true)"
    if [ -n "$line" ]; then
        export "${line?}"
    fi
}

import_env_value_if_missing "WORKER_REPLICATION_SECRET" "$FALLBACK_ENV_FILE"
import_env_value_if_missing "ADMIN_SHARED_SECRET" "$FALLBACK_ENV_FILE"
import_env_value_if_missing "ADMIN_SECRET" "$FALLBACK_ENV_FILE"
import_env_value_if_missing "REGISTRATION_SHARED_SECRET" "$FALLBACK_ENV_FILE"
import_env_value_if_missing "REGISTRATION_SECRET" "$FALLBACK_ENV_FILE"

export ADMIN_SECRET="${ADMIN_SECRET:-${ADMIN_SHARED_SECRET:-}}"
export ADMIN_SHARED_SECRET="${ADMIN_SHARED_SECRET:-${ADMIN_SECRET:-}}"
export REGISTRATION_SECRET="${REGISTRATION_SECRET:-${REGISTRATION_SHARED_SECRET:-}}"
export REGISTRATION_SHARED_SECRET="${REGISTRATION_SHARED_SECRET:-${REGISTRATION_SECRET:-}}"
export DB_USER="${DB_USER:-${POSTGRES_USER:-synapse}}"
export DB_PASSWORD="${DB_PASSWORD:-${POSTGRES_PASSWORD:-}}"
export DB_NAME="${DB_NAME:-${POSTGRES_DB:-synapse}}"
export SYNAPSE_SERVER="$ADMIN_ENDPOINT"

if [ -z "$RUNTIME_ENV_FILE" ]; then
    RUNTIME_ENV_FILE="$(mktemp "${TMPDIR:-/tmp}/split_minimal_env.XXXXXX")"
    trap 'rm -f "$RUNTIME_ENV_FILE"' EXIT
fi

python3 - "$ENV_FILE" "$RUNTIME_ENV_FILE" "$PROJECT_NAME" <<'PY'
from pathlib import Path
import sys

src = Path(sys.argv[1])
dst = Path(sys.argv[2])
project_name = sys.argv[3]

lines = src.read_text().splitlines()
out = []
found = False
for line in lines:
    if line.startswith("COMPOSE_PROJECT_NAME="):
        out.append(f"COMPOSE_PROJECT_NAME={project_name}")
        found = True
    else:
        out.append(line)

if not found:
    out.append(f"COMPOSE_PROJECT_NAME={project_name}")

dst.write_text("\n".join(out) + "\n")
PY

# Docker Compose prefers the current process environment over --env-file values,
# so pin the generated project name here to avoid reusing stale stack names.
export COMPOSE_PROJECT_NAME="$PROJECT_NAME"

if [ -z "${ADMIN_SECRET:-}" ]; then
    echo "ADMIN_SECRET or ADMIN_SHARED_SECRET must be set in $ENV_FILE" >&2
    exit 1
fi

if [ -z "${REGISTRATION_SECRET:-}" ]; then
    echo "REGISTRATION_SECRET or REGISTRATION_SHARED_SECRET must be set in $ENV_FILE" >&2
    exit 1
fi

if [ -z "${DB_PASSWORD:-}" ]; then
    echo "DB_PASSWORD or POSTGRES_PASSWORD must be set in $ENV_FILE" >&2
    exit 1
fi

if [ -z "${WORKER_REPLICATION_SECRET:-}" ]; then
    echo "WORKER_REPLICATION_SECRET must be set in $ENV_FILE" >&2
    exit 1
fi

COMPOSE_CMD=(docker compose --env-file "$RUNTIME_ENV_FILE" -f "$COMPOSE_FILE")

on_error() {
    local exit_code="$1"
    echo "split_minimal smoke failed (exit=$exit_code)" >&2
    "${COMPOSE_CMD[@]}" ps >&2 || true
}

cleanup() {
    if [ "$KEEP_RUNNING" = "1" ] || [ "$SKIP_DOWN" = "1" ]; then
        return
    fi

    if [ "$DOWN_VOLUMES" = "1" ]; then
        "${COMPOSE_CMD[@]}" down -v
    else
        "${COMPOSE_CMD[@]}" down
    fi
}

trap 'on_error $?' ERR
trap cleanup EXIT

wait_for_http() {
    local name="$1"
    local url="$2"
    local expected="${3:-200}"
    local elapsed=0

    while [ "$elapsed" -lt "$WAIT_TIMEOUT_SECONDS" ]; do
        status="$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 "$url" 2>/dev/null || echo "000")"
        if [ "$status" = "$expected" ]; then
            echo "ready: $name ($url)"
            return 0
        fi
        sleep "$WAIT_INTERVAL_SECONDS"
        elapsed=$((elapsed + WAIT_INTERVAL_SECONDS))
    done

    echo "Timed out waiting for $name at $url (last status: ${status:-unknown})" >&2
    return 1
}

admin_login_token() {
    local response
    response="$(curl -s -X POST "$ADMIN_ENDPOINT/_matrix/client/v3/login" \
        -H "Content-Type: application/json" \
        -d "{\"type\":\"m.login.password\",\"user\":\"$ADMIN_USERNAME\",\"password\":\"$ADMIN_PASSWORD\"}" 2>/dev/null || true)"
    python3 -c 'import json,sys; data=json.load(sys.stdin); print(data.get("access_token",""))' <<<"$response" 2>/dev/null || true
}

register_admin_if_needed() {
    local token
    token="$(admin_login_token)"
    if [ -n "$token" ]; then
        echo "$token"
        return 0
    fi

    local register_output
    register_output="$(ADMIN_USERNAME="$ADMIN_USERNAME" ADMIN_PASSWORD="$ADMIN_PASSWORD" ADMIN_SHARED_SECRET="$ADMIN_SHARED_SECRET" SYNAPSE_SERVER="$ADMIN_ENDPOINT" python3 "$SCRIPT_DIR/deploy/register_admin.py" 2>&1 || true)"
    printf '%s\n' "$register_output" >&2

    token="$(printf '%s\n' "$register_output" | python3 -c 'import re,sys; text=sys.stdin.read(); m=re.search(r"ADMIN_TOKEN='\''([^'\'']+)'\''", text); print(m.group(1) if m else "")')"
    if [ -n "$token" ]; then
        echo "$token"
        return 0
    fi

    token="$(admin_login_token)"
    if [ -n "$token" ]; then
        echo "$token"
        return 0
    fi

    return 1
}

obtain_admin_token_with_retry() {
    local attempt=1
    local token=""

    while [ "$attempt" -le "$ADMIN_TOKEN_MAX_ATTEMPTS" ]; do
        token="$(register_admin_if_needed)"
        if [ -n "$token" ]; then
            echo "$token"
            return 0
        fi

        echo "Admin token attempt $attempt/$ADMIN_TOKEN_MAX_ATTEMPTS did not succeed yet; retrying in ${ADMIN_TOKEN_RETRY_INTERVAL_SECONDS}s..." >&2
        sleep "$ADMIN_TOKEN_RETRY_INTERVAL_SECONDS"
        attempt=$((attempt + 1))
    done

    return 1
}

run_appservice_p0_d2_if_enabled() {
    if [ "$RUN_APPSERVICE_P0_D2" != "1" ]; then
        return 0
    fi

    echo "==> Running appservice P0 D2 archive"
    ADMIN_TOKEN="$ADMIN_TOKEN" \
        BASE_URL="$ADMIN_ENDPOINT" \
        PROMETHEUS_URL="${PROMETHEUS_URL:-http://127.0.0.1:9090/metrics}" \
        APPSERVICE_D2_RESOURCE_SUMMARY="$APPSERVICE_D2_RESOURCE_SUMMARY" \
        bash "$REPO_ROOT/scripts/run_appservice_p0_d2.sh" "$APPSERVICE_P0_D2_LABEL"
}

echo "==> Validating split_minimal compose config"
echo "==> Using compose project: $COMPOSE_PROJECT_NAME"
"${COMPOSE_CMD[@]}" config >/dev/null

echo "==> Starting split_minimal stack"
if [ "$SKIP_BUILD" = "1" ]; then
    "${COMPOSE_CMD[@]}" up -d
else
    "${COMPOSE_CMD[@]}" up -d --build
fi

echo "==> Waiting for admin and proxy endpoints"
wait_for_http "admin versions" "$ADMIN_ENDPOINT/_matrix/client/versions" 200
wait_for_http "admin register nonce" "$ADMIN_ENDPOINT/_synapse/admin/v1/register/nonce" 200
wait_for_http "proxy versions" "$CLIENT_ENDPOINT/_matrix/client/versions" 200

echo "==> Ensuring admin token"
ADMIN_TOKEN="$(obtain_admin_token_with_retry)"
if [ -z "$ADMIN_TOKEN" ]; then
    echo "Failed to obtain admin token for split_minimal smoke run" >&2
    exit 1
fi

export ADMIN_AUTH_HEADER="Authorization: Bearer $ADMIN_TOKEN"
export REPLICATION_SECRET="$WORKER_REPLICATION_SECRET"

echo "==> Running deployment smoke test"
bash "$REPO_ROOT/scripts/deployment_smoke_test.sh"

run_appservice_p0_d2_if_enabled

echo "==> Split minimal smoke test completed"
if [ "$KEEP_RUNNING" = "1" ]; then
    echo "Stack is still running because KEEP_RUNNING=1"
fi
