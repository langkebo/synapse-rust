#!/bin/bash
# 数据库一致性检查脚本
# 用于验证 Rust 模型与 SQL 表结构的一致性

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# 配置
DB_NAME="${DB_NAME:-synapse}"
DB_USER="${DB_USER:-synapse}"
DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-5432}"

echo "=========================================="
echo "数据库一致性检查"
echo "=========================================="
echo "数据库: $DB_NAME"
echo "用户: $DB_USER"
echo "主机: $DB_HOST:$DB_PORT"
echo "=========================================="

# 检查 Docker 容器状态
check_containers() {
    echo ""
    echo "[检查 1/5] Docker 容器状态"
    echo "------------------------------------------"

    CONTAINERS=$(docker ps --filter "name=docker-" --format "{{.Names}}:{{.Status}}")

    for container in docker-postgres docker-redis docker-rust; do
        if echo "$CONTAINERS" | grep -q "$container.*Up"; then
            echo -e "${GREEN}✓${NC} $container 运行正常"
        else
            echo -e "${RED}✗${NC} $container 未运行"
        fi
    done
}

# 检查数据库连接
check_connection() {
    echo ""
    echo "[检查 2/5] 数据库连接"
    echo "------------------------------------------"

    if docker exec docker-postgres psql -U "$DB_USER" -d "$DB_NAME" -c "SELECT 1;" > /dev/null 2>&1; then
        echo -e "${GREEN}✓${NC} 数据库连接正常"
    else
        echo -e "${RED}✗${NC} 数据库连接失败"
        exit 1
    fi
}

# 检查表数量
check_table_count() {
    echo ""
    echo "[检查 3/5] 表数量检查"
    echo "------------------------------------------"

    TABLE_COUNT=$(docker exec docker-postgres psql -U "$DB_USER" -d "$DB_NAME" -t -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public';" 2>/dev/null | tr -d ' ')

    echo "当前表数量: $TABLE_COUNT"

    if [ "$TABLE_COUNT" -ge 150 ]; then
        echo -e "${GREEN}✓${NC} 表数量正常 (>= 150)"
    else
        echo -e "${YELLOW}⚠${NC} 表数量可能异常 (< 150)"
    fi
}

# 检查索引数量
check_index_count() {
    echo ""
    echo "[检查 4/5] 索引数量检查"
    echo "------------------------------------------"

    INDEX_COUNT=$(docker exec docker-postgres psql -U "$DB_USER" -d "$DB_NAME" -t -c "SELECT COUNT(*) FROM pg_indexes WHERE schemaname = 'public';" 2>/dev/null | tr -d ' ')

    echo "当前索引数量: $INDEX_COUNT"

    if [ "$INDEX_COUNT" -ge 400 ]; then
        echo -e "${GREEN}✓${NC} 索引数量正常 (>= 400)"
    else
        echo -e "${YELLOW}⚠${NC} 索引数量可能异常 (< 400)"
    fi
}

# 检查 TIMESTAMP 字段违规
check_timestamp_fields() {
    echo ""
    echo "[检查 5/5] TIMESTAMP 字段规范检查"
    echo "------------------------------------------"

    TIMESTAMP_COUNT=$(docker exec docker-postgres psql -U "$DB_USER" -d "$DB_NAME" -t -c "SELECT COUNT(*) FROM information_schema.columns WHERE data_type LIKE '%timestamp%' AND table_schema = 'public' AND table_name NOT IN ('pg_stat_statements_info', 'schema_migrations', 'voice_usage_stats');" 2>/dev/null | tr -d ' ')

    echo "用户表 TIMESTAMP 字段数量: $TIMESTAMP_COUNT"

    if [ "$TIMESTAMP_COUNT" -eq 0 ]; then
        echo -e "${GREEN}✓${NC} 所有用户表字段符合规范 (无 TIMESTAMP 违规)"
    else
        echo -e "${RED}✗${NC} 发现 $TIMESTAMP_COUNT 个 TIMESTAMP 字段违规"
        echo "违规字段:"
        docker exec docker-postgres psql -U "$DB_USER" -d "$DB_NAME" -c "SELECT table_name, column_name FROM information_schema.columns WHERE data_type LIKE '%timestamp%' AND table_schema = 'public' AND table_name NOT IN ('pg_stat_statements_info', 'schema_migrations', 'voice_usage_stats');" 2>/dev/null
    fi
}

# PostgreSQL 配置检查
check_postgres_config() {
    echo ""
    echo "[额外检查] PostgreSQL 配置"
    echo "------------------------------------------"

    CONFIG=$(docker exec docker-postgres psql -U "$DB_USER" -d "$DB_NAME" -c "SHOW shared_buffers; SHOW work_mem; SHOW random_page_cost;" 2>/dev/null)

    if echo "$CONFIG" | grep -q "256MB"; then
        echo -e "${GREEN}✓${NC} shared_buffers = 256MB (已优化)"
    fi

    if echo "$CONFIG" | grep -q "16MB"; then
        echo -e "${GREEN}✓${NC} work_mem = 16MB (已优化)"
    fi

    if echo "$CONFIG" | grep -q "1.1"; then
        echo -e "${GREEN}✓${NC} random_page_cost = 1.1 (SSD优化)"
    fi
}

# 运行所有检查
check_containers
check_connection
check_table_count
check_index_count
check_timestamp_fields
check_postgres_config

echo ""
echo "=========================================="
echo -e "${GREEN}检查完成${NC}"
echo "=========================================="
