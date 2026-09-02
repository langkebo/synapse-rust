#!/usr/bin/env bash
# Complement start script for synapse-rust
#
# This script follows the Complement base image contract:
#   - Configures the homeserver based on environment variables
#   - Starts the homeserver listening on 8008 (client) and 8448 (federation)
#   - Trusts the Complement CA certificate at /complement/ca
#
# Environment variables provided by Complement:
#   SERVER_NAME        — Matrix server name (e.g., "hs1")
#   COMPLEMENT_HOST    — Hostname Complement uses to reach this server
#   COMPLEMENT_CA      — Path to the Complement CA certificate (optional)

set -euo pipefail

# ── Configuration ────────────────────────────────────────────────────────

SERVER_NAME="${SERVER_NAME:-hs1}"
COMPLEMENT_HOST="${COMPLEMENT_HOST:-127.0.0.1}"
DB_NAME="synapse_rust_${SERVER_NAME}"

CONFIG_DIR="/app/config"
DATA_DIR="/app/data"
LOG_DIR="/app/logs"
CONFIG_FILE="${CONFIG_DIR}/homeserver.yaml"

mkdir -p "${CONFIG_DIR}" "${DATA_DIR}" "${LOG_DIR}"

# ── Trust Complement CA if present ───────────────────────────────────────

COMPLEMENT_CA_PATH="${COMPLEMENT_CA:-/complement/ca}"
if [[ -f "${COMPLEMENT_CA_PATH}" ]]; then
    cp "${COMPLEMENT_CA_PATH}" /usr/local/share/ca-certificates/complement-ca.crt
    update-ca-certificates 2>/dev/null || true
    echo "[complement] Trusted Complement CA at ${COMPLEMENT_CA_PATH}"
fi

# ── Generate signing key ─────────────────────────────────────────────────

SIGNING_KEY_PATH="${CONFIG_DIR}/signing.key"
if [[ ! -f "${SIGNING_KEY_PATH}" ]]; then
    # Generate an ed25519 signing key (base64-encoded 32 bytes)
    KEY_BYTES=$(head -c 32 /dev/urandom | base64 -w 0)
    echo "ed25519:0 ${KEY_BYTES}" >"${SIGNING_KEY_PATH}"
    chmod 600 "${SIGNING_KEY_PATH}"
fi

# ── Generate homeserver.yaml ─────────────────────────────────────────────

cat >"${CONFIG_FILE}" <<EOF
# Complement-generated homeserver.yaml for ${SERVER_NAME}

server:
  server_name: "${SERVER_NAME}"
  listen:
    - address: "0.0.0.0"
      port: 8008
      tls: false
  federation_listen:
    - address: "0.0.0.0"
      port: 8448
      tls: false
  public_base_url: "http://${COMPLEMENT_HOST}:8008"

database:
  host: "127.0.0.1"
  port: 5432
  user: "postgres"
  password: "postgres"
  database: "${DB_NAME}"
  max_connections: 10

signing_key_path: "${SIGNING_KEY_PATH}"

registration_shared_secret: "complement-test-secret"

# Disable rate limiting for Complement tests
rate_limit:
  enabled: false

# Disable registration verification for fast tests
registration:
  require_auth: false
  require_email_verification: false

# Logging
log:
  level: "info"

# Federation
federation:
  enabled: true
  server_name: "${SERVER_NAME}"

# Trust the Complement CA for federation TLS verification
federation_ca_cert_file: "/etc/ssl/certs/ca-certificates.crt"

# Media
media:
  storage_path: "${DATA_DIR}/media"
  max_upload_size: 10485760

# Disable runtime DB init — Complement sets up the DB externally
# SYNAPSE_SKIP_DB_INIT=true
# SYNAPSE_SKIP_SCHEMA_CHECK=false
EOF

echo "[complement] Generated ${CONFIG_FILE} for server ${SERVER_NAME}"

# ── Database setup ───────────────────────────────────────────────────────
# Complement provides a PostgreSQL instance. Create the database and apply
# migrations before starting the homeserver.

export PGPASSWORD=postgres
PSQL_HOST="${POSTGRES_HOST:-127.0.0.1}"
PSQL_PORT="${POSTGRES_PORT:-5432}"

# Wait for PostgreSQL to be ready
echo "[complement] Waiting for PostgreSQL at ${PSQL_HOST}:${PSQL_PORT}..."
for i in $(seq 1 30); do
    if psql -h "${PSQL_HOST}" -p "${PSQL_PORT}" -U postgres -c "SELECT 1" >/dev/null 2>&1; then
        echo "[complement] PostgreSQL is ready"
        break
    fi
    sleep 1
done

# Create database if it doesn't exist
psql -h "${PSQL_HOST}" -p "${PSQL_PORT}" -U postgres -tc \
    "SELECT 1 FROM pg_database WHERE datname = '${DB_NAME}'" | grep -q 1 ||
    psql -h "${PSQL_HOST}" -p "${PSQL_PORT}" -U postgres -c "CREATE DATABASE \"${DB_NAME}\""

echo "[complement] Database '${DB_NAME}' ready"

# ── Apply migrations ─────────────────────────────────────────────────────

SYNAPSE_DB_HOST="${PSQL_HOST}" \
    SYNAPSE_DB_PORT="${PSQL_PORT}" \
    SYNAPSE_DB_USER="postgres" \
    SYNAPSE_DB_PASSWORD="postgres" \
    SYNAPSE_DB_NAME="${DB_NAME}" \
    SYNAPSE_DB_DATABASE="${DB_NAME}" \
    DATABASE_URL="postgres://postgres:postgres@${PSQL_HOST}:${PSQL_PORT}/${DB_NAME}" \
    bash /app/scripts/db_migrate.sh migrate 2>&1 || {
    echo "[complement] WARNING: Migration failed, attempting to start anyway"
}

# ── Start the homeserver ─────────────────────────────────────────────────

echo "[complement] Starting synapse-rust for server ${SERVER_NAME}..."
exec /app/synapse-rust
