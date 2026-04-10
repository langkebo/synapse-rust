#!/bin/bash
# =============================================================================
# 密钥和密码自动生成脚本
# =============================================================================
# 此脚本用于生成高强度的随机密码和密钥，并自动更新 .env 文件
# =============================================================================

set -e

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
        cat /dev/urandom | head -c $((length / 2)) | xxd -p
    fi
}

# 生成随机密码 (只包含大小写字母和数字，避免URL特殊字符)
generate_password() {
    local length=${1:-32}
    if command -v openssl &> /dev/null; then
        openssl rand -base64 48 | tr -dc 'A-Za-z0-9' | head -c $length
    else
        cat /dev/urandom | tr -dc 'A-Za-z0-9' | head -c $length
    fi
}

# 生成 JWT 密钥 (Base64 编码)
generate_jwt_secret() {
    if command -v openssl &> /dev/null; then
        openssl rand -base64 64 | tr -d '\n'
    else
        cat /dev/urandom | head -c 64 | base64 | tr -d '\n'
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
    log_info "生成安全密钥和密码..."
    
    # PostgreSQL 密码
    POSTGRES_PASSWORD=$(generate_password 32)
    log_info "POSTGRES_PASSWORD: 已生成 (${#POSTGRES_PASSWORD} 字符)"
    
    # 管理员共享密钥
    ADMIN_SHARED_SECRET=$(generate_hex_key 64)
    log_info "ADMIN_SHARED_SECRET: 已生成 (${#ADMIN_SHARED_SECRET} 字符)"
    
    # JWT 密钥
    JWT_SECRET=$(generate_jwt_secret)
    log_info "JWT_SECRET: 已生成 (${#JWT_SECRET} 字符)"
    
    # 注册共享密钥
    REGISTRATION_SHARED_SECRET=$(generate_hex_key 64)
    log_info "REGISTRATION_SHARED_SECRET: 已生成 (${#REGISTRATION_SHARED_SECRET} 字符)"
    
    # 输出结果
    echo ""
    echo "=========================================="
    echo "  生成的密钥和密码"
    echo "=========================================="
    echo ""
    echo "POSTGRES_PASSWORD=${POSTGRES_PASSWORD}"
    echo "ADMIN_SHARED_SECRET=${ADMIN_SHARED_SECRET}"
    echo "JWT_SECRET=${JWT_SECRET}"
    echo "REGISTRATION_SHARED_SECRET=${REGISTRATION_SHARED_SECRET}"
    echo ""
    
    # 更新 .env 文件
    if [ -f "$ENV_FILE" ]; then
        log_info "更新 .env 文件..."
        update_env_file "POSTGRES_PASSWORD" "$POSTGRES_PASSWORD"
        update_env_file "ADMIN_SHARED_SECRET" "$ADMIN_SHARED_SECRET"
        update_env_file "JWT_SECRET" "$JWT_SECRET"
        update_env_file "REGISTRATION_SHARED_SECRET" "$REGISTRATION_SHARED_SECRET"
        log_success ".env 文件已更新"
    else
        log_warning ".env 文件不存在，跳过更新"
    fi
}

# 生成单个密钥
generate_single_secret() {
    local type=$1
    case $type in
        "postgres")
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
        *)
            log_error "未知密钥类型: $type"
            echo "可用类型: postgres, admin, jwt, registration"
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
    echo "  postgres  生成 PostgreSQL 密码"
    echo "  admin     生成管理员共享密钥"
    echo "  jwt       生成 JWT 密钥"
    echo "  registration 生成注册共享密钥"
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
        postgres|admin|jwt|registration)
            local secret=$(generate_single_secret "$command")
            echo "$secret"
            
            # 转换为大写的环境变量名
            local env_key=$(echo "$command" | tr '[:lower:]' '[:upper:]')
            if [ "$command" = "postgres" ]; then
                env_key="POSTGRES_PASSWORD"
            elif [ "$command" = "admin" ]; then
                env_key="ADMIN_SHARED_SECRET"
            elif [ "$command" = "registration" ]; then
                env_key="REGISTRATION_SHARED_SECRET"
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
