#!/bin/bash
# Performance benchmark DB seeding script
# Seeds the synapse-rust database with production-scale data for latency/QPS benchmarking.
#
# Usage:
#   DB_HOST=localhost DB_PORT=15432 DB_USER=synapse DB_PASSWORD=synapse DB_NAME=synapse bash scripts/seed_test_db.sh
#
# Defaults target the docker-compose.dev-host-access.yml port mapping (15432).
# Override via environment variables.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- Configuration -----------------------------------------------------------
DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-15432}"
DB_USER="${DB_USER:-synapse}"
DB_PASSWORD="${DB_PASSWORD:-synapse}"
DB_NAME="${DB_NAME:-synapse}"
BENCH_PASSWORD="${BENCH_PASSWORD:-benchmark123}"

# Scale knobs
NUM_USERS="${NUM_USERS:-1000}"
NUM_ROOMS="${NUM_ROOMS:-10000}"
NUM_EVENTS="${NUM_EVENTS:-100000}"
NUM_MEMBERSHIPS_PER_ROOM="${NUM_MEMBERSHIPS_PER_ROOM:-5}"
ADMIN_COUNT="${ADMIN_COUNT:-10}"

# Output files
GEN_SQL="$PROJECT_ROOT/.gstack/bench_seed.sql"
GEN_PY="$PROJECT_ROOT/.gstack/gen_hashes.py"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log()  { echo -e "${GREEN}[seed]${NC} $*"; }
warn() { echo -e "${YELLOW}[seed]${NC} $*"; }
err()  { echo -e "${RED}[seed]${NC} $*" >&2; }

# --- Pre-flight checks -------------------------------------------------------
command -v psql >/dev/null 2>&1 || { err "psql not found. Install postgresql-client."; exit 1; }
command -v python3 >/dev/null 2>&1 || { err "python3 not found."; exit 1; }

python3 -c "from argon2 import PasswordHasher" 2>/dev/null || {
    warn "argon2-cffi not installed. Attempting: pip3 install argon2-cffi"
    pip3 install argon2-cffi >/dev/null 2>&1 || {
        err "Failed to install argon2-cffi. Install manually: pip3 install argon2-cffi"
        exit 1
    }
}

export PGPASSWORD="$DB_PASSWORD"
PSQL="psql -h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME -v ON_ERROR_STOP=1"

# Verify connectivity
if ! $PSQL -c "SELECT 1;" >/dev/null 2>&1; then
    err "Cannot connect to PostgreSQL at $DB_HOST:$DB_PORT. Is docker running?"
    err "Try: cd docker && docker compose -f docker-compose.yml -f docker-compose.dev-host-access.yml up -d db"
    exit 1
fi
log "DB connection OK: $DB_HOST:$DB_PORT/$DB_NAME"

# Ask for confirmation before wiping
EXISTING_USERS=$($PSQL -t -c "SELECT COUNT(*) FROM users;" 2>/dev/null || echo "0")
EXISTING_USERS=$(echo "$EXISTING_USERS" | tr -d ' ')
if [ "$EXISTING_USERS" -gt 0 ] 2>/dev/null; then
    warn "Database has $EXISTING_USERS existing users."
    if [ "${FORCE:-0}" != "1" ]; then
        read -r -p "Continue and DELETE existing bench data? [y/N] " REPLY
        if [ "$REPLY" != "y" ] && [ "$REPLY" != "Y" ]; then
            log "Aborted."
            exit 0
        fi
    fi
fi

# --- Phase 1: Generate Argon2id password hashes via Python -------------------
log "Phase 1: Generating Argon2id password hashes for $NUM_USERS users..."
mkdir -p "$PROJECT_ROOT/.gstack"

cat > "$GEN_PY" << 'PYEOF'
import sys
import json
from argon2 import PasswordHasher

NUM_USERS = int(sys.argv[1])
BENCH_PASSWORD = sys.argv[2]

ph = PasswordHasher(
    time_cost=3,
    memory_cost=65536,
    parallelism=1,
    hash_len=32,
)

users = []
for i in range(NUM_USERS):
    user_id = f"@bench_user_{i:04d}:localhost"
    pwd_hash = ph.hash(BENCH_PASSWORD)
    users.append({"user_id": user_id, "password_hash": pwd_hash, "idx": i})

# Also generate admin users
for i in range(10):
    user_id = f"@bench_admin_{i:02d}:localhost"
    pwd_hash = ph.hash(BENCH_PASSWORD)
    users.append({"user_id": user_id, "password_hash": pwd_hash, "idx": NUM_USERS + i, "is_admin": True})

print(json.dumps(users))
PYEOF

USER_JSON=$(python3 "$GEN_PY" "$NUM_USERS" "$BENCH_PASSWORD")
log "Generated $(echo "$USER_JSON" | python3 -c 'import sys,json; print(len(json.load(sys.stdin)))') password hashes"

# --- Phase 2: Build and execute SQL ------------------------------------------
log "Phase 2: Building SQL for bulk data seeding..."

NOW_TS=$(date +%s)000  # milliseconds
NOW_SEC=$(date +%s)      # seconds

TOTAL_USERS=$((NUM_USERS + ADMIN_COUNT))

cat > "$GEN_SQL" << SQLEOF
-- =============================================================================
-- synapse-rust performance benchmark seed data
-- Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)
-- Scale: $TOTAL_USERS users, $NUM_ROOMS rooms, $NUM_EVENTS events
-- =============================================================================

BEGIN;

-- Clean existing bench data (only bench_ prefixed users/rooms for safety)
DELETE FROM events WHERE sender LIKE '%bench_%' OR room_id LIKE '%bench_%';
DELETE FROM room_memberships WHERE user_id LIKE '%bench_%' OR room_id LIKE '%bench_%';
DELETE FROM device_keys WHERE user_id LIKE '%bench_%';
DELETE FROM access_tokens WHERE user_id LIKE '%bench_%';
DELETE FROM rooms WHERE room_id LIKE '%bench_%';
DELETE FROM users WHERE user_id LIKE '%bench_%';

-- =============================================================================
-- Users (with real Argon2id password hashes)
-- =============================================================================
SQLEOF

# Append user INSERTs from the Python-generated JSON
echo "$USER_JSON" | python3 -c '
import sys, json

users = json.load(sys.stdin)
for u in users:
    uid = u["user_id"]
    phash = u["password_hash"]
    is_admin = u.get("is_admin", False)
    idx = u["idx"]
    ts = 1700000000000 + idx * 1000
    username = uid.split(":")[0].replace("@", "")
    is_admin_str = "TRUE" if is_admin else "FALSE"
    print(f"INSERT INTO users (user_id, username, password_hash, is_admin, created_ts, displayname) "
          f"VALUES ({chr(39)}{uid}{chr(39)}, {chr(39)}{username}{chr(39)}, {chr(39)}{phash}{chr(39)}, "
          f"{is_admin_str}, {ts}, {chr(39)}Bench User {idx}{chr(39)});")
' >> "$GEN_SQL"

cat >> "$GEN_SQL" << SQLEOF

-- =============================================================================
-- Access tokens (pre-computed token hashes for bench users)
-- The plaintext token format is: bench_token_<user_id>
-- Token hash is a sha256(plaintext) stored as hex for validation
-- =============================================================================

-- Generate access tokens for all bench users (using a known token format)
-- The actual token value "bench_admin_token_XXXX" will be used by the bench harness.
-- For now we store a placeholder; the bench harness registers real tokens via /login.
-- Create devices for bench users (required for FK in access_tokens)
INSERT INTO devices (device_id, user_id, display_name, created_ts, first_seen_ts, last_seen_ts)
SELECT
    'BENCHDEV01_' || u.user_id,
    u.user_id,
    'Bench Device',
    u.created_ts,
    u.created_ts,
    u.created_ts
FROM users u
WHERE u.user_id LIKE '%bench_%'
ON CONFLICT (device_id) DO NOTHING;

INSERT INTO access_tokens (token_hash, token, user_id, device_id, created_ts, is_revoked)
SELECT
    'bench_token_seed_' || u.user_id,
    'bench_token_seed_' || u.user_id,
    u.user_id,
    'BENCHDEV01_' || u.user_id,
    u.created_ts,
    FALSE
FROM users u
WHERE u.user_id LIKE '%bench_%';

-- =============================================================================
-- Rooms (bench rooms across the full ID space)
-- =============================================================================

INSERT INTO rooms (room_id, creator, room_version, is_public, created_ts, name, join_rules, history_visibility)
SELECT
    '!bench_room_' || LPAD(g::TEXT, 5, '0') || ':localhost',
    '@bench_admin_00:localhost',
    '10',
    FALSE,
    $NOW_TS + (g * 1000),
    'Bench Room ' || g,
    'invite',
    'shared'
FROM generate_series(0, $NUM_ROOMS - 1) AS g;

-- =============================================================================
-- Room memberships (distribute users across rooms)
-- Each room gets ~5 members; the first NUM_ROOMS * 5 users join 1 room each.
-- Room creator (@bench_admin_00) is a member of all rooms.
-- =============================================================================

-- Creator (@bench_admin_00) in all rooms
INSERT INTO room_memberships (room_id, user_id, membership, sender, joined_ts, display_name)
SELECT
    room_id,
    '@bench_admin_00:localhost',
    'join',
    '@bench_admin_00:localhost',
    $NOW_TS,
    'Admin'
FROM rooms
WHERE room_id LIKE '%bench_room_%';

-- Distribute regular users across rooms (each user joins 1 room, round-robin)
INSERT INTO room_memberships (room_id, user_id, membership, sender, joined_ts, display_name)
SELECT
    '!bench_room_' || LPAD((g % $NUM_ROOMS)::TEXT, 5, '0') || ':localhost',
    '@bench_user_' || LPAD(g::TEXT, 4, '0') || ':localhost',
    'join',
    '@bench_user_' || LPAD(g::TEXT, 4, '0') || ':localhost',
    $NOW_TS,
    'User ' || g
FROM generate_series(0, LEAST($NUM_USERS - 1, ($NUM_ROOMS * $NUM_MEMBERSHIPS_PER_ROOM) - 1)) AS g;

-- =============================================================================
-- Events (m.room.message events distributed across rooms)
-- =============================================================================

-- Generate N events, round-robin across rooms, back-dated linearly
INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts,
                    stream_ordering, depth, state_key)
SELECT
    '\$bench_event_' || LPAD(g::TEXT, 6, '0') || ':localhost',
    '!bench_room_' || LPAD((g % $NUM_ROOMS)::TEXT, 5, '0') || ':localhost',
    CASE
        WHEN (RANDOM() * 100)::INT < 5 THEN '@bench_admin_00:localhost'
        ELSE '@bench_user_' || LPAD(((RANDOM() * $NUM_USERS)::INT % $NUM_USERS)::TEXT, 4, '0') || ':localhost'
    END,
    'm.room.message',
    jsonb_build_object(
        'msgtype', 'm.text',
        'body', 'Benchmark message #' || g
    ),
    $NOW_TS - (($NUM_EVENTS - g) * 100),
    nextval('events_stream_ordering_seq'),
    g,
    NULL
FROM generate_series(0, $NUM_EVENTS - 1) AS g;

-- =============================================================================
-- Device keys (1 device per bench user)
-- =============================================================================

INSERT INTO device_keys (user_id, device_id, algorithm, key_id, public_key, key_data, added_ts, created_ts, display_name)
SELECT
    u.user_id,
    'BENCHDEV01',
    'ed25519',
    'ed25519:BENCHDEV01',
    'BENCH_PUBKEY_PLACEHOLDER_' || u.user_id,
    '{"keys":{"ed25519:BENCHDEV01":"BENCH_PUBKEY_PLACEHOLDER_' || REPLACE(u.user_id, '@', '') || '"}}',
    $NOW_TS,
    $NOW_TS,
    'Bench Device'
FROM users u
WHERE u.user_id LIKE '%bench_%'
ON CONFLICT (user_id, device_id, key_id) DO NOTHING;

-- =============================================================================
-- Finalize: ANALYZE for fresh statistics
-- =============================================================================

COMMIT;

ANALYZE users;
ANALYZE rooms;
ANALYZE room_memberships;
ANALYZE events;
ANALYZE device_keys;
ANALYZE access_tokens;

SQLEOF

# --- Phase 3: Execute SQL ----------------------------------------------------
log "Phase 3: Executing seed SQL ($(wc -c < "$GEN_SQL") bytes)..."
$PSQL -f "$GEN_SQL" 2>&1 | while IFS= read -r line; do
    warn "psql: $line"
done

# --- Phase 4: Verify ---------------------------------------------------------
log "Phase 4: Verification"

VERIFY_USERS=$($PSQL -t -c "SELECT COUNT(*) FROM users WHERE user_id LIKE '%bench_%';" | tr -d ' ')
VERIFY_ROOMS=$($PSQL -t -c "SELECT COUNT(*) FROM rooms WHERE room_id LIKE '%bench_room_%';" | tr -d ' ')
VERIFY_EVENTS=$($PSQL -t -c "SELECT COUNT(*) FROM events WHERE sender LIKE '%bench_%';" | tr -d ' ')
VERIFY_MEMBERS=$($PSQL -t -c "SELECT COUNT(*) FROM room_memberships WHERE user_id LIKE '%bench_%';" | tr -d ' ')

echo ""
echo "============================================"
echo " Seed Data Summary"
echo "============================================"
echo " Users:           $VERIFY_USERS"
echo " Rooms:           $VERIFY_ROOMS"
echo " Events:          $VERIFY_EVENTS"
echo " Memberships:     $VERIFY_MEMBERS"
echo ""
echo " Admin users (use for bench):"
$PSQL -t -c "SELECT user_id FROM users WHERE user_id LIKE '%bench_admin_%' AND is_admin = TRUE LIMIT 10;"
echo ""
echo " To get a real access token, register/login via the API:"
echo "   curl -X POST http://localhost:8008/_matrix/client/r0/login \\"
echo "     -H 'Content-Type: application/json' \\"
echo "     -d '{\"type\":\"m.login.password\",\"user\":\"bench_admin_00\",\"password\":\"$BENCH_PASSWORD\"}'"
echo ""
log "Seed complete. SQL preserved at: $GEN_SQL"
