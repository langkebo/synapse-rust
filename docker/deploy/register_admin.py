#!/usr/bin/env python3
"""
管理员注册脚本

根据 /Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/admin-registration-guide.md 实现
HMAC-SHA256 消息格式使用字节拼接
"""

import hmac
import hashlib
import json
import urllib.request
import urllib.error
import sys
import os

SERVER = os.environ.get("SYNAPSE_SERVER", "http://localhost:8008")
USERNAME = os.environ.get("ADMIN_USERNAME", "admin")
PASSWORD = os.environ.get("ADMIN_PASSWORD", "")
DISPLAYNAME = "System Administrator"

SHARED_SECRET = os.environ.get("ADMIN_SHARED_SECRET", "")

if not PASSWORD:
    print("ERROR: ADMIN_PASSWORD environment variable must be set", file=sys.stderr)
    sys.exit(1)
if not SHARED_SECRET:
    print("ERROR: ADMIN_SHARED_SECRET environment variable must be set", file=sys.stderr)
    sys.exit(1)


def get_nonce():
    """获取 nonce"""
    with urllib.request.urlopen(f"{SERVER}/_synapse/admin/v1/register/nonce") as resp:
        return json.loads(resp.read())["nonce"]


def calculate_mac(secret: str, nonce: str, username: str, password: str, admin: bool) -> str:
    """
    计算 HMAC-SHA256

    格式: nonce\0username\0password\0admin/notadmin
    注意: 使用字节拼接，不是字符串拼接
    """
    message = bytearray()

    # nonce
    message.extend(nonce.encode('utf-8'))
    message.extend(b'\x00')

    # username
    message.extend(username.encode('utf-8'))
    message.extend(b'\x00')

    # password
    message.extend(password.encode('utf-8'))
    message.extend(b'\x00')

    # admin or notadmin
    message.extend(b'admin\x00\x00\x00' if admin else b'notadmin')

    # 计算 HMAC
    key = secret.encode('utf-8')
    mac = hmac.new(key, bytes(message), hashlib.sha256)
    return mac.hexdigest()


def register_admin():
    """注册管理员"""
    try:
        # 1. 获取 nonce
        print(f"1. 获取 nonce from {SERVER}/_synapse/admin/v1/register/nonce")
        nonce = get_nonce()
        print(f"   Nonce: {nonce[:40]}...")

        # 2. 计算 HMAC
        print(f"2. 计算 HMAC-SHA256")
        mac = calculate_mac(SHARED_SECRET, nonce, USERNAME, PASSWORD, admin=True)
        print(f"   MAC: {mac}")

        # 3. 注册
        print(f"3. 注册管理员账号")
        data = {
            "nonce": nonce,
            "username": USERNAME,
            "password": PASSWORD,
            "admin": True,
            "displayname": DISPLAYNAME,
            "mac": mac
        }

        req = urllib.request.Request(
            f"{SERVER}/_synapse/admin/v1/register",
            data=json.dumps(data).encode('utf-8'),
            headers={"Content-Type": "application/json"}
        )

        with urllib.request.urlopen(req) as resp:
            result = json.loads(resp.read())
            print(f"\n✅ 注册成功!")
            print(f"   User ID: {result.get('user_id')}")
            print(f"   Access Token: {result.get('access_token', 'N/A')[:80]}...")
            print(f"\n保存以下信息用于测试:")
            print(f"   ADMIN_TOKEN='{result.get('access_token')}'")

            # 保存到文件
            with open("/tmp/admin_token.txt", "w") as f:
                f.write(result.get('access_token', ''))
            print(f"\n   Token 已保存到 /tmp/admin_token.txt")

            return result

    except urllib.error.HTTPError as e:
        error_body = e.read().decode()
        print(f"\n❌ HTTP Error {e.code}: {error_body}")
        return None
    except Exception as e:
        print(f"\n❌ Error: {type(e).__name__}: {e}")
        return None


def test_admin_login():
    """测试管理员登录"""
    print(f"\n4. 测试管理员登录")
    try:
        data = {
            "type": "m.login.password",
            "user": USERNAME,
            "password": PASSWORD,
            "initial_device_display_name": "AdminTest"
        }

        req = urllib.request.Request(
            f"{SERVER}/_matrix/client/v3/login",
            data=json.dumps(data).encode('utf-8'),
            headers={"Content-Type": "application/json"}
        )

        with urllib.request.urlopen(req) as resp:
            result = json.loads(resp.read())
            print(f"   ✅ 登录成功!")
            print(f"   User ID: {result.get('user_id')}")
            print(f"   Access Token: {result.get('access_token', 'N/A')[:60]}...")
            return result

    except urllib.error.HTTPError as e:
        error_body = e.read().decode()
        print(f"   ❌ 登录失败: {error_body}")
        return None


if __name__ == "__main__":
    print("=" * 60)
    print("管理员注册脚本")
    print("=" * 60)
    print(f"Server: {SERVER}")
    print(f"Username: {USERNAME}")
    print(f"Password: {PASSWORD}")
    print("=" * 60)

    result = register_admin()

    if result:
        test_admin_login()
    else:
        print("\n注册失败，请检查:")
        print("1. 服务器是否运行")
        print("2. shared_secret 是否正确")
        print("3. HMAC 计算格式是否正确")
        sys.exit(1)
