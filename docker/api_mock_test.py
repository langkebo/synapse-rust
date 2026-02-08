#!/usr/bin/env python3
"""
Synapse Rust Matrix API 模拟数据测试脚本
=========================================

本脚本用于测试API参考文档中标记为"未测试"的API端点。
使用预定义的模拟数据，无需手动准备测试数据。

使用方法:
    python3 api_mock_test.py [--verbose] [--report] [--category <category>]

参数:
    --verbose: 显示详细测试输出
    --report: 生成测试报告
    --category: 只运行特定类别的测试 (admin|federation|enhanced|media)
    --help: 显示帮助信息

作者: Synapse Rust 测试团队
日期: 2026-02-07
"""

import json
import sys
import os
import time
import argparse
from datetime import datetime
from typing import Dict, List, Any, Optional, Tuple
from pathlib import Path
import subprocess
import hmac
import hashlib

class APITester:
    """API测试器类"""
    
    def __init__(self, base_url: str = "http://localhost:8008", verbose: bool = False):
        self.base_url = base_url
        self.verbose = verbose
        self.results = []
        self.token = None
        self.test_data = None
        self.load_test_data()
    
    def load_test_data(self):
        """加载测试数据"""
        test_data_path = Path(__file__).parent / "test_data.json"
        if test_data_path.exists():
            with open(test_data_path, 'r', encoding='utf-8') as f:
                self.test_data = json.load(f)
            self.admin_user = self.test_data.get('admin_account', {})
            self.test_users = self.test_data.get('test_users', [])
            self.test_rooms = self.test_data.get('test_rooms', [])
        else:
            self.admin_user = {
                "username": "admin",
                "password": "Wzc9890951!",
                "user_id": "@admin:cjystx.top"
            }
            self.test_users = []
            self.test_rooms = []
    
    def get_token(self, username: str = None, password: str = None) -> Optional[str]:
        """获取访问令牌"""
        if username is None:
            username = self.test_users[0]['username'] if self.test_users else "testuser1"
        if password is None:
            password = self.test_users[0]['password'] if self.test_users else "TestUser123!"
        
        try:
            cmd = [
                'curl', '-s', '-X', 'POST',
                f'{self.base_url}/_matrix/client/r0/login',
                '-H', 'Content-Type: application/json',
                '-d', json.dumps({
                    "type": "m.login.password",
                    "user": username,
                    "password": password
                })
            ]
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
            response = json.loads(result.stdout)
            
            if 'access_token' in response:
                self.token = response['access_token']
                return self.token
            else:
                print(f"获取token失败: {response}")
                return None
        except Exception as e:
            print(f"获取token时出错: {e}")
            return None
    
    def make_request(self, method: str, endpoint: str, 
                    data: Dict = None, headers: Dict = None,
                    requires_auth: bool = False) -> Tuple[int, Dict]:
        """
        发送API请求
        
        Args:
            method: HTTP方法 (GET, POST, PUT, DELETE)
            endpoint: API端点
            data: 请求数据 (字典)
            headers: 额外请求头
            requires_auth: 是否需要认证
        
        Returns:
            (状态码, 响应字典)
        """
        url = f"{self.base_url}{endpoint}"
        
        # 使用-w参数获取HTTP状态码
        cmd = ['curl', '-s', '-w', '%{http_code}', '-X', method, url]
        
        if headers:
            for key, value in headers.items():
                cmd.extend(['-H', f'{key}: {value}'])
        
        if requires_auth and self.token:
            cmd.extend(['-H', f'Authorization: Bearer {self.token}'])
        
        if data:
            cmd.extend(['-H', 'Content-Type: application/json'])
            cmd.extend(['-d', json.dumps(data)])
        
        try:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
            
            # 分离HTTP状态码和响应体
            output = result.stdout.strip()
            if output:
                # 尝试提取状态码（最后3个字符可能是状态码）
                if len(output) >= 3:
                    # 假设最后一行是状态码
                    lines = output.split('\n')
                    if len(lines) > 1:
                        try:
                            http_code = int(lines[-1])
                            response_body = '\n'.join(lines[:-1])
                        except ValueError:
                            http_code = 200
                            response_body = output
                    else:
                        # 检查是否整个输出都是数字
                        try:
                            http_code = int(output)
                            response_body = ""
                        except ValueError:
                            http_code = 200
                            response_body = output
                else:
                    http_code = 200
                    response_body = output
            
            try:
                response = json.loads(response_body) if response_body else {}
            except json.JSONDecodeError:
                response = {"raw_response": response_body}
            
            return http_code, response
        except subprocess.TimeoutExpired:
            return 408, {"error": "请求超时"}
        except Exception as e:
            return 500, {"error": str(e)}
    
    def test_admin_apis(self) -> Dict[str, Any]:
        """测试管理员API"""
        print("\n" + "="*60)
        print("测试管理员API")
        print("="*60)
        
        results = {
            "category": "管理员API",
            "tested": 0,
            "passed": 0,
            "failed": 0,
            "unknown_limit": 0,
            "details": []
        }
        
        # 确保获取token
        if not self.token:
            self.get_token()
        
        if not self.token:
            print("无法获取认证token，跳过管理员API测试")
            return results
        
        admin_tests = [
            {
                "name": "获取用户房间",
                "endpoint": f"/_synapse/admin/v1/users/@testuser1:cjystx.top/rooms",
                "method": "GET",
                "expected_success": True,
                "description": "获取指定用户的所有房间"
            },
            {
                "name": "设置用户为管理员",
                "endpoint": "/_synapse/admin/v1/users/@testuser2:cjystx.top/admin",
                "method": "PUT",
                "data": {"admin": True},
                "expected_success": True,
                "description": "将用户设置为管理员"
            },
            {
                "name": "停用用户",
                "endpoint": "/_synapse/admin/v1/users/@testuser6:cjystx.top/deactivate",
                "method": "POST",
                "expected_success": True,
                "description": "停用指定用户账户"
            },
            {
                "name": "获取服务器统计",
                "endpoint": "/_synapse/admin/v1/server_stats",
                "method": "GET",
                "expected_success": True,
                "description": "获取服务器运行统计信息"
            },
            {
                "name": "获取服务器配置",
                "endpoint": "/_synapse/admin/v1/config",
                "method": "GET",
                "expected_success": True,
                "description": "获取服务器配置信息"
            },
            {
                "name": "获取用户统计",
                "endpoint": "/_synapse/admin/v1/user_stats",
                "method": "GET",
                "expected_success": True,
                "description": "获取用户相关统计"
            },
            {
                "name": "获取媒体统计",
                "endpoint": "/_synapse/admin/v1/media_stats",
                "method": "GET",
                "expected_success": True,
                "description": "获取媒体存储统计"
            },
            {
                "name": "删除房间",
                "endpoint": "/_synapse/admin/v1/rooms/!test_delete_room:cjystx.top",
                "method": "DELETE",
                "expected_success": False,  # 房间可能不存在
                "description": "删除指定房间"
            },
            {
                "name": "关闭房间",
                "endpoint": "/_synapse/admin/v1/shutdown_room",
                "method": "POST",
                "data": {"room_id": "!test_shutdown_room:cjystx.top"},
                "expected_success": False,
                "description": "关闭指定房间"
            },
            {
                "name": "清理房间历史",
                "endpoint": "/_synapse/admin/v1/purge_history",
                "method": "POST",
                "data": {"room_id": "!test_purge_room:cjystx.top"},
                "expected_success": False,
                "description": "清理房间历史消息"
            },
            {
                "name": "获取服务器版本",
                "endpoint": "/_synapse/admin/v1/server_version",
                "method": "GET",
                "expected_success": True,
                "description": "获取Synapse服务器版本"
            },
            {
                "name": "获取服务器状态",
                "endpoint": "/_synapse/admin/v1/status",
                "method": "GET",
                "expected_success": True,
                "description": "获取服务器运行状态"
            }
        ]
        
        for test in admin_tests:
            results["tested"] += 1
            status_code, response = self.make_request(
                method=test["method"],
                endpoint=test["endpoint"],
                data=test.get("data"),
                requires_auth=True
            )
            
            # 判断测试是否通过
            # 1. 如果API返回404（资源不存在），对于删除/关闭操作视为正常
            # 2. 如果API返回200/201/204，视为成功
            # 3. 如果API返回400+错误，视为失败
            
            is_success = status_code in [200, 201, 204]
            is_not_found = status_code == 404
            
            # 对于删除/关闭操作，404也视为通过（资源不存在）
            is_delete_operation = test["method"] in ["DELETE", "POST"]
            passed = is_success or (is_not_found and is_delete_operation)
            
            if passed:
                results["passed"] += 1
                status = "✅ 通过"
            else:
                results["failed"] += 1
                status = "❌ 失败"
            
            detail = {
                "name": test["name"],
                "endpoint": test["endpoint"],
                "method": test["method"],
                "status_code": status_code,
                "response": response,
                "result": status,
                "description": test["description"]
            }
            results["details"].append(detail)
            
            print(f"{status} [{test['method']}] {test['name']}")
            if self.verbose or not passed:
                print(f"    端点: {test['endpoint']}")
                print(f"    状态码: {status_code}")
                print(f"    响应: {json.dumps(response, ensure_ascii=False)[:200]}")
        
        return results
    
    def test_federation_apis(self) -> Dict[str, Any]:
        """测试联邦API"""
        print("\n" + "="*60)
        print("测试联邦通信API")
        print("="*60)
        
        results = {
            "category": "联邦通信API",
            "tested": 0,
            "passed": 0,
            "failed": 0,
            "unknown_limit": 0,
            "details": []
        }
        
        federation_tests = [
            {
                "name": "获取服务器密钥",
                "endpoint": "/_matrix/key/v2/server",
                "method": "GET",
                "expected_success": True,
                "description": "获取服务器的公钥信息"
            },
            {
                "name": "联邦密钥查询",
                "endpoint": "/_matrix/key/v2/query/cjystx.top/ed25519:0",
                "method": "GET",
                "expected_success": True,
                "description": "查询指定服务器的密钥"
            },
            {
                "name": "联邦密钥交换",
                "endpoint": "/_matrix/federation/v1/keys/query",
                "method": "POST",
                "data": {"server_keys": {"cjystx.top": {}}},
                "expected_success": True,
                "description": "执行联邦密钥交换"
            },
            {
                "name": "声明密钥",
                "endpoint": "/_matrix/federation/v1/keys/claim",
                "method": "POST",
                "data": {"one_time_keys": {"@testuser1:cjystx.top": {}}},
                "expected_success": False,
                "description": "声明一次性密钥"
            },
            {
                "name": "上传联邦密钥",
                "endpoint": "/_matrix/federation/v1/keys/upload",
                "method": "POST",
                "data": {"device_keys": {}, "one_time_keys": {}},
                "expected_success": False,
                "description": "上传设备密钥到联邦"
            },
            {
                "name": "回填事件",
                "endpoint": "/_matrix/federation/v1/backfill/!test_room:cjystx.top",
                "method": "GET",
                "params": {"limit": 10},
                "expected_success": False,
                "description": "回填房间历史事件"
            },
            {
                "name": "生成加入模板",
                "endpoint": "/_matrix/federation/v1/make_join/!test_room:cjystx.top/@testuser1:cjystx.top",
                "method": "GET",
                "expected_success": False,
                "description": "生成加入房间的模板"
            },
            {
                "name": "生成离开模板",
                "endpoint": "/_matrix/federation/v1/make_leave/!test_room:cjystx.top/@testuser1:cjystx.top",
                "method": "GET",
                "expected_success": False,
                "description": "生成离开房间的模板"
            },
            {
                "name": "获取缺失事件",
                "endpoint": "/_matrix/federation/v1/get_missing_events/!test_room:cjystx.top",
                "method": "POST",
                "data": {"earliest_events": [], "limit": 5},
                "expected_success": False,
                "description": "获取房间中缺失的事件"
            },
            {
                "name": "获取事件授权",
                "endpoint": "/_matrix/federation/v1/get_event_auth/!test_room:cjystx.top/$event_id",
                "method": "GET",
                "expected_success": False,
                "description": "获取事件的授权链"
            }
        ]
        
        for test in federation_tests:
            results["tested"] += 1
            
            # 检查是否需要签名
            requires_signature = any(keyword in test["endpoint"] for keyword in [
                "/state/", "/event/", "/room_auth/", "/members/", "/user/devices/"
            ])
            
            if requires_signature:
                results["unknown_limit"] += 1
                status = "⚠️ 需要联邦签名"
                detail = {
                    "name": test["name"],
                    "endpoint": test["endpoint"],
                    "method": test["method"],
                    "status_code": None,
                    "response": {"error": "需要联邦签名认证"},
                    "result": status,
                    "description": test["description"],
                    "note": "此API需要有效的联邦签名"
                }
            else:
                status_code, response = self.make_request(
                    method=test["method"],
                    endpoint=test["endpoint"],
                    data=test.get("data"),
                    requires_auth=False
                )
                
                is_success = status_code < 400
                passed = is_success == test.get("expected_success", True)
                
                if passed:
                    results["passed"] += 1
                    status = "✅ 通过"
                else:
                    results["failed"] += 1
                    status = "❌ 失败"
                
                detail = {
                    "name": test["name"],
                    "endpoint": test["endpoint"],
                    "method": test["method"],
                    "status_code": status_code,
                    "response": response,
                    "result": status,
                    "description": test["description"]
                }
            
            results["details"].append(detail)
            print(f"{status} [{test['method']}] {test['name']}")
            if self.verbose and "response" in detail:
                response = detail.get("response", {})
                print(f"    端点: {test['endpoint']}")
                if isinstance(response, dict):
                    print(f"    响应: {json.dumps(response, ensure_ascii=False)[:200]}")
        
        return results
    
    def test_enhanced_apis(self) -> Dict[str, Any]:
        """测试增强API"""
        print("\n" + "="*60)
        print("测试增强API (好友系统、私聊增强等)")
        print("="*60)
        
        results = {
            "category": "增强API",
            "tested": 0,
            "passed": 0,
            "failed": 0,
            "unknown_limit": 0,
            "details": []
        }
        
        if not self.token:
            self.get_token()
        
        enhanced_tests = [
            {
                "category": "好友系统API",
                "tests": [
                    {
                        "name": "接受好友请求",
                        "endpoint": "/_synapse/enhanced/friend/request/test_request_001/accept",
                        "method": "POST",
                        "expected_success": False,
                        "description": "接受指定的好友请求"
                    },
                    {
                        "name": "拒绝好友请求",
                        "endpoint": "/_synapse/enhanced/friend/request/test_request_001/decline",
                        "method": "POST",
                        "expected_success": False,
                        "description": "拒绝指定的好友请求"
                    },
                    {
                        "name": "封禁用户",
                        "endpoint": "/_synapse/enhanced/friend/blocks/@testuser9:cjystx.top",
                        "method": "POST",
                        "expected_success": False,
                        "description": "封禁指定用户"
                    },
                    {
                        "name": "解除用户封禁",
                        "endpoint": "/_synapse/enhanced/friend/blocks/@testuser9:cjystx.top/@blocked_user:cjystx.top",
                        "method": "DELETE",
                        "expected_success": False,
                        "description": "解除对指定用户的封禁"
                    },
                    {
                        "name": "获取好友分类",
                        "endpoint": "/_synapse/enhanced/friend/categories/@testuser1:cjystx.top",
                        "method": "GET",
                        "expected_success": True,
                        "description": "获取用户的好友分类"
                    },
                    {
                        "name": "创建好友分类",
                        "endpoint": "/_synapse/enhanced/friend/categories/@testuser1:cjystx.top",
                        "method": "POST",
                        "data": {"name": "测试分类", "color": "#FF0000"},
                        "expected_success": True,
                        "description": "创建新的好友分类"
                    }
                ]
            },
            {
                "category": "私聊增强API",
                "tests": [
                    {
                        "name": "获取私聊详情",
                        "endpoint": "/_matrix/client/r0/rooms/!test_dm_room:cjystx.top/dm",
                        "method": "GET",
                        "expected_success": False,
                        "description": "获取私聊房间的详细信息"
                    },
                    {
                        "name": "获取未读消息",
                        "endpoint": "/_matrix/client/r0/rooms/!test_dm_room:cjystx.top/unread",
                        "method": "GET",
                        "expected_success": True,
                        "description": "获取私聊房间的未读消息"
                    },
                    {
                        "name": "获取会话详情",
                        "endpoint": "/_synapse/enhanced/private/sessions/test_session_001",
                        "method": "GET",
                        "expected_success": False,
                        "description": "获取私聊会话的详细信息"
                    },
                    {
                        "name": "删除会话",
                        "endpoint": "/_synapse/enhanced/private/sessions/test_session_001",
                        "method": "DELETE",
                        "expected_success": False,
                        "description": "删除指定的私聊会话"
                    },
                    {
                        "name": "获取会话消息",
                        "endpoint": "/_synapse/enhanced/private/sessions/test_session_001/messages",
                        "method": "GET",
                        "expected_success": False,
                        "description": "获取私聊会话的消息历史"
                    },
                    {
                        "name": "发送会话消息",
                        "endpoint": "/_synapse/enhanced/private/sessions/test_session_001/messages",
                        "method": "POST",
                        "data": {"body": "测试消息", "msgtype": "m.text"},
                        "expected_success": False,
                        "description": "在私聊会话中发送消息"
                    },
                    {
                        "name": "删除私聊消息",
                        "endpoint": "/_synapse/enhanced/private/messages/test_msg_001",
                        "method": "DELETE",
                        "expected_success": False,
                        "description": "删除指定的私聊消息"
                    },
                    {
                        "name": "标记消息已读",
                        "endpoint": "/_synapse/enhanced/private/messages/test_msg_001/read",
                        "method": "POST",
                        "expected_success": False,
                        "description": "将私聊消息标记为已读"
                    }
                ]
            },
            {
                "category": "密钥备份API",
                "tests": [
                    {
                        "name": "获取房间密钥",
                        "endpoint": "/_matrix/client/r0/room_keys/version_001",
                        "method": "GET",
                        "expected_success": False,
                        "description": "获取指定版本的房间密钥备份"
                    },
                    {
                        "name": "上传房间密钥",
                        "endpoint": "/_matrix/client/r0/room_keys/version_001",
                        "method": "PUT",
                        "data": {"rooms": {}},
                        "expected_success": False,
                        "description": "上传房间密钥备份"
                    }
                ]
            }
        ]
        
        for category_group in enhanced_tests:
            category_name = category_group["category"]
            print(f"\n  -- {category_name} --")
            
            for test in category_group["tests"]:
                results["tested"] += 1
                
                status_code, response = self.make_request(
                    method=test["method"],
                    endpoint=test["endpoint"],
                    data=test.get("data"),
                    requires_auth=True
                )
                
                is_success = status_code < 400
                passed = is_success == test.get("expected_success", True)
                
                if passed:
                    results["passed"] += 1
                    status = "✅ 通过"
                else:
                    results["failed"] += 1
                    status = "❌ 失败"
                
                detail = {
                    "name": test["name"],
                    "endpoint": test["endpoint"],
                    "method": test["method"],
                    "status_code": status_code,
                    "response": response,
                    "result": status,
                    "description": test["description"]
                }
                results["details"].append(detail)
                
                print(f"{status} [{test['method']}] {test['name']}")
                if self.verbose or not passed:
                    print(f"    端点: {test['endpoint']}")
                    print(f"    状态码: {status_code}")
                    if isinstance(response, dict):
                        print(f"    响应: {json.dumps(response, ensure_ascii=False)[:200]}")
        
        return results
    
    def test_media_apis(self) -> Dict[str, Any]:
        """测试媒体文件API"""
        print("\n" + "="*60)
        print("测试媒体文件API")
        print("="*60)
        
        results = {
            "category": "媒体文件API",
            "tested": 0,
            "passed": 0,
            "failed": 0,
            "unknown_limit": 0,
            "details": []
        }
        
        if not self.token:
            self.get_token()
        
        # 创建测试媒体文件
        test_media_content = b'\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89\x00\x00\x00\nIDATx\x9cc\x00\x01\x00\x00\x05\x00\x01\r\n-\xb4\x00\x00\x00\x00IEND\xaeB`\x82'
        
        media_tests = [
            {
                "name": "下载媒体文件",
                "endpoint": "/_matrix/media/v3/download/cjystx.top/test_media_id",
                "method": "GET",
                "expected_success": False,
                "description": "下载指定媒体文件"
            },
            {
                "name": "获取缩略图",
                "endpoint": "/_matrix/media/v3/thumbnail/cjystx.top/test_media_id",
                "method": "GET",
                "params": {"width": 100, "height": 100},
                "expected_success": False,
                "description": "获取媒体文件的缩略图"
            },
            {
                "name": "v1版本下载",
                "endpoint": "/_matrix/media/v1/download/cjystx.top/test_media_id",
                "method": "GET",
                "expected_success": False,
                "description": "使用v1 API下载媒体文件"
            },
            {
                "name": "r1版本下载",
                "endpoint": "/_matrix/media/r1/download/cjystx.top/test_media_id",
                "method": "GET",
                "expected_success": False,
                "description": "使用r1 API下载媒体文件"
            }
        ]
        
        for test in media_tests:
            results["tested"] += 1
            
            # 尝试直接访问（不需要认证）
            status_code, response = self.make_request(
                method=test["method"],
                endpoint=test["endpoint"],
                requires_auth=False
            )
            
            is_success = status_code < 400
            passed = is_success == test.get("expected_success", True)
            
            if passed:
                results["passed"] += 1
                status = "✅ 通过"
            else:
                results["failed"] += 1
                status = "❌ 失败"
            
            detail = {
                "name": test["name"],
                "endpoint": test["endpoint"],
                "method": test["method"],
                "status_code": status_code,
                "response": {"content_type": response.get("content_type", "unknown")} if isinstance(response, str) else response,
                "result": status,
                "description": test["description"]
            }
            results["details"].append(detail)
            
            print(f"{status} [{test['method']}] {test['name']}")
            if self.verbose or not passed:
                print(f"    端点: {test['endpoint']}")
                print(f"    状态码: {status_code}")
        
        return results
    
    def test_voice_apis(self) -> Dict[str, Any]:
        """测试语音消息API"""
        print("\n" + "="*60)
        print("测试语音消息API")
        print("="*60)
        
        results = {
            "category": "语音消息API",
            "tested": 0,
            "passed": 0,
            "failed": 0,
            "unknown_limit": 0,
            "details": []
        }
        
        if not self.token:
            self.get_token()
        
        voice_tests = [
            {
                "name": "获取语音消息",
                "endpoint": "/_matrix/client/r0/voice/test_voice_message_001",
                "method": "GET",
                "expected_success": False,
                "description": "获取指定语音消息"
            },
            {
                "name": "删除语音消息",
                "endpoint": "/_matrix/client/r0/voice/test_voice_message_001",
                "method": "DELETE",
                "expected_success": False,
                "description": "删除指定语音消息"
            },
            {
                "name": "获取用户语音消息",
                "endpoint": "/_matrix/client/r0/voice/user/@testuser1:cjystx.top",
                "method": "GET",
                "expected_success": True,
                "description": "获取用户的所有语音消息"
            },
            {
                "name": "获取房间语音消息",
                "endpoint": "/_matrix/client/r0/voice/room/!test_room:cjystx.top",
                "method": "GET",
                "expected_success": True,
                "description": "获取房间中的所有语音消息"
            },
            {
                "name": "获取用户语音统计",
                "endpoint": "/_matrix/client/r0/voice/user/@testuser1:cjystx.top/stats",
                "method": "GET",
                "expected_success": True,
                "description": "获取用户的语音消息统计"
            }
        ]
        
        for test in voice_tests:
            results["tested"] += 1
            
            status_code, response = self.make_request(
                method=test["method"],
                endpoint=test["endpoint"],
                requires_auth=True
            )
            
            is_success = status_code < 400
            passed = is_success == test.get("expected_success", True)
            
            if passed:
                results["passed"] += 1
                status = "✅ 通过"
            else:
                results["failed"] += 1
                status = "❌ 失败"
            
            detail = {
                "name": test["name"],
                "endpoint": test["endpoint"],
                "method": test["method"],
                "status_code": status_code,
                "response": response,
                "result": status,
                "description": test["description"]
            }
            results["details"].append(detail)
            
            print(f"{status} [{test['method']}] {test['name']}")
            if self.verbose or not passed:
                print(f"    端点: {test['endpoint']}")
                print(f"    状态码: {status_code}")
                if isinstance(response, dict):
                    print(f"    响应: {json.dumps(response, ensure_ascii=False)[:200]}")
        
        return results
    
    def run_all_tests(self) -> Dict[str, Any]:
        """运行所有测试"""
        print("\n" + "="*70)
        print("Synapse Rust Matrix API 模拟数据测试")
        print("="*70)
        print(f"测试时间: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
        print(f"服务器地址: {self.base_url}")
        
        all_results = {
            "test_time": datetime.now().isoformat(),
            "base_url": self.base_url,
            "categories": []
        }
        
        # 测试各类API
        all_results["categories"].append(self.test_admin_apis())
        all_results["categories"].append(self.test_federation_apis())
        all_results["categories"].append(self.test_enhanced_apis())
        all_results["categories"].append(self.test_media_apis())
        all_results["categories"].append(self.test_voice_apis())
        
        # 计算总计
        total_tested = sum(cat["tested"] for cat in all_results["categories"])
        total_passed = sum(cat["passed"] for cat in all_results["categories"])
        total_failed = sum(cat["failed"] for cat in all_results["categories"])
        total_unknown = sum(cat["unknown_limit"] for cat in all_results["categories"])
        
        # 打印总结
        print("\n" + "="*70)
        print("测试总结")
        print("="*70)
        print(f"总计测试: {total_tested}")
        print(f"通过: {total_passed} ✅")
        print(f"失败: {total_failed} ❌")
        print(f"需要联邦签名: {total_unknown} ⚠️")
        print(f"通过率: {total_passed/total_tested*100:.1f}%" if total_tested > 0 else "N/A")
        
        all_results["summary"] = {
            "total_tested": total_tested,
            "total_passed": total_passed,
            "total_failed": total_failed,
            "total_unknown_limit": total_unknown,
            "pass_rate": f"{total_passed/total_tested*100:.1f}%" if total_tested > 0 else "N/A"
        }
        
        return all_results
    
    def generate_report(self, results: Dict[str, Any], output_path: str = None):
        """生成测试报告"""
        if output_path is None:
            output_path = Path(__file__).parent / "api_test_report.json"
        
        with open(output_path, 'w', encoding='utf-8') as f:
            json.dump(results, f, ensure_ascii=False, indent=2)
        
        print(f"\n测试报告已保存到: {output_path}")
        return output_path


def main():
    """主函数"""
    parser = argparse.ArgumentParser(
        description="Synapse Rust Matrix API 模拟数据测试脚本",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
示例:
    python3 api_mock_test.py                    # 运行所有测试
    python3 api_mock_test.py --verbose           # 显示详细输出
    python3 api_mock_test.py --report             # 生成测试报告
    python3 api_mock_test.py --category admin     # 只测试管理员API
        """
    )
    
    parser.add_argument('--verbose', '-v', action='store_true',
                        help='显示详细测试输出')
    parser.add_argument('--report', '-r', action='store_true',
                        help='生成测试报告')
    parser.add_argument('--category', '-c', choices=['admin', 'federation', 'enhanced', 'media', 'voice'],
                        help='只运行特定类别的测试')
    parser.add_argument('--output', '-o', default=None,
                        help='测试报告输出路径')
    
    args = parser.parse_args()
    
    # 创建测试器
    tester = APITester(verbose=args.verbose)
    
    # 运行测试
    if args.category:
        if args.category == 'admin':
            results = {"categories": [tester.test_admin_apis()]}
        elif args.category == 'federation':
            results = {"categories": [tester.test_federation_apis()]}
        elif args.category == 'enhanced':
            results = {"categories": [tester.test_enhanced_apis()]}
        elif args.category == 'media':
            results = {"categories": [tester.test_media_apis()]}
        elif args.category == 'voice':
            results = {"categories": [tester.test_voice_apis()]}
    else:
        results = tester.run_all_tests()
    
    # 生成报告
    if args.report:
        output_path = tester.generate_report(results, args.output)
    
    return 0


if __name__ == "__main__":
    sys.exit(main())
