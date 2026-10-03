#!/usr/bin/env bash
#
# scripts/federation-test/test_federation.sh
#
# 联邦互操作性测试脚本（跨实现：A = synapse-rust，B = 上游真 Synapse）
#
# 前置条件（见 docs/audit/A5_LIVE_FEDERATION_INTEROP_TESTING.md）：
#   * 两套 compose 栈已启动且健康：
#       docker/federation-test/docker-compose-synapse-a.yml  (客户端 18008 / 联邦 18448)
#       docker/federation-test/docker-compose-synapse-b.yml  (客户端 18009 / 联邦 18449)
#   * server_name 分别为 synapse-a.federation.test / synapse-b.federation.test，
#     由各栈的 nginx 边车在 8448 上终结 TLS，并在 federation_test_net_shared 上提供 DNS 别名。
#
# 测试内容：
#   1. 健康检查（两个实例）
#   2. 注册用户（A 用 admin registration nonce+HMAC；B 用 Synapse 的 register_new_matrix_user）
#   3. 登录取 access_token
#   4. 在 A 上创建公开房间
#   5. B 用户经联邦加入该房间（domainless room id 需显式 ?via=）
#   6. A 发送消息，B 经联邦接收
#
# 用法:
#   bash scripts/federation-test/test_federation.sh
#
set -uo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ARTIFACT_DIR="$ROOT_DIR/artifacts/federation-interop"

# Endpoints (Client-Server API, plaintext, host-published)
SYNAPSE_A_BASE="http://localhost:18008"
SYNAPSE_B_BASE="http://localhost:18009"

# Container names (for reading ADMIN_SECRET / server_name)
CONTAINER_A="synapse-federation-a"
CONTAINER_B="synapse-federation-b"

SERVER_A="synapse-a.federation.test"
SERVER_B="synapse-b.federation.test"

USER_A="user_a"
USER_B="user_b"
PASSWORD_A="FedTest@123"
PASSWORD_B="FedTest@456"

COLOR_RED='\033[0;31m'
COLOR_GREEN='\033[0;32m'
COLOR_YELLOW='\033[1;33m'
COLOR_BLUE='\033[0;34m'
NC='\033[0m'

FAILURES=0

log() { printf "${COLOR_BLUE}[INFO]${NC} %s\n" "$*"; }
success() { printf "${COLOR_GREEN}[✓]${NC} %s\n" "$*"; }
warn() { printf "${COLOR_YELLOW}[WARN]${NC} %s\n" "$*"; }
error() {
    printf "${COLOR_RED}[✗]${NC} %s\n" "$*" >&2
    FAILURES=$((FAILURES + 1))
}

mkdir -p "$ARTIFACT_DIR"

# ─────────────────────────────────────────────────────────────────────────────
# Step 1: Health check
# ─────────────────────────────────────────────────────────────────────────────
log "Step 1: Checking instance health..."

check_health() {
    local name="$1" base_url="$2"
    if curl -sf "$base_url/_matrix/client/versions" >/dev/null 2>&1; then
        success "$name is healthy ($base_url)"
        return 0
    fi
    error "$name is not responding ($base_url)"
    return 1
}

check_health "Synapse-A" "$SYNAPSE_A_BASE" || {
    error "Synapse-A unreachable, aborting"
    exit 1
}
check_health "Synapse-B" "$SYNAPSE_B_BASE" || {
    error "Synapse-B unreachable, aborting"
    exit 1
}

echo ""

# ─────────────────────────────────────────────────────────────────────────────
# Step 2: Register users via admin registration (nonce + HMAC-SHA256)
# ─────────────────────────────────────────────────────────────────────────────
log "Step 2: Registering test users (admin registration API)..."

# register_user <container> <base_url> <username> <password>
register_user() {
    local container="$1" base_url="$2" username="$3" password="$4"
    local secret nonce

    secret=$(docker exec "$container" printenv ADMIN_SECRET 2>/dev/null || echo "")
    if [[ -z "$secret" ]]; then
        warn "$container: ADMIN_SECRET unavailable, skipping registration"
        return 1
    fi

    nonce=$(curl -sf "$base_url/_synapse/admin/v1/register/nonce" 2>/dev/null | jq -r '.nonce // empty')
    if [[ -z "$nonce" ]]; then
        warn "$base_url: could not fetch registration nonce"
        return 1
    fi

    # mac = HMAC-SHA256(shared_secret, nonce \0 username \0 password \0 ("admin\0\0\0"|"notadmin"))
    local mac
    mac=$(printf '%s\0%s\0%s\0admin\0\0\0' "$nonce" "$username" "$password" |
        openssl dgst -sha256 -hmac "$secret" -r | awk '{print $1}')

    local response
    response=$(curl -s -X POST "$base_url/_synapse/admin/v1/register" \
        -H "Content-Type: application/json" \
        -d "{\"nonce\":\"$nonce\",\"username\":\"$username\",\"password\":\"$password\",\"admin\":true,\"mac\":\"$mac\"}")

    if echo "$response" | jq -e '.user_id' >/dev/null 2>&1; then
        success "Registered $username on $container"
        return 0
    fi

    local errcode
    errcode=$(echo "$response" | jq -r '.errcode // empty' 2>/dev/null)
    if [[ "$errcode" == "M_USER_IN_USE" ]]; then
        log "$username already exists on $container (reusing)"
        return 0
    fi

    warn "Registration for $username failed: $(echo "$response" | jq -c . 2>/dev/null)"
    return 1
}

# register_user_synapse <container> <username> <password>
# 真 Synapse 自带 register_new_matrix_user CLI，直接连本地 8008，比手算 nonce+HMAC
# 更稳健（mac 算法随 Synapse 版本演进，避免与规范漂移）。
register_user_synapse() {
    local container="$1" username="$2" password="$3"
    local out

    out=$(docker exec "$container" register_new_matrix_user \
        -c /data/homeserver.yaml \
        -u "$username" -p "$password" \
        --no-admin --exists-ok \
        http://127.0.0.1:8008 2>&1) && {
        success "Registered $username on $container"
        return 0
    }

    warn "Registration for $username on $container failed: $out"
    return 1
}

register_user "$CONTAINER_A" "$SYNAPSE_A_BASE" "$USER_A" "$PASSWORD_A" || warn "continuing; will attempt login"
register_user_synapse "$CONTAINER_B" "$USER_B" "$PASSWORD_B" || warn "continuing; will attempt login"

echo ""

# ─────────────────────────────────────────────────────────────────────────────
# Step 3: Login and get access tokens
# ─────────────────────────────────────────────────────────────────────────────
log "Step 3: Logging in to get access tokens..."

# login_user <base_url> <username> <password>
login_user() {
    local base_url="$1" username="$2" password="$3"
    curl -s -X POST "$base_url/_matrix/client/v3/login" \
        -H "Content-Type: application/json" \
        -d "{\"type\":\"m.login.password\",\"identifier\":{\"type\":\"m.id.user\",\"user\":\"$username\"},\"password\":\"$password\"}" |
        jq -r '.access_token // empty'
}

TOKEN_A=$(login_user "$SYNAPSE_A_BASE" "$USER_A" "$PASSWORD_A")
TOKEN_B=$(login_user "$SYNAPSE_B_BASE" "$USER_B" "$PASSWORD_B")

if [[ -z "$TOKEN_A" ]]; then
    error "Failed to login as $USER_A on Synapse-A"
    exit 1
fi
success "Logged in as @$USER_A:$SERVER_A on Synapse-A"

if [[ -z "$TOKEN_B" ]]; then
    error "Failed to login as $USER_B on Synapse-B"
    exit 1
fi
success "Logged in as @$USER_B:$SERVER_B on Synapse-B"

echo ""

# ─────────────────────────────────────────────────────────────────────────────
# Step 4: Create federated room on Synapse-A
# ─────────────────────────────────────────────────────────────────────────────
log "Step 4: Creating public room on Synapse-A..."

RUN_TS=$(date +%s)
ROOM_ALIAS="fedtest_$RUN_TS"
ROOM_NAME="Federation Test Room $RUN_TS"
CREATE_ROOM_RESPONSE=$(curl -s -X POST "$SYNAPSE_A_BASE/_matrix/client/v3/createRoom" \
    -H "Authorization: Bearer $TOKEN_A" \
    -H "Content-Type: application/json" \
    -d "{\"visibility\":\"public\",\"room_alias_name\":\"$ROOM_ALIAS\",\"name\":\"$ROOM_NAME\",\"preset\":\"public_chat\"}")

ROOM_ID=$(echo "$CREATE_ROOM_RESPONSE" | jq -r '.room_id // empty')
if [[ -z "$ROOM_ID" ]]; then
    error "Failed to create room on Synapse-A: $(echo "$CREATE_ROOM_RESPONSE" | jq -c .)"
    exit 1
fi
success "Created room: $ROOM_ID"

echo ""

# ─────────────────────────────────────────────────────────────────────────────
# Step 5: Join the room from Synapse-B (cross-server federation)
# ─────────────────────────────────────────────────────────────────────────────
log "Step 5: Federating - joining room from Synapse-B..."

# Room version 12+ room IDs are domainless, so the joining server must be told
# which remote server to ask; `via` is the spec parameter (repeatable).
JOIN_RESPONSE=$(curl -s -X POST "$SYNAPSE_B_BASE/_matrix/client/v3/join/$ROOM_ID?via=$SERVER_A" \
    -H "Authorization: Bearer $TOKEN_B" \
    -H "Content-Type: application/json" \
    -d '{}')

JOINED_ROOM_ID=$(echo "$JOIN_RESPONSE" | jq -r '.room_id // empty')
FEDERATION_JOIN_OK=false

if [[ -n "$JOINED_ROOM_ID" ]]; then
    FEDERATION_JOIN_OK=true
    success "User B joined room from Synapse-B: $JOINED_ROOM_ID"
else
    error "User B failed to join room from Synapse-B: $(echo "$JOIN_RESPONSE" | jq -c . 2>/dev/null)"
fi

echo ""

# ─────────────────────────────────────────────────────────────────────────────
# Step 6/7: Send from A, receive on B
# ─────────────────────────────────────────────────────────────────────────────
MESSAGE_BODY=""
EVENT_ID=""
RECEIVED_EVENT_COUNT=0
FEDERATION_MESSAGE_OK=false

if [[ "$FEDERATION_JOIN_OK" == "true" ]]; then
    log "Step 6: Sending test message from Synapse-A..."

    MESSAGE_TS=$(date +%s)
    MESSAGE_BODY="Hello from Synapse-A! Federation test at $MESSAGE_TS"
    SEND_MSG_RESPONSE=$(curl -s -X PUT \
        "$SYNAPSE_A_BASE/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.message/fedtest$MESSAGE_TS" \
        -H "Authorization: Bearer $TOKEN_A" \
        -H "Content-Type: application/json" \
        -d "{\"msgtype\":\"m.text\",\"body\":\"$MESSAGE_BODY\"}")

    EVENT_ID=$(echo "$SEND_MSG_RESPONSE" | jq -r '.event_id // empty')
    if [[ -n "$EVENT_ID" ]]; then
        success "Message sent from Synapse-A: $EVENT_ID"
    else
        error "Failed to send message from Synapse-A: $(echo "$SEND_MSG_RESPONSE" | jq -c . 2>/dev/null)"
    fi

    log "Step 7: Checking for received message on Synapse-B..."

    # Federation delivery is asynchronous; poll briefly.
    for _ in $(seq 1 10); do
        GET_EVENTS_RESPONSE=$(curl -s \
            "$SYNAPSE_B_BASE/_matrix/client/v3/rooms/$ROOM_ID/messages?dir=b&limit=50" \
            -H "Authorization: Bearer $TOKEN_B")
        RECEIVED_EVENT_COUNT=$(echo "$GET_EVENTS_RESPONSE" | jq -r '.chunk | length' 2>/dev/null || echo "0")

        if echo "$GET_EVENTS_RESPONSE" | jq -e --arg body "$MESSAGE_BODY" \
            '.chunk[] | select(.content.body == $body)' >/dev/null 2>&1; then
            FEDERATION_MESSAGE_OK=true
            break
        fi
        sleep 2
    done

    if [[ "$FEDERATION_MESSAGE_OK" == "true" ]]; then
        success "Received federated message on Synapse-B (room has $RECEIVED_EVENT_COUNT event(s) visible)"
        echo "$GET_EVENTS_RESPONSE" | jq -c '.chunk[] | select(.content.body != null) | {sender, body: .content.body, origin_server_ts}' 2>/dev/null || true
    else
        warn "Message not yet observed on Synapse-B ($RECEIVED_EVENT_COUNT event(s) visible)"
    fi
else
    warn "Skipping message send/receive - federated join did not succeed"
fi

echo ""

# ─────────────────────────────────────────────────────────────────────────────
# Results
# ─────────────────────────────────────────────────────────────────────────────
log "=========================================="
log "  Federation Test Summary"
log "=========================================="
log "  Synapse-A: $SYNAPSE_A_BASE (federation $SERVER_A:18448)"
log "  Synapse-B: $SYNAPSE_B_BASE (federation $SERVER_B:18449)"
log "  User A:    @$USER_A:$SERVER_A"
log "  User B:    @$USER_B:$SERVER_B"
log "  Room:      ${ROOM_ID:-<none>}"
log "  Artifacts: $ARTIFACT_DIR/"
log ""

cat >"$ARTIFACT_DIR/federation_join_result.json" <<EOF
{
    "timestamp": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
    "room_id": "$ROOM_ID",
    "room_alias": "#$ROOM_ALIAS:$SERVER_A",
    "federated_join_ok": $FEDERATION_JOIN_OK,
    "message_sent": $([[ -n "$EVENT_ID" ]] && echo true || echo false),
    "message_event_id": "$EVENT_ID",
    "message_body": "$MESSAGE_BODY",
    "message_received_on_b": $FEDERATION_MESSAGE_OK,
    "synapse_a_endpoint": "$SYNAPSE_A_BASE",
    "synapse_b_endpoint": "$SYNAPSE_B_BASE",
    "synapse_a_server_name": "$SERVER_A",
    "synapse_b_server_name": "$SERVER_B",
    "user_a": "@$USER_A:$SERVER_A",
    "user_b": "@$USER_B:$SERVER_B"
}
EOF
success "Saved results to $ARTIFACT_DIR/federation_join_result.json"

echo ""
if [[ "$FEDERATION_JOIN_OK" == "true" && "$FEDERATION_MESSAGE_OK" == "true" && "$FAILURES" -eq 0 ]]; then
    success "Federation interop test PASSED"
    exit 0
fi

error "Federation interop test FAILED (join=$FEDERATION_JOIN_OK, message=$FEDERATION_MESSAGE_OK, failures=$FAILURES)"
exit 1
