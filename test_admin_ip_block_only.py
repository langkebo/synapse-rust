#!/usr/bin/env python3
"""
测试管理员API的IP封禁功能
"""

import requests
import json
import time

BASE_URL = "http://localhost:8008"

# 第一步：登录获取token
print("=== 第一步：登录获取token ===")
login_response = requests.post(
    f"{BASE_URL}/_matrix/client/r0/login",
    json={
        "type": "m.login.password",
        "user": "admin",
        "password": "Wzc9890951!"
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

# 测试5: 使用ip_address字段封禁IP
print("=== 测试5: 使用ip_address字段封禁IP ===")
block_response2 = requests.post(
    f"{BASE_URL}/_synapse/admin/v1/security/ip/block",
    headers=headers,
    json={
        "ip_address": "10.0.0.1",
        "reason": "测试封禁2"
    }
)
print(f"状态码: {block_response2.status_code}")
print(f"响应: {block_response2.text}")
print()

# 测试6: 获取IP封禁列表
print("=== 测试6: 获取IP封禁列表 ===")
list_response3 = requests.get(
    f"{BASE_URL}/_synapse/admin/v1/security/ip/blocks",
    headers=headers
)
print(f"状态码: {list_response3.status_code}")
print(f"响应: {list_response3.text}")
print()

# 测试7: 使用ip_address字段解封IP
print("=== 测试7: 使用ip_address字段解封IP ===")
unblock_response2 = requests.post(
    f"{BASE_URL}/_synapse/admin/v1/security/ip/unblock",
    headers=headers,
    json={
        "ip_address": "10.0.0.1"
    }
)
print(f"状态码: {unblock_response2.status_code}")
print(f"响应: {unblock_response2.text}")
print()

print("==========================================")
print("测试完成")
print("==========================================")
