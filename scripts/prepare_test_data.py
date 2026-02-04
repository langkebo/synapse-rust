#!/usr/bin/env python3
"""
测试数据准备脚本
为API测试准备必要的测试数据
"""

import requests
import json
import os
import base64
from datetime import datetime

# 配置
BASE_URL = "http://localhost:8008"
ADMIN_USER = {
    "user_id": "@admin:matrix.cjystx.top",
    "password": "Wzc9890951!",
    "access_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYWRtaW46bWF0cml4LmNqeXN0eC50b3AiLCJ1c2VyX2lkIjoiQGFkbWluOm1hdHJpeC5janlzdHgudG9wIiwiYWRtaW4iOnRydWUsImV4cCI6MTc3MDE2NzQyNSwiaWF0IjoxNzcwMTYzODI1LCJkZXZpY2VfaWQiOiJNS21hUjZ3VklpN1A1MXRLIn0.GEv6PcxkxV9W0YPu9I8nKZVDMxTxkftbAoyAAuJ9ja4"
}

TEST_USER1 = {
    "user_id": "@testuser1:matrix.cjystx.top",
    "password": "TestUser123456!",
    "access_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXIxOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkB0ZXN0dXNlcjE6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6dHJ1ZSwiZXhwIjoxNzcwMTY4MDY0LCJpYXQiOjE3NzAxNjQ0NjQsImRldmljZV9pZCI6Ildhb1NUa1RuWkQ3TXVXRnUifQ.iQHcf7gjOs8ktsAE6U5EWK47cU4w7kKEqGrY9QQyR68"
}

TEST_USER2 = {
    "user_id": "@testuser2:matrix.cjystx.top",
    "password": "TestUser123456!",
    "access_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXIyOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkB0ZXN0dXNlcjI6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6ZmFsc2UsImV4cCI6MTc3MDE2NzQ2MCwiaWF0IjoxNzcwMTYzODYwLCJkZXZpY2VfaWQiOiJqUTJWM21MTnljNGJSNmJrIn0.PKcaXm7LW61ukV9wOKQEZ9ObCbFR7FM78vtvlQF_8AU"
}

TEST_ROOM_ID = "!BfUBQVVQfR0EQUmS9kwF_EQ2:matrix.cjystx.top"

# 测试数据存储
test_data = {}

def login(user, password):
    """登录获取Token"""
    response = requests.post(
        f"{BASE_URL}/_matrix/client/r0/login",
        json={
            "type": "m.login.password",
            "user": user,
            "password": password
        }
    )
    if response.status_code == 200:
        return response.json()
    return None

def prepare_voice_message_data():
    """准备语音消息测试数据"""
    print("\n" + "="*80)
    print("准备语音消息测试数据")
    print("="*80)
    
    # 创建测试语音文件
    voice_file_path = "/tmp/test_voice_message.wav"
    with open(voice_file_path, 'wb') as f:
        # 写入一个简单的WAV文件头
        f.write(b'RIFF\x24\x00\x00\x00WAVEfmt \x10\x00\x00\x00\x01\x00\x01\x00\x44\xAC\x00\x00\x88\x58\x01\x00\x02\x00\x10\x00data\x00\x00\x00\x00')
        f.write(b'\x00' * 100)
    
    print(f"创建测试语音文件: {voice_file_path}")
    
    # 上传语音消息
    with open(voice_file_path, 'rb') as f:
        file_content = f.read()
        file_array = list(file_content)
        headers = {"Authorization": f"Bearer {TEST_USER1['access_token']}"}
        response = requests.post(
            f"{BASE_URL}/_matrix/media/v3/upload",
            json={
                "content": file_array,
                "content_type": "audio/wav",
                "filename": "test_voice.wav"
            },
            headers=headers
        )
    
    if response.status_code == 200:
        media_id = response.json().get('content_uri')
        print(f"上传语音消息成功: {media_id}")
        test_data['voice_message_media_id'] = media_id
        test_data['voice_message_file_path'] = voice_file_path
        
        # 发送语音消息到房间
        headers = {"Authorization": f"Bearer {TEST_USER1['access_token']}"}
        response = requests.post(
            f"{BASE_URL}/_matrix/client/r0/rooms/{TEST_ROOM_ID}/send/m.room.message",
            json={
                "msgtype": "m.audio",
                "body": "Test voice message",
                "url": media_id,
                "info": {
                    "mimetype": "audio/wav",
                    "size": 100
                }
            },
            headers=headers
        )
        
        if response.status_code == 200:
            event_id = response.json().get('event_id')
            print(f"发送语音消息成功: {event_id}")
            test_data['voice_message_event_id'] = event_id
        else:
            print(f"发送语音消息失败: {response.status_code} - {response.text}")
    else:
        print(f"上传语音消息失败: {response.status_code} - {response.text}")

def prepare_friend_data():
    """准备好友关系测试数据"""
    print("\n" + "="*80)
    print("准备好友关系测试数据")
    print("="*80)
    
    # 删除现有的好友分类（避免唯一约束冲突）
    headers = {"Authorization": f"Bearer {TEST_USER1['access_token']}"}
    response = requests.delete(
        f"{BASE_URL}/_synapse/enhanced/friend/categories/{TEST_USER1['user_id']}/TestCategory",
        headers=headers
    )
    print(f"删除现有好友分类: {response.status_code}")
    
    # 创建新的好友分类
    response = requests.put(
        f"{BASE_URL}/_synapse/enhanced/friend/categories/{TEST_USER1['user_id']}/NewTestCategory",
        json={"description": "Test category for API testing"},
        headers=headers
    )
    
    if response.status_code == 200:
        print(f"创建好友分类成功: NewTestCategory")
        test_data['friend_category'] = "NewTestCategory"
    else:
        print(f"创建好友分类失败: {response.status_code} - {response.text}")
    
    # 添加好友关系（发送好友请求）
    headers = {"Authorization": f"Bearer {TEST_USER1['access_token']}"}
    response = requests.post(
        f"{BASE_URL}/_synapse/enhanced/friend/request",
        json={
            "user_id": TEST_USER2['user_id'],
            "message": "Test friend request for API testing"
        },
        headers=headers
    )
    
    if response.status_code == 200:
        request_id = response.json().get('request_id')
        print(f"发送好友请求成功: {request_id}")
        test_data['friend_request_id'] = request_id
    else:
        print(f"发送好友请求失败: {response.status_code} - {response.text}")

def prepare_media_file_data():
    """准备媒体文件测试数据"""
    print("\n" + "="*80)
    print("准备媒体文件测试数据")
    print("="*80)
    
    # 创建测试图片文件
    image_file_path = "/tmp/test_image.png"
    with open(image_file_path, 'wb') as f:
        # 写入一个简单的PNG文件
        f.write(b'\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89\x00\x00\x00\nIDATx\x9cc\x00\x01\x00\x00\x05\x00\x01\r\n-\xb4\x00\x00\x00\x00IEND\xaeB`\x82')
    
    print(f"创建测试图片文件: {image_file_path}")
    
    # 上传图片文件
    with open(image_file_path, 'rb') as f:
        file_content = f.read()
        file_array = list(file_content)
        headers = {"Authorization": f"Bearer {TEST_USER1['access_token']}"}
        response = requests.post(
            f"{BASE_URL}/_matrix/media/v3/upload",
            json={
                "content": file_array,
                "content_type": "image/png",
                "filename": "test_image.png"
            },
            headers=headers
        )
    
    if response.status_code == 200:
        media_id = response.json().get('content_uri')
        print(f"上传图片文件成功: {media_id}")
        test_data['media_file_id'] = media_id
        test_data['media_file_path'] = image_file_path
    else:
        print(f"上传图片文件失败: {response.status_code} - {response.text}")

def prepare_private_chat_data():
    """准备私聊会话测试数据"""
    print("\n" + "="*80)
    print("准备私聊会话测试数据")
    print("="*80)
    
    # 确保用户1和用户2是好友关系（接受好友请求）
    headers = {"Authorization": f"Bearer {TEST_USER2['access_token']}"}
    response = requests.post(
        f"{BASE_URL}/_synapse/enhanced/friend/request/{test_data.get('friend_request_id', '0')}/accept",
        headers=headers
    )
    
    if response.status_code == 200:
        print(f"接受好友请求成功")
        test_data['private_chat_ready'] = True
    else:
        print(f"接受好友请求失败: {response.status_code} - {response.text}")

def prepare_key_backup_data():
    """准备密钥备份测试数据"""
    print("\n" + "="*80)
    print("准备密钥备份测试数据")
    print("="*80)
    
    # 创建备份版本
    headers = {"Authorization": f"Bearer {TEST_USER1['access_token']}"}
    response = requests.post(
        f"{BASE_URL}/_matrix/client/r0/room_keys/version",
        json={
            "algorithm": "m.megolm.v1.aes-sha2",
            "auth_data": {
                "public_key": "test_public_key",
                "private_key": "test_private_key"
            }
        },
        headers=headers
    )
    
    if response.status_code == 200:
        version = response.json().get('version')
        print(f"创建密钥备份版本成功: {version}")
        test_data['key_backup_version'] = version
    else:
        print(f"创建密钥备份版本失败: {response.status_code} - {response.text}")

def main():
    """主函数"""
    print("="*80)
    print("测试数据准备脚本")
    print("="*80)
    print(f"准备时间: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    print(f"服务器地址: {BASE_URL}")
    
    # 准备所有测试数据
    prepare_voice_message_data()
    prepare_friend_data()
    prepare_media_file_data()
    prepare_private_chat_data()
    prepare_key_backup_data()
    
    # 保存测试数据到文件
    output_file = "/home/hula/synapse_rust/test_data.json"
    with open(output_file, 'w', encoding='utf-8') as f:
        json.dump({
            "prepare_time": datetime.now().strftime('%Y-%m-%d %H:%M:%S'),
            "base_url": BASE_URL,
            "test_data": test_data
        }, f, indent=2, ensure_ascii=False)
    
    print("\n" + "="*80)
    print("测试数据准备完成")
    print("="*80)
    print(f"测试数据已保存到: {output_file}")
    print("\n准备的数据:")
    for key, value in test_data.items():
        print(f"  - {key}: {value}")

if __name__ == "__main__":
    main()
