# Matrix API 全面测试报告

> **测试日期**: 2026-02-04
> **测试环境**: localhost:8008
> **服务器域名**: cjystx.top
> **测试用户**:
>   - admin: @admin:cjystx.top
>   - testuser1: @testuser1:cjystx.top
>   - testuser2: @testuser2:cjystx.top

---

## 测试摘要

| 指标 | 数值 |
|------|------|
| 总测试数 | 0 |
| 通过 | 0 |
| 失败 | 0 |
| 跳过 | 0 |
| 成功率 | 0% |

---

## 测试环境信息

### 服务状态
synapse_rust       Up 3 hours (healthy)   0.0.0.0:8008->8008/tcp, [::]:8008->8008/tcp
synapse_redis      Up 3 hours (healthy)   6379/tcp
synapse_postgres   Up 3 hours (healthy)   5432/tcp

### API 版本
```json
{"unstable_features":{"m.lazy_load_members":true,"m.require_identity_server":false,"m.supports_login_via_phone_number":true},"versions":["r0.0.1","r0.1.0","r0.2.0","r0.3.0","r0.4.0","r0.5.0","r0.6.0"]}
```

---

## 测试用例详情

## 1. 核心客户端 API 测试

### 获取客户端版本

- **方法**: GET /_matrix/client/versions
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.004562s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"unstable_features":{"m.lazy_load_members":true,"m.require_identity_server":false,"m.supports_login_via_phone_number":true},"versions":["r0.0.1","r0.1.0","r0.2.0","r0.3.0","r0.4.0","r0.5.0","r0.6.0"]}
```
- **结果**: ✅ 通过

---
### 用户登录(testuser1)

- **方法**: POST /_matrix/client/r0/login
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.159854s
- **请求体**: 
```json
{"type":"m.login.password","user":"testuser1","password":"TestUser123456!"}
```
- **响应体**: 
```json
{"access_token":"eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXIxOmNqeXN0eC50b3AiLCJ1c2VyX2lkIjoiQHRlc3R1c2VyMTpjanlzdHgudG9wIiwiYWRtaW4iOmZhbHNlLCJleHAiOjE3NzAyMDI2MzMsImlhdCI6MTc3MDE5OTAzMywiZGV2aWNlX2lkIjoiakZmWkgzVFNyUXdnUFYzYSJ9.L-1m9icy1rzz7NzQNPZ-BxYW0q-O9Tk2H72suQ_bLvg","device_id":"jFfZH3TSrQwgPV3a","expires_in":3600,"refresh_token":"ilLZ_lRIxs8IcbF3D9E5nvzKsMdIdkf_DbLvzb-djXw","user_id":"@testuser1:cjystx.top","well_known":{"m.homeserver":{"base_url":"http://cjystx.top:8008"}}}
```
- **结果**: ✅ 通过

---
### 账户WhoAmI(testuser1)

- **方法**: GET /_matrix/client/r0/account/whoami
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.008820s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"admin":false,"avatar_url":null,"displayname":"testuser1","user_id":"@testuser1:cjystx.top"}
```
- **结果**: ✅ 通过

---
### 获取用户资料

- **方法**: GET /_matrix/client/r0/profile/@testuser1:cjystx.top
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.004502s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"errcode":"UNKNOWN","error":"Unknown endpoint"}
```
- **结果**: ✅ 通过

---
### 更新用户显示名

- **方法**: PUT /_matrix/client/r0/profile/@testuser1:cjystx.top/displayname
- **状态码**: 405 (预期: 200)
- **响应时间**: 0.004478s
- **请求体**: 
```json
{"displayname":"Test User 1"}
```
- **响应体**: 
```json

```
- **结果**: ❌ 失败

---
### 获取公共房间列表

- **方法**: GET /_matrix/client/r0/publicRooms
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.007590s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"chunk":[{"canonical_alias":null,"is_public":true,"join_rule":"invite","name":"Test Room 1","room_id":"!BfUBQVVQfR0EQUmS9kwF_EQ2:matrix.cjystx.top","topic":"Test room for API testing"}],"total_room_count_estimate":1}
```
- **结果**: ✅ 通过

---
### 获取设备列表

- **方法**: GET /_matrix/client/r0/devices
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.007441s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"devices":[{"device_id":"8qXt54JPjmvBvbNc","display_name":null,"last_seen_ts":1770199033210,"user_id":"@testuser1:cjystx.top"},{"device_id":"jFfZH3TSrQwgPV3a","display_name":null,"last_seen_ts":1770199033039,"user_id":"@testuser1:cjystx.top"},{"device_id":"tSCe1Nqj2d29rnyB","display_name":null,"last_seen_ts":1770199032676,"user_id":"@testuser1:cjystx.top"},{"device_id":"dtdLm3E3haEJkVtb","display_name":null,"last_seen_ts":1770186841255,"user_id":"@testuser1:cjystx.top"},{"device_id":"zAZvmWyxp8SeXIs_PnuqkQ","display_name":null,"last_seen_ts":1770186821809,"user_id":"@testuser1:cjystx.top"}]}
```
- **结果**: ✅ 通过

---
### 刷新访问令牌

- **方法**: POST /_matrix/client/r0/tokenrefresh
- **状态码**: 405 (预期: 400)
- **响应时间**: 0.004275s
- **请求体**: 
```json
{"refresh_token":"eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXIxOmNqeXN0eC50b3AiLCJ1c2VyX2lkIjoiQHRlc3R1c2VyMTpjanlzdHgudG9wIiwiYWRtaW4iOmZhbHNlLCJleHAiOjE3NzAyMDI2MzIsImlhdCI6MTc3MDE5OTAzMiwiZGV2aWNlX2lkIjoidFNDZTFOcWoyZDI5cm55QiJ9.g5alxAaB3173UtAhAKr3AXA2kpBPRylqcgKp2HlcOSo"}
```
- **响应体**: 
```json

```
- **结果**: ❌ 失败

---
## 2. 管理员 API 测试

### 服务器版本(普通用户)

- **方法**: GET /_synapse/admin/v1/server_version
- **状态码**: 403 (预期: 403)
- **响应时间**: 0.006187s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"errcode":"M_FORBIDDEN","error":"Admin access required"}
```
- **结果**: ✅ 通过

---
### 服务器版本(管理员)

- **方法**: GET /_synapse/admin/v1/server_version
- **状态码**: 403 (预期: 200)
- **响应时间**: 0.007587s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"errcode":"M_FORBIDDEN","error":"Admin access required"}
```
- **结果**: ❌ 失败

---
### 管理员用户列表

- **方法**: GET /_synapse/admin/v2/users
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.004379s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"errcode":"UNKNOWN","error":"Unknown endpoint"}
```
- **结果**: ✅ 通过

---
### 管理员获取用户信息

- **方法**: GET /_synapse/admin/v2/users/@testuser1:cjystx.top
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.004325s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"errcode":"UNKNOWN","error":"Unknown endpoint"}
```
- **结果**: ✅ 通过

---
### 管理员创建用户

- **方法**: POST /_synapse/admin/v1/register
- **状态码**: 422 (预期: 200)
- **响应时间**: 0.005474s
- **请求体**: 
```json
{"username":"newuser","password":"NewPass123!","admin":false}
```
- **响应体**: 
```json
Failed to deserialize the JSON body into the target type: missing field `nonce` at line 1 column 61
```
- **结果**: ❌ 失败

---
### 管理员删除测试用户

- **方法**: DELETE /_synapse/admin/v2/users/@newuser:cjystx.top
- **状态码**: 405 (预期: 200)
- **响应时间**: 0.004267s
- **请求体**: 
```json

```
- **响应体**: 
```json

```
- **结果**: ❌ 失败

---
## 3. 认证与错误处理测试

### 无效令牌访问

- **方法**: GET /_matrix/client/r0/account/whoami
- **状态码**: 401 (预期: 401)
- **响应时间**: 0.005848s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"errcode":"M_UNAUTHORIZED","error":"Invalid token: InvalidToken"}
```
- **结果**: ✅ 通过

---
### 无令牌访问

- **方法**: GET /_matrix/client/r0/account/whoami
- **状态码**: 401 (预期: 401)
- **响应时间**: 0.005363s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"errcode":"M_UNAUTHORIZED","error":"Missing or invalid authorization header"}
```
- **结果**: ✅ 通过

---
### 错误密码登录

- **方法**: POST /_matrix/client/r0/login
- **状态码**: 401 (预期: 403)
- **响应时间**: 0.005279s
- **请求体**: 
```json
{"type":"m.login.password","user":"testuser1","password":"WrongPass123!"}
```
- **响应体**: 
```json
{"errcode":"M_UNAUTHORIZED","error":"Invalid credentials"}
```
- **结果**: ❌ 失败

---
### 无效用户名登录

- **方法**: POST /_matrix/client/r0/login
- **状态码**: 429 (预期: 403)
- **响应时间**: 0.005269s
- **请求体**: 
```json
{"type":"m.login.password","user":"nonexistent","password":"Pass123!"}
```
- **响应体**: 
```json
{"errcode":"M_LIMIT_EXCEEDED","error":"Rate limited","retry_after_ms":1000}
```
- **结果**: ❌ 失败

---
### 重复注册

- **方法**: POST /_matrix/client/r0/register
- **状态码**: 409 (预期: 400)
- **响应时间**: 0.006412s
- **请求体**: 
```json
{"username":"testuser1","password":"TestUser123456!","admin":false}
```
- **响应体**: 
```json
{"errcode":"M_USER_IN_USE","error":"Username already taken"}
```
- **结果**: ❌ 失败

---
## 4. 好友系统 API 测试

### 获取好友列表

- **方法**: GET /_matrix/client/r0/contacts
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.004478s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"errcode":"UNKNOWN","error":"Unknown endpoint"}
```
- **结果**: ✅ 通过

---
### 获取好友分类列表

- **方法**: GET /_matrix/client/r0/contacts/categories
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.004399s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"errcode":"UNKNOWN","error":"Unknown endpoint"}
```
- **结果**: ✅ 通过

---
### 创建好友分类

- **方法**: POST /_matrix/client/r0/contacts/categories
- **状态码**: 405 (预期: 200)
- **响应时间**: 0.004294s
- **请求体**: 
```json
{"name":"Family","order":1}
```
- **响应体**: 
```json

```
- **结果**: ❌ 失败

---
### 获取好友分类

- **方法**: GET /_matrix/client/r0/contacts/categories/1
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.004406s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"errcode":"UNKNOWN","error":"Unknown endpoint"}
```
- **结果**: ✅ 通过

---
### 更新好友分类

- **方法**: PUT /_matrix/client/r0/contacts/categories/1
- **状态码**: 405 (预期: 200)
- **响应时间**: 0.004372s
- **请求体**: 
```json
{"name":"Family Updated","order":2}
```
- **响应体**: 
```json

```
- **结果**: ❌ 失败

---
### 邀请用户为好友

- **方法**: POST /_matrix/client/r0/contacts/request
- **状态码**: 405 (预期: 200)
- **响应时间**: 0.004405s
- **请求体**: 
```json
{"user_id":"@testuser2:cjystx.top"}
```
- **响应体**: 
```json

```
- **结果**: ❌ 失败

---
### 接受好友请求

- **方法**: POST /_matrix/client/r0/contacts/accept
- **状态码**: 405 (预期: 200)
- **响应时间**: 0.004286s
- **请求体**: 
```json
{"user_id":"@testuser1:cjystx.top"}
```
- **响应体**: 
```json

```
- **结果**: ❌ 失败

---
## 5. 媒体文件 API 测试

### 上传媒体文件

- **方法**: POST /_matrix/media/r0/upload
- **状态码**: 405 (预期: 415)
- **响应时间**: 0.004378s
- **请求体**: 
```json
{"filename":"test_image.png"}
```
- **响应体**: 
```json

```
- **结果**: ❌ 失败

---
### 获取媒体配置

- **方法**: GET /_matrix/media/r0/config
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.004388s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"errcode":"UNKNOWN","error":"Unknown endpoint"}
```
- **结果**: ✅ 通过

---
### 获取用户媒体库

- **方法**: GET /_matrix/media/r0/user/@testuser1:cjystx.top
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.004297s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"errcode":"UNKNOWN","error":"Unknown endpoint"}
```
- **结果**: ✅ 通过

---
## 6. 私聊 API 测试

### 创建私聊房间

- **方法**: POST /_matrix/client/r0/createRoom
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.013540s
- **请求体**: 
```json
{"visibility":"private","name":"Private Chat Room"}
```
- **响应体**: 
```json
{"room_alias":null,"room_id":"!qQZK8J6o53EJU28jdqrZnkpJ:cjystx.top"}
```
- **结果**: ✅ 通过

---
### 获取房间信息

- **方法**: GET /_matrix/client/r0/rooms/!Il60dorLqYynZTWDvR0Gwg57:cjystx.top
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.004459s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"errcode":"UNKNOWN","error":"Unknown endpoint"}
```
- **结果**: ✅ 通过

---
### 获取用户房间列表

- **方法**: GET /_matrix/client/r0/sync
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.010435s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"account_data":{"events":[]},"next_batch":"s1770199034359","presence":{"events":[]},"rooms":{"!9v6CLP5l7dguINtAZVmV7qV7:cjystx.top":{"account_data":{},"ephemeral":{},"state":{},"timeline":{"events":[],"limited":true,"prev_batch":"t1770199034359"},"unread_notifications":{"highlight_count":0,"notification_count":0}},"!Il60dorLqYynZTWDvR0Gwg57:cjystx.top":{"account_data":{},"ephemeral":{},"state":{},"timeline":{"events":[],"limited":true,"prev_batch":"t1770199034357"},"unread_notifications":{"highlight_count":0,"notification_count":0}},"!qQZK8J6o53EJU28jdqrZnkpJ:cjystx.top":{"account_data":{},"ephemeral":{},"state":{},"timeline":{"events":[],"limited":true,"prev_batch":"t1770199034358"},"unread_notifications":{"highlight_count":0,"notification_count":0}}},"to_device":{"events":[]}}
```
- **结果**: ✅ 通过

---
### 发送房间消息

- **方法**: POST /_matrix/client/r0/rooms/!Il60dorLqYynZTWDvR0Gwg57:cjystx.top/send/m.room.message
- **状态码**: 405 (预期: 200)
- **响应时间**: 0.004566s
- **请求体**: 
```json
{"msgtype":"m.text","body":"Hello World"}
```
- **响应体**: 
```json

```
- **结果**: ❌ 失败

---
### 获取房间消息

- **方法**: GET /_matrix/client/r0/rooms/!Il60dorLqYynZTWDvR0Gwg57:cjystx.top/messages
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.007318s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"chunk":[],"end":"e1770199034427","start":"0"}
```
- **结果**: ✅ 通过

---
### 邀请用户到房间

- **方法**: POST /_matrix/client/r0/rooms/!Il60dorLqYynZTWDvR0Gwg57:cjystx.top/invite
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.010011s
- **请求体**: 
```json
{"user_id":"@testuser2:cjystx.top"}
```
- **响应体**: 
```json
{}
```
- **结果**: ✅ 通过

---
### 离开房间

- **方法**: POST /_matrix/client/r0/rooms/!Il60dorLqYynZTWDvR0Gwg57:cjystx.top/leave
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.009234s
- **请求体**: 
```json

```
- **响应体**: 
```json
{}
```
- **结果**: ✅ 通过

---
## 7. 端到端加密 API 测试

### 获取设备密钥

- **方法**: GET /_matrix/client/r0/keys/query
- **状态码**: 405 (预期: 200)
- **响应时间**: 0.005088s
- **请求体**: 
```json

```
- **响应体**: 
```json

```
- **结果**: ❌ 失败

---
### 上传设备密钥

- **方法**: POST /_matrix/client/r0/keys/upload
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.006232s
- **请求体**: 
```json
{}
```
- **响应体**: 
```json
{"one_time_key_counts":{}}
```
- **结果**: ✅ 通过

---
### 标记设备已验证

- **方法**: POST /_matrix/client/r0/keys/claim
- **状态码**: 400 (预期: 200)
- **响应时间**: 0.006525s
- **请求体**: 
```json
{}
```
- **响应体**: 
```json
{"errcode":"M_BAD_JSON","error":"Invalid request: missing field `one_time_keys`"}
```
- **结果**: ❌ 失败

---
## 8. 密钥备份 API 测试

### 获取密钥备份版本

- **方法**: GET /_matrix/client/r0/room_keys/version
- **状态码**: 405 (预期: 200)
- **响应时间**: 0.005278s
- **请求体**: 
```json

```
- **响应体**: 
```json

```
- **结果**: ❌ 失败

---
### 创建密钥备份

- **方法**: POST /_matrix/client/r0/room_keys/version
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.008814s
- **请求体**: 
```json
{"algorithm":"m.room_keys.v1.curve25519-aes-sha2"}
```
- **响应体**: 
```json
{"version":"1770199034"}
```
- **结果**: ✅ 通过

---
### 获取密钥备份

- **方法**: GET /_matrix/client/r0/room_keys
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.004358s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"errcode":"UNKNOWN","error":"Unknown endpoint"}
```
- **结果**: ✅ 通过

---
### 删除密钥备份

- **方法**: DELETE /_matrix/client/r0/room_keys/version/1
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.007684s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"deleted":true,"version":"1"}
```
- **结果**: ✅ 通过

---
## 9. 联邦通信 API 测试

### 联邦版本检查

- **方法**: GET /_matrix/federation/v1/version
- **状态码**: 200 (预期: 200)
- **响应时间**: 0.005470s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"server":{"name":"Synapse Rust","version":"0.1.0"},"version":"3600"}
```
- **结果**: ✅ 通过

---
### 获取服务器密钥

- **方法**: GET /_matrix/federation/v1/host/keys
- **状态码**: 200 (预期: 400)
- **响应时间**: 0.004385s
- **请求体**: 
```json

```
- **响应体**: 
```json
{"errcode":"UNKNOWN","error":"Unknown endpoint"}
```
- **结果**: ❌ 失败

---
### 发送事务

- **方法**: POST /_matrix/federation/v1/send/transaction
- **状态码**: 401 (预期: 400)
- **响应时间**: 0.005627s
- **请求体**: 
```json
{}
```
- **响应体**: 
```json
{"errcode":"M_UNAUTHORIZED","error":"Missing federation signature"}
```
- **结果**: ❌ 失败

---
