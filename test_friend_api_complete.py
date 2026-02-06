#!/usr/bin/env python3
"""
好友系统API完整测试脚本
测试所有好友系统API端点，使用有效的token
"""

import requests
import json
import sys
from datetime import datetime

BASE_URL = "http://localhost:8008"

class FriendAPITester:
    def __init__(self):
        self.base_url = BASE_URL
        self.user1_token = None
        self.user1_id = None
        self.user2_token = None
        self.user2_id = None
        self.user3_token = None
        self.user3_id = None
        self.test_results = []
        
    def print_header(self, title):
        print("\n" + "=" * 80)
        print(f"  {title}")
        print("=" * 80)
        
    def print_test(self, test_name, status, details=""):
        status_symbol = "✓" if status == "PASS" else "✗"
        print(f"{status_symbol} {test_name}")
        if details:
            print(f"  {details}")
        
    def log_result(self, test_name, endpoint, expected_code, actual_code, status, details=""):
        self.test_results.append({
            "test_name": test_name,
            "endpoint": endpoint,
            "expected_code": expected_code,
            "actual_code": actual_code,
            "status": status,
            "details": details
        })
        
    def register_user(self, username, password):
        """注册用户"""
        url = f"{self.base_url}/_matrix/client/r0/register"
        data = {
            "username": username,
            "password": password,
            "auth": {"type": "m.login.dummy"}
        }
        response = requests.post(url, json=data)
        if response.status_code == 200:
            return response.json()
        return None
        
    def login_user(self, username, password):
        """登录用户"""
        url = f"{self.base_url}/_matrix/client/r0/login"
        data = {
            "type": "m.login.password",
            "user": username,
            "password": password
        }
        response = requests.post(url, json=data)
        if response.status_code == 200:
            return response.json()
        return None
        
    def setup_test_users(self):
        """设置测试用户"""
        self.print_header("设置测试用户")
        
        timestamp = datetime.now().strftime("%Y%m%d%H%M%S")
        
        user1_name = f"friend_test_1_{timestamp}"
        user2_name = f"friend_test_2_{timestamp}"
        user3_name = f"friend_test_3_{timestamp}"
        
        print(f"创建测试用户: {user1_name}")
        user1 = self.register_user(user1_name, "TestUser123!")
        if user1:
            self.user1_id = user1.get("user_id")
            print(f"  用户ID: {self.user1_id}")
        else:
            print(f"  注册失败，尝试登录")
            login1 = self.login_user(user1_name, "TestUser123!")
            if login1:
                self.user1_id = login1.get("user_id")
                self.user1_token = login1.get("access_token")
                print(f"  用户ID: {self.user1_id}")
        
        print(f"创建测试用户: {user2_name}")
        user2 = self.register_user(user2_name, "TestUser123!")
        if user2:
            self.user2_id = user2.get("user_id")
            print(f"  用户ID: {self.user2_id}")
        else:
            print(f"  注册失败，尝试登录")
            login2 = self.login_user(user2_name, "TestUser123!")
            if login2:
                self.user2_id = login2.get("user_id")
                self.user2_token = login2.get("access_token")
                print(f"  用户ID: {self.user2_id}")
        
        print(f"创建测试用户: {user3_name}")
        user3 = self.register_user(user3_name, "TestUser123!")
        if user3:
            self.user3_id = user3.get("user_id")
            print(f"  用户ID: {self.user3_id}")
        else:
            print(f"  注册失败，尝试登录")
            login3 = self.login_user(user3_name, "TestUser123!")
            if login3:
                self.user3_id = login3.get("user_id")
                self.user3_token = login3.get("access_token")
                print(f"  用户ID: {self.user3_id}")
        
        if not self.user1_token:
            login1 = self.login_user(user1_name, "TestUser123!")
            if login1:
                self.user1_token = login1.get("access_token")
                
        if not self.user2_token:
            login2 = self.login_user(user2_name, "TestUser123!")
            if login2:
                self.user2_token = login2.get("access_token")
                
        if not self.user3_token:
            login3 = self.login_user(user3_name, "TestUser123!")
            if login3:
                self.user3_token = login3.get("access_token")
        
        print(f"\n用户1 Token: {self.user1_token[:20]}..." if self.user1_token else "用户1 Token: None")
        print(f"用户2 Token: {self.user2_token[:20]}..." if self.user2_token else "用户2 Token: None")
        print(f"用户3 Token: {self.user3_token[:20]}..." if self.user3_token else "用户3 Token: None")
        
        return bool(self.user1_token and self.user2_token and self.user3_token)
        
    def test_search_users(self):
        """测试1: 搜索用户"""
        self.print_header("测试1: 搜索用户")
        
        url = f"{self.base_url}/_synapse/enhanced/friends/search"
        params = {"search_term": "test"}
        headers = {"Authorization": f"Bearer {self.user1_token}"}
        
        response = requests.get(url, params=params, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("搜索用户", status, f"状态码: {response.status_code} (期望: 200)")
        self.log_result("搜索用户", f"GET {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_get_friends(self):
        """测试2: 获取好友列表"""
        self.print_header("测试2: 获取好友列表")
        
        url = f"{self.base_url}/_synapse/enhanced/friends"
        headers = {"Authorization": f"Bearer {self.user1_token}"}
        
        response = requests.get(url, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("获取好友列表", status, f"状态码: {response.status_code} (期望: 200)")
        self.log_result("获取好友列表", f"GET {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_send_friend_request(self):
        """测试3: 发送好友请求"""
        self.print_header("测试3: 发送好友请求")
        
        url = f"{self.base_url}/_synapse/enhanced/friend/request"
        headers = {"Authorization": f"Bearer {self.user1_token}"}
        data = {
            "user_id": self.user2_id,
            "message": "我想加你为好友"
        }
        
        response = requests.post(url, json=data, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("发送好友请求", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:100]}")
        self.log_result("发送好友请求", f"POST {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_get_friend_requests(self):
        """测试4: 获取好友请求"""
        self.print_header("测试4: 获取好友请求")
        
        url = f"{self.base_url}/_synapse/enhanced/friend/requests"
        headers = {"Authorization": f"Bearer {self.user2_token}"}
        
        response = requests.get(url, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("获取好友请求", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:100]}")
        self.log_result("获取好友请求", f"GET {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_accept_friend_request(self):
        """测试5: 接受好友请求"""
        self.print_header("测试5: 接受好友请求")
        
        request_id = 1
        url = f"{self.base_url}/_synapse/enhanced/friend/request/{request_id}/accept"
        headers = {"Authorization": f"Bearer {self.user2_token}"}
        
        response = requests.post(url, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("接受好友请求", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:100]}")
        self.log_result("接受好友请求", f"POST {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_block_user(self):
        """测试6: 阻止用户"""
        self.print_header("测试6: 阻止用户")
        
        url = f"{self.base_url}/_synapse/enhanced/friend/blocks/{self.user1_id}"
        headers = {"Authorization": f"Bearer {self.user1_token}"}
        data = {
            "user_id": self.user3_id,
            "reason": "测试阻止"
        }
        
        response = requests.post(url, json=data, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("阻止用户", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:100]}")
        else:
            print(f"  错误: {response.text[:200]}")
        self.log_result("阻止用户", f"POST {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_get_blocked_users(self):
        """测试7: 获取阻止用户列表"""
        self.print_header("测试7: 获取阻止用户列表")
        
        url = f"{self.base_url}/_synapse/enhanced/friend/blocks/{self.user1_id}"
        headers = {"Authorization": f"Bearer {self.user1_token}"}
        
        response = requests.get(url, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("获取阻止用户列表", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:100]}")
        else:
            print(f"  错误: {response.text[:200]}")
        self.log_result("获取阻止用户列表", f"GET {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_unblock_user(self):
        """测试8: 解除阻止用户"""
        self.print_header("测试8: 解除阻止用户")
        
        url = f"{self.base_url}/_synapse/enhanced/friend/blocks/{self.user1_id}/{self.user3_id}"
        headers = {"Authorization": f"Bearer {self.user1_token}"}
        
        response = requests.delete(url, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("解除阻止用户", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:100]}")
        else:
            print(f"  错误: {response.text[:200]}")
        self.log_result("解除阻止用户", f"DELETE {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_create_friend_category(self):
        """测试9: 创建好友分类"""
        self.print_header("测试9: 创建好友分类")
        
        url = f"{self.base_url}/_synapse/enhanced/friend/categories/{self.user1_id}"
        headers = {"Authorization": f"Bearer {self.user1_token}"}
        data = {
            "name": "测试分类",
            "color": "#FF0000"
        }
        
        response = requests.post(url, json=data, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("创建好友分类", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:100]}")
        self.log_result("创建好友分类", f"POST {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_get_friend_categories(self):
        """测试10: 获取好友分类"""
        self.print_header("测试10: 获取好友分类")
        
        url = f"{self.base_url}/_synapse/enhanced/friend/categories/{self.user1_id}"
        headers = {"Authorization": f"Bearer {self.user1_token}"}
        
        response = requests.get(url, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("获取好友分类", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:100]}")
        self.log_result("获取好友分类", f"GET {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_update_friend_category(self):
        """测试11: 更新好友分类"""
        self.print_header("测试11: 更新好友分类")
        
        url = f"{self.base_url}/_synapse/enhanced/friend/categories/{self.user1_id}/测试分类"
        headers = {"Authorization": f"Bearer {self.user1_token}"}
        data = {
            "color": "#00FF00"
        }
        
        response = requests.put(url, json=data, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("更新好友分类", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:100]}")
        self.log_result("更新好友分类", f"PUT {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_delete_friend_category(self):
        """测试12: 删除好友分类"""
        self.print_header("测试12: 删除好友分类")
        
        url = f"{self.base_url}/_synapse/enhanced/friend/categories/{self.user1_id}/测试分类"
        headers = {"Authorization": f"Bearer {self.user1_token}"}
        
        response = requests.delete(url, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("删除好友分类", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:100]}")
        self.log_result("删除好友分类", f"DELETE {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_decline_friend_request(self):
        """测试13: 拒绝好友请求"""
        self.print_header("测试13: 拒绝好友请求")
        
        request_id = 2
        url = f"{self.base_url}/_synapse/enhanced/friend/request/{request_id}/decline"
        headers = {"Authorization": f"Bearer {self.user2_token}"}
        
        response = requests.post(url, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("拒绝好友请求", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:100]}")
        self.log_result("拒绝好友请求", f"POST {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def run_all_tests(self):
        """运行所有测试"""
        self.print_header("好友系统API完整测试")
        
        if not self.setup_test_users():
            print("❌ 测试用户设置失败")
            return False
            
        print(f"\n用户1 ID: {self.user1_id}")
        print(f"用户2 ID: {self.user2_id}")
        print(f"用户3 ID: {self.user3_id}")
        
        tests = [
            ("搜索用户", self.test_search_users),
            ("获取好友列表", self.test_get_friends),
            ("发送好友请求", self.test_send_friend_request),
            ("获取好友请求", self.test_get_friend_requests),
            ("接受好友请求", self.test_accept_friend_request),
            ("阻止用户", self.test_block_user),
            ("获取阻止用户列表", self.test_get_blocked_users),
            ("解除阻止用户", self.test_unblock_user),
            ("创建好友分类", self.test_create_friend_category),
            ("获取好友分类", self.test_get_friend_categories),
            ("更新好友分类", self.test_update_friend_category),
            ("删除好友分类", self.test_delete_friend_category),
            ("拒绝好友请求", self.test_decline_friend_request),
        ]
        
        passed = 0
        failed = 0
        
        for test_name, test_func in tests:
            try:
                if test_func():
                    passed += 1
                else:
                    failed += 1
            except Exception as e:
                print(f"❌ {test_name} 测试出错: {str(e)}")
                failed += 1
                
        self.print_summary(passed, failed, len(tests))
        self.save_report()
        
        return failed == 0
        
    def print_summary(self, passed, failed, total):
        """打印测试摘要"""
        self.print_header("测试报告汇总")
        
        print(f"\n总测试数: {total}")
        print(f"通过: {passed}")
        print(f"失败: {failed}")
        print(f"成功率: {(passed/total*100):.1f}%")
        
        print("\n详细结果:")
        print("-" * 80)
        for i, result in enumerate(self.test_results, 1):
            status_symbol = "✓" if result["status"] == "PASS" else "✗"
            print(f"{i}. {result['test_name']}: {status_symbol} {result['status']}")
            print(f"   端点: {result['endpoint']}")
            print(f"   状态码: {result['actual_code']} (期望: {result['expected_code']})")
            if result['details']:
                print(f"   详情: {result['details']}")
            print()
            
    def save_report(self):
        """保存测试报告"""
        report = {
            "timestamp": datetime.now().isoformat(),
            "total_tests": len(self.test_results),
            "passed": sum(1 for r in self.test_results if r["status"] == "PASS"),
            "failed": sum(1 for r in self.test_results if r["status"] == "FAIL"),
            "results": self.test_results
        }
        
        report_path = "/home/hula/synapse_rust/friend_api_test_report.json"
        with open(report_path, "w", encoding="utf-8") as f:
            json.dump(report, f, indent=2, ensure_ascii=False)
            
        print(f"\n测试报告已保存到: {report_path}")
        
if __name__ == "__main__":
    tester = FriendAPITester()
    success = tester.run_all_tests()
    sys.exit(0 if success else 1)