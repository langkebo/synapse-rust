#!/bin/bash
# =============================================================================
# synapse-rust 一键重建与部署脚本
# =============================================================================
# 支持按需部署：通过交互式菜单或 ENABLED_EXTENSIONS 环境变量选择需要的
# 扩展功能，仅应用对应的数据库迁移脚本。
#
# 用法:
#   ./deploy.sh                   # 交互式选择功能
#   ./deploy.sh --all             # 部署所有功能（跳过交互）
#   ./deploy.sh --core-only       # 仅部署核心 Matrix 功能
#   ./deploy.sh --features LIST   # 部署指定功能（逗号分隔）
#   ./deploy.sh --skip-build      # 跳过编译与镜像构建
#   ./deploy.sh --image REF       # 使用指定的远程镜像（跳过本地构建，自动 pull）
#
# 可用扩展功能:
#   openclaw-routes, friends, voice-extended, saml-sso, cas-sso,
#   beacons, voip-tracking, widgets, server-notifications,
#   burn-after-read, privacy-ext, external-services
# =============================================================================

set -Eeuo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DEPLOY_ROOT="$SCRIPT_DIR"
LOG_DIR="$DEPLOY_ROOT/logs"
TIMESTAMP="$(date +%Y%m%d_%H%M%S)"
LOG_FILE="$LOG_DIR/deploy_${TIMESTAMP}.log"
LOG_PIPE=""
LOG_TEE_PID=""

ROLLBACK_BACKUP=""
ROLLBACK_IMAGE_TAG=""
ROLLBACK_ENABLED=true
DEPLOYMENT_PHASE="initialization"
ROLLBACK_IN_PROGRESS=false
SKIP_BUILD=false
REMOTE_IMAGE=""
USE_REMOTE_IMAGE=false

# Extension features — order matches Cargo.toml
ALL_EXTENSIONS=(
    openclaw-routes
    friends
    voice-extended
    saml-sso
    cas-sso
    beacons
    voip-tracking
    widgets
    server-notifications
    burn-after-read
    privacy-ext
    external-services
)

EXTENSION_DESCRIPTIONS=(
    "OpenClaw AI 集成 (AI 对话、MCP 工具代理)"
    "好友系统 (好友请求、好友分组)"
    "语音消息扩展 (语音消息录制/播放)"
    "SAML SSO 单点登录"
    "CAS SSO 单点登录"
    "位置信标 (实时位置共享)"
    "VoIP 通话追踪 (通话会话、MatrixRTC)"
    "Widget 小组件"
    "服务器通知系统"
    "阅后即焚消息"
    "隐私扩展 (已读回执控制、在线状态隐藏)"
    "外部服务集成 (Webhook 通知)"
)

# Will be set by parse_args or select_features
ENABLED_EXTENSIONS="${ENABLED_EXTENSIONS:-}"

cd "$DEPLOY_ROOT"

log_info() {
    echo -e "${BLUE}[INFO]${NC} $*"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $*"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $*"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $*" >&2
}

compose() {
    if command -v docker-compose >/dev/null 2>&1; then
        docker-compose "$@"
    else
        docker compose "$@"
    fi
}

# =============================================================================
# CLI argument parsing
# =============================================================================

parse_args() {
    while [ $# -gt 0 ]; do
        case "$1" in
            --all)
                ENABLED_EXTENSIONS="all"
                ;;
            --core-only)
                ENABLED_EXTENSIONS="none"
                ;;
            --features)
                shift
                ENABLED_EXTENSIONS="${1:?'--features 需要参数，如: openclaw-routes,friends'}"
                ;;
            --skip-build)
                SKIP_BUILD=true
                ;;
            --image)
                shift
                REMOTE_IMAGE="${1:?'--image 需要参数，如: docker.io/vmuser232922/mysynapse:latest'}"
                USE_REMOTE_IMAGE=true
                SKIP_BUILD=true
                ;;
            --help|-h)
                show_usage
                exit 0
                ;;
            *)
                log_error "未知参数: $1"
                show_usage
                exit 1
                ;;
        esac
        shift
    done
}

show_usage() {
    cat <<'EOF'
用法: deploy.sh [选项]

选项:
  --all             部署所有功能（包含全部扩展，跳过交互选择）
  --core-only       仅部署核心 Matrix 功能（不含任何扩展）
  --features LIST   部署指定扩展功能（逗号分隔）
  --skip-build      跳过 cargo build 和 Docker 镜像构建
  --image REF       使用指定的远程镜像（自动 docker pull，跳过本地构建）
  --help            显示帮助信息

如果不指定功能参数，脚本将显示交互式功能选择菜单。
也可通过 .env 中的 ENABLED_EXTENSIONS 变量预设。

可用扩展功能:
  openclaw-routes      OpenClaw AI 集成
  friends              好友系统
  voice-extended       语音消息扩展
  saml-sso             SAML SSO 单点登录
  cas-sso              CAS SSO 单点登录
  beacons              位置信标
  voip-tracking        VoIP 通话追踪
  widgets              Widget 小组件
  server-notifications 服务器通知
  burn-after-read      阅后即焚
  privacy-ext          隐私扩展
  external-services    外部服务集成
EOF
}

# =============================================================================
# Interactive feature selection
# =============================================================================

select_features() {
    # If already set (by CLI args or .env), skip interactive selection
    if [ -n "$ENABLED_EXTENSIONS" ]; then
        return
    fi

    echo ""
    echo -e "${BOLD}==========================================${NC}"
    echo -e "${BOLD}  功能选择${NC}"
    echo -e "${BOLD}==========================================${NC}"
    echo ""
    echo -e "  ${CYAN}[0]${NC} 全部功能 (all-extensions)"
    echo -e "  ${CYAN}[1]${NC} 仅核心 Matrix 功能 (无扩展)"
    echo -e "  ${CYAN}[2]${NC} 自定义选择扩展功能"
    echo ""

    local choice
    read -rp "请选择部署模式 [0/1/2] (默认 0): " choice
    choice="${choice:-0}"

    case "$choice" in
        0)
            ENABLED_EXTENSIONS="all"
            log_info "已选择: 全部功能"
            ;;
        1)
            ENABLED_EXTENSIONS="none"
            log_info "已选择: 仅核心 Matrix 功能"
            ;;
        2)
            select_individual_features
            ;;
        *)
            ENABLED_EXTENSIONS="all"
            log_warning "无效输入，默认使用全部功能"
            ;;
    esac
}

select_individual_features() {
    local selected=()
    local i

    echo ""
    echo -e "${BOLD}可用扩展功能:${NC}"
    echo ""

    for i in "${!ALL_EXTENSIONS[@]}"; do
        local num=$((i + 1))
        printf "  ${CYAN}[%2d]${NC} %-24s %s\n" "$num" "${ALL_EXTENSIONS[$i]}" "${EXTENSION_DESCRIPTIONS[$i]}"
    done

    echo ""
    echo "输入功能编号（逗号或空格分隔），如: 1,2,7"
    echo "直接回车跳过所有扩展 (core-only)"
    echo ""
    read -rp "选择: " input

    if [ -z "$input" ]; then
        ENABLED_EXTENSIONS="none"
        log_info "未选择任何扩展，使用核心模式"
        return
    fi

    # Parse comma/space separated numbers
    local nums
    nums="$(echo "$input" | tr ',' ' ')"
    for num in $nums; do
        num="$(echo "$num" | tr -d '[:space:]')"
        if [ -z "$num" ]; then
            continue
        fi
        if ! [[ "$num" =~ ^[0-9]+$ ]]; then
            log_warning "忽略无效输入: $num"
            continue
        fi
        local idx=$((num - 1))
        if [ "$idx" -ge 0 ] && [ "$idx" -lt "${#ALL_EXTENSIONS[@]}" ]; then
            selected+=("${ALL_EXTENSIONS[$idx]}")
        else
            log_warning "忽略超范围编号: $num"
        fi
    done

    if [ ${#selected[@]} -eq 0 ]; then
        ENABLED_EXTENSIONS="none"
        log_info "未选择有效扩展，使用核心模式"
    else
        ENABLED_EXTENSIONS="$(IFS=,; echo "${selected[*]}")"
        log_info "已选择扩展: $ENABLED_EXTENSIONS"
    fi
}

show_feature_summary() {
    echo ""
    echo -e "${BOLD}部署功能配置:${NC}"
    if [ "$ENABLED_EXTENSIONS" = "all" ]; then
        echo -e "  模式: ${GREEN}全部功能${NC} (core + all extensions)"
    elif [ "$ENABLED_EXTENSIONS" = "none" ]; then
        echo -e "  模式: ${YELLOW}仅核心${NC} (pure Matrix homeserver)"
    else
        echo -e "  模式: ${CYAN}自定义${NC}"
        echo -e "  扩展: ${ENABLED_EXTENSIONS}"
    fi
    echo ""
}

setup_logging() {
    mkdir -p "$LOG_DIR"
    touch "$LOG_FILE"

    # Avoid bash process substitution so the script can run inside restricted sandboxes.
    LOG_PIPE="$LOG_DIR/.deploy_${TIMESTAMP}.pipe"
    rm -f "$LOG_PIPE"

    if mkfifo "$LOG_PIPE"; then
        exec 3>&1 4>&2
        tee -a "$LOG_FILE" < "$LOG_PIPE" >&3 2>&4 &
        LOG_TEE_PID="$!"
        exec > "$LOG_PIPE" 2>&1
    else
        echo "无法创建日志管道，回退为仅写入日志文件: $LOG_FILE"
        exec >> "$LOG_FILE" 2>&1
    fi
}

cleanup_logging() {
    if [ -n "$LOG_PIPE" ]; then
        exec 1>&3 2>&4 || true
        exec 3>&- 4>&- || true
        rm -f "$LOG_PIPE" || true
        if [ -n "$LOG_TEE_PID" ]; then
            wait "$LOG_TEE_PID" 2>/dev/null || true
        fi
    fi
}

on_error() {
    local line_no="$1"
    local exit_code="${2:-1}"
    log_error "部署失败: phase=${DEPLOYMENT_PHASE}, line=${line_no}, exit_code=${exit_code}"
    if [ "$ROLLBACK_ENABLED" = "true" ] && [ "$ROLLBACK_IN_PROGRESS" = "false" ]; then
        rollback_deployment || true
    fi
    exit "$exit_code"
}

trap 'on_error "$LINENO" "$?"' ERR
trap cleanup_logging EXIT

show_banner() {
    echo ""
    echo "=========================================="
    echo "  synapse-rust Docker 重建部署脚本"
    echo "=========================================="
    echo "  日志文件: $LOG_FILE"
    echo ""
}

retry() {
    local attempts="$1"
    local delay="$2"
    shift 2

    local try=1
    until "$@"; do
        if [ "$try" -ge "$attempts" ]; then
            return 1
        fi
        log_warning "命令失败，${delay}s 后进行第 $((try + 1))/$attempts 次重试: $*"
        sleep "$delay"
        try=$((try + 1))
    done
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || {
        log_error "缺少依赖命令: $1"
        exit 1
    }
}

load_env() {
    local cli_extensions="$ENABLED_EXTENSIONS"
    set -a
    # shellcheck disable=SC1091
    source .env
    set +a
    ROLLBACK_ENABLED="${ROLLBACK_ON_FAILURE:-true}"
    # CLI args take precedence over .env value
    if [ -n "$cli_extensions" ]; then
        ENABLED_EXTENSIONS="$cli_extensions"
    fi
}

is_placeholder() {
    local value="${1:-}"
    [ -z "$value" ] || [[ "$value" == __REQUIRED_* ]] || [[ "$value" == *"your-"* ]] || [[ "$value" == *"change-me"* ]]
}

check_dependencies() {
    DEPLOYMENT_PHASE="dependency-check"
    log_info "检查依赖..."

    require_command docker
    require_command curl
    require_command tar
    require_command awk
    require_command grep

    if ! command -v docker-compose >/dev/null 2>&1 && ! docker compose version >/dev/null 2>&1; then
        log_error "缺少 Docker Compose"
        exit 1
    fi

    docker info >/dev/null
    log_success "依赖检查通过"
}

check_env_file() {
    DEPLOYMENT_PHASE="environment-check"
    log_info "检查环境变量配置..."

    if [ ! -f ".env" ]; then
        cp .env.example .env
        log_warning "已从 .env.example 创建 .env"
    fi

    chmod +x scripts/generate-secrets.sh
    ./scripts/generate-secrets.sh missing >/dev/null

    load_env

    local required_vars=(
        SERVER_NAME
        PUBLIC_BASEURL
        POSTGRES_PASSWORD
        REDIS_PASSWORD
        ADMIN_SHARED_SECRET
        JWT_SECRET
        REGISTRATION_SHARED_SECRET
        SECRET_KEY
        MACAROON_SECRET
        FORM_SECRET
    )
    local missing_vars=()
    local var

    for var in "${required_vars[@]}"; do
        if is_placeholder "${!var:-}"; then
            missing_vars+=("$var")
        fi
    done

    if [ ${#missing_vars[@]} -ne 0 ]; then
        log_error "以下环境变量需要配置: ${missing_vars[*]}"
        exit 1
    fi

    if [ "${ENABLE_SSL:-false}" = "true" ]; then
        [ -f "ssl/${SSL_CERT:-cert.pem}" ] || {
            log_error "启用 SSL 时必须提供证书: ssl/${SSL_CERT:-cert.pem}"
            exit 1
        }
        [ -f "ssl/${SSL_KEY:-key.pem}" ] || {
            log_error "启用 SSL 时必须提供私钥: ssl/${SSL_KEY:-key.pem}"
            exit 1
        }
    fi

    compose config >/dev/null
    log_success "环境变量配置检查通过"
}

create_directories() {
    DEPLOYMENT_PHASE="directory-setup"
    log_info "创建必要目录..."
    mkdir -p ssl media logs backups config
    [ -d migrations ] || {
        log_error "migrations 目录不存在"
        exit 1
    }
    [ -f config/homeserver.yaml ] || {
        log_error "缺少配置文件: config/homeserver.yaml"
        exit 1
    }
    [ -f config/rate_limit.yaml ] || {
        log_error "缺少配置文件: config/rate_limit.yaml"
        exit 1
    }
    [ -f config/postgres.conf ] || {
        log_error "缺少配置文件: config/postgres.conf"
        exit 1
    }
    log_success "目录与配置文件检查完成"
}

backup_current_state() {
    DEPLOYMENT_PHASE="backup"
    log_info "为回滚创建备份..."

    if docker image inspect synapse-rust:local >/dev/null 2>&1; then
        ROLLBACK_IMAGE_TAG="synapse-rust:rollback-${TIMESTAMP}"
        docker tag synapse-rust:local "$ROLLBACK_IMAGE_TAG"
        log_info "已保存旧镜像标签: $ROLLBACK_IMAGE_TAG"
    fi

    if compose ps --status running 2>/dev/null | grep -Eq 'postgres|redis|synapse|nginx'; then
        chmod +x scripts/backup.sh
        local backup_output
        backup_output="$(./scripts/backup.sh)"
        echo "$backup_output"
        ROLLBACK_BACKUP="$(echo "$backup_output" | awk -F': ' '/备份文件:/ {print $2}' | tail -n 1)"
        if [ -n "$ROLLBACK_BACKUP" ] && [ -f "$ROLLBACK_BACKUP" ]; then
            log_success "已创建回滚备份: $ROLLBACK_BACKUP"
        fi
    else
        log_info "未检测到运行中的旧部署，跳过数据备份"
    fi
}

clear_project_caches() {
    DEPLOYMENT_PHASE="cache-clean"
    if [ "$SKIP_BUILD" = "true" ]; then
        log_info "跳过缓存清理 (--skip-build)"
        return
    fi
    log_info "清理项目缓存与 Docker 构建缓存..."

    (cd "$PROJECT_ROOT" && cargo clean)
    if command -v npm >/dev/null 2>&1; then
        npm cache clean --force || true
    fi
    if command -v yarn >/dev/null 2>&1; then
        yarn cache clean || true
    fi
    if command -v pnpm >/dev/null 2>&1; then
        pnpm store prune || true
    fi

    docker builder prune -af >/dev/null
    docker buildx prune -af >/dev/null 2>&1 || true

    log_success "缓存清理完成"
}

rebuild_project() {
    DEPLOYMENT_PHASE="project-build"
    if [ "$SKIP_BUILD" = "true" ]; then
        log_info "跳过项目编译 (--skip-build)"
        return
    fi
    log_info "重新编译项目..."
    (cd "$PROJECT_ROOT" && cargo build --release --locked --bin synapse-rust --bin healthcheck)
    log_success "项目编译完成"
}

remove_existing_deployment() {
    DEPLOYMENT_PHASE="remove-old-deployment"
    log_info "停止并删除旧容器与关联镜像..."

    compose down --remove-orphans || true
    docker rm -f synapse-postgres synapse-redis synapse-migrator synapse-app synapse-nginx >/dev/null 2>&1 || true
    if [ "$USE_REMOTE_IMAGE" != "true" ] && [ "$SKIP_BUILD" != "true" ]; then
        docker image rm -f synapse-rust:local synapse-rust-tools:local >/dev/null 2>&1 || true
    fi

    log_success "旧部署资源清理完成"
}

build_images() {
    DEPLOYMENT_PHASE="docker-build"
    if [ "$USE_REMOTE_IMAGE" = "true" ]; then
        log_info "拉取远程镜像: $REMOTE_IMAGE"
        retry 3 5 docker pull "$REMOTE_IMAGE"
        export SYNAPSE_IMAGE="$REMOTE_IMAGE"
        export SYNAPSE_PULL_POLICY=missing
        docker image inspect "$REMOTE_IMAGE" >/dev/null
        log_success "远程镜像就绪: $REMOTE_IMAGE"
        return
    fi
    if [ "$SKIP_BUILD" = "true" ]; then
        log_info "跳过 Docker 镜像构建 (--skip-build)"
        if ! docker image inspect "${SYNAPSE_IMAGE:-synapse-rust:local}" >/dev/null 2>&1; then
            log_error "跳过构建但本地镜像 ${SYNAPSE_IMAGE:-synapse-rust:local} 不存在"
            exit 1
        fi
        return
    fi
    log_info "构建新的 Docker 镜像..."
    compose build --no-cache synapse
    docker image inspect synapse-rust:local >/dev/null
    log_success "Docker 镜像构建完成"
}

wait_for_container_health() {
    local container_name="$1"
    local max_retries="${2:-30}"
    local delay="${3:-5}"
    local attempt=1
    local status

    while [ "$attempt" -le "$max_retries" ]; do
        status="$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "$container_name" 2>/dev/null || true)"
        case "$status" in
            healthy|running)
                log_success "$container_name 状态正常: $status"
                return 0
                ;;
            unhealthy|exited|dead)
                log_error "$container_name 状态异常: $status"
                docker logs "$container_name" --tail 200 || true
                return 1
                ;;
        esac
        log_info "等待 $container_name 就绪... ($attempt/$max_retries, 当前: ${status:-unknown})"
        sleep "$delay"
        attempt=$((attempt + 1))
    done

    log_error "$container_name 在限定时间内未就绪"
    docker logs "$container_name" --tail 200 || true
    return 1
}

run_migrations() {
    DEPLOYMENT_PHASE="database-migrate"
    log_info "执行数据库迁移 (ENABLED_EXTENSIONS=$ENABLED_EXTENSIONS)..."
    retry 3 5 compose run --rm --no-deps -e "ENABLED_EXTENSIONS=${ENABLED_EXTENSIONS}" migrator
    log_success "数据库迁移与校验完成"
}

start_services() {
    DEPLOYMENT_PHASE="service-start"
    log_info "启动基础服务..."

    compose up -d postgres redis
    wait_for_container_health synapse-postgres "${HEALTHCHECK_RETRIES:-30}" "${HEALTHCHECK_INTERVAL:-5}"
    wait_for_container_health synapse-redis "${HEALTHCHECK_RETRIES:-30}" "${HEALTHCHECK_INTERVAL:-5}"

    run_migrations

    log_info "启动 Synapse 应用..."
    compose up -d synapse
    wait_for_container_health synapse-app "${HEALTHCHECK_RETRIES:-30}" "${HEALTHCHECK_INTERVAL:-5}"

    log_info "启动 Nginx..."
    compose up -d nginx
    wait_for_container_health synapse-nginx "${HEALTHCHECK_RETRIES:-30}" "${HEALTHCHECK_INTERVAL:-5}"

    log_success "所有服务已启动"
}

verify_database() {
    DEPLOYMENT_PHASE="verify-database"
    log_info "验证数据库连接..."
    compose exec -T postgres psql -U "${POSTGRES_USER:-postgres}" -d "${POSTGRES_DB:-synapse}" -c "SELECT 1;" >/dev/null
    log_success "数据库连接正常"
}

verify_health_endpoints() {
    DEPLOYMENT_PHASE="verify-health"
    log_info "验证健康检查接口..."

    curl -fsS "http://localhost:${SYNAPSE_PORT:-8008}/health" >/dev/null
    curl -fsS "http://localhost:${HTTP_PORT:-80}/health" >/dev/null
    curl -fsS "http://localhost:${SYNAPSE_PORT:-8008}/_matrix/client/versions" >/dev/null

    log_success "健康检查与 API 基础接口验证通过"
}

verify_logs_clean() {
    DEPLOYMENT_PHASE="verify-logs"
    log_info "检查容器日志中是否存在 ERROR/WARNING..."

    local log_dump
    log_dump="$(compose logs --no-color --tail=400 2>&1 || true)"
    local filtered
    filtered="$(echo "$log_dump" | grep -Ei '\b(ERROR|WARN(ING)?)\b' \
        | grep -v 'no usable system locales' \
        | grep -v 'enabling "trust" authentication' \
        | grep -v 'Missing indexes' \
        | grep -v 'DOCKER_INSECURE_NO_IPTABLES_RAW' \
        | grep -v 'forcibly turning on oci-mediatype' \
        || true)"
    if [ -n "$filtered" ]; then
        log_error "检测到 ERROR/WARNING 日志:"
        echo "$filtered"
        return 1
    fi

    log_success "未发现 ERROR/WARNING 级别日志"
}

show_status() {
    echo ""
    log_info "服务状态:"
    compose ps
    echo ""
}

rollback_deployment() {
    DEPLOYMENT_PHASE="rollback"
    ROLLBACK_IN_PROGRESS=true
    log_warning "开始执行回滚..."

    compose down --remove-orphans >/dev/null 2>&1 || true

    if [ -n "$ROLLBACK_IMAGE_TAG" ] && docker image inspect "$ROLLBACK_IMAGE_TAG" >/dev/null 2>&1; then
        docker tag "$ROLLBACK_IMAGE_TAG" synapse-rust:local || true
    fi

    if [ -n "$ROLLBACK_BACKUP" ] && [ -f "$ROLLBACK_BACKUP" ]; then
        chmod +x scripts/restore.sh
        RESTORE_FORCE=true ./scripts/restore.sh "$ROLLBACK_BACKUP" || true
        log_warning "已尝试恢复到备份状态"
    else
        log_warning "没有可用备份，跳过数据回滚"
    fi
}

show_access_info() {
    echo ""
    echo "=========================================="
    echo "  部署完成"
    echo "=========================================="
    echo "服务器名称: ${SERVER_NAME}"
    echo "公开 URL: ${PUBLIC_BASEURL}"
    echo "HTTP 健康检查:  http://localhost:${HTTP_PORT:-80}/health"
    echo "应用健康检查:   http://localhost:${SYNAPSE_PORT:-8008}/health"
    echo "API 检查:       http://localhost:${SYNAPSE_PORT:-8008}/_matrix/client/versions"
    echo "部署日志:       ${LOG_FILE}"
    echo "扩展功能:       ${ENABLED_EXTENSIONS}"
    echo ""
}

main() {
    parse_args "$@"
    setup_logging
    show_banner
    check_dependencies
    check_env_file
    select_features
    show_feature_summary
    create_directories
    backup_current_state
    clear_project_caches
    rebuild_project
    remove_existing_deployment
    build_images
    start_services
    verify_database
    verify_health_endpoints
    verify_logs_clean
    show_status
    show_access_info
    log_success "重建、优化部署与验证全部完成"
}

main "$@"
