#!/bin/bash

set -eEuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DOCKER_DIR="$ROOT_DIR/docker"
MODE="${1:-all}"
TEMP_ENV_CREATED=false
IMAGE_BUILT=false
SERVICES_STARTED=false
COMPOSE_FILES=(-f docker-compose.yml -f docker-compose.dev-host-access.yml)
RUN_APPSERVICE_GATE="${RUN_APPSERVICE_GATE:-0}"
APPSERVICE_GATE_DAY="${APPSERVICE_GATE_DAY:-D2}"
APPSERVICE_GATE_FAIL_ON="${APPSERVICE_GATE_FAIL_ON:-warning}"
APPSERVICE_GATE_BASE_URL="${APPSERVICE_GATE_BASE_URL:-http://localhost:8008}"
APPSERVICE_GATE_PROMETHEUS_URL="${APPSERVICE_GATE_PROMETHEUS_URL:-http://localhost:9090/metrics}"
APPSERVICE_GATE_OUTPUT_DIR="${APPSERVICE_GATE_OUTPUT_DIR:-$ROOT_DIR/artifacts/appservice_gate}"
APPSERVICE_GATE_RESOURCE_SUMMARY="${APPSERVICE_GATE_RESOURCE_SUMMARY:-ci backend validation gate; 待补 CPU/RSS/连接池摘要}"
APPSERVICE_GATE_ADMIN_TOKEN="${APPSERVICE_GATE_ADMIN_TOKEN:-}"
# `e2e` 车道（真实 HTTP）：只跑 `#[ignore]` 的那 7 条，见 run_e2e_http。
# 串行是刻意的 —— 那 7 条并发起来会在注册/搜索上撞 rate_limit，且失败信息互相污染。
E2E_BASE_URL="${E2E_BASE_URL:-}"
E2E_TEST_THREADS="${E2E_TEST_THREADS:-1}"

log() {
    printf '[ci] %s\n' "$*"
}

docker_compose() {
    (
        cd "$DOCKER_DIR"
        docker compose "${COMPOSE_FILES[@]}" "$@"
    )
}

ensure_docker_env() {
    if [ -f "$DOCKER_DIR/.env" ]; then
        return 0
    fi

    cat >"$DOCKER_DIR/.env" <<'EOF'
COMPOSE_PROJECT_NAME=synapse-ci
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
# ⚠️ 必填（`docker/config/homeserver.yaml` 里是 `${TURN_SHARED_SECRET:?…}`，
# `synapse-common/src/config/loader.rs::resolve_env_in_string_with` 对 `:?` 的语义是
# **取不到就返回 Err**）⇒ 缺它容器在配置加载阶段就退出，日志是一行
# `Environment variable TURN_SHARED_SECRET is required`，而不是"TURN 不好用"。
TURN_SHARED_SECRET=ci-turn-shared-secret
# ⚠️ 必填，同上（`${WORKER_REPLICATION_SECRET:?…}`，worker 复制面用）。
WORKER_REPLICATION_SECRET=ci-worker-replication-secret
# ⚠️ 必填：`docker/docker-compose.yml` 里写的是 `${TOKEN_HASH_SECRET:?…}`，缺它
# `docker compose up` 直接 `error while interpolating … required variable
# TOKEN_HASH_SECRET is missing a value`，一行容器都起不来。本行此前缺失，只在
# **本机已有 `docker/.env`** 时被那个文件掩盖（它被 gitignore、每人一份），
# 全新 checkout（= 所有 CI runner）必然踩到。
# 守卫：`tests/unit/ci_test_scope_tests.rs::ci_env_seeds_every_required_variable`。
TOKEN_HASH_SECRET=ci-token-hash-secret-ci-token-hash-secret-01
ADMIN_SECRET=ci-admin-secret
FEDERATION_SIGNING_KEY=ci-federation-signing-key
FEDERATION_KEY_ID=ed25519:ci
RUN_MIGRATIONS=true
VERIFY_SCHEMA=true
STOP_ON_MIGRATION_FAILURE=true
DB_WAIT_ATTEMPTS=30
DB_WAIT_INTERVAL=2
MIGRATION_TIMEOUT=300
EOF
    TEMP_ENV_CREATED=true
}

load_docker_env() {
    set -a
    . "$DOCKER_DIR/.env"
    set +a
}

cleanup() {
    if [ "$SERVICES_STARTED" = true ]; then
        docker_compose down -v --remove-orphans >/dev/null 2>&1 || true
    fi
    if [ "$TEMP_ENV_CREATED" = true ]; then
        rm -f "$DOCKER_DIR/.env"
    fi
}

dump_docker_logs() {
    if [ ! -d "$DOCKER_DIR" ]; then
        return 0
    fi
    if [ "$SERVICES_STARTED" != true ]; then
        return 0
    fi
    (
        docker_compose ps || true
        docker_compose logs --no-color --tail=300 || true
    ) >&2
}

on_error() {
    dump_docker_logs
}

wait_for_health() {
    local service_name="$1"
    local attempts="${2:-40}"
    local sleep_seconds="${3:-3}"
    for ((i = 1; i <= attempts; i++)); do
        local container_id
        container_id="$(docker_compose ps -q "$service_name" 2>/dev/null || true)"
        if [ -z "$container_id" ]; then
            sleep "$sleep_seconds"
            continue
        fi

        local health_status
        health_status="$(docker inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "$container_id" 2>/dev/null || true)"
        if [ "$health_status" = "healthy" ] || [ "$health_status" = "running" ]; then
            return 0
        fi
        sleep "$sleep_seconds"
    done

    log "服务健康检查超时: $service_name"
    dump_docker_logs
    return 1
}

wait_for_http() {
    local url="$1"
    local attempts="${2:-20}"
    local sleep_seconds="${3:-3}"

    for ((i = 1; i <= attempts; i++)); do
        if curl -fsS "$url" >/dev/null 2>&1; then
            return 0
        fi
        sleep "$sleep_seconds"
    done

    return 1
}

ensure_clean_stack() {
    docker_compose down -v --remove-orphans >/dev/null 2>&1 || true
    SERVICES_STARTED=false
}

reset_stack() {
    ensure_clean_stack
    ensure_dependencies
}

ensure_dependencies() {
    if [ "$SERVICES_STARTED" = true ]; then
        return 0
    fi

    docker_compose up -d db redis
    SERVICES_STARTED=true
    wait_for_health db 40 3
    wait_for_health redis 30 2
}

ensure_image() {
    if [ "$IMAGE_BUILT" = true ]; then
        return 0
    fi

    # `SKIP_IMAGE_BUILD=1`：镜像已由外部步骤构建（如 workflow 用 buildx + gha 层缓存
    # 建好后 `--load` 进来）。此时只校验镜像确实存在 —— 直接 `compose build` 会在 CI
    # 里白跑一次 20–40 分钟的 release 编译，且丢掉 buildx 的缓存。
    if [ "${SKIP_IMAGE_BUILD:-0}" = "1" ]; then
        local image_ref="${SYNAPSE_IMAGE:-synapse-rust}:${SYNAPSE_IMAGE_TAG:-latest}"
        if docker image inspect "$image_ref" >/dev/null 2>&1; then
            log "复用已构建镜像 ${image_ref}（SKIP_IMAGE_BUILD=1）"
            IMAGE_BUILT=true
            return 0
        fi
        log "SKIP_IMAGE_BUILD=1 但镜像 $image_ref 不存在"
        return 1
    fi

    docker_compose build synapse-rust
    IMAGE_BUILT=true
}

run_rust_checks() {
    ensure_dependencies

    export DATABASE_URL="postgres://${DB_USER}:${DB_PASSWORD}@localhost:${DB_EXPOSE_PORT}/${DB_NAME}"
    export TEST_DATABASE_URL="$DATABASE_URL"
    export REDIS_URL="redis://:${REDIS_PASSWORD}@localhost:${REDIS_EXPOSE_PORT}"
    export RUST_BACKTRACE=1
    export RUST_LOG=info

    cd "$ROOT_DIR"
    python3 scripts/ci/check_root_canonical_ledger.py
    cargo fmt --all -- --check
    cargo check --locked
    # CI 的三个测试批次，命令与 `ci.yml` **逐字一致**（sweep A13）。
    #
    # 这里过去调用 `scripts/run_ci_tests.sh`：那是同一批测试的第二份实现，且已经漂移
    # （它仍用 `--ignored` 跑 4 条"在 CI 上会假失败"的手工负载冒烟、还带 CI 已收回的
    # 重试、线程数也不同）。按铁律 2「同一职责只允许一份实现」，那份重复实现已删除，
    # 本地入口直接跑 CI 的命令。改 `ci.yml` 的批次时改这里，只有这一处。
    if ! command -v cargo-nextest >/dev/null 2>&1; then
        log "cargo-nextest is required (CI runs these batches with nextest): cargo install cargo-nextest --locked"
        exit 1
    fi
    cargo nextest run --workspace --lib --all-features --locked --test-threads 4 \
        -E 'not test(/friend_room_service::tests::bench_friend_list_/)'
    cargo nextest run --test unit --features test-utils --locked --test-threads 4
    cargo nextest run --test integration --all-features --locked --test-threads 4 --no-fail-fast
    # 跳过分类报告：nextest 只报告总数，这里区分「设计内跳过」与「覆盖缺口」，
    # 并要求每一类的覆盖车道真实存在。分析器为源码级扫描，不依赖构建产物；
    # 不带 `|| true`——出现未归类用例或车道缺失时门禁必须失败。
    python3 scripts/quality/analyze_skipped_tests.py
}

run_migration_checks() {
    ensure_dependencies
    ensure_image

    docker_compose run --rm --no-deps --entrypoint /app/scripts/db_migrate.sh synapse-rust migrate
    docker_compose run --rm --no-deps --entrypoint /app/scripts/db_migrate.sh synapse-rust validate

    cd "$ROOT_DIR"
    # The refactor-era `bash scripts/db/pre_refactor_schema_guard.sh check` line
    # used to run here. That script (and `scripts/db/`) no longer exist, so this
    # step failed on every run — which made the whole `Backend Validation`
    # workflow permanently red and therefore signal-free.
    #
    # Its stated job ("fail fast if critical tables are missing") is already
    # covered by the `db_migrate.sh validate` call above: `validate_schema()`
    # checks a 16-table `required_tables` list and errors on any missing table
    # (docker/db_migrate.sh:537-567). The workspace-wide table/schema coverage
    # checks live in the db-migration-gate workflow.
    :
}

run_docker_smoke() {
    ensure_dependencies
    ensure_image

    docker_compose up -d synapse-rust
    wait_for_http "http://localhost:${SYNAPSE_PORT}/_matrix/client/versions" 20 3
    wait_for_http "http://localhost:${FEDERATION_PORT}/_matrix/federation/v1/version" 20 3
    curl -fsS "http://localhost:${SYNAPSE_PORT}/_matrix/client/versions" >/dev/null
    curl -fsS "http://localhost:${FEDERATION_PORT}/_matrix/federation/v1/version" >/dev/null

    log "运行 /room_keys/* 端到端测试"
    BASE="http://localhost:${SYNAPSE_PORT}" \
        bash "$ROOT_DIR/tests/e2e/scripts/key_backup.sh"

    if [ "$RUN_APPSERVICE_GATE" = "1" ]; then
        run_appservice_gate
    fi
}

# E2E 真实 HTTP 车道的**前置断言**：注册必须开着。
#
# 为什么值得单独探一次：这 7 条用例的第一句就是 `POST /_matrix/client/v3/register`，
# 而 `docker/config/homeserver.yaml` 默认 `enable_registration: false`。若覆盖层没叠上，
# 7 条会一起 403 并各自打印 20 行 `M_FORBIDDEN`，看上去像「测试坏了」；实际上只是
# "栈起错了形态"。探针把这一类失败压成一行结论 + 指向 `docker/docker-compose.e2e.yml`。
assert_registration_open() {
    local base_url="$1"
    local probe_user="e2e_probe_$$_$(date +%s)"
    local body out_file response_code
    body="$(printf '{"username":"%s","password":"Probe123!","auth":{"type":"m.login.dummy"}}' "$probe_user")"
    out_file="$(mktemp)"
    response_code="$(curl -sS -o "$out_file" -w '%{http_code}' -X POST \
        -H 'Content-Type: application/json' -d "$body" \
        "${base_url}/_matrix/client/v3/register" 2>/dev/null || true)"

    if [ "$response_code" != "200" ]; then
        log "注册未开放：POST ${base_url}/_matrix/client/v3/register → HTTP ${response_code}"
        head -c 400 "$out_file" >&2 || true
        printf '\n' >&2
        rm -f "$out_file"
        log "本车道靠 docker/docker-compose.e2e.yml 覆盖 SYNAPSE__SERVER__ENABLE_REGISTRATION=true —— 检查 COMPOSE_FILES 是否叠了该覆盖层"
        return 1
    fi

    rm -f "$out_file"
}

# 真实端到端：起栈 → 等就绪 → 断言注册开放 → 只跑 `#[ignore]` 的 7 条。
#
# 与 `run_docker_smoke` 的分工：
#   - smoke 跑 `tests/e2e/scripts/key_backup.sh`（curl 脚本），验证的是「栈能起来、密钥备份
#     端点通」；
#   - 本函数跑 `tests/e2e/user_flow_tests.rs` 的 7 条 reqwest 用例（注册/登录/建房/发消息/
#     上传下载媒体/好友/多房间），验证的是「完整用户流程」。
# 两者都需要真栈，但只有后者在 `cargo nextest` 的统计里（也就只有它会被 `skipped` 计数掩盖）。
run_e2e_http() {
    local base_url="${E2E_BASE_URL:-http://localhost:${SYNAPSE_PORT:-8008}}"

    ensure_dependencies
    ensure_image

    log "启动 synapse-rust（+ docker-compose.e2e.yml：开放注册）"
    # 起栈失败时保留 compose 的输出再用：最常见的两种失败都不是脚本能自己修的 ——
    # 「Pool overlaps with other one on this address space」= 本机已有另一个栈占了
    # 192.168.97.0/24（compose 把这个子网钉死了，见 `networks.synapse_network`），
    # 此时换 `SYNAPSE_NETWORK_SUBNET` 或停掉另一个栈；容器名冲突则是
    # `synapse-postgres`/`synapse-redis` 与部署栈同名（换 `COMPOSE_PROJECT_NAME`）。
    # 不提示的话，这两条都只给一行 daemon 错误，看着像代码坏了。
    local up_log
    up_log="$(mktemp)"
    if ! docker_compose up -d synapse-rust 2>&1 | tee "$up_log"; then
        if grep -q "Pool overlaps" "$up_log"; then
            log "提示：本机已有栈占用固定子网 192.168.97.0/24 —— 用 SYNAPSE_NETWORK_SUBNET 换一个网段，或先停掉那个栈"
        fi
        rm -f "$up_log"
        return 1
    fi
    rm -f "$up_log"
    SERVICES_STARTED=true

    if ! wait_for_http "${base_url}/_matrix/client/versions" 40 3; then
        log "homeserver 在 ${base_url} 未就绪"
        return 1
    fi

    assert_registration_open "$base_url" || return 1

    # `--run-ignored only`：**只**跑被 `#[ignore]` 的那 7 条。`ignored`/`all` 会把 19 条
    # 确定性用例（`e2e_scenarios.rs` 的 `simulated_*`，PR 车道已跑）再跑一遍，既慢又会
    # 让本车道的统计混进不相干的结果。
    #
    # `--no-tests=fail`：nextest 的 `auto` 只在 CI 环境里默认 fail，本地跑会"0 条也算过"。
    # 这 7 条一旦被改名/删除/去掉 `#[ignore]`，本车道就会退化成"每次都绿" —— 显式 fail
    # 才能把那种退化暴露出来。
    log "运行 E2E 真实 HTTP 用例（E2E_RUN=1，--run-ignored only，threads=${E2E_TEST_THREADS}）"
    (
        cd "$ROOT_DIR"
        E2E_RUN=1 E2E_BASE_URL="$base_url" \
            cargo nextest run --locked --features test-utils --test e2e \
            --run-ignored only --no-tests=fail --test-threads "$E2E_TEST_THREADS"
    )
}

run_appservice_gate() {
    mkdir -p "$APPSERVICE_GATE_OUTPUT_DIR"

    if [ -z "$APPSERVICE_GATE_ADMIN_TOKEN" ]; then
        log "RUN_APPSERVICE_GATE=1 但未提供 APPSERVICE_GATE_ADMIN_TOKEN"
        return 1
    fi

    log "运行 appservice gate (${APPSERVICE_GATE_DAY}, fail-on=${APPSERVICE_GATE_FAIL_ON})"
    ADMIN_TOKEN="$APPSERVICE_GATE_ADMIN_TOKEN" \
        BASE_URL="$APPSERVICE_GATE_BASE_URL" \
        PROMETHEUS_URL="$APPSERVICE_GATE_PROMETHEUS_URL" \
        python3 "$ROOT_DIR/scripts/appservice_daily_report.py" \
        --day "$APPSERVICE_GATE_DAY" \
        --fail-on "$APPSERVICE_GATE_FAIL_ON" \
        --output-dir "$APPSERVICE_GATE_OUTPUT_DIR" \
        --resource-summary "$APPSERVICE_GATE_RESOURCE_SUMMARY"
}

trap cleanup EXIT
trap on_error ERR

ensure_docker_env
load_docker_env
ensure_clean_stack

case "$MODE" in
    rust)
        log "运行 Rust 全量测试"
        run_rust_checks
        ;;
    migration)
        log "运行迁移校验"
        run_migration_checks
        ;;
    docker)
        log "运行 Docker 启动验证"
        run_docker_smoke
        ;;
    e2e)
        log "运行 E2E 真实 HTTP 车道（起栈 + 只跑 7 条 #[ignore] 用例）"
        # 只叠 e2e 覆盖层，**不叠** dev-host-access：本车道不需要从宿主机连 db/redis，
        # 少暴露两个端口就少一处与本机其它栈抢 5432/6379 的机会。
        COMPOSE_FILES=(-f docker-compose.yml -f docker-compose.e2e.yml)
        run_e2e_http
        ;;
    all)
        log "运行完整后端 CI 验证"
        reset_stack
        run_migration_checks
        ensure_clean_stack
        reset_stack
        run_rust_checks
        ensure_clean_stack
        reset_stack
        run_docker_smoke
        ;;
    *)
        printf '用法: %s [rust|migration|docker|e2e|all]\n' "$0" >&2
        exit 1
        ;;
esac
