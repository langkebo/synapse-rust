# synapse-rust API 参考文档

> 生成时间: 2026-03-14
> 代码行数: ~18万行
> **审核日期**: 2026-03-18
> **审核状态**: ✅ 全部通过

---

## 审核总结

| 项目 | 结果 |
|------|------|
| **数据库表** | 135+ |
| **API 端点** | 284+ (333 路由定义) |
| **HTTP 方法** | 1167+ 处理器 |
| **代码模块** | 50+ |
| **测试通过率** | 100% |
| **字段一致性** | ✅ 已修复 |

### API 分类统计

| 分类 | 端点数量 |
|------|----------|
| Client API (r0) | 60 |
| Client API (v1) | 30 |
| Client API (v3) | 54 |
| Media API | 11 |
| Federation API | 12 |
| Key API | 1 |
| Admin API | 93 |
| Worker API | 8 |
| 其他 (login/logout/health) | 15 |
| **总计** | **284** |

---

## 目录

### 用户端 API
1. [认证 API](#1-认证-api)
2. [用户管理 API](#2-用户管理-api)
3. [房间管理 API](#3-房间管理-api)
4. [消息 API](#4-消息-api)
5. [设备管理 API](#5-设备管理-api)
6. [E2EE 加密 API](#6-e2ee-加密-api)
7. [媒体 API](#7-媒体-api)
8. [好友系统 API](#8-好友系统-api)
9. [Space API](#9-space-api)
10. [Thread API](#10-thread-api)
11. [搜索 API](#11-搜索-api)
12. [推送 API](#12-推送-api)
13. [Widget API](#13-widget-api)
14. [Sliding Sync API](#14-sliding-sync-api)

### 管理端 API
15. [管理后台 API](#15-管理后台-api)
16. [联邦 API](#16-联邦-api)
17. [应用服务 API](#17-应用服务-api)

### 认证 API
18. [CAS 认证 API](#18-cas-认证-api)
19. [SAML 认证 API](#19-saml-认证-api)
20. [OIDC 认证 API](#20-oidc-认证-api)
21. [QR 登录 API](#21-qr-登录-api)

### 其他 API
22. [语音消息 API](#22-语音消息-api)
23. [VoIP API](#23-voip-api)
24. [密钥备份 API](#24-密钥备份-api)
25. [保留策略 API](#25-保留策略-api)
26. [媒体配额 API](#26-媒体配额-api)
27. [服务器通知 API](#27-服务器通知-api)
28. [事件举报 API](#28-事件举报-api)
29. [账户数据 API](#29-账户数据-api)
30. [注册令牌 API](#30-注册令牌-api)
31. [Worker API](#31-worker-api)

---

## 1. 认证 API

### 1.1 端点统计

| 分类 | 端点数量 |
|------|----------|
| 用户注册 | 4 |
| 用户登录 | 4 |
| 用户登出 | 4 |
| Token 刷新 | 2 |
| 当前用户 | 2 |

### 1.2 核心端点

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/login` | POST | 用户登录 | 公开 |
| `/_matrix/client/r0/register` | POST | 用户注册 | 公开 |
| `/_matrix/client/r0/logout` | POST | 登出 | 认证 |
| `/_matrix/client/r0/refresh` | POST | 刷新Token | 认证 |
| `/_matrix/client/r0/account/whoami` | GET | 获取当前用户 | 认证 |

---

## 2. 用户管理 API

### 2.1 端点统计

| 分类 | 端点数量 |
|------|----------|
| 密码管理 | 2 |
| 账户注销 | 2 |
| 第三方身份 | 7 |
| 用户资料 | 3 |

### 2.2 核心端点

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/client/r0/account/password` | POST | 修改密码 |
| `/_matrix/client/r0/account/deactivate` | POST | 注销账户 |
| `/_matrix/client/r0/account/3pid` | GET/POST/DELETE | 第三方ID管理 |
| `/_matrix/client/r0/profile/{user_id}` | GET | 获取用户资料 |

---

## 3. 房间管理 API

### 3.1 端点统计

| 分类 | 端点数量 |
|------|----------|
| 房间创建 | 2 |
| 加入/离开 | 5 |
| 成员管理 | 4 |
| 房间信息 | 5 |
| 房间别名 | 3 |
| 公开房间 | 2 |

### 3.2 核心端点

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/client/r0/createRoom` | POST | 创建房间 |
| `/_matrix/client/r0/join/{room_id}` | POST | 加入房间 |
| `/_matrix/client/r0/rooms/{room_id}/leave` | POST | 离开房间 |
| `/_matrix/client/r0/rooms/{room_id}/invite` | POST | 邀请用户 |
| `/_matrix/client/r0/rooms/{room_id}/members` | GET | 获取成员 |

---

## 4. 消息 API

### 4.1 端点统计

| 分类 | 端点数量 |
|------|----------|
| 发送消息 | 1 |
| 获取消息 | 3 |
| 状态事件 | 4 |
| 消息撤回 | 3 |

### 4.2 核心端点

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/client/r0/rooms/{room_id}/send/{event_type}/{txn_id}` | PUT | 发送消息 |
| `/_matrix/client/r0/rooms/{room_id}/messages` | GET | 获取消息 |
| `/_matrix/client/r0/rooms/{room_id}/state/{event_type}/{state_key}` | PUT | 发送状态事件 |
| `/_matrix/client/r0/rooms/{room_id}/redact/{event_id}/{txn_id}` | PUT | 撤回消息 |

---

## 5. 设备管理 API

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/client/r0/devices` | GET | 获取设备列表 |
| `/_matrix/client/r0/devices/{device_id}` | GET/PUT/DELETE | 设备 CRUD |

---

## 6. E2EE 加密 API

### 6.1 端点统计

| 分类 | 端点数量 |
|------|----------|
| 密钥上传 | 4 |
| 交叉签名 | 2 |
| To-Device | 1 |

### 6.2 核心端点

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/client/r0/keys/upload` | POST | 上传设备密钥 |
| `/_matrix/client/r0/keys/query` | POST | 查询设备密钥 |
| `/_matrix/client/r0/keys/claim` | POST | 申领一次性密钥 |
| `/_matrix/client/r0/sendToDevice/{event_type}/{txn_id}` | PUT | 发送到设备 |

---

## 7. 媒体 API

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/media/r0/upload` | POST | 上传媒体 |
| `/_matrix/media/r0/download/{server_name}/{media_id}` | GET | 下载媒体 |
| `/_matrix/media/r0/thumbnail/{server_name}/{media_id}` | GET | 获取缩略图 |
| `/_matrix/media/r0/preview_url` | GET | URL 预览 |

---

## 8. 好友系统 API

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/client/v1/friends` | GET | 获取好友列表 |
| `/_matrix/client/v1/friends/{user_id}` | POST/DELETE | 好友 CRUD |
| `/_matrix/client/v1/friends/{user_id}/accept` | POST | 接受请求 |
| `/_matrix/client/v1/friends/{user_id}/block` | POST | 拉黑 |

---

## 9. Space API

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/client/v1/spaces` | POST | 创建空间 |
| `/_matrix/client/v1/spaces/public` | GET | 获取公开空间 |
| `/_matrix/client/v1/spaces/{space_id}/hierarchy` | GET | 获取空间层级 |

---

## 10. Thread API

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/client/v1/threads` | POST/GET | 创建/获取线程 |
| `/_matrix/client/v1/threads/{thread_id}` | GET/PUT/DELETE | 线程 CRUD |
| `/_matrix/client/v1/threads/{thread_id}/reply` | POST | 回复线程 |
| `/_matrix/client/v1/threads/{thread_id}/pin` | POST | 置顶线程 |

---

## 11. 搜索 API

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/client/r0/search` | POST | 搜索消息 |
| `/_matrix/client/r0/user_directory/search` | POST | 搜索用户 |

---

## 12. 推送 API

### 12.1 端点统计

| 分类 | 端点数量 |
|------|----------|
| 推送器管理 | 2 |
| 推送规则 | 7 |
| 通知 | 1 |

### 12.2 核心端点

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/client/r0/pushers/set` | POST | 设置推送器 |
| `/_matrix/client/r0/pushrules` | GET | 获取推送规则 |
| `/_matrix/client/r0/notifications` | GET | 获取通知 |

---

## 13. Widget API (MSC4261)

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/client/v3/widgets` | GET/POST | Widget CRUD |
| `/_matrix/client/v3/widgets/{widget_id}` | GET/PUT/DELETE | Widget 管理 |
| `/_matrix/client/v3/rooms/{room_id}/widgets` | GET/POST | 房间 Widget |

---

## 14. Sliding Sync API

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/client/unstable/org.matrix.msc3575/sync` | GET | Sliding Sync |

---

## 15. 管理后台 API

### 15.1 端点统计

| 分类 | 端点数量 |
|------|----------|
| 服务器管理 | 3 |
| 用户管理 | 6 |
| 房间管理 | 6 |
| 安全管理 | 4 |
| 媒体管理 | 6 |
| 令牌管理 | 4 |

### 15.2 核心端点

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_synapse/admin/v1/server_version` | GET | 服务器版本 |
| `/_synapse/admin/v1/users` | GET | 用户列表 |
| `/_synapse/admin/v1/rooms` | GET | 房间列表 |
| `/_synapse/admin/v1/registration_tokens` | GET/POST | 注册令牌 |

---

## 16. 联邦 API

### 16.1 端点统计

| 分类 | 端点数量 |
|------|----------|
| 服务器发现 | 2 |
| 事件同步 | 3 |
| 消息发送 | 4 |
| 密钥交换 | 2 |

### 16.2 核心端点

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/federation/v2/server` | GET | 获取服务器密钥 |
| `/_matrix/federation/v1/event/{event_id}` | GET | 获取事件 |
| `/_matrix/federation/v1/send/{txn_id}` | PUT | 发送事务 |
| `/_matrix/federation/v1/keys/claim` | POST | 申领密钥 |

---

## 17. 应用服务 API

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/app/v1/transactions/{txn_id}` | PUT | 接收事务 |
| `/_matrix/app/v1/users/{user_id}` | GET | 查询用户 |
| `/_matrix/app/v1/rooms/{room_alias}` | GET | 查询房间 |

---

## 18. CAS 认证 API

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/client/r0/auth/cas/redirect` | GET | CAS 重定向 |
| `/_matrix/client/r0/auth/cas/ticket` | GET | CAS 票据 |
| `/_synapse/admin/v1/cas/config` | GET/PUT | CAS 配置 |

---

## 19. SAML 认证 API

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/client/r0/auth/saml/redirect` | GET | SAML 重定向 |
| `/_matrix/client/r0/auth/saml/response` | POST | SAML 响应 |
| `/_synapse/admin/v1/saml/config` | GET/PUT | SAML 配置 |

---

## 20. OIDC 认证 API

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/client/r0/auth/oidc/redirect` | GET | OIDC 重定向 |
| `/_matrix/client/r0/auth/oidc/callback` | GET | OIDC 回调 |

---

## 21. QR 登录 API (MSC4388)

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/client/v1/login/get_qr_code` | GET | 获取二维码 |
| `/_matrix/client/v1/login/qr/start` | POST | 开始 QR 登录 |
| `/_matrix/client/v1/login/qr/{transaction_id}/status` | GET | 检查状态 |
| `/_matrix/client/v1/login/qr/confirm` | POST | 确认登录 |

---

## 22. 语音消息 API

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/client/r0/voice/upload` | POST | 上传语音 |
| `/_matrix/client/r0/voice/stats` | GET | 语音统计 |
| `/_matrix/client/r0/voice/{message_id}` | GET/DELETE | 语音 CRUD |
| `/_matrix/client/v1/voice/transcription` | POST | 语音转文字 |

---

## 23. VoIP API

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/client/r0/voip/turnServer` | GET | 获取 TURN 服务器 |
| `/_matrix/client/r0/voip/signaling` | GET | 获取信令配置 |

---

## 24. 密钥备份 API

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/client/r0/room_keys/version` | POST/GET | 版本 CRUD |
| `/_matrix/client/r0/room_keys/keys` | GET/PUT | 密钥 CRUD |
| `/_matrix/client/r0/room_keys/recover` | POST | 恢复密钥 |

---

## 25. 保留策略 API

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_synapse/admin/v1/retention/policies` | GET | 策略列表 |
| `/_synapse/admin/v1/retention/policies/{policy_id}` | GET/PUT/DELETE | 策略 CRUD |

---

## 26. 媒体配额 API

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_synapse/admin/v1/media/quota` | GET/PUT | 配额设置 |
| `/_synapse/admin/v1/media/quota/users/{user_id}` | GET/PUT | 用户配额 |

---

## 27. 服务器通知 API

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_synapse/admin/v1/server_notifications` | GET | 通知列表 |

---

## 28. 事件举报 API

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/client/v1/rooms/{room_id}/report/{event_id}` | POST | 举报事件 |
| `/_synapse/admin/v1/event_reports` | GET | 举报列表 |

---

## 29. 账户数据 API

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_matrix/client/r0/user/{user_id}/account_data/{type}` | GET/PUT | 账户数据 |
| `/_matrix/client/r0/user/{user_id}/rooms/{room_id}/account_data/{type}` | GET/PUT | 房间账户数据 |

---

## 30. 注册令牌 API

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_synapse/admin/v1/registration_tokens` | GET/POST | 令牌 CRUD |
| `/_synapse/admin/v1/registration_tokens/validate` | POST | 验证令牌 |

---

## 31. Worker API

| 端点 | 方法 | 功能 |
|------|------|------|
| `/_synapse/worker/v1/health` | GET | Worker 健康检查 |
| `/_synapse/worker/v1/stats` | GET | Worker 统计 |
| `/_synapse/worker/v1/config` | GET | Worker 配置 |

---

## 附录 A: 数据库表统计

### 核心表 (用户/认证)

| 表名 | 说明 |
|------|------|
| users | 用户 |
| user_threepids | 第三方身份 |
| devices | 设备 |
| access_tokens | 访问令牌 |
| refresh_tokens | 刷新令牌 |
| registration_tokens | 注册令牌 |

### 房间表

| 表名 | 说明 |
|------|------|
| rooms | 房间 |
| room_memberships | 房间成员 |
| room_summaries | 房间摘要 |
| room_aliases | 房间别名 |
| events | 事件 |
| thread_roots | 线程根 |

### 加密表

| 表名 | 说明 |
|------|------|
| device_keys | 设备密钥 |
| cross_signing_keys | 交叉签名密钥 |
| megolm_sessions | Megolm 会话 |
| key_backups | 密钥备份 |

### 媒体表

| 表名 | 说明 |
|------|------|
| media_metadata | 媒体元数据 |
| thumbnails | 缩略图 |
| media_quota | 媒体配额 |

---

## 附录 B: MSC 功能支持

| MSC | 功能名称 | 状态 |
|-----|----------|------|
| MSC3575 | Sliding Sync | ✅ 已实现 |
| MSC3983 | Thread | ✅ 已实现 |
| MSC4380 | 邀请屏蔽 | ✅ 已实现 |
| MSC4354 | Sticky Events | ✅ 已实现 |
| MSC4388 | QR 登录 | ✅ 已实现 |
| MSC4261 | Widget API | ✅ 已实现 |
| MSC3245 | Room Summary | ✅ 已实现 |

---

## 附录 C: API 版本兼容性

| API版本 | 说明 | 状态 |
|---------|------|------|
| r0 | 旧版API | ⚠️ 兼容 |
| v3 | 当前稳定版本 | ✅ 推荐 |
| v1 | 特定功能版本 | ✅ 支持 |
| unstable | 实验性功能 | ⚠️ 不稳定 |

---

## 附录 D: 状态码说明

| 状态码 | 说明 |
|--------|------|
| 200 | 成功 |
| 201 | 创建成功 |
| 204 | 无内容 |
| 400 | 请求错误 |
| 401 | 未认证 |
| 403 | 禁止访问 |
| 404 | 未找到 |
| 409 | 冲突 |
| 429 | 请求过多 |
| 500 | 服务器内部错误 |

---

*文档生成完成 - 基于 synapse-rust 项目实际代码统计 (2026-03-14)*
