#!/usr/bin/env python3
"""
Synapse Rust 私聊API测试脚本
测试所有私聊API端点，记录测试结果
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
dm_room_id = None
session_id = None
message_id = None

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

def test_8_private_chat():
    """测试八、私聊API"""
    print("\n" + "="*80)
    print("测试 八、私聊API")
    print("="*80 + "\n")
    
    token1 = TEST_ACCOUNTS["testuser1"]["access_token"]
    token2 = TEST_ACCOUNTS["testuser2"]["access_token"]
    user_id1 = TEST_ACCOUNTS["testuser1"]["user_id"]
    user_id2 = TEST_ACCOUNTS["testuser2"]["user_id"]
    
    global dm_room_id, session_id, message_id
    
    # 1. 获取所有私聊房间
    print("测试1: 获取所有私聊房间")
    response, data = make_request("GET", "/_matrix/client/r0/dm", token=token1)
    if response:
        log_result("8. 私聊API", "获取所有私聊房间", "GET", "/_matrix/client/r0/dm",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("8. 私聊API", "获取所有私聊房间", "GET", "/_matrix/client/r0/dm",
                  None, 200, False, data, None)
    
    # 2. 创建私聊房间
    print("测试2: 创建私聊房间")
    create_dm_data = {
        "user_id": user_id2
    }
    response, data = make_request("POST", "/_matrix/client/r0/createDM", data=create_dm_data, token=token1)
    if response:
        log_result("8. 私聊API", "创建私聊房间", "POST", "/_matrix/client/r0/createDM",
                  response.status_code, 200, response.status_code == 200, None, data)
        dm_room_id = data.get("room_id") if response.status_code == 200 else None
    else:
        log_result("8. 私聊API", "创建私聊房间", "POST", "/_matrix/client/r0/createDM",
                  None, 200, False, data, None)
        dm_room_id = None
    
    # 3. 获取DM房间详情
    print("测试3: 获取DM房间详情")
    if dm_room_id:
        response, data = make_request("GET", f"/_matrix/client/r0/rooms/{dm_room_id}/dm", token=token1)
        if response:
            log_result("8. 私聊API", "获取DM房间详情", "GET", "/_matrix/client/r0/rooms/{room_id}/dm",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("8. 私聊API", "获取DM房间详情", "GET", "/_matrix/client/r0/rooms/{room_id}/dm",
                      None, 200, False, data, None)
    else:
        print("  跳过：没有可用的DM房间ID")
    
    # 4. 获取未读通知数
    print("测试4: 获取未读通知数")
    if dm_room_id:
        response, data = make_request("GET", f"/_matrix/client/r0/rooms/{dm_room_id}/unread", token=token1)
        if response:
            log_result("8. 私聊API", "获取未读通知数", "GET", "/_matrix/client/r0/rooms/{room_id}/unread",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("8. 私聊API", "获取未读通知数", "GET", "/_matrix/client/r0/rooms/{room_id}/unread",
                      None, 200, False, data, None)
    else:
        print("  跳过：没有可用的DM房间ID")
    
    # 5. 获取私聊会话列表
    print("测试5: 获取私聊会话列表")
    response, data = make_request("GET", "/_synapse/enhanced/private/sessions", token=token1)
    if response:
        log_result("8. 私聊API", "获取私聊会话列表", "GET", "/_synapse/enhanced/private/sessions",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("8. 私聊API", "获取私聊会话列表", "GET", "/_synapse/enhanced/private/sessions",
                  None, 200, False, data, None)
    
    # 6. 创建私聊会话
    print("测试6: 创建私聊会话")
    create_session_data = {
        "other_user_id": user_id2
    }
    response, data = make_request("POST", "/_synapse/enhanced/private/sessions", data=create_session_data, token=token1)
    if response:
        log_result("8. 私聊API", "创建私聊会话", "POST", "/_synapse/enhanced/private/sessions",
                  response.status_code, 200, response.status_code == 200, None, data)
        session_id = data.get("session_id") if response.status_code == 200 else None
    else:
        log_result("8. 私聊API", "创建私聊会话", "POST", "/_synapse/enhanced/private/sessions",
                  None, 200, False, data, None)
        session_id = None
    
    # 7. 获取会话详情
    print("测试7: 获取会话详情")
    if session_id:
        response, data = make_request("GET", f"/_synapse/enhanced/private/sessions/{session_id}", token=token1)
        if response:
            log_result("8. 私聊API", "获取会话详情", "GET", "/_synapse/enhanced/private/sessions/{session_id}",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("8. 私聊API", "获取会话详情", "GET", "/_synapse/enhanced/private/sessions/{session_id}",
                      None, 200, False, data, None)
    else:
        print("  跳过：没有可用的会话ID")
    
    # 8. 删除会话
    print("测试8: 删除会话")
    if session_id:
        response, data = make_request("DELETE", f"/_synapse/enhanced/private/sessions/{session_id}", token=token1)
        if response:
            log_result("8. 私聊API", "删除会话", "DELETE", "/_synapse/enhanced/private/sessions/{session_id}",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("8. 私聊API", "删除会话", "DELETE", "/_synapse/enhanced/private/sessions/{session_id}",
                      None, 200, False, data, None)
    else:
        print("  跳过：没有可用的会话ID")
    
    # 9. 获取会话消息
    print("测试9: 获取会话消息")
    if session_id:
        response, data = make_request("GET", f"/_synapse/enhanced/private/sessions/{session_id}/messages", token=token1)
        if response:
            log_result("8. 私聊API", "获取会话消息", "GET", "/_synapse/enhanced/private/sessions/{session_id}/messages",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("8. 私聊API", "获取会话消息", "GET", "/_synapse/enhanced/private/sessions/{session_id}/messages",
                      None, 200, False, data, None)
    else:
        print("  跳过：没有可用的会话ID")
    
    # 10. 发送会话消息
    print("测试10: 发送会话消息")
    if session_id:
        send_message_data = {
            "message_type": "m.text",
            "content": {
                "body": "Hello, this is a test message!"
            }
        }
        response, data = make_request("POST", f"/_synapse/enhanced/private/sessions/{session_id}/messages", 
                               data=send_message_data, token=token1)
        if response:
            log_result("8. 私聊API", "发送会话消息", "POST", "/_synapse/enhanced/private/sessions/{session_id}/messages",
                      response.status_code, 200, response.status_code == 200, None, data)
            messages = data.get("messages", [])
            if messages:
                message_id = messages[0].get("message_id") if len(messages) > 0 else None
        else:
            log_result("8. 私聊API", "发送会话消息", "POST", "/_synapse/enhanced/private/sessions/{session_id}/messages",
                      None, 200, False, data, None)
    else:
        print("  跳过：没有可用的会话ID")
    
    # 11. 删除消息
    print("测试11: 删除消息")
    if message_id:
        response, data = make_request("DELETE", f"/_synapse/enhanced/private/messages/{message_id}", token=token1)
        if response:
            log_result("8. 私聊API", "删除消息", "DELETE", "/_synapse/enhanced/private/messages/{message_id}",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("8. 私聊API", "删除消息", "DELETE", "/_synapse/enhanced/private/messages/{message_id}",
                      None, 200, False, data, None)
    else:
        print("  跳过：没有可用的消息ID")
    
    # 12. 标记消息已读
    print("测试12: 标记消息已读")
    if message_id:
        response, data = make_request("POST", f"/_synapse/enhanced/private/messages/{message_id}/read", token=token1)
        if response:
            log_result("8. 私聊API", "标记消息已读", "POST", "/_synapse/enhanced/private/messages/{message_id}/read",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("8. 私聊API", "标记消息已读", "POST", "/_synapse/enhanced/private/messages/{message_id}/read",
                      None, 200, False, data, None)
    else:
        print("  跳过：没有可用的消息ID")
    
    # 13. 获取未读消息总数
    print("测试13: 获取未读消息总数")
    response, data = make_request("GET", "/_synapse/enhanced/private/unread-count", token=token1)
    if response:
        log_result("8. 私聊API", "获取未读消息总数", "GET", "/_synapse/enhanced/private/unread-count",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("8. 私聊API", "获取未读消息总数", "GET", "/_synapse/enhanced/private/unread-count",
                  None, 200, False, data, None)
    
    # 14. 搜索私聊消息
    print("测试14: 搜索私聊消息")
    search_data = {
        "query": "test",
        "limit": 10
    }
    response, data = make_request("POST", "/_synapse/enhanced/private/search", data=search_data, token=token1)
    if response:
        log_result("8. 私聊API", "搜索私聊消息", "POST", "/_synapse/enhanced/private/search",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("8. 私聊API", "搜索私聊消息", "POST", "/_synapse/enhanced/private/search",
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
    with open("/home/hula/synapse_rust/private_chat_api_test_results.json", "w", encoding="utf-8") as f:
        json.dump(test_results, f, indent=2, ensure_ascii=False)
    
    print("测试结果已保存到: /home/hula/synapse_rust/private_chat_api_test_results.json")

def main():
    """主函数"""
    print("="*80)
    print("Synapse Rust 私聊API测试")
    print("="*80)
    print(f"测试时间: {datetime.now().isoformat()}")
    print(f"服务器地址: {BASE_URL}")
    print()
    
    # 运行所有测试
    test_8_private_chat()
    
    # 生成测试报告
    generate_report()

if __name__ == "__main__":
    main()
