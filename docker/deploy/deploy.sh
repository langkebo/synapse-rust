#!/bin/bash
# =============================================================================
# synapse-rust 一键部署脚本
# =============================================================================
# 使用方法:
#   1. 复制 .env.example 为 .env
#   2. 修改 .env 中的配置项
#   3. 运行 ./deploy.sh
# =============================================================================

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 日志函数
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
cd "$SCRIPT_DIR"

# 显示横幅
show_banner() {
    echo ""
    echo "=========================================="
    echo "  synapse-rust Docker 部署脚本"
    echo "=========================================="
    echo ""
}

# 检查依赖
check_dependencies() {
    log_info "检查依赖..."
    
    local missing_deps=()
    
    if ! command -v docker &> /dev/null; then
        missing_deps+=("docker")
    fi
    
    if ! command -v docker-compose &> /dev/null && ! docker compose version &> /dev/null; then
        missing_deps+=("docker-compose")
    fi
    
    if [ ${#missing_deps[@]} -ne 0 ]; then
        log_error "缺少以下依赖: ${missing_deps[*]}"
        log_info "请安装 Docker 和 Docker Compose 后重试"
        exit 1
    fi
    
    log_success "依赖检查通过"
}

# 检查环境变量文件
check_env_file() {
    log_info "检查环境变量配置..."
    
    if [ ! -f ".env" ]; then
        log_warning ".env 文件不存在"
        
        if [ -f ".env.example" ]; then
            log_info "从 .env.example 创建 .env 文件..."
            cp .env.example .env
            
            # 自动生成安全密钥
            log_info "自动生成安全密钥..."
            if [ -f "scripts/generate-secrets.sh" ]; then
                chmod +x scripts/generate-secrets.sh
                ./scripts/generate-secrets.sh all > /dev/null
                log_success "安全密钥已自动生成并写入 .env 文件"
            else
                log_warning "密钥生成脚本不存在，请手动配置密钥"
            fi
            
            log_warning "请编辑 .env 文件配置您的服务器信息后重新运行"
            log_info "必须修改的配置项:"
            log_info "  - SERVER_NAME: 您的服务器名称"
            log_info "  - PUBLIC_BASEURL: 公开访问 URL"
            log_info "  (密钥已自动生成，无需修改)"
            exit 1
        else
            log_error ".env.example 文件不存在，请检查部署包完整性"
            exit 1
        fi
    fi
    
    # 加载环境变量
    source .env
    
    # 检查必要的环境变量
    local required_vars=("SERVER_NAME" "PUBLIC_BASEURL" "POSTGRES_PASSWORD" "ADMIN_SHARED_SECRET" "JWT_SECRET")
    local missing_vars=()
    
    for var in "${required_vars[@]}"; do
        if [ -z "${!var}" ] || [[ "${!var}" == *"your-"* ]] || [[ "${!var}" == *"change-me"* ]]; then
            missing_vars+=("$var")
        fi
    done
    
    if [ ${#missing_vars[@]} -ne 0 ]; then
        log_error "以下环境变量需要配置: ${missing_vars[*]}"
        
        # 提供自动生成密钥的选项
        local secret_vars=("POSTGRES_PASSWORD" "ADMIN_SHARED_SECRET" "JWT_SECRET" "REGISTRATION_SHARED_SECRET")
        local need_generate=false
        
        for var in "${secret_vars[@]}"; do
            if [[ " ${missing_vars[*]} " =~ " ${var} " ]]; then
                need_generate=true
                break
            fi
        done
        
        if [ "$need_generate" = true ]; then
            log_info "检测到密钥未配置，是否自动生成? (y/n)"
            read -r answer
            if [ "$answer" = "y" ] || [ "$answer" = "Y" ]; then
                log_info "生成安全密钥..."
                ./scripts/generate-secrets.sh all > /dev/null
                source .env
                log_success "密钥已生成，请重新检查配置"
            fi
        fi
        
        log_info "请编辑 .env 文件后重新运行"
        exit 1
    fi
    
    log_success "环境变量配置检查通过"
}

# 创建必要的目录
create_directories() {
    log_info "创建必要的目录..."
    
    mkdir -p ssl
    mkdir -p migrations
    
    log_success "目录创建完成"
}

# 复制迁移文件
copy_migrations() {
    log_info "检查数据库迁移文件..."
    
    # 检查 migrations 目录是否为空
    if [ -z "$(ls -A migrations 2>/dev/null)" ]; then
        log_warning "migrations 目录为空"
        log_info "请确保 Docker 镜像包含迁移文件，或手动复制迁移文件到 migrations 目录"
    else
        log_success "迁移文件已就绪 ($(ls migrations | wc -l) 个文件)"
    fi
}

# 准备 Docker 镜像
prepare_images() {
    log_info "检查 Docker 镜像..."
    
    if docker image inspect synapse-rust:local &> /dev/null; then
        log_success "本地镜像 synapse-rust:local 已存在"
    elif docker image inspect vmuser232922/mysynapse:latest &> /dev/null; then
        log_info "从 Docker Hub 镜像创建本地标签..."
        docker tag vmuser232922/mysynapse:latest synapse-rust:local
        log_success "镜像标签创建完成"
    else
        log_warning "未找到本地镜像 synapse-rust:local"
        log_info "尝试从 Docker Hub 拉取 vmuser232922/mysynapse:latest..."
        if docker pull vmuser232922/mysynapse:latest 2>/dev/null; then
            docker tag vmuser232922/mysynapse:latest synapse-rust:local
            log_success "镜像拉取并标记完成"
        else
            log_error "无法获取 Docker 镜像"
            log_info "请先构建镜像:"
            log_info "  cd /path/to/synapse-rust && docker build -f docker/Dockerfile --platform linux/amd64 -t synapse-rust:local ."
            log_info "或从 Docker Hub 拉取:"
            log_info "  docker pull vmuser232922/mysynapse:latest && docker tag vmuser232922/mysynapse:latest synapse-rust:local"
            exit 1
        fi
    fi
    
    log_info "拉取基础服务镜像..."
    docker-compose pull postgres redis nginx 2>/dev/null || true
    
    log_success "镜像准备完成"
}

# 停止现有容器
stop_containers() {
    log_info "停止现有容器..."
    
    docker-compose down --remove-orphans 2>/dev/null || true
    
    log_success "容器已停止"
}

# 启动服务
start_services() {
    log_info "启动服务..."
    
    # 首先启动数据库
    docker-compose up -d postgres redis
    
    log_info "等待数据库就绪..."
    sleep 10
    
    # 检查数据库健康状态
    local max_retries=30
    local retry=0
    
    while [ $retry -lt $max_retries ]; do
        if docker-compose exec -T postgres pg_isready -U "${POSTGRES_USER:-postgres}" > /dev/null 2>&1; then
            log_success "数据库已就绪"
            break
        fi
        retry=$((retry + 1))
        sleep 2
    done
    
    if [ $retry -eq $max_retries ]; then
        log_error "数据库启动超时"
        exit 1
    fi
    
    # 直接在 PostgreSQL 容器中执行迁移
    log_info "执行数据库迁移..."
    
    # 创建 schema_migrations 表
    docker-compose exec -T postgres psql -U "${POSTGRES_USER:-postgres}" -d "${POSTGRES_DB:-synapse}" <<EOF
CREATE TABLE IF NOT EXISTS schema_migrations (
    id BIGSERIAL PRIMARY KEY,
    version TEXT NOT NULL,
    name TEXT,
    checksum TEXT,
    applied_ts BIGINT,
    execution_time_ms BIGINT,
    success BOOLEAN NOT NULL DEFAULT TRUE,
    description TEXT,
    executed_at TIMESTAMPTZ DEFAULT NOW(),
    CONSTRAINT uq_schema_migrations_version UNIQUE (version)
);
EOF
    
    # 执行所有迁移文件（按文件名排序）
    local migration_count=0
    local failed_count=0
    for sql_file in $(ls migrations/*.sql 2>/dev/null | sort); do
        if [ -f "$sql_file" ]; then
            local filename=$(basename "$sql_file")
            local version="${filename%.sql}"
            
            local applied=$(docker-compose exec -T postgres psql -U "${POSTGRES_USER:-postgres}" -d "${POSTGRES_DB:-synapse}" -tAc "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = '$version' AND success = true)" 2>/dev/null | tr -d '[:space:]')
            
            if [ "$applied" != "t" ]; then
                log_info "应用迁移: $filename"
                local start_time=$(python3 -c "import time; print(int(time.time() * 1000))")
                
                local migrate_output=$(docker-compose exec -T postgres psql -U "${POSTGRES_USER:-postgres}" -d "${POSTGRES_DB:-synapse}" -v ON_ERROR_STOP=1 < "$sql_file" 2>&1)
                local migrate_status=$?
                
                if [ $migrate_status -eq 0 ]; then
                    local end_time=$(python3 -c "import time; print(int(time.time() * 1000))")
                    local duration=$((end_time - start_time))
                    docker-compose exec -T postgres psql -U "${POSTGRES_USER:-postgres}" -d "${POSTGRES_DB:-synapse}" -c "INSERT INTO schema_migrations (version, name, applied_ts, execution_time_ms, success, description) VALUES ('$version', '$filename', EXTRACT(EPOCH FROM NOW()) * 1000, $duration, true, 'Applied successfully') ON CONFLICT (version) DO UPDATE SET success = true, applied_ts = EXTRACT(EPOCH FROM NOW()) * 1000, execution_time_ms = $duration" > /dev/null 2>&1
                    migration_count=$((migration_count + 1))
                    log_success "迁移成功: $filename (${duration}ms)"
                else
                    local end_time=$(python3 -c "import time; print(int(time.time() * 1000))")
                    local duration=$((end_time - start_time))
                    docker-compose exec -T postgres psql -U "${POSTGRES_USER:-postgres}" -d "${POSTGRES_DB:-synapse}" -c "INSERT INTO schema_migrations (version, name, applied_ts, execution_time_ms, success, description) VALUES ('$version', '$filename', EXTRACT(EPOCH FROM NOW()) * 1000, $duration, false, 'Migration failed') ON CONFLICT (version) DO UPDATE SET success = false, applied_ts = EXTRACT(EPOCH FROM NOW()) * 1000" > /dev/null 2>&1
                    failed_count=$((failed_count + 1))
                    log_error "迁移失败: $filename"
                    log_error "错误信息: $migrate_output"
                fi
            else
                log_info "跳过已应用迁移: $filename"
            fi
        fi
    done
    
    if [ $failed_count -gt 0 ]; then
        log_warning "数据库迁移完成 (成功: $migration_count, 失败: $failed_count)"
    else
        log_success "数据库迁移完成 (应用了 $migration_count 个迁移)"
    fi
    
    # 验证必需表是否存在
    log_info "验证数据库表结构..."
    local required_tables=("users" "rooms" "events" "devices" "room_memberships" "access_tokens" "refresh_tokens" "presence" "event_relations" "federation_signing_keys" "rate_limits" "server_notices" "user_notification_settings" "widgets" "secure_key_backups" "secure_backup_session_keys" "space_members" "room_summary_members" "federation_cache" "feature_flags")
    local missing_tables=()
    
    for table in "${required_tables[@]}"; do
        local exists=$(docker-compose exec -T postgres psql -U "${POSTGRES_USER:-postgres}" -d "${POSTGRES_DB:-synapse}" -tAc "SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_schema = 'public' AND table_name = '$table');" 2>/dev/null | tr -d '[:space:]')
        if [ "$exists" != "t" ]; then
            missing_tables+=("$table")
        fi
    done
    
    if [ ${#missing_tables[@]} -gt 0 ]; then
        log_error "缺少以下必需表: ${missing_tables[*]}"
        log_error "请检查迁移文件是否完整"
        exit 1
    fi
    
    log_success "数据库表结构验证通过 (${#required_tables[@]} 个表)"
    
    # 启动主应用
    log_info "启动 Synapse 应用..."
    docker-compose up -d synapse
    
    # 等待应用就绪
    log_info "等待应用启动..."
    sleep 5
    
    local app_retries=30
    local app_retry=0
    
    while [ $app_retry -lt $app_retries ]; do
        if curl -sf "http://localhost:${SYNAPSE_PORT:-8008}/health" > /dev/null 2>&1; then
            log_success "Synapse 应用已就绪"
            break
        fi
        app_retry=$((app_retry + 1))
        sleep 2
    done
    
    if [ $app_retry -eq $app_retries ]; then
        log_warning "应用健康检查超时，请检查日志"
    fi
    
    # 启动 Nginx
    log_info "启动 Nginx..."
    docker-compose up -d nginx
    
    log_success "所有服务已启动"
}

# 显示服务状态
show_status() {
    echo ""
    log_info "服务状态:"
    docker-compose ps
    echo ""
}

# 显示访问信息
show_access_info() {
    echo ""
    echo "=========================================="
    echo "  部署完成!"
    echo "=========================================="
    echo ""
    echo "服务器名称: ${SERVER_NAME}"
    echo "公开 URL: ${PUBLIC_BASEURL}"
    echo ""
    echo "访问地址:"
    echo "  HTTP:  http://localhost:${HTTP_PORT:-80}"
    echo "  HTTPS: https://localhost:${HTTPS_PORT:-443}"
    echo "  API:   ${PUBLIC_BASEURL}/_matrix/client/versions"
    echo ""
    echo "管理命令:"
    echo "  查看日志:   docker-compose logs -f synapse"
    echo "  停止服务:   docker-compose down"
    echo "  重启服务:   docker-compose restart"
    echo "  查看状态:   docker-compose ps"
    echo ""
    echo "注册管理员:"
    echo "  使用 ADMIN_SHARED_SECRET 注册管理员账户"
    echo "  参考文档: https://matrix.org/docs/guides/admins"
    echo ""
}

# 主函数
main() {
    show_banner
    check_dependencies
    check_env_file
    create_directories
    copy_migrations
    prepare_images
    stop_containers
    start_services
    show_status
    show_access_info
}

# 运行主函数
main "$@"
