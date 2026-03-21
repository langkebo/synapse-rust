#!/bin/bash
# ============================================================================
# Database Backup Script
# ============================================================================
# 功能: 备份 PostgreSQL 数据库和 Redis 数据
# 使用: ./backup_database.sh
#
# 依赖: docker, pg_dump, gzip
# 环境变量:
#   BACKUP_DIR - 备份目录 (默认: /tmp/backups)
#   KEEP_DAYS  - 保留天数 (默认: 7)
#   PG_CONTAINER - PostgreSQL 容器名 (默认: synapse-postgres)
#   REDIS_CONTAINER - Redis 容器名 (默认: synapse-redis)
#
# Crontab 示例 (每日凌晨 2 点执行):
# 0 2 * * * /Users/ljf/Desktop/hu/synapse-rust/scripts/backup_database.sh >> /var/log/db_backup.log 2>&1
# ============================================================================

set -e

# 配置
BACKUP_DIR="${BACKUP_DIR:-/tmp/backups}"
KEEP_DAYS="${KEEP_DAYS:-7}"
PG_CONTAINER="${PG_CONTAINER:-synapse-postgres}"
REDIS_CONTAINER="${REDIS_CONTAINER:-synapse-redis}"
COMPRESS="${COMPRESS:-true}"

# 日期格式
DATE=$(date +%Y%m%d_%H%M%S)
LOG_FILE="${LOG_FILE:-/tmp/backups/backup.log}"

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# ----------------------------------------------------------------------------
# 函数定义
# ----------------------------------------------------------------------------

log_info() {
    echo -e "${BLUE}[INFO]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
    echo "[INFO] $(date '+%Y-%m-%d %H:%M:%S') - $1" >> "$LOG_FILE"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
    echo "[SUCCESS] $(date '+%Y-%m-%d %H:%M:%S') - $1" >> "$LOG_FILE"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
    echo "[WARNING] $(date '+%Y-%m-%d %H:%M:%S') - $1" >> "$LOG_FILE"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
    echo "[ERROR] $(date '+%Y-%m-%d %H:%M:%S') - $1" >> "$LOG_FILE"
}

cleanup() {
    local exit_code=$?
    if [ $exit_code -ne 0 ]; then
        log_error "Backup script failed with exit code $exit_code"
    fi
    exit $exit_code
}

trap cleanup EXIT

# ----------------------------------------------------------------------------
# 主流程
# ----------------------------------------------------------------------------

main() {
    log_info "=========================================="
    log_info "Database Backup Script Started"
    log_info "=========================================="
    log_info "Backup directory: $BACKUP_DIR"
    log_info "Keep days: $KEEP_DAYS"
    log_info "Date: $DATE"
    echo ""

    # 创建备份目录
    mkdir -p "$BACKUP_DIR"
    mkdir -p "$(dirname "$LOG_FILE")"

    # 检查 docker 是否可用
    if ! docker ps --format '{{.Names}}' | grep -q "$PG_CONTAINER"; then
        log_error "PostgreSQL container '$PG_CONTAINER' is not running"
        exit 1
    fi

    # ----------------------------------------------------------------------------
    # PostgreSQL 备份
    # ----------------------------------------------------------------------------
    log_info "Starting PostgreSQL backup..."

    PG_BACKUP_FILE="$BACKUP_DIR/synapse_pg_${DATE}.dump"
    PG_BACKUP_COMPRESSED="${PG_BACKUP_FILE}.gz"

    if [ "$COMPRESS" = "true" ]; then
        # 使用压缩格式备份
        if docker exec "$PG_CONTAINER" pg_dump -U synapse -d synapse -Fc -b 2>/dev/null | gzip > "$PG_BACKUP_COMPRESSED"; then
            log_success "PostgreSQL backup completed: $PG_BACKUP_COMPRESSED"
            PG_BACKUP_FINAL="$PG_BACKUP_COMPRESSED"
        else
            log_error "PostgreSQL backup failed"
            exit 1
        fi
    else
        # 无压缩备份
        if docker exec "$PG_CONTAINER" pg_dump -U synapse -d synapse -Fc -b -v -f "/tmp/synapse_pg_${DATE}.dump" 2>/dev/null; then
            docker cp "$PG_CONTAINER:/tmp/synapse_pg_${DATE}.dump" "$PG_BACKUP_FILE"
            rm -f "/tmp/synapse_pg_${DATE}.dump" 2>/dev/null || true
            log_success "PostgreSQL backup completed: $PG_BACKUP_FILE"
            PG_BACKUP_FINAL="$PG_BACKUP_FILE"
        else
            log_error "PostgreSQL backup failed"
            exit 1
        fi
    fi

    # 记录备份文件大小
    PG_SIZE=$(du -h "$PG_BACKUP_FINAL" | cut -f1)
    log_info "PostgreSQL backup size: $PG_SIZE"

    # ----------------------------------------------------------------------------
    # Redis 备份
    # ----------------------------------------------------------------------------
    log_info "Starting Redis backup..."

    REDIS_BACKUP_FILE="$BACKUP_DIR/synapse_redis_${DATE}.rdb"
    REDIS_BACKUP_COMPRESSED="${REDIS_BACKUP_FILE}.gz"

    # Redis BGSAVE 触发后台保存
    docker exec "$REDIS_CONTAINER" redis-cli BGSAVE 2>/dev/null || true

    # 等待保存完成 (最多 30 秒)
    for i in $(seq 1 30); do
        if docker exec "$REDIS_CONTAINER" redis-cli LASTSAVE 2>/dev/null | grep -q "^1"; then
            sleep 1
            break
        fi
        sleep 1
    done

    # 复制 Redis dump 文件
    if docker cp "$REDIS_CONTAINER:/data/dump.rdb" "/tmp/synapse_redis_${DATE}.rdb" 2>/dev/null; then
        if [ "$COMPRESS" = "true" ]; then
            gzip -c "/tmp/synapse_redis_${DATE}.rdb" > "$REDIS_BACKUP_COMPRESSED"
            rm -f "/tmp/synapse_redis_${DATE}.rdb"
            log_success "Redis backup completed: $REDIS_BACKUP_COMPRESSED"
            REDIS_BACKUP_FINAL="$REDIS_BACKUP_COMPRESSED"
        else
            mv "/tmp/synapse_redis_${DATE}.rdb" "$REDIS_BACKUP_FILE"
            log_success "Redis backup completed: $REDIS_BACKUP_FILE"
            REDIS_BACKUP_FINAL="$REDIS_BACKUP_FILE"
        fi
    else
        log_warning "Redis backup failed, Redis may not support backup"
        REDIS_BACKUP_FINAL=""
    fi

    # ----------------------------------------------------------------------------
    # 清理旧备份
    # ----------------------------------------------------------------------------
    log_info "Cleaning up backups older than $KEEP_DAYS days..."

    # 清理 PostgreSQL 备份
    find "$BACKUP_DIR" -name "synapse_pg_*" -type f -mtime +"$KEEP_DAYS" -delete 2>/dev/null || true

    # 清理 Redis 备份
    find "$BACKUP_DIR" -name "synapse_redis_*" -type f -mtime +"$KEEP_DAYS" -delete 2>/dev/null || true

    # 清理旧的日志文件
    find "$(dirname "$LOG_FILE")" -name "backup.log*" -type f -mtime +"$KEEP_DAYS" -delete 2>/dev/null || true

    log_info "Cleanup completed"

    # ----------------------------------------------------------------------------
    # 列出当前备份
    # ----------------------------------------------------------------------------
    echo ""
    log_info "Current backups:"
    echo "----------------------------------------"
    ls -lh "$BACKUP_DIR"/synapse_* 2>/dev/null | tail -20 || log_info "No backups found"
    echo "----------------------------------------"

    # ----------------------------------------------------------------------------
    # 生成备份清单
    # ----------------------------------------------------------------------------
    MANIFEST_FILE="$BACKUP_DIR/backup_manifest_${DATE}.txt"
    cat > "$MANIFEST_FILE" << EOF
# Database Backup Manifest
# Date: $DATE
# Generated: $(date '+%Y-%m-%d %H:%M:%S')

## PostgreSQL Backup
File: $PG_BACKUP_FINAL
Size: $PG_SIZE
Compressed: $COMPRESS

## Redis Backup
File: $REDIS_BACKUP_FINAL
Compressed: $COMPRESS

## Backup Command
docker exec $PG_CONTAINER pg_dump -U synapse -d synapse -Fc -b | gzip > $PG_BACKUP_COMPRESSED

## Restore Command
gunzip -c $PG_BACKUP_COMPRESSED | docker exec -i $PG_CONTAINER pg_restore -U synapse -d synapse

## Metadata
PostgreSQL Version: $(docker exec "$PG_CONTAINER" psql -V 2>/dev/null || echo "unknown")
Redis Version: $(docker exec "$REDIS_CONTAINER" redis-cli INFO server 2>/dev/null | grep redis_version || echo "unknown")
Backup Host: $(hostname)
EOF

    log_success "Backup manifest created: $MANIFEST_FILE"

    # ----------------------------------------------------------------------------
    # 完成
    # ----------------------------------------------------------------------------
    echo ""
    log_info "=========================================="
    log_success "Database Backup Completed Successfully"
    log_info "=========================================="
    log_info "All backups are stored in: $BACKUP_DIR"
    echo ""
}

# 运行主函数
main "$@"
