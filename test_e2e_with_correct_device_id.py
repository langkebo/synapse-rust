#!/usr/bin/env python3
"""
Synapse Rust 端到端加密API测试脚本（使用有效token和正确的device_id）
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
        print(f"✓ 用户 {username} 注册成功")
        return response.json()
    else:
        print(f"✗ 用户 {username} 注册失败: {response.text}")
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
        print(f"✓ 用户 {username} 登录成功")
        return result["user_id"], result["access_token"], result.get("device_id", "")
    else:
        print(f"✗ 用户 {username} 登录失败: {response.text}")
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
        result = response.json()
        print(f"✓ 房间创建成功: {result['room_id']}")
        return result["room_id"]
    else:
        print(f"✗ 房间创建失败: {response.text}")
        return None

def test_e2e_api(user_id, token, device_id, room_id):
    """测试端到端加密API"""
    headers = {"Authorization": f"Bearer {token}"}
    
    print("\n" + "="*80)
    print("端到端加密API测试")
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
    print(f"状态码: {response.status_code}")
    print(f"响应: {response.text if response.text else 'None'}\n")
    
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
    print(f"状态码: {response.status_code}")
    print(f"响应: {response.text if response.text else 'None'}\n")
    
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
    print(f"状态码: {response.status_code}")
    print(f"响应: {response.text if response.text else 'None'}\n")
    
    # 测试4: 获取密钥变更列表
    print("测试4: 获取密钥变更列表")
    url = f"{BASE_URL}/_matrix/client/r0/keys/changes"
    response = requests.get(url, headers=headers)
    print(f"状态码: {response.status_code}")
    print(f"响应: {response.text if response.text else 'None'}\n")
    
    # 测试5: 获取房间密钥分发信息
    print("测试5: 获取房间密钥分发信息")
    url = f"{BASE_URL}/_matrix/client/r0/rooms/{room_id}/keys/distribution"
    response = requests.get(url, headers=headers)
    print(f"状态码: {response.status_code}")
    print(f"响应: {response.text if response.text else 'None'}\n")
    
    # 测试6: 发送设备到设备消息（使用正确的device_id）
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
    print(f"状态码: {response.status_code}")
    print(f"响应: {response.text if response.text else 'None'}\n")

def main():
    """主函数"""
    print("="*80)
    print("端到端加密API测试（使用有效token和正确的device_id）")
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
    
    # 如果没有device_id，使用默认值
    if not device_id:
        device_id = "default_device"
        print(f"警告: 登录响应中没有device_id，使用默认值: {device_id}")
    
    # 创建测试房间
    room_id = create_room(user_id, token)
    
    if not room_id:
        print("房间创建失败，无法继续测试")
        return
    
    # 运行测试
    test_e2e_api(user_id, token, device_id, room_id)
    
    print("="*80)
    print("测试完成")
    print("="*80)

if __name__ == "__main__":
    main()
