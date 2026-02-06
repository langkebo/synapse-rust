# Matrix API 错误汇总

本文档记录Matrix API测试过程中遇到的错误及其解决方案。

## 测试状态概览

| 模块 | 测试数量 | 通过 | 失败 | 通过率 | 测试日期 |
|------|----------|------|------|--------|----------|
| 好友系统API | 13 | 13 | 0 | 100% | 2026-02-06 |
| 媒体文件API | 8 | 8 | 0 | 100% | 2026-02-06 |
| 私聊增强API | 9 | 9 | 0 | 100% | 2026-02-06 |
| 密钥备份API | 9 | 9 | 0 | 100% | 2026-02-06 |
| **总计** | **39** | **39** | **0** | **100%** | - |

---

## 1. 好友系统API（13/13 PASS）

### 测试结果汇总

| 序号 | 测试项目 | 端点 | 方法 | 状态码 | 结果 |
|------|----------|------|------|--------|------|
| 1 | 搜索用户 | `/_synapse/enhanced/friends/search` | GET | 200 | ✅ PASS |
| 2 | 获取好友列表 | `/_synapse/enhanced/friends` | GET | 200 | ✅ PASS |
| 3 | 发送好友请求 | `/_synapse/enhanced/friend/request` | POST | 200 | ✅ PASS |
| 4 | 获取好友请求 | `/_synapse/enhanced/friend/requests` | GET | 200 | ✅ PASS |
| 5 | 接受好友请求 | `/_synapse/enhanced/friend/request/{id}/accept` | POST | 200 | ✅ PASS |
| 6 | 阻止用户 | `/_synapse/enhanced/friend/blocks/{user_id}` | POST | 200 | ✅ PASS |
| 7 | 获取阻止列表 | `/_synapse/enhanced/friend/blocks/{user_id}` | GET | 200 | ✅ PASS |
| 8 | 解除阻止 | `/_synapse/enhanced/friend/blocks/{user_id}/{blocked_id}` | DELETE | 200 | ✅ PASS |
| 9 | 创建好友分类 | `/_synapse/enhanced/friend/categories/{user_id}` | POST | 200 | ✅ PASS |
| 10 | 获取好友分类 | `/_synapse/enhanced/friend/categories/{user_id}` | GET | 200 | ✅ PASS |
| 11 | 更新好友分类 | `/_synapse/enhanced/friend/categories/{user_id}/{name}` | PUT | 200 | ✅ PASS |
| 12 | 删除好友分类 | `/_synapse/enhanced/friend/categories/{user_id}/{name}` | DELETE | 200 | ✅ PASS |
| 13 | 拒绝好友请求 | `/_synapse/enhanced/friend/request/{id}/decline` | POST | 200 | ✅ PASS |

### 测试用户
- **测试账号**: testuser3 (@testuser3:cjystx.top)
- **测试密码**: TestUser123!

### 测试报告位置
- `/home/hula/synapse_rust/friend_api_test_report.json`

---

## 2. 媒体文件API（8/8 PASS）

### 测试结果汇总

| 序号 | 测试项目 | 端点 | 方法 | 状态码 | 结果 |
|------|----------|------|------|--------|------|
| 1 | 上传媒体文件(v3) | `/_matrix/media/v3/upload` | POST | 200 | ✅ PASS |
| 2 | 上传媒体文件(v1) | `/_matrix/media/v1/upload` | POST | 200 | ✅ PASS |
| 3 | 下载媒体文件 | `/_matrix/media/v3/download/{server}/{media_id}` | GET | 200 | ✅ PASS |
| 4 | 下载媒体文件(v1) | `/_matrix/media/v1/download/{server}/{media_id}` | GET | 200 | ✅ PASS |
| 5 | 获取缩略图 | `/_matrix/media/v3/thumbnail/{server}/{media_id}` | GET | 200 | ✅ PASS |
| 6 | 获取媒体配置 | `/_matrix/media/v1/config` | GET | 200 | ✅ PASS |
| 7 | 数组格式上传 | `/_matrix/media/v3/upload` | POST | 200 | ✅ PASS |
| 8 | 无文件名上传 | `/_matrix/media/v3/upload` | POST | 200 | ✅ PASS |

### 测试用户
- **测试账号**: admin (@admin:cjystx.top)
- **测试密码**: Wzc9890951!

### 测试报告位置
- `/home/hula/synapse_rust/media_api_test_report.json`

---

## 3. 私聊增强API（9/9 PASS）

### 测试结果汇总

| 序号 | 测试项目 | 端点 | 方法 | 状态码 | 结果 |
|------|----------|------|------|--------|------|
| 1 | 创建私聊会话 | `/_synapse/enhanced/private/sessions` | POST | 200 | ✅ PASS |
| 2 | 获取会话列表 | `/_synapse/enhanced/private/sessions` | GET | 200 | ✅ PASS |
| 3 | 获取会话详情 | `/_synapse/enhanced/private/sessions/{id}` | GET | 200 | ✅ PASS |
| 4 | 发送会话消息 | `/_synapse/enhanced/private/sessions/{id}/messages` | POST | 200 | ✅ PASS |
| 5 | 获取会话消息 | `/_synapse/enhanced/private/sessions/{id}/messages` | GET | 200 | ✅ PASS |
| 6 | 删除会话 | `/_synapse/enhanced/private/sessions/{id}` | DELETE | 200 | ✅ PASS |
| 7 | 获取未读数 | `/_synapse/enhanced/private/unread-count` | GET | 200 | ✅ PASS |
| 8 | 搜索消息 | `/_synapse/enhanced/private/search` | POST | 200 | ✅ PASS |
| 9 | 创建DM房间 | `/_matrix/client/r0/createDM` | POST | 200 | ✅ PASS |

### 测试用户
- **测试账号**: testuser3 (@testuser3:cjystx.top)
- **测试密码**: TestUser123!

### 测试报告位置
- `/home/hula/synapse_rust/private_chat_api_test_report.json`

---

## 4. 密钥备份API（9/9 PASS）

### 测试结果汇总

| 序号 | 测试项目 | 端点 | 方法 | 状态码 | 结果 |
|------|----------|------|------|--------|------|
| 1 | 创建备份版本 | `/_matrix/client/r0/room_keys/version` | POST | 200 | ✅ PASS |
| 2 | 获取备份版本 | `/_matrix/client/r0/room_keys/version/{version}` | GET | 200 | ✅ PASS |
| 3 | 更新备份版本 | `/_matrix/client/r0/room_keys/version/{version}` | PUT | 200 | ✅ PASS |
| 4 | 删除备份版本 | `/_matrix/client/r0/room_keys/version/{version}` | DELETE | 200 | ✅ PASS |
| 5 | 获取所有密钥 | `/_matrix/client/r0/room_keys/{version}` | GET | 200 | ✅ PASS |
| 6 | 上传密钥 | `/_matrix/client/r0/room_keys/{version}` | PUT | 200 | ✅ PASS |
| 7 | 批量上传密钥 | `/_matrix/client/r0/room_keys/{version}/keys` | POST | 200 | ✅ PASS |
| 8 | 获取房间密钥 | `/_matrix/client/r0/room_keys/{version}/keys/{room_id}` | GET | 200 | ✅ PASS |
| 9 | 获取会话密钥 | `/_matrix/client/r0/room_keys/{version}/keys/{room_id}/{session_id}` | GET | 200 | ✅ PASS |

### 测试用户
- **测试账号**: admin (@admin:cjystx.top)
- **测试密码**: Wzc9890951!

### 测试报告位置
- `/home/hula/synapse_rust/key_backup_api_test_report.json`

---

## 历史已修复错误

### 1. 语音消息API错误（已修复）

#### 错误1：NULL约束违规
```
Error: null value in column 'room_id' of relation 'voice_usage_stats' violates not-null constraint
```
**原因**: voice_usage_stats表不允许room_id为NULL，但语音消息没有房间ID
**解决方案**: 修改表结构，允许room_id为NULL
**修复文件**: migrations/20260206000004_fix_voice_usage_stats_room_id.sql

#### 错误2：数据类型不匹配
```
Error: mismatched types; Rust type 'i32' (as SQL type 'INT4') is not compatible with SQL type 'INT8'
```
**原因**: Rust代码使用i32但数据库使用INT8
**解决方案**: 更新UserVoiceStats结构体，将total_duration_ms和message_count改为i64类型

---

### 2. 测试账号认证错误（已修复）

#### 错误：认证失败
```
Error: {"errcode":"M_UNAUTHORIZED","error":"Invalid credentials"}
```
**原因**: 使用的测试账号(testuser1)密码不正确或账户不存在
**解决方案**: 
1. 使用管理员账号(@admin:cjystx.top)进行API测试
2. 或注册新的测试用户

---

### 3. 密钥备份API格式错误（已修复）

#### 错误：sessions格式不正确
```
Error: sessions字段期望数组格式，但发送的是对象格式
```
**原因**: API期望的格式：
```json
{
  "room_id": "!room:example.com",
  "sessions": [
    {
      "session_id": "session_001",
      "first_message_index": 0,
      ...
    }
  ]
}
```
但测试脚本发送的是对象格式
**解决方案**: 修改测试脚本，将sessions改为数组格式

---

## 测试脚本使用方法

### 运行所有API测试

```bash
# 运行好友系统API测试
python3 /home/hula/synapse_rust/test_friend_api_complete.py

# 运行媒体文件API测试
python3 /home/hula/synapse_rust/test_media_api_complete.py

# 运行私聊增强API测试
python3 /home/hula/synapse_rust/test_private_chat_api_complete.py

# 运行密钥备份API测试
python3 /home/hula/synapse_rust/test_key_backup_api.py
```

### 查看测试报告

```bash
# 查看JSON格式报告
cat /home/hula/synapse_rust/friend_api_test_report.json | python3 -m json.tool
cat /home/hula/synapse_rust/media_api_test_report.json | python3 -m json.tool
cat /home/hula/synapse_rust/key_backup_api_test_report.json | python3 -m json.tool
```

---

## 更新日志

### 2026-02-06
- ✅ 完成好友系统API测试 (13/13 PASS)
- ✅ 完成媒体文件API测试 (8/8 PASS)
- ✅ 完成私聊增强API测试 (9/9 PASS)
- ✅ 完成密钥备份API测试 (9/9 PASS)
- ✅ 总体通过率: 100% (39/39)
- ✅ 修复密钥备份API sessions格式问题
- ✅ 更新api-error.md文档

### 2026-02-05
- 🔧 修复语音消息API NULL约束问题
- 🔧 修复数据类型不匹配问题
- 🔧 修复测试账号认证问题
