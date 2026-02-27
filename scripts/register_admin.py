#!/usr/bin/env python3
"""
管理员账号注册脚本
使用 HMAC-SHA256 签名验证机制注册管理员账号
"""

import hmac
import hashlib
import requests
import sys
import os

SERVER_URL = os.environ.get("SERVER_URL", "http://localhost:8008")
SHARED_SECRET = os.environ.get("ADMIN_SECRET", "test_shared_secret")

def get_nonce():
    """获取 nonce"""
    url = f"{SERVER_URL}/_synapse/admin/v1/register/nonce"
    try:
        response = requests.get(url, timeout=10)
        response.raise_for_status()
        return response.json().get("nonce")
    except requests.exceptions.RequestException as e:
        print(f"获取 nonce 失败: {e}")
        if hasattr(e, 'response') and e.response is not None:
            print(f"响应内容: {e.response.text}")
        return None

def calculate_mac(nonce, username, password, admin=True, user_type=None):
    """计算 HMAC-SHA256"""
    message = nonce.encode('utf-8')
    message += b'\x00'
    message += username.encode('utf-8')
    message += b'\x00'
    message += password.encode('utf-8')
    message += b'\x00'
    message += b'admin' if admin else b'notadmin'
    
    if user_type:
        message += b'\x00'
        message += user_type.encode('utf-8')
    
    key = SHARED_SECRET.encode('utf-8')
    mac = hmac.new(key, message, hashlib.sha256)
    return mac.hexdigest()

def register_admin(username, password, displayname=None, admin=True):
    """注册管理员账号"""
    nonce = get_nonce()
    if not nonce:
        return None
    
    print(f"获取到 nonce: {nonce[:20]}...")
    
    mac = calculate_mac(nonce, username, password, admin)
    
    url = f"{SERVER_URL}/_synapse/admin/v1/register"
    data = {
        "nonce": nonce,
        "username": username,
        "password": password,
        "admin": admin,
        "mac": mac
    }
    
    if displayname:
        data["displayname"] = displayname
    
    try:
        response = requests.post(url, json=data, timeout=10)
        response.raise_for_status()
        return response.json()
    except requests.exceptions.RequestException as e:
        print(f"注册失败: {e}")
        if hasattr(e, 'response') and e.response is not None:
            print(f"响应内容: {e.response.text}")
        return None

def main():
    if len(sys.argv) < 3:
        print("用法: python register_admin.py <username> <password> [displayname]")
        print("示例: python register_admin.py admin Admin@123456 'System Administrator'")
        sys.exit(1)
    
    username = sys.argv[1]
    password = sys.argv[2]
    displayname = sys.argv[3] if len(sys.argv) > 3 else None
    
    print(f"正在注册管理员账号: {username}")
    print(f"服务器: {SERVER_URL}")
    print(f"共享密钥: {SHARED_SECRET[:10]}...")
    print()
    
    result = register_admin(username, password, displayname, admin=True)
    
    if result:
        print("注册成功!")
        print(f"用户 ID: {result.get('user_id')}")
        print(f"设备 ID: {result.get('device_id')}")
        print(f"Access Token: {result.get('access_token', '')[:50]}...")
        print(f"过期时间: {result.get('expires_in')} 秒")
    else:
        print("注册失败!")
        sys.exit(1)

if __name__ == "__main__":
    main()
