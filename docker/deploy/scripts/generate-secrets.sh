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
    if command -v openssl &> /dev/null; then
        openssl rand -hex $((length / 2))
    else
        head -c $((length / 2)) /dev/urandom | xxd -p
    fi
}

# 生成随机密码 (只包含大小写字母和数字，避免URL特殊字符)
generate_password() {
    local length=${1:-32}
    if command -v openssl &> /dev/null; then
        openssl rand -base64 48 | tr -dc 'A-Za-z0-9' | head -c $length
    else
        tr -dc 'A-Za-z0-9' < /dev/urandom | head -c $length
    fi
}

# 生成 JWT 密钥 (Base64 编码)
generate_jwt_secret() {
    if command -v openssl &> /dev/null; then
        openssl rand -base64 64 | tr -d '\n'
    else
        head -c 64 /dev/urandom | base64 | tr -d '\n'
    fi
}

generate_missing_or_all() {
    local force_generate="${1:-false}"

    [ -f "$ENV_FILE" ] || {
        log_error ".env 文件不存在: $ENV_FILE"
        return 1
    }

    maybe_set_secret "POSTGRES_PASSWORD" "$(generate_password 32)" "$force_generate"
    maybe_set_secret "REDIS_PASSWORD" "$(generate_password 32)" "$force_generate"
    maybe_set_secret "ADMIN_SHARED_SECRET" "$(generate_hex_key 64)" "$force_generate"
    maybe_set_secret "JWT_SECRET" "$(generate_jwt_secret)" "$force_generate"
    maybe_set_secret "REGISTRATION_SHARED_SECRET" "$(generate_hex_key 64)" "$force_generate"
    maybe_set_secret "SECRET_KEY" "$(generate_hex_key 64)" "$force_generate"
    maybe_set_secret "MACAROON_SECRET" "$(generate_hex_key 64)" "$force_generate"
    maybe_set_secret "FORM_SECRET" "$(generate_hex_key 64)" "$force_generate"
}

current_env_value() {
    local key=$1
    if grep -q "^${key}=" "$ENV_FILE"; then
        grep "^${key}=" "$ENV_FILE" | tail -n 1 | cut -d'=' -f2-
    fi
}

placeholder_or_empty() {
    local value=${1:-}
    [ -z "$value" ] || [[ "$value" == __REQUIRED_* ]] || [[ "$value" == *"change-me"* ]] || [[ "$value" == *"your-"* ]]
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

# 更新 .env 文件中的密钥
update_env_file() {
    local key=$1
    local value=$2
    
    if [ -f "$ENV_FILE" ]; then
        # 检查 key 是否存在
        if grep -q "^${key}=" "$ENV_FILE"; then
            # 更新现有值
            if [[ "$OSTYPE" == "darwin"* ]]; then
                sed -i '' "s|^${key}=.*|${key}=${value}|" "$ENV_FILE"
            else
                sed -i "s|^${key}=.*|${key}=${value}|" "$ENV_FILE"
            fi
        else
            # 添加新值
            echo "${key}=${value}" >> "$ENV_FILE"
        fi
    else
        log_error ".env 文件不存在: $ENV_FILE"
        return 1
    fi
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
        "jwt")
            generate_jwt_secret
            ;;
        "registration")
            generate_hex_key 64
            ;;
        "secret"|"macaroon"|"form")
            generate_hex_key 64
            ;;
        *)
            log_error "未知密钥类型: $type"
            echo "可用类型: postgres, redis, admin, jwt, registration, secret, macaroon, form"
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
    echo "  jwt       生成 JWT 密钥"
    echo "  registration 生成注册共享密钥"
    echo "  secret    生成应用安全密钥"
    echo "  macaroon  生成 macaroon 密钥"
    echo "  form      生成表单密钥"
    echo "  help      显示此帮助信息"
    echo ""
    echo "示例:"
    echo "  $0 all        # 生成所有密钥"
    echo "  $0 jwt        # 只生成 JWT 密钥"
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
        postgres|redis|admin|jwt|registration|secret|macaroon|form)
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
            fi
            
            if [ -f "$ENV_FILE" ]; then
                update_env_file "$env_key" "$secret"
                log_success "$env_key 已更新到 .env 文件"
            fi
            ;;
        help|--help|-h)
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
