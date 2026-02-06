#!/usr/bin/env python3
"""
批量登录获取测试用户Token
为API测试准备认证信息
"""

import requests
import json
import sys
from datetime import datetime

# 配置
BASE_URL = "http://localhost:8008"
OUTPUT_FILE = "/home/hula/synapse_rust/docker/tokens.json"

# 测试用户配置
TEST_USERS = [
    {
        "username": "testuser1",
        "password": "TestUser123!",
        "user_id": "@testuser1:cjystx.top",
        "purpose": "主要测试用户"
    },
    {
        "username": "testuser2",
        "password": "TestUser123!",
        "user_id": "@testuser2:cjystx.top",
        "purpose": "好友功能测试"
    },
    {
        "username": "testuser3",
        "password": "TestUser123!",
        "user_id": "@testuser3:cjystx.top",
        "purpose": "房间操作测试"
    },
    {
        "username": "testuser4",
        "password": "TestUser123!",
        "user_id": "@testuser4:cjystx.top",
        "purpose": "联邦API测试"
    },
    {
        "username": "testuser5",
        "password": "TestUser123!",
        "user_id": "@testuser5:cjystx.top",
        "purpose": "设备管理测试"
    },
    {
        "username": "testuser6",
        "password": "TestUser123!",
        "user_id": "@testuser6:cjystx.top",
        "purpose": "媒体文件测试"
    },
]

def login_user(username, password):
    """登录获取Token"""
    try:
        response = requests.post(
            f"{BASE_URL}/_matrix/client/r0/login",
            json={
                "type": "m.login.password",
                "user": username,
                "password": password
            },
            timeout=10
        )
        
        if response.status_code == 200:
            data = response.json()
            return {
                "success": True,
                "access_token": data.get("access_token"),
                "refresh_token": data.get("refresh_token"),
                "user_id": data.get("user_id"),
                "device_id": data.get("device_id")
            }
        else:
            return {
                "success": False,
                "error": f"HTTP {response.status_code}: {response.text[:200]}"
            }
    except Exception as e:
        return {
            "success": False,
            "error": str(e)
        }

def main():
    """主函数"""
    print("="*80)
    print("批量登录获取测试用户Token")
    print("="*80)
    print(f"服务器地址: {BASE_URL}")
    print(f"准备时间: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    print()
    
    results = {
        "info": {
            "base_url": BASE_URL,
            "created": datetime.now().strftime('%Y-%m-%d %H:%M:%S'),
            "note": "Token有效期有限，如遇401错误请重新运行此脚本"
        },
        "users": []
    }
    
    success_count = 0
    fail_count = 0
    
    for user in TEST_USERS:
        username = user["username"]
        password = user["password"]
        
        print(f"正在登录: {username}...", end=" ")
        
        result = login_user(username, password)
        
        user_result = {
            "username": username,
            "user_id": user["user_id"],
            "purpose": user["purpose"],
            "login_success": result["success"],
        }
        
        if result["success"]:
            user_result["access_token"] = result["access_token"]
            user_result["refresh_token"] = result.get("refresh_token")
            user_result["device_id"] = result.get("device_id")
            print(f"✅ 成功 (device_id: {result.get('device_id')})")
            success_count += 1
        else:
            user_result["error"] = result.get("error")
            print(f"❌ 失败 ({result.get('error')})")
            fail_count += 1
        
        results["users"].append(user_result)
    
    # 保存结果
    with open(OUTPUT_FILE, 'w', encoding='utf-8') as f:
        json.dump(results, f, indent=2, ensure_ascii=False)
    
    print()
    print("="*80)
    print("登录结果汇总")
    print("="*80)
    print(f"成功: {success_count}/{len(TEST_USERS)}")
    print(f"失败: {fail_count}/{len(TEST_USERS)}")
    print()
    print(f"结果已保存到: {OUTPUT_FILE}")
    
    # 显示环境变量配置
    print()
    print("="*80)
    print("环境变量配置（可复制到终端）")
    print("="*80)
    
    for user in results["users"]:
        if user["login_success"]:
            username = user["username"]
            token = user["access_token"][:50] + "..." if len(user["access_token"]) > 50 else user["access_token"]
            print(f"export SYNAPSE_{username.upper()}_TOKEN=\"{user['access_token']}\"  # {token}")
    
    # 返回退出码
    if fail_count > 0:
        print()
        print(f"⚠️  有 {fail_count} 个用户登录失败，请检查错误信息")
        return 1
    return 0

if __name__ == "__main__":
    sys.exit(main())
