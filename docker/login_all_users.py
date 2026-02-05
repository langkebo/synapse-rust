#!/usr/bin/env python3
"""
登录所有测试用户并更新文档
"""

import requests
import json
import sys

BASE_URL = "http://localhost:8008"

def login_user(username, password):
    """登录获取token"""
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
            token = result.get('access_token', '')
            refresh_token = result.get('refresh_token', '')
            
            with open(f"{username}_token.txt", 'w') as f:
                f.write(token)
            with open(f"{username}_refresh_token.txt", 'w') as f:
                f.write(refresh_token)
            
            print(f"✓ {username} 登录成功")
            return True, token, refresh_token
        else:
            print(f"✗ {username} 登录失败: {response.status_code}")
            return False, "", ""
    except Exception as e:
        print(f"✗ {username} 错误: {e}")
        return False, "", ""

def main():
    print("=== 登录所有测试用户 ===\n")
    
    tokens = {}
    refresh_tokens = {}
    
    users = [
        ("admin", "Wzc9890951!"),
        ("testuser1", "TestPass123!"),
        ("testuser2", "TestPass123!"),
        ("testuser3", "TestPass123!"),
        ("testuser4", "TestPass123!"),
        ("testuser5", "TestPass123!"),
        ("testuser6", "TestPass123!"),
    ]
    
    for username, password in users:
        success, token, refresh = login_user(username, password)
        if success:
            tokens[username] = token
            refresh_tokens[username] = refresh
        import time
        time.sleep(1)
    
    print("\n=== Token汇总 ===")
    for username, token in tokens.items():
        print(f"{username}: {token[:50]}...")
    
    return tokens, refresh_tokens

if __name__ == "__main__":
    main()
