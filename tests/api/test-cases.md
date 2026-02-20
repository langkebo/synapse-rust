# Synapse Rust 测试用例文档

**文档版本**: 1.0  
**制定日期**: 2026-02-12  
**基于文档**: optimization-plan.md

---

## 1. 测试环境配置

### 1.1 环境变量

```bash
# 服务器配置
SERVER_HOST=localhost
SERVER_PORT=8008
SERVER_DOMAIN=cjystx.top

# 数据库配置
DATABASE_URL=postgresql://synapse:synapse_password@localhost:5432/synapse

# Redis配置
REDIS_URL=redis://localhost:6379

# 测试账号
TEST_USER_1=@apitest_user1:cjystx.top
TEST_USER_2=@apitest_user2:cjystx.top
TEST_USER_3=@apitest_user3:cjystx.top
TEST_USER_4=@apitest_user4:cjystx.top
TEST_USER_5=@apitest_user5:cjystx.top
```

### 1.2 测试数据准备

```bash
# 创建测试账号
curl -X POST http://localhost:8008/_matrix/client/r0/register \
  -H "Content-Type: application/json" \
  -d '{
    "username": "apitest_user1",
    "password": "test_password_123",
    "auth": {"type": "m.login.dummy"}
  }'

# 获取访问令牌
curl -X POST http://localhost:8008/_matrix/client/r0/login \
  -H "Content-Type: application/json" \
  -d '{
    "type": "m.login.password",
    "user": "apitest_user1",
    "password": "test_password_123"
  }'
```

---

## 2. 好友系统测试用例

### 2.1 添加好友测试用例

#### TC-FR-001: 有效Token添加好友

**测试目的**: 验证使用有效Token可以成功添加好友

**前置条件**:
- 用户已登录并获取有效的 access_token
- 目标用户存在且不是自己

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/friends \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "@apitest_user2:cjystx.top"
  }'
```

**预期结果**:
```json
{
  "status": "success"
}
```

**HTTP状态码**: 200 OK

**验证点**:
- ✅ 返回状态码 200
- ✅ 返回 status: "success"
- ✅ 好友列表中包含新添加的好友
- ✅ 直接聊天房间已创建

---

#### TC-FR-002: 无Token添加好友

**测试目的**: 验证未提供Token时返回认证失败

**前置条件**:
- 无需登录

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/friends \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "@apitest_user2:cjystx.top"
  }'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "Unauthorized",
  "errcode": "M_UNAUTHORIZED"
}
```

**HTTP状态码**: 401 UNAUTHORIZED

**验证点**:
- ✅ 返回状态码 401
- ✅ 返回 errcode: "M_UNAUTHORIZED"
- ✅ 好友列表未改变

---

#### TC-FR-003: 无效Token添加好友

**测试目的**: 验证使用无效Token时返回认证失败

**前置条件**:
- 无需登录

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/friends \
  -H "Authorization: Bearer invalid_token_12345" \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "@apitest_user2:cjystx.top"
  }'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "Unauthorized",
  "errcode": "M_UNAUTHORIZED"
}
```

**HTTP状态码**: 401 UNAUTHORIZED

**验证点**:
- ✅ 返回状态码 401
- ✅ 返回 errcode: "M_UNAUTHORIZED"
- ✅ 好友列表未改变

---

#### TC-FR-004: 添加自己为好友

**测试目的**: 验证不能添加自己为好友

**前置条件**:
- 用户已登录并获取有效的 access_token

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/friends \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "@apitest_user1:cjystx.top"
  }'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "Cannot add yourself as a friend",
  "errcode": "M_BAD_JSON"
}
```

**HTTP状态码**: 400 BAD_REQUEST

**验证点**:
- ✅ 返回状态码 400
- ✅ 返回 errcode: "M_BAD_JSON"
- ✅ 好友列表未改变

---

#### TC-FR-005: 添加不存在的用户

**测试目的**: 验证添加不存在的用户时返回错误

**前置条件**:
- 用户已登录并获取有效的 access_token

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/friends \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "@nonexistent_user:cjystx.top"
  }'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "User not found",
  "errcode": "M_NOT_FOUND"
}
```

**HTTP状态码**: 404 NOT_FOUND

**验证点**:
- ✅ 返回状态码 404
- ✅ 返回 errcode: "M_NOT_FOUND"
- ✅ 好友列表未改变

---

#### TC-FR-006: 空user_id

**测试目的**: 验证空user_id时返回参数错误

**前置条件**:
- 用户已登录并获取有效的 access_token

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/friends \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": ""
  }'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "Invalid user_id",
  "errcode": "M_BAD_JSON"
}
```

**HTTP状态码**: 400 BAD_REQUEST

**验证点**:
- ✅ 返回状态码 400
- ✅ 返回 errcode: "M_BAD_JSON"
- ✅ 好友列表未改变

---

#### TC-FR-007: 缺少user_id字段

**测试目的**: 验证缺少user_id字段时返回参数错误

**前置条件**:
- 用户已登录并获取有效的 access_token

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/friends \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{}'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "Missing user_id field",
  "errcode": "M_BAD_JSON"
}
```

**HTTP状态码**: 400 BAD_REQUEST

**验证点**:
- ✅ 返回状态码 400
- ✅ 返回 errcode: "M_BAD_JSON"
- ✅ 好友列表未改变

---

#### TC-FR-008: 无效user_id格式

**测试目的**: 验证无效user_id格式时返回参数错误

**前置条件**:
- 用户已登录并获取有效的 access_token

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/friends \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "invalid_format_without_at_sign"
  }'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "Invalid user_id format",
  "errcode": "M_BAD_JSON"
}
```

**HTTP状态码**: 400 BAD_REQUEST

**验证点**:
- ✅ 返回状态码 400
- ✅ 返回 errcode: "M_BAD_JSON"
- ✅ 好友列表未改变

---

#### TC-FR-009: 超长user_id

**测试目的**: 验证超长user_id时返回参数错误

**前置条件**:
- 用户已登录并获取有效的 access_token

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/friends \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "@very_long_username_that_exceeds_maximum_allowed_length_of_255_characters_very_long_username_that_exceeds_maximum_allowed_length_of_255_characters_very_long_username_that_exceeds_maximum_allowed_length_of_255_characters_very_long_username_that_exceeds_maximum_allowed_length_of_255_characters_very_long_username_that_exceeds_maximum_allowed_length_of_255_characters:cjystx.top"
  }'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "user_id exceeds maximum length",
  "errcode": "M_BAD_JSON"
}
```

**HTTP状态码**: 400 BAD_REQUEST

**验证点**:
- ✅ 返回状态码 400
- ✅ 返回 errcode: "M_BAD_JSON"
- ✅ 好友列表未改变

---

### 2.2 删除好友测试用例

#### TC-FR-010: 有效Token删除好友

**测试目的**: 验证使用有效Token可以成功删除好友

**前置条件**:
- 用户已登录并获取有效的 access_token
- 目标用户已在好友列表中

**测试步骤**:
```bash
curl -X DELETE http://localhost:8008/_matrix/client/r0/friends/@apitest_user2:cjystx.top \
  -H "Authorization: Bearer $ACCESS_TOKEN"
```

**预期结果**:
```json
{
  "status": "success"
}
```

**HTTP状态码**: 200 OK

**验证点**:
- ✅ 返回状态码 200
- ✅ 返回 status: "success"
- ✅ 好友列表中不再包含该好友

---

#### TC-FR-011: 无Token删除好友

**测试目的**: 验证未提供Token时返回认证失败

**前置条件**:
- 无需登录

**测试步骤**:
```bash
curl -X DELETE http://localhost:8008/_matrix/client/r0/friends/@apitest_user2:cjystx.top
```

**预期结果**:
```json
{
  "status": "error",
  "error": "Unauthorized",
  "errcode": "M_UNAUTHORIZED"
}
```

**HTTP状态码**: 401 UNAUTHORIZED

**验证点**:
- ✅ 返回状态码 401
- ✅ 返回 errcode: "M_UNAUTHORIZED"
- ✅ 好友列表未改变

---

#### TC-FR-012: 删除不存在的用户

**测试目的**: 验证删除不存在的用户时返回错误

**前置条件**:
- 用户已登录并获取有效的 access_token
- 目标用户不在好友列表中

**测试步骤**:
```bash
curl -X DELETE http://localhost:8008/_matrix/client/r0/friends/@nonexistent_user:cjystx.top \
  -H "Authorization: Bearer $ACCESS_TOKEN"
```

**预期结果**:
```json
{
  "status": "error",
  "error": "Friend not found",
  "errcode": "M_NOT_FOUND"
}
```

**HTTP状态码**: 404 NOT_FOUND

**验证点**:
- ✅ 返回状态码 404
- ✅ 返回 errcode: "M_NOT_FOUND"
- ✅ 好友列表未改变

---

#### TC-FR-013: 删除非好友用户

**测试目的**: 验证删除非好友用户时返回错误

**前置条件**:
- 用户已登录并获取有效的 access_token
- 目标用户存在但不在好友列表中

**测试步骤**:
```bash
curl -X DELETE http://localhost:8008/_matrix/client/r0/friends/@apitest_user3:cjystx.top \
  -H "Authorization: Bearer $ACCESS_TOKEN"
```

**预期结果**:
```json
{
  "status": "error",
  "error": "Friend not found",
  "errcode": "M_NOT_FOUND"
}
```

**HTTP状态码**: 404 NOT_FOUND

**验证点**:
- ✅ 返回状态码 404
- ✅ 返回 errcode: "M_NOT_FOUND"
- ✅ 好友列表未改变

---

### 2.3 更新好友备注测试用例

#### TC-FR-014: 有效Token更新备注

**测试目的**: 验证使用有效Token可以成功更新好友备注

**前置条件**:
- 用户已登录并获取有效的 access_token
- 目标用户已在好友列表中

**测试步骤**:
```bash
curl -X PUT http://localhost:8008/_matrix/client/r0/friends/@apitest_user2:cjystx.top/note \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "note": "My best friend"
  }'
```

**预期结果**:
```json
{
  "status": "success"
}
```

**HTTP状态码**: 200 OK

**验证点**:
- ✅ 返回状态码 200
- ✅ 返回 status: "success"
- ✅ 好友备注已更新

---

#### TC-FR-015: 无Token更新备注

**测试目的**: 验证未提供Token时返回认证失败

**前置条件**:
- 无需登录

**测试步骤**:
```bash
curl -X PUT http://localhost:8008/_matrix/client/r0/friends/@apitest_user2:cjystx.top/note \
  -H "Content-Type: application/json" \
  -d '{
    "note": "test"
  }'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "Unauthorized",
  "errcode": "M_UNAUTHORIZED"
}
```

**HTTP状态码**: 401 UNAUTHORIZED

**验证点**:
- ✅ 返回状态码 401
- ✅ 返回 errcode: "M_UNAUTHORIZED"
- ✅ 好友备注未改变

---

#### TC-FR-016: 更新不存在的用户备注

**测试目的**: 验证更新不存在的用户备注时返回错误

**前置条件**:
- 用户已登录并获取有效的 access_token
- 目标用户不在好友列表中

**测试步骤**:
```bash
curl -X PUT http://localhost:8008/_matrix/client/r0/friends/@nonexistent_user:cjystx.top/note \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "note": "test"
  }'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "Friend not found",
  "errcode": "M_NOT_FOUND"
}
```

**HTTP状态码**: 404 NOT_FOUND

**验证点**:
- ✅ 返回状态码 404
- ✅ 返回 errcode: "M_NOT_FOUND"
- ✅ 好友备注未改变

---

#### TC-FR-017: 空备注

**测试目的**: 验证可以设置空备注

**前置条件**:
- 用户已登录并获取有效的 access_token
- 目标用户已在好友列表中

**测试步骤**:
```bash
curl -X PUT http://localhost:8008/_matrix/client/r0/friends/@apitest_user2:cjystx.top/note \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "note": ""
  }'
```

**预期结果**:
```json
{
  "status": "success"
}
```

**HTTP状态码**: 200 OK

**验证点**:
- ✅ 返回状态码 200
- ✅ 返回 status: "success"
- ✅ 好友备注已清空

---

#### TC-FR-018: 超长备注

**测试目的**: 验证超长备注时返回参数错误

**前置条件**:
- 用户已登录并获取有效的 access_token
- 目标用户已在好友列表中

**测试步骤**:
```bash
curl -X PUT http://localhost:8008/_matrix/client/r0/friends/@apitest_user2:cjystx.top/note \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "note": "This is a very long note that exceeds the maximum allowed length of 255 characters. This is a very long note that exceeds the maximum allowed length of 255 characters. This is a very long note that exceeds the maximum allowed length of 255 characters. This is a very long note that exceeds the maximum allowed length of 255 characters."
  }'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "note exceeds maximum length",
  "errcode": "M_BAD_JSON"
}
```

**HTTP状态码**: 400 BAD_REQUEST

**验证点**:
- ✅ 返回状态码 400
- ✅ 返回 errcode: "M_BAD_JSON"
- ✅ 好友备注未改变

---

### 2.4 更新好友状态测试用例

#### TC-FR-019: 有效Token更新状态为blocked

**测试目的**: 验证使用有效Token可以成功更新好友状态

**前置条件**:
- 用户已登录并获取有效的 access_token
- 目标用户已在好友列表中

**测试步骤**:
```bash
curl -X PUT http://localhost:8008/_matrix/client/r0/friends/@apitest_user2:cjystx.top/status \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "status": "blocked"
  }'
```

**预期结果**:
```json
{
  "status": "success"
}
```

**HTTP状态码**: 200 OK

**验证点**:
- ✅ 返回状态码 200
- ✅ 返回 status: "success"
- ✅ 好友状态已更新

---

#### TC-FR-020: 更新状态为favorite

**测试目的**: 验证可以更新好友状态为favorite

**前置条件**:
- 用户已登录并获取有效的 access_token
- 目标用户已在好友列表中

**测试步骤**:
```bash
curl -X PUT http://localhost:8008/_matrix/client/r0/friends/@apitest_user2:cjystx.top/status \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "status": "favorite"
  }'
```

**预期结果**:
```json
{
  "status": "success"
}
```

**HTTP状态码**: 200 OK

**验证点**:
- ✅ 返回状态码 200
- ✅ 返回 status: "success"
- ✅ 好友状态已更新

---

#### TC-FR-021: 更新不存在的用户状态

**测试目的**: 验证更新不存在的用户状态时返回错误

**前置条件**:
- 用户已登录并获取有效的 access_token
- 目标用户不在好友列表中

**测试步骤**:
```bash
curl -X PUT http://localhost:8008/_matrix/client/r0/friends/@nonexistent_user:cjystx.top/status \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "status": "blocked"
  }'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "Friend not found",
  "errcode": "M_NOT_FOUND"
}
```

**HTTP状态码**: 404 NOT_FOUND

**验证点**:
- ✅ 返回状态码 404
- ✅ 返回 errcode: "M_NOT_FOUND"
- ✅ 好友状态未改变

---

#### TC-FR-022: 无效状态值

**测试目的**: 验证无效状态值时返回参数错误

**前置条件**:
- 用户已登录并获取有效的 access_token
- 目标用户已在好友列表中

**测试步骤**:
```bash
curl -X PUT http://localhost:8008/_matrix/client/r0/friends/@apitest_user2:cjystx.top/status \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "status": "invalid_status"
  }'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "Invalid status value",
  "errcode": "M_BAD_JSON"
}
```

**HTTP状态码**: 400 BAD_REQUEST

**验证点**:
- ✅ 返回状态码 400
- ✅ 返回 errcode: "M_BAD_JSON"
- ✅ 好友状态未改变

---

### 2.5 发送好友请求测试用例

#### TC-FR-023: 有效Token发送请求

**测试目的**: 验证使用有效Token可以成功发送好友请求

**前置条件**:
- 用户已登录并获取有效的 access_token
- 目标用户存在且不是自己

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/friends/requests \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "@apitest_user3:cjystx.top",
    "message": "Hi, lets be friends"
  }'
```

**预期结果**:
```json
{
  "status": "success",
  "request_id": "req_123456789"
}
```

**HTTP状态码**: 200 OK

**验证点**:
- ✅ 返回状态码 200
- ✅ 返回 status: "success"
- ✅ 返回 request_id
- ✅ 好友请求已发送

---

#### TC-FR-024: 无Token发送请求

**测试目的**: 验证未提供Token时返回认证失败

**前置条件**:
- 无需登录

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/friends/requests \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "@apitest_user3:cjystx.top",
    "message": "Hi"
  }'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "Unauthorized",
  "errcode": "M_UNAUTHORIZED"
}
```

**HTTP状态码**: 401 UNAUTHORIZED

**验证点**:
- ✅ 返回状态码 401
- ✅ 返回 errcode: "M_UNAUTHORIZED"
- ✅ 好友请求未发送

---

#### TC-FR-025: 发送给自己

**测试目的**: 验证不能发送好友请求给自己

**前置条件**:
- 用户已登录并获取有效的 access_token

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/friends/requests \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "@apitest_user1:cjystx.top",
    "message": "Hi"
  }'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "Cannot send friend request to yourself",
  "errcode": "M_BAD_JSON"
}
```

**HTTP状态码**: 400 BAD_REQUEST

**验证点**:
- ✅ 返回状态码 400
- ✅ 返回 errcode: "M_BAD_JSON"
- ✅ 好友请求未发送

---

#### TC-FR-026: 发送给不存在的用户

**测试目的**: 验证发送给不存在的用户时返回错误

**前置条件**:
- 用户已登录并获取有效的 access_token

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/friends/requests \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "@nonexistent_user:cjystx.top",
    "message": "Hi"
  }'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "User not found",
  "errcode": "M_NOT_FOUND"
}
```

**HTTP状态码**: 404 NOT_FOUND

**验证点**:
- ✅ 返回状态码 404
- ✅ 返回 errcode: "M_NOT_FOUND"
- ✅ 好友请求未发送

---

#### TC-FR-027: 空message

**测试目的**: 验证可以发送空消息

**前置条件**:
- 用户已登录并获取有效的 access_token
- 目标用户存在且不是自己

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/friends/requests \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "@apitest_user3:cjystx.top",
    "message": ""
  }'
```

**预期结果**:
```json
{
  "status": "success",
  "request_id": "req_123456789"
}
```

**HTTP状态码**: 200 OK

**验证点**:
- ✅ 返回状态码 200
- ✅ 返回 status: "success"
- ✅ 返回 request_id
- ✅ 好友请求已发送

---

#### TC-FR-028: 超长message

**测试目的**: 验证超长消息时返回参数错误

**前置条件**:
- 用户已登录并获取有效的 access_token
- 目标用户存在且不是自己

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/friends/requests \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "@apitest_user3:cjystx.top",
    "message": "This is a very long message that exceeds the maximum allowed length of 1000 characters..."
  }'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "message exceeds maximum length",
  "errcode": "M_BAD_JSON"
}
```

**HTTP状态码**: 400 BAD_REQUEST

**验证点**:
- ✅ 返回状态码 400
- ✅ 返回 errcode: "M_BAD_JSON"
- ✅ 好友请求未发送

---

### 2.6 接受/拒绝好友请求测试用例

#### TC-FR-029: 有效Token接受请求

**测试目的**: 验证使用有效Token可以成功接受好友请求

**前置条件**:
- 用户已登录并获取有效的 access_token
- 存在待处理的好友请求

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/friends/requests/@apitest_user3:cjystx.top/accept \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{}'
```

**预期结果**:
```json
{
  "status": "success"
}
```

**HTTP状态码**: 200 OK

**验证点**:
- ✅ 返回状态码 200
- ✅ 返回 status: "success"
- ✅ 好友请求已接受
- ✅ 好友列表中包含新好友

---

#### TC-FR-030: 有效Token拒绝请求

**测试目的**: 验证使用有效Token可以成功拒绝好友请求

**前置条件**:
- 用户已登录并获取有效的 access_token
- 存在待处理的好友请求

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/friends/requests/@apitest_user3:cjystx.top/reject \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{}'
```

**预期结果**:
```json
{
  "status": "success"
}
```

**HTTP状态码**: 200 OK

**验证点**:
- ✅ 返回状态码 200
- ✅ 返回 status: "success"
- ✅ 好友请求已拒绝
- ✅ 好友列表中不包含该用户

---

#### TC-FR-031: 接受不存在的请求

**测试目的**: 验证接受不存在的请求时返回错误

**前置条件**:
- 用户已登录并获取有效的 access_token

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/friends/requests/@nonexistent_user:cjystx.top/accept \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{}'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "Friend request not found",
  "errcode": "M_NOT_FOUND"
}
```

**HTTP状态码**: 404 NOT_FOUND

**验证点**:
- ✅ 返回状态码 404
- ✅ 返回 errcode: "M_NOT_FOUND"
- ✅ 好友列表未改变

---

#### TC-FR-032: 拒绝不存在的请求

**测试目的**: 验证拒绝不存在的请求时返回错误

**前置条件**:
- 用户已登录并获取有效的 access_token

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/friends/requests/@nonexistent_user:cjystx.top/reject \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{}'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "Friend request not found",
  "errcode": "M_NOT_FOUND"
}
```

**HTTP状态码**: 404 NOT_FOUND

**验证点**:
- ✅ 返回状态码 404
- ✅ 返回 errcode: "M_NOT_FOUND"
- ✅ 好友列表未改变

---

## 3. 邮箱验证测试用例

### 3.1 请求邮箱验证令牌测试用例

#### TC-EM-001: 有效邮箱请求Token

**测试目的**: 验证使用有效邮箱可以成功请求验证令牌

**前置条件**:
- 用户已登录并获取有效的 access_token

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/account/3pid/email/requestToken \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "email": "test@example.com",
    "client_secret": "test_secret_12345"
  }'
```

**预期结果**:
```json
{
  "status": "success",
  "sid": "session_id_12345"
}
```

**HTTP状态码**: 200 OK

**验证点**:
- ✅ 返回状态码 200
- ✅ 返回 status: "success"
- ✅ 返回 sid
- ✅ 验证邮件已发送

---

#### TC-EM-002: 无效邮箱格式

**测试目的**: 验证无效邮箱格式时返回参数错误

**前置条件**:
- 用户已登录并获取有效的 access_token

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/account/3pid/email/requestToken \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "email": "invalid_email",
    "client_secret": "test_secret_12345"
  }'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "Invalid email format",
  "errcode": "M_BAD_JSON"
}
```

**HTTP状态码**: 400 BAD_REQUEST

**验证点**:
- ✅ 返回状态码 400
- ✅ 返回 errcode: "M_BAD_JSON"
- ✅ 验证邮件未发送

---

#### TC-EM-003: 空邮箱

**测试目的**: 验证空邮箱时返回参数错误

**前置条件**:
- 用户已登录并获取有效的 access_token

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/account/3pid/email/requestToken \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "email": "",
    "client_secret": "test_secret_12345"
  }'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "Email is required",
  "errcode": "M_BAD_JSON"
}
```

**HTTP状态码**: 400 BAD_REQUEST

**验证点**:
- ✅ 返回状态码 400
- ✅ 返回 errcode: "M_BAD_JSON"
- ✅ 验证邮件未发送

---

#### TC-EM-004: 缺少client_secret

**测试目的**: 验证缺少client_secret时返回参数错误

**前置条件**:
- 用户已登录并获取有效的 access_token

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/account/3pid/email/requestToken \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "email": "test@example.com"
  }'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "client_secret is required",
  "errcode": "M_BAD_JSON"
}
```

**HTTP状态码**: 400 BAD_REQUEST

**验证点**:
- ✅ 返回状态码 400
- ✅ 返回 errcode: "M_BAD_JSON"
- ✅ 验证邮件未发送

---

### 3.2 提交邮箱验证令牌测试用例

#### TC-EM-005: 提交有效Token

**测试目的**: 验证使用有效Token可以成功验证邮箱

**前置条件**:
- 用户已登录并获取有效的 access_token
- 已获取有效的验证令牌

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/account/3pid/email/submitToken \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "token": "valid_token_12345",
    "client_secret": "test_secret_12345"
  }'
```

**预期结果**:
```json
{
  "status": "success"
}
```

**HTTP状态码**: 200 OK

**验证点**:
- ✅ 返回状态码 200
- ✅ 返回 status: "success"
- ✅ 邮箱已验证

---

#### TC-EM-006: 提交过期Token

**测试目的**: 验证提交过期Token时返回错误

**前置条件**:
- 用户已登录并获取有效的 access_token

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/account/3pid/email/submitToken \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "token": "expired_token_12345",
    "client_secret": "test_secret_12345"
  }'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "Token has expired",
  "errcode": "M_INVALID_TOKEN"
}
```

**HTTP状态码**: 400 BAD_REQUEST

**验证点**:
- ✅ 返回状态码 400
- ✅ 返回 errcode: "M_INVALID_TOKEN"
- ✅ 邮箱未验证

---

#### TC-EM-007: 提交无效Token

**测试目的**: 验证提交无效Token时返回错误

**前置条件**:
- 用户已登录并获取有效的 access_token

**测试步骤**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/account/3pid/email/submitToken \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "token": "invalid_token_12345",
    "client_secret": "test_secret_12345"
  }'
```

**预期结果**:
```json
{
  "status": "error",
  "error": "Token not found",
  "errcode": "M_INVALID_TOKEN"
}
```

**HTTP状态码**: 404 NOT_FOUND

**验证点**:
- ✅ 返回状态码 404
- ✅ 返回 errcode: "M_INVALID_TOKEN"
- ✅ 邮箱未验证

---

## 4. 测试用例统计

| 测试类别 | 测试用例数 | 涵盖场景 |
|---------|-----------|---------|
| 添加好友 | 9 | 正常、异常、边界值 |
| 删除好友 | 4 | 正常、异常 |
| 更新好友备注 | 5 | 正常、异常、边界值 |
| 更新好友状态 | 4 | 正常、异常、边界值 |
| 发送好友请求 | 6 | 正常、异常、边界值 |
| 接受/拒绝好友请求 | 4 | 正常、异常 |
| 邮箱验证 | 7 | 正常、异常、边界值 |
| **总计** | **39** | - |

---

**文档结束**
