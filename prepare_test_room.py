#!/usr/bin/env python3
"""
准备测试数据：创建测试房间
"""

import requests
import json

BASE_URL = "http://localhost:8008"

# 登录管理员账户
print("=== 登录管理员账户 ===")
login_response = requests.post(
    f"{BASE_URL}/_matrix/client/r0/login",
    json={
        "type": "m.login.password",
        "user": "admin",
        "password": "Wzc9890951!"
    }
)

if login_response.status_code != 200:
    print(f"登录失败: {login_response.status_code}")
    print(login_response.text)
    exit(1)

token = login_response.json()["access_token"]
print(f"Token: {token[:50]}...")
print()

headers = {
    "Authorization": f"Bearer {token}",
    "Content-Type": "application/json"
}

# 创建测试房间
print("=== 创建测试房间 ===")
room_response = requests.post(
    f"{BASE_URL}/_matrix/client/r0/createRoom",
    headers=headers,
    json={
        "name": "Test Room 1",
        "preset": "public_chat"
    }
)

print(f"状态码: {room_response.status_code}")
print(f"响应: {room_response.text}")

if room_response.status_code == 200:
    room_id = room_response.json()["room_id"]
    print(f"房间ID: {room_id}")
    print()
    print("请更新 scripts/test_admin_api.py 中的 TEST_ROOMS:")
    print(f'  "room1": {{')
    print(f'    "room_id": "{room_id}",')
    print(f'    "name": "Test Room 1"')
    print(f'  }}')
else:
    print("创建房间失败")
