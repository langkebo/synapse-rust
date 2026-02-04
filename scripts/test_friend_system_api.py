#!/usr/bin/env python3
"""
Synapse Rust 好友系统API测试脚本
测试所有好友系统API端点，记录测试结果
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

def test_6_friend_system():
    """测试六、好友系统API"""
    print("\n" + "="*80)
    print("测试 六、好友系统API")
    print("="*80 + "\n")
    
    token1 = TEST_ACCOUNTS["testuser1"]["access_token"]
    token2 = TEST_ACCOUNTS["testuser2"]["access_token"]
    user_id1 = TEST_ACCOUNTS["testuser1"]["user_id"]
    user_id2 = TEST_ACCOUNTS["testuser2"]["user_id"]
    
    # 1. 搜索用户
    print("测试1: 搜索用户")
    response, data = make_request("GET", "/_synapse/enhanced/friends/search", params={"query": "testuser"}, token=token1)
    if response:
        log_result("6. 好友系统API", "搜索用户", "GET", "/_synapse/enhanced/friends/search",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("6. 好友系统API", "搜索用户", "GET", "/_synapse/enhanced/friends/search",
                  None, 200, False, data, None)
    
    # 2. 获取好友列表
    print("测试2: 获取好友列表")
    response, data = make_request("GET", "/_synapse/enhanced/friends", token=token1)
    if response:
        log_result("6. 好友系统API", "获取好友列表", "GET", "/_synapse/enhanced/friends",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("6. 好友系统API", "获取好友列表", "GET", "/_synapse/enhanced/friends",
                  None, 200, False, data, None)
    
    # 3. 发送好友请求
    print("测试3: 发送好友请求")
    friend_request_data = {
        "user_id": user_id2,
        "message": "Hello, let's be friends!"
    }
    response, data = make_request("POST", "/_synapse/enhanced/friend/request", data=friend_request_data, token=token1)
    if response:
        log_result("6. 好友系统API", "发送好友请求", "POST", "/_synapse/enhanced/friend/request",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("6. 好友系统API", "发送好友请求", "POST", "/_synapse/enhanced/friend/request",
                  None, 200, False, data, None)
    
    # 4. 获取好友请求列表
    print("测试4: 获取好友请求列表")
    response, data = make_request("GET", "/_synapse/enhanced/friend/requests", token=token2)
    if response:
        log_result("6. 好友系统API", "获取好友请求列表", "GET", "/_synapse/enhanced/friend/requests",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("6. 好友系统API", "获取好友请求列表", "GET", "/_synapse/enhanced/friend/requests",
                  None, 200, False, data, None)
    
    # 5. 获取封禁用户列表
    print("测试5: 获取封禁用户列表")
    response, data = make_request("GET", f"/_synapse/enhanced/friend/blocks/{user_id1}", token=token1)
    if response:
        log_result("6. 好友系统API", "获取封禁用户列表", "GET", "/_synapse/enhanced/friend/blocks/{user_id}",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("6. 好友系统API", "获取封禁用户列表", "GET", "/_synapse/enhanced/friend/blocks/{user_id}",
                  None, 200, False, data, None)
    
    # 6. 获取好友分类
    print("测试6: 获取好友分类")
    response, data = make_request("GET", f"/_synapse/enhanced/friend/categories/{user_id1}", token=token1)
    if response:
        log_result("6. 好友系统API", "获取好友分类", "GET", "/_synapse/enhanced/friend/categories/{user_id}",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("6. 好友系统API", "获取好友分类", "GET", "/_synapse/enhanced/friend/categories/{user_id}",
                  None, 200, False, data, None)
    
    # 7. 创建好友分类
    print("测试7: 创建好友分类")
    category_data = {
        "name": "Test Category",
        "color": "#FF0000"
    }
    response, data = make_request("POST", f"/_synapse/enhanced/friend/categories/{user_id1}", data=category_data, token=token1)
    if response:
        log_result("6. 好友系统API", "创建好友分类", "POST", "/_synapse/enhanced/friend/categories/{user_id}",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("6. 好友系统API", "创建好友分类", "POST", "/_synapse/enhanced/friend/categories/{user_id}",
                  None, 200, False, data, None)
    
    # 8. 获取好友推荐
    print("测试8: 获取好友推荐")
    response, data = make_request("GET", f"/_synapse/enhanced/friend/recommendations/{user_id1}", token=token1)
    if response:
        log_result("6. 好友系统API", "获取好友推荐", "GET", "/_synapse/enhanced/friend/recommendations/{user_id}",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("6. 好友系统API", "获取好友推荐", "GET", "/_synapse/enhanced/friend/recommendations/{user_id}",
                  None, 200, False, data, None)
    
    # 9. 更新好友分类
    print("测试9: 更新好友分类")
    update_category_data = {
        "name": "Test Category Updated",
        "color": "#00FF00"
    }
    response, data = make_request("PUT", f"/_synapse/enhanced/friend/categories/{user_id1}/Test Category", 
                               data=update_category_data, token=token1)
    if response:
        log_result("6. 好友系统API", "更新好友分类", "PUT", "/_synapse/enhanced/friend/categories/{user_id}/{category_name}",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("6. 好友系统API", "更新好友分类", "PUT", "/_synapse/enhanced/friend/categories/{user_id}/{category_name}",
                  None, 200, False, data, None)
    
    # 10. 删除好友分类
    print("测试10: 删除好友分类")
    response, data = make_request("DELETE", f"/_synapse/enhanced/friend/categories/{user_id1}/Test Category", token=token1)
    if response:
        log_result("6. 好友系统API", "删除好友分类", "DELETE", "/_synapse/enhanced/friend/categories/{user_id}/{category_name}",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("6. 好友系统API", "删除好友分类", "DELETE", "/_synapse/enhanced/friend/categories/{user_id}/{category_name}",
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
    with open("/home/hula/synapse_rust/friend_system_api_test_results.json", "w", encoding="utf-8") as f:
        json.dump(test_results, f, indent=2, ensure_ascii=False)
    
    print("测试结果已保存到: /home/hula/synapse_rust/friend_system_api_test_results.json")

def main():
    """主函数"""
    print("="*80)
    print("Synapse Rust 好友系统API测试")
    print("="*80)
    print(f"测试时间: {datetime.now().isoformat()}")
    print(f"服务器地址: {BASE_URL}")
    print()
    
    # 运行所有测试
    test_6_friend_system()
    
    # 生成测试报告
    generate_report()

if __name__ == "__main__":
    main()
