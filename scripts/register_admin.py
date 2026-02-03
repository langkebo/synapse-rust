#!/usr/bin/env python3
"""
Synapse Rust 管理员注册脚本
"""

import hmac
import hashlib
import requests
import json

BASE_URL = "http://localhost:8008"
SHARED_SECRET = "test_shared_secret"
USERNAME = "admin"
PASSWORD = "Wzc9890951!"
ADMIN = True
DISPLAYNAME = "System Administrator"

def calculate_hmac(nonce, username, password, admin, user_type=None):
    """
    计算HMAC-SHA256
    格式: HMAC-SHA256(shared_secret, nonce + "\0" + username + "\0" + password + "\0" + "admin"/"notadmin" + ("\0" + user_type if user_type else ""))
    """
    key = SHARED_SECRET.encode('utf-8')
    
    # 构建消息
    message = nonce.encode('utf-8')
    message += b'\x00'
    message += username.encode('utf-8')
    message += b'\x00'
    message += password.encode('utf-8')
    message += b'\x00'
    message += b'admin' if admin else b'notadmin'
    
    # 只有当user_type存在时才添加
    if user_type:
        message += b'\x00'
        message += user_type.encode('utf-8')
    
    mac = hmac.new(key, message, hashlib.sha256)
    return mac.hexdigest()

def register_admin():
    print("=== Synapse Rust 管理员注册 ===\n")
    
    # 步骤1: 获取nonce
    print("步骤1: 获取nonce...")
    nonce_response = requests.get(f"{BASE_URL}/_synapse/admin/v1/register/nonce")
    
    if nonce_response.status_code != 200:
        print(f"获取nonce失败: {nonce_response.status_code}")
        print(nonce_response.text)
        return False
    
    nonce_data = nonce_response.json()
    nonce = nonce_data['nonce']
    print(f"Nonce: {nonce}\n")
    
    # 步骤2: 计算HMAC
    print("步骤2: 计算HMAC-SHA256...")
    mac = calculate_hmac(nonce, USERNAME, PASSWORD, ADMIN)
    print(f"MAC: {mac}\n")
    
    # 步骤3: 注册管理员
    print("步骤3: 注册管理员账号...")
    register_data = {
        "nonce": nonce,
        "username": USERNAME,
        "password": PASSWORD,
        "admin": ADMIN,
        "displayname": DISPLAYNAME,
        "mac": mac
    }
    
    register_response = requests.post(
        f"{BASE_URL}/_synapse/admin/v1/register",
        json=register_data,
        headers={"Content-Type": "application/json"}
    )
    
    if register_response.status_code == 200:
        print("✓ 管理员注册成功！\n")
        result = register_response.json()
        print("注册结果:")
        print(json.dumps(result, indent=2))
        return True
    else:
        print(f"✗ 注册失败: {register_response.status_code}")
        print(register_response.text)
        return False

if __name__ == "__main__":
    success = register_admin()
    exit(0 if success else 1)
