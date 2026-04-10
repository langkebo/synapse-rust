#!/bin/bash
# =============================================================================
# 数据库迁移脚本
# =============================================================================
# 此脚本用于手动执行数据库迁移
# =============================================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

# 加载环境变量
if [ -f ".env" ]; then
    source .env
fi

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

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# 检查数据库连接
check_db_connection() {
    log_info "检查数据库连接..."
    
    local db_host="${POSTGRES_HOST:-postgres}"
    local db_port="${POSTGRES_PORT:-5432}"
    local db_user="${POSTGRES_USER:-synapse}"
    local db_name="${POSTGRES_DB:-synapse}"
    
    if docker-compose exec -T postgres pg_isready -U "$db_user" -d "$db_name" > /dev/null 2>&1; then
        log_success "数据库连接正常"
        return 0
    else
        log_error "无法连接到数据库"
        return 1
    fi
}

# 运行迁移
run_migrations() {
    log_info "运行数据库迁移..."
    
    # 使用 migrator 服务
    docker-compose up migrator --force-recreate --no-deps
    
    log_success "迁移完成"
}

# 检查迁移状态
check_migration_status() {
    log_info "检查迁移状态..."
    
    docker-compose exec -T postgres psql -U "${POSTGRES_USER:-synapse}" -d "${POSTGRES_DB:-synapse}" -c "SELECT * FROM schema_migrations ORDER BY version;" 2>/dev/null || true
}

# 主函数
main() {
    case "${1:-run}" in
        run)
            check_db_connection
            run_migrations
            check_migration_status
            ;;
        status)
            check_migration_status
            ;;
        check)
            check_db_connection
            ;;
        *)
            echo "用法: $0 {run|status|check}"
            exit 1
            ;;
    esac
}

main "$@"
