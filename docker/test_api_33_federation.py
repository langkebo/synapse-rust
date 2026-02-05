#!/usr/bin/env python3
"""
联邦通信API测试脚本 (3.3 Federation APIs)
测试32个联邦API端点
"""

import requests
import json
import time
import sys

BASE_URL = "http://localhost:8008"
MATRIX_ORG_URL = "https://matrix.org"

class FederationAPITester:
    def __init__(self, base_url=BASE_URL):
        self.base_url = base_url
        self.results = []
        
    def test_api(self, name, method, endpoint, expected_status=None, 
                 data=None, headers=None, is_external=False, external_url=None):
        """测试单个API端点"""
        url = (external_url or self.base_url) + endpoint
        req_headers = {"Content-Type": "application/json"}
        if headers:
            req_headers.update(headers)
            
        start_time = time.time()
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
                return {"name": name, "status": "❌", "code": None, 
                        "error": f"Unknown method: {method}"}
                        
            elapsed = round((time.time() - start_time) * 1000, 2)
            
            is_success = expected_status is None or response.status_code == expected_status
            
            result = {
                "name": name,
                "method": method.upper(),
                "endpoint": endpoint,
                "url": url,
                "status": "✅" if is_success else "❌",
                "code": response.status_code,
                "elapsed": elapsed,
                "is_external": is_external
            }
            
            try:
                result["response"] = response.json()
            except:
                result["response"] = response.text[:500]
                
            if not is_success:
                result["expected"] = expected_status
                
        except requests.exceptions.Timeout:
            result = {
                "name": name, "status": "❌", "code": "TIMEOUT",
                "error": "Request timeout after 30s", "endpoint": endpoint
            }
        except requests.exceptions.ConnectionError as e:
            result = {
                "name": name, "status": "❌", "code": "CONN_ERR",
                "error": str(e)[:200], "endpoint": endpoint
            }
        except Exception as e:
            result = {
                "name": name, "status": "❌", "code": "ERROR",
                "error": str(e)[:200], "endpoint": endpoint
            }
            
        self.results.append(result)
        return result
        
    def print_result(self, result):
        """打印测试结果"""
        status_icon = result.get("status", "❓")
        code = result.get("code", "N/A")
        elapsed = result.get("elapsed", 0)
        name = result.get("name", "Unknown")
        is_ext = " [外部]" if result.get("is_external") else ""
        
        if result.get("status") == "❌":
            error_info = f" | Error: {result.get('error', result.get('expected', 'N/A'))}"
            print(f"  {status_icon} {name}: HTTP {code} ({elapsed}ms){error_info}")
        else:
            print(f"  {status_icon} {name}: HTTP {code} ({elapsed}ms){is_ext}")
            
    def run_tests(self):
        """运行所有联邦API测试"""
        print("\n" + "="*70)
        print("3.3 联邦通信API测试")
        print("="*70)
        
        passed = 0
        failed = 0
        total = 0
        
        # 3.3.1 密钥与发现 (6个端点)
        print("\n3.3.1 密钥与发现")
        print("-" * 50)
        
        federation_tests = [
            ("获取服务器密钥", "GET", "/_matrix/federation/v2/server"),
            ("获取服务器密钥(v2)", "GET", "/_matrix/key/v2/server"),
            ("查询密钥", "GET", "/_matrix/federation/v2/query/{server_name}/{key_id}", 
             {"server_name": "cjystx.top", "key_id": "ed25519:auto"}),
            ("查询密钥(v2)", "GET", "/_matrix/key/v2/query/{server_name}/{key_id}",
             {"server_name": "cjystx.top", "key_id": "ed25519:auto"}),
            ("获取联邦版本", "GET", "/_matrix/federation/v1/version"),
            ("联邦发现", "GET", "/_matrix/federation/v1"),
        ]
        
        for test in federation_tests:
            total += 1
            name, method, endpoint = test[0], test[1], test[2]
            params = test[3] if len(test) > 3 else {}
            
            formatted_endpoint = endpoint
            for key, value in params.items():
                formatted_endpoint = formatted_endpoint.replace("{" + key + "}", value)
                
            result = self.test_api(name, method, formatted_endpoint)
            self.print_result(result)
            if result["status"] == "✅":
                passed += 1
            else:
                failed += 1
        
        # 3.3.2 房间操作 (19个端点)
        print("\n3.3.2 房间操作")
        print("-" * 50)
        
        room_tests = [
            ("获取公共房间", "GET", "/_matrix/federation/v1/publicRooms"),
            ("发送事务", "PUT", "/_matrix/federation/v1/send/{txn_id}",
             {"txn_id": f"test_txn_{int(time.time())}"}),
            ("生成加入模板", "GET", "/_matrix/federation/v1/make_join/{room_id}/{user_id}",
             {"room_id": "!test:cjystx.top", "user_id": "@test:cjystx.top"}),
            ("生成离开模板", "GET", "/_matrix/federation/v1/make_leave/{room_id}/{user_id}",
             {"room_id": "!test:cjystx.top", "user_id": "@test:cjystx.top"}),
            ("发送加入", "PUT", "/_matrix/federation/v1/send_join/{room_id}/{event_id}",
             {"room_id": "!test:cjystx.top", "event_id": "$test_event"}),
            ("发送离开", "PUT", "/_matrix/federation/v1/send_leave/{room_id}/{event_id}",
             {"room_id": "!test:cjystx.top", "event_id": "$test_event"}),
            ("邀请", "PUT", "/_matrix/federation/v1/invite/{room_id}/{event_id}",
             {"room_id": "!test:cjystx.top", "event_id": "$test_event"}),
            ("获取缺失事件", "POST", "/_matrix/federation/v1/get_missing_events/{room_id}",
             {"room_id": "!test:cjystx.top"}, {"limit": 10}),
            ("获取事件授权", "GET", "/_matrix/federation/v1/get_event_auth/{room_id}/{event_id}",
             {"room_id": "!test:cjystx.top", "event_id": "$test_event"}),
            ("获取房间状态", "GET", "/_matrix/federation/v1/state/{room_id}",
             {"room_id": "!test:cjystx.top"}),
            ("获取事件", "GET", "/_matrix/federation/v1/event/{event_id}",
             {"event_id": "$test_event"}),
            ("获取状态ID", "GET", "/_matrix/federation/v1/state_ids/{room_id}",
             {"room_id": "!test:cjystx.top"}),
            ("房间目录查询", "GET", "/_matrix/federation/v1/query/directory/room/{room_id}",
             {"room_id": "!test:cjystx.top"}),
            ("用户资料查询", "GET", "/_matrix/federation/v1/query/profile/{user_id}",
             {"user_id": "@test:cjystx.top"}),
            ("回填事件", "GET", "/_matrix/federation/v1/backfill/{room_id}",
             {"room_id": "!test:cjystx.top"}, {"limit": 100}),
            ("声明密钥", "POST", "/_matrix/federation/v1/keys/claim", None,
             {"one_time_keys": {"@test:cjystx.top": {" Curve25519": "test_key"}}}),
            ("上传密钥", "POST", "/_matrix/federation/v1/keys/upload", None,
             {"device_keys": {}, "one_time_keys": {}}),
            ("克隆密钥", "POST", "/_matrix/federation/v2/key/clone", None,
             {"server_name": "cjystx.top"}),
            ("查询用户密钥", "POST", "/_matrix/federation/v2/user/keys/query", None,
             {"user_ids": ["@test:cjystx.top"]}),
        ]
        
        for test in room_tests:
            total += 1
            name, method, endpoint = test[0], test[1], test[2]
            params = test[3] if len(test) > 3 and test[3] else {}
            data = test[4] if len(test) > 4 else None
            
            formatted_endpoint = endpoint
            for key, value in params.items():
                formatted_endpoint = formatted_endpoint.replace("{" + key + "}", value)
                
            result = self.test_api(name, method, formatted_endpoint, data=data)
            self.print_result(result)
            if result["status"] == "✅":
                passed += 1
            else:
                failed += 1
        
        # 外部服务器测试 (matrix.org)
        print("\n外部服务器测试 (matrix.org)")
        print("-" * 50)
        
        external_tests = [
            ("外部: 获取服务器密钥", "GET", "/_matrix/federation/v2/server", True),
            ("外部: 获取联邦版本", "GET", "/_matrix/federation/v1/version", True),
            ("外部: 公共房间", "GET", "/_matrix/federation/v1/publicRooms", True),
        ]
        
        for test in external_tests:
            total += 1
            name, method, endpoint, is_external = test
            result = self.test_api(name, method, endpoint, is_external=is_external, 
                                   external_url=MATRIX_ORG_URL)
            self.print_result(result)
            if result["status"] == "✅":
                passed += 1
            else:
                failed += 1
        
        # 汇总结果
        print("\n" + "="*70)
        print("测试结果汇总")
        print("="*70)
        print(f"总数: {total} | 通过: {passed} | 失败: {failed}")
        print(f"通过率: {round(passed/total*100, 1)}%")
        print("="*70)
        
        # 保存详细结果
        self.save_results()
        
        return passed, failed, total
    
    def save_results(self):
        """保存测试结果到JSON文件"""
        output_file = "/home/hula/synapse_rust/docker/federation_test_results.json"
        with open(output_file, "w", encoding="utf-8") as f:
            json.dump({
                "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
                "base_url": self.base_url,
                "results": self.results
            }, f, ensure_ascii=False, indent=2)
        print(f"\n详细结果已保存到: {output_file}")


if __name__ == "__main__":
    print("联邦通信API测试脚本")
    print("="*70)
    
    tester = FederationAPITester(BASE_URL)
    passed, failed, total = tester.run_tests()
    
    sys.exit(0 if failed == 0 else 1)
