#!/usr/bin/env python3
"""
注册 api-reference.md 中定义的所有测试用户和房间
"""

import hmac
import hashlib
import requests
import json
import sys
import time

BASE_URL = "http://localhost:8008"
SHARED_SECRET = "test_shared_secret"

def calculate_hmac(nonce, username, password, admin):
    key = SHARED_SECRET.encode('utf-8')
    message = f"{nonce}\x00{username}\x00{password}\x00{'admin' if admin else 'notadmin'}"
    mac = hmac.new(key, message.encode('utf-8'), hashlib.sha256)
    return mac.hexdigest()

def register_user(username, password, is_admin=False):
    """使用HMAC注册用户"""
    print(f"  注册用户: {username} (admin: {is_admin})")
    
    try:
        nonce_response = requests.get(f"{BASE_URL}/_synapse/admin/v1/register/nonce", timeout=10)
        
        if nonce_response.status_code != 200:
            print(f"    ✗ 获取nonce失败: {nonce_response.status_code}")
            return False
        
        nonce = nonce_response.json()['nonce']
        mac = calculate_hmac(nonce, username, password, is_admin)
        
        reg_response = requests.post(
            f"{BASE_URL}/_synapse/admin/v1/register",
            json={
                "nonce": nonce,
                "username": username,
                "password": password,
                "admin": is_admin,
                "mac": mac
            },
            headers={"Content-Type": "application/json"},
            timeout=10
        )
        
        if reg_response.status_code == 200:
            result = reg_response.json()
            print(f"    ✓ {username} 注册成功! UserID: {result.get('user_id')}")
            return True
        else:
            error = reg_response.json()
            if error.get('errcode') == "M_USER_IN_USE":
                print(f"    ⚠ {username} 已存在")
                return True
            else:
                print(f"    ✗ 注册失败: {error.get('errcode')}")
                return False
                
    except Exception as e:
        print(f"    ✗ 错误: {e}")
        return False

def login_user(username, password):
    """登录获取token"""
    print(f"  登录用户: {username}")
    
    try:
        response = requests.post(
            f"{BASE_URL}/_matrix/client/r0/login",
            json={
                "type": "m.login.password",
                "user": username,
                "password": password
            },
            headers={"Content-Type": "application/json"},
            timeout=10
        )
        
        if response.status_code == 200:
            result = response.json()
            token = result.get('access_token', '')
            user_id = result.get('user_id', '')
            print(f"    ✓ 登录成功! UserID: {user_id}")
            
            with open(f"{username}_token.txt", 'w') as f:
                f.write(token)
            with open(f"{username}_userid.txt", 'w') as f:
                f.write(user_id)
            return True
        else:
            print(f"    ✗ 登录失败")
            return False
    except Exception as e:
        print(f"    ✗ 错误: {e}")
        return False

def create_room(token, room_alias, name, is_private=True):
    """创建房间"""
    print(f"  创建房间: {room_alias} ({name})")
    
    headers = {
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json"
    }
    
    data = {
        "room_alias_name": room_alias,
        "name": name,
        "visibility": "private" if is_private else "public"
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
            room_id = result.get('room_id')
            print(f"    ✓ 房间创建成功! Room ID: {room_id}")
            return room_id
        else:
            error = response.json()
            if error.get('errcode') == 'M_ROOM_IN_USE':
                print(f"    ⚠ 房间已存在")
                alias_resp = requests.get(
                    f"{BASE_URL}/_matrix/client/r0/directory/room/{room_alias}",
                    headers=headers,
                    timeout=10
                )
                if alias_resp.status_code == 200:
                    return alias_resp.json().get('room_id')
            else:
                print(f"    ✗ 创建失败: {error.get('errcode')}")
            return None
    except Exception as e:
        print(f"    ✗ 错误: {e}")
        return None

def main():
    print("=" * 60)
    print("   注册 api-reference.md 中的测试用户和房间")
    print("=" * 60)
    
    results = {
        "users": {},
        "rooms": {}
    }
    
    # 1. 注册管理员
    print("\n[1/3] 注册管理员账号")
    print("-" * 40)
    if register_user("admin", "Wzc9890951!", True):
        results["users"]["admin"] = True
        time.sleep(1)
    
    # 2. 注册普通用户
    print("\n[2/3] 注册测试用户")
    print("-" * 40)
    
    for i in range(1, 7):
        username = f"testuser{i}"
        if register_user(username, "TestPass123!"):
            results["users"][username] = True
        
        time.sleep(2)  # 避免rate limit
    
    # 3. 登录并创建房间
    print("\n[3/3] 登录并创建测试房间")
    print("-" * 40)
    
    # 登录testuser1获取token
    if login_user("testuser1", "TestPass123!"):
        with open("testuser1_token.txt", 'r') as f:
            token = f.read().strip()
        
        # 创建测试房间
        room_id = create_room(token, "testroom", "测试房间", True)
        if room_id:
            results["rooms"]["测试房间"] = room_id
        time.sleep(1)
        
        # 创建公共房间
        room_id = create_room(token, "publicroom", "公共房间", False)
        if room_id:
            results["rooms"]["公共房间"] = room_id
    
    # 总结
    print("\n" + "=" * 60)
    print("   注册完成!")
    print("=" * 60)
    
    print("\n测试用户:")
    for user, success in results["users"].items():
        status = "✓" if success else "✗"
        print(f"  {status} {user}")
    
    print("\n测试房间:")
    for room, room_id in results["rooms"].items():
        print(f"  ✓ {room}: {room_id}")
    
    return True

if __name__ == "__main__":
    main()
