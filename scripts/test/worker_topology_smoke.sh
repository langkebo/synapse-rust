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
# 隔离：本脚本用专用的 compose 项目名（默认 `synapse-worker-smoke`）与专用端口
# （默认 18008），并在起栈前断言端口空闲 —— 因为 `docker-compose.yml` 的
# `container_name` 写死成 `${COMPOSE_PROJECT_NAME:-synapse}-*`，用默认项目名会与
# **正在运行的生产栈同名**并把它重建掉。详见下面 PROJECT_NAME 处的注释。
#
# 退出码：0 = 全部断言通过；非 0 = 任一步失败（失败处会打印它期望什么、实际看到什么）。
#
# ✅ 曾经的缺口已修（W-G，2026-10-17）：worker 进程**没有自注册路径** —— 唯一入口
#   `POST /_synapse/worker/v1/register` 挂在 admin 鉴权后面（要管理员令牌），而 worker
#   只有共享密钥；而 `update_worker_status` 是 `UPDATE workers ... WHERE worker_id = $1`
#   （非 upsert、且**不检查影响行数**）⇒ 未知 worker 的心跳**静默改 0 行**。
#   现在心跳会带上 worker 自己的身份（`worker_name`/`worker_type`/`host`/`port`），
#   master 在首次心跳时为它建注册行（`synapse-web/src/routes/worker/worker_self_announce.rs`），
#   不动鉴权面、不新增路由 ⇒ 不触发契约链。旧客户端（不带身份）行为不变。
#
# ⚠️ 本脚本踩过、后来者也会踩的两个鉴权细节：
#   1. 管理员注册的密码**必须满足密码策略**（至少一个大写字母）；纯小写十六进制会被
#      400 `M_INVALID_PARAM` 拒掉，表现为"拿不到管理员令牌"。
#   2. 注册管理员**必须带 `user_type`（如 `super_admin`）且它要参与 HMAC**；否则角色
#      归一化成"无角色"，RBAC 会拒掉 `/_synapse/worker/*`（实测 403）。
#
# ── 现在能断言到哪一步（W-H，2026-10-22）──────────────────────────────────────
# 栈里有两种进程（见 docker/docker-compose.workers.yml 头部）：
#   * 4 个 HTTP 角色实例 = 根二进制 + `surface_gate: true`（client_reader / sync_worker /
#     federation_reader / media_repository），各自只答自己角色的路径，其余 404；
#   * 1 个任务消费者 = `synapse_worker`（event_persister），只开 `/metrics`。
#
# 因此本脚本断言三件事：
#   1. 拓扑对账：`runtime.valid` 且 `runtime.missing == []`（五个实例都注册了）；
#   2. **准入是真的**：直连角色实例的端口（只绑 127.0.0.1），
#      `sync_worker` 答 `/sync`（401 而非 404）、对 `createRoom` 返 **404**；
#      `client_reader` 对 `createRoom` 返 **405**（路由存在、只是 GET 不允许）——
#      404 与 405 的差别正是 surface_gate 生效的证据；
#   3. 客户端面仍能建房间 / 发消息 / `/sync` 读回（这一路走 master，证明拓扑没破坏功能）。
#
# 反向代理的分流（`docker/nginx/worker-routing.conf`）不在本脚本里跑：dev 栈没有 nginx
# 服务，那份 conf 属于生产栈；它的 upstream 域名在 nginx 启动时就要解析，脱离
# compose 网络无法 `nginx -t`（真要有 CI 覆盖，得单独起一个挂该 conf 的 nginx 容器）。
# =============================================================================

set -euo pipefail

readonly ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly COMPOSE_BASE="$ROOT_DIR/docker/docker-compose.yml"
readonly COMPOSE_WORKERS="$ROOT_DIR/docker/docker-compose.workers.yml"

# ⚠️ 专用 compose 项目名，**不要**用默认值。默认项目从 compose 文件所在目录推导（`docker`），
# 而 `docker-compose.yml` 里每个 `container_name` 都写死成 `${COMPOSE_PROJECT_NAME:-synapse}-*`
# ⇒ 默认情况下本脚本想要的 `synapse-postgres` / `synapse-redis` 与**正在运行的生产栈同名**，
# `up -d` 会直接把它们**重建掉**（实测本机就有一台在跑的 deploy 栈，容器 `synapse-app`）。
# 换成一个专用项目名后，容器名/网络/数据卷全部独立，脚本不会碰到任何既有栈。
readonly PROJECT_NAME="${COMPOSE_PROJECT_NAME:-synapse-worker-smoke}"
export COMPOSE_PROJECT_NAME="$PROJECT_NAME"

# 同理：不要占用生产端口。8008 通常已被在跑的实例占用，默认换到 18008；
# 启动前还会显式检查端口是否空闲（见 assert_port_free）。
# ⚠️ 与在跑的生产/开发栈**错开网段**：`docker-compose.yml` 的 subnet 是
# `${SYNAPSE_NETWORK_SUBNET:-192.168.97.0/24}`，同网段建第二个栈会直接失败
# （`invalid pool request: Pool overlaps with other one on this address space`）。
# 这个栈本来就是"旁挂"的，必须能和生产栈并存。
export SYNAPSE_NETWORK_SUBNET="${SMOKE_NETWORK_SUBNET:-192.168.98.0/24}"
# 同理：postgres 默认发布到 127.0.0.1:5432，与在跑的栈冲突 ⇒ 旁挂栈换端口。
export DB_HOST_PORT="${SMOKE_DB_HOST_PORT:-15432}"

# ⚠️ 必须 **export**：compose 用 `${SYNAPSE_PORT}` / `${FEDERATION_PORT}` 做主机端口映射，
# 只写成 shell 变量时 compose 读不到 ⇒ 仍然去绑 8008，与在跑的栈撞
# `Bind for 127.0.0.1:8008 failed: port is already allocated`。
export SYNAPSE_PORT="${SYNAPSE_PORT:-18008}"
export FEDERATION_PORT="${FEDERATION_PORT:-28449}"
readonly BASE_URL="http://127.0.0.1:${SYNAPSE_PORT}"
# 角色实例的发布端口（compose 里绑 127.0.0.1，仅供本脚本直连证明准入）。
readonly CLIENT_READER_PORT="${WORKER_CLIENT_READER_PORT:-18009}"
readonly SYNC_WORKER_PORT="${WORKER_SYNC_WORKER_PORT:-18010}"
readonly FEDERATION_READER_PORT="${WORKER_FEDERATION_READER_PORT:-18011}"
readonly MEDIA_REPOSITORY_PORT="${WORKER_MEDIA_REPOSITORY_PORT:-18012}"

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
    docker compose -p "$PROJECT_NAME" -f "$COMPOSE_BASE" -f "$COMPOSE_WORKERS" "$@"
}

# 端口必须先空闲：否则我们要么起不来，要么把自己的请求打到**别人的**实例上，
# 于是断言对象不是这套栈（假绿），或"清理"时把别人的容器一起 `down` 掉。
assert_port_free() {
    if lsof -nP -iTCP:"$SYNAPSE_PORT" -sTCP:LISTEN >/dev/null 2>&1; then
        fail "端口 ${SYNAPSE_PORT} 已被占用 —— 换一个 SYNAPSE_PORT（不要指向正在运行的实例）"
        return 1
    fi
    return 0
}

teardown() {
    if [[ "${KEEP_STACK:-0}" == "1" ]]; then
        skip "KEEP_STACK=1 ⇒ 栈保留；清理：docker compose -p $PROJECT_NAME -f docker/docker-compose.yml -f docker/docker-compose.workers.yml down"
        return
    fi
    log "收栈"
    # 不带 -v：保留 db/redis 数据卷，脚本可以反复跑而不必每次重建数据。
    # 带 -p 专用项目名：绝不触碰其他 compose 项目（例如正在运行的生产栈）。
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
    # ⚠️ 必须满足本仓的密码策略（至少含大写字母等）——纯小写十六进制会被
    # `POST /_synapse/admin/v1/register` 以 400 `M_INVALID_PARAM` 拒掉，
    # 表现为"拿不到管理员令牌"，很容易被误当成权限/nonce 问题。
    local password="Smoke-$(openssl rand -hex 8)!Aa1"
    local mac
    mac="$(
        python3 - "$nonce" "$username" "$password" "$admin_secret" <<'PY'
import hashlib, hmac, sys
nonce, username, password, secret = sys.argv[1:5]
# 布局见 synapse-services/src/admin_registration_service.rs 的 update_admin_registration_mac：
#   nonce \0 username \0 password \0 ("admin\0\0\0" | "notadmin") [\0 user_type]
message = (nonce.encode() + b"\x00" + username.encode() + b"\x00" + password.encode()
           + b"\x00" + b"admin\x00\x00\x00" + b"\x00" + b"super_admin")
print(hmac.new(secret.encode(), message, hashlib.sha256).hexdigest())
PY
    )"

    # ⚠️ `user_type` 不能省，且必须参与上面的 MAC：没有它，管理员角色归一化成"无角色"，
    # RBAC 会拒掉 `/_synapse/worker/*`（实测 403）⇒ 本脚本的拓扑断言永远过不去。
    local response
    response="$(curl -fsS -X POST "$BASE_URL/_synapse/admin/v1/register" \
        -H 'Content-Type: application/json' \
        -d "{\"nonce\":\"$nonce\",\"username\":\"$username\",\"password\":\"$password\",\"admin\":true,\"user_type\":\"super_admin\",\"mac\":\"$mac\"}")" || {
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

# 直连角色实例，返回 HTTP 状态码（连不上记 000，便于把"没起来"与"被拒"分开）。
http_status() { # http_status <port> <path>
    curl -sS -o /dev/null -m 5 -w '%{http_code}' "http://127.0.0.1:${1}${2}" 2>/dev/null || echo 000
}

# W-H：证明 `surface_gate` 真的在收敛每个实例的 HTTP 面。
#
# 判据用 **404 vs 405/401**，而不是"200 与否"：
#   * 路由存在、缺凭据 ⇒ 401；路由存在、方法不对 ⇒ 405；
#   * 路由被 gate 掉 ⇒ 404（`M_NOT_FOUND`，由 middleware 直接返回）。
# 所以 `createRoom` 在 sync_worker 上 404、在 client_reader 上 405，正是准入生效的证据。
assert_surface_gate() {
    log "验证 surface_gate：每个角色实例只答自己的面"

    # 所有实例都必须答探活（INSTANCE_AGNOSTIC_PATHS）
    assert_eq "$(http_status "$SYNC_WORKER_PORT" /health)" "200" "sync_worker 答 /health（与角色无关的探活面）"
    assert_eq "$(http_status "$CLIENT_READER_PORT" /health)" "200" "client_reader 答 /health"

    # sync_worker：拥有 /sync，不拥有 createRoom
    local sync_code
    sync_code="$(http_status "$SYNC_WORKER_PORT" '/_matrix/client/v3/sync?timeout=0')"
    assert_eq "$sync_code" "401" "sync_worker 答 /sync（401 = 路由在、缺令牌；404 就是没收敛对）"
    assert_eq "$(http_status "$SYNC_WORKER_PORT" /_matrix/client/v3/createRoom)" "404" \
        "sync_worker 对 createRoom 返 404（surface_gate 生效）"

    # client_reader：拥有 client 面（createRoom 在，GET 只是方法不对 ⇒ 405）
    assert_eq "$(http_status "$CLIENT_READER_PORT" /_matrix/client/v3/createRoom)" "405" \
        "client_reader 对 createRoom 返 405（路由存在 ⇒ 没被 gate 掉）"

    # media_repository / federation_reader：各自的入口不是 404
    local media_code fed_code
    media_code="$(http_status "$MEDIA_REPOSITORY_PORT" /_matrix/media/v3/config)"
    if [[ "$media_code" == "404" || "$media_code" == "000" ]]; then
        fail "media_repository 的 /_matrix/media/v3/config 返回 ${media_code}（期望非 404/000）"
    else
        ok "media_repository 答 /_matrix/media/v3/config（${media_code}）"
    fi
    fed_code="$(http_status "$FEDERATION_READER_PORT" /_matrix/federation/v1/version)"
    assert_eq "$fed_code" "200" "federation_reader 答 /_matrix/federation/v1/version"
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
print("OK   /sync 读回了刚发的事件（这一路走 master；按角色分流见 assert_surface_gate）")
PY
}

main() {
    log "环境检查"
    require_commands || return 1
    require_env_file || return 1
    assert_port_free || return 1

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
    assert_surface_gate
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
echo "有失败项（failures=${failures}）" >&2
exit 1
