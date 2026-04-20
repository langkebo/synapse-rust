#!/bin/bash
# =============================================================================
# synapse-rust 一键重建与部署脚本
# =============================================================================

set -Eeuo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
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
    set -a
    # shellcheck disable=SC1091
    source .env
    set +a
    ROLLBACK_ENABLED="${ROLLBACK_ON_FAILURE:-true}"
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
    log_info "重新编译项目..."
    (cd "$PROJECT_ROOT" && cargo build --release --locked --bin synapse-rust --bin healthcheck)
    log_success "项目编译完成"
}

remove_existing_deployment() {
    DEPLOYMENT_PHASE="remove-old-deployment"
    log_info "停止并删除旧容器与关联镜像..."

    compose down --remove-orphans || true
    docker rm -f synapse-postgres synapse-redis synapse-migrator synapse-app synapse-nginx >/dev/null 2>&1 || true
    docker image rm -f synapse-rust:local synapse-rust-tools:local vmuser232922/mysynapse:latest >/dev/null 2>&1 || true

    log_success "旧部署资源清理完成"
}

build_images() {
    DEPLOYMENT_PHASE="docker-build"
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
    log_info "执行数据库迁移..."
    retry 3 5 compose run --rm --no-deps migrator
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
    if echo "$log_dump" | grep -Eiq '\b(ERROR|WARN(ING)?)\b'; then
        log_error "检测到 ERROR/WARNING 日志:"
        echo "$log_dump" | grep -Ein '\b(ERROR|WARN(ING)?)\b' || true
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
    echo ""
}

main() {
    setup_logging
    show_banner
    check_dependencies
    check_env_file
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
