#!/bin/bash
# =============================================================================
# 更新管理员密码
#
# 用法:
#   ADMIN_PASSWORD='xxx' ./update_admin_password.sh
#   ./update_admin_password.sh --password 'xxx'
#
# 说明:
#   - 密码不再硬编码，必须通过 --password 或环境变量 ADMIN_PASSWORD 提供；
#   - 管理员 user_id 由 SERVER_NAME 推导为 @admin:${SERVER_NAME}（与注册脚本一致）；
#   - 数据库连接参数从 .env 读取（POSTGRES_USER/POSTGRES_DB），避免与真实配置不一致；
#   - 通过 psql 变量绑定传参，避免 SQL 注入。
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

compose() {
    if command -v docker-compose &>/dev/null; then
        docker-compose "$@"
    else
        docker compose "$@"
    fi
}

# 加载 .env（提供 SERVER_NAME / POSTGRES_USER / POSTGRES_DB 等）
load_env() {
    if [ ! -f .env ]; then
        echo "WARNING: .env 不存在，使用默认值" >&2
        return 0
    fi

    while IFS= read -r raw_line || [ -n "$raw_line" ]; do
        local line="$raw_line"
        line="${line%$'\r'}"

        case "$line" in
            '' | \#*)
                continue
                ;;
        esac

        local key="${line%%=*}"
        local value="${line#*=}"

        key="${key#"${key%%[![:space:]]*}"}"
        key="${key%"${key##*[![:space:]]}"}"
        value="${value#"${value%%[![:space:]]*}"}"
        value="${value%"${value##*[![:space:]]}"}"

        if [[ "$key" == export\ * ]]; then
            key="${key#export }"
            key="${key#"${key%%[![:space:]]*}"}"
        fi

        if [[ "$value" == \"*\" && "$value" == *\" ]]; then
            value="${value:1:${#value}-2}"
        elif [[ "$value" == \'*\' && "$value" == *\' ]]; then
            value="${value:1:${#value}-2}"
        fi

        export "$key=$value"
    done <.env
}

load_env

# 解析参数（--password 优先于环境变量 ADMIN_PASSWORD）
PASSWORD="${ADMIN_PASSWORD:-}"
while [ $# -gt 0 ]; do
    case "$1" in
        --password)
            PASSWORD="${2:-}"
            shift 2
            ;;
        --password=*)
            PASSWORD="${1#*=}"
            shift
            ;;
        -h | --help)
            echo "用法: $0 [--password <password>]  (或设置 ADMIN_PASSWORD 环境变量)"
            exit 0
            ;;
        *)
            echo "ERROR: 未知参数: $1" >&2
            exit 1
            ;;
    esac
done

if [ -z "$PASSWORD" ]; then
    echo "ERROR: 必须通过 --password 或环境变量 ADMIN_PASSWORD 提供管理员密码" >&2
    exit 1
fi

SERVER_NAME="${SERVER_NAME:-localhost}"
ADMIN_USERNAME="${ADMIN_USERNAME:-admin}"
ADMIN_USER_ID="@${ADMIN_USERNAME}:${SERVER_NAME}"
POSTGRES_USER="${POSTGRES_USER:-postgres}"
POSTGRES_DB="${POSTGRES_DB:-synapse}"

# 生成 Argon2id PHC 哈希，参数对齐服务端默认（m=65536, t=3, p=1, hash_len=32）。
HASH="$(
    ADMIN_PASSWORD="$PASSWORD" python3 - <<'PY'
import os
import argon2

ph = argon2.PasswordHasher(
    time_cost=3,
    memory_cost=65536,
    parallelism=1,
    hash_len=32,
    salt_len=16,
)
print(ph.hash(os.environ["ADMIN_PASSWORD"]))
PY
)"

if [ -z "$HASH" ]; then
    echo "ERROR: 生成密码哈希失败（请确认已安装 python3 argon2 模块）" >&2
    exit 1
fi

# 使用 psql 变量绑定传参，避免 SQL 注入。
compose exec -T postgres psql \
    -U "$POSTGRES_USER" -d "$POSTGRES_DB" \
    -v ON_ERROR_STOP=1 \
    -v user_id="$ADMIN_USER_ID" \
    -v password_hash="$HASH" \
    -c "UPDATE users SET password_hash = :'password_hash' WHERE user_id = :'user_id';"

echo "管理员密码已更新: $ADMIN_USER_ID"
