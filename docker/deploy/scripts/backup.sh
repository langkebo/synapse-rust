#!/bin/bash
# =============================================================================
# 备份脚本
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
        compose exec -T postgres pg_dump -U "${POSTGRES_USER:-postgres}" "${POSTGRES_DB:-synapse}" > "$BACKUP_DIR/$BACKUP_NAME/database.sql"
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
    rm -rf "$BACKUP_DIR/$BACKUP_NAME"
    
    log_success "压缩包创建完成: $BACKUP_DIR/$BACKUP_NAME.tar.gz"
}

# 主函数
main() {
    log_info "开始备份..."

    backup_database
    backup_media
    backup_config
    create_archive

    log_success "备份完成!"
    echo "备份文件: $BACKUP_DIR/$BACKUP_NAME.tar.gz"
    echo "文件大小: $(du -h "$BACKUP_DIR/$BACKUP_NAME.tar.gz" | cut -f1)"
}

main "$@"
