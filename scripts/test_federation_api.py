#!/usr/bin/env python3
"""
Synapse Rust 联邦通信API测试脚本
测试所有联邦通信API端点，记录测试结果
"""

import requests
import json
from datetime import datetime

BASE_URL = "http://localhost:8008"

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

def make_request(method, endpoint, data=None, params=None, headers=None):
    """发送HTTP请求"""
    url = f"{BASE_URL}{endpoint}"
    
    if headers is None:
        headers = {}
    
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

def test_3_1_public_endpoints():
    """测试3.1 公开端点（无需认证）"""
    print("\n" + "="*80)
    print("测试 3.1 公开端点（无需认证）")
    print("="*80 + "\n")
    
    # 1. 获取服务器签名密钥
    print("测试1: 获取服务器签名密钥")
    response, data = make_request("GET", "/_matrix/federation/v2/server")
    if response:
        log_result("3.1 公开端点", "获取服务器签名密钥", "GET", "/_matrix/federation/v2/server",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("3.1 公开端点", "获取服务器签名密钥", "GET", "/_matrix/federation/v2/server",
                  None, 200, False, data, None)
    
    # 2. 获取服务器密钥（v1兼容）
    print("测试2: 获取服务器密钥（v1兼容）")
    response, data = make_request("GET", "/_matrix/key/v2/server")
    if response:
        log_result("3.1 公开端点", "获取服务器密钥（v1兼容）", "GET", "/_matrix/key/v2/server",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("3.1 公开端点", "获取服务器密钥（v1兼容）", "GET", "/_matrix/key/v2/server",
                  None, 200, False, data, None)
    
    # 3. 查询服务器密钥
    print("测试3: 查询服务器密钥")
    response, data = make_request("GET", "/_matrix/federation/v2/query/matrix.cjystx.top/ed25519")
    if response:
        log_result("3.1 公开端点", "查询服务器密钥", "GET", "/_matrix/federation/v2/query/{server_name}/{key_id}",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("3.1 公开端点", "查询服务器密钥", "GET", "/_matrix/federation/v2/query/{server_name}/{key_id}",
                  None, 200, False, data, None)
    
    # 4. 获取联邦版本信息
    print("测试4: 获取联邦版本信息")
    response, data = make_request("GET", "/_matrix/federation/v1/version")
    if response:
        log_result("3.1 公开端点", "获取联邦版本信息", "GET", "/_matrix/federation/v1/version",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("3.1 公开端点", "获取联邦版本信息", "GET", "/_matrix/federation/v1/version",
                  None, 200, False, data, None)
    
    # 5. 联邦服务发现
    print("测试5: 联邦服务发现")
    response, data = make_request("GET", "/_matrix/federation/v1")
    if response:
        log_result("3.1 公开端点", "联邦服务发现", "GET", "/_matrix/federation/v1",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("3.1 公开端点", "联邦服务发现", "GET", "/_matrix/federation/v1",
                  None, 200, False, data, None)
    
    # 6. 获取公共房间列表
    print("测试6: 获取公共房间列表")
    response, data = make_request("GET", "/_matrix/federation/v1/publicRooms", params={"limit": 10})
    if response:
        log_result("3.1 公开端点", "获取公共房间列表", "GET", "/_matrix/federation/v1/publicRooms",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("3.1 公开端点", "获取公共房间列表", "GET", "/_matrix/federation/v1/publicRooms",
                  None, 200, False, data, None)

def test_3_2_protected_endpoints():
    """测试3.2 保护端点（需要联邦认证）"""
    print("\n" + "="*80)
    print("测试 3.2 保护端点（需要联邦认证）")
    print("="*80 + "\n")
    print("注意：保护端点需要服务器之间的联邦认证，")
    print("      在单服务器测试环境中可能无法正常工作")
    print("      以下测试为基本连通性测试\n")
    
    # 测试几个保护端点的连通性
    protected_endpoints = [
        ("发送事务", "PUT", "/_matrix/federation/v1/send/test_txn_123", {"origin": "matrix.cjystx.top", "pdus": []}),
        ("生成加入事件模板", "GET", "/_matrix/federation/v1/make_join/!testroom:matrix.cjystx.top/@testuser1:matrix.cjystx.top", None),
        ("获取房间状态", "GET", "/_matrix/federation/v1/state/!testroom:matrix.cjystx.top", None),
        ("获取事件授权链", "GET", "/_matrix/federation/v1/get_event_auth/!testroom:matrix.cjystx.top/$event", None),
    ]
    
    for api_name, method, endpoint, data in protected_endpoints:
        print(f"测试: {api_name}")
        response, response_data = make_request(method, endpoint, data=data)
        if response:
            # 保护端点可能返回401或其他错误，这是正常的
            log_result("3.2 保护端点", api_name, method, endpoint,
                      response.status_code, 200, response.status_code == 200, None)
        else:
            log_result("3.2 保护端点", api_name, method, endpoint,
                      None, 200, False, None)

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
    with open("/home/hula/synapse_rust/federation_api_test_results.json", "w", encoding="utf-8") as f:
        json.dump(test_results, f, indent=2, ensure_ascii=False)
    
    print("测试结果已保存到: /home/hula/synapse_rust/federation_api_test_results.json")

def main():
    """主函数"""
    print("="*80)
    print("Synapse Rust 联邦通信API测试")
    print("="*80)
    print(f"测试时间: {datetime.now().isoformat()}")
    print(f"服务器地址: {BASE_URL}")
    print()
    print("注意：联邦通信API用于服务器之间的通信，")
    print("      在单服务器测试环境中，保护端点可能无法正常工作")
    print()
    
    # 运行所有测试
    test_3_1_public_endpoints()
    test_3_2_protected_endpoints()
    
    # 生成测试报告
    generate_report()

if __name__ == "__main__":
    main()
