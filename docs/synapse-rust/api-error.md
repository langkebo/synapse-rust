
## 五、媒体管理 API 测试问题

**测试时间**: 2026-02-18  
**测试章节**: 4.13 媒体管理 API (12 个端点)

### 5.1 测试执行情况

| 问题ID | 测试描述 | 预期结果 | 实际结果 | 状态 |
|--------|----------|----------|----------|------|
| #M1 | 下载不存在的媒体 | 404 Not Found | 404 Not Found | ✅ 已修复 |
| #M2 | 删除自己上传的媒体 | 200 OK | 403 Forbidden | ❌ 需修复 |
| #M3 | 上传媒体指定ID | 200 OK | 400 Bad Request | ❌ 需修复 |
| #M4 | 获取缩略图错误状态码 | 400 Bad Request | 200 OK (JSON错误) | ❌ 需修复 |

### 5.2 问题详情

#### 问题 #M1: 下载不存在的媒体 ✅ 已修复

**测试用例**:
```bash
curl "http://localhost:8008/_matrix/media/v3/download/cjystx.top/nonexistent_media_id"
```

**修复前响应**:
```json
{
  "errcode": "M_NOT_FOUND",
  "error": "Media not found"
}
```
**HTTP状态码**: 200

**修复后响应**:
```json
{
  "status": "error",
  "error": "Media not found",
  "errcode": "M_NOT_FOUND"
}
```
**HTTP状态码**: 404

**修复方案**:
- 修改 `download_media` 函数返回 `Result<impl IntoResponse, ApiError>`
- 错误时返回正确的HTTP状态码

**相关代码位置**: [media.rs](file:///home/hula/synapse_rust/synapse/src/web/routes/media.rs)

---

#### 问题 #M2: 删除自己上传的媒体返回403 ❌ 仍需修复

**测试用例**:
```bash
# 用户上传媒体
curl -X POST "http://localhost:8008/_matrix/media/v3/upload" \
  -H "Authorization: Bearer {token}" \
  -d '{"content": "dGVzdA==", "content_type": "text/plain"}'

# 用户删除自己上传的媒体
curl -X POST "http://localhost:8008/_matrix/media/v3/delete/cjystx.top/{media_id}" \
  -H "Authorization: Bearer {token}"
```

**实际响应**:
```json
{
  "status": "error",
  "error": "You can only delete your own media",
  "errcode": "M_FORBIDDEN"
}
```
**HTTP状态码**: 403

**问题分析**:
- 权限检查逻辑错误，无法正确识别媒体所有者
- `media_repository` 表中缺少 `uploader` 列或数据未正确存储

**相关代码位置**: [media.rs](file:///home/hula/synapse_rust/synapse/src/web/routes/media.rs)

---

#### 问题 #M3: 上传媒体指定ID返回400 ❌ 需修复

**测试用例**:
```bash
curl -X POST "http://localhost:8008/_matrix/media/v3/upload/cjystx.top/custom_media_id" \
  -H "Authorization: Bearer {token}" \
  -H "Content-Type: application/json" \
  -d '{"content": "dGVzdCBjb250ZW50", "content_type": "text/plain"}'
```

**实际响应**:
```json
{
  "status": "error",
  "error": "No file provided",
  "errcode": "M_BAD_JSON"
}
```
**HTTP状态码**: 400

**问题分析**:
- `upload_media` 路由处理函数未正确解析请求体
- 与 `upload_media_v3` 使用相同的请求体格式，但处理逻辑不同

**相关代码位置**: [media.rs](file:///home/hula/synapse_rust/synapse/src/web/routes/media.rs)

---

#### 问题 #M4: 获取缩略图错误状态码 ❌ 需修复

**测试用例**:
```bash
curl "http://localhost:8008/_matrix/media/v3/thumbnail/cjystx.top/{media_id}?width=100&height=100"
```

**实际响应**:
```json
{
  "errcode": "M_BAD_JSON",
  "error": "Invalid image data: The image format could not be determined"
}
```
**HTTP状态码**: 200

**预期响应**:
- HTTP状态码: 400 (对于无效图片格式)
- 或 404 (对于不支持的媒体类型)

**问题分析**:
- 错误响应使用200状态码
- 应该使用适当的4xx状态码

**相关代码位置**: [media.rs](file:///home/hula/synapse_rust/synapse/src/web/routes/media.rs)

### 5.3 测试通过的用例

以下测试用例已通过：

| 测试用例 | 描述 | HTTP状态码 | 结果 |
|---------|------|-----------|------|
| 1.1 | 正常上传媒体文件 | 200 | ✅ 通过 |
| 1.2 | 无认证上传 | 401 | ✅ 通过 |
| 1.4 | 缺少content字段 | 400 | ✅ 通过 |
| 2.1 | 正常上传 (v1) | 200 | ✅ 通过 |
| 4.1 | 正常下载媒体文件 | 200 | ✅ 通过 |
| 4.3 | 无认证下载 (允许) | 200 | ✅ 通过 |
| 5.1 | 正常下载 (v1) | 200 | ✅ 通过 |
| 6.1 | 正常下载 (r1) | 200 | ✅ 通过 |
| 8.1 | URL预览 | 200 | ✅ 通过 |
| 9.1 | URL预览 (v1) | 200 | ✅ 通过 |
| 10.1 | 获取媒体配置 | 200 | ✅ 通过 |
| 11.1 | 获取媒体配置 (v1) | 200 | ✅ 通过 |
| 12.2 | 删除不存在的媒体 | 404 | ✅ 通过 |
| 12.3 | 无认证删除 | 401 | ✅ 通过 |

### 5.4 参考资料

- Matrix Media API: https://spec.matrix.org/v1.11/client-server-api/#media
- Content Repository: https://spec.matrix.org/v1.11/client-server-api/#content-repository

---

## 六、语音消息 API 测试问题

**测试时间**: 2026-02-18  
**测试章节**: 4.14 语音消息 API (10 个端点)

### 6.1 测试执行情况

| 问题ID | 测试描述 | 预期结果 | 实际结果 | 状态 |
|--------|----------|----------|----------|------|
| #V1 | 上传语音消息 | 200 OK | 400 文件类型错误 | ❌ 需修复 |
| #V2 | 获取语音消息 | 404 Not Found | 404 Not Found | ✅ 已修复 |
| #V3 | 获取用户语音消息 | 200 OK | 200 OK | ✅ 已修复 |
| #V4 | 获取房间语音消息 | 200 OK | 200 OK | ✅ 已修复 |
| #V5 | 删除不存在的语音消息 | 404 Not Found | 200 OK (deleted: false) | ❌ 需修复 |

### 6.2 问题详情

#### 问题 #V1: 上传语音消息返回400文件类型错误 ❌ 需修复

**测试用例**:
```bash
curl -X POST "http://localhost:8008/_matrix/client/r0/voice/upload" \
  -H "Authorization: Bearer {token}" \
  -H "Content-Type: application/json" \
  -d '{
    "content": "dGVzdA==",
    "content_type": "audio/ogg",
    "duration_ms": 5000
  }'
```

**实际响应**:
```json
{
  "status": "error",
  "error": "Could not determine file type. Please upload a valid audio file.",
  "errcode": "M_BAD_JSON"
}
```
**HTTP状态码**: 400

**问题分析**:
- 文件类型检测逻辑过于严格
- 需要支持更多音频格式或放宽检测

**相关代码位置**: [voice.rs](file:///home/hula/synapse_rust/synapse/src/web/routes/voice.rs)

---

#### 问题 #V5: 删除不存在的语音消息返回200 ❌ 需修复

**测试用例**:
```bash
curl -X DELETE "http://localhost:8008/_matrix/client/r0/voice/nonexistent_message_id" \
  -H "Authorization: Bearer {token}"
```

**实际响应**:
```json
{
  "deleted": false,
  "message_id": "nonexistent_message_id"
}
```
**HTTP状态码**: 200

**预期响应**:
```json
{
  "status": "error",
  "error": "Voice message not found",
  "errcode": "M_NOT_FOUND"
}
```
**预期HTTP状态码**: 404

**问题分析**:
- 未验证语音消息是否存在
- 对不存在的消息返回成功状态

**相关代码位置**: [voice.rs](file:///home/hula/synapse_rust/synapse/src/web/routes/voice.rs)

### 6.3 测试通过的用例

以下测试用例已通过：

| 测试用例 | 描述 | HTTP状态码 | 结果 |
|---------|------|-----------|------|
| 1.2 | 无认证上传 | 401 | ✅ 通过 |
| 1.3 | 缺少duration_ms | 400 | ✅ 通过 |
| 3.2 | 无认证删除 | 401 | ✅ 通过 |
| 6.1 | 正常获取语音统计 | 200 | ✅ 通过 |
| 6.2 | 无认证获取统计 | 401 | ✅ 通过 |
| 7.1 | 正常获取用户语音统计 | 200 | ✅ 通过 |
| 8.1 | 正常获取语音配置 | 200 | ✅ 通过 |
| 8.2 | 无认证获取配置 | 200 | ✅ 通过 |
| 9.1 | 正常转换语音格式 | 200 | ✅ 通过 |
| 9.2 | 缺少必要字段 | 400 | ✅ 通过 |
| 9.3 | 无效格式 | 400 | ✅ 通过 |
| 10.1 | 正常优化语音消息 | 200 | ✅ 通过 |
| 10.2 | 缺少message_id | 400 | ✅ 通过 |
| 10.3 | 无认证优化 | 401 | ✅ 通过 |

### 6.4 参考资料

- Matrix Voice Messages: https://spec.matrix.org/v1.11/client-server-api/#voice-messages
- MSC3245: Voice Messages: https://github.com/matrix-org/matrix-spec-proposals/pull/3245

---

## 七、推送通知 API 测试问题

**测试时间**: 2026-02-18  
**测试章节**: 4.16 推送通知 API (12 个端点)

### 7.1 测试执行情况

| 问题ID | 测试描述 | 预期结果 | 实际结果 | 状态 |
|--------|----------|----------|----------|------|
| #P1 | 获取推送器列表 | 200 OK | 200 OK | ✅ 已修复 |
| #P2 | 设置推送器 | 200 OK | 422 Unprocessable Entity | ❌ 需修复 |
| #P3 | 获取类型推送规则 | 200 OK | 500 缺少pattern列 | ❌ 需修复 |
| #P4 | 获取特定推送规则 | 200 OK | 200 OK | ✅ 已修复 |
| #P5 | 设置推送规则 | 200 OK | 200 OK | ✅ 已修复 |
| #P6 | 删除推送规则 | 200 OK | 200 OK | ✅ 已修复 |
| #P7 | 设置推送规则动作 | 200 OK | 200 OK | ✅ 已修复 |
| #P8 | 设置推送规则启用状态 | 200 OK | 200 OK | ✅ 已修复 |
| #P9 | 获取不存在的作用域 | 404 Not Found | 200 OK (空对象) | ❌ 需修复 |

### 7.2 问题详情

#### 问题 #P1: 获取推送器列表 ✅ 已修复

**测试用例**:
```bash
curl "http://localhost:8008/_matrix/client/v3/pushers" \
  -H "Authorization: Bearer {token}"
```

**修复前响应**:
```json
{
  "status": "error",
  "error": "Internal error: Database error: error returned from database: relation \"pushers\" does not exist",
  "errcode": "M_INTERNAL_ERROR"
}
```

**修复后响应**:
```json
{
  "pushers": []
}
```
**HTTP状态码**: 200

**修复方案**:
- 创建 `pushers` 表，包含 `device_id`, `created_at` 等必要列
- 迁移脚本: `20260219000000_unified_fix.sql`

**相关代码位置**: [push.rs](file:///home/hula/synapse_rust/synapse/src/web/routes/push.rs)

---

#### 问题 #P2: 设置推送器返回422 ❌ 需修复

**测试用例**:
```bash
curl -X POST "http://localhost:8008/_matrix/client/v3/pushers/set" \
  -H "Authorization: Bearer {token}" \
  -H "Content-Type: application/json" \
  -d '{"pushkey": "test123", "kind": "http", "app_id": "com.test.app"}'
```

**实际响应**: HTTP 422 Unprocessable Entity

**问题分析**:
- 请求体验证过于严格
- 需要检查必填字段要求

**相关代码位置**: [push.rs](file:///home/hula/synapse_rust/synapse/src/web/routes/push.rs)

---

#### 问题 #P3: 获取类型推送规则返回500 ❌ 需修复

**测试用例**:
```bash
curl "http://localhost:8008/_matrix/client/v3/pushrules/global/override" \
  -H "Authorization: Bearer {token}"
```

**实际响应**:
```json
{
  "status": "error",
  "error": "Internal error: Database error: error returned from database: column \"pattern\" does not exist",
  "errcode": "M_INTERNAL_ERROR"
}
```
**HTTP状态码**: 500

**问题分析**:
- `push_rules` 表缺少 `pattern` 列
- 需要添加缺失的列

**相关代码位置**: [push.rs](file:///home/hula/synapse_rust/synapse/src/web/routes/push.rs)

---

#### 问题 #P9: 获取不存在的作用域返回200 ❌ 需修复

**测试用例**:
```bash
curl "http://localhost:8008/_matrix/client/v3/pushrules/nonexistent" \
  -H "Authorization: Bearer {token}"
```

**实际响应**:
```json
{}
```
**HTTP状态码**: 200

**预期响应**:
```json
{
  "status": "error",
  "error": "Invalid scope",
  "errcode": "M_NOT_FOUND"
}
```
**预期HTTP状态码**: 404

**问题分析**:
- 未验证作用域是否有效
- 对无效作用域返回空对象而非错误

**相关代码位置**: [push.rs](file:///home/hula/synapse_rust/synapse/src/web/routes/push.rs)

### 7.3 测试通过的用例

以下测试用例已通过：

| 测试用例 | 描述 | HTTP状态码 | 结果 |
|---------|------|-----------|------|
| 1.2 | 无认证获取推送器列表 | 401 | ✅ 通过 |
| 2.2 | 无认证设置推送器 | 401 | ✅ 通过 |
| 5.1 | 正常获取推送规则 | 200 | ✅ 通过 |
| 5.2 | 无认证获取推送规则 | 401 | ✅ 通过 |
| 6.1 | 获取global作用域规则 | 200 | ✅ 通过 |
| 9.2 | 无认证设置推送规则 | 401 | ✅ 通过 |
| 10.2 | 无认证删除推送规则 | 401 | ✅ 通过 |
| 11.2 | 无认证设置规则动作 | 401 | ✅ 通过 |
| 12.2 | 无认证设置规则启用状态 | 401 | ✅ 通过 |

### 7.4 参考资料

- Matrix Push Notifications: https://spec.matrix.org/v1.11/client-server-api/#push-notifications
- Push Gateway API: https://spec.matrix.org/v1.11/push-gateway-api/

---

## 八、搜索 API 测试问题

**测试时间**: 2026-02-18  
**测试章节**: 4.17 搜索 API (5 个端点)

### 8.1 测试执行情况

| 问题ID | 测试描述 | 预期结果 | 实际结果 | 状态 |
|--------|----------|----------|----------|------|
| #S1 | 搜索功能 | 200 OK | 200 OK | ✅ 已修复 |
| #S2 | 获取房间线程 | 200 OK | 500 内部错误 | ❌ 需修复 |
| #S3 | 获取房间层级 | 200 OK | 500 缺少guest_access列 | ❌ 需修复 |
| #S4 | 时间戳转事件 | 200 OK | 403 权限错误 | ✅ 已修复 |

### 8.2 问题详情

#### 问题 #S1: 搜索功能 ✅ 已修复

**测试用例**:
```bash
curl -X POST "http://localhost:8008/_matrix/client/v3/search" \
  -H "Authorization: Bearer {token}" \
  -H "Content-Type: application/json" \
  -d '{"search_categories": {"room_events": {"search_term": "test"}}}'
```

**修复前响应**:
```json
{
  "status": "error",
  "error": "Internal error: Database error: error returned from database: column \"type\" does not exist",
  "errcode": "M_INTERNAL_ERROR"
}
```

**修复后响应**:
```json
{
  "search_categories": {
    "room_events": {
      "count": 0,
      "results": []
    }
  }
}
```
**HTTP状态码**: 200

**修复方案**:
- 添加 `events.type` 列
- 迁移脚本: `20260219000000_unified_fix.sql`

**相关代码位置**: [search.rs](file:///home/hula/synapse_rust/synapse/src/web/routes/search.rs)

---

#### 问题 #S2: 获取房间线程返回500 ❌ 需修复

**测试用例**:
```bash
curl "http://localhost:8008/_matrix/client/v3/user/{user_id}/rooms/{room_id}/threads" \
  -H "Authorization: Bearer {token}"
```

**实际响应**: HTTP 500 内部服务器错误

**问题分析**:
- 内部处理逻辑错误
- 需要进一步排查具体原因

**相关代码位置**: [thread.rs](file:///home/hula/synapse_rust/synapse/src/web/routes/thread.rs)

---

#### 问题 #S3: 获取房间层级返回500 ❌ 需修复

**测试用例**:
```bash
curl "http://localhost:8008/_matrix/client/v1/rooms/{room_id}/hierarchy" \
  -H "Authorization: Bearer {token}"
```

**实际响应**:
```json
{
  "status": "error",
  "error": "Internal error: Database error: error returned from database: column \"guest_access\" does not exist",
  "errcode": "M_INTERNAL_ERROR"
}
```
**HTTP状态码**: 500

**问题分析**:
- `rooms` 表缺少 `guest_access` 列
- 需要添加缺失的列

**相关代码位置**: [room_hierarchy.rs](file:///home/hula/synapse_rust/synapse/src/web/routes/room_hierarchy.rs)

---

#### 问题 #S4: 时间戳转事件 ✅ 已修复

**测试用例**:
```bash
curl "http://localhost:8008/_matrix/client/v1/rooms/{room_id}/timestamp_to_event?ts=1234567890000&dir=f" \
  -H "Authorization: Bearer {token}"
```

**修复前响应**:
```json
{
  "status": "error",
  "error": "Internal error: Database error: error returned from database: relation \"room_members\" does not exist",
  "errcode": "M_INTERNAL_ERROR"
}
```

**修复后响应**:
```json
{
  "status": "error",
  "error": "Not a member of this room",
  "errcode": "M_FORBIDDEN"
}
```
**HTTP状态码**: 403

**修复方案**:
- 创建 `room_members` 表
- 迁移脚本: `20260219000000_unified_fix.sql`
- 注: 403是正确的权限错误响应

**相关代码位置**: [room.rs](file:///home/hula/synapse_rust/synapse/src/web/routes/room.rs)

### 8.3 测试通过的用例

以下测试用例已通过：

| 测试用例 | 描述 | HTTP状态码 | 结果 |
|---------|------|-----------|------|
| 1.2 | 无认证搜索 | 401 | ✅ 通过 |
| 3.3 | 无认证获取房间线程 | 401 | ✅ 通过 |
| 4.3 | 无认证获取房间层级 | 401 | ✅ 通过 |
| 5.2 | 缺少必要参数 | 400 | ✅ 通过 |
| 5.3 | 无认证时间戳转事件 | 401 | ✅ 通过 |

### 8.4 参考资料

- Matrix Search API: https://spec.matrix.org/v1.11/client-server-api/#searching
- MSC3316: Room Hierarchy: https://github.com/matrix-org/matrix-spec-proposals/pull/3316

---

## 九、问题修复汇总

### 9.1 已修复问题 (11个)

| 问题ID | 描述 | 修复方案 |
|--------|------|----------|
| #M1 | 下载不存在的媒体返回200 | 修改返回类型使用ApiError |
| #V2 | 获取语音消息返回500 | 添加processed_ts列 |
| #V3 | 获取用户语音消息返回500 | 添加processed_ts列 |
| #V4 | 获取房间语音消息返回500 | 添加processed_ts列 |
| #P1 | 获取推送器列表返回500 | 创建pushers表 |
| #P4 | 获取特定推送规则返回500 | 创建push_rules表 |
| #P5 | 设置推送规则返回500 | 创建push_rules表 |
| #P6 | 删除推送规则返回500 | 创建push_rules表 |
| #P7 | 设置推送规则动作返回500 | 创建push_rules表 |
| #P8 | 设置推送规则启用状态返回500 | 创建push_rules表 |
| #S1 | 搜索功能返回500 | 添加events.type列 |
| #S4 | 时间戳转事件返回500 | 创建room_members表 |

### 9.2 仍需修复问题 (9个)

| 问题ID | 描述 | 优先级 |
|--------|------|--------|
| #M2 | 删除自己上传的媒体返回403 | 高 |
| #M3 | 上传媒体指定ID返回400 | 中 |
| #M4 | 获取缩略图错误状态码 | 中 |
| #V1 | 上传语音消息返回400 | 高 |
| #V5 | 删除不存在的语音消息返回200 | 中 |
| #P2 | 设置推送器返回422 | 高 |
| #P3 | 获取类型推送规则返回500 | 高 |
| #P9 | 获取不存在的作用域返回200 | 低 |
| #S2 | 获取房间线程返回500 | 高 |
| #S3 | 获取房间层级返回500 | 高 |

### 9.3 修复进度

- **总问题数**: 20个
- **已修复**: 11个 (55%)
- **仍需修复**: 9个 (45%)
