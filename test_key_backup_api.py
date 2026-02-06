#!/usr/bin/env python3
"""
密钥备份API完整测试脚本
Test script for Matrix Key Backup APIs

覆盖以下9个端点:
1. POST /_matrix/client/r0/room_keys/version - 创建备份
2. GET /_matrix/client/r0/room_keys/version/{version} - 获取备份
3. PUT /_matrix/client/r0/room_keys/version/{version} - 更新备份
4. DELETE /_matrix/client/r0/room_keys/version/{version} - 删除备份
5. GET /_matrix/client/r0/room_keys/{version} - 获取所有密钥
6. PUT /_matrix/client/r0/room_keys/{version} - 上传密钥
7. POST /_matrix/client/r0/room_keys/{version}/keys - 批量上传
8. GET /_matrix/client/r0/room_keys/{version}/keys/{room_id} - 获取房间密钥
9. GET /_matrix/client/r0/room_keys/{version}/keys/{room_id}/{session_id} - 获取会话密钥
"""

import json
import requests
import time
import sys
import os
from datetime import datetime

class KeyBackupAPITester:
    def __init__(self):
        self.base_url = "http://localhost:8008"
        self.test_user = "admin"
        self.test_password = "Wzc9890951!"
        self.user_id = None
        self.access_token = None
        self.session = requests.Session()
        self.session.headers.update({"Content-Type": "application/json"})
        self.backup_version = None
        self.room_id = "!test_room_key_backup:cjystx.top"
        self.session_id = "test_session_key_backup"
        self.test_results = []

    def print_header(self, text):
        print("\n" + "=" * 80)
        print(f"  {text}")
        print("=" * 80)
        
    def print_test(self, name, status, detail=""):
        symbol = "✓" if status == "PASS" else "✗" if status == "FAIL" else "○"
        print(f"{symbol} {name}")
        if detail:
            print(f"  {detail}")
            
    def log_result(self, test_name, endpoint, expected_code, actual_code, status, response_text):
        self.test_results.append({
            "timestamp": datetime.now().isoformat(),
            "test_name": test_name,
            "endpoint": endpoint,
            "expected_code": expected_code,
            "actual_code": actual_code,
            "status": status,
            "response": response_text[:500] if response_text else ""
        })
        
    def print_summary(self, passed, failed, skipped, total):
        self.print_header("测试报告汇总")
        print(f"总测试数: {total}")
        print(f"通过: {passed}")
        print(f"失败: {failed}")
        print(f"跳过: {skipped}")
        print(f"成功率: {passed/total*100:.1f}%")
        
        print("\n详细结果:")
        print("-" * 80)
        for i, result in enumerate(self.test_results, 1):
            symbol = "✓" if result["status"] == "PASS" else "✗" if result["status"] == "FAIL" else "○"
            print(f"{symbol} {result['test_name']}: {result['status']}")
            print(f"   端点: {result['endpoint']}")
            print(f"   状态码: {result['actual_code']} (期望: {result['expected_code']})")
            if result['response']:
                print(f"   响应: {result['response'][:100]}...")
            print()
            
    def save_report(self):
        report_file = "/home/hula/synapse_rust/key_backup_api_test_report.json"
        with open(report_file, 'w', encoding='utf-8') as f:
            json.dump({
                "test_time": datetime.now().isoformat(),
                "summary": {
                    "total": len(self.test_results),
                    "passed": sum(1 for r in self.test_results if r["status"] == "PASS"),
                    "failed": sum(1 for r in self.test_results if r["status"] == "FAIL"),
                    "skipped": sum(1 for r in self.test_results if r["status"] == "SKIP")
                },
                "results": self.test_results
            }, f, indent=2, ensure_ascii=False)
        print(f"\n测试报告已保存到: {report_file}")
        
    def setup_test_user(self):
        """设置测试用户 - 尝试登录或注册"""
        self.print_header("步骤1: 设置测试用户")
        
        login_url = f"{self.base_url}/_matrix/client/r0/login"
        login_data = {
            "type": "m.login.password",
            "user": self.test_user,
            "password": self.test_password
        }
        
        try:
            response = self.session.post(login_url, json=login_data)
            if response.status_code == 200:
                result = response.json()
                self.user_id = result.get("user_id")
                self.access_token = result.get("access_token")
                self.session.headers.update({"Authorization": f"Bearer {self.access_token}"})
                print(f"✓ 用户登录成功")
                print(f"  用户ID: {self.user_id}")
                return True
            else:
                print(f"✗ 用户登录失败: {response.text[:200]}")
                return False
        except Exception as e:
            print(f"✗ 连接失败: {str(e)}")
            return False
            
    def test_01_create_backup_version(self):
        """测试1: 创建备份版本"""
        self.print_header("测试1: 创建备份版本")
        
        url = f"{self.base_url}/_matrix/client/r0/room_keys/version"
        
        backup_data = {
            "auth_data": {
                "algorithm": "m.megolm_backup.v1",
                "signatures": {
                    f"{self.user_id}": {
                        "ed25519:test_key": "KYWxCiAiIGt0eXBlIjogIm0ubWVnb2xtLmJhY2t1cC52MSIKfQ"
                    }
                },
                "public_key": "OlLSl0lTlbSmEcNhP5tKcgrOlLSl0lTlbSmEcNhP5tKc"
            },
            "secret": "test_backup_secret_key_123456789"
        }
        
        response = self.session.post(url, json=backup_data)
        status = "PASS" if response.status_code in [200, 201] else "FAIL"
        
        self.print_test("创建备份版本", status, f"状态码: {response.status_code} (期望: 200/201)")
        print(f"  响应: {response.text[:300]}")
        
        if status == "PASS":
            try:
                result = response.json()
                self.backup_version = result.get("version")
                print(f"  备份版本: {self.backup_version}")
            except:
                pass
                
        self.log_result(
            "创建备份版本",
            f"POST {url}",
            200,
            response.status_code,
            status,
            response.text[:200]
        )
        
        return status == "PASS"
        
    def test_02_get_backup_version(self):
        """测试2: 获取备份版本"""
        self.print_header("测试2: 获取备份版本")
        
        if not self.backup_version:
            print("  跳过：没有可用的备份版本")
            self.log_result(
                "获取备份版本",
                "GET /_matrix/client/r0/room_keys/version/{version}",
                200,
                0,
                "SKIP",
                "No backup version available"
            )
            return False
            
        url = f"{self.base_url}/_matrix/client/r0/room_keys/version/{self.backup_version}"
        response = self.session.get(url)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("获取备份版本", status, f"状态码: {response.status_code} (期望: 200)")
        print(f"  响应: {response.text[:300]}")
        
        self.log_result(
            "获取备份版本",
            f"GET {url}",
            200,
            response.status_code,
            status,
            response.text[:200]
        )
        
        return status == "PASS"
        
    def test_03_update_backup_version(self):
        """测试3: 更新备份版本"""
        self.print_header("测试3: 更新备份版本")
        
        if not self.backup_version:
            print("  跳过：没有可用的备份版本")
            self.log_result(
                "更新备份版本",
                "PUT /_matrix/client/r0/room_keys/version/{version}",
                200,
                0,
                "SKIP",
                "No backup version available"
            )
            return False
            
        url = f"{self.base_url}/_matrix/client/r0/room_keys/version/{self.backup_version}"
        
        update_data = {
            "auth_data": {
                "algorithm": "m.megolm_backup.v1",
                "signatures": {
                    f"{self.user_id}": {
                        "ed25519:test_key": "updated_signature_here"
                    }
                },
                "public_key": "UpdatedPublicKeyHere123456"
            }
        }
        
        response = self.session.put(url, json=update_data)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("更新备份版本", status, f"状态码: {response.status_code} (期望: 200)")
        print(f"  响应: {response.text[:300]}")
        
        self.log_result(
            "更新备份版本",
            f"PUT {url}",
            200,
            response.status_code,
            status,
            response.text[:200]
        )
        
        return status == "PASS"
        
    def test_04_get_all_keys(self):
        """测试4: 获取所有密钥"""
        self.print_header("测试4: 获取所有密钥")
        
        if not self.backup_version:
            print("  跳过：没有可用的备份版本")
            self.log_result(
                "获取所有密钥",
                "GET /_matrix/client/r0/room_keys/{version}",
                200,
                0,
                "SKIP",
                "No backup version available"
            )
            return False
            
        url = f"{self.base_url}/_matrix/client/r0/room_keys/{self.backup_version}"
        response = self.session.get(url)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("获取所有密钥", status, f"状态码: {response.status_code} (期望: 200)")
        print(f"  响应: {response.text[:300]}")
        
        self.log_result(
            "获取所有密钥",
            f"GET {url}",
            200,
            response.status_code,
            status,
            response.text[:200]
        )
        
        return status == "PASS"
        
    def test_05_upload_room_key(self):
        """测试5: 上传房间密钥"""
        self.print_header("测试5: 上传房间密钥")
        
        if not self.backup_version:
            print("  跳过：没有可用的备份版本")
            self.log_result(
                "上传房间密钥",
                "PUT /_matrix/client/r0/room_keys/{version}",
                200,
                0,
                "SKIP",
                "No backup version available"
            )
            return False
            
        url = f"{self.base_url}/_matrix/client/r0/room_keys/{self.backup_version}"
        
        key_data = {
            "room_id": self.room_id,
            "sessions": [
                {
                    "session_id": self.session_id,
                    "first_message_index": 0,
                    "forwarded_count": 0,
                    "is_verified": True,
                    "session_data": {
                        "alg": "m.megolm.v1.curve25519-aes-sha2",
                        "ciphertext": "test_ciphertext_data_here",
                        "session_key": "AgAAAAgdW5vbgYGBwYGBwYGBwYGBwYGBwYGBwYGBwYGBwYGBwYGBwYGBwYGBwYGBwYGBwYGBwYGBwYGBwYGBwYGBw==",
                        "sender_claimed_ed25519_key": "test_ed25519_key",
                        "sender_key": "test_sender_curve25519_key"
                    }
                }
            ]
        }
        
        response = self.session.put(url, json=key_data)
        status = "PASS" if response.status_code in [200, 201] else "FAIL"
        
        self.print_test("上传房间密钥", status, f"状态码: {response.status_code} (期望: 200/201)")
        print(f"  响应: {response.text[:300]}")
        
        self.log_result(
            "上传房间密钥",
            f"PUT {url}",
            200,
            response.status_code,
            status,
            response.text[:200]
        )
        
        return status == "PASS"
        
    def test_06_batch_upload_keys(self):
        """测试6: 批量上传密钥"""
        self.print_header("测试6: 批量上传密钥")
        
        if not self.backup_version:
            print("  跳过：没有可用的备份版本")
            self.log_result(
                "批量上传密钥",
                "POST /_matrix/client/r0/room_keys/{version}/keys",
                200,
                0,
                "SKIP",
                "No backup version available"
            )
            return False
            
        url = f"{self.base_url}/_matrix/client/r0/room_keys/{self.backup_version}/keys"
        
        batch_keys = {
            "rooms": {
                self.room_id: {
                    "sessions": {
                        "session_001": {
                            "first_message_index": 0,
                            "forwarded_count": 0,
                            "is_verified": True,
                            "session_data": {
                                "alg": "m.megolm.v1.curve25519-aes-sha2",
                                "ciphertext": "batch_test_ciphertext",
                                "session_key": "BgcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBw==",
                                "sender_claimed_ed25519_key": "batch_ed25519_key",
                                "sender_key": "batch_sender_curve25519_key"
                            }
                        },
                        "session_002": {
                            "first_message_index": 1,
                            "forwarded_count": 1,
                            "is_verified": False,
                            "session_data": {
                                "alg": "m.megolm.v1.curve25519-aes-sha2",
                                "ciphertext": "batch_test_ciphertext_2",
                                "session_key": "CgcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBw==",
                                "sender_claimed_ed25519_key": "batch_ed25519_key_2",
                                "sender_key": "batch_sender_curve25519_key_2"
                            }
                        }
                    }
                }
            }
        }
        
        response = self.session.post(url, json=batch_keys)
        status = "PASS" if response.status_code in [200, 201] else "FAIL"
        
        self.print_test("批量上传密钥", status, f"状态码: {response.status_code} (期望: 200/201)")
        print(f"  响应: {response.text[:300]}")
        
        self.log_result(
            "批量上传密钥",
            f"POST {url}",
            200,
            response.status_code,
            status,
            response.text[:200]
        )
        
        return status == "PASS"
        
    def test_07_get_room_key(self):
        """测试7: 获取房间密钥"""
        self.print_header("测试7: 获取房间密钥")
        
        if not self.backup_version:
            print("  跳过：没有可用的备份版本")
            self.log_result(
                "获取房间密钥",
                "GET /_matrix/client/r0/room_keys/{version}/keys/{room_id}",
                200,
                0,
                "SKIP",
                "No backup version available"
            )
            return False
            
        url = f"{self.base_url}/_matrix/client/r0/room_keys/{self.backup_version}/keys/{self.room_id}"
        response = self.session.get(url)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("获取房间密钥", status, f"状态码: {response.status_code} (期望: 200)")
        print(f"  响应: {response.text[:400]}")
        
        self.log_result(
            "获取房间密钥",
            f"GET {url}",
            200,
            response.status_code,
            status,
            response.text[:200]
        )
        
        return status == "PASS"
        
    def test_08_get_session_key(self):
        """测试8: 获取会话密钥"""
        self.print_header("测试8: 获取会话密钥")
        
        if not self.backup_version:
            print("  跳过：没有可用的备份版本")
            self.log_result(
                "获取会话密钥",
                "GET /_matrix/client/r0/room_keys/{version}/keys/{room_id}/{session_id}",
                200,
                0,
                "SKIP",
                "No backup version available"
            )
            return False
            
        url = f"{self.base_url}/_matrix/client/r0/room_keys/{self.backup_version}/keys/{self.room_id}/{self.session_id}"
        response = self.session.get(url)
        status = "PASS" if response.status_code == 200 else "FAIL"
        
        self.print_test("获取会话密钥", status, f"状态码: {response.status_code} (期望: 200)")
        print(f"  响应: {response.text[:400]}")
        
        self.log_result(
            "获取会话密钥",
            f"GET {url}",
            200,
            response.status_code,
            status,
            response.text[:200]
        )
        
        return status == "PASS"
        
    def test_09_delete_backup_version(self):
        """测试9: 删除备份版本"""
        self.print_header("测试9: 删除备份版本")
        
        if not self.backup_version:
            print("  跳过：没有可用的备份版本")
            self.log_result(
                "删除备份版本",
                "DELETE /_matrix/client/r0/room_keys/version/{version}",
                200,
                0,
                "SKIP",
                "No backup version available"
            )
            return False
            
        url = f"{self.base_url}/_matrix/client/r0/room_keys/version/{self.backup_version}"
        response = self.session.delete(url)
        status = "PASS" if response.status_code in [200, 201, 204] else "FAIL"
        
        self.print_test("删除备份版本", status, f"状态码: {response.status_code} (期望: 200/201/204)")
        print(f"  响应: {response.text[:300]}")
        
        if status == "PASS":
            self.backup_version = None
            
        self.log_result(
            "删除备份版本",
            f"DELETE {url}",
            200,
            response.status_code,
            status,
            response.text[:200]
        )
        
        return status == "PASS"
        
    def run_all_tests(self):
        """运行所有测试"""
        self.print_header("密钥备份API完整测试")
        
        if not self.setup_test_user():
            print("❌ 测试用户设置失败")
            return False
            
        print(f"\n测试用户: {self.user_id}")
        
        tests = [
            ("创建备份版本", self.test_01_create_backup_version),
            ("获取备份版本", self.test_02_get_backup_version),
            ("更新备份版本", self.test_03_update_backup_version),
            ("获取所有密钥", self.test_04_get_all_keys),
            ("上传房间密钥", self.test_05_upload_room_key),
            ("批量上传密钥", self.test_06_batch_upload_keys),
            ("获取房间密钥", self.test_07_get_room_key),
            ("获取会话密钥", self.test_08_get_session_key),
            ("删除备份版本", self.test_09_delete_backup_version),
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

if __name__ == "__main__":
    tester = KeyBackupAPITester()
    success = tester.run_all_tests()
    sys.exit(0 if success else 1)
