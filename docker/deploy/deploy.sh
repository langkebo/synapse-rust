#!/bin/bash
# =============================================================================
# synapse-rust 一键重建与部署脚本
# =============================================================================
# 支持按需部署：通过交互式菜单或 ENABLED_EXTENSIONS 环境变量选择需要的
# 扩展功能，仅应用对应的数据库迁移脚本。
#
# 用法:
#   ./deploy.sh                   # 交互式选择功能
#   ./deploy.sh --all             # 部署所有功能（跳过交互）
#   ./deploy.sh --core-private-chat # 部署核心私密聊天能力（默认推荐）
#   ./deploy.sh --core-only       # 仅部署核心 Matrix 功能
#   ./deploy.sh --features LIST   # 部署指定功能（逗号分隔）
#   ./deploy.sh --skip-build      # 跳过编译与镜像构建
#   ./deploy.sh --install-deps    # 自动安装缺失依赖 (brew/apt/yum)
#   ./deploy.sh --no-turn         # 跳过本地 coturn TURN 检查
#   ./deploy.sh --image REF       # 使用指定的远程镜像（跳过本地构建，自动 pull）
#
# 完整流程: 环境检查 → 依赖安装(可选) → 配置检查 → SSL 证书自动生成 →
#           /etc/hosts 检查 → 本地 coturn 检查/启动 → 备份 → 缓存清理 →
#           镜像构建 → 数据库迁移 → 服务启动 → 健康/HTTPS 验证 → 日志检查
#
# 可用扩展功能:
#   friends, voice-extended, saml-sso, cas-sso,
#   beacons, voip-tracking, widgets, server-notifications,
#   burn-after-read, privacy-ext, external-services
# =============================================================================

set -Eeuo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DEPLOY_ROOT="$SCRIPT_DIR"
LOG_DIR="$DEPLOY_ROOT/logs"
TIMESTAMP="$(date +%Y%m%d_%H%M%S)"
LOG_FILE="$LOG_DIR/deploy_${TIMESTAMP}.log"
LOG_PIPE=""
LOG_TEE_PID=""

ROLLBACK_BACKUP=""
ROLLBACK_IMAGE_TAG=""
ROLLBACK_ENABLED=true
DEPLOYMENT_PHASE="initialization"
ROLLBACK_IN_PROGRESS=false
SKIP_BUILD=false
REMOTE_IMAGE=""
USE_REMOTE_IMAGE=false
INSTALL_DEPS=false
CHECK_TURN=true

# Extension features — order matches Cargo.toml
ALL_EXTENSIONS=(
    friends
    voice-extended
    saml-sso
    cas-sso
    beacons
    voip-tracking
    widgets
    server-notifications
    burn-after-read
    privacy-ext
    external-services
)

EXTENSION_DESCRIPTIONS=(
    "好友系统 (好友请求、好友分组)"
    "语音消息扩展 (语音消息录制/播放)"
    "SAML SSO 单点登录"
    "CAS SSO 单点登录"
    "位置信标 (实时位置共享)"
    "VoIP 通话追踪 (通话会话、MatrixRTC)"
    "Widget 小组件"
    "服务器通知系统"
    "阅后即焚消息"
    "隐私扩展 (已读回执控制、在线状态隐藏)"
    "外部服务集成 (Webhook 通知)"
)

# Default product mode: preserve the private chat core while slimming optional
# modules from the default deployment shape.
CORE_PRIVATE_CHAT_EXTENSIONS="friends,burn-after-read"

# Will be set by parse_args or select_features
ENABLED_EXTENSIONS="${ENABLED_EXTENSIONS:-}"

cd "$DEPLOY_ROOT"

log_info() {
    echo -e "${BLUE}[INFO]${NC} $*"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $*"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $*"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $*" >&2
}

compose() {
    if command -v docker-compose >/dev/null 2>&1; then
        docker-compose "$@"
    else
        docker compose "$@"
    fi
}

# =============================================================================
# CLI argument parsing
# =============================================================================

parse_args() {
    while [ $# -gt 0 ]; do
        case "$1" in
            --all)
                ENABLED_EXTENSIONS="all"
                ;;
            --core-private-chat)
                ENABLED_EXTENSIONS="$CORE_PRIVATE_CHAT_EXTENSIONS"
                ;;
            --core-only)
                ENABLED_EXTENSIONS="none"
                ;;
            --features)
                shift
                ENABLED_EXTENSIONS="${1:?'--features 需要参数，如: friends,voice-extended'}"
                ;;
            --skip-build)
                SKIP_BUILD=true
                ;;
            --install-deps)
                INSTALL_DEPS=true
                ;;
            --no-turn)
                CHECK_TURN=false
                ;;
            --image)
                shift
                REMOTE_IMAGE="${1:?'--image 需要参数，如: docker.io/vmuser232922/mysynapse:latest'}"
                USE_REMOTE_IMAGE=true
                SKIP_BUILD=true
                ;;
            --help | -h)
                show_usage
                exit 0
                ;;
            *)
                log_error "未知参数: $1"
                show_usage
                exit 1
                ;;
        esac
        shift
    done
}

show_usage() {
    cat <<'EOF'
用法: deploy.sh [选项]

选项:
  --all             部署所有功能（包含全部扩展，跳过交互选择）
  --core-private-chat 部署核心私密聊天能力（friends + burn-after-read）
  --core-only       仅部署核心 Matrix 功能（不含任何扩展）
  --features LIST   部署指定扩展功能（逗号分隔）
  --skip-build      跳过 cargo build 和 Docker 镜像构建
  --install-deps    自动安装缺失的依赖 (macOS: brew / Linux: apt/yum)
  --no-turn         跳过本地 coturn TURN 服务检查与启动
  --image REF       使用指定的远程镜像（自动 docker pull，跳过本地构建）
  --help            显示帮助信息

如果不指定功能参数，脚本将显示交互式功能选择菜单。
也可通过 .env 中的 ENABLED_EXTENSIONS 变量预设。

可用扩展功能:
  friends              好友系统
  voice-extended       语音消息扩展
  saml-sso             SAML SSO 单点登录
  cas-sso              CAS SSO 单点登录
  beacons              位置信标
  voip-tracking        VoIP 通话追踪
  widgets              Widget 小组件
  server-notifications 服务器通知
  burn-after-read      阅后即焚
  privacy-ext          隐私扩展
  external-services    外部服务集成
EOF
}

# =============================================================================
# Interactive feature selection
# =============================================================================

select_features() {
    # If already set (by CLI args or .env), skip interactive selection
    if [ -n "$ENABLED_EXTENSIONS" ]; then
        return
    fi

    echo ""
    echo -e "${BOLD}==========================================${NC}"
    echo -e "${BOLD}  功能选择${NC}"
    echo -e "${BOLD}==========================================${NC}"
    echo ""
    echo -e "  ${CYAN}[0]${NC} 核心私密聊天 (friends + burn-after-read，默认推荐)"
    echo -e "  ${CYAN}[1]${NC} 全部功能 (all-extensions)"
    echo -e "  ${CYAN}[2]${NC} 仅核心 Matrix 功能 (无扩展)"
    echo -e "  ${CYAN}[3]${NC} 自定义选择扩展功能"
    echo ""

    local choice
    read -rp "请选择部署模式 [0/1/2/3] (默认 0): " choice
    choice="${choice:-0}"

    case "$choice" in
        0)
            ENABLED_EXTENSIONS="$CORE_PRIVATE_CHAT_EXTENSIONS"
            log_info "已选择: 核心私密聊天"
            ;;
        1)
            ENABLED_EXTENSIONS="all"
            log_info "已选择: 全部功能"
            ;;
        2)
            ENABLED_EXTENSIONS="none"
            log_info "已选择: 仅核心 Matrix 功能"
            ;;
        3)
            select_individual_features
            ;;
        *)
            ENABLED_EXTENSIONS="$CORE_PRIVATE_CHAT_EXTENSIONS"
            log_warning "无效输入，默认使用核心私密聊天"
            ;;
    esac
}

select_individual_features() {
    local selected=()
    local i

    echo ""
    echo -e "${BOLD}可用扩展功能:${NC}"
    echo ""

    for i in "${!ALL_EXTENSIONS[@]}"; do
        local num=$((i + 1))
        printf "  ${CYAN}[%2d]${NC} %-24s %s\n" "$num" "${ALL_EXTENSIONS[$i]}" "${EXTENSION_DESCRIPTIONS[$i]}"
    done

    echo ""
    echo "输入功能编号（逗号或空格分隔），如: 1,2,7"
    echo "直接回车跳过所有扩展 (core-only)"
    echo ""
    read -rp "选择: " input

    if [ -z "$input" ]; then
        ENABLED_EXTENSIONS="none"
        log_info "未选择任何扩展，使用核心模式"
        return
    fi

    # Parse comma/space separated numbers
    local nums
    nums="$(echo "$input" | tr ',' ' ')"
    for num in $nums; do
        num="$(echo "$num" | tr -d '[:space:]')"
        if [ -z "$num" ]; then
            continue
        fi
        if ! [[ "$num" =~ ^[0-9]+$ ]]; then
            log_warning "忽略无效输入: $num"
            continue
        fi
        local idx=$((num - 1))
        if [ "$idx" -ge 0 ] && [ "$idx" -lt "${#ALL_EXTENSIONS[@]}" ]; then
            selected+=("${ALL_EXTENSIONS[$idx]}")
        else
            log_warning "忽略超范围编号: $num"
        fi
    done

    if [ ${#selected[@]} -eq 0 ]; then
        ENABLED_EXTENSIONS="none"
        log_info "未选择有效扩展，使用核心模式"
    else
        ENABLED_EXTENSIONS="$(
            IFS=,
            echo "${selected[*]}"
        )"
        log_info "已选择扩展: $ENABLED_EXTENSIONS"
    fi
}

show_feature_summary() {
    echo ""
    echo -e "${BOLD}部署功能配置:${NC}"
    if [ "$ENABLED_EXTENSIONS" = "all" ]; then
        echo -e "  模式: ${GREEN}全部功能${NC} (core + all extensions)"
    elif [ "$ENABLED_EXTENSIONS" = "$CORE_PRIVATE_CHAT_EXTENSIONS" ]; then
        echo -e "  模式: ${GREEN}核心私密聊天${NC} (friends + burn-after-read)"
    elif [ "$ENABLED_EXTENSIONS" = "none" ]; then
        echo -e "  模式: ${YELLOW}仅核心${NC} (pure Matrix homeserver)"
    else
        echo -e "  模式: ${CYAN}自定义${NC}"
        echo -e "  扩展: ${ENABLED_EXTENSIONS}"
    fi
    echo ""
}

setup_logging() {
    mkdir -p "$LOG_DIR"
    touch "$LOG_FILE"

    # Avoid bash process substitution so the script can run inside restricted sandboxes.
    LOG_PIPE="$LOG_DIR/.deploy_${TIMESTAMP}.pipe"
    rm -f "$LOG_PIPE"

    if mkfifo "$LOG_PIPE"; then
        exec 3>&1 4>&2
        tee -a "$LOG_FILE" <"$LOG_PIPE" >&3 2>&4 &
        LOG_TEE_PID="$!"
        exec >"$LOG_PIPE" 2>&1
    else
        echo "无法创建日志管道，回退为仅写入日志文件: $LOG_FILE"
        exec >>"$LOG_FILE" 2>&1
    fi
}

cleanup_logging() {
    if [ -n "$LOG_PIPE" ]; then
        exec 1>&3 2>&4 || true
        exec 3>&- 4>&- || true
        rm -f "$LOG_PIPE" || true
        if [ -n "$LOG_TEE_PID" ]; then
            wait "$LOG_TEE_PID" 2>/dev/null || true
        fi
    fi
}

on_error() {
    local line_no="$1"
    local exit_code="${2:-1}"
    log_error "部署失败: phase=${DEPLOYMENT_PHASE}, line=${line_no}, exit_code=${exit_code}"
    if [ "$ROLLBACK_ENABLED" = "true" ] && [ "$ROLLBACK_IN_PROGRESS" = "false" ]; then
        rollback_deployment || true
    fi
    exit "$exit_code"
}

trap 'on_error "$LINENO" "$?"' ERR
trap cleanup_logging EXIT

show_banner() {
    echo ""
    echo "=========================================="
    echo "  synapse-rust Docker 重建部署脚本"
    echo "=========================================="
    echo "  日志文件: $LOG_FILE"
    echo ""
}

retry() {
    local attempts="$1"
    local delay="$2"
    shift 2

    local try=1
    until "$@"; do
        if [ "$try" -ge "$attempts" ]; then
            return 1
        fi
        log_warning "命令失败，${delay}s 后进行第 $((try + 1))/$attempts 次重试: $*"
        sleep "$delay"
        try=$((try + 1))
    done
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || {
        log_error "缺少依赖命令: $1"
        exit 1
    }
}

load_env() {
    local cli_extensions="$ENABLED_EXTENSIONS"

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

    ROLLBACK_ENABLED="${ROLLBACK_ON_FAILURE:-true}"
    # CLI args take precedence over .env value
    if [ -n "$cli_extensions" ]; then
        ENABLED_EXTENSIONS="$cli_extensions"
    fi
}

local_image_ref() {
    echo "${SYNAPSE_IMAGE:-synapse-rust:local}"
}

docker_feature_args() {
    if [ "$ENABLED_EXTENSIONS" = "all" ]; then
        echo "--features all-extensions"
    elif [ "$ENABLED_EXTENSIONS" = "none" ]; then
        echo "--features server --no-default-features"
    elif [ "$ENABLED_EXTENSIONS" = "$CORE_PRIVATE_CHAT_EXTENSIONS" ]; then
        echo "--features server,core-private-chat --no-default-features"
    else
        echo "--features server,$ENABLED_EXTENSIONS --no-default-features"
    fi
}

is_placeholder() {
    local value="${1:-}"
    [ -z "$value" ] || [[ "$value" == __REQUIRED_* ]] || [[ "$value" == *"your-"* ]] || [[ "$value" == *"change-me"* ]]
}

check_dependencies() {
    DEPLOYMENT_PHASE="dependency-check"
    log_info "检查依赖..."

    require_command docker
    require_command curl
    require_command tar
    require_command awk
    require_command grep
    require_command openssl

    if ! command -v docker-compose >/dev/null 2>&1 && ! docker compose version >/dev/null 2>&1; then
        log_error "缺少 Docker Compose"
        exit 1
    fi

    docker info >/dev/null
    log_success "依赖检查通过"
}

# =============================================================================
# 依赖自动安装 (--install-deps)
# =============================================================================
install_missing_deps() {
    DEPLOYMENT_PHASE="dependency-install"
    log_info "检查并安装缺失依赖..."

    local missing=()
    command -v docker >/dev/null 2>&1 || missing+=(docker)
    command -v curl >/dev/null 2>&1 || missing+=(curl)
    command -v openssl >/dev/null 2>&1 || missing+=(openssl)
    command -v mkcert >/dev/null 2>&1 || missing+=(mkcert)
    if ! command -v docker-compose >/dev/null 2>&1 && ! docker compose version >/dev/null 2>&1; then
        missing+=(docker-compose)
    fi

    if [ ${#missing[@]} -eq 0 ]; then
        log_success "所有依赖已就绪"
        return
    fi

    log_warning "缺少依赖: ${missing[*]}"

    if [ "$(uname -s)" = "Darwin" ]; then
        if command -v brew >/dev/null 2>&1; then
            log_info "使用 Homebrew 安装: ${missing[*]}"
            brew install "${missing[@]}"
        else
            log_error "未找到 Homebrew，请先安装: https://brew.sh"
            exit 1
        fi
    elif command -v apt-get >/dev/null 2>&1; then
        log_info "使用 apt 安装: ${missing[*]}"
        sudo apt-get update
        sudo apt-get install -y "${missing[@]//docker-compose/docker-compose-v2}"
    elif command -v yum >/dev/null 2>&1; then
        log_info "使用 yum 安装: ${missing[*]}"
        sudo yum install -y "${missing[@]}"
    else
        log_error "无法自动安装依赖，请手动安装: ${missing[*]}"
        exit 1
    fi

    require_command docker
    command -v docker-compose >/dev/null 2>&1 || docker compose version >/dev/null 2>&1 || {
        log_error "Docker Compose 安装失败"
        exit 1
    }
    log_success "依赖安装完成"
}

# =============================================================================
# SSL 证书检查与自动生成 (matrix.test)
# 优先使用 mkcert（生成受本机信任的 CA 证书），回退到 openssl 自签名。
# =============================================================================
ensure_ssl_certs() {
    DEPLOYMENT_PHASE="ssl-certs"
    log_info "检查 SSL 证书..."

    local cert_file="ssl/${SSL_CERT:-cert.pem}"
    local key_file="ssl/${SSL_KEY:-key.pem}"
    local server_name="${SERVER_NAME:-matrix.test}"

    cert_valid_for_server() {
        openssl x509 -in "$cert_file" -noout -text 2>/dev/null |
            grep -A2 "Subject Alternative Name" |
            grep -q "$server_name"
    }

    cert_is_self_signed() {
        local issuer subject
        issuer="$(openssl x509 -in "$cert_file" -noout -issuer 2>/dev/null)"
        subject="$(openssl x509 -in "$cert_file" -noout -subject 2>/dev/null)"
        [ -n "$issuer" ] && [ "$issuer" = "$subject" ]
    }

    if [ -f "$cert_file" ] && [ -f "$key_file" ] && cert_valid_for_server; then
        if command -v mkcert >/dev/null 2>&1 && cert_is_self_signed; then
            log_warning "检测到自签名证书（浏览器不信任），将用 mkcert 重新签发..."
        else
            log_success "SSL 证书已存在且匹配域名 $server_name"
            return
        fi
    fi

    if [ -f "$cert_file" ] || [ -f "$key_file" ]; then
        log_warning "SSL 证书缺失或不匹配域名 $server_name，重新生成..."
    fi
    mkdir -p ssl

    if command -v mkcert >/dev/null 2>&1; then
        log_info "使用 mkcert 生成证书 ($server_name, localhost, 127.0.0.1)..."
        # mkcert 首次使用需安装本地 CA 到系统信任库
        if [ ! -f "$(mkcert -CAROOT 2>/dev/null)/rootCA.pem" ]; then
            mkcert -install >/dev/null 2>&1 ||
                log_warning "mkcert CA 安装失败，证书将不被系统浏览器信任（curl -k 仍可访问）"
        fi
        mkcert -cert-file "$cert_file" -key-file "$key_file" \
            "$server_name" localhost 127.0.0.1
    else
        log_warning "未找到 mkcert，使用 openssl 生成自签名证书（需手动信任）..."
        openssl req -x509 -newkey rsa:2048 -nodes \
            -keyout "$key_file" -out "$cert_file" -days 3650 \
            -subj "/CN=$server_name" \
            -addext "subjectAltName=DNS:$server_name,DNS:localhost,IP:127.0.0.1" \
            >/dev/null 2>&1
    fi

    [ -f "$cert_file" ] && [ -f "$key_file" ] || {
        log_error "SSL 证书生成失败: $cert_file / $key_file"
        exit 1
    }
    chmod 600 "$key_file"
    log_success "SSL 证书已生成: $cert_file / $key_file"
}

# =============================================================================
# /etc/hosts 域名映射检查 (matrix.test -> 127.0.0.1)
# =============================================================================
ensure_hosts_entry() {
    DEPLOYMENT_PHASE="hosts-check"
    log_info "检查 /etc/hosts 域名映射..."
    local server_name="${SERVER_NAME:-matrix.test}"

    if grep -Eq "(^|[[:space:]])127\.0\.0\.1([[:space:]]+.*)?${server_name}\b" /etc/hosts 2>/dev/null; then
        log_success "/etc/hosts 已包含: 127.0.0.1 $server_name"
        return
    fi

    log_warning "/etc/hosts 缺少域名映射: 127.0.0.1 $server_name"
    if [ "$(id -u)" = "0" ]; then
        echo "127.0.0.1 $server_name" >>/etc/hosts
        log_success "已自动添加 /etc/hosts 条目"
    else
        log_warning "请手动执行以下命令（HTTPS 域名解析必需）:"
        log_warning "  sudo sh -c 'echo \"127.0.0.1 $server_name\" >> /etc/hosts'"
    fi
}

# =============================================================================
# 本地 coturn TURN 服务检查与启动
# 连接 /Users/ljf/Desktop/hu_ts/coturn 的本地 coturn（容器化），确保:
#   1. coturn 容器运行中（未运行则自动 docker compose up -d 启动）
#   2. TURN 共享密钥与 homeserver 配置一致
# =============================================================================
COTURN_DIR="${COTURN_DIR:-/Users/ljf/Desktop/hu_ts/coturn}"

check_local_turn() {
    DEPLOYMENT_PHASE="turn-check"
    if [ "$CHECK_TURN" != "true" ]; then
        log_info "跳过 TURN 检查 (--no-turn)"
        return
    fi
    log_info "检查本地 coturn TURN 服务..."

    local turn_ok=false
    local turn_host="${TURN_HOST:-127.0.0.1}"
    local turn_port="${TURN_PORT:-3478}"

    # 1) 检查 coturn 容器
    if docker ps --format '{{.Names}}' 2>/dev/null | grep -qx 'coturn'; then
        turn_ok=true
        log_success "coturn 容器运行中"
    fi

    # 2) 检查端口连通性（容器未命名 coturn 时的兜底）
    if ! $turn_ok; then
        if nc -z -w 2 "$turn_host" "$turn_port" >/dev/null 2>&1; then
            turn_ok=true
            log_success "coturn 端口可达: ${turn_host}:${turn_port}"
        fi
    fi

    # 3) 未运行则自动启动
    if ! $turn_ok; then
        if [ -d "$COTURN_DIR" ] && [ -f "$COTURN_DIR/docker-compose.yml" ]; then
            log_warning "coturn 未运行，尝试启动 ($COTURN_DIR)..."
            if (cd "$COTURN_DIR" && docker compose up -d); then
                sleep 3
                if nc -z -w 2 "$turn_host" "$turn_port" >/dev/null 2>&1 ||
                    docker ps --format '{{.Names}}' 2>/dev/null | grep -qx 'coturn'; then
                    turn_ok=true
                    log_success "coturn 已启动"
                else
                    log_error "coturn 启动后端口仍不可达: ${turn_host}:${turn_port}"
                    log_error "请检查: cd $COTURN_DIR && docker compose logs coturn"
                fi
            else
                log_error "coturn 启动失败，请手动检查: cd $COTURN_DIR && docker compose up -d"
            fi
        else
            log_warning "未找到 coturn 配置目录: $COTURN_DIR (可用 --no-turn 跳过)"
        fi
    fi

    # 4) 校验共享密钥一致性
    if $turn_ok; then
        local coturn_secret=""
        if docker ps --format '{{.Names}}' 2>/dev/null | grep -qx 'coturn'; then
            coturn_secret="$(
                docker exec coturn sh -c '
                    grep -h "static-auth-secret" /etc/coturn/turnserver.conf 2>/dev/null |
                    grep -v "^#" | head -1 | awk -F= "{gsub(/[ \t\r]/,\"\",\$2); print \$2}"
                ' 2>/dev/null || true
            )"
        fi
        if [ -z "$coturn_secret" ] && [ -f "$COTURN_DIR/turnserver.conf" ]; then
            coturn_secret="$(
                grep -h "static-auth-secret" "$COTURN_DIR/turnserver.conf" 2>/dev/null |
                grep -v "^#" | head -1 | awk -F= '{gsub(/[ \t\r]/,"",$2); print $2}'
            )"
        fi

        local synapse_secret="${TURN_SHARED_SECRET:-dev-turn-secret}"
        if [ -n "$coturn_secret" ] && [ "$coturn_secret" != "$synapse_secret" ]; then
            log_warning "TURN 共享密钥不一致: coturn='$coturn_secret' vs synapse='$synapse_secret'"
            log_warning "请修改 .env 中的 TURN_SHARED_SECRET 为 '$coturn_secret'"
        else
            log_success "TURN 共享密钥一致: $synapse_secret"
        fi
    else
        log_warning "coturn 不可用，VoIP 通话功能将不可用（不影响其他服务）"
    fi
}

# =============================================================================
# HTTPS 端点验证 (https://matrix.test)
# =============================================================================
verify_https_endpoints() {
    DEPLOYMENT_PHASE="verify-https"
    log_info "验证 HTTPS 端点 (${SERVER_NAME:-matrix.test})..."

    local https_port="${HTTPS_PORT:-443}"
    local base="https://${SERVER_NAME:-matrix.test}"
    [ "$https_port" != "443" ] && base="$base:$https_port"

    curl -kfsS "$base/health" >/dev/null ||
        { log_error "HTTPS 健康检查失败: $base/health"; return 1; }
    curl -kfsS "$base/_matrix/client/versions" >/dev/null ||
        { log_error "HTTPS API 检查失败: $base/_matrix/client/versions"; return 1; }
    curl -kfsS "$base/.well-known/matrix/server" >/dev/null ||
        { log_warning "HTTPS .well-known/matrix/server 检查失败（不影响核心功能）"; }

    log_success "HTTPS 验证通过: $base"
}

check_env_file() {
    DEPLOYMENT_PHASE="environment-check"
    log_info "检查环境变量配置..."

    if [ ! -f ".env" ]; then
        cp .env.example .env
        log_warning "已从 .env.example 创建 .env"
    fi

    chmod +x scripts/generate-secrets.sh
    ./scripts/generate-secrets.sh missing >/dev/null

    load_env

    local required_vars=(
        SERVER_NAME
        PUBLIC_BASEURL
        POSTGRES_PASSWORD
        REDIS_PASSWORD
        ADMIN_SHARED_SECRET
        JWT_SECRET
        REGISTRATION_SHARED_SECRET
        SECRET_KEY
        MACAROON_SECRET
        FORM_SECRET
    )
    local missing_vars=()
    local var

    for var in "${required_vars[@]}"; do
        if is_placeholder "${!var:-}"; then
            missing_vars+=("$var")
        fi
    done

    if [ ${#missing_vars[@]} -ne 0 ]; then
        log_error "以下环境变量需要配置: ${missing_vars[*]}"
        exit 1
    fi

    if [ "${ENABLE_SSL:-false}" = "true" ]; then
        [ -f "ssl/${SSL_CERT:-cert.pem}" ] || {
            log_error "启用 SSL 时必须提供证书: ssl/${SSL_CERT:-cert.pem}"
            exit 1
        }
        [ -f "ssl/${SSL_KEY:-key.pem}" ] || {
            log_error "启用 SSL 时必须提供私钥: ssl/${SSL_KEY:-key.pem}"
            exit 1
        }
    fi

    compose config >/dev/null
    log_success "环境变量配置检查通过"
}

create_directories() {
    DEPLOYMENT_PHASE="directory-setup"
    log_info "创建必要目录..."
    mkdir -p ssl media logs backups config
    [ -d migrations ] || {
        log_error "migrations 目录不存在"
        exit 1
    }
    [ -f config/homeserver.yaml ] || {
        log_error "缺少配置文件: config/homeserver.yaml"
        exit 1
    }
    [ -f config/rate_limit.yaml ] || {
        log_error "缺少配置文件: config/rate_limit.yaml"
        exit 1
    }
    [ -f config/postgres.conf ] || {
        log_error "缺少配置文件: config/postgres.conf"
        exit 1
    }
    log_success "目录与配置文件检查完成"
}

backup_current_state() {
    DEPLOYMENT_PHASE="backup"
    log_info "为回滚创建备份..."

    local current_image
    current_image="$(local_image_ref)"
    if docker image inspect "$current_image" >/dev/null 2>&1; then
        # 清理历史 rollback 标签：每次部署仅保留即将创建的最新一个，避免
        # rollback-* 镜像无限累积（每个约 200MB）。仅按 tag 名删除，不会误删
        # 当前 ${current_image} 指向的镜像。
        local old_rollback
        for old_rollback in $(docker images --format '{{.Repository}}:{{.Tag}}' 2>/dev/null | grep ':rollback-' || true); do
            docker rmi -f "$old_rollback" >/dev/null 2>&1 || true
        done
        ROLLBACK_IMAGE_TAG="${current_image%:*}:rollback-${TIMESTAMP}"
        docker tag "$current_image" "$ROLLBACK_IMAGE_TAG"
        log_info "已保存旧镜像标签: $ROLLBACK_IMAGE_TAG"
    fi

    if compose ps --status running 2>/dev/null | grep -Eq 'postgres|redis|synapse|nginx'; then
        chmod +x scripts/backup.sh
        local backup_output
        backup_output="$(./scripts/backup.sh)"
        echo "$backup_output"
        ROLLBACK_BACKUP="$(echo "$backup_output" | awk -F': ' '/备份文件:/ {print $2}' | tail -n 1)"
        if [ -n "$ROLLBACK_BACKUP" ] && [ -f "$ROLLBACK_BACKUP" ]; then
            log_success "已创建回滚备份: $ROLLBACK_BACKUP"
        fi
    else
        log_info "未检测到运行中的旧部署，跳过数据备份"
    fi
}

clear_project_caches() {
    DEPLOYMENT_PHASE="cache-clean"
    if [ "$SKIP_BUILD" = "true" ]; then
        log_info "跳过缓存清理 (--skip-build)"
        return
    fi
    log_info "清理项目缓存与 Docker 构建缓存..."

    (cd "$PROJECT_ROOT" && cargo clean)
    # 注意：不清理 npm/yarn/pnpm 全局缓存——本项目为 Rust 后端，这些 JS 包管理器
    # 缓存与构建无关，且 `pnpm store prune` 会触发环境 safe-delete hook
    # （删除 ~/.cache 下大量文件），故明确移除，仅清理项目级与 Docker 缓存。

    docker builder prune -af >/dev/null
    docker buildx prune -af >/dev/null 2>&1 || true

    log_success "缓存清理完成"
}

rebuild_project() {
    DEPLOYMENT_PHASE="project-build"
    if [ "$SKIP_BUILD" = "true" ]; then
        log_info "跳过项目编译 (--skip-build)"
        return
    fi
    # Docker 构建已在 builder 阶段完成 cargo build --release，主机编译为冗余步骤。
    # 设置 SKIP_HOST_BUILD=true（默认）可跳过主机编译，节省 15-30 分钟。
    # 如需在 Docker 构建前做编译预检，设置 SKIP_HOST_BUILD=false。
    if [ "${SKIP_HOST_BUILD:-true}" = "true" ]; then
        log_info "跳过主机编译 (Docker 构建阶段已含编译，SKIP_HOST_BUILD=true)"
        return
    fi
    log_info "重新编译项目 (主机预检)..."
    if [ "$ENABLED_EXTENSIONS" = "all" ]; then
        (cd "$PROJECT_ROOT" && cargo build --release --locked --features all-extensions --bin synapse-rust --bin healthcheck)
    elif [ "$ENABLED_EXTENSIONS" = "none" ]; then
        (cd "$PROJECT_ROOT" && cargo build --release --locked --no-default-features --features server --bin synapse-rust --bin healthcheck)
    elif [ "$ENABLED_EXTENSIONS" = "$CORE_PRIVATE_CHAT_EXTENSIONS" ]; then
        (cd "$PROJECT_ROOT" && cargo build --release --locked --no-default-features --features server,core-private-chat --bin synapse-rust --bin healthcheck)
    else
        (cd "$PROJECT_ROOT" && cargo build --release --locked --no-default-features --features "server,$ENABLED_EXTENSIONS" --bin synapse-rust --bin healthcheck)
    fi
    log_success "项目编译完成"
}

remove_existing_deployment() {
    DEPLOYMENT_PHASE="remove-old-deployment"
    log_info "停止并删除旧容器与关联镜像..."

    compose down --remove-orphans || true
    docker rm -f synapse-postgres synapse-redis synapse-migrator synapse-app synapse-nginx >/dev/null 2>&1 || true
    if [ "$USE_REMOTE_IMAGE" != "true" ] && [ "$SKIP_BUILD" != "true" ]; then
        docker image rm -f "$(local_image_ref)" synapse-rust-tools:local >/dev/null 2>&1 || true
    fi

    log_success "旧部署资源清理完成"
}

build_images() {
    DEPLOYMENT_PHASE="docker-build"
    if [ "$USE_REMOTE_IMAGE" = "true" ]; then
        log_info "拉取远程镜像: $REMOTE_IMAGE"
        retry 3 5 docker pull "$REMOTE_IMAGE"
        export SYNAPSE_IMAGE="$REMOTE_IMAGE"
        export SYNAPSE_PULL_POLICY=missing
        docker image inspect "$REMOTE_IMAGE" >/dev/null
        log_success "远程镜像就绪: $REMOTE_IMAGE"
        return
    fi
    if [ "$SKIP_BUILD" = "true" ]; then
        log_info "跳过 Docker 镜像构建 (--skip-build)"
        if ! docker image inspect "$(local_image_ref)" >/dev/null 2>&1; then
            log_error "跳过构建但本地镜像 $(local_image_ref) 不存在"
            exit 1
        fi
        return
    fi
    log_info "构建新的 Docker 镜像..."
    local feature_args
    feature_args="$(docker_feature_args)"
    # 覆盖基础镜像 digest pin，使用本地已拉取的 tag 版本，避免网络抖动导致 digest 拉取失败
    docker build --no-cache \
        -f "$PROJECT_ROOT/docker/Dockerfile" \
        --target tools \
        --build-arg "CARGO_FEATURE_ARGS=${feature_args}" \
        --build-arg "RUST_BUILDER_IMAGE=rust:1.93.0-slim-bookworm" \
        --build-arg "DEBIAN_BASE_IMAGE=debian:bookworm-slim" \
        -t "$(local_image_ref)" \
        "$PROJECT_ROOT"
    docker image inspect "$(local_image_ref)" >/dev/null
    log_success "Docker 镜像构建完成"
}

wait_for_container_health() {
    local container_name="$1"
    local max_retries="${2:-30}"
    local delay="${3:-5}"
    local attempt=1
    local status

    while [ "$attempt" -le "$max_retries" ]; do
        status="$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "$container_name" 2>/dev/null || true)"
        case "$status" in
            healthy | running)
                log_success "$container_name 状态正常: $status"
                return 0
                ;;
            unhealthy | exited | dead)
                log_error "$container_name 状态异常: $status"
                docker logs "$container_name" --tail 200 || true
                return 1
                ;;
        esac
        log_info "等待 $container_name 就绪... ($attempt/$max_retries, 当前: ${status:-unknown})"
        sleep "$delay"
        attempt=$((attempt + 1))
    done

    log_error "$container_name 在限定时间内未就绪"
    docker logs "$container_name" --tail 200 || true
    return 1
}

run_migrations() {
    DEPLOYMENT_PHASE="database-migrate"
    log_info "执行数据库迁移 (ENABLED_EXTENSIONS=$ENABLED_EXTENSIONS)..."
    retry 3 5 compose run -T --rm --no-deps -e "ENABLED_EXTENSIONS=${ENABLED_EXTENSIONS}" migrator migrate < /dev/null
    log_success "数据库迁移完成"
    log_info "验证数据库架构完整性..."
    retry 3 5 compose run -T --rm --no-deps -e "ENABLED_EXTENSIONS=${ENABLED_EXTENSIONS}" migrator validate < /dev/null
    log_success "数据库架构验证通过"
}

start_services() {
    DEPLOYMENT_PHASE="service-start"
    log_info "启动基础服务..."

    compose up -d postgres redis
    wait_for_container_health synapse-postgres "${HEALTHCHECK_RETRIES:-30}" "${HEALTHCHECK_INTERVAL:-5}"
    wait_for_container_health synapse-redis "${HEALTHCHECK_RETRIES:-30}" "${HEALTHCHECK_INTERVAL:-5}"

    run_migrations

    log_info "启动 Synapse 应用..."
    compose up -d synapse
    wait_for_container_health synapse-app "${HEALTHCHECK_RETRIES:-30}" "${HEALTHCHECK_INTERVAL:-5}"

    log_info "启动 Nginx..."
    compose up -d nginx
    wait_for_container_health synapse-nginx "${HEALTHCHECK_RETRIES:-30}" "${HEALTHCHECK_INTERVAL:-5}"

    log_success "所有服务已启动"
}

verify_database() {
    DEPLOYMENT_PHASE="verify-database"
    log_info "验证数据库连接..."
    compose exec -T postgres psql -U "${POSTGRES_USER:-postgres}" -d "${POSTGRES_DB:-synapse}" -c "SELECT 1;" >/dev/null
    log_success "数据库连接正常"
}

verify_health_endpoints() {
    DEPLOYMENT_PHASE="verify-health"
    log_info "验证健康检查接口..."

    curl -fsS "http://localhost:${SYNAPSE_PORT:-8008}/health" >/dev/null
    curl -fsS "http://localhost:${HTTP_PORT:-80}/health" >/dev/null
    curl -fsS "http://localhost:${SYNAPSE_PORT:-8008}/_matrix/client/versions" >/dev/null

    log_success "健康检查与 API 基础接口验证通过"
}

verify_logs_clean() {
    DEPLOYMENT_PHASE="verify-logs"
    log_info "检查容器日志中是否存在异常 ERROR，WARNING 仅做提示..."

    local log_dump
    log_dump="$(compose logs --no-color --tail=400 2>&1 || true)"
    local errors
    errors="$(echo "$log_dump" | grep -Ei '\b(ERROR)\b' |
        grep -v 'no usable system locales' |
        grep -v 'enabling "trust" authentication' |
        grep -v 'Missing indexes' |
        grep -v 'DOCKER_INSECURE_NO_IPTABLES_RAW' |
        grep -v 'forcibly turning on oci-mediatype' |
        grep -v '_sqlx_migrations' ||
        true)"
    if [ -n "$errors" ]; then
        log_error "检测到 ERROR 日志:"
        echo "$errors"
        return 1
    fi

    local warnings
    warnings="$(echo "$log_dump" | grep -Ei '\bWARN(ING)?\b' |
        grep -v 'no usable system locales' |
        grep -v 'enabling "trust" authentication' |
        grep -v 'Missing indexes' |
        grep -v 'DOCKER_INSECURE_NO_IPTABLES_RAW' |
        grep -v 'forcibly turning on oci-mediatype' |
        grep -v '_sqlx_migrations' ||
        true)"
    if [ -n "$warnings" ]; then
        log_warning "检测到 WARNING 日志（不阻断部署）:"
        echo "$warnings"
    else
        log_success "未发现 ERROR/WARNING 级别日志"
    fi
}

show_status() {
    echo ""
    log_info "服务状态:"
    compose ps
    echo ""
}

rollback_deployment() {
    DEPLOYMENT_PHASE="rollback"
    ROLLBACK_IN_PROGRESS=true
    log_warning "开始执行回滚..."

    compose down --remove-orphans >/dev/null 2>&1 || true

    if [ -n "$ROLLBACK_IMAGE_TAG" ] && docker image inspect "$ROLLBACK_IMAGE_TAG" >/dev/null 2>&1; then
        docker tag "$ROLLBACK_IMAGE_TAG" "$(local_image_ref)" || true
    fi

    if [ -n "$ROLLBACK_BACKUP" ] && [ -f "$ROLLBACK_BACKUP" ]; then
        chmod +x scripts/restore.sh
        RESTORE_FORCE=true ./scripts/restore.sh "$ROLLBACK_BACKUP" || true
        log_warning "已尝试恢复到备份状态"
    else
        log_warning "没有可用备份，跳过数据回滚"
    fi
}

show_access_info() {
    local https_port="${HTTPS_PORT:-443}"
    local https_base="https://${SERVER_NAME:-matrix.test}"
    [ "$https_port" != "443" ] && https_base="$https_base:$https_port"

    echo ""
    echo "=========================================="
    echo "  部署完成"
    echo "=========================================="
    echo "服务器名称: ${SERVER_NAME}"
    echo "公开 URL: ${PUBLIC_BASEURL}"
    echo "HTTPS 客户端:   ${https_base}"
    echo "HTTPS 联邦:     ${https_base}:${FEDERATION_PORT:-8448}"
    echo "HTTPS 健康检查: ${https_base}/health"
    echo "HTTP 健康检查:  http://localhost:${HTTP_PORT:-80}/health"
    echo "应用健康检查:   http://localhost:${SYNAPSE_PORT:-8008}/health"
    echo "TURN 服务:      ${TURN_HOST:-127.0.0.1}:${TURN_PORT:-3478} (coturn, secret=${TURN_SHARED_SECRET:-dev-turn-secret})"
    echo "部署日志:       ${LOG_FILE}"
    echo "扩展功能:       ${ENABLED_EXTENSIONS}"
    echo ""
}

main() {
    parse_args "$@"
    setup_logging
    show_banner
    check_dependencies
    if [ "$INSTALL_DEPS" = "true" ]; then
        install_missing_deps
    fi
    check_env_file
    select_features
    show_feature_summary
    create_directories
    ensure_ssl_certs
    ensure_hosts_entry
    check_local_turn
    backup_current_state
    clear_project_caches
    rebuild_project
    remove_existing_deployment
    build_images
    start_services
    verify_database
    verify_health_endpoints
    verify_https_endpoints
    verify_logs_clean
    show_status
    show_access_info
    log_success "重建、优化部署与验证全部完成"
}

main "$@"
