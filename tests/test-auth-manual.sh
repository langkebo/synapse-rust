#!/bin/bash

###############################################################################
# Synapse Rust 认证模块手动测试脚本
#
# 功能: 手动测试认证模块的各个API（用于调试）
# 版本: 1.0.0
# 创建日期: 2026-02-02
###############################################################################

set -e

# 配置
SERVER_URL="${SERVER_URL:-http://localhost:8008}"
SERVER_NAME="${SERVER_NAME:-localhost}"

# 颜色
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

# 辅助函数
print_section() {
    echo ""
    echo "========================================"
    echo "$1"
    echo "========================================"
}

print_request() {
    echo -e "${BLUE}请求:${NC} $1"
}

print_response() {
    echo -e "${GREEN}响应:${NC}"
    echo "$1" | jq . 2>/dev/null || echo "$1"
    echo ""
}

###############################################################################
# 测试函数
###############################################################################

# 1. 获取客户端版本
test_get_versions() {
    print_section "1. 获取客户端版本"
    print_request "GET /_matrix/client/versions"
    
    local response
    response=$(curl -s -X GET "$SERVER_URL/_matrix/client/versions")
    print_response "$response"
}

# 2. 检查用户名可用性
test_check_available() {
    local username="${1:-testuser_$(date +%s)}"
    
    print_section "2. 检查用户名可用性"
    print_request "GET /_matrix/client/r0/register/available?username=$username"
    
    local response
    response=$(curl -s -X GET "$SERVER_URL/_matrix/client/r0/register/available?username=$username")
    print_response "$response"
}

# 3. 用户注册
test_register() {
    local username="${1:-testuser_$(date +%s)}"
    local password="${2:-TestPassword123}"
    local displayname="${3:-Test User}"
    
    print_section "3. 用户注册"
    print_request "POST /_matrix/client/r0/register"
    echo "用户名: $username"
    echo "密码: $password"
    echo "显示名: $displayname"
    
    local data="{\"username\":\"$username\",\"password\":\"$password\",\"displayname\":\"$displayname\"}"
    local response
    response=$(curl -s -X POST -H "Content-Type: application/json" \
        -d "$data" "$SERVER_URL/_matrix/client/r0/register")
    print_response "$response"
    
    # 返回用户ID和令牌
    echo "$response"
}

# 4. 用户登录
test_login() {
    local username="${1:-testuser}"
    local password="${2:-TestPassword123}"
    
    print_section "4. 用户登录"
    print_request "POST /_matrix/client/r0/login"
    echo "用户名: $username"
    echo "密码: $password"
    
    local data="{\"user\":\"$username\",\"password\":\"$password\"}"
    local response
    response=$(curl -s -X POST -H "Content-Type: application/json" \
        -d "$data" "$SERVER_URL/_matrix/client/r0/login")
    print_response "$response"
    
    # 返回响应
    echo "$response"
}

# 5. 获取当前用户信息
test_whoami() {
    local token="$1"
    
    print_section "5. 获取当前用户信息"
    print_request "GET /_matrix/client/r0/account/whoami"
    echo "令牌: ${token:0:20}..."
    
    local response
    response=$(curl -s -X GET -H "Authorization: Bearer $token" \
        "$SERVER_URL/_matrix/client/r0/account/whoami")
    print_response "$response"
}

# 6. 获取用户资料
test_get_profile() {
    local user_id="$1"
    local token="${2:-}"
    
    print_section "6. 获取用户资料"
    print_request "GET /_matrix/client/r0/account/profile/$user_id"
    
    local curl_cmd="curl -s -X GET"
    if [ -n "$token" ]; then
        curl_cmd="$curl_cmd -H 'Authorization: Bearer $token'"
    fi
    curl_cmd="$curl_cmd '$SERVER_URL/_matrix/client/r0/account/profile/$user_id'"
    
    local response
    response=$(eval "$curl_cmd")
    print_response "$response"
}

# 7. 更新显示名称
test_update_displayname() {
    local user_id="$1"
    local displayname="$2"
    local token="$3"
    
    print_section "7. 更新显示名称"
    print_request "PUT /_matrix/client/r0/account/profile/$user_id/displayname"
    echo "显示名: $displayname"
    
    local data="{\"displayname\":\"$displayname\"}"
    local response
    response=$(curl -s -X PUT -H "Authorization: Bearer $token" \
        -H "Content-Type: application/json" \
        -d "$data" "$SERVER_URL/_matrix/client/r0/account/profile/$user_id/displayname")
    print_response "$response"
}

# 8. 更新头像
test_update_avatar() {
    local user_id="$1"
    local avatar_url="$2"
    local token="$3"
    
    print_section "8. 更新头像"
    print_request "PUT /_matrix/client/r0/account/profile/$user_id/avatar_url"
    echo "头像URL: $avatar_url"
    
    local data="{\"avatar_url\":\"$avatar_url\"}"
    local response
    response=$(curl -s -X PUT -H "Authorization: Bearer $token" \
        -H "Content-Type: application/json" \
        -d "$data" "$SERVER_URL/_matrix/client/r0/account/profile/$user_id/avatar_url")
    print_response "$response"
}

# 9. 修改密码
test_change_password() {
    local new_password="$1"
    local token="$2"
    
    print_section "9. 修改密码"
    print_request "POST /_matrix/client/r0/account/password"
    echo "新密码: $new_password"
    
    local data="{\"new_password\":\"$new_password\"}"
    local response
    response=$(curl -s -X POST -H "Authorization: Bearer $token" \
        -H "Content-Type: application/json" \
        -d "$data" "$SERVER_URL/_matrix/client/r0/account/password")
    print_response "$response"
}

# 10. 刷新令牌
test_refresh_token() {
    local refresh_token="$1"
    
    print_section "10. 刷新令牌"
    print_request "POST /_matrix/client/r0/refresh"
    echo "刷新令牌: ${refresh_token:0:20}..."
    
    local data="{\"refresh_token\":\"$refresh_token\"}"
    local response
    response=$(curl -s -X POST -H "Content-Type: application/json" \
        -d "$data" "$SERVER_URL/_matrix/client/r0/refresh")
    print_response "$response"
}

# 11. 登出
test_logout() {
    local token="$1"
    
    print_section "11. 登出"
    print_request "POST /_matrix/client/r0/logout"
    echo "令牌: ${token:0:20}..."
    
    local response
    response=$(curl -s -X POST -H "Authorization: Bearer $token" \
        "$SERVER_URL/_matrix/client/r0/logout")
    print_response "$response"
}

# 12. 全部登出
test_logout_all() {
    local token="$1"
    
    print_section "12. 全部登出"
    print_request "POST /_matrix/client/r0/logout/all"
    echo "令牌: ${token:0:20}..."
    
    local response
    response=$(curl -s -X POST -H "Authorization: Bearer $token" \
        "$SERVER_URL/_matrix/client/r0/logout/all")
    print_response "$response"
}

# 13. 停用账户
test_deactivate() {
    local token="$1"
    
    print_section "13. 停用账户"
    print_request "POST /_matrix/client/r0/account/deactivate"
    echo "令牌: ${token:0:20}..."
    
    local response
    response=$(curl -s -X POST -H "Authorization: Bearer $token" \
        "$SERVER_URL/_matrix/client/r0/account/deactivate")
    print_response "$response"
}

###############################################################################
# 完整测试流程
###############################################################################

run_complete_test() {
    echo "========================================"
    echo "认证模块完整测试流程"
    echo "========================================"
    
    # 1. 获取客户端版本
    test_get_versions
    
    # 2. 检查用户名可用性
    local username="testuser_$(date +%s)"
    test_check_available "$username"
    
    # 3. 注册用户
    local register_response
    register_response=$(test_register "$username" "TestPassword123" "Test User")
    
    # 提取令牌和用户ID
    local user_id=$(echo "$register_response" | jq -r '.user_id // empty')
    local access_token=$(echo "$register_response" | jq -r '.access_token // empty')
    local refresh_token=$(echo "$register_response" | jq -r '.refresh_token // empty')
    
    if [ -z "$user_id" ] || [ -z "$access_token" ]; then
        echo "错误: 注册失败，无法获取用户ID或令牌"
        exit 1
    fi
    
    echo "用户ID: $user_id"
    echo "访问令牌: ${access_token:0:20}..."
    echo "刷新令牌: ${refresh_token:0:20}..."
    
    # 4. 获取当前用户信息
    test_whoami "$access_token"
    
    # 5. 获取用户资料
    test_get_profile "$user_id" "$access_token"
    
    # 6. 更新显示名称
    test_update_displayname "$user_id" "Updated Test User" "$access_token"
    
    # 7. 更新头像
    test_update_avatar "$user_id" "mxc://server.com/media123" "$access_token"
    
    # 8. 修改密码
    test_change_password "NewPassword456" "$access_token"
    
    # 9. 刷新令牌
    local refresh_response
    refresh_response=$(test_refresh_token "$refresh_token")
    
    # 更新访问令牌
    access_token=$(echo "$refresh_response" | jq -r '.access_token // empty')
    echo "新的访问令牌: ${access_token:0:20}..."
    
    # 10. 登出
    test_logout "$access_token"
    
    echo "========================================"
    echo "完整测试流程结束"
    echo "========================================"
}

###############################################################################
# 菜单系统
###############################################################################

show_menu() {
    echo ""
    echo "========================================"
    echo "认证模块测试菜单"
    echo "========================================"
    echo "1. 获取客户端版本"
    echo "2. 检查用户名可用性"
    echo "3. 用户注册"
    echo "4. 用户登录"
    echo "5. 获取当前用户信息"
    echo "6. 获取用户资料"
    echo "7. 更新显示名称"
    echo "8. 更新头像"
    echo "9. 修改密码"
    echo "10. 刷新令牌"
    echo "11. 登出"
    echo "12. 全部登出"
    echo "13. 停用账户"
    echo "14. 运行完整测试流程"
    echo "0. 退出"
    echo "========================================"
    echo -n "请选择: "
}

###############################################################################
# 主程序
###############################################################################

main() {
    echo "========================================"
    echo "Synapse Rust 认证模块手动测试"
    echo "========================================"
    echo "服务器URL: $SERVER_URL"
    echo "服务器名称: $SERVER_NAME"
    echo "========================================"
    
    # 如果有命令行参数，直接执行对应测试
    if [ $# -gt 0 ]; then
        case "$1" in
            1) test_get_versions ;;
            2) test_check_available "$2" ;;
            3) test_register "$2" "$3" "$4" ;;
            4) test_login "$2" "$3" ;;
            5) test_whoami "$2" ;;
            6) test_get_profile "$2" "$3" ;;
            7) test_update_displayname "$2" "$3" "$4" ;;
            8) test_update_avatar "$2" "$3" "$4" ;;
            9) test_change_password "$2" "$3" ;;
            10) test_refresh_token "$2" ;;
            11) test_logout "$2" ;;
            12) test_logout_all "$2" ;;
            13) test_deactivate "$2" ;;
            14) run_complete_test ;;
            *) echo "无效选项" ;;
        esac
        exit 0
    fi
    
    # 交互式菜单
    while true; do
        show_menu
        read -r choice
        
        case $choice in
            1)
                test_get_versions
                ;;
            2)
                echo -n "输入用户名: "
                read -r username
                test_check_available "$username"
                ;;
            3)
                echo -n "输入用户名: "
                read -r username
                echo -n "输入密码: "
                read -rs password
                echo
                echo -n "输入显示名: "
                read -r displayname
                test_register "$username" "$password" "$displayname"
                ;;
            4)
                echo -n "输入用户名: "
                read -r username
                echo -n "输入密码: "
                read -rs password
                echo
                test_login "$username" "$password"
                ;;
            5)
                echo -n "输入访问令牌: "
                read -r token
                test_whoami "$token"
                ;;
            6)
                echo -n "输入用户ID: "
                read -r user_id
                echo -n "输入访问令牌(可选): "
                read -r token
                test_get_profile "$user_id" "$token"
                ;;
            7)
                echo -n "输入用户ID: "
                read -r user_id
                echo -n "输入显示名: "
                read -r displayname
                echo -n "输入访问令牌: "
                read -r token
                test_update_displayname "$user_id" "$displayname" "$token"
                ;;
            8)
                echo -n "输入用户ID: "
                read -r user_id
                echo -n "输入头像URL: "
                read -r avatar_url
                echo -n "输入访问令牌: "
                read -r token
                test_update_avatar "$user_id" "$avatar_url" "$token"
                ;;
            9)
                echo -n "输入新密码: "
                read -rs new_password
                echo
                echo -n "输入访问令牌: "
                read -r token
                test_change_password "$new_password" "$token"
                ;;
            10)
                echo -n "输入刷新令牌: "
                read -r refresh_token
                test_refresh_token "$refresh_token"
                ;;
            11)
                echo -n "输入访问令牌: "
                read -r token
                test_logout "$token"
                ;;
            12)
                echo -n "输入访问令牌: "
                read -r token
                test_logout_all "$token"
                ;;
            13)
                echo -n "输入访问令牌: "
                read -r token
                test_deactivate "$token"
                ;;
            14)
                run_complete_test
                ;;
            0)
                echo "退出测试"
                exit 0
                ;;
            *)
                echo "无效选项，请重新选择"
                ;;
        esac
    done
}

# 运行主程序
main "$@"
