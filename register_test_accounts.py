import hmac
import hashlib
import json
import urllib.request
import os
import sys

SERVER = "http://localhost:28008"
SHARED_SECRET = "15c7a2cfdeb364a8d6d0dd8fa45ef45209021023c4b7937f6c3887c0788b155d"

def get_nonce():
    with urllib.request.urlopen(f"{SERVER}/_synapse/admin/v1/register/nonce") as resp:
        return json.loads(resp.read())["nonce"]

def calculate_mac(secret, nonce, username, password, admin):
    message = bytearray()
    message.extend(nonce.encode('utf-8'))
    message.extend(b'\x00')
    message.extend(username.encode('utf-8'))
    message.extend(b'\x00')
    message.extend(password.encode('utf-8'))
    message.extend(b'\x00')
    message.extend(b'admin\x00\x00\x00' if admin else b'notadmin')
    key = secret.encode('utf-8')
    mac = hmac.new(key, bytes(message), hashlib.sha256)
    return mac.hexdigest()

def register(username, password, admin):
    nonce = get_nonce()
    mac = calculate_mac(SHARED_SECRET, nonce, username, password, admin)
    data = {
        "nonce": nonce,
        "username": username,
        "password": password,
        "admin": admin,
        "displayname": username,
        "mac": mac
    }
    req = urllib.request.Request(f"{SERVER}/_synapse/admin/v1/register", data=json.dumps(data).encode('utf-8'), headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req) as resp:
            print(f"Registered {username}: {resp.read().decode()}")
    except Exception as e:
        print(f"Failed to register {username}: {e}")

if __name__ == "__main__":
    # Super Admin
    register("superadmin", "Admin@123", True)
    # Admin
    register("admin_role", "Admin@123", True)
    # Regular User
    register("regularuser", "User@123", False)
