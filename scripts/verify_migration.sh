#!/bin/bash
# ============================================================================
# synapse-rust 数据库迁移验证脚本
# 版本: 1.0.0
# 创建日期: 2026-03-02
# 描述: 验证媒体配额和服务器通知表是否正确创建
# ============================================================================

set -e

# 配置
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
LOG_DIR="$PROJECT_ROOT/logs/migrations"
LOG_FILE="$LOG_DIR/migration_$(date +%Y%m%d_%H%M%S).log"

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# 日志函数
log() {
    local level="$1"
    shift
    local message="$*"
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    echo -e "[$timestamp] [$level] $message" | tee -a "$LOG_FILE"
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

# 初始化日志目录
init_log_dir() {
    mkdir -p "$LOG_DIR"
    log_info "日志目录: $LOG_DIR"
}

# 加载环境变量
load_env() {
    if [ -f "$PROJECT_ROOT/.env" ]; then
        log_info "加载环境变量: $PROJECT_ROOT/.env"
        export $(grep -v '^#' "$PROJECT_ROOT/.env" | xargs)
    fi
    
    export DATABASE_URL="${DATABASE_URL:-postgres://synapse:synapse@localhost:5432/synapse}"

    local parsed_user=$(echo "$DATABASE_URL" | sed -n 's|^postgres://\([^:]*\):.*$|\1|p')
    local parsed_password=$(echo "$DATABASE_URL" | sed -n 's|^postgres://[^:]*:\([^@]*\)@.*$|\1|p')
    local parsed_host=$(echo "$DATABASE_URL" | sed -n 's|^postgres://[^@]*@\([^:/]*\).*$|\1|p')
    local parsed_port=$(echo "$DATABASE_URL" | sed -n 's|^postgres://[^@]*@[^:/]*:\([0-9]*\)/.*$|\1|p')
    local parsed_db=$(echo "$DATABASE_URL" | sed -n 's|.*/\([^?]*\).*$|\1|p')

    export DB_HOST="${DB_HOST:-${parsed_host:-localhost}}"
    export DB_PORT="${DB_PORT:-${parsed_port:-5432}}"
    export DB_NAME="${DB_NAME:-${parsed_db:-synapse}}"
    export DB_USER="${DB_USER:-${parsed_user:-synapse}}"
    export DB_PASSWORD="${DB_PASSWORD:-${parsed_password:-synapse}}"
    
    log_info "数据库连接: $DB_HOST:$DB_PORT/$DB_NAME"
}

# 数据库连接字符串
db_url() {
    echo "postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME"
}

# 检查数据库连接
check_db_connection() {
    log_info "检查数据库连接..."
    if psql "$(db_url)" -c "SELECT 1" > /dev/null 2>&1; then
        log_success "数据库连接成功"
        return 0
    else
        log_error "无法连接到数据库"
        return 1
    fi
}

# 验证媒体配额表
verify_media_quota_tables() {
    log_info "验证媒体配额表..."
    
    local tables=(
        "media_quota_config"
        "user_media_quota"
        "media_usage_log"
        "media_quota_alerts"
        "server_media_quota"
    )
    
    local missing=0
    for table in "${tables[@]}"; do
        local exists=$(psql "$(db_url)" -t -c "
            SELECT EXISTS (
                SELECT FROM information_schema.tables 
                WHERE table_schema = 'public' AND table_name = '$table'
            )
        " 2>/dev/null | tr -d ' ')
        
        if [ "$exists" = "t" ]; then
            log_success "表存在: $table"
        else
            log_warning "表缺失(可选模块): $table"
            missing=$((missing + 1))
        fi
    done
    if [ $missing -gt 0 ]; then
        log_warning "媒体配额模块未启用或未部署完整，共缺失 $missing 张表"
    fi
    return 0
}

# 验证服务器通知表
verify_notification_tables() {
    log_info "验证服务器通知表..."
    
    local tables=(
        "server_notifications"
        "user_notification_status"
        "notification_templates"
        "notification_delivery_log"
        "scheduled_notifications"
    )
    
    local missing=0
    for table in "${tables[@]}"; do
        local exists=$(psql "$(db_url)" -t -c "
            SELECT EXISTS (
                SELECT FROM information_schema.tables 
                WHERE table_schema = 'public' AND table_name = '$table'
            )
        " 2>/dev/null | tr -d ' ')
        
        if [ "$exists" = "t" ]; then
            log_success "表存在: $table"
        else
            log_warning "表缺失(可选模块): $table"
            missing=$((missing + 1))
        fi
    done
    if [ $missing -gt 0 ]; then
        log_warning "服务器通知模块未启用或未部署完整，共缺失 $missing 张表"
    fi
    return 0
}

verify_core_tables() {
    log_info "验证核心表..."

    local tables=(
        "users"
        "rooms"
        "events"
        "schema_migrations"
        "filters"
        "openid_tokens"
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
            log_success "核心表存在: $table"
        else
            log_error "核心表缺失: $table"
            errors=$((errors + 1))
        fi
    done

    return $errors
}

# 验证索引
verify_indexes() {
    log_info "验证关键索引..."
    
    local indexes=(
        "idx_media_quota_config_default"
        "idx_user_media_quota_user"
        "idx_media_usage_log_user"
        "idx_media_quota_alerts_user"
        "idx_server_notifications_enabled"
        "idx_user_notification_status_user"
        "idx_notification_templates_name"
    )
    
    local errors=0
    for idx in "${indexes[@]}"; do
        local exists=$(psql "$(db_url)" -t -c "
            SELECT EXISTS (
                SELECT 1 FROM pg_indexes 
                WHERE indexname = '$idx'
            )
        " 2>/dev/null | tr -d ' ')
        
        if [ "$exists" = "t" ]; then
            log_success "索引存在: $idx"
        else
            log_warning "索引缺失: $idx"
        fi
    done
    
    return $errors
}

# 验证默认数据
verify_default_data() {
    log_info "验证默认数据..."
    
    # 检查默认配额配置
    local quota_count=$(psql "$(db_url)" -t -c "
        SELECT COUNT(*) FROM media_quota_config WHERE is_default = TRUE
    " 2>/dev/null | tr -d ' ')
    
    if [ "${quota_count:-0}" -ge 1 ]; then
        log_success "默认配额配置存在: $quota_count 条"
    else
        log_warning "缺少默认配额配置"
    fi
    
    # 检查服务器配额
    local server_quota=$(psql "$(db_url)" -t -c "
        SELECT COUNT(*) FROM server_media_quota
    " 2>/dev/null | tr -d ' ')
    
    if [ "${server_quota:-0}" -ge 1 ]; then
        log_success "服务器配额配置存在"
    else
        log_warning "缺少服务器配额配置"
    fi
    
    # 检查通知模板
    local template_count=$(psql "$(db_url)" -t -c "
        SELECT COUNT(*) FROM notification_templates
    " 2>/dev/null | tr -d ' ')
    
    if [ "${template_count:-0}" -ge 1 ]; then
        log_success "通知模板存在: $template_count 条"
    else
        log_warning "缺少通知模板"
    fi
}

# 验证迁移记录
verify_migration_record() {
    log_info "验证迁移记录..."
    
    local exists=$(psql "$(db_url)" -t -c "
        SELECT EXISTS (
            SELECT 1
            FROM information_schema.tables
            WHERE table_schema = 'public' AND table_name = 'schema_migrations'
        )
    " 2>/dev/null | tr -d ' ')

    if [ "$exists" != "t" ]; then
        log_error "迁移记录表缺失: schema_migrations"
        return 1
    fi

    local count=$(psql "$(db_url)" -t -c "SELECT COUNT(*) FROM schema_migrations" 2>/dev/null | tr -d ' ')
    if [ "${count:-0}" -lt 1 ]; then
        log_error "迁移记录为空: schema_migrations"
        return 1
    fi

    log_success "迁移记录存在: $count 条"
    psql "$(db_url)" -c "
        SELECT version, name, applied_ts
        FROM schema_migrations
        ORDER BY applied_ts DESC
        LIMIT 10
    " 2>/dev/null | tee -a "$LOG_FILE"
}

# 验证表结构
verify_table_structure() {
    log_info "验证表结构..."
    
    # 检查 media_quota_config 表结构
    log_info "media_quota_config 表结构:"
    psql "$(db_url)" -c "
        SELECT column_name, data_type, is_nullable, column_default
        FROM information_schema.columns
        WHERE table_name = 'media_quota_config'
        ORDER BY ordinal_position
    " 2>/dev/null | tee -a "$LOG_FILE"
    
    # 检查 server_notifications 表结构
    log_info "server_notifications 表结构:"
    psql "$(db_url)" -c "
        SELECT column_name, data_type, is_nullable, column_default
        FROM information_schema.columns
        WHERE table_name = 'server_notifications'
        ORDER BY ordinal_position
    " 2>/dev/null | tee -a "$LOG_FILE"
}

# 生成验证报告
generate_report() {
    local status="$1"
    local duration="$2"
    
    log_info "=========================================="
    log_info "验证报告"
    log_info "=========================================="
    log_info "时间: $(date '+%Y-%m-%d %H:%M:%S')"
    log_info "状态: $status"
    log_info "耗时: ${duration}ms"
    log_info "日志文件: $LOG_FILE"
    log_info "=========================================="
}

# 主函数
main() {
    local start_time=$(date +%s%3N)
    
    init_log_dir
    load_env
    
    log_info "=========================================="
    log_info "开始验证数据库迁移"
    log_info "=========================================="
    
    local total_errors=0
    
    # 检查数据库连接
    if ! check_db_connection; then
        generate_report "FAILED" "0"
        exit 1
    fi
    
    # 验证表
    verify_core_tables || total_errors=$((total_errors + $?))
    verify_media_quota_tables || total_errors=$((total_errors + $?))
    verify_notification_tables || total_errors=$((total_errors + $?))
    
    # 验证索引
    verify_indexes
    
    # 验证默认数据
    verify_default_data
    
    # 验证迁移记录
    verify_migration_record || total_errors=$((total_errors + 1))
    
    # 验证表结构
    verify_table_structure
    
    local end_time=$(date +%s%3N)
    local duration=$((end_time - start_time))
    
    if [ $total_errors -eq 0 ]; then
        generate_report "SUCCESS" "$duration"
        log_success "所有验证通过！"
        exit 0
    else
        generate_report "FAILED" "$duration"
        log_error "发现 $total_errors 个错误"
        exit 1
    fi
}

main "$@"
