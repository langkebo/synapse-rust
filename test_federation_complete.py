#!/usr/bin/env python3
"""
测试联邦API端点 - 完整测试
"""

import requests
import json

BASE_URL = "http://localhost:8008"

print("="*80)
print("联邦API完整测试")
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

# 测试4: 查询目的地
print("测试4: 查询目的地 (GET /_matrix/federation/v1/query/destination)")
response = requests.get(f"{BASE_URL}/_matrix/federation/v1/query/destination")
print(f"状态码: {response.status_code}")
print(f"响应: {response.text}")
print()

# 测试5: 获取房间成员
print("测试5: 获取房间成员 (GET /_matrix/federation/v1/members/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top)")
response = requests.get(f"{BASE_URL}/_matrix/federation/v1/members/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top")
print(f"状态码: {response.status_code}")
print(f"响应: {response.text}")
print()

# 测试6: 获取已加入的房间成员
print("测试6: 获取已加入的房间成员 (GET /_matrix/federation/v1/members/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top/joined)")
response = requests.get(f"{BASE_URL}/_matrix/federation/v1/members/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top/joined")
print(f"状态码: {response.status_code}")
print(f"响应: {response.text}")
print()

# 测试7: 获取用户设备
print("测试7: 获取用户设备 (GET /_matrix/federation/v1/user/devices/@admin:cjystx.top)")
response = requests.get(f"{BASE_URL}/_matrix/federation/v1/user/devices/@admin:cjystx.top")
print(f"状态码: {response.status_code}")
print(f"响应: {response.text}")
print()

# 测试8: 获取房间授权
print("测试8: 获取房间授权 (GET /_matrix/federation/v1/room_auth/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top)")
response = requests.get(f"{BASE_URL}/_matrix/federation/v1/room_auth/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top")
print(f"状态码: {response.status_code}")
print(f"响应: {response.text}")
print()

# 测试9: 敲门
print("测试9: 敲门 (GET /_matrix/federation/v1/knock/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top/@testuser:cjystx.top)")
response = requests.get(f"{BASE_URL}/_matrix/federation/v1/knock/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top/@testuser:cjystx.top")
print(f"状态码: {response.status_code}")
print(f"响应: {response.text}")
print()

# 测试10: 获取加入规则
print("测试10: 获取加入规则 (GET /_matrix/federation/v1/get_joining_rules/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top)")
response = requests.get(f"{BASE_URL}/_matrix/federation/v1/get_joining_rules/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top")
print(f"状态码: {response.status_code}")
print(f"响应: {response.text}")
print()

# 测试11: make_join
print("测试11: make_join (GET /_matrix/federation/v1/make_join/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top/@testuser:cjystx.top)")
response = requests.get(f"{BASE_URL}/_matrix/federation/v1/make_join/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top/@testuser:cjystx.top")
print(f"状态码: {response.status_code}")
print(f"响应: {response.text}")
print()

# 测试12: 获取事件
print("测试12: 获取事件 (GET /_matrix/federation/v1/event/$test_event)")
response = requests.get(f"{BASE_URL}/_matrix/federation/v1/event/$test_event")
print(f"状态码: {response.status_code}")
print(f"响应: {response.text}")
print()

# 测试13: 获取房间状态
print("测试13: 获取房间状态 (GET /_matrix/federation/v1/state/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top)")
response = requests.get(f"{BASE_URL}/_matrix/federation/v1/state/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top")
print(f"状态码: {response.status_code}")
print(f"响应: {response.text}")
print()

# 测试14: 获取状态ID
print("测试14: 获取状态ID (GET /_matrix/federation/v1/state_ids/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top)")
response = requests.get(f"{BASE_URL}/_matrix/federation/v1/state_ids/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top")
print(f"状态码: {response.status_code}")
print(f"响应: {response.text}")
print()

# 测试15: 房间目录查询
print("测试15: 房间目录查询 (GET /_matrix/federation/v1/query/directory/room/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top)")
response = requests.get(f"{BASE_URL}/_matrix/federation/v1/query/directory/room/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top")
print(f"状态码: {response.status_code}")
print(f"响应: {response.text}")
print()

# 测试16: 用户资料查询
print("测试16: 用户资料查询 (GET /_matrix/federation/v1/query/profile/@admin:cjystx.top)")
response = requests.get(f"{BASE_URL}/_matrix/federation/v1/query/profile/@admin:cjystx.top")
print(f"状态码: {response.status_code}")
print(f"响应: {response.text}")
print()

# 测试17: 获取事件授权
print("测试17: 获取事件授权 (GET /_matrix/federation/v1/get_event_auth/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top/$test_event)")
response = requests.get(f"{BASE_URL}/_matrix/federation/v1/get_event_auth/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top/$test_event")
print(f"状态码: {response.status_code}")
print(f"响应: {response.text}")
print()

# 测试18: 获取房间事件
print("测试18: 获取房间事件 (GET /_matrix/federation/v1/room/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top/$test_event)")
response = requests.get(f"{BASE_URL}/_matrix/federation/v1/room/!ZvtusAjkfrGp5ZEBbL9EfptT:cjystx.top/$test_event")
print(f"状态码: {response.status_code}")
print(f"响应: {response.text}")
print()

print("="*80)
print("测试完成")
print("="*80)
