#!/usr/bin/env python3
"""
语音消息API完整测试脚本（使用有效token）
测试所有7个语音消息API端点
"""

import requests
import json
import base64
from datetime import datetime

BASE_URL = "http://localhost:8008"

def register_user(username, password):
    """注册新用户"""
    url = f"{BASE_URL}/_matrix/client/r0/register"
    data = {
        "username": username,
        "password": password,
        "auth": {"type": "m.login.dummy"}
    }
    response = requests.post(url, json=data)
    if response.status_code == 200:
        return response.json()
    return None

def login_user(username, password):
    """登录用户获取有效token"""
    url = f"{BASE_URL}/_matrix/client/r0/login"
    data = {
        "type": "m.login.password",
        "user": username,
        "password": password
    }
    response = requests.post(url, json=data)
    if response.status_code == 200:
        result = response.json()
        return result["user_id"], result["access_token"], result.get("device_id", "")
    return None, None, None

def create_room(user_id, token, room_name="Voice Test Room"):
    """创建测试房间"""
    url = f"{BASE_URL}/_matrix/client/r0/createRoom"
    headers = {"Authorization": f"Bearer {token}"}
    data = {
        "name": room_name,
        "preset": "private_chat",
        "is_direct": True
    }
    response = requests.post(url, json=data, headers=headers)
    if response.status_code == 200:
        return response.json()["room_id"]
    return None

def test_voice_api(user_id, token, room_id):
    """测试语音消息API"""
    headers = {"Authorization": f"Bearer {token}"}
    results = []
    
    print("\n" + "="*80)
    print("语音消息API完整测试")
    print("="*80 + "\n")
    
    uploaded_message_id = None
    
    # 测试1: 上传语音消息
    print("测试1: 上传语音消息")
    url = f"{BASE_URL}/_matrix/client/r0/voice/upload"
    voice_data = base64.b64encode(b"test audio content for voice message").decode()
    data = {
        "content": voice_data,
        "content_type": "audio/ogg",
        "duration_ms": 1000,
        "room_id": room_id,
        "session_id": "test_session_123"
    }
    response = requests.post(url, json=data, headers=headers)
    success = response.status_code == 200
    if success:
        response_data = response.json()
        uploaded_message_id = response_data.get("message_id")
        print(f"状态码: {response.status_code} ✓")
        print(f"上传成功，message_id: {uploaded_message_id}")
    else:
        print(f"状态码: {response.status_code} ✗")
        print(f"错误: {response.text}")
    
    results.append({
        "test": "上传语音消息",
        "endpoint": "POST /_matrix/client/r0/voice/upload",
        "status_code": response.status_code,
        "expected": 200,
        "success": success,
        "response": response.text if response.text else None,
        "message_id": uploaded_message_id
    })
    print()
    
    # 测试2: 获取当前用户语音统计
    print("测试2: 获取当前用户语音统计")
    url = f"{BASE_URL}/_matrix/client/r0/voice/stats"
    response = requests.get(url, headers=headers)
    success = response.status_code == 200
    print(f"状态码: {response.status_code} {'✓' if success else '✗'}")
    if success:
        print(f"响应: {response.text[:100]}...")
    else:
        print(f"错误: {response.text}")
    
    results.append({
        "test": "获取当前用户语音统计",
        "endpoint": "GET /_matrix/client/r0/voice/stats",
        "status_code": response.status_code,
        "expected": 200,
        "success": success,
        "response": response.text if response.text else None
    })
    print()
    
    # 测试3: 获取语音消息
    print("测试3: 获取语音消息")
    message_id = uploaded_message_id or "test_message_123"
    url = f"{BASE_URL}/_matrix/client/r0/voice/{message_id}"
    response = requests.get(url)
    success = response.status_code == 200
    print(f"状态码: {response.status_code} {'✓' if success else '✗'}")
    if success:
        print(f"响应: {response.text[:100]}...")
    else:
        print(f"错误: {response.text}")
    
    results.append({
        "test": "获取语音消息",
        "endpoint": f"GET /_matrix/client/r0/voice/{message_id}",
        "status_code": response.status_code,
        "expected": 200,
        "success": success,
        "response": response.text if response.text else None
    })
    print()
    
    # 测试4: 删除语音消息
    print("测试4: 删除语音消息")
    url = f"{BASE_URL}/_matrix/client/r0/voice/{message_id}"
    response = requests.delete(url, headers=headers)
    success = response.status_code == 200
    print(f"状态码: {response.status_code} {'✓' if success else '✗'}")
    if success:
        print(f"响应: {response.text}")
    else:
        print(f"错误: {response.text}")
    
    results.append({
        "test": "删除语音消息",
        "endpoint": f"DELETE /_matrix/client/r0/voice/{message_id}",
        "status_code": response.status_code,
        "expected": 200,
        "success": success,
        "response": response.text if response.text else None
    })
    print()
    
    # 测试5: 获取用户语音消息
    print("测试5: 获取用户语音消息")
    url = f"{BASE_URL}/_matrix/client/r0/voice/user/{user_id}"
    response = requests.get(url)
    success = response.status_code == 200
    print(f"状态码: {response.status_code} {'✓' if success else '✗'}")
    if success:
        print(f"响应: {response.text[:100]}...")
    else:
        print(f"错误: {response.text}")
    
    results.append({
        "test": "获取用户语音消息",
        "endpoint": f"GET /_matrix/client/r0/voice/user/{user_id}",
        "status_code": response.status_code,
        "expected": 200,
        "success": success,
        "response": response.text if response.text else None
    })
    print()
    
    # 测试6: 获取房间语音消息
    print("测试6: 获取房间语音消息")
    url = f"{BASE_URL}/_matrix/client/r0/voice/room/{room_id}"
    response = requests.get(url)
    success = response.status_code == 200
    print(f"状态码: {response.status_code} {'✓' if success else '✗'}")
    if success:
        print(f"响应: {response.text[:100]}...")
    else:
        print(f"错误: {response.text}")
    
    results.append({
        "test": "获取房间语音消息",
        "endpoint": f"GET /_matrix/client/r0/voice/room/{room_id}",
        "status_code": response.status_code,
        "expected": 200,
        "success": success,
        "response": response.text if response.text else None
    })
    print()
    
    # 测试7: 获取指定用户语音统计
    print("测试7: 获取指定用户语音统计")
    url = f"{BASE_URL}/_matrix/client/r0/voice/user/{user_id}/stats"
    response = requests.get(url)
    success = response.status_code == 200
    print(f"状态码: {response.status_code} {'✓' if success else '✗'}")
    if success:
        print(f"响应: {response.text[:100]}...")
    else:
        print(f"错误: {response.text}")
    
    results.append({
        "test": "获取指定用户语音统计",
        "endpoint": f"GET /_matrix/client/r0/voice/user/{user_id}/stats",
        "status_code": response.status_code,
        "expected": 200,
        "success": success,
        "response": response.text if response.text else None
    })
    print()
    
    return results

def main():
    """主函数"""
    print("="*80)
    print("语音消息API完整测试（使用有效token）")
    print("="*80 + "\n")
    
    # 创建新测试用户
    username = f"voice_test_{datetime.now().strftime('%Y%m%d%H%M%S')}"
    password = "TestPassword123!"
    
    print(f"创建测试用户: {username}")
    register_result = register_user(username, password)
    
    if not register_result:
        print("注册失败，尝试使用已有用户...")
        username = "testuser1"
        password = "TestUser123!"
    
    # 登录获取token
    user_id, token, device_id = login_user(username, password)
    
    if not user_id or not token:
        print("登录失败，无法继续测试")
        print("请确保服务正在运行并且用户存在")
        return
    
    print(f"\n用户ID: {user_id}")
    print(f"Token: {token[:50]}...")
    print(f"Device ID: {device_id}\n")
    
    # 创建测试房间
    room_id = create_room(user_id, token)
    
    if not room_id:
        print("房间创建失败，无法继续测试")
        return
    
    print(f"房间ID: {room_id}\n")
    
    # 运行测试
    results = test_voice_api(user_id, token, room_id)
    
    # 生成报告
    print("="*80)
    print("测试报告汇总")
    print("="*80 + "\n")
    
    total_tests = len(results)
    passed_tests = sum(1 for r in results if r["success"])
    failed_tests = total_tests - passed_tests
    
    print(f"总测试数: {total_tests}")
    print(f"通过: {passed_tests}")
    print(f"失败: {failed_tests}")
    print(f"成功率: {passed_tests/total_tests*100:.1f}%\n")
    
    print("详细结果:")
    print("-" * 80)
    for i, result in enumerate(results, 1):
        status = "✓ PASS" if result["success"] else "✗ FAIL"
        print(f"{i}. {result['test']}: {status}")
        print(f"   端点: {result['endpoint']}")
        print(f"   状态码: {result['status_code']} (期望: {result['expected']})")
        if not result["success"]:
            print(f"   响应: {result['response']}")
        print()
    
    print("="*80)
    print("测试完成")
    print("="*80)
    
    # 保存结果到文件
    report = {
        "timestamp": datetime.now().isoformat(),
        "total_tests": total_tests,
        "passed_tests": passed_tests,
        "failed_tests": failed_tests,
        "success_rate": f"{passed_tests/total_tests*100:.1f}%",
        "results": results
    }
    
    with open("/home/hula/synapse_rust/voice_api_test_report.json", "w") as f:
        json.dump(report, f, indent=2)
    
    print(f"\n测试报告已保存到: /home/hula/synapse_rust/voice_api_test_report.json")

if __name__ == "__main__":
    main()
