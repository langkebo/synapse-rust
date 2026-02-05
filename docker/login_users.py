#!/usr/bin/env python3
"""
登录并获取token
"""

import requests
import json
import sys

BASE_URL = "http://localhost:8008"

def login_user(username, password):
    print(f"\n=== 登录用户: {username} ===")
    
    try:
        response = requests.post(
            f"{BASE_URL}/_matrix/client/r0/login",
            json={
                "type": "m.login.password",
                "user": username,
                "password": password
            },
            headers={"Content-Type": "application/json"},
            timeout=10
        )
        
        if response.status_code == 200:
            result = response.json()
            print(f"✓ 登录成功!")
            print(f"  User ID: {result.get('user_id', 'N/A')}")
            token = result.get('access_token', '')
            print(f"  Access Token: {token[:50]}...")
            
            # 保存token
            with open(f"{username}_token.txt", 'w') as f:
                f.write(token)
            with open(f"{username}_userid.txt", 'w') as f:
                f.write(result.get('user_id', ''))
            return token
        else:
            error = response.json()
            print(f"✗ 登录失败: {error.get('errcode', 'UNKNOWN')}")
            print(f"  {error.get('error', 'Unknown error')}")
            return None
            
    except Exception as e:
        print(f"✗ 错误: {e}")
        return None

def main():
    print("=" * 50)
    print("   Matrix 用户登录脚本")
    print("=" * 50)
    
    tokens = {}
    
    # 尝试登录管理员
    token = login_user("admin", "AdminPass123!")
    if token:
        tokens["admin"] = token
    
    # 尝试登录 testuser1
    token = login_user("testuser1", "TestPass123!")
    if token:
        tokens["testuser1"] = token
    
    print("\n" + "=" * 50)
    print(f"   获取到 {len(tokens)} 个token")
    print("=" * 50)
    
    return tokens

if __name__ == "__main__":
    tokens = main()
    print("\nTokens:", list(tokens.keys()))
