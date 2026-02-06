#!/usr/bin/env python3
"""
测试联邦API端点
"""

import requests
import json

BASE_URL = "http://localhost:8008"

print("="*80)
print("联邦API测试")
print("="*80)
print()

# 测试1: 获取联邦版本
print("测试1: 获取联邦版本 (GET /_matrix/federation/v1/version)")
response = requests.get(f"{BASE_URL}/_matrix/federation/v1/version")
print(f"状态码: {response.status_code}")
print(f"响应: {response.text}")
print()

# 测试2: 联邦发现
print("测试2: 联邦发现 (GET /_matrix/federation/v1)")
response = requests.get(f"{BASE_URL}/_matrix/federation/v1")
print(f"状态码: {response.status_code}")
print(f"响应: {response.text}")
print()

# 测试3: 获取公共房间
print("测试3: 获取公共房间 (GET /_matrix/federation/v1/publicRooms)")
response = requests.get(f"{BASE_URL}/_matrix/federation/v1/publicRooms")
print(f"状态码: {response.status_code}")
print(f"响应: {response.text}")
print()

# 测试4: 服务器密钥
print("测试4: 服务器密钥 (GET /_matrix/federation/v2/server)")
response = requests.get(f"{BASE_URL}/_matrix/federation/v2/server")
print(f"状态码: {response.status_code}")
print(f"响应: {response.text}")
print()

# 测试5: 密钥查询
print("测试5: 密钥查询 (GET /_matrix/federation/v2/query/cjystx.top/ed25519:auto)")
response = requests.get(f"{BASE_URL}/_matrix/federation/v2/query/cjystx.top/ed25519:auto")
print(f"状态码: {response.status_code}")
print(f"响应: {response.text}")
print()

print("="*80)
print("测试完成")
print("="*80)
