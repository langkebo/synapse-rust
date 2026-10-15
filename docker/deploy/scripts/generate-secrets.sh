#!/bin/bash
# =============================================================================
# 密钥和密码自动生成脚本
# =============================================================================
# 此脚本用于生成高强度的随机密码和密钥，并自动更新 .env 文件
# =============================================================================

set -euo pipefail

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# 获取脚本所在目录
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPLOY_DIR="$(dirname "$SCRIPT_DIR")"
ENV_FILE="${DEPLOY_DIR}/.env"

# 生成随机密钥 (64 字符十六进制)
generate_hex_key() {
    local length=${1:-64}
    if command -v openssl &>/dev/null; then
        openssl rand -hex $((length / 2))
    else
        head -c $((length / 2)) /dev/urandom | xxd -p
    fi
}

# 生成随机密码 (只包含大小写字母和数字，避免URL特殊字符)
generate_password() {
    local length=${1:-32}
    if command -v openssl &>/dev/null; then
        openssl rand -base64 48 | tr -dc 'A-Za-z0-9' | head -c $length
    else
        tr -dc 'A-Za-z0-9' </dev/urandom | head -c $length
    fi
}

# 生成 TOTP base32 密钥 (RFC 4648 字母表，20 字节 → 32 字符，无 '=' 填充)。
# admin_auth::decode_secret 会优先按 base32 解码，回退原始字节；这里必须是
# 合法 base32，否则认证器 App 与服务端算出的 TOTP 不一致。
generate_base32_secret() {
    if command -v base32 &>/dev/null; then
        head -c 20 /dev/urandom | base32 | tr -d '=\n'
    elif command -v python3 &>/dev/null; then
        python3 -c 'import base64,os;print(base64.b32encode(os.urandom(20)).decode().rstrip("="))'
    else
        log_error "需要 base32 或 python3 以生成 TOTP 密钥"
        return 1
    fi
}

generate_missing_or_all() {
    local force_generate="${1:-false}"

    [ -f "$ENV_FILE" ] || {
        log_error ".env 文件不存在: $ENV_FILE"
        return 1
    }

    # 口令类键走 maybe_set_password：它把"弱值"也当成"缺失"处理。
    # 只用 placeholder_or_empty 是不够的 —— 它认得出 `CHANGE_ME`，却认不出
    # `synapse` / `postgres` 这类"看着像真值、其实是弱值"的口令，于是生成阶段
    # 静默跳过，弱口令就一直留在生产里（2026-10-15 实测：`.env` 的
    # POSTGRES_PASSWORD 就是 `synapse`，而 deploy.sh 的检查一路放行）。
    maybe_set_password "POSTGRES_PASSWORD" "$(generate_password 32)" "$force_generate"
    maybe_set_password "REDIS_PASSWORD" "$(generate_password 32)" "$force_generate"
    maybe_set_secret "ADMIN_SHARED_SECRET" "$(generate_hex_key 64)" "$force_generate"
    maybe_set_secret "REGISTRATION_SHARED_SECRET" "$(generate_hex_key 64)" "$force_generate"
    maybe_set_secret "SECRET_KEY" "$(generate_hex_key 64)" "$force_generate"
    maybe_set_secret "MACAROON_SECRET" "$(generate_hex_key 64)" "$force_generate"
    maybe_set_secret "FORM_SECRET" "$(generate_hex_key 64)" "$force_generate"
    # 64 十六进制字符 = 32 字节，满足 signing_key_master_key 的长度下限。
    maybe_set_secret "FEDERATION_MASTER_KEY" "$(generate_hex_key 64)" "$force_generate"
    # 64 十六进制字符 = 32 字节，满足 token 哈希密钥 >=32 字节的下限
    # （docker-compose.yml 以 `:?` 强制要求非空）。
    maybe_set_secret "TOKEN_HASH_SECRET" "$(generate_hex_key 64)" "$force_generate"
    # worker 间复制认证密钥（docker-compose.yml 以 `:?` 强制要求非空）。
    maybe_set_secret "WORKER_REPLICATION_SECRET" "$(generate_hex_key 64)" "$force_generate"
    # Olm 账户 pickle 密钥：**恰好** 32 字节（64 个十六进制字符）。缺失或长度不对
    # 时 OLM 服务 fail loudly（E-06），且每次重启都会使已持久化的 Olm 账户不可解密。
    maybe_set_secret "OLM_PICKLE_KEY" "$(generate_hex_key 64)" "$force_generate"
    # C9: 管理员 MFA 的 TOTP base32 密钥（docker-compose.yml 以 `:?` 强制要求非空）。
    maybe_set_secret "ADMIN_MFA_SHARED_SECRET" "$(generate_base32_secret)" "$force_generate"
}

current_env_value() {
    local key=$1
    if grep -q "^${key}=" "$ENV_FILE"; then
        grep "^${key}=" "$ENV_FILE" | tail -n 1 | cut -d'=' -f2-
    fi
}

placeholder_or_empty() {
    local value=${1:-}
    # CHANGE_ME 是 .env.example 使用的占位符（全大写下划线），与旧式的
    # change-me / your- 形态都要识别，否则 `missing` 会把它当成"已配置"而跳过生成。
    [ -z "$value" ] || [[ "$value" == __REQUIRED_* ]] || [[ "$value" == *"change-me"* ]] || [[ "$value" == *"your-"* ]] || [[ "$value" == *"CHANGE_ME"* ]]
}

# 口令强度的**单一判据**（deploy.sh 通过 `check-password` 复用它，不另抄一份名单）。
#
# 为什么需要它：`placeholder_or_empty` 只认 `.env.example` 的 `CHANGE_ME` 形态。
# 但真实事故形态是"值看起来是合法的"——`synapse` / `postgres` / `admin` 这类
# 与项目名、角色名同形的弱口令，能通过一切"非空且不像占位符"的检查，
# 于是一路留到生产。2026-10-15 实测本仓库 `.env` 的 POSTGRES_PASSWORD 就是
# `synapse`。这里显式列名单 + 设长度下限。
#
# 放行：确有必要保留自定义口令时置 `ALLOW_WEAK_DB_PASSWORD=1`（会打警告，不静默）。
MIN_PASSWORD_LENGTH=16
WEAK_PASSWORDS="synapse postgres password passwd changeme change_me admin root test dev secret matrix synapse_dev synapse_rust"

is_weak_password() {
    local value="${1:-}"
    [ -z "$value" ] && return 0
    local lower
    lower="$(printf '%s' "$value" | tr '[:upper:]' '[:lower:]')"
    local candidate
    for candidate in $WEAK_PASSWORDS; do
        [ "$lower" = "$candidate" ] && return 0
    done
    [ "${#value}" -lt "$MIN_PASSWORD_LENGTH" ]
}

maybe_set_secret() {
    local key=$1
    local new_value=$2
    local force_generate=${3:-false}
    local old_value
    old_value="$(current_env_value "$key")"

    if [ "$force_generate" = "true" ] || placeholder_or_empty "$old_value"; then
        update_env_file "$key" "$new_value"
        log_info "$key: 已生成"
    else
        log_info "$key: 已存在，跳过"
    fi
}

# 密码类键：把"弱"也当作"缺失"来补。
#
# 与 maybe_set_secret 的唯一区别是多了 is_weak_password 这一条判据，并且
# 轮换时会明确说"因为原值弱"而不是笼统的"已生成"——否则运维会以为口令一直是
# 随机值，从而失去"我改过它"的线索。
maybe_set_password() {
    local key=$1
    local new_value=$2
    local force_generate=${3:-false}
    local old_value
    old_value="$(current_env_value "$key")"

    if [ "${ALLOW_WEAK_DB_PASSWORD:-0}" = "1" ] && ! placeholder_or_empty "$old_value"; then
        log_warning "$key: 当前值偏弱，但 ALLOW_WEAK_DB_PASSWORD=1 显式放行，保持原值"
        return 0
    fi

    if [ "$force_generate" = "true" ]; then
        update_env_file "$key" "$new_value"
        log_warning "$key: 已强制重新生成"
    elif placeholder_or_empty "$old_value"; then
        update_env_file "$key" "$new_value"
        log_info "$key: 缺失或为占位符，已生成"
    elif is_weak_password "$old_value"; then
        update_env_file "$key" "$new_value"
        log_warning "$key: 原值偏弱（长度 ${#old_value}，命中弱口令判据），已轮换为强随机值"
    else
        log_info "$key: 已存在且强度足够，跳过"
    fi
}

# 更新 .env 文件中的密钥
#
# 刻意**不用** `sed -i`：BSD 与 GNU 的 `-i` 语义不同（`-i ''` vs `-i`），
# 而且在被包装过的 shell 环境里 `-i ''` 会被当成文件名吃掉，实测报
# `sed: s|^A=.*|A=CHANGED|: No such file or directory` —— 于是"生成了密钥"
# 的日志是真的，写回 `.env` 却根本没发生。纯 bash 逐行重写两边都成立，
# 也顺带避免了"值里含 `|` / `&` 时 sed 替换语义被吃掉"这一类问题。
update_env_file() {
    local key=$1
    local value=$2

    [ -f "$ENV_FILE" ] || {
        log_error ".env 文件不存在: $ENV_FILE"
        return 1
    }

    local tmp
    tmp="$(mktemp "${ENV_FILE}.tmp.XXXXXX")" || {
        log_error "无法在 $(dirname "$ENV_FILE") 创建临时文件"
        return 1
    }

    local replaced=false
    local line
    while IFS= read -r line || [ -n "$line" ]; do
        case "$line" in
            "${key}="*)
                printf '%s=%s\n' "$key" "$value" >>"$tmp"
                replaced=true
                ;;
            *)
                printf '%s\n' "$line" >>"$tmp"
                ;;
        esac
    done <"$ENV_FILE"

    if [ "$replaced" = "false" ]; then
        printf '%s=%s\n' "$key" "$value" >>"$tmp"
    fi

    # 覆盖写回（保留原 inode 与权限：.env 常被 chmod 600，也可能是 bind mount）
    cat "$tmp" >"$ENV_FILE"
    rm -f "$tmp"
}

# 生成所有密钥
generate_all_secrets() {
    log_info "生成并覆盖所有安全密钥和密码..."
    generate_missing_or_all true
    log_success ".env 文件已更新"
}

generate_missing_secrets() {
    log_info "补齐缺失的安全密钥和密码..."
    generate_missing_or_all false
    log_success ".env 文件已更新"
}

# 生成单个密钥
generate_single_secret() {
    local type=$1
    case $type in
        "postgres")
            generate_password 32
            ;;
        "redis")
            generate_password 32
            ;;
        "admin")
            generate_hex_key 64
            ;;
        "registration")
            generate_hex_key 64
            ;;
        "secret" | "macaroon" | "form")
            generate_hex_key 64
            ;;
        "worker-replication")
            # 64 十六进制字符 = 32 字节，满足 MIN_REPLICATION_SECRET_LEN。
            generate_hex_key 64
            ;;
        "olm-pickle")
            # Olm 账户 pickle 密钥：恰好 32 字节（64 个十六进制字符）。
            generate_hex_key 64
            ;;
        "admin-mfa")
            generate_base32_secret
            ;;
        *)
            log_error "未知密钥类型: $type"
            echo "可用类型: postgres, redis, admin, registration, secret, macaroon, form, worker-replication, olm-pickle, admin-mfa"
            return 1
            ;;
    esac
}

# 显示帮助信息
show_help() {
    echo "用法: $0 [命令]"
    echo ""
    echo "命令:"
    echo "  all       生成所有密钥并更新 .env 文件 (默认)"
    echo "  missing   仅补齐缺失或占位符密钥"
    echo "  postgres  生成 PostgreSQL 密码"
    echo "  redis     生成 Redis 密码"
    echo "  admin     生成管理员共享密钥"
    echo "  registration 生成注册共享密钥"
    echo "  secret    生成应用安全密钥"
    echo "  macaroon  生成 macaroon 密钥"
    echo "  form      生成表单密钥"
    echo "  worker-replication  轮换 worker 复制密钥（轮换后需重启所有 worker）"
    echo "  olm-pickle  生成 OLM 账户 pickle 密钥（32 字节 hex）"
    echo "  admin-mfa  生成管理员 MFA 的 TOTP base32 密钥"
    echo "  check-password [postgres|redis]  检查口令强度（弱则退出码 1；供 deploy.sh 复用）"
    echo "  help      显示此帮助信息"
    echo ""
    echo "示例:"
    echo "  $0 all        # 生成所有密钥"
    echo "  $0 postgres   # 只生成 PostgreSQL 密码"
}

# 主函数
main() {
    local command=${1:-all}

    case $command in
        all)
            generate_all_secrets
            ;;
        missing)
            generate_missing_secrets
            ;;
        check-password | check-db-password)
            # 口令强度的**唯一实现**在这里；deploy.sh 调这个子命令，不另抄名单。
            local target=${2:-postgres}
            local key
            case "$target" in
                postgres | postgresql | db) key="POSTGRES_PASSWORD" ;;
                redis) key="REDIS_PASSWORD" ;;
                *)
                    log_error "未知目标: $target（可用: postgres, redis）"
                    return 1
                    ;;
            esac
            local value
            value="$(current_env_value "$key")"
            if [ "${ALLOW_WEAK_DB_PASSWORD:-0}" = "1" ]; then
                log_warning "${key}: 弱口令检查被 ALLOW_WEAK_DB_PASSWORD=1 跳过"
                return 0
            fi
            if placeholder_or_empty "$value" || is_weak_password "$value"; then
                log_error "${key} 未设置或属于弱口令（长度 ${#value}，下限 ${MIN_PASSWORD_LENGTH}）"
                return 1
            fi
            log_success "${key} 强度检查通过（长度 ${#value}）"
            ;;
        postgres | redis | admin | registration | secret | macaroon | form | worker-replication | olm-pickle | admin-mfa)
            local secret=$(generate_single_secret "$command")
            echo "$secret"

            # 转换为大写的环境变量名
            local env_key=$(echo "$command" | tr '[:lower:]' '[:upper:]')
            if [ "$command" = "postgres" ]; then
                env_key="POSTGRES_PASSWORD"
            elif [ "$command" = "redis" ]; then
                env_key="REDIS_PASSWORD"
            elif [ "$command" = "admin" ]; then
                env_key="ADMIN_SHARED_SECRET"
            elif [ "$command" = "registration" ]; then
                env_key="REGISTRATION_SHARED_SECRET"
            elif [ "$command" = "secret" ]; then
                env_key="SECRET_KEY"
            elif [ "$command" = "macaroon" ]; then
                env_key="MACAROON_SECRET"
            elif [ "$command" = "form" ]; then
                env_key="FORM_SECRET"
            elif [ "$command" = "worker-replication" ]; then
                env_key="WORKER_REPLICATION_SECRET"
            elif [ "$command" = "olm-pickle" ]; then
                env_key="OLM_PICKLE_KEY"
            elif [ "$command" = "admin-mfa" ]; then
                env_key="ADMIN_MFA_SHARED_SECRET"
            fi

            if [ -f "$ENV_FILE" ]; then
                update_env_file "$env_key" "$secret"
                log_success "$env_key 已更新到 .env 文件"
            fi
            ;;
        help | --help | -h)
            show_help
            ;;
        *)
            log_error "未知命令: $command"
            show_help
            exit 1
            ;;
    esac
}

main "$@"
