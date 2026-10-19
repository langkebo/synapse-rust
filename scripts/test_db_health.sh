#!/usr/bin/env bash
# 测试库健康检查：catalog 体积 / 残留隔离 schema / 单测耗时 P95。
#
# ── 为什么需要它 ────────────────────────────────────────────────────────────
# 隔离 schema 每次 clone 要写 ~1000 条 catalog。残留 schema 一多，每条 DDL 都要
# 从磁盘重读 `pg_attribute`：2026-10-19 实测同一个测试在 10 GB / 765 个残留
# schema 的库上 95 s，在重建后的 37 MB 库上 11 s（约 17 倍）。这不是"慢一点"，
# 而是整轮测试从可用变成不可用，且会正反馈（SIGKILL → 更多泄漏 → 更慢）。
#
# 所以把三个量作为运行前后的健康指标：`pg_attribute` 体积、残留 schema 数、
# 单测 P95。跑全量前后各执行一次，差值就是这一轮的健康状况。
#
# ── 用法 ────────────────────────────────────────────────────────────────────
#   bash scripts/test_db_health.sh
#   DATABASE_URL=postgres://... bash scripts/test_db_health.sh
#   bash scripts/test_db_health.sh --residual-limit 20
#
# 连接优先级：DATABASE_URL > TEST_DATABASE_URL。退出码：0 正常，1 残留超阈值，
# 2 未配置连接串。
set -euo pipefail

RESIDUAL_LIMIT=50
JUNIT_PATH="artifacts/nextest-junit.xml"

while [ $# -gt 0 ]; do
    case "${1}" in
        --residual-limit)
            RESIDUAL_LIMIT="${2:?--residual-limit 需要参数}"
            shift 2
            ;;
        --junit)
            JUNIT_PATH="${2:?--junit 需要参数}"
            shift 2
            ;;
        *)
            echo "未知参数：${1}" >&2
            exit 2
            ;;
    esac
done

DATABASE_URL="${DATABASE_URL:-${TEST_DATABASE_URL:-}}"
if [ -z "${DATABASE_URL}" ]; then
    echo "未提供连接串：设置 DATABASE_URL 或 TEST_DATABASE_URL（本地测试库是 127.0.0.1:55432/synapse_test）。" >&2
    exit 2
fi
if ! command -v psql >/dev/null 2>&1; then
    echo "未找到 psql：把它放进 PATH（本机是 /opt/homebrew/opt/postgresql@15/bin）。" >&2
    exit 2
fi

q() {
    psql "${DATABASE_URL}" -tAX -v ON_ERROR_STOP=1 -c "${1}"
}

db_name="$(q "SELECT current_database()")"
db_size="$(q "SELECT pg_size_pretty(pg_database_size(current_database()))")"
attr_size="$(q "SELECT pg_size_pretty(pg_relation_size('pg_attribute'))")"
class_rows="$(q "SELECT count(*) FROM pg_class")"
stamped="$(q "SELECT count(*) FROM pg_namespace WHERE nspname ~ '^test_[0-9]+_[0-9]+_[0-9]+$'")"
uuid_family="$(q "SELECT count(*) FROM pg_namespace WHERE nspname ~ '^test_[0-9a-f]{32}$'")"
templates="$(q "SELECT count(*) FROM pg_namespace WHERE nspname ~ '^test_(template_v[0-9]+_|isolation_template_)'")"

# 最老的 stamped 残留：名字末段是创建时刻的纳秒。
oldest_line="$(q "SELECT nspname FROM pg_namespace WHERE nspname ~ '^test_[0-9]+_[0-9]+_[0-9]+$' ORDER BY nspname LIMIT 1")"
oldest_age="n/a"
if [ -n "${oldest_line}" ]; then
    oldest_age="$(
        python3 - "${oldest_line}" <<'PY'
import sys
import time

name = sys.argv[1]
stamp = int(name.rsplit('_', 1)[1])
age_s = max(0, (time.time_ns() - stamp) // 1_000_000_000)
hours, rem = divmod(age_s, 3600)
minutes, seconds = divmod(rem, 60)
print(f"{hours}h{minutes:02d}m{seconds:02d}s")
PY
    )"
fi

echo "测试库健康指标"
echo "  数据库              ${db_name}"
echo "  库体大小            ${db_size}"
echo "  pg_attribute        ${attr_size}   （>256 MB 起 clone 明显变慢）"
echo "  pg_class 行数       ${class_rows}"
echo "  残留 stamped schema ${stamped}   （最老：${oldest_age}）"
echo "  残留 uuid schema    ${uuid_family}   （无时间戳，守卫不收，需脚本清理）"
echo "  共享模板            ${templates}"

p95="n/a"
if [ -f "${JUNIT_PATH}" ]; then
    p95="$(
        python3 - "${JUNIT_PATH}" <<'PY'
import sys
import xml.etree.ElementTree as ET

try:
    root = ET.parse(sys.argv[1]).getroot()
except ET.ParseError:
    print('junit 解析失败')
    raise SystemExit(0)

durations = sorted(float(c.get('time', 0)) for c in root.iter('testcase'))
if not durations:
    print('无 testcase')
    raise SystemExit(0)
index = min(len(durations) - 1, int(len(durations) * 0.95))
print(f"{durations[index]:.2f}s over {len(durations)} cases")
PY
    )"
else
    p95="n/a（${JUNIT_PATH} 不存在；先跑 --profile ci 生成）"
fi
echo "  单测 P95            ${p95}"

residual_total=$((stamped + uuid_family))
if [ "${residual_total}" -gt "${RESIDUAL_LIMIT}" ]; then
    echo ""
    echo "WARN: 残留 ${residual_total} 个，超过阈值 ${RESIDUAL_LIMIT}。" >&2
    echo "      bash scripts/cleanup_test_schemas.sh --apply" >&2
    exit 1
fi
echo ""
echo "OK: 残留 ${residual_total} 个，阈值 ${RESIDUAL_LIMIT}。"
