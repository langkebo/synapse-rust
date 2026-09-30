#!/bin/bash
# =============================================================================
# 恢复脚本
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

compose() {
    if command -v docker-compose &>/dev/null; then
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
        compose exec -T postgres psql -U "${POSTGRES_USER:-postgres}" -d "${POSTGRES_DB:-synapse}" <"$backup_dir/database.sql"
    else
        log_warning "数据库备份文件不存在，跳过数据库恢复"
    fi

    log_success "数据库恢复完成"
}

# 恢复媒体文件
restore_media() {
    local backup_dir="$1"

    log_info "恢复媒体文件..."

    if [ ! -f "$backup_dir/media.tar.gz" ]; then
        log_warning "媒体备份文件不存在，跳过媒体恢复"
        return 0
    fi

    mkdir -p media

    # 先解压到 staging 目录，校验成功后再原子替换 live 目录，
    # 避免"先清空 media 再解压、解压失败导致媒体全部丢失"。
    local staging="media.restore.$$"
    rm -rf "$staging"
    mkdir -p "$staging"
    if ! tar xzf "$backup_dir/media.tar.gz" -C "$staging"; then
        log_error "媒体解压失败，保留现有媒体文件不做改动"
        rm -rf "$staging"
        return 1
    fi

    local old="media.old.$$"
    mv media "$old"
    mv "$staging" media
    if ! rm -rf "$old" 2>/dev/null; then
        log_warning "旧媒体目录清理未完成（可能被 safe-delete hook 拦截）: $old"
    fi

    log_success "媒体文件恢复完成"
}

# 恢复配置
restore_config() {
    local backup_dir="$1"

    log_info "恢复配置文件..."

    if [ -f "$backup_dir/.env" ]; then
        cp "$backup_dir/.env" .env
        # 备份快照可能带启用态 MALLOC_CONF（jemalloc profiler）：一旦还原，synapse
        # 会持续向 synapse-data/ 落 heap dump 直到撑满磁盘。它是临时诊断开关而非部署
        # 配置，故还原后一律强制注释，避免任何来历的备份把它灌回 live。
        if grep -qE '^[[:space:]]*MALLOC_CONF=' .env; then
            local tmp_env
            tmp_env="$(mktemp)"
            sed 's/^[[:space:]]*MALLOC_CONF=/#MALLOC_CONF=/' .env >"$tmp_env"
            cp "$tmp_env" .env
            rm -f "$tmp_env" 2>/dev/null || true
            log_warning "已强制注释还原的 MALLOC_CONF（jemalloc profiler 不随备份恢复）"
        fi
    fi
    # canonical 配置真相源位于 ../config（= docker/config），还原到上级目录。
    [ -d "$backup_dir/config" ] && rm -rf ../config && cp -r "$backup_dir/config" ..
    [ -d "$backup_dir/nginx" ] && rm -rf nginx && cp -r "$backup_dir/nginx" ./
    # 不还原 scripts/：restore.sh 自身即位于 scripts/ 内，覆盖会中断当前脚本并回退 live 脚本。
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
