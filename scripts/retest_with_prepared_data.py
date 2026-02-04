#!/usr/bin/env python3
"""
使用准备好的测试数据重新运行失败的测试
"""

import requests
import json
from datetime import datetime

BASE_URL = "http://localhost:8008"

# 测试账号信息
TEST_ACCOUNTS = {
    "testuser1": {
        "username": "testuser1",
        "password": "TestUser123456!",
        "user_id": "@testuser1:matrix.cjystx.top",
        "access_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXIxOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkB0ZXN0dXNlcjE6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6dHJ1ZSwiZXhwIjoxNzcwMTcwMTQ5LCJpYXQiOjE3NzAxNjY1NDksImRldmljZV9pZCI6InBsT1FGd1hVUWVOUXliSGsifQ.VIoAGokevemWjGlWwWPUXo_7wcXgBzhgQqJs4ZAWJ30"
    },
    "testuser2": {
        "username": "testuser2",
        "password": "TestUser123456!",
        "user_id": "@testuser2:matrix.cjystx.top",
        "access_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXIyOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkB0ZXN0dXNlcjI6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6ZmFsc2UsImV4cCI6MTc3MDE3MDE4NSwiaWF0IjoxNzcwMTY2NTg1LCJkZXZpY2VfaWQiOiJLRkFkejR0cVRPblZwT2h5In0.zcZYm-k7Rl4MHj_sC7nMdgHtu5Cjf24f5fMFt6BYMxg"
    }
}

# 测试房间信息
TEST_ROOMS = {
    "room1": {
        "room_id": "!BfUBQVVQfR0EQUmS9kwF_EQ2:matrix.cjystx.top",
        "name": "Test Room 1"
    }
}

# 加载测试数据
with open('/home/hula/synapse_rust/test_data.json', 'r') as f:
    test_data = json.load(f)['test_data']

# 测试结果存储
test_results = []

def log_result(category, api_name, method, endpoint, status_code, expected_code, success, error=None, response_data=None):
    """记录测试结果"""
    result = {
        "category": category,
        "api_name": api_name,
        "method": method,
        "endpoint": endpoint,
        "status_code": status_code,
        "expected_code": expected_code,
        "success": success,
        "error": error,
        "timestamp": datetime.now().isoformat(),
        "response_data": response_data
    }
    test_results.append(result)
    
    status = "✅ 通过" if success else "❌ 失败"
    print(f"状态: {status}")
    if error:
        print(f"错误: {error}")
    if response_data:
        print(f"响应: {json.dumps(response_data, indent=2, ensure_ascii=False)}")

def make_request(method, endpoint, data=None, token=None, user_id="testuser1"):
    """发送HTTP请求"""
    url = f"{BASE_URL}{endpoint}"
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    
    try:
        if method == "GET":
            response = requests.get(url, headers=headers, timeout=10)
        elif method == "POST":
            response = requests.post(url, json=data, headers=headers, timeout=10)
        elif method == "PUT":
            response = requests.put(url, json=data, headers=headers, timeout=10)
        elif method == "DELETE":
            response = requests.delete(url, headers=headers, timeout=10)
        else:
            return None, None
        
        try:
            response_data = response.json()
        except:
            response_data = response.text
        
        return response, response_data
    except Exception as e:
        return None, {"error": str(e)}

def main():
    """主函数"""
    print("="*80)
    print("使用准备好的测试数据重新运行失败的测试")
    print("="*80)
    print(f"测试时间: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    print(f"服务器地址: {BASE_URL}")
    print(f"\n准备好的测试数据:")
    print(json.dumps(test_data, indent=2, ensure_ascii=False))
    
    token = TEST_ACCOUNTS["testuser1"]["access_token"]
    user_id = TEST_ACCOUNTS["testuser1"]["user_id"]
    
    print("\n" + "="*80)
    print("1. 语音消息API测试")
    print("="*80)
    
    # 测试1: 获取语音消息（使用准备好的message_id）
    print("测试1: 获取语音消息")
    message_id = test_data.get('voice_message_media_id', '').split('/')[-1].replace('.bin', '')
    if message_id:
        response, data = make_request("GET", f"/_matrix/client/r0/voice/{message_id}", token=token)
        if response:
            log_result("5. 语音消息API", "获取语音消息", "GET", f"/_matrix/client/r0/voice/{{message_id}}",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("5. 语音消息API", "获取语音消息", "GET", f"/_matrix/client/r0/voice/{{message_id}}",
                      None, 200, False, data, None)
    else:
        print("跳过测试1: 没有可用的语音消息ID")
    
    print("\n" + "="*80)
    print("2. 好友系统API测试")
    print("="*80)
    
    # 测试2: 更新好友分类（使用准备好的分类名称）
    print("测试2: 更新好友分类")
    category_name = test_data.get('friend_category', 'NewTestCategory')
    response, data = make_request("PUT", f"/_synapse/enhanced/friend/categories/{user_id}/{category_name}",
                               data={"description": "Updated test category"}, token=token)
    if response:
        log_result("6. 好友系统API", "更新好友分类", "PUT",
                  f"/_synapse/enhanced/friend/categories/{{user_id}}/{{category_name}}",
                  response.status_code, 200, response.status_code == 200, None, data)
    else:
        log_result("6. 好友系统API", "更新好友分类", "PUT",
                  f"/_synapse/enhanced/friend/categories/{{user_id}}/{{category_name}}",
                  None, 200, False, data, None)
    
    print("\n" + "="*80)
    print("3. 媒体文件API测试")
    print("="*80)
    
    # 测试3: 下载媒体文件（使用准备好的media_id）
    print("测试3: 下载媒体文件")
    media_id = test_data.get('media_file_id', '')
    if media_id:
        response, data = make_request("GET", media_id, token=token)
        if response:
            log_result("7. 媒体文件API", "下载媒体文件", "GET", media_id,
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("7. 媒体文件API", "下载媒体文件", "GET", media_id,
                      None, 200, False, data, None)
    else:
        print("跳过测试3: 没有可用的媒体文件ID")
    
    print("\n" + "="*80)
    print("4. 私聊API测试")
    print("="*80)
    
    # 测试4: 创建私聊会话（好友关系已建立）
    print("测试4: 创建私聊会话")
    if test_data.get('private_chat_ready', False):
        other_user_id = TEST_ACCOUNTS["testuser2"]["user_id"]
        response, data = make_request("POST", "/_synapse/enhanced/private/sessions",
                                   data={"other_user_id": other_user_id}, token=token)
        if response:
            log_result("8. 私聊API", "创建私聊会话", "POST", "/_synapse/enhanced/private/sessions",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("8. 私聊API", "创建私聊会话", "POST", "/_synapse/enhanced/private/sessions",
                      None, 200, False, data, None)
    else:
        print("跳过测试4: 私聊会话未准备好")
    
    print("\n" + "="*80)
    print("5. 密钥备份API测试")
    print("="*80)
    
    # 测试5: 获取备份版本信息（使用准备好的version）
    print("测试5: 获取备份版本信息")
    version = test_data.get('key_backup_version', '')
    if version:
        response, data = make_request("GET", f"/_matrix/client/r0/room_keys/version/{version}", token=token)
        if response:
            log_result("9. 密钥备份API", "获取备份版本信息", "GET", f"/_matrix/client/r0/room_keys/version/{{version}}",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("9. 密钥备份API", "获取备份版本信息", "GET", f"/_matrix/client/r0/room_keys/version/{{version}}",
                      None, 200, False, data, None)
    else:
        print("跳过测试5: 没有可用的备份版本")
    
    # 测试6: 更新备份版本
    print("测试6: 更新备份版本")
    if version:
        response, data = make_request("PUT", f"/_matrix/client/r0/room_keys/version/{version}",
                                   data={"auth_data": {"public_key": "updated_key"}}, token=token)
        if response:
            log_result("9. 密钥备份API", "更新备份版本", "PUT", f"/_matrix/client/r0/room_keys/version/{{version}}",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("9. 密钥备份API", "更新备份版本", "PUT", f"/_matrix/client/r0/room_keys/version/{{version}}",
                      None, 200, False, data, None)
    else:
        print("跳过测试6: 没有可用的备份版本")
    
    # 测试7: 删除备份版本
    print("测试7: 删除备份版本")
    if version:
        response, data = make_request("DELETE", f"/_matrix/client/r0/room_keys/version/{version}", token=token)
        if response:
            log_result("9. 密钥备份API", "删除备份版本", "DELETE", f"/_matrix/client/r0/room_keys/version/{{version}}",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("9. 密钥备份API", "删除备份版本", "DELETE", f"/_matrix/client/r0/room_keys/version/{{version}}",
                      None, 200, False, data, None)
    else:
        print("跳过测试7: 没有可用的备份版本")
    
    # 测试8: 获取所有房间密钥
    print("测试8: 获取所有房间密钥")
    if version:
        response, data = make_request("GET", f"/_matrix/client/r0/room_keys/{version}", token=token)
        if response:
            log_result("9. 密钥备份API", "获取所有房间密钥", "GET", f"/_matrix/client/r0/room_keys/{{version}}",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("9. 密钥备份API", "获取所有房间密钥", "GET", f"/_matrix/client/r0/room_keys/{{version}}",
                      None, 200, False, data, None)
    else:
        print("跳过测试8: 没有可用的备份版本")
    
    # 测试9: 上传房间密钥
    print("测试9: 上传房间密钥")
    if version:
        room_id = TEST_ROOMS["room1"]["room_id"]
        response, data = make_request("PUT", f"/_matrix/client/r0/room_keys/{version}",
                                   data={
                                       "rooms": {
                                           room_id: {
                                               "sessions": {
                                                   "session_id": {
                                                       "first_message_index": 1,
                                                       "forwarded_count": 0,
                                                       "is_verified": False,
                                                       "session_data": {
                                                           "ciphertext": "test_data",
                                                           "mac": "test_mac"
                                                       }
                                                   }
                                               }
                                           }
                                       }
                                   }, token=token)
        if response:
            log_result("9. 密钥备份API", "上传房间密钥", "PUT", f"/_matrix/client/r0/room_keys/{{version}}",
                      response.status_code, 200, response.status_code == 200, None, data)
        else:
            log_result("9. 密钥备份API", "上传房间密钥", "PUT", f"/_matrix/client/r0/room_keys/{{version}}",
                      None, 200, False, data, None)
    else:
        print("跳过测试9: 没有可用的备份版本")
    
    # 保存测试结果
    print("\n" + "="*80)
    print("测试结果汇总")
    print("="*80)
    
    passed = sum(1 for r in test_results if r['success'])
    failed = sum(1 for r in test_results if not r['success'])
    total = len(test_results)
    success_rate = (passed / total * 100) if total > 0 else 0
    
    print(f"总测试数: {total}")
    print(f"通过数: {passed}")
    print(f"失败数: {failed}")
    print(f"成功率: {success_rate:.2f}%")
    
    if failed > 0:
        print("\n失败的测试:")
        for r in test_results:
            if not r['success']:
                print(f"\n测试: {r['api_name']}")
                print(f"端点: {r['endpoint']}")
                print(f"期望状态码: {r['expected_code']}")
                print(f"实际状态码: {r['status_code']}")
                if r['error']:
                    print(f"响应: {json.dumps(r['error'], indent=2, ensure_ascii=False)}")
    
    # 保存测试结果到文件
    output_file = "/home/hula/synapse_rust/retest_with_prepared_data_results.json"
    with open(output_file, 'w', encoding='utf-8') as f:
        json.dump(test_results, f, indent=2, ensure_ascii=False)
    
    print(f"\n测试结果已保存到: {output_file}")
    print("\n测试完成!")

if __name__ == "__main__":
    main()
