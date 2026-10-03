#!/usr/bin/env bash
#
# scripts/federation-test/test_federation_v2v7.sh
#
# 联邦互操作性「里程碑 V-2 / V-7」联调脚本（跨实现：A = synapse-rust，B = 上游真 Synapse）
#
# 与 test_federation.sh（happy-path 冒烟：join + 消息）互补，本脚本聚焦判据文档
# docs/synapse-rust-vs-synapse-comparison.md §18.6 的 V-2、V-7 各项：
#
#   V-2a  A → B 联邦邀请：A 建私有房 → 邀请 B 用户 → B /sync 出现 rooms.invite
#   V-2b  B → A 联邦邀请：B 建房 → 邀请 A 用户 → A /sync 出现 rooms.invite
#   V-2c  knock（rust 入站）：B 签名直连 A 的 POST /_matrix/federation/v1/knock，
#         body 为「裸 knock member event」，断言 200 且 state == "knock"
#   V-2d  MSC4311 严格校验单变量对照（零副作用探针）：
#         strict=false → 400 且报错来自 DAG 字段缺失（无 msc4311 标记）
#         strict=true  → 400 且报错含 msc4311_strict_validation
#   V-7a  客户端 hierarchy 的 allowed_room_ids：restricted 子房 → v1/hierarchy
#         的 self entry 带 allowed_room_ids（含父 space）
#   V-7b  联邦面 hierarchy：对 public space 的 /_matrix/federation/v1/hierarchy
#         返回 {"rooms",...} 且**不得**出现 allowed_room_ids（该字段是客户端专用投影）
#
# 前置条件（见 docs/audit/A5_LIVE_FEDERATION_INTEROP_TESTING.md）：
#   * 两套 compose 栈已启动且健康：
#       docker/federation-test/docker-compose-synapse-a.yml  (客户端 18008 / 联邦 18448)
#       docker/federation-test/docker-compose-synapse-b.yml  (客户端 18009 / 联邦 18449)
#   * server_name 分别为 synapse-a.federation.test / synapse-b.federation.test，
#     由各栈的 nginx 边车在 8448 上终结 TLS，并在 federation_test_net_shared 上提供 DNS 别名。
#   * B 侧 homeserver.yaml 已清空 federation_ip_range_blacklist（私网双向联邦必需）。
#
# 用法:
#   bash scripts/federation-test/test_federation_v2v7.sh
#
# 环境变量开关:
#   RUN_V2D=0  跳过 V-2d（V-2d 需强制重建 A 的 synapse-rust 容器 + 重启其 nginx 边车）
#
set -uo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ARTIFACT_DIR="$ROOT_DIR/artifacts/federation-interop"
COMPOSE_A_DIR="$ROOT_DIR/docker/federation-test"

# Endpoints (Client-Server API, plaintext, host-published)
SYNAPSE_A_BASE="http://localhost:18008"
SYNAPSE_B_BASE="http://localhost:18009"

# Container names
CONTAINER_A="synapse-federation-a"
CONTAINER_B="synapse-federation-b"
NGINX_A="synapse-federation-a-nginx"

SERVER_A="synapse-a.federation.test"
SERVER_B="synapse-b.federation.test"

USER_A="user_a"
USER_B="user_b"
PASSWORD_A="FedTest@123"
PASSWORD_B="FedTest@456"

RUN_V2D="${RUN_V2D:-1}"

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

RUN_TS=$(date +%s)

# ─────────────────────────────────────────────────────────────────────────────
# Result flags (literal true/false for the JSON artifact)
# ─────────────────────────────────────────────────────────────────────────────
V2A_OK=false; V2A_DETAIL=""
V2B_OK=false; V2B_DETAIL=""
V2C_OK=false; V2C_DETAIL=""
V2D_STRICT_FALSE_OK=false; V2D_STRICT_TRUE_OK=false; V2D_DETAIL=""
V7A_OK=false; V7A_DETAIL=""
V7B_OK=false; V7B_NO_ALLOWED_OK=false; V7B_DETAIL=""

# ─────────────────────────────────────────────────────────────────────────────
# Step 0: Health check
# ─────────────────────────────────────────────────────────────────────────────
log "Step 0: Checking instance health..."

check_health() {
    local name="$1" base_url="$2"
    if curl -sf "$base_url/_matrix/client/versions" >/dev/null 2>&1; then
        success "$name is healthy ($base_url)"
        return 0
    fi
    error "$name is not responding ($base_url)"
    return 1
}

check_health "Synapse-A (synapse-rust)" "$SYNAPSE_A_BASE" || {
    error "Synapse-A unreachable. 先启动 A 栈（会重建 rust 镜像）："
    error "  cd docker/federation-test && docker compose --env-file .env.a -f docker-compose-synapse-a.yml up -d"
    exit 1
}
check_health "Synapse-B (upstream Synapse)" "$SYNAPSE_B_BASE" || {
    error "Synapse-B unreachable. 先启动 B 栈："
    error "  cd docker/federation-test && docker compose --env-file .env.b -f docker-compose-synapse-b.yml up -d"
    exit 1
}

echo ""

# ─────────────────────────────────────────────────────────────────────────────
# Step 1: Register + login test users
# ─────────────────────────────────────────────────────────────────────────────
log "Step 1: Registering / logging in test users..."

register_user() {
    local container="$1" base_url="$2" username="$3" password="$4"
    local secret nonce mac response errcode

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

    mac=$(printf '%s\0%s\0%s\0admin\0\0\0' "$nonce" "$username" "$password" |
        openssl dgst -sha256 -hmac "$secret" -r | awk '{print $1}')

    response=$(curl -s -X POST "$base_url/_synapse/admin/v1/register" \
        -H "Content-Type: application/json" \
        -d "{\"nonce\":\"$nonce\",\"username\":\"$username\",\"password\":\"$password\",\"admin\":true,\"mac\":\"$mac\"}")

    if echo "$response" | jq -e '.user_id' >/dev/null 2>&1; then
        success "Registered $username on $container"
        return 0
    fi
    errcode=$(echo "$response" | jq -r '.errcode // empty' 2>/dev/null)
    if [[ "$errcode" == "M_USER_IN_USE" ]]; then
        log "$username already exists on $container (reusing)"
        return 0
    fi
    warn "Registration for $username failed: $(echo "$response" | jq -c . 2>/dev/null)"
    return 1
}

register_user_synapse() {
    local container="$1" username="$2" password="$3" out
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

login_user() {
    local base_url="$1" username="$2" password="$3"
    curl -s -X POST "$base_url/_matrix/client/v3/login" \
        -H "Content-Type: application/json" \
        -d "{\"type\":\"m.login.password\",\"identifier\":{\"type\":\"m.id.user\",\"user\":\"$username\"},\"password\":\"$password\"}" |
        jq -r '.access_token // empty'
}

register_user "$CONTAINER_A" "$SYNAPSE_A_BASE" "$USER_A" "$PASSWORD_A" || warn "continuing; will attempt login"
register_user_synapse "$CONTAINER_B" "$USER_B" "$PASSWORD_B" || warn "continuing; will attempt login"

TOKEN_A=$(login_user "$SYNAPSE_A_BASE" "$USER_A" "$PASSWORD_A")
TOKEN_B=$(login_user "$SYNAPSE_B_BASE" "$USER_B" "$PASSWORD_B")

if [[ -z "$TOKEN_A" ]]; then error "Failed to login as $USER_A on Synapse-A"; exit 1; fi
if [[ -z "$TOKEN_B" ]]; then error "Failed to login as $USER_B on Synapse-B"; exit 1; fi
success "Logged in: @$USER_A:$SERVER_A , @$USER_B:$SERVER_B"

USER_ID_A="@$USER_A:$SERVER_A"
USER_ID_B="@$USER_B:$SERVER_B"

echo ""

# ─────────────────────────────────────────────────────────────────────────────
# Helpers
# ─────────────────────────────────────────────────────────────────────────────

# poll_sync_invite <base_url> <token> <room_id> [attempts]
# Returns 0 once /sync shows rooms.invite[room_id].
poll_sync_invite() {
    local base_url="$1" token="$2" room_id="$3" attempts="${4:-15}" resp
    local i
    for ((i = 0; i < attempts; i++)); do
        resp=$(curl -s "$base_url/_matrix/client/v3/sync?timeout=0" -H "Authorization: Bearer $token")
        if echo "$resp" | jq -e --arg r "$room_id" '.rooms.invite[$r] != null' >/dev/null 2>&1; then
            return 0
        fi
        sleep 2
    done
    return 1
}

# a_signed_request / b_signed_request <method> <uri> <body|""> <out_file>
# 在目标容器内用 signedjson 签名（宿主缺 canonicaljson/signedjson），再经 nginx TLS 打到对端。
# 输出文件首行 `STATUS <code>`，其余为响应体。
b_signed_request() { _signed_request "$CONTAINER_B" "$SERVER_B" "$SERVER_A" "$1" "$2" "$3" "$4"; }
a_signed_request() { _signed_request "$CONTAINER_A" "$SERVER_A" "$SERVER_B" "$1" "$2" "$3" "$4"; }

_signed_request() {
    local container="$1" origin="$2" destination="$3"
    local method="$4" uri="$5" body="$6" out_file="$7"
    local body_b64
    body_b64=$(printf '%s' "$body" | base64 | tr -d '\n')

    docker exec -i \
        -e REQ_METHOD="$method" -e REQ_URI="$uri" -e REQ_BODY_B64="$body_b64" \
        -e REQ_ORIGIN="$origin" -e REQ_DEST="$destination" \
        -e REQ_TARGET="https://$destination:8448" \
        "$container" python3 - >"$out_file" 2>"${out_file}.err" <<'PY'
import os, json, ssl, base64, time, urllib.request, urllib.error

method = os.environ["REQ_METHOD"]
uri = os.environ["REQ_URI"]
origin = os.environ["REQ_ORIGIN"]
destination = os.environ["REQ_DEST"]
target = os.environ["REQ_TARGET"]
body_b64 = os.environ.get("REQ_BODY_B64", "").strip()

content = None
if body_b64:
    raw = base64.b64decode(body_b64)
    if raw:
        content = json.loads(raw.decode("utf-8"))

try:
    from signedjson.key import read_signing_keys
    from signedjson.sign import sign_json
    with open("/data/signing.key") as fh:
        key = list(read_signing_keys(fh))[0]
    key_id = "%s:%s" % (key.alg, key.version)

    obj = {"method": method, "uri": uri, "origin": origin, "destination": destination}
    if content is not None:
        obj["content"] = content
    signed = sign_json(obj, origin, key)
    sig = signed["signatures"][origin][key_id]
    ts = int(time.time() * 1000)
    auth = 'X-Matrix origin="%s",destination="%s",key="%s",sig="%s",ts=%d' % (
        origin, destination, key_id, sig, ts)
except Exception as e:
    print("STATUS 0")
    print("SIGNING_ERROR %r" % (e,))
    raise SystemExit(0)

data = json.dumps(content).encode("utf-8") if content is not None else None
req = urllib.request.Request(target + uri, data=data, method=method)
req.add_header("Authorization", auth)
if data is not None:
    req.add_header("Content-Type", "application/json")

ctx = ssl.create_default_context(cafile="/certs/ca.crt")
try:
    with urllib.request.urlopen(req, context=ctx, timeout=30) as resp:
        print("STATUS %d" % resp.status)
        print(resp.read().decode("utf-8", "replace"))
except urllib.error.HTTPError as e:
    print("STATUS %d" % e.code)
    print(e.read().decode("utf-8", "replace"))
except Exception as e:
    print("STATUS 0")
    print("ERROR %r" % (e,))
PY
}

req_status() { head -n 1 "$1" 2>/dev/null | awk '{print $2}'; }
req_body() { tail -n +2 "$1" 2>/dev/null; }

wait_healthy_a() {
    local i
    for ((i = 0; i < 60; i++)); do
        if curl -sf "$SYNAPSE_A_BASE/_matrix/client/versions" >/dev/null 2>&1; then
            return 0
        fi
        sleep 2
    done
    return 1
}

# ─────────────────────────────────────────────────────────────────────────────
# V-2a: A → B federated invite
# ─────────────────────────────────────────────────────────────────────────────
log "V-2a: A creates a private room and invites @$USER_B:$SERVER_B ..."

ROOM_V2A=""
CREATE_V2A=$(curl -s -X POST "$SYNAPSE_A_BASE/_matrix/client/v3/createRoom" \
    -H "Authorization: Bearer $TOKEN_A" -H "Content-Type: application/json" \
    -d "{\"name\":\"V2a invite $RUN_TS\",\"preset\":\"private_chat\"}")
ROOM_V2A=$(echo "$CREATE_V2A" | jq -r '.room_id // empty')

if [[ -z "$ROOM_V2A" ]]; then
    error "V-2a: room creation failed: $(echo "$CREATE_V2A" | jq -c . 2>/dev/null)"
    V2A_DETAIL="room_create_failed"
else
    INV_V2A=$(curl -s -X POST "$SYNAPSE_A_BASE/_matrix/client/v3/rooms/$ROOM_V2A/invite" \
        -H "Authorization: Bearer $TOKEN_A" -H "Content-Type: application/json" \
        -d "{\"user_id\":\"$USER_ID_B\"}")
    if echo "$INV_V2A" | jq -e '.errcode' >/dev/null 2>&1; then
        error "V-2a: invite rejected by A: $(echo "$INV_V2A" | jq -c .)"
        V2A_DETAIL="invite_rejected"
    elif poll_sync_invite "$SYNAPSE_B_BASE" "$TOKEN_B" "$ROOM_V2A"; then
        success "V-2a: B /sync shows the invite for $ROOM_V2A"
        V2A_OK=true
        V2A_DETAIL="ok"
    else
        error "V-2a: B /sync never showed the invite for $ROOM_V2A"
        V2A_DETAIL="invite_not_observed_on_b"
    fi
fi

echo ""

# ─────────────────────────────────────────────────────────────────────────────
# V-2b: B → A federated invite (upstream Synapse → synapse-rust)
# ─────────────────────────────────────────────────────────────────────────────
log "V-2b: B (upstream Synapse) creates a room and invites @$USER_A:$SERVER_A ..."

ROOM_V2B=""
CREATE_V2B=$(curl -s -X POST "$SYNAPSE_B_BASE/_matrix/client/v3/createRoom" \
    -H "Authorization: Bearer $TOKEN_B" -H "Content-Type: application/json" \
    -d "{\"name\":\"V2b invite $RUN_TS\",\"preset\":\"private_chat\"}")
ROOM_V2B=$(echo "$CREATE_V2B" | jq -r '.room_id // empty')

if [[ -z "$ROOM_V2B" ]]; then
    error "V-2b: room creation on B failed: $(echo "$CREATE_V2B" | jq -c . 2>/dev/null)"
    V2B_DETAIL="room_create_failed"
else
    INV_V2B=$(curl -s -X POST "$SYNAPSE_B_BASE/_matrix/client/v3/rooms/$ROOM_V2B/invite" \
        -H "Authorization: Bearer $TOKEN_B" -H "Content-Type: application/json" \
        -d "{\"user_id\":\"$USER_ID_A\"}")
    if echo "$INV_V2B" | jq -e '.errcode' >/dev/null 2>&1; then
        error "V-2b: invite rejected by B: $(echo "$INV_V2B" | jq -c .)"
        V2B_DETAIL="invite_rejected"
    elif poll_sync_invite "$SYNAPSE_A_BASE" "$TOKEN_A" "$ROOM_V2B"; then
        success "V-2b: A /sync shows the invite for $ROOM_V2B"
        V2B_OK=true
        V2B_DETAIL="ok"
    else
        error "V-2b: A /sync never showed the invite for $ROOM_V2B"
        V2B_DETAIL="invite_not_observed_on_a"
    fi
fi

echo ""

# ─────────────────────────────────────────────────────────────────────────────
# V-2c: knock, inbound to synapse-rust (B signs a raw knock member event)
# ─────────────────────────────────────────────────────────────────────────────
log "V-2c: A creates a knock room, invites B user as anchor, then B knocks ..."

ROOM_V2C=""
CREATE_V2C=$(curl -s -X POST "$SYNAPSE_A_BASE/_matrix/client/v3/createRoom" \
    -H "Authorization: Bearer $TOKEN_A" -H "Content-Type: application/json" \
    -d "{\"name\":\"V2c knock $RUN_TS\",\"preset\":\"private_chat\",\"initial_state\":[{\"type\":\"m.room.join_rules\",\"state_key\":\"\",\"content\":{\"join_rule\":\"knock\"}}]}")
ROOM_V2C=$(echo "$CREATE_V2C" | jq -r '.room_id // empty')

if [[ -z "$ROOM_V2C" ]]; then
    error "V-2c: room creation failed: $(echo "$CREATE_V2C" | jq -c . 2>/dev/null)"
    V2C_DETAIL="room_create_failed"
else
    # anchor: invite makes @user_b's server a non-banned member → observe check passes.
    curl -s -X POST "$SYNAPSE_A_BASE/_matrix/client/v3/rooms/$ROOM_V2C/invite" \
        -H "Authorization: Bearer $TOKEN_A" -H "Content-Type: application/json" \
        -d "{\"user_id\":\"$USER_ID_B\"}" >/dev/null

    KNOCK_TS=$(( $(date +%s) * 1000 ))
    KNOCK_BODY=$(jq -nc \
        --arg room "$ROOM_V2C" --arg sender "$USER_ID_B" \
        --arg origin "$SERVER_B" --argjson ts "$KNOCK_TS" \
        '{type:"m.room.member",room_id:$room,sender:$sender,state_key:$sender,origin:$origin,origin_server_ts:$ts,content:{membership:"knock"}}')
    KNOCK_URI="/_matrix/federation/v1/knock/$ROOM_V2C/$USER_ID_B"

    OUT_V2C="$ARTIFACT_DIR/v2c_knock.out"
    b_signed_request POST "$KNOCK_URI" "$KNOCK_BODY" "$OUT_V2C"
    ST_V2C=$(req_status "$OUT_V2C")
    BODY_V2C=$(req_body "$OUT_V2C")

    if [[ "$ST_V2C" == "200" ]] && echo "$BODY_V2C" | jq -e '.state == "knock"' >/dev/null 2>&1; then
        success "V-2c: A accepted the inbound knock (200, state=knock)"
        V2C_OK=true
        V2C_DETAIL="ok"
    else
        error "V-2c: knock failed (status=$ST_V2C): $(echo "$BODY_V2C" | jq -c . 2>/dev/null || echo "$BODY_V2C")"
        V2C_DETAIL="status_${ST_V2C}"
    fi
fi

echo ""

# ─────────────────────────────────────────────────────────────────────────────
# V-7a: client /hierarchy annotates allowed_room_ids for a restricted child room
# ─────────────────────────────────────────────────────────────────────────────
log "V-7a: A creates a public space + restricted child room, asserts allowed_room_ids ..."

SPACE_ID=""
CHILD_ID=""
CREATE_SPACE=$(curl -s -X POST "$SYNAPSE_A_BASE/_matrix/client/v3/createRoom" \
    -H "Authorization: Bearer $TOKEN_A" -H "Content-Type: application/json" \
    -d "{\"name\":\"V7 space $RUN_TS\",\"visibility\":\"public\",\"creation_content\":{\"type\":\"m.space\"}}")
SPACE_ID=$(echo "$CREATE_SPACE" | jq -r '.room_id // empty')

if [[ -z "$SPACE_ID" ]]; then
    error "V-7a: space creation failed: $(echo "$CREATE_SPACE" | jq -c . 2>/dev/null)"
    V7A_DETAIL="space_create_failed"
else
    success "V-7a: created public space $SPACE_ID"
    CREATE_CHILD=$(curl -s -X POST "$SYNAPSE_A_BASE/_matrix/client/v3/createRoom" \
        -H "Authorization: Bearer $TOKEN_A" -H "Content-Type: application/json" \
        -d "{\"name\":\"V7 child $RUN_TS\",\"initial_state\":[{\"type\":\"m.room.join_rules\",\"state_key\":\"\",\"content\":{\"join_rule\":\"restricted\",\"allow\":[{\"type\":\"m.room_membership\",\"room_id\":\"$SPACE_ID\"}]}}]}")
    CHILD_ID=$(echo "$CREATE_CHILD" | jq -r '.room_id // empty')

    if [[ -z "$CHILD_ID" ]]; then
        error "V-7a: child room creation failed: $(echo "$CREATE_CHILD" | jq -c . 2>/dev/null)"
        V7A_DETAIL="child_create_failed"
    else
        HIER_V7A="$ARTIFACT_DIR/v7a_client_hierarchy.json"
        curl -s "$SYNAPSE_A_BASE/_matrix/client/v1/rooms/$CHILD_ID/hierarchy" \
            -H "Authorization: Bearer $TOKEN_A" >"$HIER_V7A"

        if jq -e --arg child "$CHILD_ID" --arg space "$SPACE_ID" \
            '.rooms[] | select(.room_id == $child) | (.allowed_room_ids // []) | index($space) != null' \
            "$HIER_V7A" >/dev/null 2>&1; then
            success "V-7a: child entry carries allowed_room_ids containing the space"
            V7A_OK=true
            V7A_DETAIL="ok"
        else
            error "V-7a: allowed_room_ids missing/incorrect on child entry"
            jq -c '{rooms: [.rooms[] | {room_id, join_rule, allowed_room_ids}]}' "$HIER_V7A" 2>/dev/null || warn "$(head -c 400 "$HIER_V7A")"
            V7A_DETAIL="allowed_room_ids_missing"
        fi
    fi
fi

echo ""

# ─────────────────────────────────────────────────────────────────────────────
# V-7b: federation /hierarchy on the public space — must NOT expose allowed_room_ids
# ─────────────────────────────────────────────────────────────────────────────
log "V-7b: B signs GET /_matrix/federation/v1/hierarchy/$SPACE_ID ..."

if [[ -z "$SPACE_ID" ]]; then
    warn "V-7b: skipped (no space from V-7a)"
    V7B_DETAIL="skipped_no_space"
else
    OUT_V7B="$ARTIFACT_DIR/v7b_federation_hierarchy.json.raw"
    b_signed_request GET "/_matrix/federation/v1/hierarchy/$SPACE_ID" "" "$OUT_V7B"
    ST_V7B=$(req_status "$OUT_V7B")
    BODY_V7B=$(req_body "$OUT_V7B")
    echo "$BODY_V7B" >"$ARTIFACT_DIR/v7b_federation_hierarchy.json"

    if [[ "$ST_V7B" == "200" ]] && echo "$BODY_V7B" | jq -e 'has("rooms")' >/dev/null 2>&1; then
        success "V-7b: federation hierarchy returned 200 with a rooms array"
        V7B_OK=true
        if echo "$BODY_V7B" | jq -e '[.rooms[]? | has("allowed_room_ids")] | any' >/dev/null 2>&1; then
            error "V-7b: federation hierarchy leaked allowed_room_ids (client-only projection)"
            V7B_DETAIL="leaked_allowed_room_ids"
        else
            success "V-7b: no allowed_room_ids key present (as expected)"
            V7B_NO_ALLOWED_OK=true
            V7B_DETAIL="ok"
        fi
    else
        error "V-7b: federation hierarchy failed (status=$ST_V7B): $(echo "$BODY_V7B" | jq -c . 2>/dev/null || echo "$BODY_V7B")"
        V7B_DETAIL="status_${ST_V7B}"
    fi
fi

echo ""

# ─────────────────────────────────────────────────────────────────────────────
# V-2d: MSC4311 strict-validation single-variable probe (zero side effects)
# ─────────────────────────────────────────────────────────────────────────────
if [[ "$RUN_V2D" != "1" ]]; then
    warn "V-2d: skipped (RUN_V2D=$RUN_V2D)"
    V2D_DETAIL="skipped"
else
    log "V-2d: MSC4311 probe — strict=false vs strict=true ..."

    PROBE_ROOM="!msc4311probe:$SERVER_A"
    PROBE_EVENT_ID='$msc4311probe'
    PROBE_URI="/_matrix/federation/v2/invite/$PROBE_ROOM/$PROBE_EVENT_ID"
    PROBE_TS=$(( $(date +%s) * 1000 ))

    # deliberately omits depth/prev_events/auth_events so the non-strict path
    # fails at the DAG guard (invite_v2 line ~176), *after* the MSC4311 shape check.
    PROBE_BODY=$(jq -nc \
        --arg room "$PROBE_ROOM" --arg sender "$USER_ID_B" \
        --arg state_key "$USER_ID_A" --arg origin "$SERVER_B" \
        --argjson ts "$PROBE_TS" \
        '{event:{type:"m.room.member",room_id:$room,sender:$sender,state_key:$state_key,origin:$origin,origin_server_ts:$ts,content:{membership:"invite"}},room_version:"12",invite_room_state:[{type:"m.room.create",state_key:"",content:{creator:$state_key}}]}')

    # --- strict=false (default) ---
    OUT_V2D_F="$ARTIFACT_DIR/v2d_strict_false.out"
    b_signed_request PUT "$PROBE_URI" "$PROBE_BODY" "$OUT_V2D_F"
    ST_F=$(req_status "$OUT_V2D_F")
    BODY_F=$(req_body "$OUT_V2D_F")
    if [[ "$ST_F" == "400" ]] && echo "$BODY_F" | grep -q "depth/prev_events/auth_events" \
        && ! echo "$BODY_F" | grep -q "msc4311_strict_validation"; then
        success "V-2d: strict=false rejected at the DAG guard (no MSC4311 marker)"
        V2D_STRICT_FALSE_OK=true
    else
        error "V-2d: strict=false unexpected (status=$ST_F): $(echo "$BODY_F" | head -c 300)"
    fi

    # --- strict=true ---
    log "V-2d: recreating Synapse-A with MSC4311_STRICT_VALIDATION=true ..."
    if (cd "$COMPOSE_A_DIR" && MSC4311_STRICT_VALIDATION=true docker compose --env-file .env.a \
        -f docker-compose-synapse-a.yml up -d --force-recreate --no-deps synapse-rust) >/dev/null 2>&1; then
        docker restart "$NGINX_A" >/dev/null 2>&1 || warn "V-2d: could not restart $NGINX_A"
        if wait_healthy_a; then
            success "V-2d: Synapse-A restarted with strict=true"
        else
            error "V-2d: Synapse-A did not become healthy after strict=true restart"
        fi
    else
        error "V-2d: failed to recreate Synapse-A with strict=true"
    fi

    OUT_V2D_T="$ARTIFACT_DIR/v2d_strict_true.out"
    b_signed_request PUT "$PROBE_URI" "$PROBE_BODY" "$OUT_V2D_T"
    ST_T=$(req_status "$OUT_V2D_T")
    BODY_T=$(req_body "$OUT_V2D_T")
    if [[ "$ST_T" == "400" ]] && echo "$BODY_T" | grep -q "msc4311_strict_validation"; then
        success "V-2d: strict=true rejected at the MSC4311 shape check"
        V2D_STRICT_TRUE_OK=true
    else
        error "V-2d: strict=true unexpected (status=$ST_T): $(echo "$BODY_T" | head -c 300)"
    fi

    # --- restore default (strict=false) ---
    log "V-2d: restoring Synapse-A to default (strict=false) ..."
    if (cd "$COMPOSE_A_DIR" && docker compose --env-file .env.a \
        -f docker-compose-synapse-a.yml up -d --force-recreate --no-deps synapse-rust) >/dev/null 2>&1; then
        docker restart "$NGINX_A" >/dev/null 2>&1 || true
        wait_healthy_a || warn "V-2d: Synapse-A health not confirmed after restore"
    else
        warn "V-2d: could not restore Synapse-A defaults — manual recreate recommended"
    fi

    if [[ "$V2D_STRICT_FALSE_OK" == "true" && "$V2D_STRICT_TRUE_OK" == "true" ]]; then
        V2D_DETAIL="ok"
    else
        V2D_DETAIL="strict_false=$V2D_STRICT_FALSE_OK strict_true=$V2D_STRICT_TRUE_OK"
    fi
fi

echo ""

# ─────────────────────────────────────────────────────────────────────────────
# Results
# ─────────────────────────────────────────────────────────────────────────────
log "=========================================="
log "  V-2 / V-7 Federation Interop Summary"
log "=========================================="
log "  V-2a (A→B invite):            $V2A_OK   ($V2A_DETAIL)"
log "  V-2b (B→A invite):            $V2B_OK   ($V2B_DETAIL)"
log "  V-2c (inbound knock):         $V2C_OK   ($V2C_DETAIL)"
log "  V-2d (MSC4311 strict switch): $V2D_DETAIL"
log "  V-7a (client allowed_room_ids): $V7A_OK   ($V7A_DETAIL)"
log "  V-7b (federation hierarchy):    $V7B_OK / no_allowed_room_ids=$V7B_NO_ALLOWED_OK   ($V7B_DETAIL)"
log "  Artifacts: $ARTIFACT_DIR/"
log ""

RESULT_JSON="$ARTIFACT_DIR/v2v7_result.json"
jq -n \
    --arg ts "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" \
    --argjson v2a "$V2A_OK" --arg v2a_detail "$V2A_DETAIL" \
    --argjson v2b "$V2B_OK" --arg v2b_detail "$V2B_DETAIL" \
    --argjson v2c "$V2C_OK" --arg v2c_detail "$V2C_DETAIL" \
    --argjson v2d_strict_false "$V2D_STRICT_FALSE_OK" \
    --argjson v2d_strict_true "$V2D_STRICT_TRUE_OK" \
    --arg v2d_detail "$V2D_DETAIL" \
    --argjson v7a "$V7A_OK" --arg v7a_detail "$V7A_DETAIL" \
    --argjson v7b "$V7B_OK" --argjson v7b_no_allowed "$V7B_NO_ALLOWED_OK" --arg v7b_detail "$V7B_DETAIL" \
    --arg room_v2a "$ROOM_V2A" --arg room_v2b "$ROOM_V2B" --arg room_v2c "$ROOM_V2C" \
    --arg space "$SPACE_ID" --arg child "$CHILD_ID" \
    --arg server_a "$SERVER_A" --arg server_b "$SERVER_B" \
    --argjson failures "$FAILURES" \
    '{
        timestamp: $ts,
        v2a_a_to_b_invite: {ok: $v2a, detail: $v2a_detail, room_id: $room_v2a},
        v2b_b_to_a_invite: {ok: $v2b, detail: $v2b_detail, room_id: $room_v2b},
        v2c_inbound_knock: {ok: $v2c, detail: $v2c_detail, room_id: $room_v2c},
        v2d_msc4311_strict: {
            strict_false_ok: $v2d_strict_false,
            strict_true_ok: $v2d_strict_true,
            detail: $v2d_detail
        },
        v7a_client_allowed_room_ids: {ok: $v7a, detail: $v7a_detail, space_id: $space, child_room_id: $child},
        v7b_federation_hierarchy: {ok: $v7b, no_allowed_room_ids: $v7b_no_allowed, detail: $v7b_detail},
        synapse_a_server_name: $server_a,
        synapse_b_server_name: $server_b,
        failures: $failures
    }' >"$RESULT_JSON"
success "Saved results to $RESULT_JSON"

echo ""

V2D_PASS=true
if [[ "$RUN_V2D" == "1" ]]; then
    [[ "$V2D_STRICT_FALSE_OK" == "true" && "$V2D_STRICT_TRUE_OK" == "true" ]] || V2D_PASS=false
fi

if [[ "$V2A_OK" == "true" && "$V2B_OK" == "true" && "$V2C_OK" == "true" \
    && "$V7A_OK" == "true" && "$V7B_OK" == "true" && "$V7B_NO_ALLOWED_OK" == "true" \
    && "$V2D_PASS" == "true" && "$FAILURES" -eq 0 ]]; then
    success "V-2 / V-7 federation interop test PASSED"
    exit 0
fi

error "V-2 / V-7 federation interop test FAILED (failures=$FAILURES, RUN_V2D=$RUN_V2D)"
exit 1
