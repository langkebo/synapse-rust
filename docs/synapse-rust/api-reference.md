# synapse-rust API 参考文档

> 生成时间: 2026-03-11
> 代码行数: ~16万行

---

## 目录

1. [基础服务 API](#1-基础服务-api) - 9 个端点
2. [用户认证 API](#2-用户认证-api) - 16 个端点
3. [账户管理 API](#3-账户管理-api) - 14 个端点
4. [房间管理 API](#4-房间管理-api) - 28 个端点
5. [消息发送 API](#5-消息发送-api) - 18 个端点
6. [设备管理 API](#6-设备管理-api) - 8 个端点
7. [推送通知 API](#7-推送通知-api) - 25 个端点
8. [E2EE 加密 API](#8-e2ee-加密-api) - 15 个端点
9. [媒体服务 API](#9-媒体服务-api) - 27 个端点
10. [好友系统 API](#10-好友系统-api) - 9 个端点
11. [Space 空间 API](#11-space-空间-api) - 7 个端点
12. [Thread 线程 API](#12-thread-线程-api) - 16 个端点
13. [搜索服务 API](#13-搜索服务-api) - 19 个端点
14. [管理后台 API](#14-管理后台-api) - 69 个端点
15. [联邦 API](#15-联邦-api) - 54 个端点
16. [应用服务 API](#16-应用服务-api) - 6 个端点
17. [语音消息 API](#17-语音消息-api) - 16 个端点
18. [VoIP 服务 API](#18-voip-服务-api) - 0 个端点
19. [验证码服务 API](#19-验证码服务-api) - 1 个端点
20. [后台更新 API](#20-后台更新-api) - 1 个端点
21. [事件举报 API](#21-事件举报-api) - 6 个端点
22. [账户数据 API](#22-账户数据-api) - 2 个端点
23. [密钥备份 API](#23-密钥备份-api) - 8 个端点
24. [保留策略 API](#24-保留策略-api) - 1 个端点
25. [服务器通知 API](#25-服务器通知-api) - 1 个端点
26. [注册令牌 API](#26-注册令牌-api) - 2 个端点
27. [媒体配额 API](#27-媒体配额-api) - 8 个端点
28. [CAS 认证 API](#28-cas-认证-api) - 9 个端点
29. [SAML 认证 API](#29-saml-认证-api) - 3 个端点
30. [OIDC 认证 API](#30-oidc-认证-api) - 2 个端点
31. [Rendezvous API](#31-rendezvous-api) - 8 个端点
32. [Worker API](#32-worker-api) - 9 个端点
33. [联邦黑名单 API](#33-联邦黑名单-api) - 0 个端点
34. [联邦缓存 API](#34-联邦缓存-api) - 0 个端点
35. [刷新令牌 API](#35-刷新令牌-api) - 3 个端点
36. [推送通知管理 API](#36-推送通知管理-api) - 7 个端点
37. [速率限制管理 API](#37-速率限制管理-api) - 8 个端点
38. [Sliding Sync API](#38-sliding-sync-api) - 1 个端点
39. [遥测 API](#39-遥测-api) - 2 个端点

---

## 1. 基础服务 API (9 个端点)

### 1.1 健康检查

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/health` | GET | 健康检查 | 公开 |
| `/_matrix/client/versions` | GET | 获取客户端版本 | 公开 |
| `/_matrix/client/v3/versions` | GET | 获取客户端版本 | 公开 |
| `/_matrix/client/r0/version` | GET | 获取服务器版本 | 公开 |
| `/_matrix/server_version` | GET | 获取服务器版本 | 公开 |
| `/_matrix/client/r0/capabilities` | GET | 获取客户端能力 | 认证 |
| `/_matrix/client/v3/capabilities` | GET | 获取客户端能力 | 认证 |
| `/.well-known/matrix/server` | GET | 服务器发现 | 公开 |
| `/.well-known/matrix/client` | GET | 客户端发现 | 公开 |

---

## 2. 用户认证 API (16 个端点)

### 2.1 用户注册

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/register` | POST | 用户注册 | 公开 |
| `/_matrix/client/v3/register` | POST | 用户注册 | 公开 |
| `/_matrix/client/r0/register/available` | GET | 检查用户名可用性 | 公开 |
| `/_matrix/client/v3/register/available` | GET | 检查用户名可用性 | 公开 |

### 2.2 用户登录

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/login` | GET | 获取登录流程 | 公开 |
| `/_matrix/client/r0/login` | POST | 用户登录 | 公开 |
| `/_matrix/client/v3/login` | GET | 获取登录流程 | 公开 |
| `/_matrix/client/v3/login` | POST | 用户登录 | 公开 |

### 2.3 用户登出

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/logout` | POST | 登出当前设备 | 认证 |
| `/_matrix/client/v3/logout` | POST | 登出当前设备 | 认证 |
| `/_matrix/client/r0/logout/all` | POST | 登出所有设备 | 认证 |
| `/_matrix/client/v3/logout/all` | POST | 登出所有设备 | 认证 |

### 2.4 Token 刷新

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/refresh` | POST | 刷新Token | 认证 |
| `/_matrix/client/v3/refresh` | POST | 刷新Token | 认证 |

### 2.5 当前用户

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/account/whoami` | GET | 获取当前用户信息 | 认证 |
| `/_matrix/client/v3/account/whoami` | GET | 获取当前用户信息 | 认证 |

---

## 3. 账户管理 API (14 个端点)

### 3.1 密码管理

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/account/password` | POST | 修改密码 | 认证 |
| `/_matrix/client/v3/account/password` | POST | 修改密码 | 认证 |

### 3.2 账户注销

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/account/deactivate` | POST | 注销账户 | 认证 |
| `/_matrix/client/v3/account/deactivate` | POST | 注销账户 | 认证 |

### 3.3 第三方身份绑定

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/account/3pid` | GET | 获取绑定列表 | 认证 |
| `/_matrix/client/r0/account/3pid` | POST | 绑定第三方ID | 认证 |
| `/_matrix/client/r0/account/3pid` | DELETE | 解绑第三方ID | 认证 |
| `/_matrix/client/r0/account/3pid/add` | POST | 添加第三方ID | 认证 |
| `/_matrix/client/r0/account/3pid/bind` | POST | 绑定第三方ID | 认证 |
| `/_matrix/client/r0/account/3pid/delete` | POST | 删除第三方ID | 认证 |
| `/_matrix/client/r0/account/3pid/unbind` | POST | 解绑第三方ID | 认证 |

### 3.4 用户资料

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/profile/{user_id}` | GET | 获取用户资料 | 认证 |
| `/_matrix/client/r0/profile/{user_id}/displayname` | GET/PUT | 获取/设置显示名 | 认证 |
| `/_matrix/client/r0/profile/{user_id}/avatar_url` | GET/PUT | 获取/设置头像URL | 认证 |

---

## 4. 房间管理 API (28 个端点)

### 4.1 房间创建

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/createRoom` | POST | 创建房间 | 认证 |
| `/_matrix/client/v3/createRoom` | POST | 创建房间 | 认证 |

### 4.2 房间加入/离开

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/join/{room_id_or_alias}` | POST | 通过ID或别名加入房间 | 认证 |
| `/_matrix/client/r0/knock/{room_id_or_alias}` | POST | 敲门请求加入 | 认证 |
| `/_matrix/client/r0/rooms/{room_id}/join` | POST | 加入房间 | 认证 |
| `/_matrix/client/r0/rooms/{room_id}/leave` | POST | 离开房间 | 认证 |
| `/_matrix/client/r0/rooms/{room_id}/forget` | POST | 忘记房间 | 认证 |

### 4.3 成员管理

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/rooms/{room_id}/invite` | POST | 邀请用户 | 认证 |
| `/_matrix/client/r0/rooms/{room_id}/kick` | POST | 踢出用户 | 认证 |
| `/_matrix/client/r0/rooms/{room_id}/ban` | POST | 封禁用户 | 认证 |
| `/_matrix/client/r0/rooms/{room_id}/unban` | POST | 解除封禁 | 认证 |

### 4.4 房间信息

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/joined_rooms` | GET | 获取已加入房间列表 | 认证 |
| `/_matrix/client/r0/rooms/{room_id}` | GET | 获取房间信息 | 认证 |
| `/_matrix/client/r0/rooms/{room_id}/state` | GET | 获取房间状态 | 认证 |
| `/_matrix/client/r0/rooms/{room_id}/members` | GET | 获取房间成员 | 认证 |
| `/_matrix/client/r0/rooms/{room_id}/joined_members` | GET | 获取已加入成员 | 认证 |

### 4.5 房间别名

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/directory/room/{room_alias}` | GET | 解析房间别名 | 认证 |
| `/_matrix/client/r0/directory/room/{room_alias}` | PUT/DELETE | 创建/删除房间别名 | 认证 |
| `/_matrix/client/r0/rooms/{room_id}/aliases` | GET | 获取房间别名列表 | 认证 |

### 4.6 公开房间

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/publicRooms` | GET/POST | 获取/搜索公开房间 | 公开/认证 |

---

## 5. 消息发送 API (18 个端点)

### 5.1 发送消息

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/rooms/{room_id}/send/{event_type}/{txn_id}` | PUT | 发送消息事件 | 认证 |

### 5.2 获取消息

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/rooms/{room_id}/messages` | GET | 获取房间消息 | 认证 |
| `/_matrix/client/r0/rooms/{room_id}/event/{event_id}` | GET | 获取单个事件 | 认证 |
| `/_matrix/client/r0/rooms/{room_id}/context/{event_id}` | GET | 获取事件上下文 | 认证 |

### 5.3 状态事件

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/rooms/{room_id}/state/{event_type}` | PUT/GET | 发送/获取状态事件 | 认证 |
| `/_matrix/client/r0/rooms/{room_id}/state/{event_type}/{state_key}` | PUT/GET | 发送/获取状态事件 | 认证 |

### 5.4 消息撤回与已读

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/rooms/{room_id}/redact/{event_id}/{txn_id}` | PUT | 撤回消息 | 认证 |
| `/_matrix/client/r0/rooms/{room_id}/receipt/{receipt_type}/{event_id}` | POST | 设置已读标记 | 认证 |
| `/_matrix/client/r0/rooms/{room_id}/read_markers` | POST | 设置已读标记 | 认证 |

---

## 6. 设备管理 API (8 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/devices` | GET | 获取设备列表 | 认证 |
| `/_matrix/client/r0/delete_devices` | POST | 批量删除设备 | 认证 |
| `/_matrix/client/r0/devices/{device_id}` | GET/PUT/DELETE | 设备CRUD | 认证 |

---

## 7. 推送通知 API (25 个端点)

### 7.1 推送器管理

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/pushers` | GET | 获取推送器列表 | 认证 |
| `/_matrix/client/r0/pushers/set` | POST | 设置推送器 | 认证 |

### 7.2 推送规则

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/pushrules` | GET | 获取推送规则 | 认证 |
| `/_matrix/client/r0/pushrules/{scope}` | GET | 获取范围规则 | 认证 |
| `/_matrix/client/r0/pushrules/{scope}/{kind}` | GET | 获取类型规则 | 认证 |
| `/_matrix/client/r0/pushrules/{scope}/{kind}/{rule_id}` | GET/PUT/DELETE | 规则CRUD | 认证 |

### 7.3 通知

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/notifications` | GET | 获取通知列表 | 认证 |

---

## 8. E2EE 加密 API (15 个端点)

### 8.1 密钥上传

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/keys/upload` | POST | 上传设备密钥 | 认证 |
| `/_matrix/client/r0/keys/query` | POST | 查询设备密钥 | 认证 |
| `/_matrix/client/r0/keys/claim` | POST | 申领一次性密钥 | 认证 |
| `/_matrix/client/r0/keys/changes` | GET | 获取密钥变更 | 认证 |

### 8.2 交叉签名

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/unstable/keys/cross_signing/upload` | POST | 上传交叉签名密钥 | 认证 |
| `/_matrix/client/unstable/keys/cross_signing/sign` | POST | 签名其他用户密钥 | 认证 |

### 8.3 To-Device 消息

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/sendToDevice/{event_type}/{txn_id}` | PUT | 发送到设备消息 | 认证 |

---

## 9. 媒体服务 API (27 个端点)

### 9.1 媒体上传/下载

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/media/r0/upload` | POST | 上传媒体文件 | 认证 |
| `/_matrix/media/r0/download/{server_name}/{media_id}` | GET | 下载媒体文件 | 认证 |
| `/_matrix/media/r0/thumbnail/{server_name}/{media_id}` | GET | 获取缩略图 | 认证 |
| `/_matrix/media/r0/preview_url` | GET | URL预览 | 认证 |

---

## 10. 好友系统 API (9 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/v1/friends` | GET | 获取好友列表 | 认证 |
| `/_matrix/client/v1/friends/{user_id}` | GET/POST/DELETE | 好友CRUD | 认证 |
| `/_matrix/client/v1/friends/{user_id}/accept` | POST | 接受好友请求 | 认证 |
| `/_matrix/client/v1/friends/{user_id}/reject` | POST | 拒绝好友请求 | 认证 |
| `/_matrix/client/v1/friends/{user_id}/block` | POST | 拉黑好友 | 认证 |

---

## 11. Space 空间 API (7 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/v1/spaces` | POST | 创建空间 | 认证 |
| `/_matrix/client/v1/spaces/public` | GET | 获取公开空间 | 认证 |
| `/_matrix/client/v1/spaces/user` | GET | 获取用户空间 | 认证 |
| `/_matrix/client/v1/spaces/{space_id}` | GET/PUT/DELETE | 空间CRUD | 认证 |
| `/_matrix/client/v1/spaces/{space_id}/hierarchy` | GET | 获取空间层级 | 认证 |

---

## 12. Thread 线程 API (16 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/v1/threads` | POST/GET | 创建/获取线程列表 | 认证 |
| `/_matrix/client/v1/threads/{thread_id}` | GET/PUT/DELETE | 线程CRUD | 认证 |
| `/_matrix/client/v1/threads/{thread_id}/reply` | POST | 回复线程 | 认证 |
| `/_matrix/client/v1/threads/{thread_id}/replies` | GET | 获取线程回复 | 认证 |
| `/_matrix/client/v1/threads/{thread_id}/subscribe` | POST | 订阅线程 | 认证 |
| `/_matrix/client/v1/threads/{thread_id}/unsubscribe` | POST | 取消订阅 | 认证 |
| `/_matrix/client/v1/threads/unread` | GET | 获取未读线程 | 认证 |
| `/_matrix/client/v1/threads/{thread_id}/pin` | POST | 置顶线程 | 认证 |

---

## 13. 搜索服务 API (19 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/search` | POST | 搜索消息 | 认证 |
| `/_matrix/client/r0/user_directory/search` | POST | 搜索用户 | 认证 |

---

## 14. 管理后台 API (69 个端点)

### 14.1 服务器管理

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_synapse/admin/v1/server_version` | GET | 获取服务器版本 | 管理员 |
| `/_synapse/admin/v1/server_stats` | GET | 获取服务器统计 | 管理员 |
| `/_synapse/admin/v1/statistics` | GET | 获取统计数据 | 管理员 |

### 14.2 用户管理

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_synapse/admin/v1/users` | GET | 获取用户列表 | 管理员 |
| `/_synapse/admin/v1/users/{user_id}` | GET/DELETE | 用户CRUD | 管理员 |
| `/_synapse/admin/v1/users/{user_id}/admin` | PUT | 设置管理员 | 管理员 |
| `/_synapse/admin/v1/users/{user_id}/login` | POST | 用户登录 | 管理员 |
| `/_synapse/admin/v1/users/{user_id}/devices` | GET | 获取用户设备 | 管理员 |

### 14.3 房间管理

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_synapse/admin/v1/rooms` | GET | 获取房间列表 | 管理员 |
| `/_synapse/admin/v1/rooms/{room_id}` | GET/DELETE | 房间CRUD | 管理员 |
| `/_synapse/admin/v1/rooms/{room_id}/members` | GET | 获取房间成员 | 管理员 |
| `/_synapse/admin/v1/rooms/{room_id}/block` | POST | 封禁房间 | 管理员 |
| `/_synapse/admin/v1/shutdown_room` | POST | 关闭房间 | 管理员 |

### 14.4 安全管理

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_synapse/admin/v1/security/ip/blocks` | GET | 获取IP封禁列表 | 管理员 |
| `/_synapse/admin/v1/security/ip/block` | POST | 封禁IP | 管理员 |

---

## 15. 联邦 API (54 个端点)

### 15.1 服务器发现

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/federation/v2/server` | GET | 获取服务器密钥 | 公开 |
| `/_matrix/federation/v1/version` | GET | 获取服务器版本 | 公开 |

### 15.2 事件同步

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/federation/v1/event/{event_id}` | GET | 获取事件 | 联邦 |
| `/_matrix/federation/v1/state/{room_id}` | GET | 获取房间状态 | 联邦 |
| `/_matrix/federation/v1/backfill/{room_id}` | GET | 回填事件 | 联邦 |

### 15.3 消息发送

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/federation/v1/send/{txn_id}` | PUT | 发送事务 | 联邦 |
| `/_matrix/federation/v1/send_join/{room_id}/{event_id}` | PUT | 发送加入事件 | 联邦 |
| `/_matrix/federation/v1/send_leave/{room_id}/{event_id}` | PUT | 发送离开事件 | 联邦 |
| `/_matrix/federation/v1/invite/{room_id}/{event_id}` | PUT | 发送邀请 | 联邦 |

### 15.4 密钥交换

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/federation/v1/keys/claim` | POST | 申领密钥 | 联邦 |
| `/_matrix/federation/v1/keys/upload` | POST | 上传密钥 | 联邦 |

---

## 16. 应用服务 API (6 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/app/v1/transactions/{txn_id}` | PUT | 接收事务 | 应用服务 |
| `/_matrix/app/v1/users/{user_id}` | GET | 查询用户 | 应用服务 |
| `/_matrix/app/v1/rooms/{room_alias}` | GET | 查询房间 | 应用服务 |

---

## 17. 语音消息 API (16 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/voice/upload` | POST | 上传语音消息 | 认证 |
| `/_matrix/client/r0/voice/stats` | GET | 获取语音统计 | 认证 |
| `/_matrix/client/r0/voice/{message_id}` | GET/DELETE | 语音消息CRUD | 认证 |
| `/_matrix/client/r0/voice/user/{user_id}` | GET | 获取用户语音消息 | 认证 |
| `/_matrix/client/r0/voice/room/{room_id}` | GET | 获取房间语音消息 | 认证 |
| `/_matrix/client/v1/voice/transcription` | POST | 语音转文字 | 认证 |

---

## 18. VoIP 服务 API (0 个端点)

> 注意: VoIP 相关功能已移至客户端 SDK 实现

---

## 19. 验证码服务 API (1 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_synapse/admin/v1/captcha/generate` | POST | 生成验证码 | 公开 |

---

## 20. 后台更新 API (1 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_synapse/admin/v1/background_updates` | GET | 获取更新列表 | 管理员 |

---

## 21. 事件举报 API (6 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/v1/rooms/{room_id}/report/{event_id}` | POST | 举报事件 | 认证 |
| `/_synapse/admin/v1/event_reports` | GET | 获取举报列表 | 管理员 |
| `/_synapse/admin/v1/event_reports/{report_id}` | GET/DELETE | 举报CRUD | 管理员 |

---

## 22. 账户数据 API (2 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/user/{user_id}/account_data/{type}` | GET/PUT | 账户数据CRUD | 认证 |
| `/_matrix/client/r0/user/{user_id}/rooms/{room_id}/account_data/{type}` | GET/PUT | 房间账户数据CRUD | 认证 |

---

## 23. 密钥备份 API (8 个端点)

### 23.1 备份版本管理

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/room_keys/version` | POST/GET | 版本CRUD | 认证 |
| `/_matrix/client/r0/room_keys/version/{version}` | GET/PUT/DELETE | 版本详情CRUD | 认证 |

### 23.2 密钥操作

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/room_keys/keys` | GET/PUT | 密钥CRUD | 认证 |
| `/_matrix/client/r0/room_keys/recover` | POST | 恢复密钥 | 认证 |

---

## 24. 保留策略 API (1 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_synapse/admin/v1/retention/policies` | GET | 获取策略列表 | 管理员 |

---

## 25. 服务器通知 API (1 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_synapse/admin/v1/server_notifications` | GET | 获取通知列表 | 管理员 |

---

## 26. 注册令牌 API (2 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_synapse/admin/v1/registration_tokens` | GET/POST | 令牌CRUD | 管理员 |
| `/_synapse/admin/v1/registration_tokens/validate` | POST | 验证令牌 | 公开 |

---

## 27. 媒体配额 API (8 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_synapse/admin/v1/media/quota` | GET/PUT | 配额设置 | 管理员 |
| `/_synapse/admin/v1/media/quota/users/{user_id}` | GET/PUT | 用户配额 | 管理员 |
| `/_synapse/admin/v1/media/quota/stats` | GET | 配额统计 | 管理员 |

---

## 28. CAS 认证 API (9 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/auth/cas/redirect` | GET | CAS重定向 | 公开 |
| `/_matrix/client/r0/auth/cas/ticket` | GET | CAS票据验证 | 公开 |
| `/_synapse/admin/v1/cas/config` | GET/PUT | CAS配置 | 管理员 |

---

## 29. SAML 认证 API (3 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/auth/saml/redirect` | GET | SAML重定向 | 公开 |
| `/_matrix/client/r0/auth/saml/response` | POST | SAML响应 | 公开 |
| `/_synapse/admin/v1/saml/config` | GET/PUT | SAML配置 | 管理员 |

---

## 30. OIDC 认证 API (2 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/r0/auth/oidc/redirect` | GET | OIDC重定向 | 公开 |
| `/_matrix/client/r0/auth/oidc/callback` | GET | OIDC回调 | 公开 |

---

## 31. Rendezvous API (8 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/v1/rendezvous` | POST | 创建会话 | 公开 |
| `/_matrix/client/v1/rendezvous/{session_id}` | GET/PUT/DELETE | 会话CRUD | 公开 |
| `/_matrix/client/v1/rendezvous/{session_id}/complete` | POST | 完成会话 | 公开 |
| `/_matrix/client/v1/rendezvous/{session_id}/cancel` | POST | 取消会话 | 公开 |

---

## 32. Worker API (9 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_synapse/worker/v1/health` | GET | Worker健康检查 | 内部 |
| `/_synapse/worker/v1/stats` | GET | Worker统计 | 内部 |
| `/_synapse/worker/v1/config` | GET | Worker配置 | 内部 |
| `/_synapse/worker/v1/tasks` | GET | Worker任务 | 内部 |

---

## 33. 联邦黑名单 API (0 个端点)

> 功能已整合到管理后台

---

## 34. 联邦缓存 API (0 个端点)

> 功能已整合到管理后台

---

## 35. 刷新令牌 API (3 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_synapse/admin/v1/refresh_tokens` | GET | 获取令牌列表 | 管理员 |
| `/_synapse/admin/v1/refresh_tokens/{token_id}` | GET/DELETE | 令牌CRUD | 管理员 |
| `/_synapse/admin/v1/refresh_tokens/cleanup` | POST | 清理过期令牌 | 管理员 |

---

## 36. 推送通知管理 API (7 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_synapse/admin/v1/push_notifications` | GET | 获取推送列表 | 管理员 |
| `/_synapse/admin/v1/push_notifications/stats` | GET | 获取推送统计 | 管理员 |
| `/_synapse/admin/v1/push_notifications/retry` | POST | 重试推送 | 管理员 |
| `/_synapse/admin/v1/push_notifications/providers` | GET | 获取推送服务商 | 管理员 |

---

## 37. 速率限制管理 API (8 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_synapse/admin/v1/rate_limits` | GET/POST | 限制CRUD | 管理员 |
| `/_synapse/admin/v1/rate_limits/{limit_id}` | GET/PUT/DELETE | 限制详情 | 管理员 |
| `/_synapse/admin/v1/rate_limits/blocked` | GET | 获取被封禁列表 | 管理员 |
| `/_synapse/admin/v1/rate_limits/unblock/{user_id}` | POST | 解除封禁 | 管理员 |

---

## 38. Sliding Sync API (1 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_matrix/client/unstable/org.matrix.msc3575/sync` | GET | Sliding Sync | 认证 |

---

## 39. 遥测 API (2 个端点)

| 端点 | 方法 | 功能 | 权限 |
|------|------|------|------|
| `/_synapse/admin/v1/telemetry` | GET | 获取遥测数据 | 管理员 |
| `/_synapse/admin/v1/telemetry/config` | GET/PUT | 遥测配置 | 管理员 |

---

## 附录 A: 状态码说明

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

## 附录 B: 错误响应格式

```json
{
  "errcode": "M_ERROR_CODE",
  "error": "Human-readable error message"
}
```

---

## 附录 C: API 版本兼容性

| API版本 | 说明 | 状态 |
|---------|------|------|
| r0 | 旧版API | ⚠️ 兼容 |
| v3 | 当前稳定版本 | ✅ 推荐 |
| v1 | 特定功能版本 | ✅ 支持 |
| unstable | 实验性功能 | ⚠️ 不稳定 |

---

*文档生成完成 - 基于 synapse-rust 项目实际代码统计*
