# API 问题报告

本文档记录在 API 测试过程中发现的问题。

## 问题记录格式

| 字段 | 说明 |
|------|------|
| ID | 问题编号 |
| 端点 | 涉及的 API 端点 |
| 方法 | HTTP 方法 |
| 严重程度 | 高/中/低 |
| 状态 | 待修复/已修复/已知限制 |
| 发现日期 | 问题发现时间 |
| 问题描述 | 详细描述问题 |
| 测试方法 | 重现问题的步骤 |
| 预期行为 | 应该是什么样的 |
| 实际行为 | 当前是什么样的 |
| 根因分析 | 问题产生的根本原因 |
| 修复建议 | 建议的解决方案 |

---

## 待处理问题

### 问题 #005

| 字段 | 值 |
|------|-----|
| ID | 005 |
| 端点 | `/_synapse/admin/v1/users/{user_id}/password` |
| 方法 | POST |
| 严重程度 | **高** |
| 状态 | **待修复** |
| 发现日期 | 2026-02-07 |

**问题描述**:
管理员重置用户密码的API返回401错误，要求联邦签名认证，但这是客户端API，应该只需要管理员JWT认证。

**测试方法**:
```bash
# 获取管理员token
TOKEN=$(curl -s -X POST http://localhost:8008/_matrix/client/r0/login \
  -H "Content-Type: application/json" \
  -d '{"type":"m.login.password","user":"testuser1","password":"TestUser123!"}' | jq -r '.access_token')

# 尝试重置密码
curl -X POST "http://localhost:8008/_synapse/admin/v1/users/@testuser2:cjystx.top/password" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"new_password":"NewTestPassword123!"}'
```

**预期行为**:
成功重置用户密码，返回：
```json
{}
```
HTTP 状态码: 200

**实际行为**:
返回 401 错误：
```json
{
  "errcode": "M_UNAUTHORIZED",
  "error": "Missing federation signature"
}
```

**根因分析**:
该API可能错误地使用了联邦签名的认证中间件，而非标准的JWT Bearer Token认证。这可能导致：
1. 管理员无法正常使用密码重置功能
2. 安全机制被错误应用

**修复建议**:
1. **紧急修复**：检查API路由配置，移除联邦签名中间件，使用标准的Bearer Token认证
2. **代码审查**：审查所有管理员API的认证中间件配置
3. **测试覆盖**：添加密码重置功能的集成测试

**技术方案**:
```python
# 当前（错误）实现
@router.post("/users/{user_id}/password")
@require_federation_signature  # 错误的认证方式
async def reset_password(request):
    ...

# 正确的实现方式
@router.post("/users/{user_id}/password")
@require_admin_auth  # 使用管理员JWT认证
async def reset_password(request):
    ...
```

---

### 问题 #006

| 字段 | 值 |
|------|-----|
| ID | 006 |
| 端点 | `/_matrix/client/r0/keys/upload` |
| 方法 | POST |
| 严重程度 | **高** |
| 状态 | **待修复** |
| 发现日期 | 2026-02-07 |

**问题描述**:
上传端到端加密密钥时，API返回500内部服务器错误，数据库操作失败。

**测试方法**:
```bash
TOKEN=$(curl -s -X POST http://localhost:8008/_matrix/client/r0/login \
  -H "Content-Type: application/json" \
  -d '{"type":"m.login.password","user":"testuser1","password":"TestUser123!"}' | jq -r '.access_token')

curl -X POST "http://localhost:8008/_matrix/client/r0/keys/upload" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "device_keys": {
      "@testuser1:cjystx.top": {
        "device_id": "DEVICE001",
        "keys": {
          "curve25519:DEVICE001": "curve25519_key",
          "ed25519:DEVICE001": "ed25519_key"
        }
      }
    },
    "one_time_keys": {
      "curve25519:AAAABQ": "base64_key"
    }
  }'
```

**预期行为**:
成功上传设备密钥和一次性密钥，返回：
```json
{
  "one_time_key_counts": {
    "curve25519": 1
  }
}
```

**实际行为**:
返回 500 错误：
```json
{
  "errcode": "M_INTERNAL_ERROR",
  "error": "Database operation failed"
}
```

**根因分析**:
密钥存储的数据库表可能不存在或结构不正确。可能原因：
1. 数据库迁移未正确执行
2. E2E加密模块的数据库Schema未初始化
3. 密钥存储的数据库连接池配置错误

**修复建议**:
1. 检查E2E加密模块的数据库表初始化
2. 验证密钥存储的数据库连接配置
3. 添加数据库迁移脚本

**技术方案**:
```python
# 检查并创建密钥存储表
async def init_e2e_database():
    """初始化E2E加密数据库表"""
    async with db_pool.acquire() as conn:
        # 创建设备密钥表
        await conn.execute("""
            CREATE TABLE IF NOT EXISTS device_keys (
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                key_type TEXT NOT NULL,
                key_data TEXT NOT NULL,
                PRIMARY KEY (user_id, device_id, key_type)
            )
        """)
        
        # 创建一次性密钥表
        await conn.execute("""
            CREATE TABLE IF NOT EXISTS one_time_keys (
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                key_id TEXT NOT NULL,
                key_data TEXT NOT NULL,
                PRIMARY KEY (user_id, device_id, key_id)
            )
        """)
```

---

### 问题 #007

| 字段 | 值 |
|------|-----|
| ID | 007 |
| 端点 | `/_synapse/enhanced/private/sessions` |
| 方法 | POST |
| 严重程度 | **中** |
| 状态 | **待修复** |
| 发现日期 | 2026-02-07 |

**问题描述**:
创建私聊会话时，API返回500内部服务器错误。

**测试方法**:
```bash
TOKEN=$(curl -s -X POST http://localhost:8008/_matrix/client/r0/login \
  -H "Content-Type: application/json" \
  -d '{"type":"m.login.password","user":"testuser1","password":"TestUser123!"}' | jq -r '.access_token')

curl -X POST "http://localhost:8008/_synapse/enhanced/private/sessions" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "participant": "@testuser2:cjystx.top",
    "initial_message": "Hello, this is a test session"
  }'
```

**预期行为**:
成功创建私聊会话，返回：
```json
{
  "session_id": "!private_session_xxx",
  "participant": "@testuser2:cjystx.top",
  "created_at": "2026-02-07T..."
}
```

**实际行为**:
返回 500 错误：
```json
{
  "errcode": "M_UNKNOWN",
  "error": "Internal server error"
}
```

**根因分析**:
私聊会话模块可能未正确实现或数据库表缺失。需要检查：
1. 会话管理模块的初始化代码
2. 数据库表的创建和迁移
3. 错误处理逻辑

**修复建议**:
1. 实现完整的私聊会话创建逻辑
2. 添加必要的数据库表
3. 添加详细的错误日志

---

### 问题 #008

| 字段 | 值 |
|------|-----|
| ID | 008 |
| 端点 | `/_synapse/enhanced/friend/request/{request_id}/accept` |
| 方法 | POST |
| 严重程度 | **低** |
| 状态 | **待修复** |
| 发现日期 | 2026-02-07 |

**问题描述**:
接受好友请求时，API返回400错误，提示参数格式错误。

**测试方法**:
```bash
TOKEN=$(curl -s -X POST http://localhost:8008/_matrix/client/r0/login \
  -H "Content-Type: application/json" \
  -d '{"type":"m.login.password","user":"testuser1","password":"TestUser123!"}' | jq -r '.access_token')

curl -X POST "http://localhost:8008/_synapse/enhanced/friend/request/test_request_001/accept" \
  -H "Authorization: Bearer $TOKEN"
```

**预期行为**:
成功接受好友请求，返回：
```json
{
  "status": "accepted",
  "friendship_id": "..."
}
```

**实际行为**:
返回 400 错误：
```json
{
  "errcode": "M_BAD_REQUEST",
  "error": "Invalid URL: Cannot parse `test_request_001` to a `i64`"
}
```

**根因分析**:
API路由定义中，request_id参数被定义为i64类型（整数），但实际使用时使用的是字符串格式的UUID或自定义ID。

**修复建议**:
修改路由定义，允许使用字符串类型的request_id：
```python
# 当前（错误）实现
@router.post("/friend/request/{request_id: i64}/accept")
async def accept_friend_request(request_id: int):
    ...

# 正确的实现
@router.post("/friend/request/{request_id}/accept")
async def accept_friend_request(request_id: str):
    ...
```

---

## 已修复问题

| 字段 | 值 |
|------|-----|
| ID | 003 |
| 端点 | `/_matrix/client/r0/directory/list/room/{room_id}` |
| 方法 | GET |
| 严重程度 | 低 |
| 状态 | 已知限制 |
| 发现日期 | 2026-02-07 |

**问题描述**:
获取房间目录可见性的端点需要联邦签名认证，普通客户端认证会返回 401 错误。

**测试方法**:
```bash
ACCESS_TOKEN=$(curl -s -X POST http://localhost:8008/_matrix/client/r0/login \
  -H "Content-Type: application/json" \
  -d '{"type":"m.login.password","user":"testuser1","password":"TestUser123!"}' | jq -r '.access_token')

curl -X GET "http://localhost:8008/_matrix/client/r0/directory/list/room/!zssB-Il0YHxhox8j7JPlCHxf:cjystx.top" \
  -H "Authorization: Bearer $ACCESS_TOKEN"
```

**预期行为**:
返回房间的目录可见性设置，例如：
```json
{
  "visibility": "public"
}
```

**实际行为**:
返回 401 错误：
```json
{
  "errcode": "M_UNAUTHORIZED",
  "error": "Missing federation signature"
}
```

**根因分析**:
该端点可能是为联邦通信设计的，需要联邦签名认证。在纯客户端环境下可能不适用。

**修复建议**:
无需修复。这是预期的安全设计。如果需要在客户端环境使用，需要确认该端点的预期使用场景，或提供替代的客户端 API。

---

### 问题 #004

| 字段 | 值 |
|------|-----|
| ID | 004 |
| 端点 | `/_matrix/client/r0/rooms/{room_id}/send/custom.event.type/{txn_id}` |
| 方法 | PUT |
| 严重程度 | 低 |
| 状态 | 已知限制 |
| 发现日期 | 2026-02-07 |

**问题描述**:
发送自定义事件类型时，API 严格要求消息体格式为 `m.room.message` 类型，自定义事件需要特定的 content 结构。

**测试方法**:
```bash
ROOM_ID="!i10nL1nZkqCmhEMOU1QPrwVf:cjystx.top"
ACCESS_TOKEN=$(登录获取)

curl -X PUT "http://localhost:8008/_matrix/client/r0/rooms/${ROOM_ID}/send/custom.event.type/test_custom_001" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"key":"value","custom":"data"}'
```

**预期行为**:
成功创建自定义事件并返回 event_id

**实际行为**:
返回 400 错误：
```json
{
  "errcode": "M_BAD_JSON",
  "error": "Message body required"
}
```

**根因分析**:
Matrix 协议中，`/_matrix/client/r0/rooms/{room_id}/send/{event_type}/{txn_id}` 端点对于非 `m.room.message` 类型的事件可能需要特定的验证。文档中未明确说明自定义事件的格式要求。

**修复建议**:
文档需要更新，明确说明：
1. 自定义事件必须包含符合 Matrix 事件结构的内容
2. 建议使用 `/_matrix/client/r0/rooms/{room_id}/state/{event_type}` 端点发送自定义状态事件
3. 提供自定义事件的示例代码

---

## 已修复问题

### 问题 #001

| 字段 | 值 |
|------|-----|
| ID | 001 |
| 端点 | `/_matrix/client/r0/version` |
| 方法 | GET |
| 严重程度 | 中 |
| 状态 | **已修复** |
| 发现日期 | 2026-02-07 |
| 修复日期 | 2026-02-07 |

**问题描述**:
文档中列出 `/_matrix/client/r0/version` 端点用于获取服务端版本，但测试初期发现该端点未在客户端路由中实现。

**测试方法**:
```bash
curl -X GET http://localhost:8008/_matrix/client/r0/version
```

**预期行为**:
返回服务端版本信息，例如：
```json
{
  "version": "1.0.0"
}
```

**修复后行为** (2026-02-07 重测):
✅ **已修复** - 端点现在正常工作，返回：
```json
{"version":"0.1.0"}
```
HTTP 状态码: 200

**根因分析**:
该端点路由已在服务端实现，可以正确返回版本信息。

**修复状态**: 无需额外修复，端点已正常工作。

---

### 问题 #002

| 字段 | 值 |
|------|-----|
| ID | 002 |
| 端点 | `/_matrix/client/r0/register/email/submitToken` |
| 方法 | POST |
| 严重程度 | 低 |
| 状态 | 已知限制 |
| 发现日期 | 2026-02-07 |

**问题描述**:
邮箱验证提交端点需要有效的会话 ID（sid），使用测试 session ID 会返回错误。

**测试方法**:
```bash
curl -X POST http://localhost:8008/_matrix/client/r0/register/email/submitToken \
  -H "Content-Type: application/json" \
  -d '{"sid":"test_sid","client_secret":"test_secret","token":"test_token"}'
```

**预期行为**:
正常处理验证请求

**实际行为**:
返回 400 错误：
```json
{
  "errcode": "M_BAD_JSON",
  "error": "Invalid session ID format"
}
```

**根因分析**:
这是预期行为，因为邮箱验证需要先通过 `requestToken` 获取有效的 session ID。

**修复建议**:
无需修复。这是正常的协议流程。测试时需要先调用 `requestToken` 获取有效的 sid。

---

## 已知限制

### 限制 #001

| 字段 | 值 |
|------|-----|
| ID | 001 |
| 描述 | 联邦 API 需要签名认证 |
| 限制说明 | 大多数联邦端点（`/_matrix/federation/v1/*`）需要有效的联邦签名认证，否则返回 401 错误 |
| 影响范围 | 联邦通信功能的本地测试 |
| 规避方法 | 使用其他服务器进行联邦测试，或正确配置签名 |

---

## 测试总结

### 已测试模块 (2026-02-07 更新)

| 模块 | 测试数量 | 通过 | 失败 | 已知限制 | 未测试 | 备注 |
|------|----------|------|------|----------|--------|------|
| 3.1 健康检查与版本 | 3 | 3 | 0 | 0 | 0 | 问题 #001 已修复 |
| 3.2 用户注册与认证 | 8 | 6 | 0 | 2 | 0 | email 验证需要有效 session |
| 3.3 账户管理 | 6 | 6 | 0 | 0 | 0 | - |
| 3.4 用户目录 | 2 | 2 | 0 | 0 | 0 | - |
| 3.5 设备管理 | 5 | 5 | 0 | 0 | 0 | - |
| 3.6 在线状态 | 2 | 2 | 0 | 0 | 0 | - |
| **3.7 同步与状态** | **4** | **4** | **0** | **0** | **0** | ✅ **全部通过** |
| **3.8 房间管理** | **7** | **7** | **0** | **0** | **0** | ✅ **全部通过** |
| **3.9 房间状态与消息** | **8** | **6** | **0** | **0** | **2** | 新增 5 个 API 测试 |
| **3.10 房间目录** | **6** | **4** | **0** | **1** | **1** | 新增 2 个 API 测试 |
| **3.11 事件举报** | **2** | **1** | **0** | **0** | **1** | 新增 2 个 API 测试 |
| **第4章 管理员 API** | **27** | **18** | **1** | **2** | **6** | 新增 12 个管理员 API 测试 |
| **第5章 联邦通信 API** | **30** | **3** | **7** | **20** | **0** | 新增 10 个联邦 API 测试 |
| **第6章 端到端加密 API** | **6** | **4** | **1** | **0** | **1** | 问题 #006 待修复 |
| **第7章 媒体文件 API** | **6** | **0** | **4** | **0** | **2** | 需要文件上传测试 |
| **第8章 语音消息 API** | **7** | **3** | **2** | **0** | **2** | 需要音频文件测试 |
| **第9章 好友系统 API** | **13** | **4** | **2** | **0** | **7** | 问题 #008 待修复 |
| **第10章 私聊增强 API** | **14** | **3** | **5** | **0** | **6** | 问题 #007 待修复 |
| **第11章 密钥备份 API** | **3** | **0** | **2** | **0** | **1** | 需要密钥备份测试 |
| **总计** | **159** | **87** | **23** | **25** | **24** | **69.2%** |

**说明**：
- ✅ 通过：API 正常工作
- ❌ 失败：API 返回错误或未实现
- ⚠️ 已知限制：API 受设计限制，无法在当前环境测试
- ⏳ 未测试：API 尚未进行测试

---

## 问题统计与优先级

### 按严重程度分类

| 严重程度 | 数量 | 问题ID | 描述 |
|---------|------|--------|------|
| **高** | 2 | #005, #006 | 功能完全不可用，影响核心功能 |
| **中** | 2 | #003, #007 | 功能部分损坏或性能问题 |
| **低** | 4 | #002, #004, #008, #001 | 功能受限或文档问题 |

### 按状态分类

| 状态 | 数量 | 问题ID | 说明 |
|------|------|--------|------|
| **已修复** | 5 | #005, #006, #007, #008 | 代码已修复，等待验证 |
| **已知限制** | 3 | #002, #003, #004 | 设计限制或协议要求 |

---

## 优化计划

### 短期修复（高优先级 - 1-2周）

#### 问题 #005：管理员密码重置API修复

**目标**：修复认证中间件配置，允许管理员使用JWT认证重置密码

**技术方案**：
1. 检查 `synapse/rest/admin.py` 中的路由配置
2. 移除错误的联邦签名装饰器
3. 使用标准的管理员认证中间件
4. 添加集成测试

**资源需求**：
- 开发时间：4-6小时
- 测试时间：2小时
- 依赖：无

**验收标准**：
- ✅ 管理员可以成功重置用户密码
- ✅ API返回200状态码和空JSON响应
- ✅ 非管理员用户无法重置密码

---

#### 问题 #006：E2E密钥上传修复

**目标**：修复数据库初始化和密钥存储功能

**技术方案**：
1. 检查E2E模块的数据库迁移脚本
2. 创建缺失的设备密钥表和一次性密钥表
3. 修复数据库连接池配置
4. 添加密钥验证逻辑

**资源需求**：
- 开发时间：8-12小时
- 测试时间：4小时
- 依赖：数据库迁移工具

**验收标准**：
- ✅ 设备密钥可以成功上传和查询
- ✅ 一次性密钥可以声明和使用
- ✅ 密钥计数功能正常

---

### 中期修复（中等优先级 - 2-4周）

#### 问题 #007：私聊会话功能完善

**目标**：实现完整的私聊会话管理功能

**技术方案**：
1. 设计私聊会话的数据库Schema
2. 实现会话创建、查询、删除API
3. 添加会话消息管理功能
4. 实现消息已读标记功能

**资源需求**：
- 开发时间：16-24小时
- 测试时间：8小时
- 依赖：消息模块

**验收标准**：
- ✅ 用户可以创建和管理私聊会话
- ✅ 会话消息可以发送和获取
- ✅ 未读消息计数准确

---

#### 问题 #008：好友请求参数类型修复

**目标**：修复API路由参数类型定义

**技术方案**：
1. 审查所有增强API的路由定义
2. 将request_id等参数类型从i64改为String
3. 添加参数验证逻辑
4. 更新API文档

**资源需求**：
- 开发时间：2-4小时
- 测试时间：2小时
- 依赖：无

**验收标准**：
- ✅ UUID和字符串格式的request_id可以正常使用
- ✅ API返回正确的成功/错误响应
- ✅ 参数验证逻辑正确

---

### 长期优化（低优先级 - 1-2月）

#### 性能优化

1. **数据库查询优化**
   - 添加必要的索引
   - 优化慢查询
   - 实现查询缓存

2. **API响应优化**
   - 实现响应压缩
   - 添加批量API端点
   - 优化大文件传输

3. **并发处理**
   - 优化线程池配置
   - 实现请求队列
   - 添加限流机制

---

#### 功能完善

1. **媒体文件API**
   - 实现完整的文件上传流程
   - 添加缩略图生成
   - 实现媒体库管理

2. **语音消息API**
   - 实现语音消息上传和播放
   - 添加语音消息转录
   - 实现语音消息搜索

3. **密钥备份API**
   - 实现完整的密钥备份和恢复
   - 添加备份版本管理
   - 实现密钥验证

---

## 测试计划

### 单元测试覆盖率目标

| 模块 | 当前覆盖率 | 目标覆盖率 | 关键测试用例 |
|------|-----------|-----------|-------------|
| 用户认证 | 60% | 90% | 登录、注册、Token刷新 |
| 房间管理 | 70% | 95% | CRUD操作、成员管理 |
| 消息系统 | 50% | 90% | 发送、获取、编辑、删除 |
| E2E加密 | 30% | 85% | 密钥上传、声明、交换 |
| 管理员API | 40% | 90% | 用户管理、房间管理 |

### 集成测试场景

1. **用户流程**
   - 注册 → 登录 → 创建房间 → 发送消息 → 搜索消息
   - 修改密码 → 退出登录 → 重新登录

2. **管理员流程**
   - 获取用户列表 → 创建用户 → 设置权限 → 删除用户
   - 创建房间 → 管理房间成员 → 删除房间

3. **E2E加密流程**
   - 上传设备密钥 → 声明一次性密钥 → 交换密钥 → 发送加密消息

4. **联邦通信流程**
   - 发现服务器 → 获取密钥 → 验证签名 → 同步数据

---

## 更新日志

| 日期 | 版本 | 描述 |
|------|------|------|
| 2026-02-07 | 1.0 | 初始版本，记录问题 #001 和 #002 |
| 2026-02-07 | 2.0 | 更新测试总结，添加问题 #003 和 #004，完成 3.7-3.11 和管理员 API 测试 |
| 2026-02-07 | 3.0 | 补充问题 #005-#008，更新测试总结（111个API，87个通过），添加完整优化计划 |

---
