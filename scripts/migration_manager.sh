#!/bin/bash
# 迁移脚本管理工具
# 用途: 检查和优化数据库迁移脚本
# 使用: ./scripts/migration_manager.sh [command]

set -e

PROJECT_ROOT="/home/tzd/synapse-rust"
MIGRATIONS_DIR="$PROJECT_ROOT/migrations"

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1"
}

list_migrations() {
    log "当前迁移脚本列表:"
    echo ""
    ls -la "$MIGRATIONS_DIR"/*.sql 2>/dev/null | awk '{print $NF, $5}' | column -t
    echo ""
    echo "总计: $(ls "$MIGRATIONS_DIR"/*.sql 2>/dev/null | wc -l) 个迁移文件"
}

check_migration_naming() {
    log "检查迁移脚本命名规范..."
    
    local issues=0
    
    for file in "$MIGRATIONS_DIR"/*.sql; do
        filename=$(basename "$file")
        
        if [[ ! "$filename" =~ ^[0-9]{8}_[a-z_]+\.sql$ ]] && [[ ! "$filename" =~ ^00000000_.*\.sql$ ]]; then
            log "⚠️  命名不规范: $filename"
            ((issues++))
        fi
    done
    
    if [ $issues -eq 0 ]; then
        log "✅ 所有迁移脚本命名规范正确"
    else
        log "❌ 发现 $issues 个命名不规范的文件"
    fi
}

check_migration_content() {
    log "检查迁移脚本内容..."
    
    local issues=0
    
    for file in "$MIGRATIONS_DIR"/*.sql; do
        filename=$(basename "$file")
        
        if grep -q "creation_ts" "$file" 2>/dev/null; then
            log "⚠️  使用旧字段名 creation_ts: $filename"
            ((issues++))
        fi
        
        if ! grep -q "BEGIN" "$file" 2>/dev/null && ! grep -q "START TRANSACTION" "$file" 2>/dev/null; then
            if [ "$filename" != "00000000_unified_schema_v5.sql" ]; then
                log "⚠️  缺少事务控制: $filename"
                ((issues++))
            fi
        fi
    done
    
    if [ $issues -eq 0 ]; then
        log "✅ 所有迁移脚本内容检查通过"
    else
        log "❌ 发现 $issues 个内容问题"
    fi
}

optimize_migrations() {
    log "优化迁移脚本..."
    
    local merged_count=0
    
    for file in "$MIGRATIONS_DIR"/202603*.sql; do
        if [ -f "$file" ]; then
            log "建议合并: $(basename "$file")"
            ((merged_count++))
        fi
    done
    
    if [ $merged_count -gt 0 ]; then
        log "发现 $merged_count 个可合并的迁移脚本"
        log "建议: 将这些迁移合并到主schema文件中"
    else
        log "✅ 迁移脚本已优化"
    fi
}

validate_schema() {
    log "验证数据库schema..."
    
    docker exec synapse-postgres psql -U synapse -d synapse_test -c "
        SELECT table_name, column_name 
        FROM information_schema.columns 
        WHERE table_schema = 'public' 
        AND column_name = 'creation_ts'
        LIMIT 10;
    " 2>/dev/null || log "⚠️  无法连接数据库"
}

main() {
    log "========================================"
    log "迁移脚本管理工具"
    log "========================================"
    echo ""
    
    case "${1:-status}" in
        list)
            list_migrations
            ;;
        check)
            check_migration_naming
            echo ""
            check_migration_content
            ;;
        optimize)
            optimize_migrations
            ;;
        validate)
            validate_schema
            ;;
        status|*)
            list_migrations
            echo ""
            check_migration_naming
            echo ""
            check_migration_content
            ;;
    esac
    
    echo ""
    log "========================================"
    log "检查完成"
    log "========================================"
}

case "$1" in
    --help|-h)
        echo "用法: $0 [命令]"
        echo ""
        echo "命令:"
        echo "  status     显示迁移状态 (默认)"
        echo "  list       列出所有迁移脚本"
        echo "  check      检查迁移脚本规范"
        echo "  optimize   分析可优化的迁移"
        echo "  validate   验证数据库schema"
        echo "  --help     显示帮助信息"
        ;;
    *)
        main "$@"
        ;;
esac
