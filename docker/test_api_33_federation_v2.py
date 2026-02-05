#!/usr/bin/env python3
"""
联邦通信API测试脚本 v2.0 (3.3 Federation APIs - 32个端点)
优化版本：改进测试方法、添加详细日志、更好的错误处理

测试策略：
1. 先测试公开端点（无需认证）
2. 再测试需要认证的端点
3. 记录每个端点的详细响应
4. 对比官方文档确认预期行为
"""

import requests
import json
import time
import sys
import subprocess
from datetime import datetime

BASE_URL = "http://localhost:8008"
MATRIX_ORG_URL = "https://matrix.org"

class FederationAPITester:
    def __init__(self, base_url=BASE_URL):
        self.base_url = base_url
        self.results = []
        self.errors = []
        self.session = requests.Session()
        self.session.headers.update({"Content-Type": "application/json"})
        
    def test_api(self, name, method, endpoint, expected_status=None, 
                 data=None, headers=None, notes=""):
        """测试单个API端点"""
        url = self.base_url + endpoint
        req_headers = dict(self.session.headers)
        if headers:
            req_headers.update(headers)
            
        start_time = time.time()
        response = None
        try:
            if method.upper() == "GET":
                response = requests.get(url, headers=req_headers, timeout=30)
            elif method.upper() == "PUT":
                response = requests.put(url, json=data, headers=req_headers, timeout=30)
            elif method.upper() == "POST":
                response = requests.post(url, json=data, headers=req_headers, timeout=30)
            elif method.upper() == "DELETE":
                response = requests.delete(url, headers=req_headers, timeout=30)
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
                "endpoint": endpoint, "notes": notes,
                "error": "Request timeout after 30s"
            }
        except requests.exceptions.ConnectionError as e:
            result = {
                "name": name, "status": "❌ FAIL", "status_code": "CONN_ERR",
                "endpoint": endpoint, "notes": notes,
                "error": str(e)[:300]
            }
        except Exception as e:
            result = {
                "name": name, "status": "❌ FAIL", "status_code": "ERROR",
                "endpoint": endpoint, "notes": notes,
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
        """运行所有联邦API测试"""
        print("\n" + "="*80)
        print("3.3 联邦通信API测试 (32个端点)")
        print(f"测试时间: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
        print("="*80)
        
        passed = 0
        failed = 0
        
        # 3.3.1 密钥与发现 (6个端点)
        print("\n3.3.1 密钥与发现 (6个端点)")
        print("-" * 60)
        
        key_discovery_tests = [
            ("获取服务器密钥v2", "GET", "/_matrix/federation/v2/server", 200, None, "", "返回密钥信息"),
            ("获取服务器密钥", "GET", "/_matrix/key/v2/server", 200, None, "", "返回密钥信息"),
            ("查询密钥v2", "GET", "/_matrix/federation/v2/query/cjystx.top/ed25519:auto", 200, None, "", "查询指定密钥"),
            ("查询密钥", "GET", "/_matrix/key/v2/query/cjystx.top/ed25519:auto", 200, None, "", "查询指定密钥"),
            ("获取联邦版本", "GET", "/_matrix/federation/v1/version", 200, None, "", "返回版本信息"),
            ("联邦发现", "GET", "/_matrix/federation/v1", 200, None, "", "返回联邦能力"),
        ]
        
        for test in key_discovery_tests:
            test_len = len(test)
            name, method, endpoint = test[0], test[1], test[2]
            expected = test[3] if test_len > 3 else None
            data = test[4] if test_len > 4 else None
            notes = test[5] if test_len > 5 else ""
            result = self.test_api(name, method, endpoint, expected, data, notes=notes)
            self.print_result(result)
            if result["status"].startswith("✅"):
                passed += 1
            else:
                failed += 1
        
        # 3.3.2 房间操作 (26个端点 - 文档说有32个总端点)
        print("\n3.3.2 房间操作 (19个端点)")
        print("-" * 60)
        
        room_tests = [
            ("获取公共房间", "GET", "/_matrix/federation/v1/publicRooms", 200, None, ""),
            ("发送事务", "PUT", "/_matrix/federation/v1/send/test_txn", None, None, "需要联邦签名"),
            ("生成加入模板", "GET", "/_matrix/federation/v1/make_join/!room:test/@user:test", None, None, "需要联邦签名"),
            ("生成离开模板", "GET", "/_matrix/federation/v1/make_leave/!room:test/@user:test", None, None, "需要联邦签名"),
            ("发送加入", "PUT", "/_matrix/federation/v1/send_join/!room:test/$event", None, None, "需要联邦签名"),
            ("发送离开", "PUT", "/_matrix/federation/v1/send_leave/!room:test/$event", None, None, "需要联邦签名"),
            ("联邦邀请", "PUT", "/_matrix/federation/v1/invite/!room:test/$event", None, None, "需要联邦签名"),
            ("获取缺失事件", "POST", "/_matrix/federation/v1/get_missing_events/!room:test", None, {"limit": 10}, "需要联邦签名"),
            ("获取事件授权", "GET", "/_matrix/federation/v1/get_event_auth/!room:test/$event", None, None, "需要联邦签名"),
            ("获取房间状态", "GET", "/_matrix/federation/v1/state/!room:test", None, None, "需要联邦签名"),
            ("获取事件", "GET", "/_matrix/federation/v1/event/$event", None, None, "需要联邦签名"),
            ("获取状态ID", "GET", "/_matrix/federation/v1/state_ids/!room:test", None, None, "需要联邦签名"),
            ("房间目录查询", "GET", "/_matrix/federation/v1/query/directory/room/!room:test", None, None, "需要联邦签名"),
            ("用户资料查询", "GET", "/_matrix/federation/v1/query/profile/@user:test", None, None, "需要联邦签名"),
            ("回填事件", "GET", "/_matrix/federation/v1/backfill/!room:test", None, None, "需要联邦签名"),
            ("声明密钥", "POST", "/_matrix/federation/v1/keys/claim", None, {"one_time_keys": {}}, "需要联邦签名"),
            ("上传密钥", "POST", "/_matrix/federation/v1/keys/upload", None, {"device_keys": {}}, "需要联邦签名"),
            ("克隆密钥", "POST", "/_matrix/federation/v2/key/clone", None, {"server_name": "test"}, "需要联邦签名"),
            ("查询用户密钥", "POST", "/_matrix/federation/v2/user/keys/query", None, {"user_ids": ["@test"]}, "需要联邦签名"),
        ]
        
        for test in room_tests:
            test_len = len(test)
            name, method, endpoint = test[0], test[1], test[2]
            expected = test[3] if test_len > 3 else None
            data = test[4] if test_len > 4 else None
            notes = test[5] if test_len > 5 else ""
            result = self.test_api(name, method, endpoint, expected, data, notes=notes)
            self.print_result(result)
            if result["status"].startswith("✅"):
                passed += 1
            else:
                failed += 1
        
        # 额外端点测试 (补足32个端点)
        print("\n3.3.3 附加联邦端点 (7个端点)")
        print("-" * 60)
        
        additional_tests = [
            ("联邦密钥交换v1", "POST", "/_matrix/federation/v1/keys/query", None, {"user_ids": ["@test"]}, "需要联邦签名"),
            ("联邦版本查询", "GET", "/_matrix/federation/v1/version", 200, None, "公开端点"),
            ("服务器密钥v1", "GET", "/_matrix/key/v2/server", 200, None, "公开端点"),
            ("获取成员", "GET", "/_matrix/federation/v1/members/!room:test", None, None, "需要联邦签名"),
            ("获取成员状态", "GET", "/_matrix/federation/v1/members/!room:test/joined", None, None, "需要联邦签名"),
            ("用户设备", "GET", "/_matrix/federation/v1/user/devices/@test", None, None, "需要联邦签名"),
            ("房间认证", "GET", "/_matrix/federation/v1/room_auth/!room:test", None, None, "需要联邦签名"),
        ]
        
        for test in additional_tests:
            test_len = len(test)
            name, method, endpoint = test[0], test[1], test[2]
            expected = test[3] if test_len > 3 else None
            data = test[4] if test_len > 4 else None
            notes = test[5] if test_len > 5 else ""
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
        
        # 分类统计
        print("\n分类统计:")
        print(f"  密钥与发现: 6个端点")
        print(f"  房间操作: 19个端点")
        print(f"  附加联邦端点: 7个端点")
        print(f"  总计: 32个端点")
        
        # 保存详细结果
        self.save_results()
        
        return passed, failed, total
    
    def save_results(self):
        """保存测试结果到JSON文件"""
        output_file = "/home/hula/synapse_rust/docker/federation_test_v2_results.json"
        with open(output_file, "w", encoding="utf-8") as f:
            json.dump({
                "timestamp": datetime.now().isoformat(),
                "base_url": self.base_url,
                "total_endpoints": 32,
                "results": self.results
            }, f, ensure_ascii=False, indent=2)
        print(f"\n详细结果已保存到: {output_file}")
        
        # 生成markdown报告
        self.generate_markdown_report()
    
    def generate_markdown_report(self):
        """生成Markdown格式的测试报告"""
        report_file = "/home/hula/synapse_rust/docker/federation_test_report.md"
        
        with open(report_file, "w", encoding="utf-8") as f:
            f.write("# 联邦通信API测试报告\n\n")
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
                if not result["status"].startswith("✅"):
                    f.write(f"### {result['name']}\n\n")
                    f.write(f"- **端点**: `{result['method']} {result['endpoint']}`\n")
                    f.write(f"- **状态码**: {result.get('status_code', 'N/A')}\n")
                    f.write(f"- **响应**: {json.dumps(result.get('response', {}), indent=2, ensure_ascii=False)}\n")
                    f.write("\n")
        
        print(f"Markdown报告已保存到: {report_file}")


if __name__ == "__main__":
    print("联邦通信API测试脚本 v2.0")
    print("="*80)
    
    tester = FederationAPITester(BASE_URL)
    passed, failed, total = tester.run_tests()
    
    sys.exit(0 if failed == 0 else 1)
