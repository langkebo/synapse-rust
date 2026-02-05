#!/usr/bin/env python3
"""
完整的Matrix API测试
"""

import requests
import json
import sys

BASE_URL = "http://localhost:8008"

def main():
    # 读取token
    try:
        with open("testuser1_token.txt", 'r') as f:
            token = f.read().strip()
    except Exception as e:
        print(f"无法读取token: {e}")
        return False

    print(f"Token: {token[:50]}...")
    print()

    headers = {
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json"
    }

    # 1. 创建房间
    print("=" * 50)
    print("1. 创建房间")
    print("=" * 50)
    
    room_data = {
        "room_alias_name": "testroom",
        "name": "测试房间",
        "visibility": "private"
    }
    
    resp = requests.post(
        f"{BASE_URL}/_matrix/client/r0/createRoom",
        json=room_data,
        headers=headers,
        timeout=10
    )
    
    if resp.status_code == 200:
        room_id = resp.json().get('room_id')
        print(f"✓ 房间创建成功: {room_id}")
    else:
        error = resp.json()
        if error.get('errcode') == 'M_ROOM_IN_USE':
            print("⚠ 房间已存在")
            # 获取现有房间ID
            alias_resp = requests.get(
                f"{BASE_URL}/_matrix/client/r0/directory/room/testroom",
                headers=headers,
                timeout=10
            )
            if alias_resp.status_code == 200:
                room_id = alias_resp.json().get('room_id')
                print(f"  使用现有房间: {room_id}")
            else:
                print(f"  ✗ 无法获取房间信息")
                return False
        else:
            print(f"✗ 创建失败: {error}")
            return False
    
    print()

    # 2. 发送消息
    print("=" * 50)
    print("2. 发送消息")
    print("=" * 50)
    
    message_data = {
        "msgtype": "m.room.message",
        "body": "Hello from testuser1! 这是一条测试消息",
        "format": "org.matrix.custom.html",
        "formatted_body": "<b>Hello from testuser1!</b><br>这是一条测试消息"
    }
    
    resp = requests.post(
        f"{BASE_URL}/_matrix/client/r0/rooms/{room_id}/send/m.room.message",
        json=message_data,
        headers=headers,
        timeout=10
    )
    
    if resp.status_code == 200:
        event_id = resp.json().get('event_id')
        print(f"✓ 消息发送成功! Event ID: {event_id}")
    else:
        print(f"✗ 发送失败 ({resp.status_code}): {resp.text[:200]}")
        return False
    
    print()

    # 3. 获取消息
    print("=" * 50)
    print("3. 获取消息历史")
    print("=" * 50)
    
    resp = requests.get(
        f"{BASE_URL}/_matrix/client/r0/rooms/{room_id}/messages?direction=b&limit=10",
        headers=headers,
        timeout=10
    )
    
    if resp.status_code == 200:
        data = resp.json()
        chunk = data.get('chunk', [])
        print(f"✓ 获取到 {len(chunk)} 条消息")
        for msg in chunk:
            body = msg.get('content', {}).get('body', '')
            print(f"  - {body[:60]}...")
    else:
        print(f"✗ 获取失败 ({resp.status_code}): {resp.text[:200]}")
    
    print()

    # 4. 同步
    print("=" * 50)
    print("4. 同步数据")
    print("=" * 50)
    
    resp = requests.get(
        f"{BASE_URL}/_matrix/client/r0/sync?timeout=3000",
        headers=headers,
        timeout=15
    )
    
    if resp.status_code == 200:
        data = resp.json()
        rooms = data.get('rooms', {}).get('join', {})
        print(f"✓ 同步成功! 加入的房间数: {len(rooms)}")
        for room_id_str, room_data in rooms.items():
            name = room_data.get('room_state', {}).get('m.room.name', {}).get('content', {}).get('name', 'Unknown')
            print(f"  - {room_id_str}: {name}")
    else:
        print(f"✗ 同步失败 ({resp.status_code})")
    
    print()
    print("=" * 50)
    print("测试完成!")
    print("=" * 50)
    
    return True

if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)
