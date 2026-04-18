#!/bin/bash
# =============================================================================
# 恢复脚本
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

compose() {
    if command -v docker-compose &> /dev/null; then
        docker-compose "$@"
    else
        docker compose "$@"
    fi
}

# 颜色定义
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

# 检查备份文件
if [ -z "$1" ]; then
    log_error "请指定备份文件"
    echo "用法: $0 <backup_file.tar.gz>"
    exit 1
fi

BACKUP_FILE="$1"

if [ ! -f "$BACKUP_FILE" ]; then
    log_error "备份文件不存在: $BACKUP_FILE"
    exit 1
fi

# 确认恢复
if [ "${RESTORE_FORCE:-false}" != "true" ]; then
    log_warning "此操作将覆盖现有数据!"
    read -r -p "确认要恢复吗? (yes/no): " confirm

    if [ "$confirm" != "yes" ]; then
        log_info "操作已取消"
        exit 0
    fi
fi

# 解压备份
extract_backup() {
    log_info "解压备份文件..."
    
    BACKUP_DIR=$(dirname "$BACKUP_FILE")
    BACKUP_NAME=$(basename "$BACKUP_FILE" .tar.gz)
    
    tar xzf "$BACKUP_FILE" -C "$BACKUP_DIR"
    
    echo "$BACKUP_DIR/$BACKUP_NAME"
}

# 恢复数据库
restore_database() {
    local backup_dir="$1"
    
    log_info "恢复数据库..."
    
    # 停止应用
    compose stop synapse
    
    # 恢复数据库
    if [ -f "$backup_dir/database.sql" ]; then
        compose exec -T postgres psql -U "${POSTGRES_USER:-postgres}" -d "${POSTGRES_DB:-synapse}" < "$backup_dir/database.sql"
    else
        log_warning "数据库备份文件不存在，跳过数据库恢复"
    fi
    
    log_success "数据库恢复完成"
}

# 恢复媒体文件
restore_media() {
    local backup_dir="$1"
    
    log_info "恢复媒体文件..."
    
    mkdir -p media
    rm -rf media/*
    if [ -f "$backup_dir/media.tar.gz" ]; then
        tar xzf "$backup_dir/media.tar.gz" -C media
    else
        log_warning "媒体备份文件不存在，跳过媒体恢复"
    fi
    
    log_success "媒体文件恢复完成"
}

# 恢复配置
restore_config() {
    local backup_dir="$1"
    
    log_info "恢复配置文件..."

    [ -f "$backup_dir/.env" ] && cp "$backup_dir/.env" .env
    [ -d "$backup_dir/config" ] && rm -rf config && cp -r "$backup_dir/config" ./
    [ -d "$backup_dir/nginx" ] && rm -rf nginx && cp -r "$backup_dir/nginx" ./
    [ -d "$backup_dir/scripts" ] && rm -rf scripts && cp -r "$backup_dir/scripts" ./
    [ -f "$backup_dir/docker-compose.yml" ] && cp "$backup_dir/docker-compose.yml" ./
    
    log_success "配置文件恢复完成"
}

# 主函数
main() {
    log_info "开始恢复..."
    
    backup_dir=$(extract_backup)
    
    restore_database "$backup_dir"
    restore_media "$backup_dir"
    restore_config "$backup_dir"
    
    # 清理
    rm -rf "$backup_dir"
    
    # 重启服务
    log_info "重启服务..."
    compose up -d
    
    log_success "恢复完成!"
}

main "$@"
