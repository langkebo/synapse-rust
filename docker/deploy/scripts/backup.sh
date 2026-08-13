#!/bin/bash
# =============================================================================
# 备份脚本
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

# 加载环境变量
if [ -f ".env" ]; then
    source .env
fi

# 备份目录
BACKUP_DIR="${BACKUP_DIR:-./backups}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_NAME="synapse_backup_${TIMESTAMP}"

# 颜色定义
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
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
    echo -e "${RED}[ERROR]${NC} $1" >&2
}

# 创建备份目录
mkdir -p "$BACKUP_DIR/$BACKUP_NAME"

# 备份数据库
backup_database() {
    log_info "备份数据库..."

    if compose ps --status running postgres | grep -q postgres; then
        compose exec -T postgres pg_dump -U "${POSTGRES_USER:-postgres}" "${POSTGRES_DB:-synapse}" >"$BACKUP_DIR/$BACKUP_NAME/database.sql"
    else
        log_warning "PostgreSQL 容器未运行，跳过数据库备份"
    fi

    log_success "数据库备份完成"
}

# 备份媒体文件
backup_media() {
    log_info "备份媒体文件..."

    mkdir -p media
    tar czf "$BACKUP_DIR/$BACKUP_NAME/media.tar.gz" -C media .

    log_success "媒体文件备份完成"
}

# 备份配置
backup_config() {
    log_info "备份配置文件..."

    if [ -f .env ]; then
        cp .env "$BACKUP_DIR/$BACKUP_NAME/.env"
    else
        log_warning ".env 不存在，跳过 .env 备份"
    fi

    for dir in config nginx scripts; do
        if [ -d "$dir" ]; then
            cp -r "$dir" "$BACKUP_DIR/$BACKUP_NAME/"
        else
            log_warning "$dir 目录不存在，跳过"
        fi
    done

    [ -f docker-compose.yml ] && cp docker-compose.yml "$BACKUP_DIR/$BACKUP_NAME/"

    log_success "配置文件备份完成"
}

# 创建压缩包
create_archive() {
    log_info "创建压缩包..."

    tar czf "$BACKUP_DIR/$BACKUP_NAME.tar.gz" -C "$BACKUP_DIR" "$BACKUP_NAME"
    # 清理临时目录：归档已生成，清理失败不应阻断备份/部署（环境 safe-delete
    # hook 可能拦截 rm -rf，此时保留临时目录、仅告警）。
    if ! rm -rf "$BACKUP_DIR/$BACKUP_NAME" 2>/dev/null; then
        log_warning "临时备份目录清理未完成（可能被 safe-delete hook 拦截）: $BACKUP_DIR/$BACKUP_NAME"
    fi

    log_success "压缩包创建完成: $BACKUP_DIR/$BACKUP_NAME.tar.gz"
}

# 备份保留策略：仅保留最近 N 个备份包，防止 backups/ 目录随时间无界增长（每包约 160MB）。
KEEP_BACKUPS="${BACKUP_KEEP_COUNT:-5}"

prune_old_backups() {
    log_info "清理过期备份（保留最近 ${KEEP_BACKUPS} 个）..."
    local count
    count="$(ls -1 "$BACKUP_DIR"/synapse_backup_*.tar.gz 2>/dev/null | wc -l | tr -d ' ')"
    if [ "$count" -le "$KEEP_BACKUPS" ]; then
        return 0
    fi
    # 按名称倒序（时间戳递增），跳过前 KEEP_BACKUPS 个，删除其余旧包。
    local keep=$KEEP_BACKUPS
    for old in $(ls -1 "$BACKUP_DIR"/synapse_backup_*.tar.gz 2>/dev/null | sort -r); do
        if [ "$keep" -gt 0 ]; then
            keep=$((keep - 1))
            continue
        fi
        if ! rm -f "$old" 2>/dev/null; then
            log_warning "旧备份清理未完成（可能被 safe-delete hook 拦截）: $old"
        else
            log_info "已删除旧备份: $(basename "$old")"
        fi
    done
}

# 主函数
main() {
    log_info "开始备份..."

    backup_database
    backup_media
    backup_config
    create_archive
    prune_old_backups

    log_success "备份完成!"
    echo "备份文件: $BACKUP_DIR/$BACKUP_NAME.tar.gz"
    echo "文件大小: $(du -h "$BACKUP_DIR/$BACKUP_NAME.tar.gz" | cut -f1)"
}

main "$@"
