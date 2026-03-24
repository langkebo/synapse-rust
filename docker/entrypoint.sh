#!/bin/bash
# =============================================================================
# synapse-rust 启动脚本
# 自动执行数据库迁移
# =============================================================================

set -e

echo "[synapse-rust] 启动脚本开始执行..."

# 数据库连接配置
DB_HOST=${DB_HOST:-db}
DB_PORT=${DB_PORT:-5432}
DB_USER=${DB_USER:-synapse}
DB_PASSWORD=${DB_PASSWORD:-synapse}
DB_NAME=${DB_NAME:-synapse}

# 迁移配置
RUN_MIGRATIONS=${RUN_MIGRATIONS:-true}
MIGRATIONS_DIR="/app/migrations"

# 等待数据库就绪
wait_for_db() {
    echo "[synapse-rust] 等待数据库就绪..."
    local max_attempts=30
    local attempt=1

    while [ $attempt -le $max_attempts ]; do
        if PGPASSWORD="$DB_PASSWORD" psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -c "SELECT 1" > /dev/null 2>&1; then
            echo "[synapse-rust] 数据库已就绪"
            return 0
        fi
        echo "[synapse-rust] 等待数据库... ($attempt/$max_attempts)"
        sleep 2
        attempt=$((attempt + 1))
    done

    echo "[synapse-rust] 数据库连接失败"
    return 1
}

# 执行迁移
run_migrations() {
    echo "[synapse-rust] 开始执行数据库迁移..."

    if [ ! -d "$MIGRATIONS_DIR" ]; then
        echo "[synapse-rust] 迁移目录不存在: $MIGRATIONS_DIR"
        return 1
    fi

    # 按文件名排序执行迁移
    for migration_file in $(ls -1 "$MIGRATIONS_DIR"/*.sql 2>/dev/null | sort); do
        local filename=$(basename "$migration_file")
        echo "[synapse-rust] 执行迁移: $filename"

        # 检查是否已执行
        local version=$(echo "$filename" | sed 's/_.*//')
        local already_run=$(PGPASSWORD="$DB_PASSWORD" psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -t -c "SELECT COUNT(*) FROM schema_migrations WHERE version = '$version';" 2>/dev/null | tr -d ' ')

        if [ "$already_run" -gt 0 ]; then
            echo "[synapse-rust] 迁移 $filename 已执行，跳过"
            continue
        fi

        # 执行迁移
        if PGPASSWORD="$DB_PASSWORD" psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -f "$migration_file" > /tmp/migration_output.log 2>&1; then
            echo "[synapse-rust] 迁移 $filename 执行成功"
        else
            local error=$(cat /tmp/migration_output.log | tail -5)
            echo "[synapse-rust] 迁移 $filename 执行失败: $error"

            # 如果是索引已存在错误，尝试继续
            if grep -q "already exists" /tmp/migration_output.log; then
                echo "[synapse-rust] 索引已存在，继续执行"
                continue
            fi

            # 其他错误，退出
            return 1
        fi
    done

    echo "[synapse-rust] 所有迁移执行完成"
    return 0
}

# 显示数据库状态
show_db_status() {
    echo "[synapse-rust] 数据库状态:"
    PGPASSWORD="$DB_PASSWORD" psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -c "SELECT COUNT(*) as table_count FROM information_schema.tables WHERE table_schema = 'public' AND table_type = 'BASE TABLE';" 2>/dev/null
    PGPASSWORD="$DB_PASSWORD" psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -c "SELECT version, success, description FROM schema_migrations ORDER BY applied_ts;" 2>/dev/null
}

# 主流程
main() {
    # 等待数据库
    if ! wait_for_db; then
        echo "[synapse-rust] 无法连接到数据库"
        exit 1
    fi

    # 执行迁移
    if [ "$RUN_MIGRATIONS" = "true" ]; then
        if ! run_migrations; then
            echo "[synapse-rust] 迁移执行失败"
            exit 1
        fi
    else
        echo "[synapse-rust] 迁移已禁用 (RUN_MIGRATIONS=false)"
    fi

    # 显示状态
    show_db_status

    # 启动应用
    echo "[synapse-rust] 启动 synapse-rust..."
    exec /app/synapse-rust "$@"
}

main "$@"
