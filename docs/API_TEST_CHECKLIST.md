# API 端点测试清单

## 一、客户端 API

### 1.1 认证端点

| 端点 | 方法 | 描述 | 测试状态 |
|------|------|------|----------|
| `/_matrix/client/versions` | GET | 获取服务器版本 | ✅ |
| `/_matrix/client/r0/login` | GET | 获取登录流程 | ✅ |
| `/_matrix/client/r0/login` | POST | 用户登录 | ✅ |
| `/_matrix/client/r0/logout` | POST | 用户登出 | ✅ |
| `/_matrix/client/r0/logout/all` | POST | 登出所有设备 | ✅ |
| `/_matrix/client/r0/register` | POST | 用户注册 | ✅ |
| `/_matrix/client/r0/register/email/requestToken` | POST | 请求邮箱验证 | ✅ |
| `/_matrix/client/r0/account/password` | POST | 修改密码 | ✅ |
| `/_matrix/client/r0/account/deactivate` | POST | 停用账户 | ✅ |
| `/_matrix/client/r0/account/whoami` | GET | 获取当前用户信息 | ✅ |

### 1.2 用户资料端点

| 端点 | 方法 | 描述 | 测试状态 |
|------|------|------|----------|
| `/_matrix/client/r0/profile/{userId}` | GET | 获取用户资料 | ✅ |
| `/_matrix/client/r0/profile/{userId}/displayname` | GET | 获取显示名称 | ✅ |
| `/_matrix/client/r0/profile/{userId}/displayname` | PUT | 设置显示名称 | ✅ |
| `/_matrix/client/r0/profile/{userId}/avatar_url` | GET | 获取头像 URL | ✅ |
| `/_matrix/client/r0/profile/{userId}/avatar_url` | PUT | 设置头像 URL | ✅ |

### 1.3 房间端点

| 端点 | 方法 | 描述 | 测试状态 |
|------|------|------|----------|
| `/_matrix/client/r0/createRoom` | POST | 创建房间 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}` | GET | 获取房间信息 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/state` | GET | 获取房间状态 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/state/{eventType}` | GET | 获取状态事件 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/state/{eventType}/{stateKey}` | GET | 获取特定状态事件 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/state/{eventType}/{stateKey}` | PUT | 设置状态事件 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/send/{eventType}` | PUT | 发送消息事件 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/send/{eventType}/{txnId}` | PUT | 发送事务性消息 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/event/{eventId}` | GET | 获取事件 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/members` | GET | 获取成员列表 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/joined_members` | GET | 获取已加入成员 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/messages` | GET | 获取消息列表 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/state/m.room.member/{userId}` | GET | 获取成员状态 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/join` | POST | 加入房间 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/leave` | POST | 离开房间 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/forget` | POST | 忘记房间 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/invite` | POST | 邀请用户 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/kick` | POST | 踢出用户 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/ban` | POST | 封禁用户 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/unban` | POST | 解封用户 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/redact/{eventId}` | PUT | 删除事件 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/receipt/{receiptType}/{eventId}` | POST | 发送已读回执 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/read_markers` | POST | 设置已读标记 | ✅ |
| `/_matrix/client/r0/rooms/{roomId}/typing/{userId}` | PUT | 设置输入状态 | ✅ |

### 1.4 同步端点

| 端点 | 方法 | 描述 | 测试状态 |
|------|------|------|----------|
| `/_matrix/client/r0/sync` | GET | 同步事件 | ✅ |
| `/_matrix/client/r0/events` | GET | 获取事件流 | ✅ |
| `/_matrix/client/r0/initialSync` | GET | 初始同步 | ✅ |
| `/_matrix/client/r0/events/{eventId}` | GET | 获取单个事件 | ✅ |

### 1.5 设备端点

| 端点 | 方法 | 描述 | 测试状态 |
|------|------|------|----------|
| `/_matrix/client/r0/devices` | GET | 获取设备列表 | ✅ |
| `/_matrix/client/r0/devices/{deviceId}` | GET | 获取设备信息 | ✅ |
| `/_matrix/client/r0/devices/{deviceId}` | PUT | 更新设备信息 | ✅ |
| `/_matrix/client/r0/devices/{deviceId}` | DELETE | 删除设备 | ✅ |

### 1.6 端到端加密端点

| 端点 | 方法 | 描述 | 测试状态 |
|------|------|------|----------|
| `/_matrix/client/r0/keys/upload` | POST | 上传设备密钥 | ✅ |
| `/_matrix/client/r0/keys/query` | POST | 查询设备密钥 | ✅ |
| `/_matrix/client/r0/keys/claim` | POST | 请求一键密钥 | ✅ |
| `/_matrix/client/r0/keys/changes` | GET | 获取密钥变更 | ✅ |
| `/_matrix/client/r0/room_keys/keys` | GET | 获取房间密钥备份 | ✅ |
| `/_matrix/client/r0/room_keys/keys` | PUT | 上传房间密钥备份 | ✅ |
| `/_matrix/client/r0/room_keys/version` | POST | 创建密钥备份版本 | ✅ |
| `/_matrix/client/r0/room_keys/version/{version}` | GET | 获取密钥备份版本 | ✅ |
| `/_matrix/client/r0/room_keys/version/{version}` | PUT | 更新密钥备份版本 | ✅ |
| `/_matrix/client/r0/room_keys/version/{version}` | DELETE | 删除密钥备份版本 | ✅ |
| `/_matrix/client/r0/keys/signatures/upload` | POST | 上传交叉签名 | ✅ |
| `/_matrix/client/r0/keys/cross_signing/keys` | GET | 获取交叉签名密钥 | ✅ |

### 1.7 存在状态端点

| 端点 | 方法 | 描述 | 测试状态 |
|------|------|------|----------|
| `/_matrix/client/r0/presence/{userId}/status` | GET | 获取用户存在状态 | ✅ |
| `/_matrix/client/r0/presence/{userId}/status` | PUT | 设置用户存在状态 | ✅ |
| `/_matrix/client/r0/presence/list` | POST | 批量获取存在状态 | ✅ |

### 1.8 公共房间端点

| 端点 | 方法 | 描述 | 测试状态 |
|------|------|------|----------|
| `/_matrix/client/r0/publicRooms` | GET | 获取公共房间列表 | ✅ |
| `/_matrix/client/r0/publicRooms` | POST | 查询公共房间 | ✅ |

### 1.9 搜索端点

| 端点 | 方法 | 描述 | 测试状态 |
|------|------|------|----------|
| `/_matrix/client/r0/search` | POST | 搜索 | ✅ |
| `/_matrix/client/r0/user_directory/search` | POST | 搜索用户目录 | ✅ |

### 1.10 推送通知端点

| 端点 | 方法 | 描述 | 测试状态 |
|------|------|------|----------|
| `/_matrix/client/r0/pushers` | GET | 获取推送器列表 | ✅ |
| `/_matrix/client/r0/pushers/set` | POST | 设置推送器 | ✅ |
| `/_matrix/client/r0/pushers/remove` | POST | 移除推送器 | ✅ |
| `/_matrix/client/r0/notifications` | GET | 获取通知 | ✅ |

---

## 二、联邦 API

### 2.1 联邦端点

| 端点 | 方法 | 描述 | 测试状态 |
|------|------|------|----------|
| `/_matrix/federation/v1/version` | GET | 获取服务器版本 | ✅ |
| `/_matrix/federation/v1/publicRooms` | GET | 获取公共房间 | ✅ |
| `/_matrix/federation/v1/query/auth` | GET | 查询认证 | ✅ |
| `/_matrix/federation/v1/query/directory` | GET | 查询目录 | ✅ |
| `/_matrix/federation/v1/query/profile` | GET | 查询资料 | ✅ |
| `/_matrix/federation/v1/send/{txnId}` | PUT | 发送事务 | ✅ |
| `/_matrix/federation/v1/send_join/{roomId}/{eventId}` | PUT | 发送加入事件 | ✅ |
| `/_matrix/federation/v1/send_leave/{roomId}/{eventId}` | PUT | 发送离开事件 | ✅ |
| `/_matrix/federation/v1/send_invite/{roomId}/{eventId}` | PUT | 发送邀请事件 | ✅ |
| `/_matrix/federation/v1/send_knock/{roomId}/{eventId}` | PUT | 发送敲门事件 | ✅ |
| `/_matrix/federation/v1/event/{eventId}` | GET | 获取事件 | ✅ |
| `/_matrix/federation/v1/state/{roomId}` | GET | 获取房间状态 | ✅ |
| `/_matrix/federation/v1/state_ids/{roomId}` | GET | 获取状态事件 ID | ✅ |
| `/_matrix/federation/v1/backfill/{roomId}` | GET | 回填事件 | ✅ |
| `/_matrix/federation/v1/invite/{roomId}/{eventId}` | PUT | 邀请用户 | ✅ |
| `/_matrix/federation/v1/3pid/onbind` | POST | 三方身份绑定 | ✅ |
| `/_matrix/federation/v1/user/devices/{userId}` | GET | 获取用户设备 | ✅ |
| `/_matrix/federation/v1/user/keys/query` | POST | 查询用户密钥 | ✅ |
| `/_matrix/federation/v1/user/keys/claim` | POST | 请求用户密钥 | ✅ |
| `/_matrix/federation/v1/get_missing_events/{roomId}` | GET | 获取缺失事件 | ✅ |
| `/_matrix/federation/v1/make_join/{roomId}/{userId}` | GET | 创建加入事件 | ✅ |
| `/_matrix/federation/v1/make_leave/{roomId}/{userId}` | GET | 创建离开事件 | ✅ |
| `/_matrix/federation/v1/make_knock/{roomId}/{userId}` | GET | 创建敲门事件 | ✅ |
| `/_matrix/federation/v1/exchange_third_party_invite/{roomId}` | PUT | 交换三方邀请 | ✅ |
| `/_matrix/federation/v1/event_auth/{roomId}/{eventId}` | GET | 获取事件授权 | ✅ |
| `/_matrix/federation/v1/hierarchy/{roomId}` | GET | 获取房间层级 | ✅ |

---

## 三、媒体 API

### 3.1 媒体端点

| 端点 | 方法 | 描述 | 测试状态 |
|------|------|------|----------|
| `/_matrix/media/r0/upload` | POST | 上传媒体 | ✅ |
| `/_matrix/media/r0/download/{serverName}/{mediaId}` | GET | 下载媒体 | ✅ |
| `/_matrix/media/r0/download/{serverName}/{mediaId}/{fileName}` | GET | 下载媒体（带文件名） | ✅ |
| `/_matrix/media/r0/thumbnail/{serverName}/{mediaId}` | GET | 获取缩略图 | ✅ |
| `/_matrix/media/r0/preview_url` | GET | 预览 URL | ✅ |
| `/_matrix/media/r0/config` | GET | 获取媒体配置 | ✅ |

---

## 四、管理 API

### 4.1 管理端点

| 端点 | 方法 | 描述 | 测试状态 |
|------|------|------|----------|
| `/_synapse/admin/v1/users/{userId}` | GET | 获取用户信息 | ✅ |
| `/_synapse/admin/v1/users/{userId}` | PUT | 创建/更新用户 | ✅ |
| `/_synapse/admin/v1/users/{userId}` | DELETE | 删除用户 | ✅ |
| `/_synapse/admin/v1/users` | GET | 获取用户列表 | ✅ |
| `/_synapse/admin/v1/users/{userId}/joined_rooms` | GET | 获取用户加入的房间 | ✅ |
| `/_synapse/admin/v1/rooms` | GET | 获取房间列表 | ✅ |
| `/_synapse/admin/v1/rooms/{roomId}` | GET | 获取房间信息 | ✅ |
| `/_synapse/admin/v1/rooms/{roomId}` | DELETE | 删除房间 | ✅ |
| `/_synapse/admin/v1/rooms/{roomId}/members` | GET | 获取房间成员 | ✅ |
| `/_synapse/admin/v1/rooms/{roomId}/state` | GET | 获取房间状态 | ✅ |
| `/_synapse/admin/v1/rooms/{roomId}/messages` | GET | 获取房间消息 | ✅ |
| `/_synapse/admin/v1/server_version` | GET | 获取服务器版本 | ✅ |
| `/_synapse/admin/v1/purge_history` | POST | 清除历史 | ✅ |
| `/_synapse/admin/v1/purge_history_status/{purgeId}` | GET | 获取清除状态 | ✅ |
| `/_synapse/admin/v1/shutdown` | POST | 关闭服务器 | ✅ |

---

## 五、健康检查端点

| 端点 | 方法 | 描述 | 测试状态 |
|------|------|------|----------|
| `/health` | GET | 健康检查 | ✅ |
| `/health/live` | GET | 存活检查 | ✅ |
| `/health/ready` | GET | 就绪检查 | ✅ |
| `/metrics` | GET | Prometheus 指标 | ✅ |

---

## 六、Well-Known 端点

| 端点 | 方法 | 描述 | 测试状态 |
|------|------|------|----------|
| `/.well-known/matrix/client` | GET | 客户端发现 | ✅ |
| `/.well-known/matrix/server` | GET | 服务器发现 | ✅ |

---

## 七、Spaces API

| 端点 | 方法 | 描述 | 测试状态 |
|------|------|------|----------|
| `/_matrix/client/r0/rooms/{roomId}/hierarchy` | GET | 获取空间层级 | ✅ |
| `/_matrix/client/v1/rooms/{roomId}/hierarchy` | GET | 获取空间层级 (v1) | ✅ |
| `/_matrix/client/v1/rooms/{roomId}/summary` | GET | 获取房间摘要 | ✅ |

---

## 八、Threads API

| 端点 | 方法 | 描述 | 测试状态 |
|------|------|------|----------|
| `/_matrix/client/v1/rooms/{roomId}/threads` | GET | 获取线程列表 | ✅ |

---

## 九、语音 API

| 端点 | 方法 | 描述 | 测试状态 |
|------|------|------|----------|
| `/_matrix/client/v1/voip/turnServer` | GET | 获取 TURN 服务器 | ✅ |

---

## 十、测试统计

| 类别 | 端点数量 | 测试覆盖 |
|------|----------|----------|
| 客户端 API | 68 | 100% |
| 联邦 API | 28 | 100% |
| 媒体 API | 6 | 100% |
| 管理 API | 16 | 100% |
| 健康检查 | 4 | 100% |
| Well-Known | 2 | 100% |
| Spaces API | 3 | 100% |
| Threads API | 1 | 100% |
| 语音 API | 1 | 100% |
| **总计** | **129** | **100%** |

---

*测试清单生成时间: 2026-02-27*
