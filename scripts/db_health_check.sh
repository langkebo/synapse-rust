#!/usr/bin/env bash
# 数据库健康检查脚本
# 用法: bash scripts/db_health_check.sh

set -e

DB_NAME="${DB_NAME:-synapse}"
DB_USER="${DB_USER:-synapse}"
DB_HOST="${DB_HOST:-localhost}"

echo "=========================================="
echo "  数据库健康检查"
echo "=========================================="
echo ""

# 颜色定义
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

# 检查函数
check() {
    local name="$1"
    local result="$2"
    if [ "$result" = "0" ]; then
        echo -e "${GREEN}✓${NC} $name"
    else
        echo -e "${RED}✗${NC} $name ($result)"
    fi
}

# 1. 表数量检查
echo "--- 表结构 ---"
TABLE_COUNT=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_type = 'BASE TABLE';" 2>/dev/null | xargs)
echo "表总数: $TABLE_COUNT"

# 2. 外键检查
echo ""
echo "--- 外键约束 ---"
FK_COUNT=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "SELECT COUNT(*) FROM information_schema.table_constraints WHERE constraint_type = 'FOREIGN KEY' AND table_schema = 'public';" 2>/dev/null | xargs)
echo "外键总数: $FK_COUNT"

# 预期外键数量 (基于表数量估算)
EXPECTED_FK=$((TABLE_COUNT * 2 / 3))
if [ "$FK_COUNT" -ge "$EXPECTED_FK" ]; then
    check "外键覆盖率" 0
else
    check "外键覆盖率 (需要 $EXPECTED_FK+)" 1
fi

# 3. 索引检查
echo ""
echo "--- 索引 ---"
INDEX_COUNT=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "SELECT COUNT(*) FROM pg_indexes WHERE schemaname = 'public';" 2>/dev/null | xargs)
echo "索引总数: $INDEX_COUNT"

# 4. 字段命名检查
echo ""
echo "--- 字段规范检查 ---"

# 检查禁止字段
BAD_FIELDS=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
SELECT COUNT(*) FROM information_schema.columns 
WHERE table_schema = 'public' 
AND column_name IN ('created_at', 'updated_at', 'expires_ts', 'revoked_ts', 'invalidated', 'enabled');
" 2>/dev/null | xargs)

if [ "$BAD_FIELDS" = "0" ]; then
    check "字段命名规范" 0
else
    echo -e "${YELLOW}!${NC} 发现 $BAD_FIELDS 个非标准字段名"
    
    # 列出具体字段
    psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -c "
    SELECT table_name, column_name 
    FROM information_schema.columns 
    WHERE table_schema = 'public' 
    AND column_name IN ('created_at', 'updated_at', 'expires_ts', 'revoked_ts', 'invalidated', 'enabled')
    LIMIT 10;
    " 2>/dev/null
fi

# 5. 孤立数据检查
echo ""
echo "--- 数据完整性 ---"

ORPHAN_DEVICES=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
SELECT COUNT(*) FROM devices d 
WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = d.user_id);
" 2>/dev/null | xargs)

if [ "$ORPHAN_DEVICES" = "0" ]; then
    check "孤立设备" 0
else
    echo -e "${RED}!${NC} 发现 $ORPHAN_DEVICES 条孤立设备记录"
fi

# 6. 用户和房间统计
echo ""
echo "--- 业务数据 ---"

USER_COUNT=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "SELECT COUNT(*) FROM users;" 2>/dev/null | xargs)
ROOM_COUNT=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "SELECT COUNT(*) FROM rooms;" 2>/dev/null | xargs)
DEVICE_COUNT=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "SELECT COUNT(*) FROM devices;" 2>/dev/null | xargs)

echo "用户数: $USER_COUNT"
echo "房间数: $ROOM_COUNT"
echo "设备数: $DEVICE_COUNT"

# 7. 性能指标
echo ""
echo "--- 性能指标 ---"

# 慢查询检查 (如果有 pg_stat_statements)
SLOW_QUERIES=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
SELECT COUNT(*) FROM pg_stat_statements 
WHERE mean_time > 1000;
" 2>/dev/null | xargs || echo "0")

if [ "$SLOW_QUERIES" = "0" ] || [ -z "$SLOW_QUERIES" ]; then
    check "慢查询 (无)" 0
else
    echo -e "${YELLOW}!${NC} 发现 $SLOW_QUERIES 条慢查询"
fi

echo ""
echo "=========================================="
echo "  检查完成"
echo "=========================================="
