#!/usr/bin/env python3
"""
私聊增强API完整测试脚本
测试所有私聊增强API端点，使用有效的token
"""

import requests
import json
import sys
from datetime import datetime

BASE_URL = "http://localhost:8008"

class PrivateChatAPITester:
    def __init__(self):
        self.base_url = BASE_URL
        self.user1_token = None
        self.user1_id = None
        self.user2_token = None
        self.user2_id = None
        self.test_results = []
        self.session_id = None
        self.message_id = None
        
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
        user1_name = f"private_test_1_{timestamp}"
        user2_name = f"private_test_2_{timestamp}"
        
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
        
        if not self.user1_token:
            login1 = self.login_user(user1_name, "TestUser123!")
            if login1:
                self.user1_token = login1.get("access_token")
                
        if not self.user2_token:
            login2 = self.login_user(user2_name, "TestUser123!")
            if login2:
                self.user2_token = login2.get("access_token")
        
        print(f"\n用户1 Token: {self.user1_token[:20]}..." if self.user1_token else "用户1 Token: None")
        print(f"用户2 Token: {self.user2_token[:20]}..." if self.user2_token else "用户2 Token: None")
        
        return bool(self.user1_token and self.user2_token)
        
    def test_create_session(self):
        """测试1: 创建私聊会话"""
        self.print_header("测试1: 创建私聊会话")
        
        url = f"{self.base_url}/_synapse/enhanced/private/sessions"
        headers = {"Authorization": f"Bearer {self.user1_token}"}
        data = {
            "other_user_id": self.user2_id
        }
        
        response = requests.post(url, json=data, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("创建私聊会话", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:200]}")
            try:
                result = response.json()
                self.session_id = result.get("session_id")
                print(f"  会话ID: {self.session_id}")
            except:
                pass
        else:
            print(f"  错误: {response.text[:200]}")
        
        self.log_result("创建私聊会话", f"POST {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_create_dm_room(self):
        """测试9: 创建DM房间"""
        self.print_header("测试9: 创建DM房间")
        
        url = f"{self.base_url}/_matrix/client/r0/createDM"
        headers = {"Authorization": f"Bearer {self.user1_token}"}
        data = {
            "user_id": self.user2_id
        }
        
        response = requests.post(url, json=data, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("创建DM房间", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:200]}")
            try:
                result = response.json()
                if not self.session_id:
                    self.session_id = result.get("room_id")
                    print(f"  会话ID: {self.session_id}")
            except:
                pass
        else:
            print(f"  错误: {response.text[:200]}")
        
        self.log_result("创建DM房间", f"POST {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_get_sessions(self):
        """测试2: 获取私聊会话列表"""
        self.print_header("测试2: 获取私聊会话列表")
        
        url = f"{self.base_url}/_synapse/enhanced/private/sessions"
        headers = {"Authorization": f"Bearer {self.user1_token}"}
        
        response = requests.get(url, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("获取私聊会话列表", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:200]}")
            try:
                result = response.json()
                sessions = result.get("sessions", [])
                if sessions and not self.session_id:
                    self.session_id = sessions[0].get("session_id")
                    print(f"  自动获取会话ID: {self.session_id}")
            except:
                pass
        else:
            print(f"  错误: {response.text[:200]}")
        
        self.log_result("获取私聊会话列表", f"GET {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_get_session_details(self):
        """测试3: 获取会话详情"""
        self.print_header("测试3: 获取会话详情")
        
        if not self.session_id:
            print("  跳过：没有可用的会话ID")
            self.log_result("获取会话详情", "GET /_synapse/enhanced/private/sessions/{id}", 200, 0, "SKIP", "No session ID available")
            return False
        
        url = f"{self.base_url}/_synapse/enhanced/private/sessions/{self.session_id}"
        headers = {"Authorization": f"Bearer {self.user1_token}"}
        
        response = requests.get(url, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("获取会话详情", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:200]}")
        else:
            print(f"  错误: {response.text[:200]}")
        
        self.log_result("获取会话详情", f"GET {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_send_message(self):
        """测试4: 发送会话消息"""
        self.print_header("测试4: 发送会话消息")
        
        if not self.session_id:
            print("  跳过：没有可用的会话ID")
            self.log_result("发送会话消息", "POST /_synapse/enhanced/private/sessions/{id}/messages", 200, 0, "SKIP", "No session ID available")
            return False
        
        url = f"{self.base_url}/_synapse/enhanced/private/sessions/{self.session_id}/messages"
        headers = {"Authorization": f"Bearer {self.user1_token}"}
        data = {
            "message_type": "m.text",
            "content": {
                "body": "Hello, this is a test message!",
                "msgtype": "m.text"
            }
        }
        
        response = requests.post(url, json=data, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("发送会话消息", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:200]}")
            try:
                result = response.json()
                self.message_id = result.get("message_id")
                print(f"  消息ID: {self.message_id}")
            except:
                pass
        else:
            print(f"  错误: {response.text[:200]}")
        
        self.log_result("发送会话消息", f"POST {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_get_session_messages(self):
        """测试5: 获取会话消息"""
        self.print_header("测试5: 获取会话消息")
        
        if not self.session_id:
            print("  跳过：没有可用的会话ID")
            self.log_result("获取会话消息", "GET /_synapse/enhanced/private/sessions/{id}/messages", 200, 0, "SKIP", "No session ID available")
            return False
        
        url = f"{self.base_url}/_synapse/enhanced/private/sessions/{self.session_id}/messages"
        headers = {"Authorization": f"Bearer {self.user1_token}"}
        
        response = requests.get(url, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("获取会话消息", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:200]}")
        else:
            print(f"  错误: {response.text[:200]}")
        
        self.log_result("获取会话消息", f"GET {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_delete_session(self):
        """测试6: 删除会话"""
        self.print_header("测试6: 删除会话")
        
        if not self.session_id:
            print("  跳过：没有可用的会话ID")
            self.log_result("删除会话", "DELETE /_synapse/enhanced/private/sessions/{id}", 200, 0, "SKIP", "No session ID available")
            return False
        
        url = f"{self.base_url}/_synapse/enhanced/private/sessions/{self.session_id}"
        headers = {"Authorization": f"Bearer {self.user1_token}"}
        
        response = requests.delete(url, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("删除会话", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:200]}")
        else:
            print(f"  错误: {response.text[:200]}")
        
        self.log_result("删除会话", f"DELETE {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_get_unread_count(self):
        """测试7: 获取未读消息数"""
        self.print_header("测试7: 获取未读消息数")
        
        url = f"{self.base_url}/_synapse/enhanced/private/unread-count"
        headers = {"Authorization": f"Bearer {self.user1_token}"}
        
        response = requests.get(url, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("获取未读消息数", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:200]}")
        else:
            print(f"  错误: {response.text[:200]}")
        
        self.log_result("获取未读消息数", f"GET {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_search_messages(self):
        """测试8: 搜索消息"""
        self.print_header("测试8: 搜索消息")
        
        url = f"{self.base_url}/_synapse/enhanced/private/search"
        headers = {"Authorization": f"Bearer {self.user1_token}"}
        data = {
            "query": "test",
            "limit": 10
        }
        
        response = requests.post(url, json=data, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("搜索消息", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:200]}")
        else:
            print(f"  错误: {response.text[:200]}")
        
        self.log_result("搜索消息", f"POST {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_create_dm_room(self):
        """测试9: 创建DM房间"""
        self.print_header("测试9: 创建DM房间")
        
        url = f"{self.base_url}/_matrix/client/r0/createDM"
        headers = {"Authorization": f"Bearer {self.user1_token}"}
        data = {
            "user_id": self.user2_id
        }
        
        response = requests.post(url, json=data, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("创建DM房间", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:200]}")
        else:
            print(f"  错误: {response.text[:200]}")
        
        self.log_result("创建DM房间", f"POST {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_get_dm_rooms(self):
        """测试10: 获取DM房间列表"""
        self.print_header("测试10: 获取DM房间列表")
        
        url = f"{self.base_url}/_matrix/client/r0/dm"
        headers = {"Authorization": f"Bearer {self.user1_token}"}
        
        response = requests.get(url, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("获取DM房间列表", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:200]}")
        else:
            print(f"  错误: {response.text[:200]}")
        
        self.log_result("获取DM房间列表", f"GET {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def run_all_tests(self):
        """运行所有测试"""
        self.print_header("私聊增强API完整测试")
        
        if not self.setup_test_users():
            print("❌ 测试用户设置失败")
            return False
            
        print(f"\n用户1 ID: {self.user1_id}")
        print(f"用户2 ID: {self.user2_id}")
        
        tests = [
            ("创建DM房间", self.test_create_dm_room),
            ("获取私聊会话列表", self.test_get_sessions),
            ("获取会话详情", self.test_get_session_details),
            ("发送会话消息", self.test_send_message),
            ("获取会话消息", self.test_get_session_messages),
            ("删除会话", self.test_delete_session),
            ("获取未读消息数", self.test_get_unread_count),
            ("搜索消息", self.test_search_messages),
            ("获取DM房间列表", self.test_get_dm_rooms),
        ]
        
        passed = 0
        failed = 0
        skipped = 0
        
        for test_name, test_func in tests:
            try:
                result = test_func()
                if result is None:
                    skipped += 1
                elif result:
                    passed += 1
                else:
                    failed += 1
            except Exception as e:
                print(f"❌ {test_name} 测试出错: {str(e)}")
                failed += 1
                
        self.print_summary(passed, failed, skipped, len(tests))
        self.save_report()
        
        return failed == 0
        
    def print_summary(self, passed, failed, skipped, total):
        """打印测试摘要"""
        self.print_header("测试报告汇总")
        
        print(f"\n总测试数: {total}")
        print(f"通过: {passed}")
        print(f"失败: {failed}")
        print(f"跳过: {skipped}")
        if total - skipped > 0:
            print(f"成功率: {(passed/(total-skipped)*100):.1f}%")
        
        print("\n详细结果:")
        print("-" * 80)
        for i, result in enumerate(self.test_results, 1):
            if result["status"] == "SKIP":
                status_symbol = "○"
            else:
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
            "skipped": sum(1 for r in self.test_results if r["status"] == "SKIP"),
            "results": self.test_results
        }
        
        report_path = "/home/hula/synapse_rust/private_chat_api_test_report.json"
        with open(report_path, "w", encoding="utf-8") as f:
            json.dump(report, f, indent=2, ensure_ascii=False)
            
        print(f"\n测试报告已保存到: {report_path}")
        
if __name__ == "__main__":
    tester = PrivateChatAPITester()
    success = tester.run_all_tests()
    sys.exit(0 if success else 1)