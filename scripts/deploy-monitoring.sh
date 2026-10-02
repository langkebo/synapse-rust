#!/bin/bash
# =============================================================================
# Synapse-Rust 监控栈一键部署脚本
# =============================================================================
# 用法:
#   ./scripts/deploy-monitoring.sh [start|stop|restart|logs|validate|help]
# =============================================================================

set -euo pipefail

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 脚本所在目录
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Docker Compose 配置文件
COMPOSE_FILES=(
    "$PROJECT_ROOT/docker/docker-compose.yml"
    "$PROJECT_ROOT/docker/docker-compose.monitoring.yml"
)

# 环境变量文件
ENV_FILE="$PROJECT_ROOT/.env.monitoring.local"

# -----------------------------------------------------------------------------
# 函数定义
# -----------------------------------------------------------------------------

usage() {
    echo -e "${BLUE}Synapse-Rust 监控栈部署脚本${NC}"
    echo ""
    echo "用法：$0 [COMMAND]"
    echo ""
    echo "可用命令:"
    echo "  start      启动监控栈 (Prometheus + Grafana + Alertmanager)"
    echo "  stop       停止监控栈"
    echo "  restart    重启监控栈"
    echo "  logs       查看监控栈日志"
    echo "  validate   验证配置文件语法"
    echo "  health     检查监控栈健康状态"
    echo "  clean      清理所有监控栈容器和数据卷"
    echo "  help       显示此帮助信息"
    echo ""
    echo "示例:"
    echo "  $0 start              # 启动监控栈"
    echo "  $0 logs prometheus    # 查看 Prometheus 日志"
    echo "  $0 health             # 检查服务健康状态"
}

check_prerequisites() {
    echo -e "${BLUE}检查前置条件...${NC}"

    # 检查 Docker
    if ! command -v docker &>/dev/null; then
        echo -e "${RED}❌ Docker 未安装，请先安装 Docker${NC}"
        exit 1
    fi

    # 检查 Docker Compose
    if ! command -v docker-compose &>/dev/null; then
        echo -e "${RED}❌ Docker Compose 未安装，请先安装 Docker Compose${NC}"
        exit 1
    fi

    # 检查配置文件
    if [[ ! -f "$PROJECT_ROOT/monitoring/prometheus/prometheus.yml" ]]; then
        echo -e "${RED}❌ Prometheus 配置文件不存在${NC}"
        exit 1
    fi

    if [[ ! -f "$PROJECT_ROOT/monitoring/alertmanager/alertmanager.yml" ]]; then
        echo -e "${RED}❌ Alertmanager 配置文件不存在${NC}"
        exit 1
    fi

    if [[ ! -f "$PROJECT_ROOT/monitoring/grafana/datasources/datasources.yml" ]]; then
        echo -e "${RED}❌ Grafana 数据源配置文件不存在${NC}"
        exit 1
    fi

    echo -e "${GREEN}✓ 前置条件检查通过${NC}"
}

setup_environment() {
    echo -e "${BLUE}设置环境变量...${NC}"

    # 如果没有本地的 .env.monitoring.local，从示例复制
    if [[ ! -f "$ENV_FILE" ]]; then
        echo -e "${YELLOW}⚠️  未找到 .env.monitoring.local，从示例创建...${NC}"
        cp "$PROJECT_ROOT/.env.monitoring.example" "$ENV_FILE"
        echo -e "${YELLOW}⚠️  请编辑 $ENV_FILE 配置敏感信息（密码、Webhook 等）${NC}"
    fi

    # 加载环境变量
    set -a
    source "$ENV_FILE"
    set +a

    echo -e "${GREEN}✓ 环境变量已加载${NC}"
}

start_monitoring() {
    check_prerequisites
    setup_environment

    echo -e "${BLUE}启动监控栈...${NC}"
    echo ""

    cd "$PROJECT_ROOT"

    # 启动基础服务（如果还没启动）
    echo -e "${BLUE}检查基础服务状态...${NC}"
    if ! docker-compose -f docker/docker-compose.yml ps | grep -q "Up"; then
        echo -e "${YELLOW}⚠️  基础服务未运行，先启动基础服务...${NC}"
        docker-compose -f docker/docker-compose.yml up -d
        echo -e "${GREEN}✓ 基础服务已启动${NC}"
        echo ""
        echo -e "${YELLOW}⏳ 等待基础服务就绪 (30 秒)...${NC}"
        sleep 30
    fi

    # 启动监控栈
    echo -e "${BLUE}启动 Prometheus + Grafana + Alertmanager...${NC}"
    ENVIRONMENT="${ENVIRONMENT:-production}" \
        docker-compose -f docker/docker-compose.yml \
        -f docker/docker-compose.monitoring.yml \
        up -d

    echo ""
    echo -e "${GREEN}========================================${NC}"
    echo -e "${GREEN}✓ 监控栈启动成功！${NC}"
    echo -e "${GREEN}========================================${NC}"
    echo ""
    echo -e "${BLUE}访问地址:${NC}"
    echo "  - Prometheus: http://localhost:9090"
    echo "  - Grafana:    http://localhost:9091 (admin/admin123)"
    echo "  - Alertmanager: http://localhost:9093"
    echo ""
    echo -e "${YELLOW}提示:${NC}"
    echo "  - 首次启动 Grafana 需要 1-2 分钟初始化"
    echo "  - Prometheus 需要 5-10 分钟开始显示完整数据"
    echo "  - 可以通过 '$0 logs <service>' 查看服务日志"
    echo ""
}

stop_monitoring() {
    check_prerequisites
    setup_environment

    echo -e "${BLUE}停止监控栈...${NC}"

    cd "$PROJECT_ROOT"

    docker-compose -f docker/docker-compose.yml \
        -f docker/docker-compose.monitoring.yml \
        down

    echo -e "${GREEN}✓ 监控栈已停止${NC}"
}

restart_monitoring() {
    check_prerequisites
    setup_environment

    echo -e "${BLUE}重启监控栈...${NC}"

    cd "$PROJECT_ROOT"

    docker-compose -f docker/docker-compose.yml \
        -f docker/docker-compose.monitoring.yml \
        restart

    echo -e "${GREEN}✓ 监控栈已重启${NC}"
    echo ""
    echo "  Prometheus: http://localhost:9090"
    echo "  Grafana:    http://localhost:9091"
    echo "  Alertmanager: http://localhost:9093"
}

show_logs() {
    local service=${1:-}

    check_prerequisites
    setup_environment

    cd "$PROJECT_ROOT"

    if [[ -z "$service" ]]; then
        docker-compose -f docker/docker-compose.yml \
            -f docker/docker-compose.monitoring.yml \
            logs -f
    else
        docker-compose -f docker/docker-compose.yml \
            -f docker/docker-compose.monitoring.yml \
            logs -f "$service"
    fi
}

check_health() {
    echo -e "${BLUE}检查监控栈健康状态...${NC}"
    echo ""

    # Prometheus
    echo -n "  Prometheus (9090): "
    if curl -s --max-time 5 http://localhost:9090/-/healthy >/dev/null 2>&1; then
        echo -e "${GREEN}✓ Healthy${NC}"
    else
        echo -e "${RED}✗ Unhealthy${NC}"
    fi

    # Grafana
    echo -n "  Grafana (9091): "
    if curl -s --max-time 5 http://localhost:9091/api/health >/dev/null 2>&1; then
        echo -e "${GREEN}✓ Healthy${NC}"
    else
        echo -e "${RED}✗ Unhealthy${NC}"
    fi

    # Alertmanager
    echo -n "  Alertmanager (9093): "
    if curl -s --max-time 5 http://localhost:9093/-/healthy >/dev/null 2>&1; then
        echo -e "${GREEN}✓ Healthy${NC}"
    else
        echo -e "${RED}✗ Unhealthy${NC}"
    fi

    echo ""
}

validate_config() {
    echo -e "${BLUE}验证配置文件语法...${NC}"
    echo ""

    # 检查文件存在性
    echo "📁 检查配置文件完整性："

    local files_ok=true

    if [[ -f "$PROJECT_ROOT/monitoring/prometheus/prometheus.yml" ]]; then
        echo -e "  ✓ Prometheus config: File exists"
    else
        echo -e "  ✗ Prometheus config: Missing"
        files_ok=false
    fi

    if [[ -f "$PROJECT_ROOT/monitoring/alertmanager/alertmanager.yml" ]]; then
        echo -e "  ✓ Alertmanager config: File exists"
    else
        echo -e "  ✗ Alertmanager config: Missing"
        files_ok=false
    fi

    if [[ -f "$PROJECT_ROOT/monitoring/grafana/datasources/datasources.yml" ]]; then
        echo -e "  ✓ Grafana datasources: File exists"
    else
        echo -e "  ✗ Grafana datasources: Missing"
        files_ok=false
    fi

    if [[ -f "$PROJECT_ROOT/monitoring/grafana/dashboards/providers.yml" ]]; then
        echo -e "  ✓ Grafana providers: File exists"
    else
        echo -e "  ✗ Grafana providers: Missing"
        files_ok=false
    fi

    if [[ -f "$PROJECT_ROOT/monitoring/grafana/dashboards/grafana-dashboard-slo.json" ]] &&
        [[ -f "$PROJECT_ROOT/monitoring/grafana/dashboards/grafana-dashboard-business.json" ]]; then
        echo -e "  ✓ Grafana dashboards: Files exist"
    else
        echo -e "  ✗ Grafana dashboards: Missing files"
        files_ok=false
    fi

    echo ""

    # 尝试使用 promtool 验证（如果可用）
    if command -v promtool &>/dev/null; then
        echo "🔧 使用 promtool 验证 Prometheus 配置："
        if promtool check config "$PROJECT_ROOT/monitoring/prometheus/prometheus.yml"; then
            echo -e "${GREEN}✓ Prometheus config: Valid${NC}"
        else
            echo -e "${RED}✗ Prometheus config: Invalid${NC}"
            files_ok=false
        fi
    else
        echo "ℹ️  promtool 未安装，跳过语法验证（Docker 环境中会自动验证）"
    fi

    # 尝试使用 amtool 验证（如果可用）
    if command -v amtool &>/dev/null; then
        echo "🔧 使用 amtool 验证 Alertmanager 配置："
        if amtool check-config "$PROJECT_ROOT/monitoring/alertmanager/alertmanager.yml" >/dev/null 2>&1; then
            echo -e "${GREEN}✓ Alertmanager config: Valid${NC}"
        else
            echo -e "${RED}✗ Alertmanager config: Invalid${NC}"
            files_ok=false
        fi
    else
        echo "ℹ️  amtool 未安装，跳过语法验证（Docker 环境中会自动验证）"
    fi

    echo ""

    # 检查 JSON 格式
    if command -v python3 &>/dev/null; then
        echo "🔧 验证 JSON 格式："
        for json_file in \
            "$PROJECT_ROOT/monitoring/grafana/dashboards/grafana-dashboard-slo.json" \
            "$PROJECT_ROOT/monitoring/grafana/dashboards/grafana-dashboard-business.json"; do

            if [[ -f "$json_file" ]]; then
                if python3 -m json.tool "$json_file" >/dev/null 2>&1; then
                    echo -e "  ✓ $(basename "$json_file"): Valid JSON"
                else
                    echo -e "  ✗ $(basename "$json_file"): Invalid JSON"
                    files_ok=false
                fi
            fi
        done
    fi

    echo ""

    if [[ "$files_ok" == "true" ]]; then
        echo -e "${GREEN}✓ 所有配置文件完整且有效${NC}"
        echo ""
        echo -e "${YELLOW}提示:${NC}"
        echo "  • 如需在 Docker 中启动，请运行：./scripts/deploy-monitoring.sh start"
        echo "  • 如需本地验证，请先安装 Docker 和 Docker Compose"
        return 0
    else
        echo -e "${RED}✗ 配置文件存在问题，请检查上述错误${NC}"
        return 1
    fi
}

clean_monitoring() {
    echo -e "${YELLOW}⚠️  警告：这将删除所有监控栈容器和数据卷！${NC}"
    read -p "确定要继续？(yes/no): " confirm

    if [[ "$confirm" != "yes" ]]; then
        echo -e "${YELLOW}取消操作${NC}"
        exit 0
    fi

    check_prerequisites

    cd "$PROJECT_ROOT"

    echo -e "${BLUE}停止并删除监控栈...${NC}"

    docker-compose -f docker/docker-compose.yml \
        -f docker/docker-compose.monitoring.yml \
        down -v

    echo -e "${GREEN}✓ 监控栈已清理${NC}"
}

# -----------------------------------------------------------------------------
# 主逻辑
# -----------------------------------------------------------------------------

case "${1:-help}" in
    start)
        start_monitoring
        ;;
    stop)
        stop_monitoring
        ;;
    restart)
        restart_monitoring
        ;;
    logs)
        show_logs "${2:-}"
        ;;
    health)
        check_health
        ;;
    validate)
        validate_config
        ;;
    clean)
        clean_monitoring
        ;;
    help | --help | -h)
        usage
        ;;
    *)
        echo -e "${RED}错误：未知命令 '$1'${NC}"
        echo ""
        usage
        exit 1
        ;;
esac
