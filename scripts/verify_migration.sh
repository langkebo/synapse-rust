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
        "SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_name = '$table');" 2>/dev/null)
    
    if [[ "$result" == *"t"* ]]; then
        echo "✅ 表 $table 存在"
        return 0
    else
        echo "❌ 表 $table 不存在"
        return 1
    fi
}

verify_index_exists() {
    local index=$1
    local result=$(psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -t -c \
        "SELECT EXISTS (SELECT FROM pg_indexes WHERE indexname = '$index');" 2>/dev/null)
    
    if [[ "$result" == *"t"* ]]; then
        echo "✅ 索引 $index 存在"
        return 0
    else
        echo "❌ 索引 $index 不存在"
        return 1
    fi
}

verify_field_naming() {
    local table=$1
    local field=$2
    
    local result=$(psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -t -c \
        "SELECT column_name FROM information_schema.columns WHERE table_name = '$table' AND column_name = '$field';" 2>/dev/null)
    
    if [[ -n "$result" ]]; then
        echo "✅ 字段 $table.$field 存在"
        return 0
    else
        echo "❌ 字段 $table.$field 不存在"
        return 1
    fi
}

# 验证核心表
echo "=========================================="
echo "验证核心表..."
echo "=========================================="

errors=0

# P1 高优先级表
errors=$((verify_table_exists "presence_subscriptions" || errors))
errors=$((verify_table_exists "call_sessions" || errors))
errors=$((verify_table_exists "call_candidates" || errors))
errors=$((verify_table_exists "qr_login_transactions" || errors))
errors=$((verify_table_exists "room_invite_blocklist" || errors))
errors=$((verify_table_exists "room_invite_allowlist" || errors))
errors=$((verify_table_exists "room_sticky_events" || errors))

# P2 中优先级表
errors=$((verify_table_exists "background_update_history" || errors))
errors=$((verify_table_exists "background_update_locks" || errors))
errors=$((verify_table_exists "background_update_stats" || errors))
errors=$((verify_table_exists "federation_blacklist_config" || errors))
errors=$((verify_table_exists "federation_blacklist_rule" || errors))
errors=$((verify_table_exists "federation_blacklist_log" || errors))
errors=$((verify_table_exists "retention_cleanup_logs" || errors))
errors=$((verify_table_exists "retention_cleanup_queue" || errors))
errors=$((verify_table_exists "notification_templates" || errors))
errors=$((verify_table_exists "notification_delivery_log" || errors))
errors=$((verify_table_exists "scheduled_notifications" || errors))
errors=$((verify_table_exists "user_notification_status" || errors))
errors=$((verify_table_exists "push_device" || errors))
errors=$((verify_table_exists "registration_token_batches" || errors))
errors=$((verify_table_exists "rendezvous_messages" || errors))

# P3 低优先级表
errors=$((verify_table_exists "beacon_info" || errors))
errors=$((verify_table_exists "beacon_locations" || errors))
errors=$((verify_table_exists "dehydrated_devices" || errors))
errors=$((verify_table_exists "email_verification" || errors))
errors=$((verify_table_exists "federation_stats" || errors))
errors=$((verify_table_exists "performance_metrics" || errors))
errors=$((verify_table_exists "audit_log" || errors))

echo ""
echo "=========================================="
echo "验证索引..."
echo "=========================================="

errors=$((verify_index_exists "idx_presence_subscriptions_subscriber" || errors))
errors=$((verify_index_exists "idx_presence_subscriptions_target" || errors))
errors=$((verify_index_exists "idx_call_sessions_room" || errors))
errors=$((verify_index_exists "idx_call_sessions_caller" || errors))
errors=$((verify_index_exists "idx_qr_login_transactions_user" || errors))
errors=$((verify_index_exists "idx_room_invite_blocklist_room" || errors))
errors=$((verify_index_exists "idx_room_sticky_events_room" || errors))

echo ""
echo "=========================================="
echo "验证字段命名..."
echo "=========================================="

# 验证时间戳字段
errors=$((verify_field_naming "presence_subscriptions" "created_ts" || errors))
errors=$((verify_field_naming "call_sessions" "created_ts" || errors))
errors=$((verify_field_naming "call_sessions" "updated_ts" || errors))
errors=$((verify_field_naming "qr_login_transactions" "created_ts" || errors))
errors=$((verify_field_naming "qr_login_transactions" "updated_ts" || errors))
errors=$((verify_field_naming "qr_login_transactions" "expires_at" || errors))
errors=$((verify_field_naming "room_invite_blocklist" "created_ts" || errors))
errors=$((verify_field_naming "room_sticky_events" "created_ts" || errors))
errors=$((verify_field_naming "room_sticky_events" "updated_ts" || errors))

echo ""
echo "=========================================="
echo "验证结果汇总"
echo "=========================================="

if [ $errors -eq 0 ]; then
    echo "✅ 所有验证通过"
    exit 0
else
    echo "❌ 发现 $errors 个错误"
    exit 1
fi
