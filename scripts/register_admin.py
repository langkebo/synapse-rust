#!/usr/bin/env python3
import hmac
import hashlib
import subprocess
import json

SERVER_NAME = "cjystx.top"
SERVER_URL = "http://localhost:15808"
SHARED_SECRET = "test_admin_secret_key_for_dev_only"

print("=== Step 1: Get nonce ===")
result = subprocess.run([
    'curl', '-s', f'{SERVER_URL}/_synapse/admin/v1/register/nonce',
    '-H', f'Host: {SERVER_NAME}'
], capture_output=True, text=True)
nonce = json.loads(result.stdout)['nonce']
print(f"Nonce: {nonce}")

print("\n=== Step 2: Calculate HMAC ===")
username = "admin11"
password = "Wzc9890951!"

# CORRECT: admin\x00\x00\x00 (not just admin)
message = nonce + "\x00" + username + "\x00" + password + "\x00" + "admin\x00\x00\x00"
print(f"Message: {repr(message)}")
print(f"Message hex: {message.encode('utf-8').hex()}")

mac = hmac.new(SHARED_SECRET.encode('utf-8'), message.encode('utf-8'), hashlib.sha256)
mac_hex = mac.hexdigest()
print(f"HMAC: {mac_hex}")

print("\n=== Step 3: Register admin user ===")

register_data = {
    "nonce": nonce,
    "username": username,
    "password": password,
    "admin": True,
    "mac": mac_hex
}

curl_cmd = [
    'curl', '-s', '-X', 'POST',
    f'{SERVER_URL}/_synapse/admin/v1/register',
    '-H', f'Host: {SERVER_NAME}',
    '-H', 'Content-Type: application/json',
    '-d', json.dumps(register_data)
]

result = subprocess.run(curl_cmd, capture_output=True, text=True)
print(f"Response: {result.stdout}")

try:
    resp_data = json.loads(result.stdout)
    if 'access_token' in resp_data:
        print("\n=== SUCCESS! ===")
        print(f"User ID: {resp_data.get('user_id')}")
        print(f"Token: {resp_data.get('access_token')}")
    else:
        print("\n=== FAILED ===")
        print(f"Error: {resp_data}")
except json.JSONDecodeError:
    print(f"\n=== FAILED (not JSON) ===")
    print(f"Response: {result.stdout}")