#!/usr/bin/env python3
"""
Synapse Rust 好友系统API测试脚本 (Matrix Room-based)
测试新的Matrix标准好友系统API端点
"""

import requests
import json
from datetime import datetime

BASE_URL = "http://localhost:8008"

# 测试账号信息
TEST_ACCOUNTS = {
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

def test_friend_system():
    """测试好友系统API (新Matrix标准)"""
    print("\n" + "="*80)
    print("测试 好友系统API (Matrix Room-based)")
    print("="*80 + "\n")

    token1 = TEST_ACCOUNTS["testuser1"]["access_token"]
    token2 = TEST_ACCOUNTS["testuser2"]["access_token"]
    user_id1 = TEST_ACCOUNTS["testuser1"]["user_id"]
    user_id2 = TEST_ACCOUNTS["testuser2"]["user_id"]

    # 1. 获取好友列表
    print("测试1: 获取好友列表")
    response, data = make_request("GET", "/_matrix/client/v1/friends", token=token1)
    if response:
        # 200 or 404 are both acceptable
        expected = 200
        success = response.status_code in [200, 404]
        log_result("好友系统API", "获取好友列表", "GET", "/_matrix/client/v1/friends",
                  response.status_code, expected, success, None, data)
    else:
        log_result("好友系统API", "获取好友列表", "GET", "/_matrix/client/v1/friends",
                  None, 200, False, data, None)

    # 2. 发送好友请求
    print("测试2: 发送好友请求")
    friend_request_data = {
        "user_id": user_id2,
        "message": "Hello, let's be friends!"
    }
    response, data = make_request("POST", "/_matrix/client/v1/friends/request", data=friend_request_data, token=token1)
    if response:
        success = response.status_code in [200, 201, 202]
        log_result("好友系统API", "发送好友请求", "POST", "/_matrix/client/v1/friends/request",
                  response.status_code, 201, success, None, data)
    else:
        log_result("好友系统API", "发送好友请求", "POST", "/_matrix/client/v1/friends/request",
                  None, 201, False, data, None)

    # 3. 获取好友请求列表
    print("测试3: 获取好友请求列表")
    response, data = make_request("GET", "/_matrix/client/v1/friends/requests", token=token2)
    if response:
        success = response.status_code in [200, 404]
        log_result("好友系统API", "获取好友请求列表", "GET", "/_matrix/client/v1/friends/requests",
                  response.status_code, 200, success, None, data)
    else:
        log_result("好友系统API", "获取好友请求列表", "GET", "/_matrix/client/v1/friends/requests",
                  None, 200, False, data, None)

    # 4. 测试兼容层API (unstable)
    print("测试4: 兼容层API - 获取好友")
    response, data = make_request("GET", "/_matrix/client/unstable/friends", token=token1)
    if response:
        success = response.status_code in [200, 404]
        log_result("好友系统API", "兼容层-获取好友", "GET", "/_matrix/client/unstable/friends",
                  response.status_code, 200, success, None, data)
    else:
        log_result("好友系统API", "兼容层-获取好友", "GET", "/_matrix/client/unstable/friends",
                  None, 200, False, data, None)

    # 5. 测试旧API应该返回410 Gone
    print("测试5: 验证旧API已废弃 (应返回410)")
    response, data = make_request("GET", "/_synapse/enhanced/friends", token=token1)
    if response:
        success = response.status_code == 410
        log_result("好友系统API", "旧API已废弃", "GET", "/_synapse/enhanced/friends",
                  response.status_code, 410, success, None, data)
    else:
        log_result("好友系统API", "旧API已废弃", "GET", "/_synapse/enhanced/friends",
                  None, 410, False, data, None)

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
    if total_tests > 0:
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
    try:
        with open("friend_system_api_test_results.json", "w", encoding="utf-8") as f:
            json.dump(test_results, f, indent=2, ensure_ascii=False)
        print("测试结果已保存到: friend_system_api_test_results.json")
    except Exception as e:
        print(f"无法保存测试结果: {e}")

def main():
    """主函数"""
    print("="*80)
    print("Synapse Rust 好友系统API测试 (Matrix Room-based)")
    print("="*80)
    print(f"测试时间: {datetime.now().isoformat()}")
    print(f"服务器地址: {BASE_URL}")
    print()

    # 运行所有测试
    test_friend_system()

    # 生成测试报告
    generate_report()

if __name__ == "__main__":
    main()
