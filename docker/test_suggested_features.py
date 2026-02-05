#!/usr/bin/env python3
"""
测试建议的功能：
1. 停用账户后的恢复功能
2. 邮箱验证完整流程
3. 密码修改后的登录验证
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

def test_deactivate_and_recover():
    """测试1: 停用账户后的恢复功能"""
    print("\n" + "=" * 60)
    print("  测试1: 停用账户后的恢复功能")
    print("=" * 60)
    
    # 1. 先登录获取token
    print("\n步骤1: 登录用户")
    login_resp = requests.post(
        f"{BASE_URL}/_matrix/client/r0/login",
        json={
            "type": "m.login.password",
            "user": "testuser1",
            "password": "TestPass123!"
        },
        timeout=10
    )
    print(f"登录状态: {login_resp.status_code}")
    if login_resp.status_code != 200:
        print(f"登录失败: {login_resp.text}")
        return False
    
    token = login_resp.json().get("access_token")
    device_id = login_resp.json().get("device_id")
    print(f"登录成功! device_id: {device_id}")
    
    # 2. 获取用户信息
    print("\n步骤2: 获取当前用户信息")
    whoami_resp = requests.get(
        f"{BASE_URL}/_matrix/client/r0/account/whoami",
        headers={"Authorization": f"Bearer {token}"},
        timeout=10
    )
    print(f"whoami状态: {whoami_resp.status_code}")
    print(f"用户信息: {whoami_resp.json()}")
    
    # 3. 停用账户
    print("\n步骤3: 停用账户")
    deactivate_resp = requests.post(
        f"{BASE_URL}/_matrix/client/r0/account/deactivate",
        headers={"Authorization": f"Bearer {token}"},
        json={},
        timeout=10
    )
    print(f"停用状态: {deactivate_resp.status_code}")
    print(f"响应: {deactivate_resp.text}")
    
    if deactivate_resp.status_code != 200:
        print("❌ 停用失败")
        return False
    print("✅ 账户已停用")
    
    # 4. 尝试使用旧token访问
    print("\n步骤4: 尝试使用停用前的token访问")
    whoami_after = requests.get(
        f"{BASE_URL}/_matrix/client/r0/account/whoami",
        headers={"Authorization": f"Bearer {token}"},
        timeout=10
    )
    print(f"访问状态: {whoami_after.status_code}")
    print(f"响应: {whoami_after.text}")
    if whoami_after.status_code == 401:
        print("✅ Token已失效 (预期行为)")
    
    # 5. 尝试登录（应该失败）
    print("\n步骤5: 尝试使用原密码登录")
    login_after = requests.post(
        f"{BASE_URL}/_matrix/client/r0/login",
        json={
            "type": "m.login.password",
            "user": "testuser1",
            "password": "TestPass123!"
        },
        timeout=10
    )
    print(f"登录状态: {login_after.status_code}")
    print(f"响应: {login_after.text}")
    
    # 6. 通过数据库恢复账户
    print("\n步骤6: 通过数据库直接恢复账户")
    import subprocess
    recover_result = subprocess.run(
        ["docker", "exec", "synapse_postgres", "psql", "-U", "synapse", "-d", "synapse_test", "-t", "-c",
         "UPDATE users SET deactivated = FALSE WHERE user_id = '@testuser1:cjystx.top';"],
        capture_output=True, text=True
    )
    print(f"数据库恢复结果: {recover_result.returncode}")
    
    # 验证恢复
    check_result = subprocess.run(
        ["docker", "exec", "synapse_postgres", "psql", "-U", "synapse", "-d", "synapse_test", "-t", "-c",
         "SELECT deactivated FROM users WHERE user_id = '@testuser1:cjystx.top';"],
        capture_output=True, text=True
    )
    print(f"验证账户状态: {check_result.stdout.strip()}")
    
    # 7. 尝试恢复后登录
    print("\n步骤7: 恢复后尝试登录")
    login_after_recover = requests.post(
        f"{BASE_URL}/_matrix/client/r0/login",
        json={
            "type": "m.login.password",
            "user": "testuser1",
            "password": "TestPass123!"
        },
        timeout=10
    )
    print(f"恢复后登录状态: {login_after_recover.status_code}")
    print(f"响应: {login_after_recover.text}")
    
    if login_after_recover.status_code == 200:
        print("✅ 账户恢复成功，可以重新登录")
        new_token = login_after_recover.json().get("access_token")
        # 保存新token
        with open("testuser1_token.txt", "w") as f:
            f.write(new_token)
        return True
    else:
        print("❌ 恢复后登录失败")
        return False

def test_email_verification():
    """测试2: 邮箱验证完整流程"""
    print("\n" + "=" * 60)
    print("  测试2: 邮箱验证完整流程")
    print("=" * 60)
    
    # 1. 请求邮箱验证
    print("\n步骤1: 请求邮箱验证token")
    request_resp = requests.post(
        f"{BASE_URL}/_matrix/client/r0/register/email/requestToken",
        json={
            "email": "test@example.com",
            "client_secret": "test_secret_123",
            "send_attempt": 1
        },
        timeout=10
    )
    print(f"请求状态: {request_resp.status_code}")
    print(f"响应: {request_resp.text}")
    
    if request_resp.status_code != 200:
        print("❌ 请求邮箱验证失败")
        return False
    
    response_data = request_resp.json()
    sid = response_data.get("sid")
    submit_url = response_data.get("submit_url")
    expires_in = response_data.get("expires_in", 3600)
    
    print(f"\n验证信息:")
    print(f"  Session ID: {sid}")
    print(f"  Submit URL: {submit_url}")
    print(f"  有效期: {expires_in}秒")
    
    # 2. 检查submit_url格式
    print("\n步骤2: 检查submit_url格式")
    if submit_url:
        print(f"✅ submit_url存在: {submit_url}")
        # Matrix规范中，submit_url应该包含完整的验证端点
        # 实际应该调用 submit_url 提交 token
    else:
        print("❌ submit_url为空")
    
    # 3. 检查项目代码是否实现了submitToken
    print("\n步骤3: 检查submitToken端点实现")
    submit_resp = requests.post(
        f"{BASE_URL}/_matrix/client/r0/register/email/submitToken",
        json={
            "sid": sid,
            "client_secret": "test_secret_123",
            "token": "test_verification_token"
        },
        timeout=10
    )
    print(f"submitToken状态: {submit_resp.status_code}")
    print(f"响应: {submit_resp.text}")
    
    if submit_resp.status_code == 200:
        print("✅ submitToken端点已实现")
        return True
    elif submit_resp.status_code == 400 or submit_resp.status_code == 404:
        print("⚠️ submitToken端点未完全实现或参数错误")
        print("   这是常见问题，submitToken通常需要额外的验证逻辑")
        return False
    else:
        print(f"❌ submitToken返回未知状态码: {submit_resp.status_code}")
        return False

def test_password_change_login():
    """测试3: 密码修改后的登录验证"""
    print("\n" + "=" * 60)
    print("  测试3: 密码修改后的登录验证")
    print("=" * 60)
    
    # 1. 先登录获取token
    print("\n步骤1: 登录用户")
    login_resp = requests.post(
        f"{BASE_URL}/_matrix/client/r0/login",
        json={
            "type": "m.login.password",
            "user": "testuser1",
            "password": "TestPass123!"
        },
        timeout=10
    )
    print(f"登录状态: {login_resp.status_code}")
    if login_resp.status_code != 200:
        print(f"登录失败: {login_resp.text}")
        return False
    
    token = login_resp.json().get("access_token")
    print("✅ 登录成功")
    
    # 2. 修改密码
    print("\n步骤2: 修改密码")
    new_password = "NewPass456!"
    change_resp = requests.post(
        f"{BASE_URL}/_matrix/client/r0/account/password",
        headers={"Authorization": f"Bearer {token}"},
        json={"new_password": new_password},
        timeout=10
    )
    print(f"修改密码状态: {change_resp.status_code}")
    print(f"响应: {change_resp.text}")
    
    if change_resp.status_code != 200:
        print("❌ 修改密码失败")
        return False
    print("✅ 密码修改成功")
    
    # 3. 尝试使用旧密码登录
    print("\n步骤3: 尝试使用旧密码登录")
    login_old = requests.post(
        f"{BASE_URL}/_matrix/client/r0/login",
        json={
            "type": "m.login.password",
            "user": "testuser1",
            "password": "TestPass123!"
        },
        timeout=10
    )
    print(f"旧密码登录状态: {login_old.status_code}")
    print(f"响应: {login_old.text}")
    
    if login_old.status_code == 401:
        print("✅ 旧密码已失效 (预期行为)")
    else:
        print("❌ 旧密码仍然有效 (安全隐患!)")
    
    # 4. 尝试使用新密码登录
    print("\n步骤4: 尝试使用新密码登录")
    login_new = requests.post(
        f"{BASE_URL}/_matrix/client/r0/login",
        json={
            "type": "m.login.password",
            "user": "testuser1",
            "password": new_password
        },
        timeout=10
    )
    print(f"新密码登录状态: {login_new.status_code}")
    print(f"响应: {login_new.text}")
    
    if login_new.status_code == 200:
        print("✅ 新密码登录成功")
        new_token = login_new.json().get("access_token")
        
        # 5. 恢复原始密码
        print("\n步骤5: 恢复原始密码")
        recover_resp = requests.post(
            f"{BASE_URL}/_matrix/client/r0/account/password",
            headers={"Authorization": f"Bearer {new_token}"},
            json={"new_password": "TestPass123!"},
            timeout=10
        )
        print(f"恢复密码状态: {recover_resp.status_code}")
        if recover_resp.status_code == 200:
            print("✅ 原始密码已恢复")
            with open("testuser1_token.txt", "w") as f:
                f.write(new_token)
            return True
        else:
            print("❌ 恢复原始密码失败")
            return False
    else:
        print("❌ 新密码登录失败")
        return False

def main():
    print("=" * 60)
    print("  建议功能测试")
    print("  1. 停用账户恢复功能")
    print("  2. 邮箱验证完整流程")
    print("  3. 密码修改后登录验证")
    print("=" * 60)
    
    results = []
    
    # 测试1: 停用账户恢复
    result1 = test_deactivate_and_recover()
    results.append(("停用账户恢复功能", result1))
    
    # 测试2: 邮箱验证
    result2 = test_email_verification()
    results.append(("邮箱验证完整流程", result2))
    
    # 测试3: 密码修改后登录
    result3 = test_password_change_login()
    results.append(("密码修改后登录", result3))
    
    # 汇总结果
    print("\n" + "=" * 60)
    print("  测试结果汇总")
    print("=" * 60)
    
    for name, passed in results:
        status = "✅ 通过" if passed else "❌ 失败"
        print(f"{name}: {status}")
    
    passed_count = sum(1 for _, p in results if p)
    total = len(results)
    print(f"\n总计: {passed_count}/{total} 通过")
    
    # 保存详细结果
    with open("test_suggestions_results.json", "w", encoding="utf-8") as f:
        json.dump(results, f, ensure_ascii=False, indent=2)
    print(f"\n结果已保存到 test_suggestions_results.json")

if __name__ == "__main__":
    main()
