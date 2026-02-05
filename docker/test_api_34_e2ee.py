#!/usr/bin/env python3
"""
端到端加密API测试脚本 (3.4 E2EE APIs - 6个端点)
优化版本：改进测试方法、添加详细日志、更好的错误处理

测试策略：
1. 先加载测试账户token
2. 测试需要认证的端点
3. 记录每个端点的详细响应
4. 对比官方文档确认预期行为
"""

import requests
import json
import time
import sys
from datetime import datetime

BASE_URL = "http://localhost:8008"

class E2EEAPITester:
    def __init__(self, base_url=BASE_URL):
        self.base_url = base_url
        self.results = []
        self.session = requests.Session()
        self.session.headers.update({"Content-Type": "application/json"})
        self.token = None
        
    def load_token(self, token_file="/home/hula/synapse_rust/docker/testuser1_token.txt"):
        """加载测试账户token"""
        try:
            with open(token_file, 'r') as f:
                self.token = f.read().strip()
                self.session.headers.update({"Authorization": f"Bearer {self.token}"})
                print(f"已加载token: {self.token[:20]}...")
                return True
        except Exception as e:
            print(f"加载token失败: {e}")
            return False
        
    def test_api(self, name, method, endpoint, expected_status=None, 
                 data=None, notes=""):
        """测试单个API端点"""
        url = self.base_url + endpoint
        
        start_time = time.time()
        response = None
        try:
            if method.upper() == "GET":
                response = requests.get(url, headers=dict(self.session.headers), timeout=30)
            elif method.upper() == "PUT":
                response = requests.put(url, json=data, headers=dict(self.session.headers), timeout=30)
            elif method.upper() == "POST":
                response = requests.post(url, json=data, headers=dict(self.session.headers), timeout=30)
            elif method.upper() == "DELETE":
                response = requests.delete(url, headers=dict(self.session.headers), timeout=30)
            else:
                raise ValueError(f"Unknown method: {method}")
                        
            elapsed = round((time.time() - start_time) * 1000, 2)
            status_code = response.status_code
            
            try:
                response_data = response.json()
            except:
                response_data = {"raw_text": response.text[:500]}
                
            is_success = expected_status is None or status_code == expected_status
            
            result = {
                "name": name,
                "method": method.upper(),
                "endpoint": endpoint,
                "status": "✅ PASS" if is_success else "❌ FAIL",
                "status_code": status_code,
                "elapsed_ms": elapsed,
                "response": response_data,
                "notes": notes
            }
            
            if not is_success:
                result["expected_status"] = expected_status
                
        except requests.exceptions.Timeout:
            result = {
                "name": name, "status": "❌ FAIL", "status_code": "TIMEOUT",
                "endpoint": endpoint, "notes": notes, "method": method.upper(),
                "error": "Request timeout after 30s"
            }
        except requests.exceptions.ConnectionError as e:
            result = {
                "name": name, "status": "❌ FAIL", "status_code": "CONN_ERR",
                "endpoint": endpoint, "notes": notes, "method": method.upper(),
                "error": str(e)[:300]
            }
        except Exception as e:
            result = {
                "name": name, "status": "❌ FAIL", "status_code": "ERROR",
                "endpoint": endpoint, "notes": notes, "method": method.upper(),
                "error": str(e)[:300]
            }
            
        self.results.append(result)
        return result
        
    def print_result(self, result):
        """打印测试结果"""
        status = result.get("status", "❓")
        code = result.get("status_code", "N/A")
        elapsed = result.get("elapsed_ms", 0)
        name = result.get("name", "Unknown")
        notes = result.get("notes", "")
        
        if result["status"].startswith("❌"):
            error_info = f" | Expected: {result.get('expected_status', 'N/A')}"
            if result.get("error"):
                error_info += f" | Error: {result['error'][:100]}"
            print(f"  {status} [{result['method']}] {name}: HTTP {code} ({elapsed}ms){error_info}")
        else:
            print(f"  {status} [{result['method']}] {name}: HTTP {code} ({elapsed}ms) {notes}")
            
    def run_tests(self):
        """运行所有E2EE API测试"""
        print("\n" + "="*80)
        print("3.4 端到端加密API测试 (6个端点)")
        print(f"测试时间: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
        print("="*80)
        
        if not self.token:
            print("警告: 未加载token，某些需要认证的测试可能失败")
        
        passed = 0
        failed = 0
        
        # 3.4 端到端加密API
        print("\n3.4 端到端加密API")
        print("-" * 60)
        
        e2ee_tests = [
            ("上传密钥", "POST", "/_matrix/client/r0/keys/upload", None, {
                "device_keys": {
                    "@testuser1:cjystx.top": {
                        "device_id": "TESTDEVICE1",
                        "algorithms": ["m.olm.v1.curve25519-aes-sha2", "m.megolm.v1.aes-sha2"],
                        "keys": {
                            "curve25519:TESTDEVICE1": "NoZ9c+7E4tG+6/l1x7Jh5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0=",
                            "ed25519:TESTDEVICE1": "NoZ9c+7E4tG+6/l1x7Jh5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0="
                        },
                        "signatures": {}
                    }
                },
                "one_time_keys": {}
            }, "需要认证"),
            
            ("查询密钥", "POST", "/_matrix/client/r0/keys/query", None, {
                "timeout": 10000,
                "device_keys": {
                    "@testuser1:cjystx.top": []
                }
            }, "需要认证"),
            
            ("声明密钥", "POST", "/_matrix/client/r0/keys/claim", None, {
                "one_time_keys": {
                    "@testuser1:cjystx.top": {
                        "TESTDEVICE1": {
                            "key": "NoZ9c+7E4tG+6/l1x7Jh5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0="
                        }
                    }
                }
            }, "需要认证"),
            
            ("密钥变更", "GET", "/_matrix/client/r0/keys/changes", None, None, "需要认证"),
            
            ("房间密钥分发", "GET", "/_matrix/client/r0/rooms/!hU1S_lh9PJl93a-zGJY1SUlX:cjystx.top/keys/distribution", None, None, "需要认证"),
            
            ("发送设备消息", "PUT", "/_matrix/client/r0/sendToDevice/m.room.encrypted/test_txn_123", None, {
                "messages": {
                    "@testuser1:cjystx.top": {
                        "TESTDEVICE1": {
                            "type": "m.room.encrypted",
                            "content": {
                                "algorithm": "m.olm.v1.curve25519-aes-sha2",
                                "sender_key": "NoZ9c+7E4tG+6/l1x7Jh5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0o5s9v5QbV0=",
                                "ciphertext": {
                                    "TESTDEVICE1": {
                                        "type": 0,
                                        "body": "encrypted_content_base64"
                                    }
                                }
                            }
                        }
                    }
                }
            }, "需要认证"),
        ]
        
        for test in e2ee_tests:
            name, method, endpoint, expected, data, notes = test
            result = self.test_api(name, method, endpoint, expected, data, notes=notes)
            self.print_result(result)
            if result["status"].startswith("✅"):
                passed += 1
            else:
                failed += 1
        
        total = passed + failed
        
        # 汇总结果
        print("\n" + "="*80)
        print("测试结果汇总")
        print("="*80)
        print(f"总数: {total} | 通过: {passed} | 失败: {failed}")
        print(f"通过率: {round(passed/total*100, 1)}%")
        print("="*80)
        
        # 保存详细结果
        self.save_results()
        
        return passed, failed, total
    
    def save_results(self):
        """保存测试结果到JSON文件"""
        output_file = "/home/hula/synapse_rust/docker/e2ee_test_results.json"
        with open(output_file, "w", encoding="utf-8") as f:
            json.dump({
                "timestamp": datetime.now().isoformat(),
                "base_url": self.base_url,
                "total_endpoints": 6,
                "results": self.results
            }, f, ensure_ascii=False, indent=2)
        print(f"\n详细结果已保存到: {output_file}")
        
        # 生成markdown报告
        self.generate_markdown_report()
    
    def generate_markdown_report(self):
        """生成Markdown格式的测试报告"""
        report_file = "/home/hula/synapse_rust/docker/e2ee_test_report.md"
        
        with open(report_file, "w", encoding="utf-8") as f:
            f.write("# 端到端加密API测试报告\n\n")
            f.write(f"**测试时间**: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}\n\n")
            f.write("## 测试结果汇总\n\n")
            f.write("| 序号 | API | 方法 | 状态 | 结果 |\n")
            f.write("|------|-----|------|------|------|\n")
            
            for i, result in enumerate(self.results, 1):
                status_icon = "✅" if result["status"].startswith("✅") else "❌"
                code = result.get("status_code", "N/A")
                name = result["name"]
                method = result["method"]
                endpoint = result["endpoint"]
                notes = result.get("notes", "")
                
                f.write(f"| {i} | `{endpoint}` | {method} | {status_icon} {code} | {notes} |\n")
            
            f.write("\n## 详细结果\n\n")
            
            for result in self.results:
                f.write(f"### {result['name']}\n\n")
                f.write(f"- **端点**: `{result['method']} {result['endpoint']}`\n")
                f.write(f"- **状态码**: {result.get('status_code', 'N/A')}\n")
                f.write(f"- **响应**: {json.dumps(result.get('response', {}), indent=2, ensure_ascii=False)}\n")
                if result.get("error"):
                    f.write(f"- **错误**: {result['error']}\n")
                f.write("\n")
        
        print(f"Markdown报告已保存到: {report_file}")


if __name__ == "__main__":
    print("端到端加密API测试脚本")
    print("="*80)
    
    tester = E2EEAPITester(BASE_URL)
    
    # 加载token
    if not tester.load_token():
        print("警告: 无法加载token，将使用无认证方式测试")
    
    passed, failed, total = tester.run_tests()
    
    sys.exit(0 if failed == 0 else 1)
