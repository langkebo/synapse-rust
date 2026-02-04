#!/usr/bin/env python3
"""
Synapse Rust 媒体文件API测试脚本
测试所有媒体文件API端点，记录测试结果
"""

import requests
import json
import base64
from datetime import datetime

BASE_URL = "http://localhost:8008"

# 测试账号信息
TEST_ACCOUNTS = {
    "testuser1": {
        "username": "testuser1",
        "password": "TestUser123456!",
        "user_id": "@testuser1:matrix.cjystx.top",
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

def test_7_media_file():
    """测试七、媒体文件API"""
    print("\n" + "="*80)
    print("测试 七、媒体文件API")
    print("="*80 + "\n")
    
    token = TEST_ACCOUNTS["testuser1"]["access_token"]
    
    # 1. 获取媒体配置
    print("测试1: 获取媒体配置")
    response, data = make_request("GET", "/_matrix/media/v1/config")
    if response:
        log_result("7. 媒体文件API", "获取媒体配置", "GET", "/_matrix/media/v1/config",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("7. 媒体文件API", "获取媒体配置", "GET", "/_matrix/media/v1/config",
                  None, 200, False, data, None)
    
    # 2. 上传媒体文件（v1）
    print("测试2: 上传媒体文件（v1）")
    # API期望content是整数数组（字节值），而不是base64字符串
    upload_data = {
        "content": list(b"fake image content"),
        "content_type": "image/jpeg",
        "filename": "test.jpg"
    }
    response, data = make_request("POST", "/_matrix/media/v1/upload", data=upload_data, token=token)
    if response:
        log_result("7. 媒体文件API", "上传媒体文件（v1）", "POST", "/_matrix/media/v1/upload",
                  response.status_code, 200, response.status_code == 200, None, data)
        media_id_v1 = data.get("content_uri", "").split("/")[-1] if response.status_code == 200 else None
    else:
        log_result("7. 媒体文件API", "上传媒体文件（v1）", "POST", "/_matrix/media/v1/upload",
                  None, 200, False, data, None)
        media_id_v1 = None
    
    # 3. 上传媒体文件（v3）
    print("测试3: 上传媒体文件（v3）")
    # API期望content是整数数组（字节值），而不是base64字符串
    upload_data = {
        "content": list(b"fake image content 2"),
        "content_type": "image/jpeg",
        "filename": "test2.jpg"
    }
    response, data = make_request("POST", "/_matrix/media/v3/upload", data=upload_data, token=token)
    if response:
        log_result("7. 媒体文件API", "上传媒体文件（v3）", "POST", "/_matrix/media/v3/upload",
                  response.status_code, 200, response.status_code == 200, None, data)
        media_id_v3 = data.get("content_uri", "").split("/")[-1] if response.status_code == 200 else None
    else:
        log_result("7. 媒体文件API", "上传媒体文件（v3）", "POST", "/_matrix/media/v3/upload",
                  None, 200, False, data, None)
        media_id_v3 = None
    
    # 4. 下载媒体文件（v1）
    print("测试4: 下载媒体文件（v1）")
    test_media_id = media_id_v1 if media_id_v1 else "test_media_id"
    response, data = make_request("GET", f"/_matrix/media/v1/download/matrix.cjystx.top/{test_media_id}")
    if response:
        log_result("7. 媒体文件API", "下载媒体文件（v1）", "GET", "/_matrix/media/v1/download/{server_name}/{media_id}",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("7. 媒体文件API", "下载媒体文件（v1）", "GET", "/_matrix/media/v1/download/{server_name}/{media_id}",
                  None, 200, False, data, None)
    
    # 5. 下载媒体文件（r1）
    print("测试5: 下载媒体文件（r1）")
    response, data = make_request("GET", f"/_matrix/media/r1/download/matrix.cjystx.top/{test_media_id}")
    if response:
        log_result("7. 媒体文件API", "下载媒体文件（r1）", "GET", "/_matrix/media/r1/download/{server_name}/{media_id}",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("7. 媒体文件API", "下载媒体文件（r1）", "GET", "/_matrix/media/r1/download/{server_name}/{media_id}",
                  None, 200, False, data, None)
    
    # 6. 下载媒体文件（v3）
    print("测试6: 下载媒体文件（v3）")
    response, data = make_request("GET", f"/_matrix/media/v3/download/matrix.cjystx.top/{test_media_id}")
    if response:
        log_result("7. 媒体文件API", "下载媒体文件（v3）", "GET", "/_matrix/media/v3/download/{server_name}/{media_id}",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("7. 媒体文件API", "下载媒体文件（v3）", "GET", "/_matrix/media/v3/download/{server_name}/{media_id}",
                  None, 200, False, data, None)
    
    # 7. 获取媒体缩略图
    print("测试7: 获取媒体缩略图")
    response, data = make_request("GET", f"/_matrix/media/v3/thumbnail/matrix.cjystx.top/{test_media_id}")
    if response:
        log_result("7. 媒体文件API", "获取媒体缩略图", "GET", "/_matrix/media/v3/thumbnail/{server_name}/{media_id}",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("7. 媒体文件API", "获取媒体缩略图", "GET", "/_matrix/media/v3/thumbnail/{server_name}/{media_id}",
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
    with open("/home/hula/synapse_rust/media_file_api_test_results.json", "w", encoding="utf-8") as f:
        json.dump(test_results, f, indent=2, ensure_ascii=False)
    
    print("测试结果已保存到: /home/hula/synapse_rust/media_file_api_test_results.json")

def main():
    """主函数"""
    print("="*80)
    print("Synapse Rust 媒体文件API测试")
    print("="*80)
    print(f"测试时间: {datetime.now().isoformat()}")
    print(f"服务器地址: {BASE_URL}")
    print()
    
    # 运行所有测试
    test_7_media_file()
    
    # 生成测试报告
    generate_report()

if __name__ == "__main__":
    main()
