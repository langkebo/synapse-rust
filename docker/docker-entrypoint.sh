#!/bin/bash
# ============================================================================
# synapse-rust 容器入口脚本
# 版本: 1.1.0
# 描述: 容器启动时自动执行数据库迁移和健康检查
# ============================================================================

set -euo pipefail

# 配置
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="/app"
MIGRATIONS_DIR="$PROJECT_ROOT/migrations"
LOG_DIR="$PROJECT_ROOT/logs"
HEALTHCHECK_ENDPOINT="${HEALTHCHECK_ENDPOINT:-http://localhost:8008/_matrix/federation/v1/version}"

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

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

log_debug() {
    if [ "${RUST_LOG:-info}" = "debug" ]; then
        echo -e "${CYAN}[DEBUG]${NC} $*"
    fi
}

# 等待数据库就绪
wait_for_db() {
    local max_attempts="${DB_WAIT_ATTEMPTS:-30}"
    local attempt=0
    local wait_interval="${DB_WAIT_INTERVAL:-2}"
    
    log_info "等待数据库就绪..."
    
    while [ $attempt -lt $max_attempts ]; do
        # 每次尝试前清理连接状态
        psql "$DATABASE_URL" -c "ABORT;" >/dev/null 2>&1 || true
        
        if psql "$DATABASE_URL" -c "SELECT 1" > /dev/null 2>&1; then
            log_success "数据库连接成功"
            return 0
        fi
        
        attempt=$((attempt + 1))
        log_info "等待数据库... ($attempt/$max_attempts)"
        sleep $wait_interval
    done
    
    log_error "数据库连接超时"
    return 1
}

# 等待 Redis 就绪
wait_for_redis() {
    if [ "${REDIS_ENABLED:-true}" != "true" ]; then
        log_info "Redis 已禁用，跳过"
        return 0
    fi
    
    local max_attempts="${REDIS_WAIT_ATTEMPTS:-30}"
    local attempt=0
    
    log_info "等待 Redis 就绪..."
    
    while [ $attempt -lt $max_attempts ]; do
        if redis-cli -u "$REDIS_URL" ping > /dev/null 2>&1; then
            log_success "Redis 连接成功"
            return 0
        fi
        
        attempt=$((attempt + 1))
        log_info "等待 Redis... ($attempt/$max_attempts)"
        sleep 2
    done
    
    log_warning "Redis 连接超时，继续启动..."
    return 0
}

# 执行迁移
run_migrations() {
    if [ "${RUN_MIGRATIONS:-true}" != "true" ]; then
        log_info "跳过数据库迁移 (RUN_MIGRATIONS=false)"
        return 0
    fi
    
    log_info "开始执行数据库迁移..."
    
    mkdir -p "$LOG_DIR"
    local log_file="$LOG_DIR/migration_$(date +%Y%m%d_%H%M%S).log"
    
    # 确保数据库连接处于干净状态
    log_debug "确保数据库连接处于干净状态..."
    psql "$DATABASE_URL" -c "ABORT;" >/dev/null 2>&1 || true
    
    # 使用 db_migrate.sh 执行迁移
    if [ -f "$PROJECT_ROOT/scripts/db_migrate.sh" ]; then
        log_info "使用迁移脚本执行..."
        
        # 设置单独的环境变量，避免 set -e 影响
        set +e
        bash "$PROJECT_ROOT/scripts/db_migrate.sh" migrate 2>&1 | tee "$log_file"
        local exit_code=${PIPESTATUS[0]}
        set -e
        
        if [ $exit_code -eq 0 ]; then
            log_success "迁移执行成功"
            return 0
        else
            log_error "迁移执行失败 (exit code: $exit_code)"
            
            # 尝试恢复连接状态
            log_info "尝试恢复数据库连接状态..."
            psql "$DATABASE_URL" -c "ABORT;" >/dev/null 2>&1 || true
            
            if [ "${STOP_ON_MIGRATION_FAILURE:-true}" = "true" ]; then
                log_error "迁移失败，退出"
                exit 1
            else
                log_warning "迁移失败，但继续启动 (STOP_ON_MIGRATION_FAILURE=false)"
                return 0
            fi
        fi
    else
        log_warning "迁移脚本不存在，跳过迁移"
        return 0
    fi
}

# 验证迁移
verify_migrations() {
    if [ "${VERIFY_SCHEMA:-true}" != "true" ]; then
        log_info "跳过架构验证 (VERIFY_SCHEMA=false)"
        return 0
    fi
    
    log_info "验证数据库架构..."
    
    # 简单验证 - 检查必要表是否存在
    local required_tables=(
        "users"
        "devices"
        "rooms"
        "events"
        "schema_migrations"
    )
    
    for table in "${required_tables[@]}"; do
        local exists=$(psql "$DATABASE_URL" -t -c "
            SELECT EXISTS (
                SELECT FROM information_schema.tables 
                WHERE table_schema = 'public' AND table_name = '$table'
            )
        " 2>/dev/null | tr -d ' ')
        
        if [ "$exists" = "t" ]; then
            log_debug "表验证通过: $table"
        else
            log_error "表验证失败: $table"
            return 1
        fi
    done
    
    log_success "架构验证通过"
    return 0
}

# 健康检查
healthcheck_db() {
    if ! psql "$DATABASE_URL" -c "SELECT 1" > /dev/null 2>&1; then
        log_error "数据库健康检查失败"
        return 1
    fi
    return 0
}

healthcheck_redis() {
    if [ "${REDIS_ENABLED:-true}" != "true" ]; then
        return 0
    fi
    
    if ! redis-cli -u "$REDIS_URL" ping > /dev/null 2>&1; then
        log_error "Redis 健康检查失败"
        return 1
    fi
    return 0
}

# 启动应用
start_application() {
    log_info "=========================================="
    log_info "synapse-rust 容器启动"
    log_info "版本: $(date '+%Y-%m-%d %H:%M:%S')"
    log_info "=========================================="
    
    # 设置配置文件路径
    export SYNAPSE_CONFIG_PATH="${SYNAPSE_CONFIG_PATH:-/app/config/homeserver.yaml}"
    
    # 如果配置文件存在，复制到工作目录
    if [ -f "$SYNAPSE_CONFIG_PATH" ]; then
        log_info "使用配置文件: $SYNAPSE_CONFIG_PATH"
    else
        log_error "配置文件不存在: $SYNAPSE_CONFIG_PATH"
        exit 1
    fi
    
    # 执行传入的命令或默认启动命令
    if [ $# -gt 0 ]; then
        log_info "执行命令: $*"
        exec "$@"
    else
        log_info "启动应用: /app/synapse-rust"
        exec /app/synapse-rust
    fi
}

# 主函数
main() {
    log_info "=========================================="
    log_info "synapse-rust 容器初始化"
    log_info "=========================================="
    
    # 等待数据库
    if ! wait_for_db; then
        log_error "数据库连接失败，退出"
        exit 1
    fi
    
    # 等待 Redis (跳过检查)
    log_warning "Redis 检查已跳过"
    # if ! wait_for_redis; then
    #     log_warning "Redis 连接失败，继续启动..."
    # fi
    
    # 执行迁移（如果启用）
    if ! run_migrations; then
        log_error "迁移失败，退出"
        exit 1
    fi
    
    # 验证迁移
    if ! verify_migrations; then
        log_error "迁移验证失败，退出"
        exit 1
    fi
    
    log_info "=========================================="
    log_info "启动应用服务"
    log_info "=========================================="
    
    # 启动应用
    start_application "$@"
}

main "$@"
