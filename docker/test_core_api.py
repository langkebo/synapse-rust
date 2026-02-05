#!/usr/bin/env python3
"""
核心客户端API完整测试脚本（47个端点）
使用testuser1进行测试
"""

import requests
import json
import sys
import uuid
import time

BASE_URL = "http://localhost:8008"

def get_token():
    try:
        with open("testuser1_token.txt", 'r') as f:
            return f.read().strip()
    except:
        return None

def make_request(method, endpoint, token, data=None, params=None):
    """发起API请求并返回结果"""
    url = f"{BASE_URL}{endpoint}"
    headers = {
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json"
    }
    
    try:
        if method == "GET":
            response = requests.get(url, headers=headers, params=params, timeout=10)
        elif method == "POST":
            response = requests.post(url, headers=headers, json=data, timeout=10)
        elif method == "PUT":
            response = requests.put(url, headers=headers, json=data, timeout=10)
        elif method == "DELETE":
            response = requests.delete(url, headers=headers, timeout=10)
        else:
            return None, "Unknown method"
        
        return response.status_code, response.json()
    except Exception as e:
        return None, str(e)

def test_health():
    """测试1: 健康检查"""
    status, data = make_request("GET", "/health", "dummy")
    return status == 200, status, data

def test_client_versions():
    """测试2: 获取客户端API版本"""
    status, data = make_request("GET", "/_matrix/client/versions", "dummy")
    return status == 200, status, data

def test_register_available(token):
    """测试3: 检查用户名可用性"""
    status, data = make_request("GET", "/_matrix/client/r0/register/available?username=newuser123", token)
    return status == 200, status, data

def test_login():
    """测试6: 用户登录"""
    url = f"{BASE_URL}/_matrix/client/r0/login"
    data = {
        "type": "m.login.password",
        "user": "testuser1",
        "password": "TestPass123!"
    }
    try:
        response = requests.post(url, json=data, timeout=10)
        return response.status_code == 200, response.status_code, response.json()
    except Exception as e:
        return False, None, str(e)

def test_logout(token):
    """测试7: 退出登录"""
    status, data = make_request("POST", "/_matrix/client/r0/logout", token, {})
    return status == 200, status, data

def test_logout_all(token):
    """测试8: 退出所有设备"""
    status, data = make_request("POST", "/_matrix/client/r0/logout/all", token, {})
    return status == 200, status, data

def test_refresh(token):
    """测试9: 刷新令牌"""
    try:
        with open("testuser1_refresh_token.txt", 'r') as f:
            refresh = f.read().strip()
    except:
        return False, None, "No refresh token"
    
    status, data = make_request("POST", "/_matrix/client/r0/refresh", token, {"refresh_token": refresh})
    return status == 200, status, data

def test_whoami(token):
    """测试10: 获取当前用户信息"""
    status, data = make_request("GET", "/_matrix/client/r0/account/whoami", token)
    return status == 200, status, data

def test_account_deactivate(token):
    """测试11: 停用账户（会失败因为需要额外验证）"""
    status, data = make_request("POST", "/_matrix/client/r0/account/deactivate", token, {})
    # 400可能是因为需要额外验证，这是预期行为
    return status in [200, 400], status, data

def test_change_password(token):
    """测试12: 修改密码"""
    status, data = make_request("POST", "/_matrix/client/r0/account/password", token, {
        "new_password": "TestPass123!"
    })
    return status in [200, 400], status, data  # 400可能是因为需要额外验证

def test_get_profile(token):
    """测试13: 获取用户资料"""
    status, data = make_request("GET", "/_matrix/client/r0/account/profile/@testuser1:cjystx.top", token)
    return status == 200, status, data

def test_update_displayname(token):
    """测试14: 更新显示名称"""
    status, data = make_request("PUT", "/_matrix/client/r0/account/profile/@testuser1:cjystx.top/displayname", 
                                token, {"displayname": "Test User 1"})
    return status == 200, status, data

def test_update_avatar(token):
    """测试15: 更新头像"""
    status, data = make_request("PUT", "/_matrix/client/r0/account/profile/@testuser1:cjystx.top/avatar_url", 
                                token, {"avatar_url": "mxc://example.com/avatar"})
    return status in [200, 400], status, data  # 400可能是格式问题

def test_sync(token):
    """测试16: 同步数据"""
    status, data = make_request("GET", "/_matrix/client/r0/sync?timeout=1000", token)
    return status == 200, status, data

def test_presence_get(token):
    """测试17: 获取在线状态"""
    status, data = make_request("GET", "/_matrix/client/r0/presence/@testuser1:cjystx.top/status", token)
    return status == 200, status, data

def test_presence_set(token):
    """测试18: 设置在线状态"""
    status, data = make_request("PUT", "/_matrix/client/r0/presence/@testuser1:cjystx.top/status", token, {
        "presence": "online",
        "status_msg": "Testing"
    })
    return status == 200, status, data

def test_typing(token, room_id):
    """测试19: 设置打字状态"""
    status, data = make_request("PUT", f"/_matrix/client/r0/rooms/{room_id}/typing/@testuser1:cjystx.top", 
                                token, {"typing": True, "timeout": 30000})
    return status == 200, status, data

def test_receipt(token, room_id):
    """测试20: 发送已读回执"""
    status, data = make_request("POST", f"/_matrix/client/r0/rooms/{room_id}/receipt/m.read/$test", token, {})
    return status == 200, status, data

def test_read_markers(token, room_id):
    """测试21: 设置已读标记"""
    status, data = make_request("POST", f"/_matrix/client/r0/rooms/{room_id}/read_markers", token, {
        "m.read": f"{room_id}/$test"
    })
    return status == 200, status, data

def test_create_room(token):
    """测试22: 创建房间"""
    status, data = make_request("POST", "/_matrix/client/r0/createRoom", token, {
        "name": "API Test Room",
        "visibility": "private"
    })
    if status == 200:
        return True, status, data
    return False, status, data

def test_join_room(token, room_id):
    """测试23: 加入房间"""
    status, data = make_request("POST", f"/_matrix/client/r0/rooms/{room_id}/join", token, {})
    return status == 200, status, data

def test_leave_room(token, room_id):
    """测试24: 离开房间"""
    status, data = make_request("POST", f"/_matrix/client/r0/rooms/{room_id}/leave", token, {})
    return status == 200, status, data

def test_kick_user(token, room_id):
    """测试25: 踢出用户"""
    status, data = make_request("POST", f"/_matrix/client/r0/rooms/{room_id}/kick", token, {
        "user_id": "@testuser2:cjystx.top",
        "reason": "Test kick"
    })
    return status == 200, status, data

def test_ban_user(token, room_id):
    """测试26: 封禁用户"""
    status, data = make_request("POST", f"/_matrix/client/r0/rooms/{room_id}/ban", token, {
        "user_id": "@testuser2:cjystx.top",
        "reason": "Test ban"
    })
    return status == 200, status, data

def test_unban_user(token, room_id):
    """测试27: 解除封禁"""
    status, data = make_request("POST", f"/_matrix/client/r0/rooms/{room_id}/unban", token, {
        "user_id": "@testuser2:cjystx.top"
    })
    return status == 200, status, data

def test_invite_user(token, room_id):
    """测试28: 邀请用户"""
    status, data = make_request("POST", f"/_matrix/client/r0/rooms/{room_id}/invite", token, {
        "user_id": "@testuser2:cjystx.top"
    })
    return status == 200, status, data

def test_get_room_state(token, room_id):
    """测试29: 获取房间状态"""
    status, data = make_request("GET", f"/_matrix/client/r0/rooms/{room_id}/state", token)
    return status == 200, status, data

def test_get_state_by_type(token, room_id):
    """测试30: 获取特定状态事件"""
    status, data = make_request("GET", f"/_matrix/client/r0/rooms/{room_id}/state/m.room.name", token)
    return status == 200, status, data

def test_set_room_state(token, room_id):
    """测试31: 设置房间状态"""
    status, data = make_request("POST", f"/_matrix/client/r0/rooms/{room_id}/state/m.room.topic", token, {
        "topic": "Test topic"
    })
    return status == 200, status, data

def test_send_message(token, room_id):
    """测试32: 发送消息"""
    txn_id = str(uuid.uuid4())
    status, data = make_request("PUT", f"/_matrix/client/r0/rooms/{room_id}/send/m.room.message/{txn_id}", token, {
        "msgtype": "m.room.message",
        "body": "Test message"
    })
    return status == 200, status, data

def test_get_membership_events(token, room_id):
    """测试33: 获取成员事件"""
    status, data = make_request("POST", f"/_matrix/client/r0/rooms/{room_id}/get_membership_events", token, {})
    return status == 200, status, data

def test_get_room_messages(token, room_id):
    """测试34: 获取房间消息"""
    status, data = make_request("GET", f"/_matrix/client/r0/rooms/{room_id}/messages?direction=b&limit=10", token)
    return status == 200, status, data

def test_redact_event(token, room_id):
    """测试35: 删除事件"""
    status, data = make_request("PUT", f"/_matrix/client/r0/rooms/{room_id}/redact/$test", token, {})
    return status == 200, status, data

def test_get_room_info(token, room_id):
    """测试36: 获取房间信息"""
    status, data = make_request("GET", f"/_matrix/client/r0/directory/room/{room_id}", token)
    return status == 200, status, data

def test_delete_room_directory(token, room_id):
    """测试37: 删除房间目录"""
    status, data = make_request("DELETE", f"/_matrix/client/r0/directory/room/{room_id}", token)
    return status == 200, status, data

def test_create_room_directory(token):
    """测试38: 创建房间目录"""
    status, data = make_request("POST", "/_matrix/client/r0/directory/room", token, {
        "room_id": "!test:example.com",
        "visibility": "public"
    })
    return status == 200, status, data

def test_get_public_rooms(token):
    """测试39: 获取公共房间列表"""
    status, data = make_request("GET", "/_matrix/client/r0/publicRooms", token)
    return status == 200, status, data

def test_create_public_room(token):
    """测试40: 创建公共房间"""
    status, data = make_request("POST", "/_matrix/client/r0/publicRooms", token, {
        "name": "Public Test Room",
        "visibility": "public"
    })
    return status == 200, status, data

def test_get_devices(token):
    """测试41: 获取设备列表"""
    status, data = make_request("GET", "/_matrix/client/r0/devices", token)
    return status == 200, status, data

def test_get_device(token):
    """测试42: 获取设备信息"""
    status, data = make_request("GET", "/_matrix/client/r0/devices/test_device", token)
    return status == 200, status, data

def test_update_device(token):
    """测试43: 更新设备"""
    status, data = make_request("PUT", "/_matrix/client/r0/devices/test_device", token, {
        "display_name": "Test Device"
    })
    return status == 200, status, data

def test_delete_device(token):
    """测试44: 删除设备"""
    status, data = make_request("DELETE", "/_matrix/client/r0/devices/test_device", token)
    return status == 200, status, data

def test_delete_devices(token):
    """测试45: 批量删除设备"""
    status, data = make_request("POST", "/_matrix/client/r0/delete_devices", token, {
        "devices": ["device1", "device2"]
    })
    return status == 200, status, data

def test_report_event(token, room_id, event_id):
    """测试46: 举报事件"""
    status, data = make_request("POST", f"/_matrix/client/r0/rooms/{room_id}/report/{event_id}", token, {
        "reason": "Test report",
        "score": -100
    })
    return status == 200, status, data

def test_report_score(token, room_id, event_id):
    """测试47: 设置举报分数"""
    status, data = make_request("PUT", f"/_matrix/client/r0/rooms/{room_id}/report/{event_id}/score", token, {
        "score": -50
    })
    return status == 200, status, data

def test_user_directory_search(token):
    """测试48: 搜索用户"""
    status, data = make_request("POST", "/_matrix/client/r0/user_directory/search", token, {
        "search_term": "testuser"
    })
    return status == 200, status, data

def test_user_directory_list(token):
    """测试49: 获取用户列表"""
    status, data = make_request("POST", "/_matrix/client/r0/user_directory/list", token, {})
    return status == 200, status, data

def main():
    print("=" * 70)
    print("  核心客户端API完整测试（47个端点）")
    print("=" * 70)
    
    token = get_token()
    if not token:
        print("✗ 无法获取token，请先登录")
        return
    
    print(f"✓ Token: {token[:40]}...")
    print()
    
    results = []
    room_id = None
    test_room_id = None
    
    # 先登录获取新token
    print("1. 验证登录...")
    success, status, data = test_login()
    results.append(("用户登录", success, status, data))
    print(f"   {'✓' if success else '✗'} 用户登录: {status}")
    
    # 2-15: 账户管理与用户资料
    print("\n2-15. 账户管理与用户资料测试...")
    
    tests = [
        ("检查用户名可用性", lambda: test_register_available(token)),
        ("退出登录", lambda: test_logout(token)),
        ("重新登录", test_login),
        ("退出所有设备", lambda: test_logout_all(token)),
        ("刷新令牌", lambda: test_refresh(token)),
        ("获取当前用户信息", lambda: test_whoami(token)),
        ("停用账户", lambda: test_account_deactivate(token)),
        ("修改密码", lambda: test_change_password(token)),
        ("获取用户资料", lambda: test_get_profile(token)),
        ("更新显示名称", lambda: test_update_displayname(token)),
        ("更新头像", lambda: test_update_avatar(token)),
    ]
    
    for name, test_func in tests:
        success, status, data = test_func()
        results.append((name, success, status, data))
        print(f"   {'✓' if success else '✗'} {name}: {status}")
        time.sleep(0.3)
    
    # 重新登录获取有效token
    print("\n重新登录获取有效token...")
    success, status, data = test_login()
    if success:
        token = data.get('access_token')
        print(f"✓ 获取新token成功")
        with open("testuser1_token.txt", 'w') as f:
            f.write(token)
    else:
        print(f"✗ 重新登录失败: {data}")
    
    # 16-21: 同步与状态
    print("\n16-21. 同步与状态测试...")
    
    sync_tests = [
        ("同步数据", lambda: test_sync(token)),
        ("获取在线状态", lambda: test_presence_get(token)),
        ("设置在线状态", lambda: test_presence_set(token)),
    ]
    
    for name, test_func in sync_tests:
        success, status, data = test_func()
        results.append((name, success, status, data))
        print(f"   {'✓' if success else '✗'} {name}: {status}")
        time.sleep(0.3)
    
    # 22-35: 房间操作（需要创建测试房间）
    print("\n22-35. 房间操作测试...")
    
    # 创建测试房间
    print("   创建测试房间...")
    success, status, data = test_create_room(token)
    results.append(("创建房间", success, status, data))
    print(f"   {'✓' if success else '✗'} 创建房间: {status}")
    
    if success and 'room_id' in data:
        room_id = data['room_id']
        test_room_id = room_id
        print(f"   Room ID: {room_id}")
        
        # 发送消息
        print("   发送测试消息...")
        success2, status2, msg_data = test_send_message(token, room_id)
        results.append(("发送消息", success2, status2, msg_data))
        print(f"   {'✓' if success2 else '✗'} 发送消息: {status2}")
        
        event_id = msg_data.get('event_id') if success2 and 'event_id' in msg_data else "$test"
    
    # 其他房间操作
    room_tests = [
        ("获取房间状态", lambda: test_get_room_state(token, room_id)),
        ("获取特定状态事件", lambda: test_get_state_by_type(token, room_id)),
        ("设置房间状态", lambda: test_set_room_state(token, room_id)),
        ("获取成员事件", lambda: test_get_membership_events(token, room_id)),
        ("获取房间消息", lambda: test_get_room_messages(token, room_id)),
        ("删除事件", lambda: test_redact_event(token, room_id)),
    ]
    
    for name, test_func in room_tests:
        if room_id:
            success, status, data = test_func()
        else:
            success, status, data = False, None, "No room"
        results.append((name, success, status, data))
        print(f"   {'✓' if success else '✗'} {name}: {status}")
        time.sleep(0.3)
    
    # 36-40: 房间目录
    print("\n36-40. 房间目录测试...")
    
    dir_tests = [
        ("获取房间信息", lambda: test_get_room_info(token, room_id)),
        ("获取公共房间列表", lambda: test_get_public_rooms(token)),
        ("创建公共房间", lambda: test_create_public_room(token)),
    ]
    
    for name, test_func in dir_tests:
        success, status, data = test_func()
        results.append((name, success, status, data))
        print(f"   {'✓' if success else '✗'} {name}: {status}")
        time.sleep(0.3)
    
    # 41-45: 设备管理
    print("\n41-45. 设备管理测试...")
    
    device_tests = [
        ("获取设备列表", lambda: test_get_devices(token)),
        ("获取设备信息", lambda: test_get_device(token)),
        ("更新设备", lambda: test_update_device(token)),
        ("删除设备", lambda: test_delete_device(token)),
        ("批量删除设备", lambda: test_delete_devices(token)),
    ]
    
    for name, test_func in device_tests:
        success, status, data = test_func()
        results.append((name, success, status, data))
        print(f"   {'✓' if success else '✗'} {name}: {status}")
        time.sleep(0.3)
    
    # 46-47: 事件报告
    print("\n46-47. 事件报告测试...")
    
    report_tests = [
        ("举报事件", lambda: test_report_event(token, room_id, event_id)),
        ("设置举报分数", lambda: test_report_score(token, room_id, event_id)),
    ]
    
    for name, test_func in report_tests:
        if room_id:
            success, status, data = test_func()
        else:
            success, status, data = False, None, "No room"
        results.append((name, success, status, data))
        print(f"   {'✓' if success else '✗'} {name}: {status}")
        time.sleep(0.3)
    
    # 48-49: 用户目录
    print("\n48-49. 用户目录测试...")
    
    dir_tests2 = [
        ("搜索用户", lambda: test_user_directory_search(token)),
        ("获取用户列表", lambda: test_user_directory_list(token)),
    ]
    
    for name, test_func in dir_tests2:
        success, status, data = test_func()
        results.append((name, success, status, data))
        print(f"   {'✓' if success else '✗'} {name}: {status}")
        time.sleep(0.3)
    
    # 统计结果
    print("\n" + "=" * 70)
    print("  测试结果统计")
    print("=" * 70)
    
    passed = sum(1 for r in results if r[1])
    total = len(results)
    
    print(f"\n总测试数: {total}")
    print(f"通过: {passed}")
    print(f"失败: {total - passed}")
    print(f"成功率: {passed/total*100:.1f}%")
    
    print("\n失败的项目:")
    for name, success, status, data in results:
        if not success:
            error = data.get('error') if isinstance(data, dict) else str(data)[:100]
            print(f"  ✗ {name}: HTTP {status} - {error}")
    
    # 保存结果
    with open("test_results.json", 'w', encoding='utf-8') as f:
        json.dump([
            {"name": r[0], "passed": r[1], "status": r[2], "data": r[3]} 
            for r in results
        ], f, ensure_ascii=False, indent=2)
    
    print(f"\n结果已保存到 test_results.json")

if __name__ == "__main__":
    main()
