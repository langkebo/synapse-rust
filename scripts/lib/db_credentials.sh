#!/usr/bin/env bash
# =============================================================================
# 本机数据库凭据的**单一解析来源**
# =============================================================================
#
# 为什么需要它：十几个脚本和 Makefile 各自把 `synapse:synapse@localhost:5432`
# 写死在默认值里。而 deploy 栈自 2026-10-15 起会在部署时生成**随机强口令**
# （`docker/deploy/scripts/generate-secrets.sh`）—— 于是那些硬编码默认值会以
# "看起来只是用了默认值、其实是错口令"的方式失败：报出来的是
# `password authentication failed`，而不是"你没配密码"，最难排查。
#
# 解析优先级（第一个非空者胜）：
#   1. 显式环境变量 `DB_PASSWORD` / `TEST_DB_PASSWORD` —— 一次性覆盖与 CI；
#      **CI 里没有 `docker/deploy/.env`**，所以会落到第 4 条，与历史行为
#      （口令 `synapse`）完全一致 ⇒ 这条改动对 CI 是零影响。
#   2. `docker/deploy/.env` 的 `POSTGRES_PASSWORD` —— **本机真正在跑的 deploy 栈**。
#   3. `docker/.env` 的 `DB_PASSWORD` —— dev 栈约定（首次部署前的形态）。
#   4. 兜底 `synapse`。
#
# 用法：
#   source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib/db_credentials.sh"
#   export PGPASSWORD="$(db_password)"
#   psql -h "$(db_host)" -p "$(db_port)" -U "$(db_user)" -d synapse
#
# 也可直接当命令用（适合 `eval`）：
#   eval "$(bash scripts/lib/db_credentials.sh exports)"
#   bash scripts/lib/db_credentials.sh url synapse_test
#
# 注意：本文件**不做任何输出副作用**（source 时静默），也不打印口令到日志。
# =============================================================================

# 仓库根由本文件自身位置推导（`<repo>/scripts/lib/` ⇒ 上两级），与调用方 cwd 无关。
_db_creds_lib_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_DB_CREDS_ROOT="$(cd "${_db_creds_lib_dir}/../.." && pwd)"

# 从 KEY=VALUE 文件里取一个键的值。去掉两侧引号与 CR；键不存在则输出空。
_db_creds_env_value() {
    local file="$1"
    local key="$2"
    local line=""
    local value=""

    [ -f "$file" ] || return 0

    while IFS= read -r line || [ -n "$line" ]; do
        case "$line" in
            "${key}="*) value="${line#*=}" ;;
            *) continue ;;
        esac
        value="${value%$'\r'}"
        case "$value" in
            \"*\") value="${value#\"}" && value="${value%\"}" ;;
            \'*\') value="${value#\'}" && value="${value%\'}" ;;
        esac
        printf '%s' "$value"
        return 0
    done <"$file"
}

_db_creds_first_non_empty() {
    local candidate
    for candidate in "$@"; do
        if [ -n "$candidate" ]; then
            printf '%s' "$candidate"
            return 0
        fi
    done
}

_db_creds_init() {
    [ -n "${_DB_CREDS_READY:-}" ] && return 0

    local deploy_env="${_DB_CREDS_ROOT}/docker/deploy/.env"
    local dev_env="${_DB_CREDS_ROOT}/docker/.env"

    _DB_CREDS_PASSWORD="$(_db_creds_first_non_empty \
        "${DB_PASSWORD:-}" \
        "${TEST_DB_PASSWORD:-}" \
        "$(_db_creds_env_value "$deploy_env" POSTGRES_PASSWORD)" \
        "$(_db_creds_env_value "$dev_env" DB_PASSWORD)" \
        "synapse")"

    _DB_CREDS_USER="$(_db_creds_first_non_empty \
        "${DB_USER:-}" \
        "${TEST_DB_USER:-}" \
        "$(_db_creds_env_value "$deploy_env" POSTGRES_USER)" \
        "$(_db_creds_env_value "$dev_env" DB_USER)" \
        "synapse")"

    _DB_CREDS_HOST="$(_db_creds_first_non_empty "${DB_HOST:-}" "${TEST_DB_HOST:-}" "127.0.0.1")"
    _DB_CREDS_PORT="$(_db_creds_first_non_empty "${DB_PORT:-}" "${TEST_DB_PORT:-}" "5432")"

    _DB_CREDS_READY=1
}

db_password() {
    _db_creds_init
    printf '%s' "$_DB_CREDS_PASSWORD"
}

db_user() {
    _db_creds_init
    printf '%s' "$_DB_CREDS_USER"
}

db_host() {
    _db_creds_init
    printf '%s' "$_DB_CREDS_HOST"
}

db_port() {
    _db_creds_init
    printf '%s' "$_DB_CREDS_PORT"
}

# 完整 URL：db_url [数据库名]（默认应用库 `synapse`）。
db_url() {
    _db_creds_init
    local database="${1:-synapse}"
    printf 'postgresql://%s:%s@%s:%s/%s' \
        "$_DB_CREDS_USER" "$_DB_CREDS_PASSWORD" "$_DB_CREDS_HOST" "$_DB_CREDS_PORT" "$database"
}

# 口令来源（排障用，不含口令本身）。
db_password_source() {
    _db_creds_init
    if [ -n "${DB_PASSWORD:-}" ]; then
        printf 'env:DB_PASSWORD'
    elif [ -n "${TEST_DB_PASSWORD:-}" ]; then
        printf 'env:TEST_DB_PASSWORD'
    elif [ -n "$(_db_creds_env_value "${_DB_CREDS_ROOT}/docker/deploy/.env" POSTGRES_PASSWORD)" ]; then
        printf 'file:docker/deploy/.env'
    elif [ -n "$(_db_creds_env_value "${_DB_CREDS_ROOT}/docker/.env" DB_PASSWORD)" ]; then
        printf 'file:docker/.env'
    else
        printf 'fallback:synapse'
    fi
}

# ---------------------------------------------------------------------------
# 直接执行时的命令行入口（被 source 时不做任何事）
# ---------------------------------------------------------------------------
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
    case "${1:-url}" in
        url)
            db_url "${2:-synapse}"
            printf '\n'
            ;;
        password)
            db_password
            printf '\n'
            ;;
        source)
            db_password_source
            printf '\n'
            ;;
        exports)
            _db_creds_init
            printf 'export DB_USER=%q\n' "$_DB_CREDS_USER"
            printf 'export DB_PASSWORD=%q\n' "$_DB_CREDS_PASSWORD"
            printf 'export DB_HOST=%q\n' "$_DB_CREDS_HOST"
            printf 'export DB_PORT=%q\n' "$_DB_CREDS_PORT"
            printf 'export PGPASSWORD=%q\n' "$_DB_CREDS_PASSWORD"
            ;;
        *)
            printf '用法: %s [url|password|source|exports] [数据库名]\n' "$0" >&2
            exit 2
            ;;
    esac
fi
