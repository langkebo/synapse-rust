#!/usr/bin/env python3
import requests
import json
import random
import string

BASE_URL = "http://localhost:8008"
UNIQUE_ID = ''.join(random.choices(string.ascii_lowercase + string.digits, k=8))

print(f"=== 创建用户 ===")
reg_resp = requests.post(
    f"{BASE_URL}/_matrix/client/r0/register",
    json={"username": f"test_{UNIQUE_ID}", "password": "Password123!"}
)
reg_data = reg_resp.json()
TOKEN = reg_data.get("access_token")
print(f"Token: {TOKEN[:30]}...")

print(f"\n=== 创建房间 ===")
room_resp = requests.post(
    f"{BASE_URL}/_matrix/client/r0/createRoom",
    headers={"Authorization": f"Bearer {TOKEN}"},
    json={"name": "Test", "visibility": "private"}
)
room_data = room_resp.json()
ROOM_ID = room_data.get("room_id")
print(f"Room: {ROOM_ID}")

print(f"\n=== 发送消息 ===")
msg_resp = requests.put(
    f"{BASE_URL}/_matrix/client/r0/rooms/{ROOM_ID}/send/m.room.message/txn_{UNIQUE_ID}",
    headers={"Authorization": f"Bearer {TOKEN}"},
    json={"msgtype": "m.text", "body": "hello"}
)
msg_data = msg_resp.json()
EVENT_ID = msg_data.get("event_id")
print(f"Event ID: {EVENT_ID}")

print(f"\n=== 测试 3.1.4-33: PUT /directory/room/{{room_alias}} ===")
alias_resp = requests.put(
    f"{BASE_URL}/_matrix/client/r0/directory/room/%23alias_{UNIQUE_ID}%3Acjystx.top",
    headers={"Authorization": f"Bearer {TOKEN}"},
    json={"room_id": ROOM_ID}
)
print(f"Status: {alias_resp.status_code}")
print(f"Response: {alias_resp.text}")

print(f"\n=== 测试 3.1.4-32: DELETE /directory/room/{{room_id}} ===")
delete_resp = requests.delete(
    f"{BASE_URL}/_matrix/client/r0/directory/room/{ROOM_ID}",
    headers={"Authorization": f"Bearer {TOKEN}"}
)
print(f"Status: {delete_resp.status_code}")
print(f"Response: {delete_resp.text}")

print(f"\n=== 测试 3.1.7-5: POST /receipt/m.read/{{event_id}} ===")
receipt_resp = requests.post(
    f"{BASE_URL}/_matrix/client/r0/rooms/{ROOM_ID}/receipt/m.read/{EVENT_ID}",
    headers={"Authorization": f"Bearer {TOKEN}"},
    json={}
)
print(f"Status: {receipt_resp.status_code}")
print(f"Response: {receipt_resp.text}")

print(f"\n=== 测试 3.1.7-6: POST /read_markers ===")
read_markers_resp = requests.post(
    f"{BASE_URL}/_matrix/client/r0/rooms/{ROOM_ID}/read_markers",
    headers={"Authorization": f"Bearer {TOKEN}"},
    json={"event_id": EVENT_ID}
)
print(f"Status: {read_markers_resp.status_code}")
print(f"Response: {read_markers_resp.text}")

print(f"\n==========================================")
print(f"             测试结果汇总")
print(f"==========================================")
print(f"API端点                           | 状态码 | 结果")
print(f"-----------------------------------|--------|------")
print(f"3.1.4-33 PUT /directory/room    | {alias_resp.status_code} | {'OK' if alias_resp.status_code == 200 else 'FAIL'}")
print(f"3.1.4-32 DELETE /directory/room  | {delete_resp.status_code} | {'OK' if delete_resp.status_code == 200 else 'FAIL'}")
print(f"3.1.7-5 POST /receipt           | {receipt_resp.status_code} | {'OK' if receipt_resp.status_code == 200 else 'FAIL'}")
print(f"3.1.7-6 POST /read_markers      | {read_markers_resp.status_code} | {'OK' if read_markers_resp.status_code == 200 else 'FAIL'}")
