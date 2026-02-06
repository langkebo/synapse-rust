#!/usr/bin/env python3
"""
Synapse Rust 管理员API测试脚本
测试所有管理员API端点，记录测试结果
"""

import requests
import json
from datetime import datetime

BASE_URL = "http://localhost:8008"

# 测试账号信息
TEST_ACCOUNTS = {
    "admin": {
        "username": "admin",
        "password": "Wzc9890951!",
        "user_id": "@admin:cjystx.top",
    },
    "testuser1": {
        "username": "testuser1",
        "password": "TestUser123!",
        "user_id": "@testuser1:cjystx.top",
    }
}

# 测试房间信息
TEST_ROOMS = {
    "room1": {
        "room_id": "!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top",
        "name": "Test Room 1"
    },
    "room2": {
        "room_id": "!pdsb0b_OqRVJazC6JYW1CZRQ:cjystx.top",
        "name": "Test Room 2"
    }
}

# 测试结果存储
test_results = []

def log_result(category, api_name, method, endpoint, status_code, expected_code, success, error=None, response_data=None):
    """记录测试结果"""
    result = {
        "category": category,
        "api_name": api_name,
        "method": method,
        "endpoint": endpoint,
        "status_code": status_code,
        "expected_code": expected_code,
        "success": success,
        "error": error,
        "timestamp": datetime.now().isoformat(),
        "response_data": response_data
    }
    test_results.append(result)
    
    status_symbol = "✓" if success else "✗"
    print(f"{status_symbol} [{category}] {api_name}: {method} {endpoint}")
    if error:
        print(f"  错误: {error}")
    print(f"  状态码: {status_code} (期望: {expected_code})")
    print()

def make_request(method, endpoint, data=None, params=None, headers=None, token=None):
    """发送HTTP请求"""
    url = f"{BASE_URL}{endpoint}"
    
    if headers is None:
        headers = {}
    
    if token:
        headers["Authorization"] = f"Bearer {token}"
    
    if data is not None and isinstance(data, dict):
        headers["Content-Type"] = "application/json"
    
    try:
        if method == "GET":
            response = requests.get(url, params=params, headers=headers)
        elif method == "POST":
            response = requests.post(url, json=data, headers=headers)
        elif method == "PUT":
            response = requests.put(url, json=data, headers=headers)
        elif method == "DELETE":
            response = requests.delete(url, headers=headers)
        else:
            return None, None
        
        try:
            response_data = response.json()
        except:
            response_data = response.text
        
        return response, response_data
    except Exception as e:
        return None, str(e)

def test_2_1_server_management():
    """测试2.1 服务器管理"""
    print("\n" + "="*80)
    print("测试 2.1 服务器管理")
    print("="*80 + "\n")
    
    token = TEST_ACCOUNTS["admin"]["access_token"]
    
    # 1. 获取服务器版本
    print("测试1: 获取服务器版本")
    response, data = make_request("GET", "/_synapse/admin/v1/server_version", token=token)
    if response:
        log_result("2.1 服务器管理", "获取服务器版本", "GET", "/_synapse/admin/v1/server_version",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("2.1 服务器管理", "获取服务器版本", "GET", "/_synapse/admin/v1/server_version",
                  None, 200, False, data, None)
    
    # 2. 获取服务器状态
    print("测试2: 获取服务器状态")
    response, data = make_request("GET", "/_synapse/admin/v1/status", token=token)
    if response:
        log_result("2.1 服务器管理", "获取服务器状态", "GET", "/_synapse/admin/v1/status",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("2.1 服务器管理", "获取服务器状态", "GET", "/_synapse/admin/v1/status",
                  None, 200, False, data, None)

def test_2_2_user_management():
    """测试2.2 用户管理"""
    print("\n" + "="*80)
    print("测试 2.2 用户管理")
    print("="*80 + "\n")
    
    token = TEST_ACCOUNTS["admin"]["access_token"]
    user_id = TEST_ACCOUNTS["admin"]["user_id"]
    
    # 1. 获取用户列表（分页）
    print("测试1: 获取用户列表（分页）")
    response, data = make_request("GET", "/_synapse/admin/v1/users", params={"limit": 10, "offset": 0}, token=token)
    if response:
        log_result("2.2 用户管理", "获取用户列表（分页）", "GET", "/_synapse/admin/v1/users",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("2.2 用户管理", "获取用户列表（分页）", "GET", "/_synapse/admin/v1/users",
                  None, 200, False, data, None)
    
    # 2. 获取用户详情
    print("测试2: 获取用户详情")
    response, data = make_request("GET", f"/_synapse/admin/v1/users/{user_id}", token=token)
    if response:
        log_result("2.2 用户管理", "获取用户详情", "GET", "/_synapse/admin/v1/users/{{user_id}}",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("2.2 用户管理", "获取用户详情", "GET", "/_synapse/admin/v1/users/{{user_id}}",
                  None, 200, False, data, None)
    
    # 3. 设置管理员权限
    print("测试3: 设置管理员权限")
    response, data = make_request("PUT", f"/_synapse/admin/v1/users/{user_id}/admin", 
                               data={"admin": True}, token=token)
    if response:
        log_result("2.2 用户管理", "设置管理员权限", "PUT", "/_synapse/admin/v1/users/{{user_id}}/admin",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("2.2 用户管理", "设置管理员权限", "PUT", "/_synapse/admin/v1/users/{{user_id}}/admin",
                  None, 200, False, data, None)
    
    # 4. 获取用户的房间列表
    print("测试4: 获取用户的房间列表")
    response, data = make_request("GET", f"/_synapse/admin/v1/users/{user_id}/rooms", token=token)
    if response:
        log_result("2.2 用户管理", "获取用户的房间列表", "GET", "/_synapse/admin/v1/users/{{user_id}}/rooms",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("2.2 用户管理", "获取用户的房间列表", "GET", "/_synapse/admin/v1/users/{{user_id}}/rooms",
                  None, 200, False, data, None)

def test_2_3_room_management():
    """测试2.3 房间管理"""
    print("\n" + "="*80)
    print("测试 2.3 房间管理")
    print("="*80 + "\n")
    
    token = TEST_ACCOUNTS["admin"]["access_token"]
    room_id = TEST_ROOMS["room1"]["room_id"]
    
    # 1. 获取房间列表（分页）
    print("测试1: 获取房间列表（分页）")
    response, data = make_request("GET", "/_synapse/admin/v1/rooms", params={"limit": 10, "offset": 0}, token=token)
    if response:
        log_result("2.3 房间管理", "获取房间列表（分页）", "GET", "/_synapse/admin/v1/rooms",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("2.3 房间管理", "获取房间列表（分页）", "GET", "/_synapse/admin/v1/rooms",
                  None, 200, False, data, None)
    
    # 2. 获取房间详情
    print("测试2: 获取房间详情")
    response, data = make_request("GET", f"/_synapse/admin/v1/rooms/{room_id}", token=token)
    if response:
        log_result("2.3 房间管理", "获取房间详情", "GET", "/_synapse/admin/v1/rooms/{{room_id}}",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("2.3 房间管理", "获取房间详情", "GET", "/_synapse/admin/v1/rooms/{{room_id}}",
                  None, 200, False, data, None)

def test_2_4_security_management():
    """测试2.4 安全管理"""
    print("\n" + "="*80)
    print("测试 2.4 安全管理")
    print("="*80 + "\n")
    
    token = TEST_ACCOUNTS["admin"]["access_token"]
    
    # 1. 获取安全事件日志
    print("测试1: 获取安全事件日志")
    response, data = make_request("GET", "/_synapse/admin/v1/security/events", token=token)
    if response:
        log_result("2.4 安全管理", "获取安全事件日志", "GET", "/_synapse/admin/v1/security/events",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("2.4 安全管理", "获取安全事件日志", "GET", "/_synapse/admin/v1/security/events",
                  None, 200, False, data, None)
    
    # 2. 获取IP封禁列表
    print("测试2: 获取IP封禁列表")
    response, data = make_request("GET", "/_synapse/admin/v1/security/ip/blocks", token=token)
    if response:
        log_result("2.4 安全管理", "获取IP封禁列表", "GET", "/_synapse/admin/v1/security/ip/blocks",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("2.4 安全管理", "获取IP封禁列表", "GET", "/_synapse/admin/v1/security/ip/blocks",
                  None, 200, False, data, None)

def test_2_5_admin_registration():
    """测试2.5 管理员注册"""
    print("\n" + "="*80)
    print("测试 2.5 管理员注册")
    print("="*80 + "\n")
    
    # 1. 获取管理员注册nonce
    print("测试1: 获取管理员注册nonce")
    response, data = make_request("GET", "/_synapse/admin/v1/register/nonce")
    if response:
        log_result("2.5 管理员注册", "获取管理员注册nonce", "GET", "/_synapse/admin/v1/register/nonce",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("2.5 管理员注册", "获取管理员注册nonce", "GET", "/_synapse/admin/v1/register/nonce",
                  None, 200, False, data, None)

def generate_report():
    """生成测试报告"""
    print("\n" + "="*80)
    print("测试报告汇总")
    print("="*80 + "\n")
    
    total_tests = len(test_results)
    passed_tests = sum(1 for r in test_results if r["success"])
    failed_tests = total_tests - passed_tests
    
    print(f"总测试数: {total_tests}")
    print(f"通过: {passed_tests}")
    print(f"失败: {failed_tests}")
    print(f"成功率: {passed_tests/total_tests*100:.2f}%")
    print()
    
    # 按类别统计
    categories = {}
    for result in test_results:
        category = result["category"]
        if category not in categories:
            categories[category] = {"total": 0, "passed": 0, "failed": 0}
        categories[category]["total"] += 1
        if result["success"]:
            categories[category]["passed"] += 1
        else:
            categories[category]["failed"] += 1
    
    print("按类别统计:")
    for category, stats in categories.items():
        print(f"  {category}: {stats['passed']}/{stats['total']} 通过")
        if stats["failed"] > 0:
            print(f"    失败: {stats['failed']}")
    print()
    
    # 列出失败的测试
    if failed_tests > 0:
        print("失败的测试:")
        for result in test_results:
            if not result["success"]:
                print(f"  - [{result['category']}] {result['api_name']}: {result['method']} {result['endpoint']}")
                print(f"    状态码: {result['status_code']} (期望: {result['expected_code']})")
                if result["error"]:
                    print(f"    错误: {result['error']}")
        print()
    
    # 保存测试结果到JSON文件
    with open("/home/hula/synapse_rust/admin_api_test_results.json", "w", encoding="utf-8") as f:
        json.dump(test_results, f, indent=2, ensure_ascii=False)
    
    print("测试结果已保存到: /home/hula/synapse_rust/admin_api_test_results.json")

def main():
    """主函数"""
    print("="*80)
    print("Synapse Rust 管理员API测试")
    print("="*80)
    print(f"测试时间: {datetime.now().isoformat()}")
    print(f"服务器地址: {BASE_URL}")
    print()
    
    # 登录获取token
    print("正在登录管理员账户...")
    login_response = requests.post(
        f"{BASE_URL}/_matrix/client/r0/login",
        json={
            "type": "m.login.password",
            "user": TEST_ACCOUNTS["admin"]["username"],
            "password": TEST_ACCOUNTS["admin"]["password"]
        }
    )
    
    if login_response.status_code != 200:
        print(f"登录失败: {login_response.status_code}")
        print(login_response.text)
        exit(1)
    
    TEST_ACCOUNTS["admin"]["access_token"] = login_response.json()["access_token"]
    print(f"登录成功，Token: {TEST_ACCOUNTS['admin']['access_token'][:50]}...")
    print()
    
    # 运行所有测试
    test_2_1_server_management()
    test_2_2_user_management()
    test_2_3_room_management()
    test_2_4_security_management()
    test_2_5_admin_registration()
    
    # 生成测试报告
    generate_report()

if __name__ == "__main__":
    main()
