# Synapse Rust Matrix Server API Reference

## 1. 概述

本文档描述了 Synapse Rust Matrix 服务器实现的 API 端点。所有 API 均遵循 Matrix 客户端-服务器协议规范。

### 服务器信息
- **服务器地址**: `http://localhost:8008`
- **版本**: 0.1.0
- **文档版本**: 3.0
- **最后更新**: 2026-02-11

### API 分类
- 核心客户端 API: 用户认证、房间管理、消息操作等
- 管理员 API: 服务器管理、用户管理、房间管理等
- 联邦 API: 服务器间通信
- 好友系统 API: 基于 Matrix 房间的好友管理 (新)
- 端到端加密 API: E2EE 相关功能
- 媒体文件 API: 媒体上传下载
- 语音消息 API: 语音消息处理
- 密钥备份 API: 密钥备份管理

---

## 2. 核心客户端 API

### 2.1 健康检查与版本

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 1 | `/` | GET | 服务器欢迎信息 |
| 2 | `/health` | GET | 服务健康检查 |
| 3 | `/_matrix/client/versions` | GET | 获取客户端 API 版本 |
| 4 | `/_matrix/client/r0/version` | GET | 获取服务端版本 |

### 2.2 用户注册与认证

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 5 | `/_matrix/client/r0/register/available` | GET | 检查用户名可用性 |
| 6 | `/_matrix/client/r0/register/email/requestToken` | POST | 请求邮箱验证 |
| 7 | `/_matrix/client/r0/register/email/submitToken` | POST | 提交邮箱验证 Token |
| 8 | `/_matrix/client/r0/register` | POST | 用户注册 |
| 9 | `/_matrix/client/r0/login` | POST | 用户登录 |
| 10 | `/_matrix/client/r0/logout` | POST | 退出登录 |
| 11 | `/_matrix/client/r0/logout/all` | POST | 退出所有设备 |
| 12 | `/_matrix/client/r0/refresh` | POST | 刷新令牌 |

### 2.3 账户管理

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 13 | `/_matrix/client/r0/account/whoami` | GET | 获取当前用户信息 |
| 14 | `/_matrix/client/r0/account/deactivate` | POST | 停用账户 |
| 15 | `/_matrix/client/r0/account/password` | POST | 修改密码 |
| 16 | `/_matrix/client/r0/account/profile/{user_id}` | GET | 获取用户资料 |
| 17 | `/_matrix/client/r0/account/profile/{user_id}/displayname` | PUT | 更新显示名称 |
| 18 | `/_matrix/client/r0/account/profile/{user_id}/avatar_url` | PUT | 更新头像 |

### 2.4 用户目录

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 19 | `/_matrix/client/r0/user_directory/search` | POST | 搜索用户 |
| 20 | `/_matrix/client/r0/user_directory/list` | POST | 获取用户列表 |

### 2.5 设备管理

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 21 | `/_matrix/client/r0/devices` | GET | 获取设备列表 |
| 22 | `/_matrix/client/r0/devices/{device_id}` | GET | 获取设备信息 |
| 23 | `/_matrix/client/r0/devices/{device_id}` | PUT | 更新设备 |
| 24 | `/_matrix/client/r0/devices/{device_id}` | DELETE | 删除设备 |
| 25 | `/_matrix/client/r0/delete_devices` | POST | 批量删除设备 |

### 2.6 在线状态

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 26 | `/_matrix/client/r0/presence/{user_id}/status` | GET | 获取在线状态 |
| 27 | `/_matrix/client/r0/presence/{user_id}/status` | PUT | 设置在线状态 |

### 2.7 同步与状态

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 28 | `/_matrix/client/r0/sync` | GET | 同步数据 |
| 29 | `/_matrix/client/r0/rooms/{room_id}/typing/{user_id}` | PUT | 设置打字状态 |
| 30 | `/_matrix/client/r0/rooms/{room_id}/receipt/{receipt_type}/{event_id}` | POST | 发送已读回执 |
| 31 | `/_matrix/client/r0/rooms/{room_id}/read_markers` | POST | 设置已读标记 |

### 2.8 房间管理

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 32 | `/_matrix/client/r0/createRoom` | POST | 创建房间 |
| 33 | `/_matrix/client/r0/rooms/{room_id}/join` | POST | 加入房间 |
| 34 | `/_matrix/client/r0/rooms/{room_id}/leave` | POST | 离开房间 |
| 35 | `/_matrix/client/r0/rooms/{room_id}/kick` | POST | 踢出用户 |
| 36 | `/_matrix/client/r0/rooms/{room_id}/ban` | POST | 封禁用户 |
| 37 | `/_matrix/client/r0/rooms/{room_id}/unban` | POST | 解除封禁 |
| 38 | `/_matrix/client/r0/rooms/{room_id}/invite` | POST | 邀请用户 |

### 2.9 房间状态与消息

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 39 | `/_matrix/client/r0/rooms/{room_id}/state` | GET | 获取房间状态 |
| 40 | `/_matrix/client/r0/rooms/{room_id}/state/{event_type}` | GET | 获取特定状态事件 |
| 41 | `/_matrix/client/r0/rooms/{room_id}/state/{event_type}/{state_key}` | GET | 获取状态事件 |
| 42 | `/_matrix/client/r0/rooms/{room_id}/state/{event_type}/{state_key}` | PUT | 设置房间状态 |
| 43 | `/_matrix/client/r0/rooms/{room_id}/state/{event_type}` | POST | 设置房间状态 |
| 44 | `/_matrix/client/r0/rooms/{room_id}/send/{event_type}/{txn_id}` | PUT | 发送事件/消息 |
| 45 | `/_matrix/client/r0/rooms/{room_id}/messages` | GET | 获取房间消息 |
| 46 | `/_matrix/client/r0/rooms/{room_id}/members` | GET | 获取房间成员 |
| 47 | `/_matrix/client/r0/rooms/{room_id}/get_membership_events` | POST | 获取成员事件 |
| 48 | `/_matrix/client/r0/rooms/{room_id}/redact/{event_id}` | PUT | 删除事件 |

### 2.10 房间目录

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 49 | `/_matrix/client/r0/directory/room/{room_id}` | GET | 获取房间信息 |
| 50 | `/_matrix/client/r0/directory/room/{room_id}` | DELETE | 删除房间目录 |
| 51 | `/_matrix/client/r0/directory/room/{param}` | GET | 获取房间目录 |
| 52 | `/_matrix/client/r0/directory/room/{param}` | PUT | 创建房间目录 |
| 53 | `/_matrix/client/r0/directory/room/{param}` | DELETE | 删除房间目录 |
| 54 | `/_matrix/client/r0/directory/room/alias/{room_alias}` | GET | 通过别名获取房间 |
| 55 | `/_matrix/client/r0/directory/room/{room_id}/alias` | GET | 获取房间别名 |
| 56 | `/_matrix/client/r0/directory/room/{room_id}/alias/{room_alias}` | PUT | 设置房间别名 |
| 57 | `/_matrix/client/r0/directory/room/{room_id}/alias/{room_alias}` | DELETE | 删除房间别名 |
| 58 | `/_matrix/client/r0/publicRooms` | GET | 获取公共房间列表 |
| 59 | `/_matrix/client/r0/publicRooms` | POST | 创建公共房间 |

### 2.11 用户房间

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 60 | `/_matrix/client/r0/user/{user_id}/rooms` | GET | 获取用户房间列表 |

### 2.12 事件举报

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 61 | `/_matrix/client/r0/rooms/{room_id}/report/{event_id}` | POST | 举报事件 |
| 62 | `/_matrix/client/r0/rooms/{room_id}/report/{event_id}/score` | PUT | 设置举报分数 |

---

## 3. 管理员 API

> 所有管理员 API 需要管理员认证。

### 3.1 服务器信息

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 1 | `/_synapse/admin/v1/server_version` | GET | 获取服务器版本 |
| 2 | `/_synapse/admin/v1/status` | GET | 获取服务器状态 |
| 3 | `/_synapse/admin/v1/server_stats` | GET | 获取服务器统计 |
| 4 | `/_synapse/admin/v1/config` | GET | 获取服务器配置 |
| 5 | `/_synapse/admin/v1/user_stats` | GET | 获取用户统计 |
| 6 | `/_synapse/admin/v1/media_stats` | GET | 获取媒体统计 |
| 7 | `/_synapse/admin/v1/logs` | GET | 获取服务器日志 |

### 3.2 用户管理

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 8 | `/_synapse/admin/v1/users` | GET | 获取用户列表 |
| 9 | `/_synapse/admin/v1/users/{user_id}` | GET | 获取用户信息 |
| 10 | `/_synapse/admin/v1/users/{user_id}` | DELETE | 删除用户 |
| 11 | `/_synapse/admin/v1/users/{user_id}/admin` | PUT | 设置管理员 |
| 12 | `/_synapse/admin/v1/users/{user_id}/deactivate` | POST | 停用用户 |
| 13 | `/_synapse/admin/v1/users/{user_id}/rooms` | GET | 获取用户房间 |
| 14 | `/_synapse/admin/v1/users/{user_id}/password` | POST | 重置用户密码 |
| 15 | `/_synapse/admin/v1/register/nonce` | GET | 获取注册 nonce |
| 16 | `/_synapse/admin/v1/register` | POST | 管理员注册 |

### 3.3 房间管理

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 17 | `/_synapse/admin/v1/rooms` | GET | 获取房间列表 |
| 18 | `/_synapse/admin/v1/rooms/{room_id}` | GET | 获取房间信息 |
| 19 | `/_synapse/admin/v1/rooms/{room_id}` | DELETE | 删除房间 |
| 20 | `/_synapse/admin/v1/rooms/{room_id}/delete` | POST | 删除房间（官方API） |
| 21 | `/_synapse/admin/v1/purge_history` | POST | 清理历史 |
| 22 | `/_synapse/admin/v1/shutdown_room` | POST | 关闭房间 |

### 3.4 安全相关

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 23 | `/_synapse/admin/v1/security/events` | GET | 获取安全事件 |
| 24 | `/_synapse/admin/v1/security/ip/blocks` | GET | 获取IP阻止列表 |
| 25 | `/_synapse/admin/v1/security/ip/block` | POST | 阻止IP |
| 26 | `/_synapse/admin/v1/security/ip/unblock` | POST | 解除IP阻止 |
| 27 | `/_synapse/admin/v1/security/ip/reputation/{ip}` | GET | 获取IP信誉 |

---

## 4. 联邦通信 API

### 4.1 密钥与发现 (无需签名)

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 1 | `/_matrix/federation/v2/server` | GET | 获取服务器密钥 |
| 2 | `/_matrix/key/v2/server` | GET | 获取服务器密钥 |
| 3 | `/_matrix/federation/v2/query/{server_name}/{key_id}` | GET | 查询密钥 |
| 4 | `/_matrix/key/v2/query/{server_name}/{key_id}` | GET | 查询密钥 |
| 5 | `/_matrix/federation/v1/version` | GET | 获取联邦版本 |
| 6 | `/_matrix/federation/v1` | GET | 联邦发现 |
| 7 | `/_matrix/federation/v1/publicRooms` | GET | 获取公共房间 |
| 8 | `/_matrix/federation/v1/query/destination` | GET | 查询目标服务器 |
| 9 | `/_matrix/federation/v1/room/{room_id}/{event_id}` | GET | 获取房间事件 |

### 4.2 房间操作 (需要签名)

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 10 | `/_matrix/federation/v1/send/{txn_id}` | PUT | 发送事务 |
| 11 | `/_matrix/federation/v1/make_join/{room_id}/{user_id}` | GET | 生成加入模板 |
| 12 | `/_matrix/federation/v1/make_leave/{room_id}/{user_id}` | GET | 生成离开模板 |
| 13 | `/_matrix/federation/v1/send_join/{room_id}/{event_id}` | PUT | 发送加入 |
| 14 | `/_matrix/federation/v1/send_leave/{room_id}/{event_id}` | PUT | 发送离开 |
| 15 | `/_matrix/federation/v2/invite/{room_id}/{event_id}` | PUT | 邀请 (v2) |
| 16 | `/_matrix/federation/v1/invite/{room_id}/{event_id}` | PUT | 邀请 |
| 17 | `/_matrix/federation/v1/get_missing_events/{room_id}` | POST | 获取缺失事件 |
| 18 | `/_matrix/federation/v1/get_event_auth/{room_id}/{event_id}` | GET | 获取事件授权 |
| 19 | `/_matrix/federation/v1/state/{room_id}` | GET | 获取房间状态 |
| 20 | `/_matrix/federation/v1/event/{event_id}` | GET | 获取事件 |
| 21 | `/_matrix/federation/v1/state_ids/{room_id}` | GET | 获取状态ID |
| 22 | `/_matrix/federation/v1/query/directory/room/{room_id}` | GET | 房间目录查询 |
| 23 | `/_matrix/federation/v1/query/profile/{user_id}` | GET | 用户资料查询 |
| 24 | `/_matrix/federation/v1/backfill/{room_id}` | GET | 回填事件 |
| 25 | `/_matrix/federation/v1/keys/claim` | POST | 声明密钥 |
| 26 | `/_matrix/federation/v1/keys/upload` | POST | 上传密钥 |
| 27 | `/_matrix/federation/v2/key/clone` | POST | 克隆密钥 |
| 28 | `/_matrix/federation/v2/user/keys/query` | POST | 查询用户密钥 |
| 29 | `/_matrix/federation/v1/members/{room_id}` | GET | 获取房间成员 |
| 30 | `/_matrix/federation/v1/members/{room_id}/joined` | GET | 获取成员状态 |
| 31 | `/_matrix/federation/v1/user/devices/{user_id}` | GET | 用户设备查询 |
| 32 | `/_matrix/federation/v1/room_auth/{room_id}` | GET | 房间认证 |
| 33 | `/_matrix/federation/v1/knock/{room_id}/{user_id}` | GET | 敲门 |
| 34 | `/_matrix/federation/v1/thirdparty/invite` | POST | 第三方邀请 |
| 35 | `/_matrix/federation/v1/get_joining_rules/{room_id}` | GET | 获取加入规则 |

### 4.3 好友系统联邦 (需要签名)

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 36 | `/_matrix/federation/v1/friends/query/{user_id}` | GET | 查询用户好友列表 |
| 37 | `/_matrix/federation/v1/friends/relationship/{user_id}/{friend_id}` | GET | 验证好友关系 |
| 38 | `/_matrix/federation/v1/friends/request` | POST | 发送跨服务器好友请求 |
| 39 | `/_matrix/federation/v1/friends/accept/{request_id}` | POST | 接受跨服务器好友请求 |

---

## 5. 好友系统 API (新)

> 好友系统已完全重构为基于 Matrix 房间的实现。

### 5.1 好友管理 (新 API)

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 1 | `/_matrix/client/v1/friends/room` | GET | 获取好友列表房间 ID |
| 2 | `/_matrix/client/v1/friends` | GET | 获取好友列表 |
| 3 | `/_matrix/client/v1/friends` | DELETE | 删除好友 |
| 4 | `/_matrix/client/v1/friends/request` | POST | 发送好友请求 |
| 5 | `/_matrix/client/v1/friends/requests` | GET | 获取待处理好友请求 |
| 6 | `/_matrix/client/v1/friends/request/{request_id}/accept` | POST | 接受好友请求 |
| 7 | `/_matrix/client/v1/friends/request/{request_id}/decline` | POST | 拒绝好友请求 |
| 8 | `/_matrix/client/v1/friends/dm/{user_id}` | GET | 获取与好友的私信房间 |
| 9 | `/_matrix/client/v1/friends/dm/{user_id}` | POST | 创建与好友的私信房间 |
| 10 | `/_matrix/client/v1/friends/check/{user_id}` | GET | 检查是否为好友 |

### 5.2 旧 API (已废弃)

> 以下端点已废弃，返回 410 Gone 响应：

| 序号 | 端点 | 方法 | 新端点 |
|------|------|------|--------|
| - | `/_synapse/enhanced/friends/search` | GET | 使用 `/_matrix/client/r0/user_directory/search` |
| - | `/_synapse/enhanced/friends` | GET | `/_matrix/client/v1/friends` |
| - | `/_synapse/enhanced/friend/request` | GET/POST | `/_matrix/client/v1/friends/request` |
| - | `/_synapse/enhanced/friend/requests` | GET | `/_matrix/client/v1/friends/requests` |
| - | `/_synapse/enhanced/friends/categories/*` | - | 使用 Matrix Spaces |
| - | `/_synapse/enhanced/friends/suggestions` | GET | 使用用户目录 |

### 5.3 技术实现

好友系统使用 Matrix 房间机制实现：

1. **好友列表房间**: `!friends:user_id:server.com`
   - 使用 `m.friends.list` 状态事件存储好友关系

2. **私信房间**: `!dm:user1_user2:server.com`
   - 使用 `m.friends.related_users` 事件标记私信关系
   - 支持 `m.private` 类型实现私密聊天

3. **好友请求**: `m.friend.request` 事件
   - 存储在好友列表房间中
   - 包含状态：pending/accepted/declined

---

## 6. 端到端加密 API

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 1 | `/_matrix/client/r0/keys/upload` | POST | 上传设备密钥和一次性密钥 |
| 2 | `/_matrix/client/r0/keys/query` | POST | 查询设备密钥 |
| 3 | `/_matrix/client/r0/keys/claim` | POST | 声明一次性密钥 |
| 4 | `/_matrix/client/r0/keys/changes` | GET | 获取密钥变更通知 |
| 5 | `/_matrix/client/r0/rooms/{room_id}/keys/distribution` | GET | 获取房间备份密钥 |
| 6 | `/_matrix/client/r0/sendToDevice/{event_type}/{txn_id}` | PUT | 发送设备到设备消息 |

---

## 7. 媒体文件 API

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 1 | `/_matrix/media/v3/upload/{server_name}/{media_id}` | POST | 上传媒体 |
| 2 | `/_matrix/media/v3/download/{server_name}/{media_id}` | GET | 下载媒体 |
| 3 | `/_matrix/media/v3/thumbnail/{server_name}/{media_id}` | GET | 获取缩略图 |
| 4 | `/_matrix/media/v1/config` | GET | 获取配置 |
| 5 | `/_matrix/media/v1/download/{server_name}/{media_id}` | GET | 下载（v1） |
| 6 | `/_matrix/media/r1/download/{server_name}/{media_id}` | GET | 下载（r1） |

---

## 8. 语音消息 API

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 1 | `/_matrix/client/r0/voice/config` | GET | 获取语音配置 |
| 2 | `/_matrix/client/r0/voice/upload` | POST | 上传语音消息 |
| 3 | `/_matrix/client/r0/voice/convert` | POST | 语音格式转换 |
| 4 | `/_matrix/client/r0/voice/optimize` | POST | 语音优化 |
| 5 | `/_matrix/client/r0/voice/stats` | GET | 获取语音统计 |
| 6 | `/_matrix/client/r0/voice/{message_id}` | GET | 获取语音消息 |
| 7 | `/_matrix/client/r0/voice/{message_id}` | DELETE | 删除语音消息 |
| 8 | `/_matrix/client/r0/voice/user/{user_id}` | GET | 获取用户语音 |
| 9 | `/_matrix/client/r0/voice/room/{room_id}` | GET | 获取房间语音 |
| 10 | `/_matrix/client/r0/voice/user/{user_id}/stats` | GET | 获取用户语音统计 |

---

## 9. 密钥备份 API

| 序号 | 端点 | 方法 | 描述 |
|------|------|------|------|
| 1 | `/_matrix/client/r0/room_keys/version` | GET | 获取备份版本 |
| 2 | `/_matrix/client/r0/room_keys/version` | POST | 创建备份版本 |
| 3 | `/_matrix/client/r0/room_keys/version/{version}` | GET | 获取特定备份版本 |
| 4 | `/_matrix/client/r0/room_keys/version/{version}` | PUT | 更新备份版本 |
| 5 | `/_matrix/client/r0/room_keys/version/{version}` | DELETE | 删除备份版本 |
| 6 | `/_matrix/client/r0/room_keys/{version}` | GET | 获取房间密钥 |
| 7 | `/_matrix/client/r0/room_keys/{version}` | PUT | 上传房间密钥 |

---

## 10. API 统计

| 分类 | 端点数量 |
|------|---------|
| 核心客户端 API | 62 |
| 管理员 API | 27 |
| 联邦通信 API | 39 |
| 好友系统 API (新) | 10 |
| 好友系统 API (旧) | 15 (已废弃) |
| 端到端加密 API | 6 |
| 媒体文件 API | 6 |
| 语音消息 API | 10 |
| 密钥备份 API | 7 |
| **总计** | **182** |

---

## 11. 更新日志

### 2026-02-11 (v3.0)
- ✅ 完全重写 API 参考文档，基于实际代码实现
- ✅ 更新好友系统 API：新 `/_matrix/client/v1/friends/*` 端点
- ✅ 标记旧 `/_synapse/enhanced/friends/*` 端点为已废弃 (410 Gone)
- ✅ 添加联邦好友系统 API 端点
- ✅ 更新 API 统计总数

### 之前版本
- 详见 friend_system_optimization.md 中的完成记录
