#!/usr/bin/env python3
"""
注册所有测试用户
"""

import hmac
import hashlib
import requests
import json
import sys

BASE_URL = "http://localhost:8008"
SHARED_SECRET = "test_shared_secret"

def calculate_hmac(nonce, username, password, admin):
    key = SHARED_SECRET.encode('utf-8')
    message = f"{nonce}\x00{username}\x00{password}\x00{'admin' if admin else 'notadmin'}"
    mac = hmac.new(key, message.encode('utf-8'), hashlib.sha256)
    return mac.hexdigest()

def register_user(username, password, is_admin=False):
    print(f"\n=== 注册用户: {username} (admin: {is_admin}) ===")
    
    try:
        # 获取nonce
        nonce_response = requests.get(f"{BASE_URL}/_synapse/admin/v1/register/nonce", timeout=10)
        
        if nonce_response.status_code != 200:
            print(f"✗ 获取nonce失败: {nonce_response.status_code}")
            return False
        
        nonce = nonce_response.json()['nonce']
        
        # 计算HMAC
        mac = calculate_hmac(nonce, username, password, is_admin)
        
        # 注册
        register_data = {
            "nonce": nonce,
            "username": username,
            "password": password,
            "admin": is_admin,
            "mac": mac
        }
        
        reg_response = requests.post(
            f"{BASE_URL}/_synapse/admin/v1/register",
            json=register_data,
            headers={"Content-Type": "application/json"},
            timeout=10
        )
        
        if reg_response.status_code == 200:
            result = reg_response.json()
            print(f"✓ {username} 注册成功!")
            print(f"  User ID: {result.get('user_id', 'N/A')}")
            print(f"  Access Token: {result.get('access_token', 'N/A')[:50]}...")
            
            # 保存token
            with open(f"{username}_token.txt", 'w') as f:
                f.write(result.get('access_token', ''))
            with open(f"{username}_userid.txt", 'w') as f:
                f.write(result.get('user_id', ''))
            return True
        else:
            error = reg_response.json()
            errcode = error.get('errcode', 'UNKNOWN')
            
            if errcode == "M_USER_IN_USE":
                print(f"⚠ {username} 已存在")
                return True
            else:
                print(f"✗ 注册失败: {errcode}")
                print(f"  {error.get('error', 'Unknown error')}")
                return False
                
    except requests.exceptions.ConnectionError as e:
        print(f"✗ 连接失败: {e}")
        return False
    except Exception as e:
        print(f"✗ 错误: {e}")
        return False

def main():
    print("=" * 50)
    print("   Matrix 测试用户注册脚本")
    print("=" * 50)
    
    success_count = 0
    fail_count = 0
    
    # 注册管理员
    if register_user("admin", "AdminPass123!", True):
        success_count += 1
    else:
        fail_count += 1
    
    # 注册测试用户
    for i in range(1, 7):
        username = f"testuser{i}"
        if register_user(username, "TestPass123!"):
            success_count += 1
        else:
            fail_count += 1
    
    print("\n" + "=" * 50)
    print(f"   注册完成! 成功: {success_count}, 失败: {fail_count}")
    print("=" * 50)
    
    # 显示token文件
    print("\n已创建的token文件:")
    import os
    for f in os.listdir('.'):
        if f.endswith('_token.txt'):
            print(f"  - {f}")
    
    return fail_count == 0

if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)
