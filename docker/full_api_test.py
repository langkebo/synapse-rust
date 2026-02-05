#!/usr/bin/env python3
"""
完整的Matrix API测试脚本
"""

import requests
import json
import sys
import time

BASE_URL = "http://localhost:8008"

def get_token(username="testuser1"):
    """读取保存的token"""
    try:
        with open(f"{username}_token.txt", 'r') as f:
            return f.read().strip()
    except:
        return None

def get_user_id(username="testuser1"):
    """读取保存的user_id"""
    try:
        with open(f"{username}_userid.txt", 'r') as f:
            return f.read().strip()
    except:
        return None

def create_room(token, room_alias, name, is_private=True):
    """创建房间"""
    print(f"\n{'='*50}")
    print(f"创建房间: {room_alias}")
    print(f"{'='*50}")
    
    headers = {
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json"
    }
    
    visibility = "private" if is_private else "public"
    
    data = {
        "room_alias_name": room_alias,
        "name": name,
        "visibility": visibility
    }
    
    try:
        response = requests.post(
            f"{BASE_URL}/_matrix/client/r0/createRoom",
            json=data,
            headers=headers,
            timeout=10
        )
        
        if response.status_code == 200:
            result = response.json()
            room_id = result.get('room_id', 'N/A')
            print(f"✓ 房间创建成功!")
            print(f"  Room ID: {room_id}")
            return room_id
        else:
            error = response.json()
            print(f"✗ 创建失败: {error}")
            return None
            
    except Exception as e:
        print(f"✗ 错误: {e}")
        return None

def join_room(token, room_id):
    """加入房间"""
    print(f"  加入房间: {room_id}")
    
    headers = {
        "Authorization": f"Bearer {token}"
    }
    
    try:
        response = requests.post(
            f"{BASE_URL}/_matrix/client/r0/rooms/{room_id}/join",
            headers=headers,
            timeout=10
        )
        
        if response.status_code == 200:
            result = response.json()
            print(f"  ✓ 加入成功! Room ID: {result.get('room_id', room_id)}")
            return True
        else:
            error = response.json()
            print(f"  ✗ 加入失败: {error}")
            return False
    except Exception as e:
        print(f"  ✗ 错误: {e}")
        return False

def send_message(token, room_id, message):
    """发送消息"""
    print(f"\n发送消息: {message}")
    
    headers = {
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json"
    }
    
    data = {
        "msgtype": "m.room.message",
        "body": message,
        "format": "org.matrix.custom.html",
        "formatted_body": f"<b>{message}</b>"
    }
    
    try:
        response = requests.post(
            f"{BASE_URL}/_matrix/client/r0/rooms/{room_id}/send/m.room.message",
            json=data,
            headers=headers,
            timeout=10
        )
        
        if response.status_code == 200:
            result = response.json()
            event_id = result.get('event_id', 'N/A')
            print(f"  ✓ 消息发送成功! Event ID: {event_id}")
            return event_id
        else:
            print(f"  ✗ 发送失败 ({response.status_code}): {response.text[:200]}")
            return None
    except Exception as e:
        print(f"  ✗ 错误: {e}")
        return None

def get_messages(token, room_id):
    """获取房间消息"""
    print(f"\n获取房间消息: {room_id}")
    
    headers = {
        "Authorization": f"Bearer {token}"
    }
    
    try:
        response = requests.get(
            f"{BASE_URL}/_matrix/client/r0/rooms/{room_id}/messages?direction=b&limit=10",
            headers=headers,
            timeout=10
        )
        
        if response.status_code == 200:
            result = response.json()
            chunk = result.get('chunk', [])
            print(f"  ✓ 获取到 {len(chunk)} 条消息")
            for msg in chunk:
                print(f"    - {msg.get('type', 'unknown')}: {msg.get('content', {}).get('body', '')[:50]}")
            return chunk
        else:
            print(f"  ✗ 获取失败 ({response.status_code}): {response.text[:100]}")
            return None
    except Exception as e:
        print(f"  ✗ 错误: {e}")
        return None

def sync_rooms(token):
    """同步数据"""
    print(f"\n{'='*50}")
    print("同步数据")
    print(f"{'='*50}")
    
    headers = {
        "Authorization": f"Bearer {token}"
    }
    
    try:
        response = requests.get(
            f"{BASE_URL}/_matrix/client/r0/sync?timeout=3000",
            headers=headers,
            timeout=15
        )
        
        if response.status_code == 200:
            result = response.json()
            rooms = result.get('rooms', {}).get('join', {})
            print(f"✓ 同步成功! 加入的房间数: {len(rooms)}")
            
            for room_id, room_data in rooms.items():
                name = room_data.get('room_state', {}).get('m.room.name', {}).get('content', {}).get('name', 'Unknown')
                print(f"  - {room_id}: {name}")
            
            return rooms
        else:
            print(f"✗ 同步失败: {response.status_code}")
            return None
    except Exception as e:
        print(f"✗ 错误: {e}")
        return None

def get_profile(token, user_id):
    """获取用户资料"""
    print(f"\n获取用户资料: {user_id}")
    
    headers = {
        "Authorization": f"Bearer {token}"
    }
    
    try:
        response = requests.get(
            f"{BASE_URL}/_matrix/client/r0/profile/{user_id}",
            headers=headers,
            timeout=10
        )
        
        if response.status_code == 200:
            result = response.json()
            print(f"  ✓ 显示名: {result.get('displayname', 'N/A')}")
            print(f"    头像: {result.get('avatar_url', 'N/A')[:50] if result.get('avatar_url') else 'N/A'}")
            return result
        else:
            print(f"  ✗ 获取失败: {response.status_code}")
            return None
    except Exception as e:
        print(f"  ✗ 错误: {e}")
        return None

def main():
    print("\n" + "=" * 60)
    print("   Matrix API 完整测试")
    print("=" * 60)
    
    token = get_token()
    user_id = get_user_id()
    
    if not token:
        print("✗ 无法获取token，请先运行 login_users.py")
        return False
    
    if not user_id:
        user_id = "@testuser1:cjystx.top"
    
    print(f"\n用户: {user_id}")
    print(f"Token: {token[:40]}...")
    
    rooms = []
    
    # 创建或加入测试房间
    test_rooms = [
        ("testroom", "测试房间", True),
        ("publicroom", "公共房间", False),
    ]
    
    for alias, name, is_private in test_rooms:
        room_id = create_room(token, alias, name, is_private)
        if room_id:
            # 尝试加入房间
            join_room(token, room_id)
            rooms.append((alias, room_id))
            time.sleep(0.5)
    
    # 同步查看房间
    print("\n" + "=" * 60)
    print("同步检查房间状态")
    print("=" * 60)
    sync_rooms(token)
    
    # 发送消息到每个房间
    print("\n" + "=" * 60)
    print("发送测试消息")
    print("=" * 60)
    
    for alias, room_id in rooms:
        send_message(token, room_id, f"测试消息 from {alias} - {time.strftime('%H:%M:%S')}")
        time.sleep(0.5)
    
    # 获取消息
    for alias, room_id in rooms:
        get_messages(token, room_id)
    
    # 获取用户资料
    get_profile(token, user_id)
    
    print("\n" + "=" * 60)
    print("   测试完成!")
    print("=" * 60)
    
    print("\n创建的房间:")
    for alias, room_id in rooms:
        print(f"  - {alias}: {room_id}")
    
    return True

if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)
