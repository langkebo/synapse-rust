#!/usr/bin/env python3
"""
Synapse Rust 核心客户端API测试脚本
测试所有核心客户端API端点，记录测试结果
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
        "user_id": "@admin:matrix.cjystx.top",
        "access_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXIxOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkB0ZXN0dXNlcjE6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6dHJ1ZSwiZXhwIjoxNzcwMTg0MDUwLCJpYXQiOjE3NzAxODA0NTAsImRldmljZV9pZCI6Ik4zbUhuam1ZWFhxZ3VBZGgifQ.G8092HdzmY_a73l-jvzYBsLTd4TLf2PVOkdkDwAy2X8"
    },
    "testuser1": {
        "username": "testuser1",
        "password": "TestUser123456!",
        "user_id": "@testuser1:matrix.cjystx.top",
        "access_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXIxOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkB0ZXN0dXNlcjE6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6dHJ1ZSwiZXhwIjoxNzcwMTg0MDUwLCJpYXQiOjE3NzAxODA0NTAsImRldmljZV9pZCI6Ik4zbUhuam1ZWFhxZ3VBZGgifQ.G8092HdzmY_a73l-jvzYBsLTd4TLf2PVOkdkDwAy2X8"
    },
    "testuser2": {
        "username": "testuser2",
        "password": "TestUser123456!",
        "user_id": "@testuser2:matrix.cjystx.top",
        "access_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXIxOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkB0ZXN0dXNlcjE6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6dHJ1ZSwiZXhwIjoxNzcwMTg0MDUwLCJpYXQiOjE3NzAxODA0NTAsImRldmljZV9pZCI6Ik4zbUhuam1ZWFhxZ3VBZGgifQ.G8092HdzmY_a73l-jvzYBsLTd4TLf2PVOkdkDwAy2X8"
    }
}

# 测试房间信息
TEST_ROOMS = {
    "room1": {
        "name": "Test Room 1",
        "room_id": "!BfUBQVVQfR0EQUmS9kwF_EQ2:matrix.cjystx.top",
        "type": "public"
    },
    "room2": {
        "name": "Test Room 2",
        "room_id": "!pdsb0b_OqRVJazC6JYW1CZRQ:matrix.cjystx.top",
        "type": "private"
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

def test_1_1_basic_info_and_auth():
    """测试1.1 基础信息与认证"""
    print("\n" + "="*80)
    print("测试 1.1 基础信息与认证")
    print("="*80 + "\n")
    
    # 1. 服务器根路径信息
    print("测试1: 服务器根路径信息")
    response, data = make_request("GET", "/")
    if response:
        log_result("1.1 基础信息与认证", "服务器根路径信息", "GET", "/", 
                  response.status_code, 200, response.status_code == 200, 
                  None if response.status_code == 200 else data, data)
    else:
        log_result("1.1 基础信息与认证", "服务器根路径信息", "GET", "/", 
                  None, 200, False, data, None)
    
    # 2. 健康检查
    print("测试2: 健康检查")
    response, data = make_request("GET", "/health")
    if response:
        log_result("1.1 基础信息与认证", "健康检查", "GET", "/health", 
                  response.status_code, 200, response.status_code == 200, 
                  None if response.status_code == 200 else data, data)
    else:
        log_result("1.1 基础信息与认证", "健康检查", "GET", "/health", 
                  None, 200, False, data, None)
    
    # 3. 获取支持的客户端版本
    print("测试3: 获取支持的客户端版本")
    response, data = make_request("GET", "/_matrix/client/versions")
    if response:
        log_result("1.1 基础信息与认证", "获取支持的客户端版本", "GET", "/_matrix/client/versions", 
                  response.status_code, 200, response.status_code == 200, 
                  None if response.status_code == 200 else data, data)
    else:
        log_result("1.1 基础信息与认证", "获取支持的客户端版本", "GET", "/_matrix/client/versions", 
                  None, 200, False, data, None)
    
    # 4. 检查用户名可用性
    print("测试4: 检查用户名可用性")
    response, data = make_request("GET", "/_matrix/client/r0/register/available", 
                                  params={"username": "newuser123"})
    if response:
        log_result("1.1 基础信息与认证", "检查用户名可用性", "GET", "/_matrix/client/r0/register/available", 
                  response.status_code, 200, response.status_code == 200, 
                  None if response.status_code == 200 else data, data)
    else:
        log_result("1.1 基础信息与认证", "检查用户名可用性", "GET", "/_matrix/client/r0/register/available", 
                  None, 200, False, data, None)
    
    # 5. 用户登录
    print("测试5: 用户登录")
    response, data = make_request("POST", "/_matrix/client/r0/login", 
                                  data={"username": "testuser1", "password": "TestUser123456!"})
    if response:
        log_result("1.1 基础信息与认证", "用户登录", "POST", "/_matrix/client/r0/login", 
                  response.status_code, 200, response.status_code == 200, 
                  None if response.status_code == 200 else data, data)
        if response.status_code == 200 and "access_token" in data:
            TEST_ACCOUNTS["testuser1"]["access_token"] = data["access_token"]
    else:
        log_result("1.1 基础信息与认证", "用户登录", "POST", "/_matrix/client/r0/login", 
                  None, 200, False, data, None)
    
    # 6. 获取当前用户信息
    print("测试6: 获取当前用户信息")
    response, data = make_request("GET", "/_matrix/client/r0/account/whoami", 
                                  token=TEST_ACCOUNTS["testuser1"]["access_token"])
    if response:
        log_result("1.1 基础信息与认证", "获取当前用户信息", "GET", "/_matrix/client/r0/account/whoami", 
                  response.status_code, 200, response.status_code == 200, 
                  None if response.status_code == 200 else data, data)
    else:
        log_result("1.1 基础信息与认证", "获取当前用户信息", "GET", "/_matrix/client/r0/account/whoami", 
                  None, 200, False, data, None)
    
    # 7. 刷新访问令牌
    print("测试7: 刷新访问令牌")
    
    refresh_token = None
    try:
        login_response, login_data = make_request("POST", "/_matrix/client/r0/login",
                                                  data={"type": "m.login.password",
                                                       "user": "testuser1",
                                                       "password": "TestUser123456!"})
        if login_response and login_response.status_code == 200:
            refresh_token = login_data.get("refresh_token")
            print(f"  获取到refresh_token: {refresh_token[:50] if refresh_token else 'None'}...")
    except Exception as e:
        print(f"  获取refresh_token失败: {e}")
    
    if refresh_token:
        response, data = make_request("POST", "/_matrix/client/r0/refresh",
                                      data={"refresh_token": refresh_token})
    else:
        response, data = None, {"error": "无法获取有效的refresh_token"}
    
    if response:
        log_result("1.1 基础信息与认证", "刷新访问令牌", "POST", "/_matrix/client/r0/refresh",
                  response.status_code, 200, response.status_code == 200,
                  None if response.status_code == 200 else data, data)
    else:
        log_result("1.1 基础信息与认证", "刷新访问令牌", "POST", "/_matrix/client/r0/refresh",
                  None, 200, False, data, None)

def test_1_2_user_profile_management():
    """测试1.2 用户资料管理"""
    print("\n" + "="*80)
    print("测试 1.2 用户资料管理")
    print("="*80 + "\n")
    
    token = TEST_ACCOUNTS["testuser1"]["access_token"]
    user_id = TEST_ACCOUNTS["testuser1"]["user_id"]
    
    # 1. 获取用户资料
    print("测试1: 获取用户资料")
    response, data = make_request("GET", f"/_matrix/client/r0/account/profile/{user_id}", token=token)
    if response:
        log_result("1.2 用户资料管理", "获取用户资料", "GET", f"/_matrix/client/r0/account/profile/{{user_id}}", 
                  response.status_code, 200, response.status_code == 200, 
                  None if response.status_code == 200 else data, data)
    else:
        log_result("1.2 用户资料管理", "获取用户资料", "GET", f"/_matrix/client/r0/account/profile/{{user_id}}", 
                  None, 200, False, data, None)
    
    # 2. 更新显示名称
    print("测试2: 更新显示名称")
    response, data = make_request("PUT", f"/_matrix/client/r0/account/profile/{user_id}/displayname", 
                                  data={"displayname": "Test User 1 Updated"}, token=token)
    if response:
        log_result("1.2 用户资料管理", "更新显示名称", "PUT", f"/_matrix/client/r0/account/profile/{{user_id}}/displayname", 
                  response.status_code, 200, response.status_code == 200, 
                  None if response.status_code == 200 else data, data)
    else:
        log_result("1.2 用户资料管理", "更新显示名称", "PUT", f"/_matrix/client/r0/account/profile/{{user_id}}/displayname", 
                  None, 200, False, data, None)
    
    # 3. 更新头像URL
    print("测试3: 更新头像URL")
    response, data = make_request("PUT", f"/_matrix/client/r0/account/profile/{user_id}/avatar_url", 
                                  data={"avatar_url": "mxc://matrix.cjystx.top/test_avatar"}, token=token)
    if response:
        log_result("1.2 用户资料管理", "更新头像URL", "PUT", f"/_matrix/client/r0/account/profile/{{user_id}}/avatar_url", 
                  response.status_code, 200, response.status_code == 200, 
                  None if response.status_code == 200 else data, data)
    else:
        log_result("1.2 用户资料管理", "更新头像URL", "PUT", f"/_matrix/client/r0/account/profile/{{user_id}}/avatar_url", 
                  None, 200, False, data, None)

def test_1_3_room_management():
    """测试1.3 房间管理"""
    print("\n" + "="*80)
    print("测试 1.3 房间管理")
    print("="*80 + "\n")
    
    token = TEST_ACCOUNTS["testuser1"]["access_token"]
    room_id = TEST_ROOMS["room1"]["room_id"]
    
    # 1. 客户端同步
    print("测试1: 客户端同步")
    response, data = make_request("GET", "/_matrix/client/r0/sync", 
                                  params={"timeout": 30000}, token=token)
    if response:
        log_result("1.3 房间管理", "客户端同步", "GET", "/_matrix/client/r0/sync", 
                  response.status_code, 200, response.status_code == 200, 
                  None if response.status_code == 200 else data, data)
    else:
        log_result("1.3 房间管理", "客户端同步", "GET", "/_matrix/client/r0/sync", 
                  None, 200, False, data, None)
    
    # 2. 获取房间消息
    print("测试2: 获取房间消息")
    response, data = make_request("GET", f"/_matrix/client/r0/rooms/{room_id}/messages", 
                                  params={"limit": 10}, token=token)
    if response:
        log_result("1.3 房间管理", "获取房间消息", "GET", "/_matrix/client/r0/rooms/{{room_id}}/messages", 
                  response.status_code, 200, response.status_code == 200, 
                  None if response.status_code == 200 else data, data)
    else:
        log_result("1.3 房间管理", "获取房间消息", "GET", "/_matrix/client/r0/rooms/{{room_id}}/messages", 
                  None, 200, False, data, None)
    
    # 3. 发送消息
    print("测试3: 发送消息")
    txn_id = f"test_txn_{int(datetime.now().timestamp())}"
    response, data = make_request("PUT", f"/_matrix/client/r0/rooms/{room_id}/send/m.room.message/{txn_id}", 
                                  data={"msgtype": "m.text", "body": "Test message from API test"}, token=token)
    if response:
        log_result("1.3 房间管理", "发送消息", "PUT", "/_matrix/client/r0/rooms/{{room_id}}/send/{{event_type}}/{{txn_id}}", 
                  response.status_code, 200, response.status_code == 200, 
                  None if response.status_code == 200 else data, data)
    else:
        log_result("1.3 房间管理", "发送消息", "PUT", "/_matrix/client/r0/rooms/{{room_id}}/send/{{event_type}}/{{txn_id}}", 
                  None, 200, False, data, None)
    
    # 4. 获取房间成员
    print("测试4: 获取房间成员")
    response, data = make_request("GET", f"/_matrix/client/r0/rooms/{room_id}/members", token=token)
    if response:
        log_result("1.3 房间管理", "获取房间成员", "GET", "/_matrix/client/r0/rooms/{{room_id}}/members", 
                  response.status_code, 200, response.status_code == 200, 
                  None if response.status_code == 200 else data, data)
    else:
        log_result("1.3 房间管理", "获取房间成员", "GET", "/_matrix/client/r0/rooms/{{room_id}}/members", 
                  None, 200, False, data, None)
    
    # 5. 获取公共房间列表
    print("测试5: 获取公共房间列表")
    response, data = make_request("GET", "/_matrix/client/r0/publicRooms", 
                                  params={"limit": 10}, token=token)
    if response:
        log_result("1.3 房间管理", "获取公共房间列表", "GET", "/_matrix/client/r0/publicRooms", 
                  response.status_code, 200, response.status_code == 200, 
                  None if response.status_code == 200 else data, data)
    else:
        log_result("1.3 房间管理", "获取公共房间列表", "GET", "/_matrix/client/r0/publicRooms", 
                  None, 200, False, data, None)
    
    # 6. 获取用户加入的房间
    print("测试6: 获取用户加入的房间")
    response, data = make_request("GET", f"/_matrix/client/r0/user/{TEST_ACCOUNTS['testuser1']['user_id']}/rooms", 
                                  token=token)
    if response:
        log_result("1.3 房间管理", "获取用户加入的房间", "GET", "/_matrix/client/r0/user/{{user_id}}/rooms", 
                  response.status_code, 200, response.status_code == 200, 
                  None if response.status_code == 200 else data, data)
    else:
        log_result("1.3 房间管理", "获取用户加入的房间", "GET", "/_matrix/client/r0/user/{{user_id}}/rooms", 
                  None, 200, False, data, None)

def test_1_4_room_state_and_permissions():
    """测试1.4 房间状态与权限"""
    print("\n" + "="*80)
    print("测试 1.4 房间状态与权限")
    print("="*80 + "\n")
    
    token = TEST_ACCOUNTS["testuser1"]["access_token"]
    room_id = TEST_ROOMS["room1"]["room_id"]
    
    # 1. 获取房间所有状态
    print("测试1: 获取房间所有状态")
    response, data = make_request("GET", f"/_matrix/client/r0/rooms/{room_id}/state", token=token)
    if response:
        log_result("1.4 房间状态与权限", "获取房间所有状态", "GET", "/_matrix/client/r0/rooms/{{room_id}}/state", 
                  response.status_code, 200, response.status_code == 200, 
                  None if response.status_code == 200 else data, data)
    else:
        log_result("1.4 房间状态与权限", "获取房间所有状态", "GET", "/_matrix/client/r0/rooms/{{room_id}}/state", 
                  None, 200, False, data, None)
    
    # 2. 按类型获取状态事件
    print("测试2: 按类型获取状态事件")
    response, data = make_request("GET", f"/_matrix/client/r0/rooms/{room_id}/state/m.room.name", token=token)
    if response:
        log_result("1.4 房间状态与权限", "按类型获取状态事件", "GET", "/_matrix/client/r0/rooms/{{room_id}}/state/{{event_type}}", 
                  response.status_code, 200, response.status_code == 200, 
                  None if response.status_code == 200 else data, data)
    else:
        log_result("1.4 房间状态与权限", "按类型获取状态事件", "GET", "/_matrix/client/r0/rooms/{{room_id}}/state/{{event_type}}", 
                  None, 200, False, data, None)

def test_1_5_device_management():
    """测试1.5 设备管理"""
    print("\n" + "="*80)
    print("测试 1.5 设备管理")
    print("="*80 + "\n")
    
    token = TEST_ACCOUNTS["testuser1"]["access_token"]
    
    # 1. 获取设备列表
    print("测试1: 获取设备列表")
    response, data = make_request("GET", "/_matrix/client/r0/devices", token=token)
    if response:
        log_result("1.5 设备管理", "获取设备列表", "GET", "/_matrix/client/r0/devices", 
                  response.status_code, 200, response.status_code == 200, 
                  None if response.status_code == 200 else data, data)
    else:
        log_result("1.5 设备管理", "获取设备列表", "GET", "/_matrix/client/r0/devices", 
                  None, 200, False, data, None)

def test_1_6_presence():
    """测试1.6 在线状态"""
    print("\n" + "="*80)
    print("测试 1.6 在线状态")
    print("="*80 + "\n")
    
    token = TEST_ACCOUNTS["testuser1"]["access_token"]
    user_id = TEST_ACCOUNTS["testuser1"]["user_id"]
    
    # 1. 获取用户在线状态
    print("测试1: 获取用户在线状态")
    response, data = make_request("GET", f"/_matrix/client/r0/presence/{user_id}/status", token=token)
    if response:
        log_result("1.6 在线状态", "获取用户在线状态", "GET", "/_matrix/client/r0/presence/{{user_id}}/status", 
                  response.status_code, 200, response.status_code == 200, 
                  None if response.status_code == 200 else data, data)
    else:
        log_result("1.6 在线状态", "获取用户在线状态", "GET", "/_matrix/client/r0/presence/{{user_id}}/status", 
                  None, 200, False, data, None)
    
    # 2. 设置用户在线状态
    print("测试2: 设置用户在线状态")
    response, data = make_request("PUT", f"/_matrix/client/r0/presence/{user_id}/status", 
                                  data={"presence": "online", "status_msg": "Testing API"}, token=token)
    if response:
        log_result("1.6 在线状态", "设置用户在线状态", "PUT", "/_matrix/client/r0/presence/{{user_id}}/status", 
                  response.status_code, 200, response.status_code == 200, 
                  None if response.status_code == 200 else data, data)
    else:
        log_result("1.6 在线状态", "设置用户在线状态", "PUT", "/_matrix/client/r0/presence/{{user_id}}/status", 
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
    with open("/home/hula/synapse_rust/test_results.json", "w", encoding="utf-8") as f:
        json.dump(test_results, f, indent=2, ensure_ascii=False)
    
    print("测试结果已保存到: /home/hula/synapse_rust/test_results.json")

def main():
    """主函数"""
    print("="*80)
    print("Synapse Rust 核心客户端API测试")
    print("="*80)
    print(f"测试时间: {datetime.now().isoformat()}")
    print(f"服务器地址: {BASE_URL}")
    print()
    
    # 运行所有测试
    test_1_1_basic_info_and_auth()
    test_1_2_user_profile_management()
    test_1_3_room_management()
    test_1_4_room_state_and_permissions()
    test_1_5_device_management()
    test_1_6_presence()
    
    # 生成测试报告
    generate_report()

if __name__ == "__main__":
    main()
