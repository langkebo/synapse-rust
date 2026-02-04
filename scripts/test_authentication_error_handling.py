#!/usr/bin/env python3
"""
认证与错误处理API测试脚本
测试认证机制、错误响应格式、状态码和错误码
"""

import requests
import json
from datetime import datetime

# 配置
BASE_URL = "http://localhost:8008"
ADMIN_USER = {
    "user_id": "@admin:matrix.cjystx.top",
    "password": "Wzc9890951!",
    "access_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXIxOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkB0ZXN0dXNlcjE6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6dHJ1ZSwiZXhwIjoxNzcwMTg0MDUwLCJpYXQiOjE3NzAxODA0NTAsImRldmljZV9pZCI6Ik4zbUhuam1ZWFhxZ3VBZGgifQ.G8092HdzmY_a73l-jvzYBsLTd4TLf2PVOkdkDwAy2X8"
}

TEST_USER = {
    "user_id": "@testuser2:matrix.cjystx.top",
    "password": "TestUser123456!",
    "access_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXIxOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkB0ZXN0dXNlcjE6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6dHJ1ZSwiZXhwIjoxNzcwMTg0MDUwLCJpYXQiOjE3NzAxODA0NTAsImRldmljZV9pZCI6Ik4zbUhuam1ZWFhxZ3VBZGgifQ.G8092HdzmY_a73l-jvzYBsLTd4TLf2PVOkdkDwAy2X8"
}

TEST_ROOM_ID = "!BfUBQVVQfR0EQUmS9kwF_EQ2:matrix.cjystx.top"

# 测试结果存储
test_results = []

def make_request(method, endpoint, data=None, headers=None, expected_status=None, description=""):
    """
    发送HTTP请求并记录结果
    
    Args:
        method: HTTP方法 (GET, POST, PUT, DELETE)
        endpoint: API端点
        data: 请求体数据
        headers: 请求头
        expected_status: 期望的状态码
        description: 测试描述
    
    Returns:
        tuple: (success, response_data, status_code)
    """
    url = f"{BASE_URL}{endpoint}"
    
    try:
        if method == "GET":
            response = requests.get(url, headers=headers)
        elif method == "POST":
            response = requests.post(url, json=data, headers=headers)
        elif method == "PUT":
            response = requests.put(url, json=data, headers=headers)
        elif method == "DELETE":
            response = requests.delete(url, headers=headers)
        else:
            return False, {"error": "Invalid method"}, 0
        
        status_code = response.status_code
        try:
            response_data = response.json()
        except:
            response_data = {"response": response.text}
        
        success = (expected_status is None) or (status_code == expected_status)
        
        result = {
            "test": description,
            "endpoint": endpoint,
            "method": method,
            "expected_status": expected_status,
            "actual_status": status_code,
            "success": success,
            "response": response_data
        }
        
        test_results.append(result)
        
        return success, response_data, status_code
        
    except Exception as e:
        result = {
            "test": description,
            "endpoint": endpoint,
            "method": method,
            "expected_status": expected_status,
            "actual_status": 0,
            "success": False,
            "response": {"error": str(e)}
        }
        test_results.append(result)
        return False, {"error": str(e)}, 0

def test_authentication():
    """测试认证机制"""
    print("\n" + "="*80)
    print("10.1 认证机制测试")
    print("="*80)
    
    # 10.1.1 测试标准用户认证（AuthenticatedUser）
    print("\n10.1.1 标准用户认证（AuthenticatedUser）")
    
    # 测试1: 使用有效Token访问需要认证的API
    print("\n测试1: 使用有效Token访问需要认证的API")
    headers = {"Authorization": f"Bearer {TEST_USER['access_token']}"}
    success, response, status = make_request(
        "GET",
        "/_matrix/client/r0/account/whoami",
        headers=headers,
        expected_status=200,
        description="使用有效Token访问whoami接口"
    )
    print(f"状态: {'✅ 通过' if success else '❌ 失败'}")
    print(f"响应: {json.dumps(response, indent=2, ensure_ascii=False)}")
    
    # 测试2: 使用无效Token访问需要认证的API
    print("\n测试2: 使用无效Token访问需要认证的API")
    headers = {"Authorization": "Bearer invalid_token"}
    success, response, status = make_request(
        "GET",
        "/_matrix/client/r0/account/whoami",
        headers=headers,
        expected_status=401,
        description="使用无效Token访问whoami接口"
    )
    print(f"状态: {'✅ 通过' if success else '❌ 失败'}")
    print(f"响应: {json.dumps(response, indent=2, ensure_ascii=False)}")
    
    # 测试3: 不携带Token访问需要认证的API
    print("\n测试3: 不携带Token访问需要认证的API")
    success, response, status = make_request(
        "GET",
        "/_matrix/client/r0/account/whoami",
        headers={},
        expected_status=401,
        description="不携带Token访问whoami接口"
    )
    print(f"状态: {'✅ 通过' if success else '❌ 失败'}")
    print(f"响应: {json.dumps(response, indent=2, ensure_ascii=False)}")
    
    # 10.1.2 测试管理员认证（AdminUser）
    print("\n10.1.2 管理员认证（AdminUser）")
    
    # 测试4: 使用管理员Token访问管理员API
    print("\n测试4: 使用管理员Token访问管理员API")
    headers = {"Authorization": f"Bearer {ADMIN_USER['access_token']}"}
    success, response, status = make_request(
        "GET",
        "/_synapse/admin/v1/server_version",
        headers=headers,
        expected_status=200,
        description="使用管理员Token访问server_version接口"
    )
    print(f"状态: {'✅ 通过' if success else '❌ 失败'}")
    print(f"响应: {json.dumps(response, indent=2, ensure_ascii=False)}")
    
    # 测试5: 使用普通用户Token访问管理员API
    print("\n测试5: 使用普通用户Token访问管理员API")
    headers = {"Authorization": f"Bearer {TEST_USER['access_token']}"}
    success, response, status = make_request(
        "GET",
        "/_synapse/admin/v1/server_version",
        headers=headers,
        expected_status=403,
        description="使用普通用户Token访问server_version接口"
    )
    print(f"状态: {'✅ 通过' if success else '❌ 失败'}")
    print(f"响应: {json.dumps(response, indent=2, ensure_ascii=False)}")

def test_error_response_format():
    """测试错误响应格式"""
    print("\n" + "="*80)
    print("10.2 错误响应格式测试")
    print("="*80)
    
    # 测试1: 验证错误响应格式
    print("\n测试1: 验证错误响应格式")
    headers = {"Authorization": "Bearer invalid_token"}
    success, response, status = make_request(
        "GET",
        "/_matrix/client/r0/account/whoami",
        headers=headers,
        expected_status=401,
        description="验证错误响应格式"
    )
    print(f"状态: {'✅ 通过' if success else '❌ 失败'}")
    print(f"响应: {json.dumps(response, indent=2, ensure_ascii=False)}")
    
    # 检查响应格式
    if success and isinstance(response, dict):
        has_errcode = "errcode" in response
        has_error = "error" in response
        format_ok = has_errcode and has_error
        print(f"包含errcode字段: {'✅' if has_errcode else '❌'}")
        print(f"包含error字段: {'✅' if has_error else '❌'}")
        print(f"格式正确: {'✅' if format_ok else '❌'}")

def test_status_codes():
    """测试状态码"""
    print("\n" + "="*80)
    print("10.3 状态码测试")
    print("="*80)
    
    # 测试200: 请求成功
    print("\n测试200: 请求成功")
    headers = {"Authorization": f"Bearer {TEST_USER['access_token']}"}
    success, response, status = make_request(
        "GET",
        "/_matrix/client/r0/account/whoami",
        headers=headers,
        expected_status=200,
        description="测试200状态码"
    )
    print(f"状态: {'✅ 通过' if success else '❌ 失败'}")
    print(f"响应: {json.dumps(response, indent=2, ensure_ascii=False)}")
    
    # 测试400: 请求格式错误
    print("\n测试400: 请求格式错误")
    headers = {"Authorization": f"Bearer {TEST_USER['access_token']}"}
    success, response, status = make_request(
        "POST",
        "/_matrix/client/r0/login",
        data={},
        headers=headers,
        expected_status=400,
        description="测试400状态码（缺少必需参数）"
    )
    print(f"状态: {'✅ 通过' if success else '❌ 失败'}")
    print(f"响应: {json.dumps(response, indent=2, ensure_ascii=False)}")
    
    # 测试401: 未授权
    print("\n测试401: 未授权")
    headers = {"Authorization": "Bearer invalid_token"}
    success, response, status = make_request(
        "GET",
        "/_matrix/client/r0/account/whoami",
        headers=headers,
        expected_status=401,
        description="测试401状态码"
    )
    print(f"状态: {'✅ 通过' if success else '❌ 失败'}")
    print(f"响应: {json.dumps(response, indent=2, ensure_ascii=False)}")
    
    # 测试403: 禁止访问
    print("\n测试403: 禁止访问")
    headers = {"Authorization": f"Bearer {TEST_USER['access_token']}"}
    success, response, status = make_request(
        "GET",
        "/_synapse/admin/v1/server_version",
        headers=headers,
        expected_status=403,
        description="测试403状态码（普通用户访问管理员API）"
    )
    print(f"状态: {'✅ 通过' if success else '❌ 失败'}")
    print(f"响应: {json.dumps(response, indent=2, ensure_ascii=False)}")
    
    # 测试404: 资源未找到
    print("\n测试404: 资源未找到")
    headers = {"Authorization": f"Bearer {TEST_USER['access_token']}"}
    success, response, status = make_request(
        "GET",
        "/_matrix/client/r0/rooms/!invalidroomid:server.com/state/m.room.name",
        headers=headers,
        expected_status=404,
        description="测试404状态码（访问不存在的房间）"
    )
    print(f"状态: {'✅ 通过' if success else '❌ 失败'}")
    print(f"响应: {json.dumps(response, indent=2, ensure_ascii=False)}")

def test_error_codes():
    """测试错误码"""
    print("\n" + "="*80)
    print("10.4 错误码测试")
    print("="*80)
    
    # 测试M_UNAUTHORIZED: 认证失败
    print("\n测试M_UNAUTHORIZED: 认证失败")
    headers = {"Authorization": "Bearer invalid_token"}
    success, response, status = make_request(
        "GET",
        "/_matrix/client/r0/account/whoami",
        headers=headers,
        expected_status=401,
        description="测试M_UNAUTHORIZED错误码"
    )
    print(f"状态: {'✅ 通过' if success else '❌ 失败'}")
    print(f"响应: {json.dumps(response, indent=2, ensure_ascii=False)}")
    if success and isinstance(response, dict):
        print(f"错误码: {response.get('errcode', 'N/A')}")
    
    # 测试M_NOT_FOUND: 资源不存在
    print("\n测试M_NOT_FOUND: 资源不存在")
    headers = {"Authorization": f"Bearer {TEST_USER['access_token']}"}
    success, response, status = make_request(
        "GET",
        "/_matrix/client/r0/rooms/!invalidroomid:server.com/state/m.room.name",
        headers=headers,
        expected_status=404,
        description="测试M_NOT_FOUND错误码"
    )
    print(f"状态: {'✅ 通过' if success else '❌ 失败'}")
    print(f"响应: {json.dumps(response, indent=2, ensure_ascii=False)}")
    if success and isinstance(response, dict):
        print(f"错误码: {response.get('errcode', 'N/A')}")
    
    # 测试M_BAD_JSON: JSON格式错误
    print("\n测试M_BAD_JSON: JSON格式错误")
    headers = {"Authorization": f"Bearer {TEST_USER['access_token']}"}
    success, response, status = make_request(
        "POST",
        "/_matrix/client/r0/login",
        data={"invalid": "data"},
        headers=headers,
        expected_status=400,
        description="测试M_BAD_JSON错误码"
    )
    print(f"状态: {'✅ 通过' if success else '❌ 失败'}")
    print(f"响应: {json.dumps(response, indent=2, ensure_ascii=False)}")
    if success and isinstance(response, dict):
        print(f"错误码: {response.get('errcode', 'N/A')}")
    
    # 测试M_FORBIDDEN: 没有权限
    print("\n测试M_FORBIDDEN: 没有权限")
    headers = {"Authorization": f"Bearer {TEST_USER['access_token']}"}
    success, response, status = make_request(
        "GET",
        "/_synapse/admin/v1/server_version",
        headers=headers,
        expected_status=403,
        description="测试M_FORBIDDEN错误码"
    )
    print(f"状态: {'✅ 通过' if success else '❌ 失败'}")
    print(f"响应: {json.dumps(response, indent=2, ensure_ascii=False)}")
    if success and isinstance(response, dict):
        print(f"错误码: {response.get('errcode', 'N/A')}")
    
    # 测试M_MISSING_PARAM: 缺少必需参数
    print("\n测试M_MISSING_PARAM: 缺少必需参数")
    headers = {"Authorization": f"Bearer {TEST_USER['access_token']}"}
    success, response, status = make_request(
        "POST",
        "/_matrix/client/r0/login",
        data={},
        headers=headers,
        expected_status=400,
        description="测试M_MISSING_PARAM错误码"
    )
    print(f"状态: {'✅ 通过' if success else '❌ 失败'}")
    print(f"响应: {json.dumps(response, indent=2, ensure_ascii=False)}")
    if success and isinstance(response, dict):
        print(f"错误码: {response.get('errcode', 'N/A')}")

def main():
    """主函数"""
    print("="*80)
    print("认证与错误处理API测试")
    print("="*80)
    print(f"测试时间: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    print(f"服务器地址: {BASE_URL}")
    
    # 运行所有测试
    test_authentication()
    test_error_response_format()
    test_status_codes()
    test_error_codes()
    
    # 统计测试结果
    print("\n" + "="*80)
    print("测试结果汇总")
    print("="*80)
    
    total_tests = len(test_results)
    passed_tests = sum(1 for result in test_results if result["success"])
    failed_tests = total_tests - passed_tests
    success_rate = (passed_tests / total_tests * 100) if total_tests > 0 else 0
    
    print(f"\n总测试数: {total_tests}")
    print(f"通过数: {passed_tests}")
    print(f"失败数: {failed_tests}")
    print(f"成功率: {success_rate:.2f}%")
    
    # 显示失败的测试
    if failed_tests > 0:
        print("\n失败的测试:")
        for result in test_results:
            if not result["success"]:
                print(f"\n测试: {result['test']}")
                print(f"端点: {result['method']} {result['endpoint']}")
                print(f"期望状态码: {result['expected_status']}")
                print(f"实际状态码: {result['actual_status']}")
                print(f"响应: {json.dumps(result['response'], indent=2, ensure_ascii=False)}")
    
    # 保存测试结果到JSON文件
    output_file = "/home/hula/synapse_rust/authentication_error_handling_test_results.json"
    with open(output_file, 'w', encoding='utf-8') as f:
        json.dump({
            "test_time": datetime.now().strftime('%Y-%m-%d %H:%M:%S'),
            "base_url": BASE_URL,
            "total_tests": total_tests,
            "passed_tests": passed_tests,
            "failed_tests": failed_tests,
            "success_rate": f"{success_rate:.2f}%",
            "test_results": test_results
        }, f, indent=2, ensure_ascii=False)
    
    print(f"\n测试结果已保存到: {output_file}")
    print("\n测试完成!")

if __name__ == "__main__":
    main()
