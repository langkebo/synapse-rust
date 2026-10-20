#!/usr/bin/env bash
# 数据库级测试模板：provision / status / drop
#
# 设计依据 docs/audit/TEST_DB_ISOLATION_DESIGN_2026-10-20.md（P0）。
# schema 级 clone 每条测试要做 221 张表的 DDL（1.2–3.5 s）；database 级
# `CREATE DATABASE ... TEMPLATE` 实测 0.29–0.44 s，且不需要任何"重建后修正"。
#
# 本脚本只负责**模板库**这一件事，不改测试夹具：
#   bash scripts/test_db_template.sh ensure     # 建/复用模板库（幂等）
#   bash scripts/test_db_template.sh status     # 看模板库状态
#   bash scripts/test_db_template.sh drop       # 删模板库
#   bash scripts/test_db_template.sh clone demo # 用模板建一个库（手工验证用）
#
# 模板库名：`synapse_tmpl_v<REVISION>_<sha256(baseline) 前 16 位>`。Rust 侧（P1）
# 若用自己的指纹，必须显式传入同样的名字，否则会各建一份 —— 见 --name。
#
# 安全：只允许在**名字含 test 的库所在集群**上操作（与 test_db_health.sh 同口径），
# 避免把生产库当模板源。
set -euo pipefail

BASELINE="migrations/00000000_unified_schema_v12.sql"
REVISION=2
TEMPLATE_NAME=""

usage() {
    # 只打印文件头的注释块（遇到第一行非注释就停）。
    awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "${BASH_SOURCE[0]}"
    exit "${1:-0}"
}

while [ $# -gt 0 ]; do
    case "${1}" in
        --name)
            TEMPLATE_NAME="${2:?--name 需要参数}"
            shift 2
            ;;
        --baseline)
            BASELINE="${2:?--baseline 需要参数}"
            shift 2
            ;;
        -h | --help)
            usage 0
            ;;
        *)
            # 第一个非选项即动作；剩下的参数留给 clone 之类的子命令。
            ACTION="${1}"
            shift
            break
            ;;
    esac
done

ACTION="${ACTION:-status}"

DATABASE_URL="${DATABASE_URL:-${TEST_DATABASE_URL:-}}"
if [ -z "${DATABASE_URL}" ]; then
    echo "未提供连接串：设置 DATABASE_URL 或 TEST_DATABASE_URL（本地测试库是 127.0.0.1:55432/synapse_test）。" >&2
    exit 2
fi
command -v psql >/dev/null 2>&1 || {
    echo "未找到 psql：把它放进 PATH（本机是 /opt/homebrew/opt/postgresql@15/bin）。" >&2
    exit 2
}

# 连到 postgres 维护库执行"集群级"语句（CREATE/DROP DATABASE）。
maintenance_url() {
    printf '%s' "${DATABASE_URL}" | sed -E 's#/[^/?]+(\?|$)#/postgres\1#'
}

q() { psql "${1:-${DATABASE_URL}}" -tAX -v ON_ERROR_STOP=1 -c "${2}"; }

base_db="$(q "${DATABASE_URL}" "SELECT current_database()")"
case "${base_db}" in
    *test*) ;;
    *)
        echo "拒绝操作：基准库 ${base_db} 名字里没有 test，模板库只允许建在测试集群。" >&2
        exit 2
        ;;
esac

if [ -z "${TEMPLATE_NAME}" ]; then
    if [ ! -f "${BASELINE}" ]; then
        echo "找不到基线 SQL：${BASELINE}" >&2
        exit 2
    fi
    fp="$(shasum -a 256 "${BASELINE}" | cut -c1-16)"
    TEMPLATE_NAME="synapse_tmpl_v${REVISION}_${fp}"
fi

MURL="$(maintenance_url)"

template_exists() {
    [ "$(q "${MURL}" "SELECT count(*) FROM pg_database WHERE datname = '${TEMPLATE_NAME}'")" = "1" ]
}

case "${ACTION}" in
    ensure)
        if template_exists; then
            echo "模板库已存在：${TEMPLATE_NAME}（复用）"
            q "${MURL}" "SELECT pg_size_pretty(pg_database_size('${TEMPLATE_NAME}'))" | sed 's/^/  体积 /'
            exit 0
        fi
        echo "创建模板库 ${TEMPLATE_NAME} ..."
        q "${MURL}" "CREATE DATABASE \"${TEMPLATE_NAME}\" TEMPLATE template0"
        turl="$(printf '%s' "${DATABASE_URL}" | sed -E "s#/[^/?]+(\\?|\$)#/${TEMPLATE_NAME}\\1#")"
        psql "${turl}" -q -v ON_ERROR_STOP=1 -f "${BASELINE}" >/dev/null
        q "${turl}" "ANALYZE" >/dev/null
        # 把迁移写脏的 shared_buffers 先刷掉：否则第一次 DROP DATABASE 会替它买单
        # （实测 26.4 s，其后每次 0.3 s）。
        q "${turl}" "CHECKPOINT" >/dev/null
        # 表数必须在禁连**之前**取：禁连之后就再也连不进模板库了。
        tables="$(q "${turl}" "SELECT count(*) FROM pg_tables WHERE schemaname = 'public'")"
        # 禁连是硬要求：CREATE DATABASE ... TEMPLATE 要求模板上没有任何会话，
        # 禁连同时也防止测试误连模板。
        q "${MURL}" "UPDATE pg_database SET datallowconn = false WHERE datname = '${TEMPLATE_NAME}'"
        echo "完成：${tables} 张业务表，datallowconn=false"
        ;;
    status)
        if ! template_exists; then
            echo "模板库不存在：${TEMPLATE_NAME}"
            exit 1
        fi
        echo "模板库：${TEMPLATE_NAME}"
        q "${MURL}" "SELECT '  体积 ' || pg_size_pretty(pg_database_size('${TEMPLATE_NAME}'))"
        q "${MURL}" "SELECT '  允许连接 ' || datallowconn FROM pg_database WHERE datname = '${TEMPLATE_NAME}'"
        ;;
    clone)
        target="${1:-}"
        if [ -z "${target}" ]; then
            echo "用法：$0 clone <目标库名>" >&2
            exit 2
        fi
        template_exists || {
            echo "模板库 ${TEMPLATE_NAME} 不存在，先跑 ensure" >&2
            exit 1
        }
        start=$(date +%s.%N)
        q "${MURL}" "CREATE DATABASE \"${target}\" TEMPLATE \"${TEMPLATE_NAME}\""
        end=$(date +%s.%N)
        echo "已创建 ${target}，耗时 $(echo "${end} - ${start}" | bc) s"
        ;;
    drop)
        if ! template_exists; then
            echo "模板库不存在：${TEMPLATE_NAME}"
            exit 0
        fi
        q "${MURL}" "DROP DATABASE \"${TEMPLATE_NAME}\" WITH (FORCE)"
        echo "已删除 ${TEMPLATE_NAME}"
        ;;
    *)
        echo "未知动作：${ACTION}" >&2
        usage 2
        ;;
esac
