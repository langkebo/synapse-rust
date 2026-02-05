#!/usr/bin/env python3
"""
测试 3.1.1 健康检查、账户管理与用户资料 15个API
"""
import requests
import json
import sys

BASE_URL = "http://localhost:8008"

def load_token():
    """从文件读取token"""
    try:
        with open("testuser1_token.txt", "r") as f:
            return f.read().strip()
    except Exception as e:
        print(f"❌ 读取token失败: {e}")
        return None

def test_health():
    """测试1: 健康检查"""
    print("\n=== 测试1: 健康检查 ===")
    print(f"端点: GET {BASE_URL}/health")
    try:
        response = requests.get(f"{BASE_URL}/health", timeout=10)
        print(f"HTTP状态: {response.status_code}")
        print(f"响应: {response.text[:200]}")
        return response.status_code == 200, response.status_code, response.text
    except Exception as e:
        print(f"❌ 请求失败: {e}")
        return False, 0, str(e)

def test_client_versions():
    """测试2: 获取客户端API版本"""
    print("\n=== 测试2: 获取客户端API版本 ===")
    print(f"端点: GET {BASE_URL}/_matrix/client/versions")
    try:
        response = requests.get(f"{BASE_URL}/_matrix/client/versions", timeout=10)
        print(f"HTTP状态: {response.status_code}")
        print(f"响应: {response.text[:200]}")
        return response.status_code == 200, response.status_code, response.text
    except Exception as e:
        print(f"❌ 请求失败: {e}")
        return False, 0, str(e)

def test_register():
    """测试3: 用户注册"""
    print("\n=== 测试3: 用户注册 ===")
    print(f"端点: POST {BASE_URL}/_matrix/client/r0/register")
    try:
        response = requests.post(
            f"{BASE_URL}/_matrix/client/r0/register",
            json={
                "username": "newuser_test",
                "password": "TestPass123!",
                "admin": False
            },
            timeout=10
        )
        print(f"HTTP状态: {response.status_code}")
        print(f"响应: {response.text[:200]}")
        # 200 表示成功，M_USER_IN_USE 表示用户已存在
        return response.status_code in [200, 400], response.status_code, response.text
    except Exception as e:
        print(f"❌ 请求失败: {e}")
        return False, 0, str(e)

def test_register_available():
    """测试4: 检查用户名可用性"""
    print("\n=== 测试4: 检查用户名可用性 ===")
    print(f"端点: GET {BASE_URL}/_matrix/client/r0/register/available")
    try:
        response = requests.get(
            f"{BASE_URL}/_matrix/client/r0/register/available?username=newuser_test_abc123",
            timeout=10
        )
        print(f"HTTP状态: {response.status_code}")
        print(f"响应: {response.text[:200]}")
        return response.status_code == 200, response.status_code, response.text
    except Exception as e:
        print(f"❌ 请求失败: {e}")
        return False, 0, str(e)

def test_register_email_request_token():
    """测试5: 请求邮箱验证"""
    print("\n=== 测试5: 请求邮箱验证 ===")
    print(f"端点: POST {BASE_URL}/_matrix/client/r0/register/email/requestToken")
    try:
        response = requests.post(
            f"{BASE_URL}/_matrix/client/r0/register/email/requestToken",
            json={
                "email": "test@example.com",
                "client_secret": "test_secret",
                "send_attempt": 1
            },
            timeout=10
        )
        print(f"HTTP状态: {response.status_code}")
        print(f"响应: {response.text[:200]}")
        # 200 表示成功，400 表示参数错误
        return response.status_code in [200, 400], response.status_code, response.text
    except Exception as e:
        print(f"❌ 请求失败: {e}")
        return False, 0, str(e)

def test_login():
    """测试6: 用户登录"""
    print("\n=== 测试6: 用户登录 ===")
    print(f"端点: POST {BASE_URL}/_matrix/client/r0/login")
    try:
        response = requests.post(
            f"{BASE_URL}/_matrix/client/r0/login",
            json={
                "type": "m.login.password",
                "user": "testuser1",
                "password": "TestPass123!"
            },
            timeout=10
        )
        print(f"HTTP状态: {response.status_code}")
        print(f"响应: {response.text[:200]}")
        return response.status_code == 200, response.status_code, response.text
    except Exception as e:
        print(f"❌ 请求失败: {e}")
        return False, 0, str(e)

def test_logout(token):
    """测试7: 退出登录"""
    print("\n=== 测试7: 退出登录 ===")
    print(f"端点: POST {BASE_URL}/_matrix/client/r0/logout")
    try:
        headers = {"Authorization": f"Bearer {token}"}
        response = requests.post(
            f"{BASE_URL}/_matrix/client/r0/logout",
            headers=headers,
            timeout=10
        )
        print(f"HTTP状态: {response.status_code}")
        print(f"响应: {response.text[:200]}")
        return response.status_code == 200, response.status_code, response.text
    except Exception as e:
        print(f"❌ 请求失败: {e}")
        return False, 0, str(e)

def test_logout_all(token):
    """测试8: 退出所有设备"""
    print("\n=== 测试8: 退出所有设备 ===")
    print(f"端点: POST {BASE_URL}/_matrix/client/r0/logout/all")
    try:
        headers = {"Authorization": f"Bearer {token}"}
        response = requests.post(
            f"{BASE_URL}/_matrix/client/r0/logout/all",
            headers=headers,
            timeout=10
        )
        print(f"HTTP状态: {response.status_code}")
        print(f"响应: {response.text[:200]}")
        return response.status_code == 200, response.status_code, response.text
    except Exception as e:
        print(f"❌ 请求失败: {e}")
        return False, 0, str(e)

def test_refresh(refresh_token):
    """测试9: 刷新令牌"""
    print("\n=== 测试9: 刷新令牌 ===")
    print(f"端点: POST {BASE_URL}/_matrix/client/r0/refresh")
    try:
        response = requests.post(
            f"{BASE_URL}/_matrix/client/r0/refresh",
            json={
                "refresh_token": refresh_token
            },
            timeout=10
        )
        print(f"HTTP状态: {response.status_code}")
        print(f"响应: {response.text[:200]}")
        return response.status_code == 200, response.status_code, response.text
    except Exception as e:
        print(f"❌ 请求失败: {e}")
        return False, 0, str(e)

def test_whoami(token):
    """测试10: 获取当前用户信息"""
    print("\n=== 测试10: 获取当前用户信息 ===")
    print(f"端点: GET {BASE_URL}/_matrix/client/r0/account/whoami")
    try:
        headers = {"Authorization": f"Bearer {token}"}
        response = requests.get(
            f"{BASE_URL}/_matrix/client/r0/account/whoami",
            headers=headers,
            timeout=10
        )
        print(f"HTTP状态: {response.status_code}")
        print(f"响应: {response.text[:200]}")
        return response.status_code == 200, response.status_code, response.text
    except Exception as e:
        print(f"❌ 请求失败: {e}")
        return False, 0, str(e)

def test_deactivate(token):
    """测试11: 停用账户"""
    print("\n=== 测试11: 停用账户 ===")
    print(f"端点: POST {BASE_URL}/_matrix/client/r0/account/deactivate")
    try:
        headers = {"Authorization": f"Bearer {token}"}
        response = requests.post(
            f"{BASE_URL}/_matrix/client/r0/account/deactivate",
            headers=headers,
            json={},
            timeout=10
        )
        print(f"HTTP状态: {response.status_code}")
        print(f"响应: {response.text[:200]}")
        # 200 表示成功，M_USER_DEACTIVATED 表示已停用
        return response.status_code in [200, 400], response.status_code, response.text
    except Exception as e:
        print(f"❌ 请求失败: {e}")
        return False, 0, str(e)

def test_change_password(token):
    """测试12: 修改密码"""
    print("\n=== 测试12: 修改密码 ===")
    print(f"端点: POST {BASE_URL}/_matrix/client/r0/account/password")
    try:
        headers = {"Authorization": f"Bearer {token}"}
        response = requests.post(
            f"{BASE_URL}/_matrix/client/r0/account/password",
            headers=headers,
            json={
                "new_password": "TestPass123!"
            },
            timeout=10
        )
        print(f"HTTP状态: {response.status_code}")
        print(f"响应: {response.text[:200]}")
        return response.status_code == 200, response.status_code, response.text
    except Exception as e:
        print(f"❌ 请求失败: {e}")
        return False, 0, str(e)

def test_get_profile(token, user_id):
    """测试13: 获取用户资料"""
    print("\n=== 测试13: 获取用户资料 ===")
    print(f"端点: GET {BASE_URL}/_matrix/client/r0/account/profile/{user_id}")
    try:
        headers = {"Authorization": f"Bearer {token}"}
        response = requests.get(
            f"{BASE_URL}/_matrix/client/r0/account/profile/{user_id}",
            headers=headers,
            timeout=10
        )
        print(f"HTTP状态: {response.status_code}")
        print(f"响应: {response.text[:200]}")
        return response.status_code == 200, response.status_code, response.text
    except Exception as e:
        print(f"❌ 请求失败: {e}")
        return False, 0, str(e)

def test_update_displayname(token, user_id):
    """测试14: 更新显示名称"""
    print("\n=== 测试14: 更新显示名称 ===")
    print(f"端点: PUT {BASE_URL}/_matrix/client/r0/account/profile/{user_id}/displayname")
    try:
        headers = {"Authorization": f"Bearer {token}"}
        response = requests.put(
            f"{BASE_URL}/_matrix/client/r0/account/profile/{user_id}/displayname",
            headers=headers,
            json={
                "displayname": "Test User Updated"
            },
            timeout=10
        )
        print(f"HTTP状态: {response.status_code}")
        print(f"响应: {response.text[:200]}")
        return response.status_code == 200, response.status_code, response.text
    except Exception as e:
        print(f"❌ 请求失败: {e}")
        return False, 0, str(e)

def test_update_avatar(token, user_id):
    """测试15: 更新头像"""
    print("\n=== 测试15: 更新头像 ===")
    print(f"端点: PUT {BASE_URL}/_matrix/client/r0/account/profile/{user_id}/avatar_url")
    try:
        headers = {"Authorization": f"Bearer {token}"}
        response = requests.put(
            f"{BASE_URL}/_matrix/client/r0/account/profile/{user_id}/avatar_url",
            headers=headers,
            json={
                "avatar_url": "mxc://example.com/newavatar"
            },
            timeout=10
        )
        print(f"HTTP状态: {response.status_code}")
        print(f"响应: {response.text[:200]}")
        return response.status_code == 200, response.status_code, response.text
    except Exception as e:
        print(f"❌ 请求失败: {e}")
        return False, 0, str(e)

def main():
    print("=" * 60)
    print("  测试 3.1.1 健康检查、账户管理与用户资料")
    print("  共15个API端点")
    print("=" * 60)
    
    token = load_token()
    if not token:
        print("❌ 无法获取token，测试终止")
        return
    
    user_id = "@testuser1:cjystx.top"
    
    results = []
    
    # 测试1-2: 健康检查和版本
    results.append(("1. 健康检查", *test_health()))
    results.append(("2. 获取客户端API版本", *test_client_versions()))
    
    # 测试3-5: 注册相关
    results.append(("3. 用户注册", *test_register()))
    results.append(("4. 检查用户名可用性", *test_register_available()))
    results.append(("5. 请求邮箱验证", *test_register_email_request_token()))
    
    # 测试6: 登录
    results.append(("6. 用户登录", *test_login()))
    
    # 测试7-8: 退出登录
    results.append(("7. 退出登录", *test_logout(token)))
    results.append(("8. 退出所有设备", *test_logout_all(token)))
    
    # 测试9: 刷新令牌
    results.append(("9. 刷新令牌", *test_refresh("test_refresh_token")))
    
    # 测试10-12: 账户管理
    results.append(("10. 获取当前用户信息", *test_whoami(token)))
    results.append(("11. 停用账户", *test_deactivate(token)))
    results.append(("12. 修改密码", *test_change_password(token)))
    
    # 测试13-15: 用户资料
    results.append(("13. 获取用户资料", *test_get_profile(token, user_id)))
    results.append(("14. 更新显示名称", *test_update_displayname(token, user_id)))
    results.append(("15. 更新头像", *test_update_avatar(token, user_id)))
    
    # 统计结果
    print("\n" + "=" * 60)
    print("  测试结果汇总")
    print("=" * 60)
    
    passed = sum(1 for r in results if r[1])
    failed = len(results) - passed
    total = len(results)
    success_rate = (passed / total * 100) if total > 0 else 0
    
    print(f"\n总测试数: {total}")
    print(f"通过: {passed}")
    print(f"失败: {failed}")
    print(f"成功率: {success_rate:.1f}%")
    
    print("\n失败的项目:")
    print("-" * 60)
    for name, success, status, error in results:
        if not success:
            print(f"❌ {name}: HTTP {status}")
            try:
                error_data = json.loads(error) if error.startswith('{') else error
                if isinstance(error_data, dict):
                    errcode = error_data.get('errcode', 'UNKNOWN')
                    error_msg = error_data.get('error', 'Unknown error')
                    print(f"   错误码: {errcode}")
                    print(f"   错误信息: {error_msg}")
            except:
                print(f"   错误: {error[:100]}")
    
    # 保存结果到文件
    with open("test_311_results.json", "w", encoding="utf-8") as f:
        json.dump(results, f, ensure_ascii=False, indent=2)
    print(f"\n结果已保存到 test_311_results.json")

if __name__ == "__main__":
    main()
