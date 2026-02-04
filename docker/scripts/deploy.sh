#!/bin/bash

set -e

#=======================================
# Matrix Synapse 生产环境部署脚本
#=======================================
# 功能说明:
# - 启动 Nginx 反向代理 (端口 80/443)
# - 启动 Matrix Synapse 服务 (端口 8008)
# - 启动 PostgreSQL 数据库 (端口 5432)
# - 启动 Redis 缓存 (端口 6379)
# - 健康检查和服务状态监控
# - SSL 证书自动申请和续期 (使用 acme.sh)
#
# 使用方法:
#   ./deploy.sh [start|stop|restart|status|logs|logs-nginx|logs-synapse|logs-db|health|ssl-init|ssl-renewal|backup|restore|scale]
#
# 环境变量:
#   DOMAIN_CERT_DIR    - SSL证书目录 (默认: /etc/nginx/ssl)
#   DATA_DIR           - 数据目录 (默认: ./data)
#   BACKUP_DIR         - 备份目录 (默认: ./backup)
#   NGINX_CONFIG_DIR   - Nginx配置目录 (默认: ./docker/nginx)
#   SYNAPSE_CONFIG_DIR - Synapse配置目录 (默认: ./docker/config)
#=======================================

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 日志函数
log_info() {
    echo -e "${GREEN}[INFO]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

log_debug() {
    echo -e "${BLUE}[DEBUG]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

# 默认配置
DOMAIN_CERT_DIR="${DOMAIN_CERT_DIR:-/etc/nginx/ssl}"
DATA_DIR="${DATA_DIR:-./data}"
BACKUP_DIR="${BACKUP_DIR:-./backup}"
NGINX_CONFIG_DIR="${NGINX_CONFIG_DIR:-./docker/nginx}"
SYNAPSE_CONFIG_DIR="${SYNAPSE_CONFIG_DIR:-./docker/config}"

# 服务域名配置
PRIMARY_DOMAIN="cjystx.top"
MATRIX_DOMAIN="matrix.cjystx.top"

# 服务名称 (与 docker-compose.yml 一致)
SYNAPSE_CONTAINER="synapse_rust"
NGINX_CONTAINER="synapse_nginx"
DB_CONTAINER="synapse_postgres"
REDIS_CONTAINER="synapse_redis"

#=======================================
# 基础命令检查
#=======================================
check_dependencies() {
    log_info "检查依赖..."

    local missing_deps=()

    # 检查 Docker
    if ! command -v docker &> /dev/null; then
        missing_deps+=("docker")
        log_error "Docker 未安装"
    fi

    # 检查 Docker Compose
    if ! command -v docker-compose &> /dev/null && ! docker compose version &> /dev/null; then
        missing_deps+=("docker-compose")
        log_error "Docker Compose 未安装"
    fi

    # 检查 curl (用于健康检查)
    if ! command -v curl &> /dev/null; then
        missing_deps+=("curl")
        log_warn "curl 未安装，部分功能可能不可用"
    fi

    if [ ${#missing_deps[@]} -gt 0 ]; then
        log_error "缺少必要依赖: ${missing_deps[*]}"
        log_info "请安装缺失的依赖后重试"
        exit 1
    fi

    log_info "依赖检查通过"
}

#=======================================
# Docker Compose 配置生成
#=======================================
generate_compose_config() {
    log_info "生成 Docker Compose 配置文件..."

    local compose_file="${DATA_DIR}/docker-compose.yml"

    cat > "$compose_file" << EOF
version: '3.8'

services:
  # Nginx 反向代理
  nginx-proxy:
    image: nginx:alpine
    container_name: ${NGINX_CONTAINER}
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ${NGINX_CONFIG_DIR}/nginx.conf:/etc/nginx/nginx.conf:ro
      - ${NGINX_CONFIG_DIR}/conf.d:/etc/nginx/conf.d:ro
      - ${DOMAIN_CERT_DIR}:/etc/nginx/ssl:ro
      - nginx_logs:/var/log/nginx
    networks:
      - synapse_network
    healthcheck:
      test: ["CMD", "wget", "-q", "--spider", "http://localhost/health", "||", "exit", "1"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 10s
    restart: unless-stopped
    depends_on:
      - synapse
    logging:
      driver: "json-file"
      options:
        max-size: "100m"
        max-file: "5"

  # Matrix Synapse 服务器
  synapse:
    image: synapse:${SYNAPSE_VERSION:-latest}
    container_name: ${SYNAPSE_CONTAINER}
    expose:
      - "8008"
    volumes:
      - ${SYNAPSE_CONFIG_DIR}/homeserver.yaml:/homeserver.yaml:ro
      - synapse_data:/data
      - synapse_media:/media
      - synapse_uploads:/uploads
    networks:
      - synapse_network
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8008/_matrix/client/versions", "||", "exit", "1"]
      interval: 30s
      timeout: 15s
      retries: 5
      start_period: 30s
    environment:
      - SYNAPSE_SERVER_NAME=${MATRIX_DOMAIN}
      - SYNAPSE_REPORT_STATS=no
      - SYNAPSE_NO_TLS=true
    restart: unless-stopped
    depends_on:
      - redis
    logging:
      driver: "json-file"
      options:
        max-size: "200m"
        max-file: "10"

  # PostgreSQL 数据库
  synapse-db:
    image: postgres:15-alpine
    container_name: ${DB_CONTAINER}
    expose:
      - "5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ${DATA_DIR}/init.sql:/docker-entrypoint-initdb.d/init.sql:ro
    networks:
      - synapse_network
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U synapse"]
      interval: 30s
      timeout: 10s
      retries: 5
      start_period: 10s
    environment:
      - POSTGRES_USER=synapse
      - POSTGRES_PASSWORD=synapse
      - POSTGRES_DB=synapse_test
    restart: unless-stopped
    volumes_from:
      - synapse

  # Redis 缓存
  synapse-redis:
    image: redis:7-alpine
    container_name: ${REDIS_CONTAINER}
    expose:
      - "6379"
    volumes:
      - redis_data:/data
    networks:
      - synapse_network
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 30s
      timeout: 10s
      retries: 5
      start_period: 5s
    restart: unless-stopped

# 数据卷定义
volumes:
  nginx_logs:
    driver: local
  synapse_data:
    driver: local
  synapse_media:
    driver: local
  synapse_uploads:
    driver: local
  postgres_data:
    driver: local
  redis_data:
    driver: local

# 网络定义
networks:
  synapse_network:
    driver: bridge
    ipam:
      config:
        - subnet: 172.28.0.0/16
          gateway: 172.28.0.1
EOF

    log_info "Docker Compose 配置文件已生成: $compose_file"
}

#=======================================
# 服务管理命令
#=======================================
start_services() {
    log_info "启动所有服务..."

    # 创建必要目录
    mkdir -p "$DATA_DIR" "$BACKUP_DIR" "$NGINX_CONFIG_DIR/conf.d"
    mkdir -p "$SYNAPSE_CONFIG_DIR"

    # 生成 Docker Compose 配置
    if [ ! -f "${DATA_DIR}/docker-compose.yml" ]; then
        generate_compose_config
    fi

    # 启动服务
    cd "$DATA_DIR"

    if command -v docker-compose &> /dev/null; then
        docker-compose up -d
    else
        docker compose up -d
    fi

    log_info "服务启动中，请等待健康检查..."
    sleep 10

    # 等待服务健康
    wait_for_health

    log_info "所有服务已启动"
    show_status
}

stop_services() {
    log_info "停止所有服务..."

    cd "$DATA_DIR"

    if command -v docker-compose &> /dev/null; then
        docker-compose down
    else
        docker compose down
    fi

    log_info "所有服务已停止"
}

restart_services() {
    log_info "重启所有服务..."
    stop_services
    sleep 5
    start_services
}

#=======================================
# 日志查看
#=======================================
show_logs() {
    local service="$1"
    local lines="${2:-100}"

    cd "$DATA_DIR"

    if [ -n "$service" ]; then
        if command -v docker-compose &> /dev/null; then
            docker-compose logs -f --tail="$lines" "$service"
        else
            docker compose logs -f --tail="$lines" "$service"
        fi
    else
        if command -v docker-compose &> /dev/null; then
            docker-compose logs -f --tail="$lines"
        else
            docker compose logs -f --tail="$lines"
        fi
    fi
}

#=======================================
# 服务状态检查
#=======================================
show_status() {
    log_info "检查服务状态..."

    local services=("$NGINX_CONTAINER" "$SYNAPSE_CONTAINER" "$DB_CONTAINER" "$REDIS_CONTAINER")

    for service in "${services[@]}"; do
        if docker ps --format '{{.Names}}' | grep -q "^${service}$"; then
            local status=$(docker inspect --format='{{.State.Status}}' "$service" 2>/dev/null)
            if [ "$status" == "running" ]; then
                echo -e "${GREEN}[✓]${NC} $service: 运行中"
            else
                echo -e "${YELLOW}[!]${NC} $service: $status"
            fi
        else
            echo -e "${RED}[✗]${NC} $service: 未运行"
        fi
    done

    # 显示资源使用情况
    echo ""
    log_info "资源使用情况:"
    docker stats --no-stream --format "table {{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}\t{{.NetIO}}\t{{.BlockIO}}" \
        "$NGINX_CONTAINER" "$SYNAPSE_CONTAINER" "$DB_CONTAINER" "$REDIS_CONTAINER" 2>/dev/null || true
}

#=======================================
# 健康检查
#=======================================
health_check() {
    log_info "执行健康检查..."

    local checks_passed=0
    local checks_failed=0

    # 检查 1: Nginx 服务 (使用 -k 跳过自签名证书验证)
    if curl -sfLk -o /dev/null "http://localhost/.well-known/matrix/server" 2>/dev/null; then
        log_info "[✓] Nginx .well-known 服务正常"
        ((checks_passed++))
    else
        log_error "[✗] Nginx .well-known 服务异常"
        ((checks_failed++))
    fi

    # 检查 2: Matrix Synapse API
    if curl -sf -o /dev/null "http://localhost:8008/_matrix/client/versions" 2>/dev/null; then
        log_info "[✓] Matrix Synapse API 正常"
        ((checks_passed++))
    else
        log_error "[✗] Matrix Synapse API 异常"
        ((checks_failed++))
    fi

    # 检查 3: PostgreSQL 数据库
    if docker exec "$DB_CONTAINER" pg_isready -U synapse &>/dev/null; then
        log_info "[✓] PostgreSQL 数据库正常"
        ((checks_passed++))
    else
        log_error "[✗] PostgreSQL 数据库异常"
        ((checks_failed++))
    fi

    # 检查 4: Redis 缓存
    if docker exec "$REDIS_CONTAINER" redis-cli ping &>/dev/null; then
        log_info "[✓] Redis 缓存正常"
        ((checks_passed++))
    else
        log_error "[✗] Redis 缓存异常"
        ((checks_failed++))
    fi

    # 检查 5: SSL 证书
    if [ -f "${DOMAIN_CERT_DIR}/${MATRIX_DOMAIN}/fullchain.pem" ]; then
        local expiry_date=$(openssl x509 -enddate -noout -in "${DOMAIN_CERT_DIR}/${MATRIX_DOMAIN}/fullchain.pem" 2>/dev/null | cut -d= -f2)
        log_info "[✓] SSL 证书有效 (过期时间: $expiry_date)"
        ((checks_passed++))
    else
        log_warn "[!] SSL 证书未配置"
        ((checks_failed++))
    fi

    echo ""
    log_info "健康检查结果: $checks_passed 通过, $checks_failed 失败"

    if [ $checks_failed -gt 0 ]; then
        return 1
    fi
    return 0
}

wait_for_health() {
    log_info "等待服务健康..."
    local max_attempts=30
    local attempt=0

    while [ $attempt -lt $max_attempts ]; do
        if health_check &>/dev/null; then
            log_info "所有服务健康检查通过"
            return 0
        fi

        ((attempt++))
        log_debug "健康检查尝试 $attempt/$max_attempts..."
        sleep 5
    done

    log_error "健康检查超时，部分服务可能未完全启动"
    return 1
}

#=======================================
# SSL 证书管理
#=======================================
ssl_init() {
    log_info "初始化 SSL 证书..."

    # 创建证书目录结构
    mkdir -p "${DOMAIN_CERT_DIR}/${PRIMARY_DOMAIN}"
    mkdir -p "${DOMAIN_CERT_DIR}/${MATRIX_DOMAIN}"
    mkdir -p "${DOMAIN_CERT_DIR}/default"

    # 检查是否已有有效证书
    if [ -f "${DOMAIN_CERT_DIR}/${PRIMARY_DOMAIN}/fullchain.pem" ] && \
       [ -f "${DOMAIN_CERT_DIR}/${MATRIX_DOMAIN}/fullchain.pem" ]; then
        log_info "SSL 证书已存在，跳过申请"
        return 0
    fi

    # 尝试使用 acme.sh 申请证书
    local use_acme=false

    if command -v ~/.acme.sh/acme.sh &> /dev/null; then
        log_info "尝试使用 acme.sh 申请证书..."
        if ~/.acme.sh/acme.sh --issue -d "$PRIMARY_DOMAIN" -d "www.${PRIMARY_DOMAIN}" \
            --webroot /var/www/html --keylength ec-256 --log "${DATA_DIR}/acme.log" 2>/dev/null; then
            use_acme=true
        fi
    fi

    if [ "$use_acme" = true ]; then
        # 安装证书
        log_info "安装 SSL 证书..."
        ~/.acme.sh/acme.sh --installcert -d "$PRIMARY_DOMAIN" \
            --fullchainfile "${DOMAIN_CERT_DIR}/${PRIMARY_DOMAIN}/fullchain.pem" \
            --keyfile "${DOMAIN_CERT_DIR}/${PRIMARY_DOMAIN}/privkey.pem"

        ~/.acme.sh/acme.sh --installcert -d "$MATRIX_DOMAIN" \
            --fullchainfile "${DOMAIN_CERT_DIR}/${MATRIX_DOMAIN}/fullchain.pem" \
            --keyfile "${DOMAIN_CERT_DIR}/${MATRIX_DOMAIN}/privkey.pem"
    else
        # 使用自签名证书作为回退
        log_warn "acme.sh 不可用或证书申请失败，使用自签名证书"

        # 为主域名创建自签名证书
        log_info "创建 ${PRIMARY_DOMAIN} 自签名证书..."
        openssl req -x509 -nodes -days 365 \
            -newkey ec:<(openssl ecparam -name prime256v1) \
            -keyout "${DOMAIN_CERT_DIR}/${PRIMARY_DOMAIN}/privkey.pem" \
            -out "${DOMAIN_CERT_DIR}/${PRIMARY_DOMAIN}/fullchain.pem" \
            -subj "/C=CN/ST=Beijing/L=Beijing/O=Synapse/CN=${PRIMARY_DOMAIN}" 2>/dev/null

        # 为 Matrix 子域名创建自签名证书
        log_info "创建 ${MATRIX_DOMAIN} 自签名证书..."
        openssl req -x509 -nodes -days 365 \
            -newkey ec:<(openssl ecparam -name prime256v1) \
            -keyout "${DOMAIN_CERT_DIR}/${MATRIX_DOMAIN}/privkey.pem" \
            -out "${DOMAIN_CERT_DIR}/${MATRIX_DOMAIN}/fullchain.pem" \
            -subj "/C=CN/ST=Beijing/L=Beijing/O=Synapse/CN=${MATRIX_DOMAIN}" 2>/dev/null
    fi

    # 为 default server 创建占位证书
    log_info "创建 default SSL 占位证书..."
    openssl req -x509 -nodes -days 365 \
        -newkey ec:<(openssl ecparam -name prime256v1) \
        -keyout "${DOMAIN_CERT_DIR}/default/privkey.pem" \
        -out "${DOMAIN_CERT_DIR}/default/default.crt" \
        -subj "/C=CN/ST=Beijing/L=Beijing/O=Synapse/CN=default" 2>/dev/null

    # 设置权限
    chmod 600 "${DOMAIN_CERT_DIR}"/*/privkey.pem
    chmod 644 "${DOMAIN_CERT_DIR}"/*/fullchain.pem
    chmod 644 "${DOMAIN_CERT_DIR}/default/default.crt"

    # 设置自动续期
    log_info "设置自动续期..."
    ~/.acme.sh/acme.sh --upgrade --auto-upgrade 2>/dev/null || true
    ~/.acme.sh/acme.sh --register-account -m "admin@${PRIMARY_DOMAIN}" 2>/dev/null || true

    log_info "SSL 证书初始化完成"
}

ssl_renewal() {
    log_info "续期 SSL 证书..."

    # 续期所有证书
    ~/.acme.sh/acme.sh --renew -d "$PRIMARY_DOMAIN" -d "www.${PRIMARY_DOMAIN}" --force
    ~/.acme.sh/acme.sh --renew -d "$MATRIX_DOMAIN" --force

    # 重启 Nginx 加载新证书
    log_info "重启 Nginx 加载新证书..."
    docker restart "$NGINX_CONTAINER"

    log_info "SSL 证书续期完成"
}

#=======================================
# 备份与恢复
#=======================================
backup() {
    local timestamp=$(date +%Y%m%d_%H%M%S)
    local backup_file="${BACKUP_DIR}/synapse_backup_${timestamp}.tar.gz"

    log_info "开始备份... (文件: $backup_file)"

    mkdir -p "$BACKUP_DIR"

    # 备份数据库
    log_info "备份 PostgreSQL 数据库..."
    docker exec "$DB_CONTAINER" pg_dump -U synapse synapse_test > "${BACKUP_DIR}/database_${timestamp}.sql"

    # 备份 Synapse 数据
    log_info "备份 Synapse 数据..."
    tar -czf "${BACKUP_DIR}/synapse_data_${timestamp}.tar.gz" \
        -C "$(dirname "$SYNAPSE_CONFIG_DIR")" \
        "$(basename "$SYNAPSE_CONFIG_DIR")" \
        synapse_data \
        synapse_media \
        synapse_uploads 2>/dev/null || \
    tar -czf "${BACKUP_DIR}/synapse_config_${timestamp}.tar.gz" \
        -C "$(dirname "$SYNAPSE_CONFIG_DIR")" \
        "$(basename "$SYNAPSE_CONFIG_DIR")" 2>/dev/null || true

    # 创建完整备份
    tar -czf "$backup_file" \
        -C "$BACKUP_DIR" \
        "database_${timestamp}.sql" \
        "synapse_config_${timestamp}.tar.gz" 2>/dev/null || \
    tar -czf "$backup_file" \
        -C "$BACKUP_DIR" \
        "database_${timestamp}.sql" 2>/dev/null || true

    # 清理临时文件
    rm -f "${BACKUP_DIR}/database_${timestamp}.sql"
    rm -f "${BACKUP_DIR}/synapse_config_${timestamp}.tar.gz"

    # 保留最近 7 天的备份
    find "$BACKUP_DIR" -name "synapse_backup_*.tar.gz" -mtime +7 -delete

    log_info "备份完成: $backup_file"
}

restore() {
    local backup_file="$1"

    if [ -z "$backup_file" ]; then
        log_error "请指定备份文件"
        log_info "可用备份:"
        ls -lh "$BACKUP_DIR"/synapse_backup_*.tar.gz 2>/dev/null || echo "无备份文件"
        return 1
    fi

    if [ ! -f "$backup_file" ]; then
        log_error "备份文件不存在: $backup_file"
        return 1
    fi

    log_info "从 $backup_file 恢复..."

    # 停止服务
    stop_services

    # 解压备份
    local temp_dir=$(mktemp -d)
    tar -xzf "$backup_file" -C "$temp_dir"

    # 恢复数据库
    if [ -f "$temp_dir"/*.sql ]; then
        log_info "恢复 PostgreSQL 数据库..."
        docker exec -i "$DB_CONTAINER" psql -U synapse synapse_test < "$temp_dir"/*.sql
    fi

    # 恢复配置
    if [ -d "$temp_dir"/*/homeserver.yaml ]; then
        log_info "恢复 Synapse 配置..."
        local config_backup=$(find "$temp_dir" -name "homeserver.yaml" -o -name "*.tar.gz" | head -1)
        if [[ "$config_backup" == *.tar.gz ]]; then
            tar -xzf "$config_backup" -C "$(dirname "$SYNAPSE_CONFIG_DIR")"
        fi
    fi

    # 清理临时目录
    rm -rf "$temp_dir"

    # 启动服务
    start_services

    log_info "恢复完成"
}

#=======================================
# 服务扩展
#=======================================
scale_services() {
    local synapse_replicas="${1:-1}"

    log_info "扩展 Synapse 服务到 $synapse_replicas 个副本..."

    cd "$DATA_DIR"

    if command -v docker-compose &> /dev/null; then
        docker-compose up -d --scale synapse="$synapse_replicas"
    else
        docker compose up -d --scale synapse="$synapse_replicas"
    fi

    log_info "Synapse 服务已扩展到 $synapse_replicas 个副本"
}

#=======================================
# 域名配置验证
#=======================================
verify_domain_config() {
    log_info "验证域名配置..."

    local checks_passed=0
    local checks_failed=0

    # 检查 DNS 解析
    log_info "检查 DNS 解析..."
    if command -v dig &> /dev/null; then
        local matrix_ip=$(dig +short "$MATRIX_DOMAIN" A 2>/dev/null)
        local matrix_aaaa=$(dig +short "$MATRIX_DOMAIN" AAAA 2>/dev/null)

        if [ -n "$matrix_ip" ]; then
            log_info "[✓] $MATRIX_DOMAIN -> $matrix_ip (A)"
            ((checks_passed++))
        else
            log_error "[✗] $MATRIX_DOMAIN 无 A 记录"
            ((checks_failed++))
        fi
    elif command -v nslookup &> /dev/null; then
        if nslookup "$MATRIX_DOMAIN" &>/dev/null; then
            log_info "[✓] DNS 解析正常"
            ((checks_passed++))
        else
            log_error "[✗] DNS 解析失败"
            ((checks_failed++))
        fi
    else
        log_warn "无法检查 DNS (dig/nslookup 未安装)"
    fi

    # 检查防火墙/端口
    log_info "检查端口可达性..."
    if command -v nc &> /dev/null; then
        if nc -zvw5 127.0.0.1 443 2>/dev/null; then
            log_info "[✓] 443 端口可达"
            ((checks_passed++))
        else
            log_warn "[!] 443 端口不可达 (可能是防火墙配置)"
        fi
    fi

    # 检查 .well-known 配置
    log_info "检查 .well-known 配置..."
    if curl -sf "http://${PRIMARY_DOMAIN}/.well-known/matrix/server" 2>/dev/null | grep -q "matrix"; then
        log_info "[✓] .well-known/matrix/server 配置正确"
        ((checks_passed++))
    else
        log_warn "[!] .well-known/matrix/server 配置可能不正确"
    fi

    echo ""
    log_info "域名配置检查: $checks_passed 通过, $checks_failed 失败"
}

#=======================================
# 使用帮助
#=======================================
show_help() {
    echo "======================================="
    echo "Matrix Synapse 生产环境部署脚本"
    echo "======================================="
    echo ""
    echo "用法: $0 <命令> [参数]"
    echo ""
    echo "服务管理:"
    echo "  start           启动所有服务"
    echo "  stop            停止所有服务"
    echo "  restart         重启所有服务"
    echo "  status          查看服务状态"
    echo "  logs [服务]     查看日志 (可选: nginx, synapse, db, redis)"
    echo "  health          执行健康检查"
    echo ""
    echo "SSL 证书管理:"
    echo "  ssl-init        初始化并申请 SSL 证书"
    echo "  ssl-renewal     续期 SSL 证书"
    echo ""
    echo "备份恢复:"
    echo "  backup          执行完整备份"
    echo "  restore <文件>  从备份文件恢复"
    echo ""
    echo "扩展:"
    echo "  scale [副本数]  扩展 Synapse 服务 (默认: 1)"
    echo ""
    echo "维护:"
    echo "  verify          验证域名配置"
    echo "  check           检查依赖和配置"
    echo ""
    echo "示例:"
    echo "  $0 start                    # 启动所有服务"
    echo "  $0 logs synapse             # 查看 Synapse 日志"
    echo "  $0 ssl-init                 # 初始化 SSL 证书"
    echo "  $0 backup                   # 执行备份"
    echo "  $0 restore backup.tar.gz    # 从备份恢复"
    echo ""
}

#=======================================
# 主程序
#=======================================
main() {
    local command="${1:-help}"

    # 检查是否在正确目录
    if [ ! -d "$DATA_DIR" ] && [ "$command" != "help" ]; then
        log_warn "数据目录不存在: $DATA_DIR"
        log_info "将使用默认配置创建..."
    fi

    case "$command" in
        help|--help|-h)
            show_help
            ;;
        start)
            check_dependencies
            start_services
            ;;
        stop)
            stop_services
            ;;
        restart)
            restart_services
            ;;
        status)
            show_status
            ;;
        logs)
            show_logs "$2" "${3:-100}"
            ;;
        health)
            health_check
            ;;
        ssl-init)
            ssl_init
            ;;
        ssl-renewal|ssl-renew)
            ssl_renewal
            ;;
        backup)
            backup
            ;;
        restore)
            restore "$2"
            ;;
        scale)
            scale_services "$2"
            ;;
        verify)
            verify_domain_config
            ;;
        check)
            check_dependencies
            verify_domain_config
            ;;
        *)
            log_error "未知命令: $command"
            show_help
            exit 1
            ;;
    esac
}

main "$@"
