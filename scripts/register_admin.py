#!/usr/bin/env python3
import hmac
import hashlib
import json
import urllib.request

# 配置
SERVER = "http://localhost:28008"
USERNAME = "admin"
PASSWORD = "Admin@123"
SHARED_SECRET = "change-me-admin-registration-secret"
ADMIN = True

def main():
    # 获取 nonce
    with urllib.request.urlopen(f"{SERVER}/_synapse/admin/v1/register/nonce") as resp:
        nonce_data = json.loads(resp.read())
        nonce = nonce_data["nonce"]

    print(f"Got nonce: {nonce}")

    # 构建 message
    # 格式: nonce\x00username\x00password\x00admin\x00\x00\x00 (for admin with no user_type)
    message = f"{nonce}\x00{USERNAME}\x00{PASSWORD}\x00"
    if ADMIN:
        message += "admin\x00\x00\x00"
    else:
        message += "notadmin"

    print(f"Message: {repr(message)}")

    # 计算 MAC
    mac = hmac.new(SHARED_SECRET.encode(), message.encode(), hashlib.sha256).hexdigest()
    print(f"MAC: {mac}")

    # 注册
    data = json.dumps({
        "nonce": nonce,
        "username": USERNAME,
        "password": PASSWORD,
        "admin": ADMIN,
        "mac": mac
    }).encode()

    req = urllib.request.Request(
        f"{SERVER}/_synapse/admin/v1/register",
        data=data,
        headers={"Content-Type": "application/json"}
    )

    with urllib.request.urlopen(req) as resp:
        result = json.loads(resp.read())
        print(f"Success: {result}")

if __name__ == "__main__":
    main()