#!/bin/bash
# 字段标准化检查工具
# 用途: 检查数据库字段是否符合标准化规范
# 使用: ./scripts/field_standards_checker.sh [command]

set -e

DB_HOST="localhost"
DB_PORT="55432"
DB_NAME="synapse_test"
DB_USER="synapse"

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1"
}

check_timestamp_fields() {
    log "检查时间戳字段命名规范..."
    
    docker exec synapse-postgres psql -U $DB_USER -d $DB_NAME -c "
        SELECT table_name, column_name 
        FROM information_schema.columns 
        WHERE table_schema = 'public' 
        AND column_name LIKE '%ts'
        AND column_name NOT IN ('created_ts', 'updated_ts', 'expires_ts', 'last_used_ts', 'started_ts', 'ended_ts', 'sent_ts', 'received_ts', 'processed_ts', 'joined_ts', 'left_ts', 'banned_ts', 'invited_ts', 'kicked_ts', 'modified_ts', 'inserted_ts', 'stream_ordering', 'topological_ordering', 'instance_id', 'event_id', 'room_id', 'user_id', 'device_id', 'session_id', 'txn_id', 'message_id', 'request_id', 'transaction_id', 'origin_server_ts')
        ORDER BY table_name, column_name;
    " 2>/dev/null | head -30
}

check_boolean_fields() {
    log "检查布尔字段命名规范..."
    
    docker exec synapse-postgres psql -U $DB_USER -d $DB_NAME -c "
        SELECT table_name, column_name 
        FROM information_schema.columns 
        WHERE table_schema = 'public' 
        AND data_type = 'boolean'
        AND column_name NOT LIKE 'is_%'
        AND column_name NOT LIKE 'has_%'
        AND column_name NOT IN ('active', 'admin', 'valid', 'verified', 'enabled', 'deleted', 'hidden', 'locked', 'public', 'federate', 'encrypted', 'direct', 'guest_access', 'history_visibility', 'join_rule')
        ORDER BY table_name, column_name;
    " 2>/dev/null | head -30
}

check_id_field_types() {
    log "检查ID字段数据类型..."
    
    docker exec synapse-postgres psql -U $DB_USER -d $DB_NAME -c "
        SELECT table_name, column_name, data_type, character_maximum_length
        FROM information_schema.columns 
        WHERE table_schema = 'public' 
        AND column_name LIKE '%_id'
        AND column_name != 'id'
        AND (data_type != 'character varying' OR character_maximum_length != 255)
        ORDER BY table_name, column_name;
    " 2>/dev/null | head -30
}

check_missing_constraints() {
    log "检查缺失的约束..."
    
    echo ""
    echo "缺少主键的表:"
    docker exec synapse-postgres psql -U $DB_USER -d $DB_NAME -c "
        SELECT t.table_name
        FROM information_schema.tables t
        LEFT JOIN information_schema.table_constraints tc 
            ON t.table_name = tc.table_name 
            AND tc.constraint_type = 'PRIMARY KEY'
            AND t.table_schema = tc.table_schema
        WHERE t.table_schema = 'public'
            AND tc.table_name IS NULL
            AND t.table_type = 'BASE TABLE'
        ORDER BY t.table_name;
    " 2>/dev/null | head -20
}

check_field_consistency() {
    log "检查字段一致性..."
    
    echo ""
    echo "检查 created_ts 字段:"
    docker exec synapse-postgres psql -U $DB_USER -d $DB_NAME -c "
        SELECT COUNT(*) as tables_with_created_ts
        FROM information_schema.columns 
        WHERE table_schema = 'public' 
        AND column_name = 'created_ts';
    " 2>/dev/null
    
    echo ""
    echo "检查 creation_ts 字段 (应为0):"
    docker exec synapse-postgres psql -U $DB_USER -d $DB_NAME -c "
        SELECT COUNT(*) as tables_with_creation_ts
        FROM information_schema.columns 
        WHERE table_schema = 'public' 
        AND column_name = 'creation_ts';
    " 2>/dev/null
}

generate_report() {
    log "生成字段标准化报告..."
    
    REPORT_FILE="/home/tzd/api-test/field_standards_report_$(date +%Y%m%d).md"
    
    cat > "$REPORT_FILE" << EOF
# 数据库字段标准化检查报告

**生成时间**: $(date '+%Y-%m-%d %H:%M:%S')

## 检查结果摘要

### 1. 时间戳字段检查
EOF
    
    docker exec synapse-postgres psql -U $DB_USER -d $DB_NAME -c "
        SELECT table_name, column_name 
        FROM information_schema.columns 
        WHERE table_schema = 'public' 
        AND column_name LIKE '%ts'
        ORDER BY table_name, column_name;
    " >> "$REPORT_FILE" 2>/dev/null
    
    echo "" >> "$REPORT_FILE"
    echo "### 2. 布尔字段检查" >> "$REPORT_FILE"
    
    docker exec synapse-postgres psql -U $DB_USER -d $DB_NAME -c "
        SELECT table_name, column_name 
        FROM information_schema.columns 
        WHERE table_schema = 'public' 
        AND data_type = 'boolean'
        ORDER BY table_name, column_name;
    " >> "$REPORT_FILE" 2>/dev/null
    
    log "报告已生成: $REPORT_FILE"
}

main() {
    log "========================================"
    log "字段标准化检查工具"
    log "========================================"
    echo ""
    
    case "${1:-check}" in
        timestamp)
            check_timestamp_fields
            ;;
        boolean)
            check_boolean_fields
            ;;
        id)
            check_id_field_types
            ;;
        constraints)
            check_missing_constraints
            ;;
        consistency)
            check_field_consistency
            ;;
        report)
            generate_report
            ;;
        check|*)
            check_field_consistency
            echo ""
            check_timestamp_fields
            echo ""
            check_boolean_fields
            echo ""
            check_id_field_types
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
        echo "  check       执行所有检查 (默认)"
        echo "  timestamp   检查时间戳字段"
        echo "  boolean     检查布尔字段"
        echo "  id          检查ID字段类型"
        echo "  constraints 检查缺失约束"
        echo "  consistency 检查字段一致性"
        echo "  report      生成详细报告"
        echo "  --help      显示帮助信息"
        ;;
    *)
        main "$@"
        ;;
esac
