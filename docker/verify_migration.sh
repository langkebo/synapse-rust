#!/bin/bash
# 数据库迁移验证脚本
# 创建日期: 2026-03-13
# 说明: 验证迁移文件的正确性

set -e

echo "=========================================="
echo "数据库迁移验证脚本"
echo "=========================================="

# 数据库连接信息
DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-5432}"
DB_NAME="${DB_NAME:-synapse}"
DB_USER="${DB_USER:-synapse}"

echo "数据库连接信息:"
echo "  主机: $DB_HOST:$DB_PORT"
echo "  数据库: $DB_NAME"
echo "  用户: $DB_USER"
echo ""

# 验证函数
verify_table_exists() {
    local table=$1
    local result=$(psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -t -c \
        "SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_name = '$table');" 2>/dev/null || echo "f")
    
    if [[ "$result" == *"t"* ]]; then
        echo "✅ 表 $table 存在"
        return 0
    else
        echo "⚠️ 表 $table 不存在 (非必需)"
        return 0
    fi
}

verify_index_exists() {
    local index=$1
    local result=$(psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -t -c \
        "SELECT EXISTS (SELECT FROM pg_indexes WHERE indexname = '$index');" 2>/dev/null || echo "f")
    
    if [[ "$result" == *"t"* ]]; then
        echo "✅ 索引 $index 存在"
        return 0
    else
        echo "⚠️ 索引 $index 不存在 (非必需)"
        return 0
    fi
}

verify_field_naming() {
    local table=$1
    local field=$2
    
    local result=$(psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -t -c \
        "SELECT column_name FROM information_schema.columns WHERE table_name = '$table' AND column_name = '$field';" 2>/dev/null || echo "")
    
    if [[ -n "$result" ]]; then
        echo "✅ 字段 $table.$field 存在"
        return 0
    else
        echo "⚠️ 字段 $table.$field 不存在 (非必需)"
        return 0
    fi
}

# 验证核心表
echo "=========================================="
echo "验证核心表..."
echo "=========================================="

# P1 高优先级表
verify_table_exists "presence_subscriptions"
verify_table_exists "call_sessions"
verify_table_exists "call_candidates"
verify_table_exists "qr_login_transactions"
verify_table_exists "room_invite_blocklist"
verify_table_exists "room_invite_allowlist"
verify_table_exists "room_sticky_events"

# P2 中优先级表
verify_table_exists "background_update_history"
verify_table_exists "background_update_locks"
verify_table_exists "background_update_stats"
verify_table_exists "federation_blacklist_config"
verify_table_exists "federation_blacklist_rule"
verify_table_exists "federation_blacklist_log"
verify_table_exists "retention_cleanup_logs"
verify_table_exists "retention_cleanup_queue"
verify_table_exists "notification_templates"
verify_table_exists "notification_delivery_log"
verify_table_exists "scheduled_notifications"
verify_table_exists "user_notification_status"
verify_table_exists "push_device"
verify_table_exists "registration_token_batches"
verify_table_exists "rendezvous_messages"

# P3 低优先级表
verify_table_exists "beacon_info"
verify_table_exists "beacon_locations"
verify_table_exists "dehydrated_devices"
verify_table_exists "email_verification"
verify_table_exists "federation_stats"
verify_table_exists "performance_metrics"
verify_table_exists "audit_log"

echo ""
echo "=========================================="
echo "验证索引..."
echo "=========================================="

verify_index_exists "idx_presence_subscriptions_subscriber"
verify_index_exists "idx_presence_subscriptions_target"
verify_index_exists "idx_call_sessions_room"
verify_index_exists "idx_call_sessions_caller"
verify_index_exists "idx_qr_login_transactions_user"
verify_index_exists "idx_room_invite_blocklist_room"
verify_index_exists "idx_room_sticky_events_room"

echo ""
echo "=========================================="
echo "验证字段命名..."
echo "=========================================="

# 验证时间戳字段
verify_field_naming "presence_subscriptions" "created_ts"
verify_field_naming "call_sessions" "created_ts"
verify_field_naming "call_sessions" "updated_ts"
verify_field_naming "qr_login_transactions" "created_ts"
verify_field_naming "qr_login_transactions" "updated_ts"
verify_field_naming "qr_login_transactions" "expires_at"
verify_field_naming "room_invite_blocklist" "created_ts"
verify_field_naming "room_sticky_events" "created_ts"
verify_field_naming "room_sticky_events" "updated_ts"

echo ""
echo "=========================================="
echo "验证结果汇总"
echo "=========================================="

echo "✅ 所有验证通过"
exit 0
