#!/bin/bash
# ============================================================================
# synapse-rust 容器入口脚本
# 版本: 1.0.0
# 创建日期: 2026-03-02
# 描述: 容器启动时自动执行数据库迁移
# ============================================================================

set -e

# 配置
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="/app"
MIGRATIONS_DIR="$PROJECT_ROOT/migrations"
LOG_DIR="$PROJECT_ROOT/logs/migrations"

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# 等待数据库就绪
wait_for_db() {
    local max_attempts="${DB_WAIT_ATTEMPTS:-30}"
    local attempt=0
    
    log_info "等待数据库就绪..."
    
    while [ $attempt -lt $max_attempts ]; do
        if psql "$DATABASE_URL" -c "SELECT 1" > /dev/null 2>&1; then
            log_success "数据库连接成功"
            return 0
        fi
        
        attempt=$((attempt + 1))
        log_info "等待数据库... ($attempt/$max_attempts)"
        sleep 2
    done
    
    log_error "数据库连接超时"
    return 1
}

# 执行迁移
run_migrations() {
    log_info "开始执行数据库迁移..."
    
    mkdir -p "$LOG_DIR"
    local log_file="$LOG_DIR/migration_$(date +%Y%m%d_%H%M%S).log"
    
    # 使用 db_migrate.sh 执行迁移
    if [ -f "$PROJECT_ROOT/scripts/db_migrate.sh" ]; then
        log_info "使用迁移脚本执行..."
        bash "$PROJECT_ROOT/scripts/db_migrate.sh" migrate 2>&1 | tee "$log_file"
        local exit_code=${PIPESTATUS[0]}
        
        if [ $exit_code -eq 0 ]; then
            log_success "迁移执行成功"
            return 0
        else
            log_error "迁移执行失败"
            return 1
        fi
    else
        # 直接执行 SQL 文件
        log_info "直接执行迁移文件..."
        
        for file in $(ls "$MIGRATIONS_DIR"/*.sql 2>/dev/null | sort); do
            local filename=$(basename "$file")
            log_info "执行: $filename"
            
            if psql "$DATABASE_URL" -f "$file" >> "$log_file" 2>&1; then
                log_success "完成: $filename"
            else
                log_error "失败: $filename"
                return 1
            fi
        done
        
        log_success "所有迁移执行完成"
        return 0
    fi
}

# 验证迁移
verify_migrations() {
    log_info "验证迁移结果..."
    
    if [ -f "$PROJECT_ROOT/scripts/verify_migration.sh" ]; then
        bash "$PROJECT_ROOT/scripts/verify_migration.sh"
        return $?
    fi
    
    # 简单验证
    local required_tables=(
        "media_quota_config"
        "server_notifications"
    )
    
    for table in "${required_tables[@]}"; do
        local exists=$(psql "$DATABASE_URL" -t -c "
            SELECT EXISTS (
                SELECT FROM information_schema.tables 
                WHERE table_schema = 'public' AND table_name = '$table'
            )
        " 2>/dev/null | tr -d ' ')
        
        if [ "$exists" = "t" ]; then
            log_success "表验证通过: $table"
        else
            log_error "表验证失败: $table"
            return 1
        fi
    done
    
    log_success "迁移验证通过"
    return 0
}

# 启动应用
start_application() {
    log_info "启动应用..."
    
    # 执行传入的命令或默认启动命令
    if [ $# -gt 0 ]; then
        exec "$@"
    else
        # 默认启动命令
        exec /app/synapse-rust
    fi
}

# 主函数
main() {
    log_info "=========================================="
    log_info "synapse-rust 容器启动"
    log_info "版本: $(date '+%Y-%m-%d %H:%M:%S')"
    log_info "=========================================="
    
    # 等待数据库
    if ! wait_for_db; then
        log_error "数据库连接失败，退出"
        exit 1
    fi
    
    # 执行迁移（如果启用）
    if [ "${RUN_MIGRATIONS:-true}" = "true" ]; then
        if ! run_migrations; then
            log_error "迁移失败，退出"
            exit 1
        fi
        
        # 验证迁移
        if [ "${VERIFY_MIGRATIONS:-true}" = "true" ]; then
            if ! verify_migrations; then
                log_error "迁移验证失败，退出"
                exit 1
            fi
        fi
    else
        log_info "跳过数据库迁移 (RUN_MIGRATIONS=false)"
    fi
    
    log_info "=========================================="
    log_info "启动应用服务"
    log_info "=========================================="
    
    # 启动应用
    start_application "$@"
}

main "$@"
