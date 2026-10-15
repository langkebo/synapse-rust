#!/bin/bash
# ============================================================================
# Database Backup Script
# ============================================================================
# 功能: 备份 PostgreSQL 数据库和 Redis 数据
# 使用: ./backup_database.sh
#
# 依赖: docker, pg_dump, gzip
# 环境变量:
#   BACKUP_DIR - 备份目录 (默认: ./backups，与仓库同级的持久目录)
#                ⚠️ 不要指向 /tmp：那是易失目录，重启/清理即丢备份。
#                生产上必须指向**数据卷之外**的挂载点（异地/对象存储见 BACKUP_OFFSITE_*）。
#   KEEP_DAYS  - 保留天数 (默认: 7)
#   PG_CONTAINER - PostgreSQL 容器名 (默认: synapse-postgres)
#   REDIS_CONTAINER - Redis 容器名 (默认: synapse-redis)
#   BACKUP_OFFSITE_REMOTE - 可选，rclone remote:path；设置后会把备份同步到异地
#   BACKUP_MIN_BYTES - 备份体积下限 (默认: 1024)，低于它即判失败（防"空备份记成功"）
#
# Crontab 示例 (每日凌晨 2 点执行):
# SCRIPT_DIR=/path/to/synapse-rust/scripts
# 0 2 * * * ${SCRIPT_DIR}/backup_database.sh >> /var/log/db_backup.log 2>&1
# ============================================================================

# ⚠️ `pipefail` 不可省：本脚本用 `docker exec … pg_dump | …` 这类管道。
# 没有它时管道退出码取自**最后一个命令**，`if` 又会屏蔽 `set -e` —— 实测（2026-10-15）
# pg_dump 失败会产出**空备份**并打印 "backup completed"，即"静默空备份"。
# 详见 docs/audit/DB_REVIEW_2026-10-15.md §1 与 DB_OPTIMIZATION_PLAN_2026-10-15.md §B1.3。
set -eo pipefail

# 配置
# ⚠️ 默认值从 /tmp/backups 改为仓库同级 ./backups：/tmp 是易失目录（重启/系统清理即丢），
# 而备份的全部价值就是"数据没了它还在"。生产请显式指向数据卷之外的挂载点。
BACKUP_DIR="${BACKUP_DIR:-./backups}"
KEEP_DAYS="${KEEP_DAYS:-7}"
PG_CONTAINER="${PG_CONTAINER:-synapse-postgres}"
REDIS_CONTAINER="${REDIS_CONTAINER:-synapse-redis}"
COMPRESS="${COMPRESS:-true}"
# 体积下限：低于它即判失败（空库场景用 BACKUP_MIN_BYTES=1 显式放行，而不是默认放行）
BACKUP_MIN_BYTES="${BACKUP_MIN_BYTES:-1024}"
# 异地同步（可选）：设置后调用 rclone；未设置时**显式打印 SKIP**，不静默跳过
BACKUP_OFFSITE_REMOTE="${BACKUP_OFFSITE_REMOTE:-}"

# 日期格式
DATE=$(date +%Y%m%d_%H%M%S)
# 日志跟随 BACKUP_DIR（而不是独立的 /tmp/backups）：否则"备份目录改了、日志还写易失目录"，
# 且 LOG_FILE 的目录必须在**第一条日志之前**就存在 —— 旧版把它放在 main() 里 mkdir，
# 于是 `/tmp/backups` 不存在时脚本在第一条 log_info 就因 set -e 退出（实测 2026-10-15，
# 新旧版都会，属既有缺陷）。下面这行把它前置。
LOG_FILE="${LOG_FILE:-$BACKUP_DIR/backup.log}"
mkdir -p "$(dirname "$LOG_FILE")" 2>/dev/null || true

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
    echo "[INFO] $(date '+%Y-%m-%d %H:%M:%S') - $1" >>"$LOG_FILE"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
    echo "[SUCCESS] $(date '+%Y-%m-%d %H:%M:%S') - $1" >>"$LOG_FILE"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
    echo "[WARNING] $(date '+%Y-%m-%d %H:%M:%S') - $1" >>"$LOG_FILE"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
    echo "[ERROR] $(date '+%Y-%m-%d %H:%M:%S') - $1" >>"$LOG_FILE"
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

    # `pg_dump -Fc` 输出**已经是压缩的自定义格式**（外层再 gzip 属重复压缩），故直接落盘。
    # ⚠️ 用**重定向**而不是管道：退出码来自 pg_dump 本身；也**不吞 stderr** ——
    #    `2>/dev/null` 会让"认证失败 / 库不存在 / 权限不足"这些真正原因彻底消失。
    if docker exec "$PG_CONTAINER" pg_dump -U synapse -d synapse -Fc -b >"$PG_BACKUP_FILE"; then
        PG_BACKUP_FINAL="$PG_BACKUP_FILE"
    else
        log_error "PostgreSQL backup failed: pg_dump 非零退出"
        exit 1
    fi

    # ① 体积下限：防"命令成功但产物为空"（这正是修复前的故障形态）
    PG_BYTES=$(wc -c <"$PG_BACKUP_FINAL" | tr -d '[:space:]')
    if [ "$PG_BYTES" -lt "$BACKUP_MIN_BYTES" ]; then
        log_error "PostgreSQL backup too small: ${PG_BYTES}B < ${BACKUP_MIN_BYTES}B（疑似空备份），判失败"
        log_error "  （空库场景请显式设 BACKUP_MIN_BYTES=1 放行，而不是让本检查默认放行）"
        exit 1
    fi

    # ② 可读性校验：自定义格式归档必须能被 pg_restore 列出目录（廉价校验，秒级）
    if ! docker exec -i "$PG_CONTAINER" pg_restore --list <"$PG_BACKUP_FINAL" >/dev/null 2>&1; then
        log_error "PostgreSQL backup is not restorable（pg_restore --list 失败）: $PG_BACKUP_FINAL"
        exit 1
    fi

    log_success "PostgreSQL backup completed: $PG_BACKUP_FINAL"

    # 记录备份文件大小
    PG_SIZE=$(du -h "$PG_BACKUP_FINAL" | cut -f1)
    log_info "PostgreSQL backup size: $PG_SIZE (${PG_BYTES} bytes)"

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
            gzip -c "/tmp/synapse_redis_${DATE}.rdb" >"$REDIS_BACKUP_COMPRESSED"
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
    # 异地同步（可选；未配置时**显式 SKIP**，绝不静默跳过）
    # ----------------------------------------------------------------------------
    if [ -n "$BACKUP_OFFSITE_REMOTE" ]; then
        log_info "Offsite sync → $BACKUP_OFFSITE_REMOTE ..."
        if command -v rclone >/dev/null 2>&1 && rclone copy "$PG_BACKUP_FINAL" "$BACKUP_OFFSITE_REMOTE/"; then
            log_success "Offsite sync completed"
        else
            # 失败必须非零退出：本机备份成功但异地没成功，等于"主机损毁仍然全失"，
            # 这种情况报绿会让运维误以为有异地副本。
            log_error "异地同步失败（本机备份仍在，但本机损毁即全失）: $BACKUP_OFFSITE_REMOTE"
            exit 1
        fi
    else
        log_warning "SKIP offsite sync：未设置 BACKUP_OFFSITE_REMOTE —— 备份与数据同机，主机损毁会同时丢失两者"
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
    cat >"$MANIFEST_FILE" <<EOF
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
docker exec $PG_CONTAINER pg_dump -U synapse -d synapse -Fc -b > $PG_BACKUP_FINAL
体积下限检查: >= ${BACKUP_MIN_BYTES} bytes
可读性校验: docker exec -i $PG_CONTAINER pg_restore --list < $PG_BACKUP_FINAL

## Restore Command
docker exec -i $PG_CONTAINER pg_restore -U synapse -d synapse --clean --if-exists < $PG_BACKUP_FINAL

## Offsite
Remote: ${BACKUP_OFFSITE_REMOTE:-<未配置：备份与数据同机>}

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
