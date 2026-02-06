#!/usr/bin/env python3
"""
端到端加密API完整测试脚本
测试所有6个端点并生成详细报告
"""

import requests
import json
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
    """登录用户"""
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

def create_room(user_id, token, room_name="Test E2EE Room"):
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

def test_e2e_api(user_id, token, device_id, room_id):
    """测试端到端加密API"""
    headers = {"Authorization": f"Bearer {token}"}
    results = []
    
    print("\n" + "="*80)
    print("端到端加密API完整测试")
    print("="*80 + "\n")
    
    # 测试1: 上传设备密钥
    print("测试1: 上传设备密钥")
    url = f"{BASE_URL}/_matrix/client/r0/keys/upload"
    data = {
        "device_keys": {
            "user_id": user_id,
            "device_id": device_id,
            "algorithms": ["m.olm.v1.curve25519-aes-sha2", "m.megolm.v1.aes-sha2"],
            "keys": {
                "curve25519:" + device_id: "test_key",
                "ed25519:" + device_id: "test_key"
            }
        }
    }
    response = requests.post(url, json=data, headers=headers)
    success = response.status_code == 200
    results.append({
        "test": "上传设备密钥",
        "endpoint": "POST /_matrix/client/r0/keys/upload",
        "status_code": response.status_code,
        "expected": 200,
        "success": success,
        "response": response.text if response.text else None
    })
    print(f"状态码: {response.status_code} {'✓' if success else '✗'}\n")
    
    # 测试2: 查询设备密钥
    print("测试2: 查询设备密钥")
    url = f"{BASE_URL}/_matrix/client/r0/keys/query"
    data = {
        "device_keys": {
            user_id: [device_id]
        },
        "timeout": 10000
    }
    response = requests.post(url, json=data, headers=headers)
    success = response.status_code == 200
    results.append({
        "test": "查询设备密钥",
        "endpoint": "POST /_matrix/client/r0/keys/query",
        "status_code": response.status_code,
        "expected": 200,
        "success": success,
        "response": response.text if response.text else None
    })
    print(f"状态码: {response.status_code} {'✓' if success else '✗'}\n")
    
    # 测试3: 声明一次性密钥
    print("测试3: 声明一次性密钥")
    url = f"{BASE_URL}/_matrix/client/r0/keys/claim"
    data = {
        "one_time_keys": {
            user_id: {
                device_id: "signed_curve25519"
            }
        }
    }
    response = requests.post(url, json=data, headers=headers)
    success = response.status_code == 200
    results.append({
        "test": "声明一次性密钥",
        "endpoint": "POST /_matrix/client/r0/keys/claim",
        "status_code": response.status_code,
        "expected": 200,
        "success": success,
        "response": response.text if response.text else None
    })
    print(f"状态码: {response.status_code} {'✓' if success else '✗'}\n")
    
    # 测试4: 获取密钥变更列表
    print("测试4: 获取密钥变更列表")
    url = f"{BASE_URL}/_matrix/client/r0/keys/changes"
    response = requests.get(url, headers=headers)
    success = response.status_code == 200
    results.append({
        "test": "获取密钥变更列表",
        "endpoint": "GET /_matrix/client/r0/keys/changes",
        "status_code": response.status_code,
        "expected": 200,
        "success": success,
        "response": response.text if response.text else None
    })
    print(f"状态码: {response.status_code} {'✓' if success else '✗'}\n")
    
    # 测试5: 获取房间密钥分发信息
    print("测试5: 获取房间密钥分发信息")
    url = f"{BASE_URL}/_matrix/client/r0/rooms/{room_id}/keys/distribution"
    response = requests.get(url, headers=headers)
    success = response.status_code == 200
    results.append({
        "test": "获取房间密钥分发信息",
        "endpoint": f"GET /_matrix/client/r0/rooms/{room_id}/keys/distribution",
        "status_code": response.status_code,
        "expected": 200,
        "success": success,
        "response": response.text if response.text else None
    })
    print(f"状态码: {response.status_code} {'✓' if success else '✗'}\n")
    
    # 测试6: 发送设备到设备消息
    print("测试6: 发送设备到设备消息")
    url = f"{BASE_URL}/_matrix/client/r0/sendToDevice/m.room_key/test_txn_123"
    data = {
        "messages": {
            user_id: {
                device_id: {
                    "*": {
                        "type": "m.room_key",
                        "content": {
                            "algorithm": "m.megolm.v1.aes-sha2",
                            "room_id": room_id,
                            "session_id": "test_session",
                            "session_key": "test_session_key"
                        }
                    }
                }
            }
        }
    }
    response = requests.put(url, json=data, headers=headers)
    success = response.status_code == 200
    results.append({
        "test": "发送设备到设备消息",
        "endpoint": "PUT /_matrix/client/r0/sendToDevice/{event_type}/{transaction_id}",
        "status_code": response.status_code,
        "expected": 200,
        "success": success,
        "response": response.text if response.text else None
    })
    print(f"状态码: {response.status_code} {'✓' if success else '✗'}\n")
    
    return results

def main():
    """主函数"""
    print("="*80)
    print("端到端加密API完整测试")
    print("="*80 + "\n")
    
    # 创建测试用户
    username = f"e2e_test_{datetime.now().strftime('%Y%m%d%H%M%S')}"
    password = "TestPassword123!"
    
    print(f"创建测试用户: {username}")
    register_result = register_user(username, password)
    
    if not register_result:
        print("注册失败，尝试登录已存在用户...")
        username = "testuser1"
        password = "TestUser123456!"
    
    # 登录获取token和device_id
    user_id, token, device_id = login_user(username, password)
    
    if not user_id or not token:
        print("登录失败，无法继续测试")
        return
    
    print(f"\n用户ID: {user_id}")
    print(f"Token: {token[:50]}...")
    print(f"Device ID: {device_id}\n")
    
    if not device_id:
        device_id = "default_device"
        print(f"警告: 登录响应中没有device_id，使用默认值: {device_id}")
    
    # 创建测试房间
    room_id = create_room(user_id, token)
    
    if not room_id:
        print("房间创建失败，无法继续测试")
        return
    
    # 运行测试
    results = test_e2e_api(user_id, token, device_id, room_id)
    
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
    
    with open("/home/hula/synapse_rust/e2e_test_report.json", "w") as f:
        json.dump(report, f, indent=2)
    
    print(f"\n测试报告已保存到: /home/hula/synapse_rust/e2e_test_report.json")

if __name__ == "__main__":
    main()
