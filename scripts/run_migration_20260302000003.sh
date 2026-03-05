#!/bin/bash
# ============================================================================
# synapse-rust 数据库迁移执行脚本
# 版本: 1.0.0
# 创建日期: 2026-03-02
# 描述: 执行媒体配额和服务器通知表的迁移
# ============================================================================

set -e

# 配置
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
MIGRATIONS_DIR="$PROJECT_ROOT/migrations"
LOG_DIR="$PROJECT_ROOT/logs/migrations"

# 迁移版本
MIGRATION_VERSION="20260302000003"
MIGRATION_FILE="$MIGRATIONS_DIR/${MIGRATION_VERSION}_add_media_quota_and_notification_tables.sql"
ROLLBACK_FILE="$MIGRATIONS_DIR/${MIGRATION_VERSION}_rollback.sql"

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

# 日志函数
log() {
    local level="$1"
    shift
    local message="$*"
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    echo -e "[$timestamp] [$level] $message"
}

log_info() {
    log "INFO" "${BLUE}$*${NC}"
}

log_success() {
    log "SUCCESS" "${GREEN}$*${NC}"
}

log_warning() {
    log "WARNING" "${YELLOW}$*${NC}"
}

log_error() {
    log "ERROR" "${RED}$*${NC}"
}

log_step() {
    log "STEP" "${CYAN}$*${NC}"
}

# 初始化
init() {
    mkdir -p "$LOG_DIR"
    LOG_FILE="$LOG_DIR/migration_${MIGRATION_VERSION}_$(date +%Y%m%d_%H%M%S).log"
    log_info "日志文件: $LOG_FILE"
}

# 加载环境变量
load_env() {
    if [ -f "$PROJECT_ROOT/.env" ]; then
        log_info "加载环境变量: $PROJECT_ROOT/.env"
        export $(grep -v '^#' "$PROJECT_ROOT/.env" | xargs)
    fi
    
    export DB_HOST="${DB_HOST:-localhost}"
    export DB_PORT="${DB_PORT:-5432}"
    export DB_NAME="${DB_NAME:-synapse}"
    export DB_USER="${DB_USER:-synapse}"
    export DB_PASSWORD="${DB_PASSWORD:-synapse}"
    export DATABASE_URL="${DATABASE_URL:-postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME}"
    
    log_info "数据库: $DB_HOST:$DB_PORT/$DB_NAME"
}

# 数据库连接字符串
db_url() {
    echo "$DATABASE_URL"
}

# 检查数据库连接
check_db_connection() {
    log_step "检查数据库连接..."
    
    if psql "$(db_url)" -c "SELECT 1" > /dev/null 2>&1; then
        log_success "数据库连接成功"
        return 0
    else
        log_error "无法连接到数据库"
        return 1
    fi
}

# 检查迁移是否已执行
check_migration_status() {
    log_step "检查迁移状态..."
    
    local exists=$(psql "$(db_url)" -t -c "
        SELECT EXISTS (
            SELECT 1 FROM schema_migrations 
            WHERE version = '$MIGRATION_VERSION'
        )
    " 2>/dev/null | tr -d ' ')
    
    if [ "$exists" = "t" ]; then
        log_warning "迁移 $MIGRATION_VERSION 已执行过"
        return 0
    else
        log_info "迁移 $MIGRATION_VERSION 未执行"
        return 1
    fi
}

# 备份数据库
backup_database() {
    log_step "备份数据库..."
    
    local backup_file="$LOG_DIR/backup_${DB_NAME}_$(date +%Y%m%d_%H%M%S).sql"
    
    if pg_dump "$(db_url)" > "$backup_file" 2>&1; then
        log_success "数据库备份完成: $backup_file"
        echo "$backup_file"
        return 0
    else
        log_warning "数据库备份失败，继续执行..."
        return 1
    fi
}

# 执行迁移
execute_migration() {
    log_step "执行迁移: $MIGRATION_FILE"
    
    local start_time=$(date +%s%3N)
    
    if [ ! -f "$MIGRATION_FILE" ]; then
        log_error "迁移文件不存在: $MIGRATION_FILE"
        return 1
    fi
    
    # 执行 SQL
    if psql "$(db_url)" -f "$MIGRATION_FILE" 2>&1 | tee -a "$LOG_FILE"; then
        local end_time=$(date +%s%3N)
        local duration=$((end_time - start_time))
        
        log_success "迁移执行成功 (${duration}ms)"
        return 0
    else
        log_error "迁移执行失败"
        return 1
    fi
}

# 执行回滚
execute_rollback() {
    log_step "执行回滚: $ROLLBACK_FILE"
    
    if [ ! -f "$ROLLBACK_FILE" ]; then
        log_error "回滚文件不存在: $ROLLBACK_FILE"
        return 1
    fi
    
    log_warning "即将执行回滚，这将删除所有相关数据！"
    read -p "确认回滚? (yes/no): " confirm
    
    if [ "$confirm" != "yes" ]; then
        log_info "取消回滚"
        return 0
    fi
    
    if psql "$(db_url)" -f "$ROLLBACK_FILE" 2>&1 | tee -a "$LOG_FILE"; then
        log_success "回滚执行成功"
        return 0
    else
        log_error "回滚执行失败"
        return 1
    fi
}

# 验证迁移
verify_migration() {
    log_step "验证迁移结果..."
    
    local tables=(
        "media_quota_config"
        "user_media_quota"
        "media_usage_log"
        "media_quota_alerts"
        "server_media_quota"
        "server_notifications"
        "user_notification_status"
        "notification_templates"
        "notification_delivery_log"
        "scheduled_notifications"
    )
    
    local errors=0
    for table in "${tables[@]}"; do
        local exists=$(psql "$(db_url)" -t -c "
            SELECT EXISTS (
                SELECT FROM information_schema.tables 
                WHERE table_schema = 'public' AND table_name = '$table'
            )
        " 2>/dev/null | tr -d ' ')
        
        if [ "$exists" = "t" ]; then
            log_success "表验证通过: $table"
        else
            log_error "表验证失败: $table"
            errors=$((errors + 1))
        fi
    done
    
    if [ $errors -eq 0 ]; then
        log_success "所有表验证通过"
        return 0
    else
        log_error "验证失败，发现 $errors 个错误"
        return 1
    fi
}

# 显示状态
show_status() {
    log_step "迁移状态:"
    
    psql "$(db_url)" -c "
        SELECT version, name, applied_ts, checksum 
        FROM schema_migrations 
        WHERE version LIKE '20260302%'
        ORDER BY version
    " 2>/dev/null
    
    log_info "表统计:"
    psql "$(db_url)" -c "
        SELECT 
            table_name,
            (SELECT COUNT(*) FROM information_schema.columns WHERE table_name = t.table_name) as columns
        FROM information_schema.tables t
        WHERE table_schema = 'public'
        AND table_name IN (
            'media_quota_config', 'user_media_quota', 'media_usage_log',
            'media_quota_alerts', 'server_media_quota',
            'server_notifications', 'user_notification_status',
            'notification_templates', 'notification_delivery_log',
            'scheduled_notifications'
        )
        ORDER BY table_name
    " 2>/dev/null
}

# 显示帮助
show_help() {
    echo "synapse-rust 数据库迁移执行工具"
    echo ""
    echo "用法: $0 <命令>"
    echo ""
    echo "命令:"
    echo "  migrate    执行迁移"
    echo "  rollback   执行回滚"
    echo "  verify     验证迁移结果"
    echo "  status     显示迁移状态"
    echo "  help       显示此帮助"
    echo ""
    echo "环境变量:"
    echo "  DATABASE_URL    数据库连接字符串"
    echo "  DB_HOST         数据库主机"
    echo "  DB_PORT         数据库端口"
    echo "  DB_NAME         数据库名称"
    echo "  DB_USER         数据库用户"
    echo "  DB_PASSWORD     数据库密码"
}

# 主函数
main() {
    local command="${1:-help}"
    
    init
    load_env
    
    case "$command" in
        migrate)
            log_info "=========================================="
            log_info "开始执行迁移: $MIGRATION_VERSION"
            log_info "=========================================="
            
            check_db_connection || exit 1
            
            if check_migration_status; then
                log_info "迁移已存在，跳过执行"
                show_status
                exit 0
            fi
            
            backup_database || true
            execute_migration || exit 1
            verify_migration || exit 1
            show_status
            
            log_success "=========================================="
            log_success "迁移完成！"
            log_success "=========================================="
            ;;
            
        rollback)
            log_info "=========================================="
            log_info "开始执行回滚: $MIGRATION_VERSION"
            log_info "=========================================="
            
            check_db_connection || exit 1
            execute_rollback || exit 1
            show_status
            
            log_success "回滚完成！"
            ;;
            
        verify)
            check_db_connection || exit 1
            verify_migration || exit 1
            show_status
            ;;
            
        status)
            check_db_connection || exit 1
            show_status
            ;;
            
        help|--help|-h)
            show_help
            ;;
            
        *)
            log_error "未知命令: $command"
            show_help
            exit 1
            ;;
    esac
}

main "$@"
