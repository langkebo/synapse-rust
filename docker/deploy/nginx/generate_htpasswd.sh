#!/usr/bin/env bash
#
# 生成 nginx Basic Auth 密码文件
#
# 用途：为 Prometheus 安全加固创建 htpasswd 文件
# 用法:
#   ./generate_htpasswd.sh
#   ./generate_htpasswd.sh admin your_password
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
AUTH_DIR="${SCRIPT_DIR}/nginx/auth"
HTPASSWD_FILE="${AUTH_DIR}/.htpasswd"

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

print_header() {
  echo -e "\n${BLUE}================================${NC}"
  echo -e "${BLUE}$1${NC}"
  echo -e "${BLUE}================================${NC}\n"
}

print_success() {
  echo -e "${GREEN}✓ $1${NC}"
}

print_error() {
  echo -e "${RED}✗ $1${NC}"
}

# ========== 主逻辑 ==========
main() {
  local username="${1:-}"
  local password="${2:-}"
  
  print_header "创建 nginx Basic Auth 密码文件"
  
  # 检查是否安装了 Apache 工具
  if ! command -v htpasswd &> /dev/null; then
    print_error "htpasswd 命令不存在"
    echo "请安装 Apache HTTP 服务器工具："
    echo "  brew install httpd"
    echo ""
    echo "或者使用 Docker 容器生成："
    echo "  docker run --rm -it -v \"$(pwd)/nginx/auth:/htpasswd\" httpd:2.4-alpine htpasswd -Bc /htpasswd/.htpasswd admin"
    exit 1
  fi
  
  # 创建目录
  mkdir -p "${AUTH_DIR}"
  
  # 如果提供了用户名和密码
  if [[ -n "${username}" ]] && [[ -n "${password}" ]]; then
    print_header "创建用户：${username}"
    htpasswd -Bbc "${HTPASSWD_FILE}" "${username}" "${password}"
    print_success "用户 ${username} 已创建，文件保存至：${HTPASSWD_FILE}"
    chmod 600 "${HTPASSWD_FILE}"
  else
    # 交互式创建
    echo -e "${YELLOW}交互模式：请手动输入用户名和密码${NC}\n"
    
    if [[ ! -f "${HTPASSWD_FILE}" ]]; then
      read -p "请输入第一个用户名 (默认 admin): " username
      username="${username:-admin}"
      
      read -sp "请输入密码：" password
      echo
      
      read -sp "请再次输入密码确认：" password_confirm
      echo
      
      if [[ "${password}" != "${password_confirm}" ]]; then
        print_error "两次输入的密码不一致"
        exit 1
      fi
      
      htpasswd -Bbc "${HTPASSWD_FILE}" "${username}" "${password}"
      print_success "用户 ${username} 已创建"
    else
      print_header "添加新用户"
      echo -e "当前已有用户："
      cut -d: -f1 "${HTPASSWD_FILE}" | while read user; do
        echo "  - ${user}"
      done
      echo
      
      read -p "请输入新用户名的 (留空退出): " username
      if [[ -z "${username}" ]]; then
        print_success "操作取消"
        exit 0
      fi
      
      read -sp "请输入密码：" password
      echo
      
      read -sp "请再次输入密码确认：" password_confirm
      echo
      
      if [[ "${password}" != "${password_confirm}" ]]; then
        print_error "两次输入的密码不一致"
        exit 1
      fi
      
      htpasswd -Bb "${HTPASSWD_FILE}" "${username}" "${password}"
      print_success "用户 ${username} 已添加"
    fi
    
    chmod 600 "${HTPASSWD_FILE}"
  fi
  
  # 验证文件
  print_header "验证配置"
  echo "文件位置：${HTPASSWD_FILE}"
  echo "文件权限：$(stat -f "%Sp" "${HTPASSWD_FILE}" | tail -c 5)"
  echo "用户列表:"
  cut -d: -f1 "${HTPASSWD_FILE}" | while read user; do
    echo "  - ${user}"
  done
  
  echo -e "\n${BLUE}下一步:${NC}"
  echo "1. 重启监控栈以启用认证:"
  echo "   /opt/homebrew/bin/docker compose -f docker/docker-compose.monitoring.yml up -d nginx-proxy"
  echo ""
  echo "2. 测试认证是否生效:"
  echo "   curl -u admin:你的密码 http://localhost:8081/prometheus/api/v1/query?query=up"
  echo ""
  echo "3. 验证无认证被拒绝:"
  echo "   curl http://localhost:8081/prometheus/api/v1/query?query=up"
  echo "   (应该返回 HTTP 401)"
}

# 启动
main "$@"
