#!/usr/bin/env python3
import hmac
import hashlib
import json
import urllib.request
import urllib.parse

shared_secret = "test_admin_secret_key_for_dev_only"
server_url = "http://localhost:8008"

req = urllib.request.Request(
    f"{server_url}/_synapse/admin/v1/register/nonce",
    headers={"Host": "matrix.cjystx.top"}
)
with urllib.request.urlopen(req) as response:
    nonce_data = json.loads(response.read())
    nonce = nonce_data["nonce"]

print(f"Got nonce: {nonce[:30]}...")

username = "admin_cjystx"
password = "Wzc9890951!"
admin = True

message = nonce.encode('utf-8') + b'\x00'
message += username.encode('utf-8') + b'\x00'
message += password.encode('utf-8') + b'\x00'
message += b'admin' if admin else b'notadmin'

key = shared_secret.encode('utf-8')
mac = hmac.new(key, message, hashlib.sha256)
mac_hex = mac.hexdigest()

print(f"HMAC: {mac_hex}")

register_data = {
    "nonce": nonce,
    "username": username,
    "password": password,
    "admin": admin,
    "displayname": "Admin CJYSTX",
    "mac": mac_hex
}

req = urllib.request.Request(
    f"{server_url}/_synapse/admin/v1/register",
    data=json.dumps(register_data).encode('utf-8'),
    headers={
        "Host": "matrix.cjystx.top",
        "Content-Type": "application/json"
    },
    method='POST'
)

try:
    with urllib.request.urlopen(req) as response:
        result = json.loads(response.read())
        print(f"\nRegistration successful!")
        print(f"User ID: {result.get('user_id', 'N/A')}")
        token = result.get('access_token', '')
        print(f"Access Token: {token[:50]}...")
        print(f"\n保存这个Token用于测试:")
        print(f"export ADMIN_ACCESS_TOKEN=\"{token}\"")
except urllib.error.HTTPError as e:
    error = json.loads(e.read())
    print(f"\nRegistration failed: {error}")
