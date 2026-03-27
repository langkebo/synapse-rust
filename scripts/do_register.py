#!/usr/bin/env python3
import hmac
import hashlib
import json
import urllib.request
import urllib.error

server_name = "cjystx.top"
server_url = "http://localhost:8008"  # Direct to synapse container
shared_secret = "test_admin_secret_key_for_dev_only"

# Get nonce
req = urllib.request.Request(
    f"{server_url}/_synapse/admin/v1/register/nonce",
    headers={"Host": server_name}
)
with urllib.request.urlopen(req) as response:
    nonce = json.loads(response.read())["nonce"]

print(f"Nonce: {nonce}")

# Calculate HMAC
username = "admin6"
password = "Wzc9890951!"
admin = True

message = nonce.encode('utf-8')
message += b'\x00'
message += username.encode('utf-8')
message += b'\x00'
message += password.encode('utf-8')
message += b'\x00'

if admin:
    message += b"admin"
else:
    message += b"notadmin"

print(f"Message: {message}")

mac = hmac.new(shared_secret.encode('utf-8'), message, hashlib.sha256)
mac_hex = mac.hexdigest()

print(f"HMAC: {mac_hex}")

# Register
register_data = {
    "nonce": nonce,
    "username": username,
    "password": password,
    "admin": admin,
    "mac": mac_hex,
    "displayname": "System Administrator"
}

req = urllib.request.Request(
    f"{server_url}/_synapse/admin/v1/register",
    data=json.dumps(register_data).encode('utf-8'),
    headers={
        "Host": server_name,
        "Content-Type": "application/json"
    },
    method='POST'
)

try:
    with urllib.request.urlopen(req) as response:
        result = json.loads(response.read())
        print(f"\nSuccess!")
        print(f"User ID: {result.get('user_id')}")
        print(f"Token: {result.get('access_token')}")
except urllib.error.HTTPError as e:
    print(f"\nHTTP Error {e.code}:")
    try:
        error_body = e.read().decode('utf-8')
        print(f"Body: {error_body}")
    except:
        pass
except Exception as e:
    print(f"\nError: {e}")