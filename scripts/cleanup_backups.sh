#!/bin/bash
# 定期清理备份文件脚本
# 用途: 清理过期的备份文件和临时文件
# 使用: ./scripts/cleanup_backups.sh [days]

set -e

RETENTION_DAYS=${1:-30}
PROJECT_ROOT="/home/tzd/synapse-rust"
BACKUP_DIRS=("backup_*" "logs" "tmp")
DRY_RUN=false

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1"
}

cleanup_old_backups() {
    log "清理超过 $RETENTION_DAYS 天的备份文件..."
    
    find "$PROJECT_ROOT" -type d -name "backup_*" -mtime +$RETENTION_DAYS 2>/dev/null | while read dir; do
        if [ "$DRY_RUN" = true ]; then
            log "[DRY RUN] 将删除: $dir"
        else
            log "删除: $dir"
            rm -rf "$dir"
        fi
    done
}

cleanup_old_logs() {
    log "清理超过 $RETENTION_DAYS 天的日志文件..."
    
    find "$PROJECT_ROOT/logs" -type f -name "*.log" -mtime +$RETENTION_DAYS 2>/dev/null | while read file; do
        if [ "$DRY_RUN" = true ]; then
            log "[DRY RUN] 将删除: $file"
        else
            log "删除: $file"
            rm -f "$file"
        fi
    done
}

cleanup_temp_files() {
    log "清理临时文件..."
    
    find "$PROJECT_ROOT" -type f \( -name "*.tmp" -o -name "*.bak" -o -name "*~" \) -mtime +7 2>/dev/null | while read file; do
        if [ "$DRY_RUN" = true ]; then
            log "[DRY RUN] 将删除: $file"
        else
            log "删除: $file"
            rm -f "$file"
        fi
    done
}

cleanup_docker_volumes() {
    log "清理未使用的Docker卷..."
    
    if command -v docker &> /dev/null; then
        if [ "$DRY_RUN" = true ]; then
            docker volume ls -qf dangling=true
        else
            docker volume prune -f
        fi
    fi
}

main() {
    log "========================================"
    log "开始清理备份文件"
    log "========================================"
    log "保留天数: $RETENTION_DAYS"
    log "项目根目录: $PROJECT_ROOT"
    log "模式: $([ "$DRY_RUN" = true ] && echo "DRY RUN" || echo "EXECUTE")"
    log ""
    
    cleanup_old_backups
    cleanup_old_logs
    cleanup_temp_files
    cleanup_docker_volumes
    
    log ""
    log "========================================"
    log "清理完成"
    log "========================================"
}

case "$1" in
    --dry-run|-n)
        DRY_RUN=true
        main
        ;;
    --help|-h)
        echo "用法: $0 [选项] [天数]"
        echo ""
        echo "选项:"
        echo "  --dry-run, -n    仅显示将删除的文件，不实际删除"
        echo "  --help, -h       显示帮助信息"
        echo ""
        echo "参数:"
        echo "  天数             备份保留天数 (默认: 30)"
        echo ""
        echo "示例:"
        echo "  $0              # 清理超过30天的备份"
        echo "  $0 7            # 清理超过7天的备份"
        echo "  $0 --dry-run    # 仅显示将删除的文件"
        ;;
    *)
        main
        ;;
esac
