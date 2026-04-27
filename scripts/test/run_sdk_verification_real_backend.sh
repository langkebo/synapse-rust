#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
WORKSPACE_ROOT="$(cd "$BACKEND_ROOT/.." && pwd)"
SDK_ROOT="$WORKSPACE_ROOT/matrix-js-sdk"
COMPOSE_FILE="${COMPOSE_FILE:-$BACKEND_ROOT/docker/docker-compose.yml}"
BASE_URL="${BASE_URL:-http://localhost:8008}"
WAIT_SECONDS="${WAIT_SECONDS:-60}"

if [ ! -f "$COMPOSE_FILE" ]; then
  echo "Compose file not found: $COMPOSE_FILE" >&2
  exit 1
fi

if [ ! -d "$SDK_ROOT" ]; then
  echo "SDK directory not found: $SDK_ROOT" >&2
  exit 1
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required but was not found in PATH" >&2
  exit 1
fi

if ! command -v pnpm >/dev/null 2>&1; then
  echo "pnpm is required but was not found in PATH" >&2
  exit 1
fi

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required but was not found in PATH" >&2
  exit 1
fi

echo "==> Building synapse-rust image"
docker compose -f "$COMPOSE_FILE" build synapse-rust

echo "==> Restarting synapse-rust stack"
docker compose -f "$COMPOSE_FILE" up -d synapse-rust

echo "==> Waiting for backend readiness at $BASE_URL"
deadline=$((SECONDS + WAIT_SECONDS))
until curl -fsS "$BASE_URL/_matrix/client/versions" >/dev/null; do
  if [ "$SECONDS" -ge "$deadline" ]; then
    echo "Backend did not become ready within ${WAIT_SECONDS}s" >&2
    exit 1
  fi
  sleep 2
done

echo "==> Running SDK real-backend verification test"
pnpm --dir "$SDK_ROOT" test:real-backend:verification
