#!/usr/bin/env bash
# =============================================================================
# 创建 postgres_exporter 用的**只读**角色（幂等；可重复执行）
# =============================================================================
# 为什么单独一个脚本而不是折进 migrations/ 基线：
#   角色口令是**密钥**，不能进 git。基线是"仓库内的唯一真相源"，但密钥必须来自
#   环境（`docker/deploy/.env`，已 gitignore）。因此角色/授权用本脚本幂等地施加。
#
# 最小权限口径（不要图省事直接给 POSTGRES_USER —— 那等于把 owner 账号塞给 exporter）：
#   - LOGIN：exporter 需要建立连接
#   - pg_monitor：PG10+ 内置角色，提供 pg_stat_activity / pg_stat_replication /
#     pg_stat_archiver 等只读视图（内含 pg_read_all_stats，故能读到 query 文本）
#   - CONNECT：仅连目标库
#   **不**给：superuser / CREATEDB / CREATEROLE / 任何表的写权限 / 全库 SELECT
#
# 用法（在 docker/deploy 下，先确认 .env 里有 MONITOR_DB_PASSWORD）：
#    MONITOR_DB_PASSWORD=$(openssl rand -hex 24)
#    echo "MONITOR_DB_PASSWORD=$MONITOR_DB_PASSWORD" >> .env
#    bash scripts/create-monitoring-role.sh
#
# 验收：脚本末尾会**用该角色走 TCP+口令**连一次库并读 pg_stat_activity（与 exporter 同路径）。
# =============================================================================
set -euo pipefail

DB_CONTAINER="${DB_CONTAINER:-synapse-postgres}"
DB_HOST="${MONITOR_DB_HOST:-127.0.0.1}"
DB_PORT="${MONITOR_DB_PORT:-5432}"
DB_NAME="${POSTGRES_DB:-synapse}"
DB_ADMIN_USER="${POSTGRES_USER:-postgres}"
ROLE="${MONITOR_DB_ROLE:-synapse_monitor}"
PW="${MONITOR_DB_PASSWORD:?需要 MONITOR_DB_PASSWORD（写进 docker/deploy/.env，勿提交）}"

admin_psql() {
    docker exec -i -u postgres "$DB_CONTAINER" psql -U "$DB_ADMIN_USER" -d "$DB_NAME" -v ON_ERROR_STOP=1 "$@"
}

echo "== 目标：${DB_CONTAINER} / ${DB_NAME} / 角色 ${ROLE} =="

if ! docker ps --format '{{.Names}}' | grep -qx "$DB_CONTAINER"; then
    echo "❌ 容器 $DB_CONTAINER 未运行" >&2
    exit 1
fi

exists="$(admin_psql -tAc "SELECT 1 FROM pg_roles WHERE rolname = '${ROLE}'" | tr -d '[:space:]')"
if [ "${exists}" = "1" ]; then
    echo "ℹ️  角色已存在，仅同步口令与授权"
else
    admin_psql -c "CREATE ROLE \"${ROLE}\" LOGIN" >/dev/null
    echo "✅ 已创建角色 ${ROLE}"
fi

# 口令用 hex 生成（无引号/转义问题）；ALTER 幂等
admin_psql -c "ALTER ROLE \"${ROLE}\" WITH LOGIN PASSWORD '${PW}'" >/dev/null
admin_psql -c "GRANT pg_monitor TO \"${ROLE}\"" >/dev/null
admin_psql -c "GRANT CONNECT ON DATABASE \"${DB_NAME}\" TO \"${ROLE}\"" >/dev/null
echo "✅ 口令已同步；pg_monitor + CONNECT 已授权"

# pg_stat_statements 是扩展视图（不在 pg_monitor 覆盖范围内），存在则显式授权
if [ "$(admin_psql -tAc "SELECT 1 FROM pg_extension WHERE extname='pg_stat_statements'" | tr -d '[:space:]')" = "1" ]; then
    admin_psql -c "GRANT SELECT ON pg_stat_statements TO \"${ROLE}\"" >/dev/null
    echo "✅ 已授予 pg_stat_statements 只读"
fi

echo "== 验收：用该角色走 TCP + 口令（与 exporter 完全相同路径）=="
docker exec -e PGPASSWORD="$PW" "$DB_CONTAINER" \
    psql -h "$DB_HOST" -p "$DB_PORT" -U "$ROLE" -d "$DB_NAME" -tAc \
    "SELECT 'role=' || current_user || ' can_read_stats=' ||
            (SELECT count(*) > 0 FROM pg_stat_activity)::text ||
            ' archiver_ok=' || (SELECT count(*) >= 0 FROM pg_stat_archiver)::text;" |
    sed 's/^/  /'

echo "== 负向验收：该角色不得能写（下面这条**必须失败**）=="
if docker exec -e PGPASSWORD="$PW" "$DB_CONTAINER" \
    psql -h "$DB_HOST" -p "$DB_PORT" -U "$ROLE" -d "$DB_NAME" -c \
    "CREATE TABLE monitor_should_not_write(x int);" >/dev/null 2>&1; then
    echo "❌ 只读约束被破坏：该角色居然建表成功" >&2
    exit 1
else
    echo "  OK  写操作被拒（符合最小权限）"
fi
