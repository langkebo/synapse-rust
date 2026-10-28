#!/usr/bin/env bash
#
# 断言没有残留的 `test_*` 数据库（CI 隔离单元是整库，"漏一个"就是 20 MB + 一行 pg_database）。
#
# ── 为什么不能只用一行 psql（2026-10-28）───────────────────────────────────
#
# 旧实现：
#     left="$(psql -tAc "SELECT count(*) FROM pg_database WHERE datname LIKE 'test\_%'")"
#     if [ "${left}" -ne 0 ]; then ...
#
# 它把三件不同的事混成一条判据，于是最需要区分的那种情况反而最不可读：
#   ① 真的残留了库；
#   ② 服务器**连不上**（`out of shared memory`、`too many clients`、刚被 OOM 杀掉）；
#   ③ 查询本身报错（权限、库被 drop 到一半）。
# ②③ 时 `left` 是空串，`[ "" -ne 0 ]` 打印 `integer expression expected` ——
# 一条**语法错误**成了这个步骤的全部诊断信息。读日志的人只会看到"残留库断言红了"，
# 而真实主因在上一屏：测试把 shared_buffers 打爆了。次生噪音掩盖主因。
#
# 现在的判据分两层：
#   1. **可达性探测**（带重试）：`out of shared memory` 多半是瞬时的（测试进程刚退出、
#      临时连接还没回收）。立刻断言会把"正在恢复"判成"有残留"。
#   2. **残留计数**：只在服务器确实应答之后才做，且严格校验返回值是整数。
#
# 三层结论各自有明确的错误标题，不再互相伪装：
#   * `Leftover test databases`          → 真的有残留（列出库名）；
#   * `Cannot determine leftover test databases` → 服务器不可达/查询异常，结论 UNKNOWN；
#   * 无输出                              → 干净。
#
# 用法：`bash scripts/ci/assert_no_leftover_test_databases.sh`
# 连接参数走 libpq 标准环境变量：PGHOST / PGPORT / PGUSER / PGPASSWORD。
# 调参：LEFTOVER_DB_PROBE_ATTEMPTS（默认 6）、LEFTOVER_DB_PROBE_DELAY_SECONDS（默认 5）。
set -uo pipefail

attempts="${LEFTOVER_DB_PROBE_ATTEMPTS:-6}"
delay="${LEFTOVER_DB_PROBE_DELAY_SECONDS:-5}"
host="${PGHOST:-localhost}"
port="${PGPORT:-5432}"
user="${PGUSER:-synapse}"

psql_q() {
    psql -h "$host" -p "$port" -U "$user" -d postgres -tAc "$1" 2>&1
}

echo "probing postgres at ${host}:${port} as ${user} (up to ${attempts} attempt(s))"

# ── 1. 可达性 ────────────────────────────────────────────────────────────────
ready=0
last_err=""
i=1
while [ "$i" -le "$attempts" ]; do
    if out="$(psql_q 'SELECT 1')" && [ "$(printf '%s' "$out" | tr -d '[:space:]')" = "1" ]; then
        ready=1
        break
    fi
    last_err="$out"
    if [ "$i" -lt "$attempts" ]; then
        echo "  attempt ${i}/${attempts}: server not ready (${last_err%%$'\n'*}); retrying in ${delay}s"
        sleep "$delay"
    fi
    i=$((i + 1))
done

if [ "$ready" -ne 1 ]; then
    echo "::error title=Cannot determine leftover test databases::postgres ${host}:${port} did not answer 'SELECT 1' after ${attempts} attempt(s). This run's conclusion about leftover test_* databases is UNKNOWN — it is NOT 'leftover' and NOT 'clean'."
    echo "  last error: ${last_err}" >&2
    echo "  A concurrent 'out of shared memory' / 'too many clients' on the test step is the usual cause." >&2
    echo "  Fix that first; do not treat this line as evidence that the janitor leaked a database." >&2
    exit 1
fi

# ── 2. 残留计数（服务器已应答）──────────────────────────────────────────────
if ! left="$(psql_q "SELECT count(*) FROM pg_database WHERE datname LIKE 'test\\_%'")"; then
    echo "::error title=Cannot determine leftover test databases::the leftover query itself failed although 'SELECT 1' answered. Conclusion UNKNOWN."
    echo "  query error: ${left}" >&2
    exit 1
fi
left="$(printf '%s' "$left" | tr -d '[:space:]')"

case "$left" in
    '' | *[!0-9]*)
        echo "::error title=Cannot determine leftover test databases::the leftover query returned a non-numeric value [${left}]. Conclusion UNKNOWN."
        exit 1
        ;;
esac

if [ "$left" -ne 0 ]; then
    echo "::error title=Leftover test databases::${left} leftover test database(s) — the janitor did not reclaim them."
    if names="$(psql_q "SELECT datname FROM pg_database WHERE datname LIKE 'test\\_%' ORDER BY datname")"; then
        printf '%s\n' "$names" >&2
    else
        echo "  (could not list names: ${names})" >&2
    fi
    exit 1
fi

echo "no leftover test databases"
exit 0
