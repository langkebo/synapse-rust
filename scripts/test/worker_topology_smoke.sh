#!/usr/bin/env bash
# =============================================================================
# 多进程拓扑端到端烟测（W-D）
# =============================================================================
# 这是"水平扩展到底能不能跑"的**唯一证据来源**：起 master + 6 个 worker 进程，
# 断言它们真的注册并心跳（而不是只有配置写着），再走一遍 client 面（建房间 →
# 发消息 → /sync 能读回）。
#
# 用法：
#   scripts/test/worker_topology_smoke.sh              # 构建镜像 + 起栈 + 断言 + 收栈
#   SKIP_BUILD=1 scripts/test/worker_topology_smoke.sh # 复用已有镜像（迭代时用）
#   KEEP_STACK=1 scripts/test/worker_topology_smoke.sh # 不清栈，便于事后看 docker logs
#
# 退出码：0 = 全部断言通过；非 0 = 任一步失败（失败处会打印它期望什么、实际看到什么）。
#
# ⚠️ 一处**故意与文档不同**的地方（勿当成漏做）：
#   `docs/audit/WORKER_TOPOLOGY_VERIFICATION_AUDIT_2026-10-16.md` §4 W-D 第 3 条原本要求
#   「断言 /sync 由 sync worker 返回」。**今天做不到**：worker 进程只 mount 了
#   `/metrics`（`src/bin/synapse_worker.rs`），不提供 Matrix API；且没有任何生产代码按
#   `owned_route_prefixes` 决定谁 serve 什么。本脚本因此断言的是**能成立的那部分**：
#   /sync 与消息都由 master 提供，而 worker 的"在场"由注册表与心跳证明。
#   要断言分流，先要有 W-F（按角色收敛 HTTP 面）。
# =============================================================================

set -euo pipefail

readonly ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly COMPOSE_BASE="$ROOT_DIR/docker/docker-compose.yml"
readonly COMPOSE_WORKERS="$ROOT_DIR/docker/docker-compose.workers.yml"
readonly PROJECT_NAME="${COMPOSE_PROJECT_NAME:-synapse}"
readonly SYNAPSE_PORT="${SYNAPSE_PORT:-8008}"
readonly BASE_URL="http://127.0.0.1:${SYNAPSE_PORT}"

# 必须与 docker/config/homeserver.yaml 的 worker.instance_map 以及
# docker/docker-compose.workers.yml 的服务名一致 —— 由
# tests/unit/worker_topology_deployment_tests.rs 离线守住这三者的同步。
readonly DECLARED_INSTANCES=(
    client_reader
    event_persister
    sync_worker
    federation_reader
    media_repository
)

readonly HEALTH_TIMEOUT_SECS="${HEALTH_TIMEOUT_SECS:-180}"
readonly REGISTRATION_TIMEOUT_SECS="${REGISTRATION_TIMEOUT_SECS:-120}"

failures=0

log() { printf '\n== %s ==\n' "$*"; }
ok() { printf 'OK   %s\n' "$*"; }
skip() { printf 'SKIP %s\n' "$*"; }

fail() {
    printf 'FAIL %s\n' "$*" >&2
    failures=$((failures + 1))
}

assert_eq() { # assert_eq <实际> <期望> <说明>
    if [[ "$1" == "$2" ]]; then
        ok "$3"
    else
        fail "$3 —— 期望 [$2]，实际 [$1]"
    fi
}

compose() {
    docker compose -f "$COMPOSE_BASE" -f "$COMPOSE_WORKERS" "$@"
}

teardown() {
    if [[ "${KEEP_STACK:-0}" == "1" ]]; then
        skip "KEEP_STACK=1 ⇒ 栈保留；清理：docker compose -f docker/docker-compose.yml -f docker/docker-compose.workers.yml down"
        return
    fi
    log "收栈"
    # 不带 -v：保留 db/redis 数据卷，脚本可以反复跑而不必每次重建数据。
    compose down --remove-orphans >/dev/null 2>&1 || true
}

require_commands() {
    local missing=0
    for cmd in docker curl python3; do
        if ! command -v "$cmd" >/dev/null 2>&1; then
            fail "缺少命令 $cmd"
            missing=1
        fi
    done
    if ! docker compose version >/dev/null 2>&1; then
        fail "docker compose (v2 插件) 不可用"
        missing=1
    fi
    [[ "$missing" == "0" ]]
}

require_env_file() {
    if [[ ! -f "$ROOT_DIR/docker/.env" ]]; then
        fail "docker/.env 不存在 —— 先从 docker/.env.example 复制并填好 DB_PASSWORD / REDIS_PASSWORD / WORKER_REPLICATION_SECRET 等必填项"
        return 1
    fi
    # 这些键必须是**非占位**的值，否则 master 起不来或 worker 不心跳。
    local key
    for key in DB_PASSWORD REDIS_PASSWORD WORKER_REPLICATION_SECRET; do
        if ! grep -qE "^${key}=.+" "$ROOT_DIR/docker/.env"; then
            fail "docker/.env 里缺少 ${key}"
            return 1
        fi
    done
    return 0
}

env_value() { # env_value <KEY> —— 从 docker/.env 读一个值（不 export 到日志里）
    grep -E "^$1=" "$ROOT_DIR/docker/.env" | head -n1 | cut -d= -f2- | tr -d '\r"'
}

wait_for_health() {
    log "等待 master 健康（最多 ${HEALTH_TIMEOUT_SECS}s）"
    local deadline=$((SECONDS + HEALTH_TIMEOUT_SECS))
    while ((SECONDS < deadline)); do
        if curl -fsS "$BASE_URL/health" >/dev/null 2>&1; then
            ok "master /health 已就绪"
            return 0
        fi
        sleep 3
    done
    fail "master 在 ${HEALTH_TIMEOUT_SECS}s 内没有就绪"
    compose ps || true
    return 1
}

assert_offline_topology_check() {
    log "master 容器内跑部署前离线校验（W-B）"
    local exit_code=0
    compose exec -T synapse-rust /app/synapse-rust --validate-topology || exit_code=$?
    assert_eq "$exit_code" "0" "容器内 --validate-topology 退出码"
}

admin_token() { # 走 W-B 无关的既有管理员引导流程（nonce + HMAC-SHA256）
    local admin_secret="${ADMIN_SECRET:-$(env_value ADMIN_SECRET)}"
    if [[ -z "$admin_secret" ]]; then
        fail "docker/.env 里没有 ADMIN_SECRET，无法建管理员令牌"
        return 1
    fi

    local nonce
    nonce="$(curl -fsS "$BASE_URL/_synapse/admin/v1/register/nonce" |
        python3 -c 'import json,sys; print(json.load(sys.stdin).get("nonce",""))' 2>/dev/null || true)"
    if [[ -z "$nonce" ]]; then
        fail "拿不到管理员注册 nonce"
        return 1
    fi

    local username="smoke_admin_$(date +%s)_$$"
    local password="smoke-$(openssl rand -hex 12)"
    local mac
    mac="$(
        python3 - "$nonce" "$username" "$password" "$admin_secret" <<'PY'
import hashlib, hmac, sys
nonce, username, password, secret = sys.argv[1:5]
message = nonce.encode() + b"\x00" + username.encode() + b"\x00" + password.encode() + b"\x00" + b"admin\x00\x00\x00"
print(hmac.new(secret.encode(), message, hashlib.sha256).hexdigest())
PY
    )"

    local response
    response="$(curl -fsS -X POST "$BASE_URL/_synapse/admin/v1/register" \
        -H 'Content-Type: application/json' \
        -d "{\"nonce\":\"$nonce\",\"username\":\"$username\",\"password\":\"$password\",\"admin\":true,\"mac\":\"$mac\"}")" || {
        fail "管理员注册请求失败"
        return 1
    }
    python3 -c 'import json,sys; print(json.load(sys.stdin).get("access_token",""))' <<<"$response"
}

assert_topology_endpoint() { # assert_topology_endpoint <admin_token>
    local token="$1"
    log "等待所有声明的实例注册并心跳（最多 ${REGISTRATION_TIMEOUT_SECS}s）"
    local deadline=$((SECONDS + REGISTRATION_TIMEOUT_SECS))
    local body=""
    while ((SECONDS < deadline)); do
        body="$(curl -fsS -H "Authorization: Bearer $token" "$BASE_URL/_synapse/worker/v1/topology/validate" 2>/dev/null || true)"
        if [[ -n "$body" ]] &&
            python3 -c 'import json,sys; d=json.load(sys.stdin); sys.exit(0 if d.get("runtime",{}).get("missing")==[] else 1)' <<<"$body" 2>/dev/null; then
            break
        fi
        sleep 3
    done

    if [[ -z "$body" ]]; then
        fail "GET /_synapse/worker/v1/topology/validate 一直没有返回可解析的响应"
        return 1
    fi

    python3 - "$body" "${DECLARED_INSTANCES[@]}" <<'PY' || failures=$((failures + 1))
import json, sys
body = json.loads(sys.argv[1])
declared = sys.argv[2:]
runtime = body.get("runtime", {})
live = runtime.get("live_instances", [])
missing = runtime.get("missing", [])
problems = []
if not body.get("validation", {}).get("valid"):
    problems.append(f"静态校验未通过: {body.get('validation', {}).get('errors')}")
if runtime.get("valid") is not True:
    problems.append(f"runtime.valid 不是 true: {runtime.get('errors')}")
if missing:
    problems.append(f"有已声明但没跑起来的实例: {missing}")
never_seen = [name for name in declared if name not in live]
if never_seen:
    problems.append(f"声明了但注册表里从未出现: {never_seen}")
if problems:
    print("FAIL 拓扑端点断言 —— " + "; ".join(problems), file=sys.stderr)
    sys.exit(1)
print(f"OK   拓扑端点：static valid + runtime.valid，live={sorted(live)}，missing=[]")
PY
}

assert_client_surface() { # assert_client_surface <admin_token>
    local token="$1"
    log "client 面走一遍：建房间 → 发消息 → /sync 读回"

    local room_id
    room_id="$(curl -fsS -X POST "$BASE_URL/_matrix/client/v3/createRoom" \
        -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
        -d '{"name":"worker-topology-smoke","preset":"private_chat"}' |
        python3 -c 'import json,sys; print(json.load(sys.stdin).get("room_id",""))')"
    assert_eq "$([[ -n "$room_id" ]] && echo yes || echo no)" "yes" "createRoom 返回 room_id"

    local body="smoke-$(date +%s)"
    local event_id
    event_id="$(curl -fsS -X PUT \
        "$BASE_URL/_matrix/client/v3/rooms/$room_id/send/m.room.message/smoke-$(date +%s)" \
        -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
        -d "{\"msgtype\":\"m.text\",\"body\":\"$body\"}" |
        python3 -c 'import json,sys; print(json.load(sys.stdin).get("event_id",""))')"
    assert_eq "$([[ -n "$event_id" ]] && echo yes || echo no)" "yes" "发消息返回 event_id"

    local sync_body
    sync_body="$(curl -fsS "$BASE_URL/_matrix/client/v3/sync?timeout=0" -H "Authorization: Bearer $token")"
    python3 - "$sync_body" "$room_id" "$body" <<'PY' || failures=$((failures + 1))
import json, sys
sync, room_id, expected = json.loads(sys.argv[1]), sys.argv[2], sys.argv[3]
events = sync.get("rooms", {}).get("join", {}).get(room_id, {}).get("timeline", {}).get("events", [])
bodies = [e.get("content", {}).get("body") for e in events]
if expected not in bodies:
    print(f"FAIL /sync 没读回刚发的消息（房间 {room_id} 的 timeline 里没有它）", file=sys.stderr)
    sys.exit(1)
print("OK   /sync 读回了刚发的事件（由 master 提供 —— 见文件头关于分流的说明）")
PY
}

main() {
    log "环境检查"
    require_commands || return 1
    require_env_file || return 1

    if [[ "${SKIP_BUILD:-0}" == "1" ]]; then
        log "起栈（复用已有镜像）"
        compose up -d >/dev/null
    else
        log "起栈（构建镜像：首次会编译 release 二进制，可能很久）"
        compose up -d --build >/dev/null
    fi

    wait_for_health || return 1
    assert_offline_topology_check
    local token
    if ! token="$(admin_token)"; then
        return 1
    fi
    assert_topology_endpoint "$token"
    assert_client_surface "$token"
    return 0
}

main
status=$?
teardown

log "结论"
if [[ "$failures" -eq 0 && "$status" -eq 0 ]]; then
    echo "全部通过"
    exit 0
fi
echo "有失败项（failures=$failures）" >&2
exit 1
