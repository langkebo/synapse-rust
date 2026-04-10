#!/usr/bin/env python3
"""
管理员账号注册脚本
用于在 Synapse Rust 服务器上注册管理员账号
"""

import hmac
import hashlib
import requests
import json
import sys
import os


def register_admin(server_url: str, shared_secret: str, username: str = "admin", password: str = "Admin@123"):
    """
    注册管理员账号
    
    Args:
        server_url: 服务器 URL
        shared_secret: 管理员共享密钥
        username: 用户名
        password: 密码
    
    Returns:
        dict: 注册结果
    """
    nonce_url = f"{server_url}/_synapse/admin/v1/register/nonce"
    try:
        response = requests.get(nonce_url, timeout=10)
        if response.status_code != 200:
            print(f"获取 nonce 失败: HTTP {response.status_code}")
            print(f"响应: {response.text}")
            return None
        
        nonce_data = response.json()
        nonce = nonce_data.get("nonce")
        if not nonce:
            print("获取 nonce 失败: nonce 为空")
            return None
        
        print(f"获取 nonce 成功: {nonce[:20]}...")
    except Exception as e:
        print(f"获取 nonce 异常: {e}")
        return None
    
    message = nonce.encode('utf-8')
    message += b'\x00'
    message += username.encode('utf-8')
    message += b'\x00'
    message += password.encode('utf-8')
    message += b'\x00'
    message += b'admin'
    
    key = shared_secret.encode('utf-8')
    mac = hmac.new(key, message, hashlib.sha256)
    mac_hex = mac.hexdigest()
    
    print(f"计算 MAC: {mac_hex[:20]}...")
    
    register_url = f"{server_url}/_synapse/admin/v1/register"
    register_data = {
        "nonce": nonce,
        "username": username,
        "password": password,
        "admin": True,
        "mac": mac_hex
    }
    
    try:
        response = requests.post(register_url, json=register_data, timeout=10)
        result = response.json()
        
        if response.status_code == 200:
            print("注册成功!")
            print(f"用户ID: {result.get('user_id')}")
            print(f"设备ID: {result.get('device_id')}")
            token = result.get('access_token', '')
            print(f"Access Token: {token[:30]}..." if token else "无 Access Token")
            return result
        else:
            errcode = result.get("errcode", "UNKNOWN")
            error = result.get("error", "Unknown error")
            print(f"注册失败: {errcode}: {error}")
            return None
    except Exception as e:
        print(f"注册异常: {e}")
        return None


def login(server_url: str, username: str, password: str):
    """
    用户登录
    
    Args:
        server_url: 服务器 URL
        username: 用户名
        password: 密码
    
    Returns:
        dict: 登录结果
    """
    login_url = f"{server_url}/_matrix/client/v3/login"
    login_data = {
        "type": "m.login.password",
        "user": username,
        "password": password
    }
    
    try:
        response = requests.post(login_url, json=login_data, timeout=10)
        result = response.json()
        
        if response.status_code == 200:
            print("登录成功!")
            print(f"用户ID: {result.get('user_id')}")
            print(f"设备ID: {result.get('device_id')}")
            token = result.get('access_token', '')
            print(f"Access Token: {token[:30]}..." if token else "无 Access Token")
            return result
        else:
            errcode = result.get("errcode", "UNKNOWN")
            error = result.get("error", "Unknown error")
            print(f"登录失败: {errcode}: {error}")
            return None
    except Exception as e:
        print(f"登录异常: {e}")
        return None


def main():
    server_url = os.environ.get("SERVER_URL", "http://localhost:8008")
    shared_secret = os.environ.get("ADMIN_SHARED_SECRET", "")
    username = os.environ.get("ADMIN_USER", "admin")
    password = os.environ.get("ADMIN_PASS", "Admin@123")
    
    env_file = os.path.join(os.path.dirname(os.path.abspath(__file__)), ".env")
    if os.path.exists(env_file) and not shared_secret:
        with open(env_file, "r") as f:
            for line in f:
                line = line.strip()
                if line.startswith("ADMIN_SHARED_SECRET="):
                    shared_secret = line.split("=", 1)[1]
                    break
    
    if not shared_secret:
        print("错误: 未配置 ADMIN_SHARED_SECRET")
        sys.exit(1)
    
    print(f"服务器: {server_url}")
    print(f"用户名: {username}")
    print(f"共享密钥: {shared_secret[:10]}...")
    print("")
    
    print("=== 尝试登录 ===")
    login_result = login(server_url, username, password)
    
    if login_result:
        print("\n管理员账号已存在且密码正确")
        return 0
    
    print("\n=== 尝试注册 ===")
    register_result = register_admin(server_url, shared_secret, username, password)
    
    if register_result:
        return 0
    
    return 1


if __name__ == "__main__":
    sys.exit(main())
