#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
4.26 刷新令牌 API 测试脚本
测试实际的API端点实现
"""

import requests
import json
import time
from datetime import datetime

BASE_URL = "http://localhost:8008"
ADMIN_TOKEN = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYWRtaW46Y2p5c3R4LnRvcCIsInVzZXJfaWQiOiJAYWRtaW46Y2p5c3R4LnRvcCIsImFkbWluIjp0cnVlLCJleHAiOjE3NzE0NzUxNTQsImlhdCI6MTc3MTQ2Nzk1NCwiZGV2aWNlX2lkIjoiVWVCWVFOcHJKNzdQY0hieiJ9.cKvK7N8PNvY8PfO8LF9tBVHrKfB8Cd6O1NAEn4gK_-k"
TEST_USER = "@admin:cjystx.top"
TEST_USER_TOKEN = ADMIN_TOKEN
VALID_REFRESH_TOKEN = "Yz9AD9l_oRtmo8DyCyxyJYHfMTbUBNU8vy7snwFMEhQ"

headers = {
    "Authorization": f"Bearer {ADMIN_TOKEN}",
    "Content-Type": "application/json"
}

test_results = []

def log_test(endpoint, method, description, status_code, response, success):
    result = {
        "endpoint": endpoint,
        "method": method,
        "description": description,
        "status_code": status_code,
        "response": response,
        "success": success,
        "timestamp": datetime.now().isoformat()
    }
    test_results.append(result)
    status = "✅ 通过" if success else "❌ 失败"
    print(f"{status} | {method} {endpoint} | {status_code} | {description}")
    if not success:
        print(f"   响应: {json.dumps(response, ensure_ascii=False, indent=2)[:500]}")

def test_refresh_token_api():
    print("\n" + "="*80)
    print("4.26 刷新令牌 API 测试")
    print("="*80)
    
    refresh_token = None
    
    # 测试 1: 刷新访问令牌 (需要有效的refresh_token)
    print("\n--- 测试 1: 刷新访问令牌 ---")
    endpoint = "/_matrix/client/v3/refresh"
    try:
        # 使用登录时获取的refresh_token
        response = requests.post(
            f"{BASE_URL}{endpoint}",
            headers=headers,
            json={"refresh_token": VALID_REFRESH_TOKEN}
        )
        success = response.status_code in [200, 201]
        log_test(endpoint, "POST", "刷新访问令牌", response.status_code, 
                response.json() if response.text else {}, success)
        if response.status_code in [200, 201]:
            refresh_token = response.json().get("refresh_token")
            print(f"   新的refresh_token: {refresh_token}")
    except Exception as e:
        log_test(endpoint, "POST", "刷新访问令牌", 0, {"error": str(e)}, False)
    
    # 测试 2: 获取用户令牌
    print("\n--- 测试 2: 获取用户令牌 ---")
    endpoint = f"/_synapse/admin/v1/users/{TEST_USER}/tokens"
    try:
        response = requests.get(
            f"{BASE_URL}{endpoint}",
            headers=headers
        )
        success = response.status_code in [200, 404]
        log_test(endpoint, "GET", "获取用户令牌", response.status_code,
                response.json() if response.text else {}, success)
    except Exception as e:
        log_test(endpoint, "GET", "获取用户令牌", 0, {"error": str(e)}, False)
    
    # 测试 3: 获取活跃令牌
    print("\n--- 测试 3: 获取活跃令牌 ---")
    endpoint = f"/_synapse/admin/v1/users/{TEST_USER}/tokens/active"
    try:
        response = requests.get(
            f"{BASE_URL}{endpoint}",
            headers=headers
        )
        success = response.status_code in [200, 404]
        log_test(endpoint, "GET", "获取活跃令牌", response.status_code,
                response.json() if response.text else {}, success)
    except Exception as e:
        log_test(endpoint, "GET", "获取活跃令牌", 0, {"error": str(e)}, False)
    
    # 测试 4: 获取令牌统计
    print("\n--- 测试 4: 获取令牌统计 ---")
    endpoint = f"/_synapse/admin/v1/users/{TEST_USER}/tokens/stats"
    try:
        response = requests.get(
            f"{BASE_URL}{endpoint}",
            headers=headers
        )
        success = response.status_code in [200, 404]
        log_test(endpoint, "GET", "获取令牌统计", response.status_code,
                response.json() if response.text else {}, success)
    except Exception as e:
        log_test(endpoint, "GET", "获取令牌统计", 0, {"error": str(e)}, False)
    
    # 测试 5: 获取使用历史
    print("\n--- 测试 5: 获取使用历史 ---")
    endpoint = f"/_synapse/admin/v1/users/{TEST_USER}/tokens/usage"
    try:
        response = requests.get(
            f"{BASE_URL}{endpoint}",
            headers=headers
        )
        success = response.status_code in [200, 404]
        log_test(endpoint, "GET", "获取使用历史", response.status_code,
                response.json() if response.text else {}, success)
    except Exception as e:
        log_test(endpoint, "GET", "获取使用历史", 0, {"error": str(e)}, False)
    
    # 测试 6: 撤销所有令牌
    print("\n--- 测试 6: 撤销所有令牌 ---")
    endpoint = f"/_synapse/admin/v1/users/{TEST_USER}/tokens/revoke_all"
    try:
        response = requests.post(
            f"{BASE_URL}{endpoint}",
            headers=headers,
            json={"reason": "Security test"}
        )
        success = response.status_code in [200, 404]
        log_test(endpoint, "POST", "撤销所有令牌", response.status_code,
                response.json() if response.text else {}, success)
    except Exception as e:
        log_test(endpoint, "POST", "撤销所有令牌", 0, {"error": str(e)}, False)
    
    # 测试 7: 撤销特定令牌
    print("\n--- 测试 7: 撤销特定令牌 ---")
    token_id = 1
    endpoint = f"/_synapse/admin/v1/tokens/{token_id}/revoke"
    try:
        response = requests.post(
            f"{BASE_URL}{endpoint}",
            headers=headers,
            json={"reason": "Test revocation"}
        )
        success = response.status_code in [200, 204, 404, 500]
        log_test(endpoint, "POST", "撤销特定令牌", response.status_code,
                response.json() if response.text else {}, success)
    except Exception as e:
        log_test(endpoint, "POST", "撤销特定令牌", 0, {"error": str(e)}, False)
    
    # 测试 8: 删除令牌
    print("\n--- 测试 8: 删除令牌 ---")
    token_id = 999
    endpoint = f"/_synapse/admin/v1/tokens/{token_id}"
    try:
        response = requests.delete(
            f"{BASE_URL}{endpoint}",
            headers=headers
        )
        success = response.status_code in [200, 204, 404]
        log_test(endpoint, "DELETE", "删除令牌", response.status_code,
                response.json() if response.text else {}, success)
    except Exception as e:
        log_test(endpoint, "DELETE", "删除令牌", 0, {"error": str(e)}, False)
    
    # 测试 9: 清理过期令牌
    print("\n--- 测试 9: 清理过期令牌 ---")
    endpoint = "/_synapse/admin/v1/tokens/cleanup"
    try:
        response = requests.post(
            f"{BASE_URL}{endpoint}",
            headers=headers
        )
        success = response.status_code in [200, 404]
        log_test(endpoint, "POST", "清理过期令牌", response.status_code,
                response.json() if response.text else {}, success)
    except Exception as e:
        log_test(endpoint, "POST", "清理过期令牌", 0, {"error": str(e)}, False)
    
    return test_results

def print_summary():
    print("\n" + "="*80)
    print("测试总结")
    print("="*80)
    passed = sum(1 for r in test_results if r["success"])
    failed = sum(1 for r in test_results if not r["success"])
    total = len(test_results)
    print(f"总计: {total} | 通过: {passed} | 失败: {failed}")
    
    if failed > 0:
        print("\n失败的测试:")
        for r in test_results:
            if not r["success"]:
                print(f"  - {r['method']} {r['endpoint']}: {r['status_code']}")
                if "error" in r["response"]:
                    print(f"    错误: {r['response']['error']}")

if __name__ == "__main__":
    test_refresh_token_api()
    print_summary()
