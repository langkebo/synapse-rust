#!/usr/bin/env python3
"""
媒体文件API完整测试脚本
测试所有媒体文件API端点，使用有效的token
"""

import requests
import json
import sys
import base64
from datetime import datetime

BASE_URL = "http://localhost:8008"

class MediaAPITester:
    def __init__(self):
        self.base_url = BASE_URL
        self.user_token = None
        self.user_id = None
        self.test_results = []
        self.uploaded_media_id = None
        self.server_name = "cjystx.top"
        
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
        
    def setup_test_user(self):
        """设置测试用户"""
        self.print_header("设置测试用户")
        
        timestamp = datetime.now().strftime("%Y%m%d%H%M%S")
        user_name = f"media_test_{timestamp}"
        
        print(f"创建测试用户: {user_name}")
        user = self.register_user(user_name, "TestUser123!")
        if user:
            self.user_id = user.get("user_id")
            print(f"  用户ID: {self.user_id}")
        else:
            print(f"  注册失败，尝试登录")
            login = self.login_user(user_name, "TestUser123!")
            if login:
                self.user_id = login.get("user_id")
                self.user_token = login.get("access_token")
                print(f"  用户ID: {self.user_id}")
        
        if not self.user_token:
            login = self.login_user(user_name, "TestUser123!")
            if login:
                self.user_token = login.get("access_token")
        
        print(f"\n用户Token: {self.user_token[:20]}..." if self.user_token else "用户Token: None")
        
        return bool(self.user_token)
        
    def test_upload_media_v3(self):
        """测试1: 上传媒体文件 (v3)"""
        self.print_header("测试1: 上传媒体文件 (v3)")
        
        url = f"{self.base_url}/_matrix/media/v3/upload"
        headers = {"Authorization": f"Bearer {self.user_token}"}
        
        test_content = b"test image content for media upload"
        content_b64 = base64.b64encode(test_content).decode('utf-8')
        
        data = {
            "content": content_b64,
            "content_type": "image/jpeg",
            "filename": "test_image.jpg"
        }
        
        response = requests.post(url, json=data, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("上传媒体文件 (v3)", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:200]}")
            try:
                result = response.json()
                self.uploaded_media_id = result.get("media_id")
                print(f"  媒体ID: {self.uploaded_media_id}")
            except:
                pass
        else:
            print(f"  错误: {response.text[:200]}")
        
        self.log_result("上传媒体文件 (v3)", f"POST {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_upload_media_v1(self):
        """测试2: 上传媒体文件 (v1)"""
        self.print_header("测试2: 上传媒体文件 (v1)")
        
        url = f"{self.base_url}/_matrix/media/v1/upload"
        headers = {"Authorization": f"Bearer {self.user_token}"}
        
        test_content = b"test image content for v1 upload"
        content_b64 = base64.b64encode(test_content).decode('utf-8')
        
        data = {
            "content": content_b64,
            "content_type": "image/png",
            "filename": "test_image.png"
        }
        
        response = requests.post(url, json=data, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("上传媒体文件 (v1)", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:200]}")
        else:
            print(f"  错误: {response.text[:200]}")
        
        self.log_result("上传媒体文件 (v1)", f"POST {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_download_media(self):
        """测试3: 下载媒体文件"""
        self.print_header("测试3: 下载媒体文件")
        
        if not self.uploaded_media_id:
            print("  跳过：没有可用的媒体ID")
            self.log_result("下载媒体文件", "GET /_matrix/media/v3/download", 200, 0, "SKIP", "No media ID available")
            return False
        
        url = f"{self.base_url}/_matrix/media/v3/download/{self.server_name}/{self.uploaded_media_id}"
        
        response = requests.get(url)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("下载媒体文件", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应长度: {len(response.content)} bytes")
        else:
            print(f"  错误: {response.text[:200]}")
        
        self.log_result("下载媒体文件", f"GET {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_download_media_v1(self):
        """测试4: 下载媒体文件 (v1)"""
        self.print_header("测试4: 下载媒体文件 (v1)")
        
        if not self.uploaded_media_id:
            print("  跳过：没有可用的媒体ID")
            self.log_result("下载媒体文件 (v1)", "GET /_matrix/media/v1/download", 200, 0, "SKIP", "No media ID available")
            return False
        
        url = f"{self.base_url}/_matrix/media/v1/download/{self.server_name}/{self.uploaded_media_id}"
        
        response = requests.get(url)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("下载媒体文件 (v1)", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应长度: {len(response.content)} bytes")
        else:
            print(f"  错误: {response.text[:200]}")
        
        self.log_result("下载媒体文件 (v1)", f"GET {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_get_thumbnail(self):
        """测试5: 获取缩略图"""
        self.print_header("测试5: 获取缩略图")
        
        if not self.uploaded_media_id:
            print("  跳过：没有可用的媒体ID")
            self.log_result("获取缩略图", "GET /_matrix/media/v3/thumbnail", 200, 0, "SKIP", "No media ID available")
            return False
        
        url = f"{self.base_url}/_matrix/media/v3/thumbnail/{self.server_name}/{self.uploaded_media_id}"
        params = {
            "width": 100,
            "height": 100,
            "method": "scale"
        }
        
        response = requests.get(url, params=params)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("获取缩略图", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应长度: {len(response.content)} bytes")
        else:
            print(f"  错误: {response.text[:200]}")
        
        self.log_result("获取缩略图", f"GET {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_media_config(self):
        """测试6: 获取媒体配置"""
        self.print_header("测试6: 获取媒体配置")
        
        url = f"{self.base_url}/_matrix/media/v1/config"
        
        response = requests.get(url)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("获取媒体配置", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:200]}")
        else:
            print(f"  错误: {response.text[:200]}")
        
        self.log_result("获取媒体配置", f"GET {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_upload_with_array_content(self):
        """测试7: 使用数组格式上传媒体"""
        self.print_header("测试7: 使用数组格式上传媒体")
        
        url = f"{self.base_url}/_matrix/media/v3/upload"
        headers = {"Authorization": f"Bearer {self.user_token}"}
        
        test_content = [ord(c) for c in "test content array format"]
        
        data = {
            "content": test_content,
            "content_type": "text/plain",
            "filename": "test.txt"
        }
        
        response = requests.post(url, json=data, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("使用数组格式上传媒体", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:200]}")
        else:
            print(f"  错误: {response.text[:200]}")
        
        self.log_result("使用数组格式上传媒体", f"POST {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def test_upload_without_filename(self):
        """测试8: 不带文件名上传媒体"""
        self.print_header("测试8: 不带文件名上传媒体")
        
        url = f"{self.base_url}/_matrix/media/v3/upload"
        headers = {"Authorization": f"Bearer {self.user_token}"}
        
        test_content = b"test content without filename"
        content_b64 = base64.b64encode(test_content).decode('utf-8')
        
        data = {
            "content": content_b64,
            "content_type": "application/octet-stream"
        }
        
        response = requests.post(url, json=data, headers=headers)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("不带文件名上传媒体", status, f"状态码: {response.status_code} (期望: 200)")
        if status == "PASS":
            print(f"  响应: {response.text[:200]}")
        else:
            print(f"  错误: {response.text[:200]}")
        
        self.log_result("不带文件名上传媒体", f"POST {url}", 200, response.status_code, status, response.text[:100])
        
        return status == "PASS"
        
    def run_all_tests(self):
        """运行所有测试"""
        self.print_header("媒体文件API完整测试")
        
        if not self.setup_test_user():
            print("❌ 测试用户设置失败")
            return False
            
        print(f"\n用户ID: {self.user_id}")
        print(f"服务器名称: {self.server_name}")
        
        tests = [
            ("上传媒体文件 (v3)", self.test_upload_media_v3),
            ("上传媒体文件 (v1)", self.test_upload_media_v1),
            ("下载媒体文件", self.test_download_media),
            ("下载媒体文件 (v1)", self.test_download_media_v1),
            ("获取缩略图", self.test_get_thumbnail),
            ("获取媒体配置", self.test_media_config),
            ("使用数组格式上传媒体", self.test_upload_with_array_content),
            ("不带文件名上传媒体", self.test_upload_without_filename),
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
        
        report_path = "/home/hula/synapse_rust/media_api_test_report.json"
        with open(report_path, "w", encoding="utf-8") as f:
            json.dump(report, f, indent=2, ensure_ascii=False)
            
        print(f"\n测试报告已保存到: {report_path}")
        
if __name__ == "__main__":
    tester = MediaAPITester()
    success = tester.run_all_tests()
    sys.exit(0 if success else 1)