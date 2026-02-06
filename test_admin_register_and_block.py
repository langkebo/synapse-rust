#!/usr/bin/env python3
"""
注册管理员账户并测试IP封禁功能
"""

import requests
import json
import hmac
import hashlib
import base64
import time

BASE_URL = "http://localhost:8008"
SHARED_SECRET = "test_shared_secret"  # 需要与配置一致

# 第一步：获取nonce
print("=== 第一步：获取nonce ===")
nonce_response = requests.get(f"{BASE_URL}/_synapse/admin/v1/register/nonce")
print(f"状态码: {nonce_response.status_code}")
print(f"响应: {nonce_response.text}")

if nonce_response.status_code != 200:
    print("获取nonce失败")
    exit(1)

nonce = nonce_response.json()["nonce"]
print(f"Nonce: {nonce}")
print()

# 第二步：使用HMAC签名注册管理员
print("=== 第二步：注册管理员账户 ===")

username = "admin"
password = "Wzc9890951!"
admin = True

# 生成HMAC签名
# 格式: nonce\x00username\x00password\x00admin
admin_str = "admin" if admin else "notadmin"
mac = hmac.new(
    SHARED_SECRET.encode(),
    f"{nonce}\x00{username}\x00{password}\x00{admin_str}".encode(),
    hashlib.sha256
)
mac_hex = mac.hexdigest()

register_response = requests.post(
    f"{BASE_URL}/_synapse/admin/v1/register",
    json={
        "nonce": nonce,
        "username": username,
        "password": password,
        "admin": admin,
        "mac": mac_hex
    }
)

print(f"状态码: {register_response.status_code}")
print(f"响应: {register_response.text}")

if register_response.status_code != 200:
    print("注册管理员失败")
    exit(1)

user_id = register_response.json()["user_id"]
print(f"用户ID: {user_id}")
print()

# 第三步：登录获取token
print("=== 第三步：登录获取token ===")
login_response = requests.post(
    f"{BASE_URL}/_matrix/client/r0/login",
    json={
        "type": "m.login.password",
        "user": username,
        "password": password
    }
)

print(f"状态码: {login_response.status_code}")
print(f"响应: {login_response.text}")

if login_response.status_code != 200:
    print("登录失败")
    exit(1)

token = login_response.json()["access_token"]
print(f"Token: {token[:50]}...")
print()

headers = {
    "Authorization": f"Bearer {token}",
    "Content-Type": "application/json"
}

# 测试1: 封禁IP
print("=== 测试1: 封禁IP ===")
block_response = requests.post(
    f"{BASE_URL}/_synapse/admin/v1/security/ip/block",
    headers=headers,
    json={
        "ip": "192.168.1.100",
        "reason": "测试封禁",
        "expires_at": None
    }
)
print(f"状态码: {block_response.status_code}")
print(f"响应: {block_response.text}")
print()

# 测试2: 获取IP封禁列表
print("=== 测试2: 获取IP封禁列表 ===")
list_response = requests.get(
    f"{BASE_URL}/_synapse/admin/v1/security/ip/blocks",
    headers=headers
)
print(f"状态码: {list_response.status_code}")
print(f"响应: {list_response.text}")
print()

# 测试3: 解封IP
print("=== 测试3: 解封IP ===")
unblock_response = requests.post(
    f"{BASE_URL}/_synapse/admin/v1/security/ip/unblock",
    headers=headers,
    json={
        "ip": "192.168.1.100"
    }
)
print(f"状态码: {unblock_response.status_code}")
print(f"响应: {unblock_response.text}")
print()

# 测试4: 再次获取IP封禁列表
print("=== 测试4: 再次获取IP封禁列表 ===")
list_response2 = requests.get(
    f"{BASE_URL}/_synapse/admin/v1/security/ip/blocks",
    headers=headers
)
print(f"状态码: {list_response2.status_code}")
print(f"响应: {list_response2.text}")
print()

print("==========================================")
print("测试完成")
print("==========================================")
