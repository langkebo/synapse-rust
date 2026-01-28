# API 参考文档

> **版本**：1.0.0  
> **创建日期**：2026-01-28  
> **项目状态**：开发中  
> **参考文档**：[Synapse 官方文档](https://element-hq.github.io/synapse/latest/)、[Matrix 规范](https://spec.matrix.org/)

---

## 一、Matrix 协议规范参考

### 1.1 Matrix 协议概述

Matrix 是一个开放的、去中心化的即时通讯协议，允许用户在联邦化的服务器网络中进行安全通信。Synapse Rust 项目实现了 Matrix 协议的 Homeserver 功能，包括客户端 API、联邦 API 和管理 API。

### 1.2 Matrix 规范版本

| 规范版本 | 状态 | 描述 |
|-----------|------|------|
| v1.11 | 稳定 | 当前稳定版本 |
| v1.10 | 稳定 | 之前稳定版本 |
| v1.9 | 稳定 | 旧版本 |

### 1.3 API 版本兼容性

Synapse Rust 项目支持以下 API 版本：

| API | 版本 | 兼容性 |
|-----|------|--------|
| Client API | r0, v1, v3 | 完全兼容 |
| Federation API | v1 | 完全兼容 |
| Admin API | v1, v3 | 完全兼容 |
| Media API | v1 | 完全兼容 |

### 1.4 Matrix 规范文档链接

- [Matrix 客户端-服务器 API 规范](https://spec.matrix.org/v1.11/client-server-api/)
- [Matrix 联邦 API 规范](https://spec.matrix.org/v1.11/server-server-api/)
- [Matrix 应用服务 API 规范](https://spec.matrix.org/v1.11/application-service-api/)
- [Matrix 推送网关 API 规范](https://spec.matrix.org/v1.11/push-gateway-api/)
- [Matrix 身份服务 API 规范](https://spec.matrix.org/v1.11/identity-service-api/)

---

## 二、Synapse 官方文档链接

### 2.1 核心文档

- [Synapse 官方主页](https://element-hq.github.io/synapse/latest/)
- [Synapse 安装指南](https://element-hq.github.io/synapse/latest/install/)
- [Synapse 配置指南](https://element-hq.github.io/synapse/latest/configure/)
- [Synapse 升级指南](https://element-hq.github.io/synapse/latest/upgrade/)
- [Synapse 管理指南](https://element-hq.github.io/synapse/latest/usage/administration/)

### 2.2 开发文档

- [Synapse 贡献指南](https://element-hq.github.io/synapse/latest/contributing_guide/)
- [Synapse 架构文档](https://element-hq.github.io/synapse/latest/development/synapse_architecture/)
- [Synapse 数据库模式](https://element-hq.github.io/synapse/latest/development/database_schemas/)
- [Synapse 测试指南](https://element-hq.github.io/synapse/latest/development/testing/)

### 2.3 API 文档

- [Synapse Admin API](https://element-hq.github.io/synapse/latest/usage/administration/admin_api/)
- [Synapse Client API](https://element-hq.github.io/synapse/latest/usage/administration/client_server_api/)
- [Synapse Federation API](https://element-hq.github.io/synapse/latest/usage/administration/federation_api/)

---

## 三、API 端点分类

### 3.1 客户端 API (Client API)

客户端 API 供 Matrix 客户端（如 Element、Riot 等）与 Homeserver 通信使用。

#### 3.1.1 认证相关

| 方法 | 路径 | 描述 | Matrix 规范 |
|------|------|------|-------------|
| POST | `/_matrix/client/r0/register` | 用户注册 | [注册](https://spec.matrix.org/v1.11/client-server-api/#post_matrixclientv3register) |
| POST | `/_matrix/client/r0/login` | 用户登录 | [登录](https://spec.matrix.org/v1.11/client-server-api/#post_matrixclientv3login) |
| POST | `/_matrix/client/r0/logout` | 用户登出 | [登出](https://spec.matrix.org/v1.11/client-server-api/#post_matrixclientv3logout) |
| POST | `/_matrix/client/r0/logout/all` | 登出所有设备 | [登出所有](https://spec.matrix.org/v1.11/client-server-api/#post_matrixclientv3logoutall) |

#### 3.1.2 同步相关

| 方法 | 路径 | 描述 | Matrix 规范 |
|------|------|------|-------------|
| GET | `/_matrix/client/r0/sync` | 同步事件 | [同步](https://spec.matrix.org/v1.11/client-server-api/#get_matrixclientv3sync) |

#### 3.1.3 房间相关

| 方法 | 路径 | 描述 | Matrix 规范 |
|------|------|------|-------------|
| POST | `/_matrix/client/r0/createRoom` | 创建房间 | [创建房间](https://spec.matrix.org/v1.11/client-server-api/#post_matrixclientv3createroom) |
| POST | `/_matrix/client/r0/rooms/{room_id}/join` | 加入房间 | [加入房间](https://spec.matrix.org/v1.11/client-server-api/#post_matrixclientv3roomsroomidjoin) |
| POST | `/_matrix/client/r0/rooms/{room_id}/leave` | 离开房间 | [离开房间](https://spec.matrix.org/v1.11/client-server-api/#post_matrixclientv3roomsroomidleave) |
| GET | `/_matrix/client/r0/rooms/{room_id}/messages` | 获取房间消息 | [获取消息](https://spec.matrix.org/v1.11/client-server-api/#get_matrixclientv3roomsroomidmessages) |
| PUT | `/_matrix/client/r0/rooms/{room_id}/send/{event_type}/{txn_id}` | 发送房间消息 | [发送消息](https://spec.matrix.org/v1.11/client-server-api/#put_matrixclientv3roomsroomidsendeventtypetxnid) |
| GET | `/_matrix/client/r0/rooms/{room_id}/members` | 获取房间成员 | [获取成员](https://spec.matrix.org/v1.11/client-server-api/#get_matrixclientv3roomsroomidmembers) |
| GET | `/_matrix/client/r0/rooms/{room_id}/state` | 获取房间状态 | [获取状态](https://spec.matrix.org/v1.11/client-server-api/#get_matrixclientv3roomsroomidstate) |

#### 3.1.4 设备相关

| 方法 | 路径 | 描述 | Matrix 规范 |
|------|------|------|-------------|
| GET | `/_matrix/client/r0/devices` | 获取用户设备列表 | [获取设备](https://spec.matrix.org/v1.11/client-server-api/#get_matrixclientv3devices) |
| POST | `/_matrix/client/r0/delete_devices` | 删除设备 | [删除设备](https://spec.matrix.org/v1.11/client-server-api/#post_matrixclientv3delete_devices) |

### 3.2 联邦 API (Federation API)

联邦 API 供 Homeserver 之间通信使用，实现联邦化网络。

| 方法 | 路径 | 描述 | Matrix 规范 |
|------|------|------|-------------|
| GET | `/_matrix/federation/v1/version` | 获取服务器版本 | [服务器版本](https://spec.matrix.org/v1.11/server-server-api/#get_matrixfederationv1version) |
| PUT | `/_matrix/federation/v1/send/{txn_id}` | 发送联邦事务 | [发送事务](https://spec.matrix.org/v1.11/server-server-api/#put_matrixfederationv1sendtxn_id) |
| GET | `/_matrix/federation/v1/backfill/{room_id}` | 填充历史事件 | [填充历史](https://spec.matrix.org/v1.11/server-server-api/#get_matrixfederationv1backfillroom_id) |
| GET | `/_matrix/federation/v1/state/{room_id}` | 获取房间状态 | [获取状态](https://spec.matrix.org/v1.11/server-server-api/#get_matrixfederationv1stateroom_id) |
| GET | `/_matrix/federation/v1/event/{event_id}` | 获取事件 | [获取事件](https://spec.matrix.org/v1.11/server-server-api/#get_matrixfederationv1eventevent_id) |
| GET | `/_matrix/federation/v1/query/directory` | 查询房间目录 | [查询目录](https://spec.matrix.org/v1.11/server-server-api/#get_matrixfederationv1querydirectory) |

### 3.3 Enhanced API

Enhanced API 是 Synapse Rust 项目特有的增强功能 API，包括好友管理、私聊管理、语音消息和安全控制。

#### 3.3.1 好友管理 API

| 方法 | 路径 | 描述 | 认证 |
|------|------|------|------|
| GET | `/_synapse/enhanced/friends` | 获取好友列表 | 是 |
| POST | `/_synapse/enhanced/friend/request` | 发送好友请求 | 是 |
| POST | `/_synapse/enhanced/friend/request/{request_id}/respond` | 响应好友请求 | 是 |
| GET | `/_synapse/enhanced/friend/requests` | 获取好友请求列表 | 是 |
| GET | `/_synapse/enhanced/friend/categories` | 获取好友分类 | 是 |
| POST | `/_synapse/enhanced/friend/categories` | 创建好友分类 | 是 |
| PUT | `/_synapse/enhanced/friend/categories/{category_name}` | 更新好友分类 | 是 |
| DELETE | `/_synapse/enhanced/friend/categories/{category_name}` | 删除好友分类 | 是 |
| GET | `/_synapse/enhanced/friend/blocks` | 获取黑名单 | 是 |
| POST | `/_synapse/enhanced/friend/blocks` | 添加到黑名单 | 是 |
| DELETE | `/_synapse/enhanced/friend/blocks/{user_id}` | 从黑名单移除 | 是 |
| GET | `/_synapse/enhanced/friend/recommendations` | 获取好友推荐 | 是 |

#### 3.3.2 私聊管理 API

| 方法 | 路径 | 描述 | 认证 |
|------|------|------|------|
| GET | `/_synapse/enhanced/private/sessions` | 获取私聊会话列表 | 是 |
| POST | `/_synapse/enhanced/private/sessions` | 创建私聊会话 | 是 |
| GET | `/_synapse/enhanced/private/sessions/{session_id}` | 获取私聊会话详情 | 是 |
| DELETE | `/_synapse/enhanced/private/sessions/{session_id}` | 删除私聊会话 | 是 |
| GET | `/_synapse/enhanced/private/sessions/{session_id}/messages` | 获取私聊消息 | 是 |
| POST | `/_synapse/enhanced/private/sessions/{session_id}/messages` | 发送私聊消息 | 是 |
| DELETE | `/_synapse/enhanced/private/messages/{message_id}` | 删除私聊消息 | 是 |
| POST | `/_synapse/enhanced/private/messages/{message_id}/read` | 标记消息已读 | 是 |
| GET | `/_synapse/enhanced/private/unread-count` | 获取未读消息数 | 是 |
| POST | `/_synapse/enhanced/private/search` | 搜索私聊消息 | 是 |

#### 3.3.3 语音消息 API

| 方法 | 路径 | 描述 | 认证 |
|------|------|------|------|
| POST | `/_synapse/enhanced/voice/upload` | 上传语音消息 | 是 |
| GET | `/_synapse/enhanced/voice/messages/{message_id}` | 获取语音消息详情 | 是 |
| DELETE | `/_synapse/enhanced/voice/messages/{message_id}` | 删除语音消息 | 是 |
| GET | `/_synapse/enhanced/voice/user/{user_id}` | 获取用户语音消息列表 | 是 |
| GET | `/_synapse/enhanced/voice/user/{user_id}/stats` | 获取用户语音消息统计 | 是 |

### 3.4 Admin API

Admin API 供管理员管理 Homeserver 使用。

#### 3.4.1 安全控制 API

| 方法 | 路径 | 描述 | 认证 |
|------|------|------|------|
| GET | `/_synapse/admin/v1/security/events` | 获取安全事件 | 是（管理员） |
| GET | `/_synapse/admin/v1/security/ip/blocks` | 获取被阻止的 IP 列表 | 是（管理员） |
| POST | `/_synapse/admin/v1/security/ip/block` | 阻止 IP 地址 | 是（管理员） |
| POST | `/_synapse/admin/v1/security/ip/unblock` | 解除 IP 阻止 | 是（管理员） |
| GET | `/_synapse/admin/v1/security/ip/reputation/{ip}` | 获取 IP 声誉 | 是（管理员） |
| GET | `/_synapse/admin/v1/status` | 获取系统状态 | 是（管理员） |

---

## 四、API 兼容性说明

### 4.1 Matrix 协议兼容性

Synapse Rust 项目完全兼容 Matrix 协议 v1.11 规范，确保与所有 Matrix 客户端和 Homeserver 的互操作性。

### 4.2 Synapse 兼容性

Synapse Rust 项目与原 Synapse Python 实现保持 API 兼容性，包括：

- 客户端 API 完全兼容
- 联邦 API 完全兼容
- Admin API 完全兼容
- Enhanced API 完全兼容

### 4.3 版本迁移

从 Synapse Python 迁移到 Synapse Rust 时，需要注意以下几点：

1. **数据库迁移**：需要运行数据库迁移脚本，将数据从 Synapse Python 的数据库迁移到 Synapse Rust 的数据库
2. **配置迁移**：需要将 Synapse Python 的配置文件转换为 Synapse Rust 的配置文件
3. **API 兼容性**：API 接口保持完全兼容，无需修改客户端代码

---

## 五、API 使用指南

### 5.1 认证方式

#### 5.1.1 访问令牌认证

大多数 API 需要使用访问令牌进行认证。访问令牌通过登录或注册获取，并在请求头中携带：

```http
Authorization: Bearer <access_token>
```

#### 5.1.2 管理员认证

Admin API 需要管理员权限。管理员用户在数据库中标记为 `admin: true`。

### 5.2 请求格式

#### 5.2.1 JSON 请求

大多数 API 使用 JSON 格式进行请求和响应：

```http
POST /_matrix/client/r0/login HTTP/1.1
Content-Type: application/json

{
  "type": "m.login.password",
  "user": "username",
  "password": "password"
}
```

#### 5.2.2 表单数据

部分 API 使用表单数据格式：

```http
POST /_matrix/media/v1/upload HTTP/1.1
Content-Type: multipart/form-data; boundary=----WebKitFormBoundary

------WebKitFormBoundary
Content-Disposition: form-data; name="file"; filename="audio.mp3"
Content-Type: audio/mpeg

[audio data]
------WebKitFormBoundary--
```

### 5.3 响应格式

#### 5.3.1 成功响应

成功响应使用 HTTP 状态码 200 OK，并返回 JSON 数据：

```json
{
  "user_id": "@username:server.com",
  "access_token": "token",
  "device_id": "device_id"
}
```

#### 5.3.2 错误响应

错误响应使用相应的 HTTP 状态码（4xx 或 5xx），并返回错误信息：

```json
{
  "errcode": "M_UNKNOWN",
  "error": "Unknown error"
}
```

### 5.4 错误码

| 错误码 | HTTP 状态码 | 描述 |
|--------|------------|------|
| M_UNKNOWN | 500 | 未知错误 |
| M_BAD_JSON | 400 | JSON 格式错误 |
| M_NOT_JSON | 400 | 非 JSON 请求 |
| M_NOT_FOUND | 404 | 资源未找到 |
| M_LIMIT_EXCEEDED | 429 | 请求频率超限 |
| M_USER_IN_USE | 400 | 用户名已被使用 |
| M_INVALID_USERNAME | 400 | 用户名无效 |
| M_MISSING_PARAM | 400 | 缺少必需参数 |
| M_INVALID_PARAM | 400 | 参数无效 |
| M_FORBIDDEN | 403 | 禁止访问 |
| M_UNAUTHORIZED | 401 | 未授权 |

---

## 六、参考资料

### 6.1 Matrix 规范

- [Matrix 客户端-服务器 API 规范](https://spec.matrix.org/v1.11/client-server-api/)
- [Matrix 联邦 API 规范](https://spec.matrix.org/v1.11/server-server-api/)
- [Matrix 应用服务 API 规范](https://spec.matrix.org/v1.11/application-service-api/)
- [Matrix 推送网关 API 规范](https://spec.matrix.org/v1.11/push-gateway-api/)
- [Matrix 身份服务 API 规范](https://spec.matrix.org/v1.11/identity-service-api/)

### 6.2 Synapse 文档

- [Synapse 官方主页](https://element-hq.github.io/synapse/latest/)
- [Synapse Admin API](https://element-hq.github.io/synapse/latest/usage/administration/admin_api/)
- [Synapse Client API](https://element-hq.github.io/synapse/latest/usage/administration/client_server_api/)
- [Synapse Federation API](https://element-hq.github.io/synapse/latest/usage/administration/federation_api/)

### 6.3 Rust 框架文档

- [Axum 框架文档](https://docs.rs/axum/latest/axum/)
- [SQLx 文档](https://docs.rs/sqlx/latest/sqlx/)
- [Tokio 文档](https://docs.rs/tokio/latest/tokio/)
- [Serde 文档](https://docs.rs/serde/latest/serde/)

---

## 七、变更日志

| 版本 | 日期 | 变更说明 |
|------|------|----------|
| 1.0.0 | 2026-01-28 | 初始版本，定义 API 参考文档 |
