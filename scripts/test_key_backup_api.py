#!/usr/bin/env python3
"""
Synapse Rust 密钥备份API测试脚本
测试所有密钥备份API端点，记录测试结果
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
    }
}

# 测试结果存储
test_results = []
backup_version = "1"

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

def test_9_key_backup():
    """测试九、密钥备份API"""
    print("\n" + "="*80)
    print("测试 九、密钥备份API")
    print("="*80 + "\n")
    
    token = TEST_ACCOUNTS["testuser1"]["access_token"]
    user_id = TEST_ACCOUNTS["testuser1"]["user_id"]
    
    global backup_version
    
    # 1. 创建备份版本
    print("测试1: 创建备份版本")
    create_version_data = {
        "algorithm": "m.megolm.v1.aes-sha2",
        "auth_data": {
            "public_key": "test_public_key",
            "private_key": "test_private_key"
        }
    }
    response, data = make_request("POST", "/_matrix/client/r0/room_keys/version", data=create_version_data, token=token)
    if response:
        log_result("9. 密钥备份API", "创建备份版本", "POST", "/_matrix/client/r0/room_keys/version",
                  response.status_code, 200, response.status_code == 200, None, data)
        backup_version = data.get("version") if response.status_code == 200 else None
    else:
        log_result("9. 密钥备份API", "创建备份版本", "POST", "/_matrix/client/r0/room_keys/version",
                  None, 200, False, data, None)
        backup_version = None
    
    # 2. 获取备份版本信息
    print("测试2: 获取备份版本信息")
    if backup_version:
        response, data = make_request("GET", f"/_matrix/client/r0/room_keys/version/{backup_version}", token=token)
        if response:
            log_result("9. 密钥备份API", "获取备份版本信息", "GET", "/_matrix/client/r0/room_keys/version/{version}",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("9. 密钥备份API", "获取备份版本信息", "GET", "/_matrix/client/r0/room_keys/version/{version}",
                      None, 200, False, data, None)
    else:
        log_result("9. 密钥备份API", "获取备份版本信息", "GET", "/_matrix/client/r0/room_keys/version/{version}",
                  None, 200, False, {"error": "没有可用的备份版本"}, None)
        print("  跳过：没有可用的备份版本")
    
    # 3. 更新备份版本
    print("测试3: 更新备份版本")
    if backup_version:
        update_version_data = {
            "auth_data": {
                "public_key": "updated_public_key",
                "private_key": "updated_private_key"
            }
        }
        response, data = make_request("PUT", f"/_matrix/client/r0/room_keys/version/{backup_version}", 
                                   data=update_version_data, token=token)
        if response:
            log_result("9. 密钥备份API", "更新备份版本", "PUT", "/_matrix/client/r0/room_keys/version/{version}",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("9. 密钥备份API", "更新备份版本", "PUT", "/_matrix/client/r0/room_keys/version/{version}",
                      None, 200, False, data, None)
    else:
        log_result("9. 密钥备份API", "更新备份版本", "PUT", "/_matrix/client/r0/room_keys/version/{version}",
                  None, 200, False, {"error": "没有可用的备份版本"}, None)
        print("  跳过：没有可用的备份版本")
    
    # 4. 获取所有房间密钥（需要在删除前测试）
    print("测试4: 获取所有房间密钥")
    if backup_version:
        response, data = make_request("GET", f"/_matrix/client/r0/room_keys/{backup_version}", token=token)
        if response:
            log_result("9. 密钥备份API", "获取所有房间密钥", "GET", "/_matrix/client/r0/room_keys/{version}",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("9. 密钥备份API", "获取所有房间密钥", "GET", "/_matrix/client/r0/room_keys/{version}",
                      response.status_code if response else None, 200, response.status_code == 200 if response else False, data, None)
    else:
        log_result("9. 密钥备份API", "获取所有房间密钥", "GET", "/_matrix/client/r0/room_keys/{version}",
                  None, 200, False, {"error": "没有可用的备份版本"}, None)
        print("  跳过：没有可用的备份版本")
    
    # 5. 上传房间密钥（需要在删除前测试）
    print("测试5: 上传房间密钥")
    if backup_version:
        upload_keys_data = {
            "sessions": {
                "!testroom:matrix.cjystx.top": {
                    "session_id": "test_session_1",
                    "first_message_index": 0,
                    "is_verified": True,
                    "session_data": {
                        "algorithm": "m.megolm.v1.aes-sha2",
                        "sender_key": "test_sender_key",
                        "session_key": "test_session_key"
                    }
                }
            }
        }
        response, data = make_request("PUT", f"/_matrix/client/r0/room_keys/{backup_version}", data=upload_keys_data, token=token)
        if response:
            log_result("9. 密钥备份API", "上传房间密钥", "PUT", "/_matrix/client/r0/room_keys/{version}",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("9. 密钥备份API", "上传房间密钥", "PUT", "/_matrix/client/r0/room_keys/{version}",
                      None, 200, False, data, None)
    else:
        log_result("9. 密钥备份API", "上传房间密钥", "PUT", "/_matrix/client/r0/room_keys/{version}",
                  None, 200, False, {"error": "没有可用的备份版本"}, None)
        print("  跳过：没有可用的备份版本")
    
    # 6. 获取指定房间的密钥
    print("测试6: 获取指定房间的密钥")
    if backup_version:
        response, data = make_request("GET", f"/_matrix/client/r0/room_keys/{backup_version}/room/!testroom:matrix.cjystx.top", token=token)
        if response:
            log_result("9. 密钥备份API", "获取指定房间的密钥", "GET", "/_matrix/client/r0/room_keys/{version}/room/{room_id}",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("9. 密钥备份API", "获取指定房间的密钥", "GET", "/_matrix/client/r0/room_keys/{version}/room/{room_id}",
                      response.status_code if response else None, 200, response.status_code == 200 if response else False, data, None)
    else:
        log_result("9. 密钥备份API", "获取指定房间的密钥", "GET", "/_matrix/client/r0/room_keys/{version}/room/{room_id}",
                  None, 200, False, {"error": "没有可用的备份版本"}, None)
        print("  跳过：没有可用的备份版本")
    
    # 7. 获取指定会话的密钥
    print("测试7: 获取指定会话的密钥")
    if backup_version:
        response, data = make_request("GET", f"/_matrix/client/r0/room_keys/{backup_version}/room/!testroom:matrix.cjystx.top/session/test_session_1", token=token)
        if response:
            log_result("9. 密钥备份API", "获取指定会话的密钥", "GET", "/_matrix/client/r0/room_keys/{version}/room/{room_id}/session/{session_id}",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("9. 密钥备份API", "获取指定会话的密钥", "GET", "/_matrix/client/r0/room_keys/{version}/room/{room_id}/session/{session_id}",
                      response.status_code if response else None, 200, response.status_code == 200 if response else False, data, None)
    else:
        log_result("9. 密钥备份API", "获取指定会话的密钥", "GET", "/_matrix/client/r0/room_keys/{version}/room/{room_id}/session/{session_id}",
                  None, 200, False, {"error": "没有可用的备份版本"}, None)
        print("  跳过：没有可用的备份版本")
    
    # 8. 删除备份版本（最后测试删除）
    print("测试8: 删除备份版本")
    if backup_version:
        response, data = make_request("DELETE", f"/_matrix/client/r0/room_keys/version/{backup_version}", token=token)
        if response:
            log_result("9. 密钥备份API", "删除备份版本", "DELETE", "/_matrix/client/r0/room_keys/version/{version}",
                      response.status_code, 200, response.status_code == 200, None, data)
            if response.status_code == 200:
                backup_version = None
        else:
            log_result("9. 密钥备份API", "删除备份版本", "DELETE", "/_matrix/client/r0/room_keys/version/{version}",
                      None, 200, False, data, None)
    else:
        log_result("9. 密钥备份API", "删除备份版本", "DELETE", "/_matrix/client/r0/room_keys/version/{version}",
                  None, 200, False, {"error": "没有可用的备份版本"}, None)
        print("  跳过：没有可用的备份版本")
    
    # 7. 批量上传房间密钥
    print("测试7: 批量上传房间密钥")
    if backup_version:
        batch_upload_data = {
            "rooms": {
                "!testroom:matrix.cjystx.top": {
                    "session_id": "test_session_2",
                    "first_message_index": 0,
                    "is_verified": True,
                    "session_data": {
                        "algorithm": "m.megolm.v1.aes-sha2",
                        "sender_key": "test_sender_key_2",
                        "session_key": "test_session_key_2"
                    }
                }
            }
        }
        response, data = make_request("POST", f"/_matrix/client/r0/room_keys/{backup_version}/keys", 
                                  data=batch_upload_data, token=token)
        if response:
            log_result("9. 密钥备份API", "批量上传房间密钥", "POST", "/_matrix/client/r0/room_keys/{version}/keys",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("9. 密钥备份API", "批量上传房间密钥", "POST", "/_matrix/client/r0/room_keys/{version}/keys",
                      None, 200, False, data, None)
    else:
        print("  跳过：没有可用的备份版本")
    
    # 8. 获取指定房间的密钥
    print("测试8: 获取指定房间的密钥")
    if backup_version:
        response, data = make_request("GET", f"/_matrix/client/r0/room_keys/{backup_version}/keys/!testroom:matrix.cjystx.top", token=token)
        if response:
            log_result("9. 密钥备份API", "获取指定房间的密钥", "GET", "/_matrix/client/r0/room_keys/{version}/keys/{room_id}",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("9. 密钥备份API", "获取指定房间的密钥", "GET", "/_matrix/client/r0/room_keys/{version}/keys/{room_id}",
                      None, 200, False, data, None)
    else:
        print("  跳过：没有可用的备份版本")
    
    # 9. 获取指定会话的密钥
    print("测试9: 获取指定会话的密钥")
    if backup_version:
        response, data = make_request("GET", f"/_matrix/client/r0/room_keys/{backup_version}/keys/!testroom:matrix.cjystx.top/test_session_1", token=token)
        if response:
            log_result("9. 密钥备份API", "获取指定会话的密钥", "GET", "/_matrix/client/r0/room_keys/{version}/keys/{room_id}/{session_id}",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("9. 密钥备份API", "获取指定会话的密钥", "GET", "/_matrix/client/r0/room_keys/{version}/keys/{room_id}/{session_id}",
                      None, 200, False, data, None)
    else:
        print("  跳过：没有可用的备份版本")

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
    with open("/home/hula/synapse_rust/key_backup_api_test_results.json", "w", encoding="utf-8") as f:
        json.dump(test_results, f, indent=2, ensure_ascii=False)
    
    print("测试结果已保存到: /home/hula/synapse_rust/key_backup_api_test_results.json")

def main():
    """主函数"""
    print("="*80)
    print("Synapse Rust 密钥备份API测试")
    print("="*80)
    print(f"测试时间: {datetime.now().isoformat()}")
    print(f"服务器地址: {BASE_URL}")
    print()
    
    # 运行所有测试
    test_9_key_backup()
    
    # 生成测试报告
    generate_report()

if __name__ == "__main__":
    main()
