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
#   ./deploy.sh --no-monitoring   # 跳过监控栈启动（prometheus/grafana/...）
#   ./deploy.sh --image REF       # 使用指定的远程镜像（跳过本地构建，自动 pull）
#   ./deploy.sh --keep-images     # 保留历史项目镜像（默认全量删除旧镜像）
#   ./deploy.sh --no-strict-warnings # 未知 WARNING 不阻断部署（默认阻断）
#   ./deploy.sh --no-rollback     # 失败时不自动回滚（默认自动回滚）
#   ./deploy.sh --stop-timeout 30 # 容器优雅停止(SIGTERM)等待秒数，默认 30
#
# 完整流程: 环境检查 → 依赖安装(可选) → 配置检查 → 功能选择 → 目录准备 →
#           SSL 证书自动生成 → 应用数据密钥( megolm.key )准备 → /etc/hosts 检查 →
#           本地 coturn 检查/启动 →
#           部署前备份 → 缓存清理 → 项目编译 → 容器优雅停止 →
#           移除旧部署 → 旧镜像清理 → 镜像构建 → 服务启动(含迁移) →
#           数据库连接验证 → DB 版本一致性校验 → 健康/HTTPS 验证 →
#           日志告警分析 → 监控栈启动 → 状态与访问信息
#
# 可靠性: 全部步骤经 run_step 包装，失败时打印失败的步骤/命令/行号/退出码，
#         并按 ROLLBACK_ENABLED 自动回滚（恢复旧镜像标签 + 还原数据库备份
#         + 以旧镜像重启服务）；任一步骤失败即停止后续步骤，避免半成品部署。
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
# 监控栈（prometheus/alertmanager/grafana/node-exporter/alert-handler）随部署启动。
# 它是**非致命**步骤：失败只告警，不触发核心栈回滚（监控不在对外服务关键路径上）。
CHECK_MONITORING="${CHECK_MONITORING:-true}"
# 旧项目镜像清理：默认全量删除（保留当前镜像与本次回滚标签）
KEEP_IMAGES=false
# 未知 WARNING 是否阻断部署：默认阻断（部署门禁要求"日志无未知告警"）
STRICT_WARNINGS="${STRICT_WARNINGS:-true}"
# 容器优雅停止（SIGTERM）等待秒数
STOP_TIMEOUT="${STOP_TIMEOUT:-30}"
# 表数漂移容忍阈值，与 schema_health_check 的 DB-02 检查保持一致(±10)
DB_DRIFT_TOLERANCE="${DB_DRIFT_TOLERANCE:-10}"
# 当前步骤与已完成步骤（用于失败上下文与回滚决策）
CURRENT_STEP=""
COMPLETED_STEPS=()

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
            --no-monitoring)
                CHECK_MONITORING=false
                ;;
            --keep-images)
                KEEP_IMAGES=true
                ;;
            --no-strict-warnings)
                STRICT_WARNINGS=false
                ;;
            --strict-warnings)
                STRICT_WARNINGS=true
                ;;
            --no-rollback)
                ROLLBACK_ENABLED=false
                ;;
            --stop-timeout)
                shift
                STOP_TIMEOUT="${1:?'--stop-timeout 需要参数，如: 30'}"
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
  --no-monitoring   跳过监控栈启动（prometheus/alertmanager/grafana/node-exporter/alert-handler）
  --image REF       使用指定的远程镜像（自动 docker pull，跳过本地构建）
  --keep-images     保留历史项目镜像（默认删除所有旧项目镜像以释放空间）
  --no-strict-warnings 未知 WARNING 仅提示、不阻断部署（默认阻断）
  --strict-warnings 未知 WARNING 阻断部署（默认行为）
  --no-rollback     失败时不自动回滚（默认自动回滚）
  --stop-timeout N  容器优雅停止(SIGTERM)等待秒数，默认 30
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
    local failed_step="${CURRENT_STEP:-$DEPLOYMENT_PHASE}"
    local done_steps="无"
    if [ ${#COMPLETED_STEPS[@]} -gt 0 ]; then
        done_steps="${COMPLETED_STEPS[*]}"
    fi

    log_error "=========================================================="
    log_error "部署失败"
    log_error "  失败步骤 : ${failed_step}"
    log_error "  失败命令 : ${BASH_COMMAND:-<unknown>}"
    log_error "  脚本行号 : ${line_no}"
    log_error "  退出码   : ${exit_code}"
    log_error "  已完成步骤(共 ${#COMPLETED_STEPS[@]}): ${done_steps}"
    log_error "  完整日志 : ${LOG_FILE}"
    log_error "=========================================================="

    if [ "$ROLLBACK_ENABLED" = "true" ] && [ "$ROLLBACK_IN_PROGRESS" = "false" ]; then
        rollback_deployment || true
    else
        log_warning "未执行自动回滚（ROLLBACK_ENABLED=${ROLLBACK_ENABLED}）；排障指引:"
        log_warning "  1) 查看失败步骤日志: $LOG_FILE"
        log_warning "  2) 当前容器状态: docker compose ps"
        log_warning "  3) 重新部署: ./deploy.sh --all"
    fi
    exit "$exit_code"
}

# 统一的步骤执行包装：记录步骤边界与进度，失败时由 ERR trap 统一报告与回滚。
# 关键约束：目标函数必须"直接调用"，绝不能放进 if/&&/|| 等条件表达式中——
# 一旦放进条件，`set -e` 会在被调用函数内部被整体禁用，函数内部的任意子命令
# 失败都将被静默忽略（部署脚本漏报失败的典型根因）。直接调用可确保任意子命令
# 失败立即触发 ERR trap -> on_error，并携带 CURRENT_STEP 上下文。
run_step() {
    local name="$1"
    shift
    CURRENT_STEP="$name"
    DEPLOYMENT_PHASE="$name"
    log_info "▶ 步骤: ${name}"

    local start_ts
    start_ts="$(date +%s)"

    "$@"

    COMPLETED_STEPS+=("$name")
    log_success "✔ 步骤完成: ${name} ($(($(date +%s) - start_ts))s)"
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
        echo "--no-default-features"
    elif [ "$ENABLED_EXTENSIONS" = "$CORE_PRIVATE_CHAT_EXTENSIONS" ]; then
        echo "--features core-private-chat --no-default-features"
    else
        echo "--features $ENABLED_EXTENSIONS --no-default-features"
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
        log_warning "SSL 证书缺失或不匹配域名 ${server_name}，重新生成..."
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
# /app/data 密钥供给 (megolm.key)
# =============================================================================
# SYNAPSE__SERVER__MEGOLM_ENCRYPTION_KEY_PATH 指向 /app/data/megolm.key，
# 它是服务端 megolm 会话的静态加密密钥。`resolve_at_rest_key` 是 fail-closed 的：
# 路径被配置而文件缺失 / 非法 base64 / 解码后不是 32 字节，服务直接拒绝启动。
#
# 而 distroless 镜像里没有 shell，应用自己无从在空目录/空卷里落盘首把密钥 ——
# 供给只能发生在部署脚本侧。2026-09-21 实测：干净命名卷首次部署时 synapse-app
# 启动即 panic（`Failed to read key file /app/data/megolm.key`），崩溃循环 exit 133。
#
# 本步骤保证：
#   1. 宿主机 $SYNAPSE_DATA_DIR/megolm.key 存在且可用（缺失则生成）；
#   2. **绝不覆盖**已存在的合法密钥（覆盖等于让已入库的密文永久不可解）；
#   3. 现存的密钥若不可用则 fail-closed 报错，而不是悄悄换一把新的。
SYNAPSE_DATA_DIR="${SYNAPSE_DATA_DIR:-synapse-data}"

ensure_app_data_keys() {
    DEPLOYMENT_PHASE="app-data-keys"
    log_info "检查 /app/data 密钥（megolm.key）..."

    local data_dir="$SYNAPSE_DATA_DIR"
    local key_file="${data_dir}/megolm.key"

    # 遗留命名卷守卫：改绑宿主机目录后，旧命名卷里的 megolm.key 会被"看不见"。
    # 静默生成新钥 → 旧密文永久不可解，所以这里 fail-closed，并给出搬运命令。
    local legacy_volume="${COMPOSE_PROJECT_NAME:-synapse}_synapse_data"
    if docker volume inspect "$legacy_volume" >/dev/null 2>&1; then
        log_error "检测到遗留命名卷 $legacy_volume —— 它可能仍保存着现役 megolm.key。"
        log_error "直接改绑宿主机目录会静默换钥，导致已入库的 megolm 密文无法解密。"
        log_error "请先确认并搬运密钥（假定卷内有 megolm.key），然后删除该卷："
        log_error "  docker run --rm -v $legacy_volume:/src -v \"\$PWD/${data_dir}:/dst\" alpine \\"
        log_error "    sh -c 'cp -n /src/megolm.key /dst/megolm.key'"
        log_error "  docker volume rm $legacy_volume"
        return 1
    fi

    mkdir -p "$data_dir"

    if [ -s "$key_file" ]; then
        # 已在位：只做完整性校验，绝不重写。
        local decoded_bytes
        decoded_bytes="$(openssl base64 -d -in "$key_file" 2>/dev/null | wc -c | tr -d '[:space:]')"
        if [ "$decoded_bytes" != "32" ]; then
            log_error "现有密钥不可用: ${key_file}（base64 解码后 ${decoded_bytes:-0} 字节，需为 32）"
            log_error "服务会因 fail-closed 拒绝启动。请从备份恢复正确密钥；"
            log_error "若确认这把钥从未被使用过，可删除该文件后重跑本步骤以重新生成。"
            return 1
        fi
        log_success "megolm.key 已存在且合法（解码 32 字节）"
        # 顺手收紧权限：历史遗留的密钥是 0644（宿主机任何本地用户可读）。与
        # ensure_ssl_certs 对 TLS 私钥的处理保持一致，统一 0600。
        local current_mode
        current_mode="$(stat -f '%Lp' "$key_file" 2>/dev/null || stat -c '%a' "$key_file" 2>/dev/null || echo '')"
        if [ -n "$current_mode" ] && [ "$current_mode" != "600" ]; then
            chmod 600 "$key_file"
            log_warning "已将 megolm.key 权限由 ${current_mode} 收紧为 600"
        fi
        return
    fi

    if [ -e "$key_file" ]; then
        log_warning "megolm.key 存在但为空，视为未供给，重新生成..."
    fi
    log_info "生成新的 megolm 静态加密密钥..."
    # `openssl rand -base64 32` 恰好给出 base64(32 字节)。先写临时文件再原子改名，
    # 避免中途失败留下半截文件 —— 半截文件同样会让服务 fail-closed。
    local tmp_file="${key_file}.tmp.$$"
    if ! (umask 077 && openssl rand -base64 32 >"$tmp_file" && [ -s "$tmp_file" ]); then
        log_error "生成 megolm.key 失败"
        rm -f "$tmp_file"
        return 1
    fi
    mv -f "$tmp_file" "$key_file"
    chmod 600 "$key_file"
    log_success "已生成 megolm.key: ${key_file}（权限 0600）"
}

# =============================================================================
# /etc/hosts 域名映射检查 (matrix.test -> 127.0.0.1)
# =============================================================================
ensure_hosts_entry() {
    DEPLOYMENT_PHASE="hosts-check"
    log_info "检查 /etc/hosts 域名映射..."
    local server_name="${SERVER_NAME:-matrix.test}"

    # 逐字段判断：先去掉行内注释，再要求该行第一个字段是 127.0.0.1 且**任一后续
    # 字段**等于域名。刻意不用
    #   grep -Eq "(^|[[:space:]])127\.0\.0\.1([[:space:]]+.*)?${server_name}\b"
    # 这种写法：BSD grep 走最左最长匹配，会把 `([[:space:]]+.*)?` 一路吃到行尾，
    # 之后再要求匹配域名就永远失败 —— 于是对**正确**的 hosts（形如
    # `127.0.0.1 matrix.test element.test`，域名不在首位）误报「缺少域名映射」，
    # 还会诱导运维再追加一条重复记录（2026-09-21 在本机 /etc/hosts 上实测：
    # 旧正则不匹配、新写法匹配）。字段比较同时天然避开注释行与 IPv6 行。
    if awk -v host="$server_name" '
        { sub(/#.*/, "") }
        $1 == "127.0.0.1" {
            for (i = 2; i <= NF; i++) if ($i == host) { found = 1 }
        }
        END { exit(found ? 0 : 1) }
    ' /etc/hosts 2>/dev/null; then
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
# 监控栈启动（prometheus / alertmanager / grafana / node-exporter / alert-handler）
# =============================================================================
# 监控栈通过独立的 `docker-compose.monitoring.yml` 编排（此前是裸 `docker run`
# 手工起的，配置能改、栈无法重建）。它复用核心栈创建的网络，因此必须在核心栈
# 起来之后再启动。
#
# **非致命**：监控不在对外服务关键路径上，任一环节失败只告警、返回 0，
# 不触发核心栈回滚。用 `--no-monitoring` 可整体跳过。
MONITORING_COMPOSE_FILE="docker-compose.monitoring.yml"
MONITORING_PROJECT="synapse-monitoring"

start_monitoring() {
    DEPLOYMENT_PHASE="monitoring"
    if [ "$CHECK_MONITORING" != "true" ]; then
        log_info "跳过监控栈启动 (--no-monitoring)"
        return 0
    fi
    if [ ! -f "$DEPLOY_ROOT/$MONITORING_COMPOSE_FILE" ]; then
        log_warning "未找到 ${MONITORING_COMPOSE_FILE}，跳过监控栈"
        return 0
    fi

    # 网络由核心栈创建；没有它监控栈抓不到 synapse-app:9090。
    # 名字必须与两份 compose 里的 `name:` 一致 —— 那里刻意用 ${SYNAPSE_NETWORK_NAME}
    # 而非 ${COMPOSE_PROJECT_NAME}，因为监控栈是以 `-p synapse-monitoring` 启动的，
    # `-p` 会覆盖 COMPOSE_PROJECT_NAME，导致监控栈去引用一个不存在的网络。
    local net="${SYNAPSE_NETWORK_NAME:-synapse_network}"
    if ! docker network inspect "$net" >/dev/null 2>&1; then
        log_warning "网络 $net 不存在（核心栈未启动？），跳过监控栈"
        return 0
    fi

    # worker 抓取凭证：缺失时生成。必须先生成再 up —— 源文件不存在时 Docker 会
    # 建一个**目录**再挂载，prometheus 会把它当文件读取失败。
    local token_file="$DEPLOY_ROOT/prometheus/auth/worker-token"
    if [ ! -s "$token_file" ]; then
        mkdir -p "$(dirname "$token_file")"
        if openssl rand -hex 32 >"$token_file" 2>/dev/null; then
            chmod 600 "$token_file" 2>/dev/null || true
            log_info "已生成 prometheus worker-token"
        else
            log_warning "无法生成 worker-token（openssl 不可用），prometheus worker 抓取将不可用"
            rm -f "$token_file" 2>/dev/null || true
        fi
    fi

    log_info "启动监控栈 (project=$MONITORING_PROJECT)..."
    if compose -p "$MONITORING_PROJECT" -f "$MONITORING_COMPOSE_FILE" up -d --remove-orphans >/dev/null 2>&1; then
        log_success "监控栈已启动: prometheus / alertmanager / grafana / node-exporter / alert-handler"
    else
        log_warning "监控栈启动失败（不影响核心服务）。手动排查:"
        log_warning "  cd $DEPLOY_ROOT && docker compose -p $MONITORING_PROJECT -f $MONITORING_COMPOSE_FILE up -d"
    fi
    return 0
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
        {
            log_error "HTTPS 健康检查失败: $base/health"
            return 1
        }
    curl -kfsS "$base/_matrix/client/versions" >/dev/null ||
        {
            log_error "HTTPS API 检查失败: $base/_matrix/client/versions"
            return 1
        }
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
        REGISTRATION_SHARED_SECRET
        SECRET_KEY
        MACAROON_SECRET
        FORM_SECRET
        # 空值不被接受：它会让 HKDF 用零熵输入派生密钥，联邦签名私钥会以
        # 看似加密、实则无保密性的形式入库（详见 docker-compose.yml 同处注释）。
        FEDERATION_MASTER_KEY
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
    # 注意：不要 mkdir config —— 配置的唯一真相源是 $PROJECT_ROOT/docker/config/，
    # 在 deploy 目录下创建空的 config/ 会形成"看似有副本"的假象，并让 compose
    # 挂载到空目录（镜像内 /app/config 本身为空，服务会因缺配置启动失败）。
    mkdir -p ssl media logs backups
    # $SYNAPSE_DATA_DIR 是 /app/data 的宿主机落点（见 docker-compose.yml 的 volumes
    # 注释）。必须在 `compose up` 之前存在，否则 Docker 会代为创建；那样在 Linux
    # 上目录属主是 root，而容器以 uid 1000 运行，后续写 signing.key 可能失败。
    mkdir -p "$SYNAPSE_DATA_DIR"
    # P3-fix: migrations are no longer a hand-synced copy under docker/deploy/.
    # The migrator mounts the canonical $PROJECT_ROOT/migrations directly, so that
    # is what must exist (and contain a baseline) before we start containers.
    [ -d "$PROJECT_ROOT/migrations" ] || {
        log_error "canonical migrations 目录不存在: $PROJECT_ROOT/migrations"
        exit 1
    }
    ls "$PROJECT_ROOT"/migrations/00000000_unified_schema_v*.sql >/dev/null 2>&1 || {
        log_error "canonical migrations 目录缺少统一基线脚本 (00000000_unified_schema_v*.sql)"
        exit 1
    }
    # 配置同 migrations 一样只有一份：$PROJECT_ROOT/docker/config/。
    # compose 以 `../config` 挂载它，docker/Dockerfile 也从同一路径打进镜像。
    for cfg in homeserver.yaml rate_limit.yaml postgres.conf; do
        [ -f "$PROJECT_ROOT/docker/config/$cfg" ] || {
            log_error "缺少配置文件: $PROJECT_ROOT/docker/config/$cfg"
            exit 1
        }
    done
    log_success "目录与配置文件检查完成"
}

backup_current_state() {
    DEPLOYMENT_PHASE="backup"
    log_info "为回滚创建备份..."

    local current_image
    current_image="$(local_image_ref)"

    # 回滚源选择：优先使用镜像标签；若标签不存在（上一次部署已删除本地标签）但旧
    # 容器仍在运行，则回退到旧容器实际使用的镜像 ID。否则本次部署将失去可用的
    # 回滚目标，失败时只能"无回滚"，可靠性无法保证。
    local rollback_source=""
    if docker image inspect "$current_image" >/dev/null 2>&1; then
        rollback_source="$current_image"
    elif docker inspect synapse-app >/dev/null 2>&1; then
        rollback_source="$(docker inspect synapse-app --format '{{.Image}}' 2>/dev/null || true)"
        if [ -n "$rollback_source" ]; then
            log_warning "本地标签 $current_image 不存在，改用运行中 synapse-app 容器的镜像作为回滚源"
        fi
    fi

    if [ -n "$rollback_source" ]; then
        # 清理历史 rollback 标签：每次部署仅保留即将创建的最新一个，避免
        # rollback-* 镜像无限累积（每个约 200MB）。仅按 tag 名删除，不会误删
        # 当前 ${current_image} 指向的镜像。
        local old_rollback
        for old_rollback in $(docker images --format '{{.Repository}}:{{.Tag}}' 2>/dev/null | grep ':rollback-' || true); do
            docker rmi -f "$old_rollback" >/dev/null 2>&1 || true
        done
        ROLLBACK_IMAGE_TAG="${current_image%:*}:rollback-${TIMESTAMP}"
        if docker tag "$rollback_source" "$ROLLBACK_IMAGE_TAG"; then
            log_info "已保存旧镜像标签: $ROLLBACK_IMAGE_TAG"
        else
            ROLLBACK_IMAGE_TAG=""
            log_warning "创建回滚镜像标签失败，本次部署将无镜像回滚目标"
        fi
    else
        log_info "无可用的旧镜像作为回滚目标（疑似首次部署）"
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
        return 0
    fi
    log_info "彻底清理项目缓存（编译缓存 / 临时文件 / Docker 构建缓存）..."

    # 1) Rust 编译缓存（cargo clean + 兜底删除残留 target 目录）
    if command -v cargo >/dev/null 2>&1; then
        (cd "$PROJECT_ROOT" && cargo clean) || log_warning "cargo clean 返回非零，继续兜底清理"
    else
        log_warning "未找到 cargo，跳过 cargo clean"
    fi
    rm -rf "$PROJECT_ROOT/target"

    # 2) 部署临时文件：日志管道 + 历史部署日志只保留最近 10 份
    find "$LOG_DIR" -maxdepth 1 -name '.deploy_*.pipe' -delete 2>/dev/null || true
    local old_logs
    old_logs="$(ls -1t "$LOG_DIR"/deploy_*.log 2>/dev/null | tail -n +11 || true)"
    if [ -n "$old_logs" ]; then
        # shellcheck disable=SC2086
        echo "$old_logs" | xargs -r rm -f
        log_info "已清理历史部署日志: $(echo "$old_logs" | wc -l | tr -d ' ') 份"
    fi

    # 3) 系统临时目录中本项目产生的临时文件
    find "${TMPDIR:-/tmp}" -maxdepth 1 -name 'synapse-rust-*' -exec rm -rf {} + 2>/dev/null || true
    find "${TMPDIR:-/tmp}" -maxdepth 1 -name 'deploy_*.pipe' -delete 2>/dev/null || true

    # 4) Docker 构建缓存与悬空镜像
    # 注意：不清理 npm/yarn/pnpm 全局缓存——本项目为 Rust 后端，这些 JS 包管理器
    # 缓存与构建无关，且 `pnpm store prune` 会触发环境 safe-delete hook
    # （删除 ~/.cache 下大量文件），故明确移除，仅清理项目级与 Docker 缓存。
    # [O2-2] 改为有界清理：只清 7 天前的缓存，不清除其他项目的构建缓存
    docker builder prune --filter 'until=168h' -f >/dev/null 2>&1 || true
    docker buildx prune --filter 'until=168h' -f >/dev/null 2>&1 || true

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
        (cd "$PROJECT_ROOT" && cargo build --release --locked --no-default-features --bin synapse-rust --bin healthcheck)
    elif [ "$ENABLED_EXTENSIONS" = "$CORE_PRIVATE_CHAT_EXTENSIONS" ]; then
        (cd "$PROJECT_ROOT" && cargo build --release --locked --no-default-features --features core-private-chat --bin synapse-rust --bin healthcheck)
    else
        (cd "$PROJECT_ROOT" && cargo build --release --locked --no-default-features --features "$ENABLED_EXTENSIONS" --bin synapse-rust --bin healthcheck)
    fi
    log_success "项目编译完成"
}

# 安全停止所有后端容器：先 SIGTERM 优雅停机（等待 STOP_TIMEOUT 秒），
# 再校验是否仍存活，最后才进入删除阶段。避免直接 `down` 造成数据/连接突断。
stop_services_gracefully() {
    DEPLOYMENT_PHASE="stop-services"
    local project="${COMPOSE_PROJECT_NAME:-synapse}"

    local running
    running="$(docker ps --filter "label=com.docker.compose.project=${project}" --format '{{.Names}}' 2>/dev/null || true)"
    if [ -z "$running" ]; then
        log_info "未检测到运行中的后端容器（project=${project}），跳过停止"
        return 0
    fi

    log_info "优雅停止后端容器 (SIGTERM，最长等待 ${STOP_TIMEOUT}s): $(echo "$running" | tr '\n' ' ')"
    if ! compose stop -t "${STOP_TIMEOUT}" >/dev/null 2>&1; then
        log_warning "compose stop 返回非零，改用逐容器 docker stop 兜底"
    fi

    local still
    still="$(docker ps --filter "label=com.docker.compose.project=${project}" --format '{{.Names}}' 2>/dev/null || true)"
    if [ -n "$still" ]; then
        log_warning "以下容器在 ${STOP_TIMEOUT}s 内未退出，将缩短超时强制停止: $(echo "$still" | tr '\n' ' ')"
        # shellcheck disable=SC2086
        echo "$still" | xargs -r docker stop -t 5 >/dev/null 2>&1 || true
        sleep 1
        still="$(docker ps --filter "label=com.docker.compose.project=${project}" --format '{{.Names}}' 2>/dev/null || true)"
        if [ -n "$still" ]; then
            log_error "容器无法停止: $(echo "$still" | tr '\n' ' ')"
            return 1
        fi
    fi

    log_success "所有后端容器已安全停止"
}

remove_existing_deployment() {
    DEPLOYMENT_PHASE="remove-old-deployment"
    log_info "删除旧容器、网络与关联资源..."

    compose down --remove-orphans || true
    docker rm -f synapse-postgres synapse-redis synapse-migrator synapse-app synapse-nginx >/dev/null 2>&1 || true

    # 显式清理项目网络：若同名网络残留（例如上次部署被中断、或容器未带 compose
    # 项目标签导致 `compose down` 未回收），后续 `compose up` 会报
    # "network with name X already exists" 并连带引发容器重名冲突，使启动步骤
    # 失败。此处仅在网络已无容器占用时删除，避免误删其它项目在用的网络。
    local net="${SYNAPSE_NETWORK_NAME:-synapse_network}"
    if docker network inspect "$net" >/dev/null 2>&1; then
        local attached
        attached="$(docker network inspect "$net" --format '{{range .Containers}}{{.Name}} {{end}}' 2>/dev/null || true)"
        if [ -z "${attached// /}" ]; then
            docker network rm "$net" >/dev/null 2>&1 || true
            log_info "已清理残留项目网络: $net"
        else
            log_warning "项目网络 $net 仍被占用，保留: ${attached}"
        fi
    fi

    # 启动前兜底校验：确保目标容器名与网络均不再残留，否则立即失败并给出明确原因。
    local leftover
    leftover="$(docker ps -a --filter 'name=^/synapse-(postgres|redis|migrator|app|nginx)$' --format '{{.Names}}' 2>/dev/null || true)"
    if [ -n "$leftover" ]; then
        log_error "存在无法清除的残留容器，会导致后续启动冲突: $(echo "$leftover" | tr '\n' ' ')"
        log_error "修复: docker rm -f $(echo "$leftover" | tr '\n' ' ')"
        return 1
    fi

    if [ "$USE_REMOTE_IMAGE" != "true" ] && [ "$SKIP_BUILD" != "true" ]; then
        docker image rm -f "$(local_image_ref)" synapse-rust-tools:local >/dev/null 2>&1 || true
    fi

    log_success "旧部署资源清理完成"
}

# 删除历史项目镜像，释放磁盘空间。安全约束：
#   - 保留 $(local_image_ref)（本次构建目标）
#   - 保留 ROLLBACK_IMAGE_TAG（本次回滚标签）
#   - 保留正在被容器引用/使用的镜像（docker rmi 本身会拒绝，force 前先判断用途）
remove_old_project_images() {
    DEPLOYMENT_PHASE="cleanup-old-images"
    if [ "$KEEP_IMAGES" = "true" ]; then
        log_info "跳过旧项目镜像清理 (--keep-images)"
        return 0
    fi
    log_info "清理历史项目镜像..."

    local keep_current keep_rollback
    keep_current="$(local_image_ref)"
    keep_rollback="${ROLLBACK_IMAGE_TAG:-}"

    # 注意：此处刻意不使用 `docker system df`。Docker Desktop 上该命令需要遍历
    # 全部镜像/层来计算磁盘占用，实测可阻塞 6 分钟以上，会让清理步骤看起来"卡死"。
    # 改为统计项目镜像数量（docker images 为本地元数据查询，毫秒级返回）。
    local images_before
    images_before="$(docker images --format '{{.Repository}}' 2>/dev/null | grep -c 'synapse-rust' || true)"

    local removed=0 skipped=0 ref repo
    while IFS= read -r ref; do
        [ -n "$ref" ] || continue
        repo="${ref%%:*}"
        case "$repo" in
            *synapse-rust*) ;;
            *) continue ;;
        esac
        if [ "$ref" = "$keep_current" ] || { [ -n "$keep_rollback" ] && [ "$ref" = "$keep_rollback" ]; }; then
            skipped=$((skipped + 1))
            continue
        fi
        if docker rmi -f "$ref" >/dev/null 2>&1; then
            removed=$((removed + 1))
            log_info "已删除旧镜像: $ref"
        else
            log_warning "未能删除镜像（可能仍被容器占用）: $ref"
        fi
    done < <(docker images --format '{{.Repository}}:{{.Tag}}' 2>/dev/null | grep -E ':.*$' || true)

    # 悬空镜像（<none>:<none>）一并回收
    docker image prune -f >/dev/null 2>&1 || true

    log_success "旧镜像清理完成: 删除 ${removed} 个，保留 ${skipped} 个 (清理前项目镜像数=${images_before:-0})"
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
    # [O1-2] 保留 Dockerfile 顶部的 digest pin，不再用浮动 tag 覆盖。
    # 若网络抖动导致 digest 拉取失败，应改为：
    #   1) 提前 `docker pull <image>@sha256:...` 并保留在本地
    #   2) 搭建内部 registry mirror（Harbor/pull-through cache）
    #   3) 在 deploy.sh 里加"预拉取并校验 digest"步骤，失败时明确报错
    docker build --no-cache \
        -f "$PROJECT_ROOT/docker/Dockerfile" \
        --target tools \
        --build-arg "CARGO_FEATURE_ARGS=${feature_args}" \
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
    retry 3 5 compose run -T --rm --no-deps -e "ENABLED_EXTENSIONS=${ENABLED_EXTENSIONS}" migrator migrate </dev/null
    log_success "数据库迁移完成"
    log_info "验证数据库架构完整性..."
    retry 3 5 compose run -T --rm --no-deps -e "ENABLED_EXTENSIONS=${ENABLED_EXTENSIONS}" migrator validate </dev/null
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

# 数据库结构与项目版本一致性校验（部署门禁）：
#   1) canonical 迁移集合（migrations/*.sql，排除 .undo.sql）必须全部已应用；
#      历史折叠进 baseline 的旧版本号允许存在于 schema_migrations 中（不算差异）。
#   2) schema_migrations 中不得存在执行失败（is_success ≠ true）的迁移。
#   3) public 表数相对 baseline（00000000_unified_schema_v*.sql 的 CREATE TABLE 数）
#      的漂移不得超过 DB_DRIFT_TOLERANCE（与 schema_health_check DB-02 阈值一致）。
verify_db_version_consistency() {
    DEPLOYMENT_PHASE="verify-db-version"
    log_info "校验数据库结构与项目版本一致性..."

    local pg_user="${POSTGRES_USER:-postgres}"
    local pg_db="${POSTGRES_DB:-synapse}"

    local psql_cmd=(compose exec -T postgres psql -U "$pg_user" -d "$pg_db" -At)

    local tmp_canonical="$LOG_DIR/.dbcheck_canonical.$$"
    local tmp_applied="$LOG_DIR/.dbcheck_applied.$$"

    if ! (cd "$PROJECT_ROOT/migrations" && ls -1 ./*.sql 2>/dev/null | sed 's#^\./##; s/\.sql$//' | grep -v '\.undo$' | sort -u) >"$tmp_canonical"; then
        rm -f "$tmp_canonical" "$tmp_applied"
        log_error "无法枚举 canonical 迁移文件: $PROJECT_ROOT/migrations"
        return 1
    fi
    if ! "${psql_cmd[@]}" -c "SELECT version FROM schema_migrations ORDER BY version;" 2>/dev/null |
        sed '/^[[:space:]]*$/d' | sort -u >"$tmp_applied"; then
        rm -f "$tmp_canonical" "$tmp_applied"
        log_error "无法读取 schema_migrations（数据库迁移记录表），请确认迁移已执行"
        return 1
    fi

    local canonical_count applied_count
    canonical_count="$(wc -l <"$tmp_canonical" | tr -d ' ')"
    applied_count="$(wc -l <"$tmp_applied" | tr -d ' ')"

    # 1) canonical 迁移是否全部应用
    local missing
    missing="$(comm -23 "$tmp_canonical" "$tmp_applied" || true)"
    if [ -n "$missing" ]; then
        log_error "以下 canonical 迁移未应用到数据库（版本不一致）:"
        echo "$missing" | sed 's/^/    - /'
        log_error "修复: 重新执行迁移 (compose run --rm migrator migrate) 后重试部署"
        rm -f "$tmp_canonical" "$tmp_applied"
        return 1
    fi

    # 2) 是否存在失败迁移
    local failed
    failed="$("${psql_cmd[@]}" -c "SELECT version FROM schema_migrations WHERE is_success IS NOT TRUE ORDER BY version;" 2>/dev/null | sed '/^[[:space:]]*$/d' || true)"
    if [ -n "$failed" ]; then
        log_error "检测到执行失败的迁移记录（is_success=false）:"
        echo "$failed" | sed 's/^/    - /'
        log_error "修复: 检查应用日志中 '迁移语句执行失败' 定位具体语句，修正后重新部署"
        rm -f "$tmp_canonical" "$tmp_applied"
        return 1
    fi

    # 3) 表数漂移（相对 baseline 声明）
    local baseline_expected actual_count drift
    baseline_expected="$(
        grep -hcE '^CREATE TABLE (IF NOT EXISTS )?[a-zA-Z_][a-zA-Z0-9_]*' \
            "$PROJECT_ROOT"/migrations/00000000_unified_schema_v*.sql 2>/dev/null |
            awk '{s += $1} END {print s + 0}'
    )"
    actual_count="$("${psql_cmd[@]}" -c \
        "SELECT count(*) FROM information_schema.tables WHERE table_schema='public' AND table_type='BASE TABLE';" 2>/dev/null |
        sed '/^[[:space:]]*$/d' | head -1 || true)"

    rm -f "$tmp_canonical" "$tmp_applied"

    if [ -n "$baseline_expected" ] && [ "$baseline_expected" -gt 0 ] && [ -n "$actual_count" ]; then
        drift=$((actual_count - baseline_expected))
        if [ "${drift#-}" -gt "$DB_DRIFT_TOLERANCE" ]; then
            log_error "数据库结构与项目版本不一致: baseline 期望 ${baseline_expected} 张表，实际 ${actual_count} 张 (drift=${drift}，阈值 ±${DB_DRIFT_TOLERANCE})"
            log_error "修复: 确认是否存在残留废弃表（如 *_legacy）或缺失迁移，再重新部署"
            return 1
        fi
        log_info "表数一致性: baseline=${baseline_expected}, 实际=${actual_count}, drift=${drift} (阈值 ±${DB_DRIFT_TOLERANCE})"
    else
        log_warning "无法计算 baseline 表数（缺少 00000000_unified_schema_v*.sql），跳过表数漂移校验"
    fi

    log_success "数据库与项目版本一致性校验通过 (canonical=${canonical_count}, 已应用记录=${applied_count}, 失败=0)"
}

verify_health_endpoints() {
    DEPLOYMENT_PHASE="verify-health"
    log_info "验证健康检查接口..."

    curl -fsS "http://localhost:${SYNAPSE_PORT:-8008}/health" >/dev/null
    curl -fsS "http://localhost:${HTTP_PORT:-80}/health" >/dev/null
    curl -fsS "http://localhost:${SYNAPSE_PORT:-8008}/_matrix/client/versions" >/dev/null

    log_success "健康检查与 API 基础接口验证通过"
}

# 已知告警白名单（格式: 匹配子串|原因）。
# 这些告警来自基础镜像或宿主 Docker 环境，与本次部署正确性无关；任何不在白名单
# 内的 WARNING 都被视为"需要修复的问题"。新增条目必须写明可核实的原因。
LOG_WARNING_ALLOWLIST=(
    'no usable system locales|基础镜像未生成 locale，仅影响 psql/i18n 输出格式'
    'enabling "trust" authentication|容器内网 pg_hba trust 认证（5432 未对外暴露）'
    'DOCKER_INSECURE_NO_IPTABLES_RAW|宿主 Docker Desktop 环境变量，非本项目设置'
    'forcibly turning on oci-mediatype|buildkit 对缺少 mediatype 的镜像提示'
    '_sqlx_migrations|sqlx-cli 迁移表探测提示（本项目使用 schema_migrations）'
    'Missing indexes|启动期索引建议（非必需索引），已由 schema 健康检查覆盖'
    'update-alternatives|基础镜像包安装信息'
    'rehash: warning: skipping ca-certificates.crt|基础镜像 ca-certificates 提示'
)

# 日志告警分析：区分 ERROR（一律阻断）、已知 WARNING（附原因，仅提示）、
# 未知 WARNING（默认阻断，可 --no-strict-warnings 降级为提示）。
verify_logs_clean() {
    DEPLOYMENT_PHASE="verify-logs"
    log_info "分析容器日志中的 ERROR / WARNING..."

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

    local warn_lines
    warn_lines="$(echo "$log_dump" | grep -Ei '\bWARN(ING)?\b' || true)"

    local allowed_summary="" unknown_warnings=""
    local allowed_count=0 unknown_count=0
    local line entry matched
    while IFS= read -r line; do
        [ -n "$line" ] || continue
        matched=""
        for entry in "${LOG_WARNING_ALLOWLIST[@]}"; do
            if [[ "$line" == *"${entry%%|*}"* ]]; then
                matched="${entry#*|}"
                break
            fi
        done
        if [ -n "$matched" ]; then
            allowed_count=$((allowed_count + 1))
            allowed_summary+="    [已知] ${matched}"$'\n'
        else
            unknown_count=$((unknown_count + 1))
            unknown_warnings+="    ${line}"$'\n'
        fi
    done <<<"$warn_lines"

    if [ -n "$errors" ]; then
        log_error "检测到 ERROR 日志（部署门禁不通过）:"
        echo "$errors"
        return 1
    fi

    log_info "告警统计: 已知=${allowed_count}, 未知=${unknown_count}"

    if [ "$allowed_count" -gt 0 ]; then
        log_info "已知告警（匹配白名单，已确认无需处理）:"
        printf '%s' "$allowed_summary" | sort -u
    fi

    if [ "$unknown_count" -gt 0 ]; then
        log_warning "检测到未知 WARNING（需要修复）:"
        printf '%s' "$unknown_warnings" | sort -u
        if [ "$STRICT_WARNINGS" = "true" ]; then
            log_error "STRICT_WARNINGS=true: 未知告警视为部署失败；修复后重试，或临时使用 --no-strict-warnings"
            return 1
        fi
        log_warning "STRICT_WARNINGS=false: 未知告警不阻断部署"
    else
        log_success "未发现 ERROR，且无未知 WARNING"
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
    log_warning "=========================================================="
    log_warning "开始回滚（失败步骤: ${CURRENT_STEP:-unknown}）"
    log_warning "=========================================================="

    # 1) 恢复旧镜像标签（构建失败/新镜像不可用时，先让旧镜像恢复可用）
    if [ -n "$ROLLBACK_IMAGE_TAG" ] && docker image inspect "$ROLLBACK_IMAGE_TAG" >/dev/null 2>&1; then
        if docker tag "$ROLLBACK_IMAGE_TAG" "$(local_image_ref)"; then
            log_success "已恢复旧镜像: $ROLLBACK_IMAGE_TAG -> $(local_image_ref)"
        else
            log_error "恢复旧镜像失败: $ROLLBACK_IMAGE_TAG"
        fi
    else
        log_warning "无可用回滚镜像标签，跳过镜像回滚"
    fi

    # 2) 数据回滚（部署前 pg_dump 备份）
    if [ -n "$ROLLBACK_BACKUP" ] && [ -f "$ROLLBACK_BACKUP" ]; then
        chmod +x scripts/restore.sh
        if RESTORE_FORCE=true ./scripts/restore.sh "$ROLLBACK_BACKUP"; then
            log_success "已从备份恢复数据库: $ROLLBACK_BACKUP"
        else
            log_error "数据库恢复失败，请手动处理备份: $ROLLBACK_BACKUP"
        fi
    else
        log_warning "无可用数据库备份，跳过数据回滚"
    fi

    # 3) 用旧镜像尝试恢复服务
    if [ -n "$ROLLBACK_IMAGE_TAG" ] && docker image inspect "$ROLLBACK_IMAGE_TAG" >/dev/null 2>&1; then
        log_info "尝试以旧镜像恢复服务..."
        if compose up -d postgres redis >/dev/null 2>&1 && compose up -d synapse nginx >/dev/null 2>&1; then
            log_success "旧版本服务已重新启动"
        else
            log_warning "旧版本服务未完全启动，请检查: docker compose ps / docker compose logs"
        fi
    fi

    log_warning "回滚流程结束；若服务仍异常，请查看 ${LOG_FILE} 与 'docker compose ps'"
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

    # --- 前置准备（只读/本地操作，失败即停止，不会破坏现有部署） ---
    run_step "环境依赖检查" check_dependencies
    if [ "$INSTALL_DEPS" = "true" ]; then
        run_step "安装缺失依赖" install_missing_deps
    fi
    run_step "配置文件检查" check_env_file
    run_step "功能选择" select_features
    run_step "功能摘要" show_feature_summary
    run_step "目录准备" create_directories
    run_step "SSL 证书准备" ensure_ssl_certs
    run_step "应用数据密钥准备" ensure_app_data_keys
    run_step "hosts 检查" ensure_hosts_entry
    run_step "本地 TURN 检查" check_local_turn

    # --- 备份与清理（此时旧服务仍在运行，可安全备份与回滚） ---
    run_step "部署前备份" backup_current_state
    run_step "缓存清理" clear_project_caches
    run_step "项目编译" rebuild_project

    # --- 安全下线旧部署 ---
    run_step "优雅停止后端容器" stop_services_gracefully
    run_step "移除旧部署资源" remove_existing_deployment
    run_step "清理旧项目镜像" remove_old_project_images

    # --- 构建与启动 ---
    run_step "构建/拉取镜像" build_images
    run_step "启动服务与迁移" start_services

    # --- 一致性校验与验证 ---
    run_step "数据库连接验证" verify_database
    run_step "数据库版本一致性校验" verify_db_version_consistency
    run_step "健康检查验证" verify_health_endpoints
    run_step "HTTPS 接口验证" verify_https_endpoints
    run_step "日志告警分析" verify_logs_clean
    # 监控栈放在最后：它抓取的核心服务此时已就绪，且失败不影响核心服务。
    run_step "启动监控栈" start_monitoring

    show_status
    show_access_info
    log_success "重建、优化部署与验证全部完成（共 ${#COMPLETED_STEPS[@]} 个步骤）"
}

main "$@"
