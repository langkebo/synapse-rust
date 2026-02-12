# Synapse Rust API 完整参考文档

> **版本**：1.0.0
> **生成日期**：2026-02-11
> **基于代码版本**：Synapse Rust Backend
> **说明**：本文档基于后端项目 `synapse/src/web/routes` 下的实际代码自动梳理生成，仅包含当前已实现的 API 接口。

---

## 目录

1. [认证与账号 (Authentication & Account)](#1-认证与账号-authentication--account)
2. [房间管理 (Room Management)](#2-房间管理-room-management)
3. [目录与发现 (Directory & Discovery)](#3-目录与发现-directory--discovery)
4. [在线状态 (Presence)](#4-在线状态-presence)
5. [设备管理 (Device Management)](#5-设备管理-device-management)
6. [媒体 (Media)](#6-媒体-media)
7. [语音消息 (Voice)](#7-语音消息-voice)
8. [端到端加密 (E2EE)](#8-端到端加密-e2ee)
9. [密钥备份 (Key Backup)](#9-密钥备份-key-backup)
10. [管理接口 (Admin API)](#10-管理接口-admin-api)
11. [联邦接口 (Federation API)](#11-联邦接口-federation-api)
12. [增强功能 API (Enhanced API)](#12-增强功能-api-enhanced-api)
13. [通用接口 (General)](#13-通用接口-general)

---

## 1. 认证与账号 (Authentication & Account)

### 1.1 注册
- **接口名称**：用户注册
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/register`
- **功能描述**：注册新用户。
- **请求参数**：
  - `username` (string, 必填): 用户名。
  - `password` (string, 必填): 密码。
  - `auth` (object, 选填): 认证数据。
  - `displayname` (string, 选填): 显示名称。
- **响应数据**：
  - `user_id` (string): 注册后的用户 ID。
  - `access_token` (string): 访问令牌。
  - `device_id` (string): 设备 ID。
  - `home_server` (string): 服务器域名。

### 1.2 检查用户名可用性
- **接口名称**：检查用户名
- **请求方法**：`GET`
- **URL 路径**：`/_matrix/client/r0/register/available`
- **请求参数**：
  - `username` (query, 必填): 待检查的用户名。
- **响应数据**：
  - `available` (boolean): 是否可用。
  - `username` (string): 用户名。

### 1.3 邮箱验证请求
- **接口名称**：请求邮箱验证 Token
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/register/email/requestToken`
- **请求参数**：
  - `email` (string, 必填): 邮箱地址。
  - `client_secret` (string, 选填): 客户端密钥。
- **响应数据**：
  - `sid` (string): 会话 ID。
  - `submit_url` (string): 提交 URL。
  - `expires_in` (integer): 过期时间（秒）。

### 1.4 提交邮箱验证
- **接口名称**：提交邮箱验证 Token
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/register/email/submitToken`
- **请求参数**：
  - `sid` (string, 必填): 会话 ID。
  - `client_secret` (string, 必填): 客户端密钥。
  - `token` (string, 必填): 验证 Token。
- **响应数据**：
  - `success` (boolean): 是否成功。

### 1.5 登录
- **接口名称**：用户登录
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/login`
- **请求参数**：
  - `type` (string, 必填): 登录类型 (通常为 "m.login.password")。
  - `user` / `username` (string, 必填): 用户名。
  - `password` (string, 必填): 密码。
  - `device_id` (string, 选填): 设备 ID。
  - `initial_display_name` (string, 选填): 设备初始显示名。
- **响应数据**：
  - `user_id` (string): 用户 ID。
  - `access_token` (string): 访问令牌。
  - `refresh_token` (string): 刷新令牌。
  - `expires_in` (integer): 过期时间。
  - `device_id` (string): 设备 ID。
  - `well_known` (object): 服务器配置信息。

### 1.6 登出
- **接口名称**：用户登出
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/logout`
- **认证**：Token
- **功能描述**：使当前 Access Token 失效。
- **响应数据**：`{}`

### 1.7 登出所有设备
- **接口名称**：登出所有设备
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/logout/all`
- **认证**：Token
- **功能描述**：使该用户所有 Access Token 失效。
- **响应数据**：`{}`

### 1.8 刷新令牌
- **接口名称**：刷新令牌
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/refresh`
- **请求参数**：
  - `refresh_token` (string, 必填): 刷新令牌。
- **响应数据**：
  - `access_token` (string): 新的访问令牌。
  - `refresh_token` (string): 新的刷新令牌。

### 1.9 获取当前用户信息
- **接口名称**：Whoami
- **请求方法**：`GET`
- **URL 路径**：`/_matrix/client/r0/account/whoami`
- **认证**：Token
- **响应数据**：
  - `user_id` (string): 用户 ID。
  - `displayname` (string): 显示名称。
  - `avatar_url` (string): 头像 URL。
  - `admin` (boolean): 是否为管理员。

### 1.10 获取用户资料
- **接口名称**：获取用户资料
- **请求方法**：`GET`
- **URL 路径**：`/_matrix/client/r0/account/profile/{user_id}`
- **认证**：Token
- **响应数据**：用户资料对象（包含 displayname, avatar_url 等）。

### 1.11 更新显示名称
- **接口名称**：更新显示名称
- **请求方法**：`PUT`
- **URL 路径**：`/_matrix/client/r0/account/profile/{user_id}/displayname`
- **认证**：Token (仅限本人或管理员)
- **请求参数**：
  - `displayname` (string, 必填): 新的显示名称。
- **响应数据**：`{}`

### 1.12 更新头像
- **接口名称**：更新头像
- **请求方法**：`PUT`
- **URL 路径**：`/_matrix/client/r0/account/profile/{user_id}/avatar_url`
- **认证**：Token (仅限本人或管理员)
- **请求参数**：
  - `avatar_url` (string, 必填): 新的头像 URL。
- **响应数据**：`{}`

### 1.13 修改密码
- **接口名称**：修改密码
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/account/password`
- **认证**：Token
- **请求参数**：
  - `new_password` (string, 必填): 新密码。
- **响应数据**：`{}`

### 1.14 注销账户
- **接口名称**：注销账户
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/account/deactivate`
- **认证**：Token
- **功能描述**：停用账户并清除缓存。
- **响应数据**：`{}`

---

## 2. 房间管理 (Room Management)

### 2.1 创建房间
- **接口名称**：创建房间
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/createRoom`
- **认证**：Token
- **请求参数**：
  - `visibility` (string, 选填): "public" 或 "private"。
  - `room_alias_name` (string, 选填): 房间别名。
  - `name` (string, 选填): 房间名称。
  - `topic` (string, 选填): 房间主题。
  - `invite` (list, 选填): 邀请用户列表。
  - `preset` (string, 选填): 预设配置。
- **响应数据**：
  - `room_id` (string): 创建的房间 ID。

### 2.2 加入房间
- **接口名称**：加入房间
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/rooms/{room_id}/join`
- **认证**：Token
- **响应数据**：`{}`

### 2.3 离开房间
- **接口名称**：离开房间
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/rooms/{room_id}/leave`
- **认证**：Token
- **响应数据**：`{}`

### 2.4 邀请用户
- **接口名称**：邀请用户
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/rooms/{room_id}/invite`
- **认证**：Token
- **请求参数**：
  - `user_id` (string, 必填): 被邀请用户 ID。
- **响应数据**：`{}`

### 2.5 踢出用户
- **接口名称**：踢出用户
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/rooms/{room_id}/kick`
- **认证**：Token
- **请求参数**：
  - `user_id` (string, 必填): 目标用户 ID。
  - `reason` (string, 选填): 原因。
- **响应数据**：`{}`

### 2.6 禁止用户 (Ban)
- **接口名称**：禁止用户
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/rooms/{room_id}/ban`
- **认证**：Token
- **请求参数**：
  - `user_id` (string, 必填): 目标用户 ID。
  - `reason` (string, 选填): 原因。
- **响应数据**：`{}`

### 2.7 解除禁止 (Unban)
- **接口名称**：解除禁止
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/rooms/{room_id}/unban`
- **认证**：Token
- **请求参数**：
  - `user_id` (string, 必填): 目标用户 ID。
- **响应数据**：`{}`

### 2.8 发送消息
- **接口名称**：发送消息
- **请求方法**：`PUT`
- **URL 路径**：`/_matrix/client/r0/rooms/{room_id}/send/{event_type}/{txn_id}`
- **认证**：Token
- **请求参数**：
  - `msgtype` (string, 选填): 消息类型 (默认为 "m.room.message")。
  - `body` (string, 必填): 消息内容。
- **响应数据**：
  - `event_id` (string): 事件 ID。

### 2.9 获取消息历史
- **接口名称**：获取消息
- **请求方法**：`GET`
- **URL 路径**：`/_matrix/client/r0/rooms/{room_id}/messages`
- **认证**：Token
- **请求参数**：
  - `from` (string, 选填): 起始 Token。
  - `limit` (integer, 选填): 数量限制 (默认 10)。
  - `dir` (string, 选填): 方向 ("b" 或 "f", 默认 "b")。
- **响应数据**：包含消息列表的 JSON。

### 2.10 同步 (Sync)
- **接口名称**：同步
- **请求方法**：`GET`
- **URL 路径**：`/_matrix/client/r0/sync`
- **认证**：Token
- **请求参数**：
  - `timeout` (integer, 选填): 超时时间 (毫秒)。
  - `full_state` (boolean, 选填): 是否全量同步。
  - `set_presence` (string, 选填): 设置在线状态 (默认 "online")。
- **响应数据**：Matrix 同步响应结构。

### 2.11 房间成员
- **接口名称**：获取房间成员
- **请求方法**：`GET`
- **URL 路径**：`/_matrix/client/r0/rooms/{room_id}/members`
- **认证**：Token
- **响应数据**：成员事件列表。

### 2.12 状态事件操作
- **获取所有状态**：`GET /_matrix/client/r0/rooms/{room_id}/state`
- **获取指定状态**：`GET /_matrix/client/r0/rooms/{room_id}/state/{event_type}/{state_key}`
- **发送状态事件**：`PUT /_matrix/client/r0/rooms/{room_id}/state/{event_type}/{state_key}`

### 2.13 举报事件
- **接口名称**：举报事件
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/rooms/{room_id}/report/{event_id}`
- **认证**：Token
- **请求参数**：
  - `reason` (string, 选填): 举报原因。
  - `score` (integer, 选填): 评分 (默认 -100)。
- **响应数据**：`{ "report_id": ... }`

### 2.14 已读回执与标记
- **发送回执**：`POST /_matrix/client/r0/rooms/{room_id}/receipt/{receipt_type}/{event_id}`
- **设置已读标记**：`POST /_matrix/client/r0/rooms/{room_id}/read_markers`

---

## 3. 目录与发现 (Directory & Discovery)

### 3.1 用户搜索
- **接口名称**：搜索用户
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/user_directory/search`
- **认证**：Token
- **请求参数**：
  - `search_term` (string, 必填): 搜索关键词。
  - `limit` (integer, 选填): 数量限制 (默认 10)。
- **响应数据**：
  - `results` (list): 用户列表。
  - `limited` (boolean): 是否被截断。

### 3.2 用户列表
- **接口名称**：获取用户列表
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/user_directory/list`
- **认证**：Token
- **请求参数**：
  - `limit` (integer, 选填): 数量 (默认 50)。
  - `offset` (integer, 选填): 偏移量 (默认 0)。
- **响应数据**：
  - `users` (list): 用户列表。
  - `total` (integer): 总数。

### 3.3 公共房间
- **获取公共房间**：`GET /_matrix/client/r0/publicRooms`
- **创建公共房间**：`POST /_matrix/client/r0/publicRooms`

### 3.4 房间别名
- **获取房间别名**：`GET /_matrix/client/r0/directory/room/{room_id}/alias`
- **设置房间别名**：`PUT /_matrix/client/r0/directory/room/{room_id}/alias/{room_alias}`
- **删除房间别名**：`DELETE /_matrix/client/r0/directory/room/{room_id}/alias/{room_alias}`
- **通过别名获取房间**：`GET /_matrix/client/r0/directory/room/alias/{room_alias}`

---

## 4. 在线状态 (Presence)

### 4.1 获取状态
- **接口名称**：获取在线状态
- **请求方法**：`GET`
- **URL 路径**：`/_matrix/client/r0/presence/{user_id}/status`
- **认证**：Token
- **响应数据**：
  - `presence` (string): 状态 ("online", "offline", "unavailable")。
  - `status_msg` (string, 选填): 状态消息。

### 4.2 设置状态
- **接口名称**：设置在线状态
- **请求方法**：`PUT`
- **URL 路径**：`/_matrix/client/r0/presence/{user_id}/status`
- **认证**：Token
- **请求参数**：
  - `presence` (string, 必填): 状态。
  - `status_msg` (string, 选填): 状态消息。
- **响应数据**：`{}`

### 4.3 设置正在输入
- **接口名称**：设置正在输入
- **请求方法**：`PUT`
- **URL 路径**：`/_matrix/client/r0/rooms/{room_id}/typing/{user_id}`
- **认证**：Token
- **请求参数**：
  - `typing` (boolean, 必填): 是否正在输入。
- **响应数据**：`{}`

---

## 5. 设备管理 (Device Management)

### 5.1 获取设备列表
- **接口名称**：获取设备列表
- **请求方法**：`GET`
- **URL 路径**：`/_matrix/client/r0/devices`
- **认证**：Token
- **响应数据**：
  - `devices` (list): 设备列表。

### 5.2 获取单个设备
- **接口名称**：获取设备信息
- **请求方法**：`GET`
- **URL 路径**：`/_matrix/client/r0/devices/{device_id}`
- **认证**：Token
- **响应数据**：设备详情。

### 5.3 更新设备
- **接口名称**：更新设备信息
- **请求方法**：`PUT`
- **URL 路径**：`/_matrix/client/r0/devices/{device_id}`
- **认证**：Token
- **请求参数**：
  - `display_name` (string, 必填): 显示名称。
- **响应数据**：`{}`

### 5.4 删除设备
- **删除单个设备**：`DELETE /_matrix/client/r0/devices/{device_id}`
- **批量删除设备**：`POST /_matrix/client/r0/delete_devices` (参数: `devices`: list of device_ids)

---

## 6. 媒体 (Media)

### 6.1 上传媒体
- **接口名称**：上传媒体
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/media/r0/upload`
- **认证**：Token
- **响应数据**：
  - `content_uri` (string): MXC URI。

### 6.2 下载媒体
- **接口名称**：下载媒体
- **请求方法**：`GET`
- **URL 路径**：`/_matrix/media/r0/download/{server_name}/{media_id}`

### 6.3 获取缩略图
- **接口名称**：获取缩略图
- **请求方法**：`GET`
- **URL 路径**：`/_matrix/media/r0/thumbnail/{server_name}/{media_id}`
- **请求参数**：
  - `width` (integer): 宽。
  - `height` (integer): 高。
  - `method` (string): 缩放方式 ("crop", "scale")。

---

## 7. 语音消息 (Voice)

### 7.1 上传语音
- **接口名称**：上传语音消息
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/voice/upload`
- **认证**：Token
- **请求参数**：
  - `content` (string, 必填): Base64 编码的音频数据。
  - `content_type` (string, 选填): MIME 类型 (默认 "audio/ogg")。
  - `duration_ms` (integer, 选填): 时长 (毫秒)。
  - `room_id` (string, 选填): 关联房间 ID。
  - `session_id` (string, 选填): 关联会话 ID。
- **响应数据**：
  - `message_id` (string): 消息 ID。
  - `content_url` (string): 存储 URL/Path。

### 7.2 获取语音消息
- **接口名称**：获取语音消息
- **请求方法**：`GET`
- **URL 路径**：`/_matrix/client/r0/voice/{message_id}`
- **认证**：Token (Implicit via path access check usually, but here explicit state check)
- **响应数据**：
  - `content` (string): Base64 编码内容。
  - `content_type` (string): 类型。

### 7.3 语音统计与配置
- **获取当前用户统计**：`GET /_matrix/client/r0/voice/stats`
- **获取用户统计**：`GET /_matrix/client/r0/voice/user/{user_id}/stats`
- **获取语音配置**：`GET /_matrix/client/r0/voice/config`

### 7.4 语音处理
- **转换语音格式**：`POST /_matrix/client/r0/voice/convert`
- **优化语音大小**：`POST /_matrix/client/r0/voice/optimize`

---

## 8. 端到端加密 (E2EE)

### 8.1 密钥上传
- **接口名称**：上传密钥
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/v3/keys/upload`
- **认证**：Token
- **请求参数**：
  - `device_keys` (object): 设备密钥。
  - `one_time_keys` (object): 一次性密钥。
- **响应数据**：
  - `one_time_key_counts` (object): 各算法剩余密钥数。

### 8.2 密钥查询
- **接口名称**：查询密钥
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/v3/keys/query`
- **认证**：Token
- **请求参数**：
  - `device_keys` (object): 待查询的 `user_id` -> `device_id` 列表。
- **响应数据**：
  - `device_keys` (object): 查询结果。
  - `failures` (object): 失败项。

### 8.3 密钥领取
- **接口名称**：领取一次性密钥
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/v3/keys/claim`
- **认证**：Token
- **请求参数**：
  - `one_time_keys` (object): 待领取的 `user_id` -> `device_id` -> `algorithm`。
- **响应数据**：
  - `one_time_keys` (object): 领取到的密钥。

---

## 9. 密钥备份 (Key Backup)

### 9.1 获取/创建版本
- **创建备份版本**：`POST /_matrix/client/v3/room_keys/version`
- **获取备份版本**：`GET /_matrix/client/v3/room_keys/version`
- **获取指定版本**：`GET /_matrix/client/v3/room_keys/version/{version}`
- **更新指定版本**：`PUT /_matrix/client/v3/room_keys/version/{version}`
- **删除指定版本**：`DELETE /_matrix/client/v3/room_keys/version/{version}`

### 9.2 备份数据操作
- **上传密钥数据**：`PUT /_matrix/client/v3/room_keys/keys/{room_id}/{session_id}`
- **获取密钥数据**：`GET /_matrix/client/v3/room_keys/keys/{room_id}/{session_id}`
- **删除密钥数据**：`DELETE /_matrix/client/v3/room_keys/keys/{room_id}/{session_id}`

---

## 10. 管理接口 (Admin API)

### 10.1 用户管理
- **管理员登录**：`POST /_synapse/admin/v1/users/{user_id}/login`
- **重置密码**：`POST /_synapse/admin/v1/users/{user_id}/password`
- **停用用户**：`POST /_synapse/admin/v1/users/{user_id}/deactivate`
- **创建用户**：`POST /_synapse/admin/v1/users/{user_id}`
- **获取用户信息**：`GET /_synapse/admin/v1/users/{user_id}`
- **列出所有用户**：`GET /_synapse/admin/v1/users`

### 10.2 房间管理
- **列出房间**：`GET /_synapse/admin/v1/rooms`
- **获取房间详情**：`GET /_synapse/admin/v1/rooms/{room_id}`
- **删除房间**：`DELETE /_synapse/admin/v1/rooms/{room_id}`

### 10.3 媒体管理
- **删除媒体**：`DELETE /_synapse/admin/v1/media/{media_id}`
- **清除远程媒体缓存**：`POST /_synapse/admin/v1/purge_remote_media_cache`

---

## 11. 联邦接口 (Federation API)

### 11.1 事务处理
- **发送事务**：`PUT /_matrix/federation/v1/send/{txn_id}`
- **获取版本**：`GET /_matrix/federation/v1/version`

### 11.2 目录与发现
- **获取公钥**：`GET /_matrix/key/v2/server/{key_id}`
- **查询服务器**：`GET /_matrix/federation/v1/query/directory`
- **查询资料**：`GET /_matrix/federation/v1/query/profile`

---

## 12. 增强功能 API (Enhanced API)

> **说明**：本章节包含 Synapse Rust 版本特有的增强功能接口，主要用于支持 `matrix-js-sdk` 中的高级功能，如好友系统和增强版私密聊天。
> **API 前缀**：`/_synapse/enhanced`

### 12.1 好友系统 (Friends System)

#### 12.1.1 获取好友列表
- **接口名称**：获取好友列表
- **请求方法**：`GET`
- **URL 路径**：`/_matrix/client/v1/friends`
- **认证**：Token
- **响应数据**：
  - `friends` (list): 好友列表。

#### 12.1.2 添加好友
- **接口名称**：添加好友
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/v1/friends/request`
- **认证**：Token
- **请求参数**：
  - `user_id` (string, 必填): 目标用户 ID。
- **响应数据**：
  - `room_id` (string): 关联的私聊房间 ID。

### 12.2 私密聊天 (Private Chat)

> **说明**：在优化后的系统中，私密聊天功能已与标准 Matrix 房间机制融合。
> 1. **创建私聊**：使用标准创建房间接口 `POST /_matrix/client/r0/createRoom`，并设置 `preset: "trusted_private_chat"` 和 `is_direct: true`。
> 2. **消息传输**：使用标准发送消息接口 `PUT /_matrix/client/r0/rooms/{room_id}/send/{event_type}/{txn_id}`。
> 3. **端到端加密**：通过标准 E2EE 接口管理密钥，并在发送消息时使用 `m.room.encrypted` 事件类型。

之前的增强版 API (`/_synapse/enhanced/private/*`) 已被废弃，建议客户端全面迁移到标准 Matrix 房间 API。

### 12.3 其他增强功能

#### 12.3.1 语音消息增强
- **接口名称**：转换语音格式
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/voice/convert`
- **功能描述**：服务端协助转换音频格式（如 Silk 转 Ogg）。

#### 12.3.2 媒体优化
- **接口名称**：优化媒体资源
- **请求方法**：`POST`
- **URL 路径**：`/_matrix/client/r0/voice/optimize`
- **功能描述**：压缩或优化媒体文件大小。

---

## 13. 通用接口 (General)

### 12.1 服务器信息
- **首页信息**：`GET /` (返回 "Synapse Rust Matrix Server")
- **健康检查**：`GET /health` (返回健康状态及 DB/Cache 状态)
- **版本信息**：
  - `GET /_matrix/client/versions`
  - `GET /_matrix/client/r0/version`

