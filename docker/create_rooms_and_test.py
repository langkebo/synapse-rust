#!/usr/bin/env python3
"""
创建测试房间并执行API测试
"""

import requests
import json
import sys
import time

BASE_URL = "http://localhost:8008"

def get_token():
    """读取保存的token"""
    try:
        with open("testuser1_token.txt", 'r') as f:
            return f.read().strip()
    except:
        return None

def create_room(token, room_alias, name, is_private=True):
    """创建房间"""
    print(f"\n=== 创建房间: {room_alias} ===")
    
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
            if error.get('errcode') == 'M_ROOM_IN_USE':
                print(f"⚠ 房间 {room_alias} 已存在")
                # 获取现有房间ID
                alias_response = requests.get(
                    f"{BASE_URL}/_matrix/client/r0/directory/room/{room_alias}",
                    headers=headers,
                    timeout=10
                )
                if alias_response.status_code == 200:
                    return alias_response.json().get('room_id')
            else:
                print(f"✗ 创建失败: {error.get('errcode', 'UNKNOWN')}")
                print(f"  {error.get('error', 'Unknown error')}")
            return None
            
    except Exception as e:
        print(f"✗ 错误: {e}")
        return None

def send_message(token, room_id, message):
    """发送消息"""
    print(f"  发送消息: {message[:50]}...")
    
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
        import uuid
        txn_id = str(uuid.uuid4())
        response = requests.put(
            f"{BASE_URL}/_matrix/client/r0/rooms/{room_id}/send/m.room.message/{txn_id}",
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
            print(f"  ✗ 发送失败: {response.status_code}")
            return None
    except Exception as e:
        print(f"  ✗ 错误: {e}")
        return None

def sync_rooms(token):
    """同步房间列表"""
    print(f"\n=== 同步数据 ===")
    
    headers = {
        "Authorization": f"Bearer {token}"
    }
    
    try:
        response = requests.get(
            f"{BASE_URL}/_matrix/client/r0/sync?timeout=1000",
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

def main():
    print("=" * 60)
    print("   创建测试房间并执行API测试")
    print("=" * 60)
    
    # 获取token
    token = get_token()
    if not token:
        print("✗ 无法获取token，请先运行 login_users.py")
        return False
    
    print(f"\n✓ Token获取成功: {token[:30]}...")
    
    rooms_created = []
    
    # 创建 testroom
    room_id = create_room(token, "testroom", "测试房间", is_private=True)
    if room_id:
        rooms_created.append(("testroom", room_id))
    
    # 创建 publicroom
    room_id = create_room(token, "publicroom", "公共房间", is_private=False)
    if room_id:
        rooms_created.append(("publicroom", room_id))
    
    # 发送测试消息
    print("\n" + "=" * 60)
    print("   发送测试消息")
    print("=" * 60)
    
    for alias, room_id in rooms_created:
        if room_id:
            # 发送普通消息
            send_message(token, room_id, f"测试消息 from {alias}!")
            time.sleep(0.5)
    
    # 同步测试
    sync_rooms(token)
    
    print("\n" + "=" * 60)
    print("   测试完成!")
    print("=" * 60)
    
    print("\n创建的房间:")
    for alias, room_id in rooms_created:
        print(f"  - {alias}: {room_id}")
    
    return True

if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)
