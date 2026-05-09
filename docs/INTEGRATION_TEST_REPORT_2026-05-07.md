# 前后端联调测试问题清单

**测试日期**: 2026-05-07
**后端地址**: https://matrix.test
**前端应用**: Element Desktop
**后端版本**: synapse-rust 0.1.0
**测试工具**: curl + Element Desktop 浏览器控制台

---

## 一、已修复问题

### FIX-01: stream_ordering 列缺失导致同步 API 500 错误
- **现象**: `GET /_matrix/client/v3/sync` 返回 500 Internal Server Error，日志报错 `no column found for name: stream_ordering`
- **根因**: `get_room_events_batch_inner` 查询的 SELECT 列表缺少 `stream_ordering` 列，但 `RoomEvent` 结构体通过 `sqlx::FromRow` 自动映射该字段
- **修复**: 在 `src/storage/event.rs` 的 `get_room_events_batch_inner` 方法的内外两层 SELECT 中均添加 `stream_ordering`
- **验证**: 同步 API 现在正常返回数据，next_batch token 格式正确 (如 `s16_0_11`)

### FIX-02: Redis 认证失败导致缓存全面不可用
- **现象**: 日志持续报错 `NOAUTH: Authentication required`，熔断器反复开启，限流功能降级
- **根因**: `config/homeserver.yaml` 中 Redis 配置缺少 `password` 字段，而 Redis 容器配置了密码认证
- **修复**: 在 `docker/deploy/config/homeserver.yaml` 的 redis 段添加 `password: "${REDIS_PASSWORD:?REDIS_PASSWORD is required}"`
- **验证**: Redis 连接正常，熔断器不再误触发，缓存功能恢复

---

## 二、严重问题 (Critical / High)

### BUG-01: 增量同步返回全量数据 [Critical] ✅ 已修复
- **修复日期**: 2026-05-07
- **修复方案**: 新增 stream_ordering 增量查询方法，sync_service 优先使用 stream_ordering 过滤
- **验证结果**: 增量同步仅返回新增事件（1条），不再返回全量数据
- **现象**: 使用 `since` token 进行增量同步时，返回了所有历史事件而非仅新增事件
- **复现步骤**:
  1. 执行初始同步获取 `next_batch` token (如 `s16_0_11`)
  2. 发送新消息
  3. 使用 `since=s16_0_11` 执行增量同步
  4. 观察返回的 timeline 包含所有事件（9条），而非仅新增事件（1条）
- **影响范围**: 所有客户端的增量同步功能，导致大量不必要的数据传输，严重影响性能和用户体验
- **严重程度**: Critical
- **根因分析**: `get_room_events_batch_inner` 方法使用 `origin_server_ts > $since` 进行过滤，但 sync token 中的 `stream_id` 是 `stream_ordering` 值（如 16、17），不是毫秒级时间戳。`event_since_ts` 方法中 `TIMESTAMP_TOKEN_MIN = 1_000_000_000_000`，当 stream_id < 此阈值时直接返回 0，导致 `since` 条件被忽略
- **优化方案**:
  1. 将增量同步查询从基于 `origin_server_ts` 过滤改为基于 `stream_ordering` 过滤
  2. 修改 `get_room_events_batch_inner` 方法，当 `since` 为 stream_ordering 类型时使用 `stream_ordering > $since` 条件
  3. 修改 `event_since_ts` 方法，正确区分 stream_ordering token 和时间戳 token
- **优先级**: P0 - 立即修复

### BUG-02: 房间成员 API 返回格式不符合 Matrix 规范 [Critical] ✅ 已修复
- **修复日期**: 2026-05-07
- **修复方案**: 将 RoomMember 数据模型转换为标准 m.room.member 事件格式
- **验证结果**: 成员API返回标准Matrix事件格式，包含type/state_key/content/event_id等字段
- **现象**: `GET /_matrix/client/v3/rooms/{roomId}/members` 返回内部数据库字段而非标准 Matrix 事件格式
- **复现步骤**:
  1. 创建房间并加入
  2. 调用 `GET /rooms/{roomId}/members`
  3. 观察返回的 chunk 中包含 `ban_reason`, `banned_by`, `banned_ts`, `is_banned`, `join_reason`, `joined_ts`, `left_ts`, `invite_token` 等内部字段
- **实际返回**:
  ```json
  {
    "chunk": [{
      "avatar_url": null,
      "ban_reason": null,
      "banned_by": null,
      "membership": "join",
      "user_id": "@apitest1:matrix.test",
      "joined_ts": 1778125688851,
      ...
    }]
  }
  ```
- **期望返回** (Matrix 规范):
  ```json
  {
    "chunk": [{
      "content": {"membership": "join", "avatar_url": null, "displayname": null},
      "event_id": "$...",
      "origin_server_ts": 1778125688851,
      "room_id": "!...",
      "sender": "@apitest1:matrix.test",
      "state_key": "@apitest1:matrix.test",
      "type": "m.room.member"
    }]
  }
  ```
- **影响范围**: Element Desktop 无法正确解析房间成员列表，导致成员面板显示异常
- **严重程度**: Critical
- **优化方案**: 修改成员查询接口，将内部数据模型转换为标准 Matrix 事件格式，包含 `content`、`state_key`、`type`、`origin_server_ts` 等标准字段
- **优先级**: P0 - 立即修复

### BUG-03: 媒体上传忽略 content_type 参数 [High] ✅ 已修复
- **修复日期**: 2026-05-07
- **修复方案**: 优先读取查询参数content_type，回退到Content-Type header
- **验证结果**: 上传文件指定content_type=text/plain，正确返回text/plain
- **现象**: `POST /_matrix/media/v3/upload?content_type=text/plain` 上传后返回的 `content_type` 为 `application/x-www-form-urlencoded`
- **复现步骤**:
  1. 上传文本文件并指定 `content_type=text/plain`
  2. 观察返回的 `content_type` 为 `application/x-www-form-urlencoded`
- **影响范围**: 媒体文件 MIME 类型错误，导致客户端无法正确渲染图片/视频/音频等媒体内容
- **严重程度**: High
- **优化方案**: 修改媒体上传处理器，正确读取并保存 `content_type` 查询参数，而非使用请求的 Content-Type header
- **优先级**: P1

### BUG-04: OTK 数量声明接口返回空响应 [High] ✅ 已修复
- **修复日期**: 2026-05-07
- **修复方案**: upload_keys方法末尾增加OTK计数查询，确保始终返回one_time_key_counts
- **验证结果**: POST /keys/upload body={} 返回 {"one_time_key_counts":{}}
- **现象**: `POST /_matrix/client/v3/keys/upload/{deviceId}` 声明 OTK 数量时返回空响应
- **复现步骤**:
  1. 调用 `POST /keys/upload/{deviceId}` body: `{"signed_curve25519": 50}`
  2. 观察返回空响应
- **期望返回**: `{"signed_curve25519": 50}` (服务端剩余 OTK 数量)
- **影响范围**: E2EE 密钥交换流程受阻，客户端无法知道需要上传多少 OTK，可能导致加密通信失败
- **严重程度**: High
- **优化方案**: 修改 OTK 数量声明接口，返回当前服务端存储的各类型 OTK 数量
- **优先级**: P1

### BUG-05: 事件上下文 API 返回 404 [High] ⚪ 非BUG
- **验证日期**: 2026-05-07
- **验证结论**: 使用正确的URL编码event_id调用context API正常返回数据，之前测试使用了错误的event_id格式
- **现象**: `GET /_matrix/client/v3/rooms/{roomId}/context/{eventId}` 返回 `M_NOT_FOUND`
- **复现步骤**:
  1. 发送消息获取 event_id
  2. 调用 context API 查询该事件
  3. 返回 404
- **影响范围**: 客户端无法获取消息上下文（前后消息），影响消息搜索和跳转功能
- **严重程度**: High
- **优化方案**: 检查事件查询逻辑，确保通过 event_id 可以正确检索事件及其上下文
- **优先级**: P1

---

## 三、中等问题 (Medium)

### BUG-06: 同步响应中事件缺少标准字段 [Medium] ⚪ 非BUG
- **验证日期**: 2026-05-07
- **验证结论**: 同步事件包含type/content/sender/origin_server_ts/event_id/room_id/state_key等所有标准字段
- **现象**: 同步返回的 timeline 事件中缺少 `origin_server_ts`、`sender` 等标准字段，或字段名不符合规范
- **影响范围**: 客户端可能无法正确显示消息时间和发送者
- **严重程度**: Medium
- **优化方案**: 检查同步响应的事件序列化逻辑，确保所有标准字段都正确包含
- **优先级**: P2

### BUG-07: 设备密钥上传签名验证过于严格 [Medium]
- **现象**: `POST /_matrix/client/v3/keys/upload` 上传设备密钥时，即使签名格式略有偏差也会返回 `M_BAD_JSON: Invalid signature on device keys: Invalid base64 encoding`
- **影响范围**: 某些客户端生成的签名可能无法通过验证，导致 E2EE 初始化失败
- **严重程度**: Medium
- **优化方案**: 参考 Synapse 实现放宽签名验证，允许更宽松的 base64 编码格式（如包含换行、空格等）
- **优先级**: P2

### BUG-08: 联邦版本端点响应格式不规范 [Medium] ✅ 已修复
- **修复日期**: 2026-05-07
- **修复方案**: 移除顶层重复的version字段
- **验证结果**: 返回 {"server":{"name":"synapse-rust","version":"0.1.0"}}
- **现象**: `GET /_matrix/federation/v1/version` 返回 `{"server":{"name":"synapse-rust","version":"0.1.0"},"version":"0.1.0"}`，存在重复的 `version` 字段
- **期望格式**: `{"server":{"name":"synapse-rust","version":"0.1.0"}}`
- **影响范围**: 联邦兼容性，其他服务器可能解析异常
- **严重程度**: Medium
- **优化方案**: 移除顶层重复的 `version` 字段
- **优先级**: P2

### BUG-09: 密钥备份版本接口返回格式待验证 [Medium]
- **现象**: `GET /_matrix/client/v3/room_keys/version` 返回 `M_NOT_FOUND: No current backup version`
- **分析**: 这是正确的行为（没有创建备份时返回 404），但需确认创建备份后能正确返回版本信息
- **严重程度**: Medium
- **优先级**: P2

### BUG-10: 事务 ID 未实现幂等性去重 [Medium] ✅ 已修复
- **修复日期**: 2026-05-07
- **修复方案**: 使用Redis缓存txnId->event_id映射，1小时过期
- **验证结果**: 相同txnId第二次发送返回相同event_id
- **现象**: `PUT /rooms/{roomId}/send/{eventType}/{txnId}` 中的 `txnId` 参数被提取但未使用
- **影响范围**: 网络重试时可能导致消息重复发送
- **严重程度**: Medium
- **优化方案**: 实现 txnId 的幂等性缓存，相同 txnId 在有效期内返回相同结果
- **优先级**: P2

---

## 四、低优先级问题 (Low)

### BUG-11: 设备列表缺少 last_seen_ip 字段 [Low]
- **现象**: `GET /_matrix/client/v3/devices` 返回的设备信息缺少 `last_seen_ip` 字段
- **影响范围**: 用户无法在设备管理页面看到设备的最后登录 IP
- **严重程度**: Low
- **优先级**: P3

### BUG-12: CORS 配置未包含 https://matrix.test [Low] ✅ 已修复
- **修复日期**: 2026-05-07
- **修复方案**: 在homeserver.yaml的CORS allowed_origins中添加https://matrix.test
- **现象**: homeserver.yaml 的 CORS allowed_origins 列表未包含 `https://matrix.test`，可能导致浏览器跨域请求被拒绝
- **影响范围**: 通过浏览器访问时可能出现 CORS 错误
- **严重程度**: Low
- **优化方案**: 在 CORS 配置中添加 `https://matrix.test` 或使用环境变量动态配置
- **优先级**: P3

---

## 五、E2EE 专项测试结果

| API 端点 | 方法 | 状态 | 说明 |
|---------|------|------|------|
| `/_matrix/client/v3/keys/upload` | POST | ⚠️ 部分正常 | 设备密钥上传需有效签名，OTK 上传功能正常 |
| `/_matrix/client/v3/keys/upload/{deviceId}` | POST | ❌ 异常 | OTK 数量声明返回空响应 |
| `/_matrix/client/v3/keys/query` | POST | ✅ 正常 | 密钥查询功能正常 |
| `/_matrix/client/v3/keys/changes` | GET | ✅ 正常 | 设备密钥变更查询正常 |
| `/_matrix/client/v3/sendToDevice/{eventType}/{txnId}` | PUT | ✅ 正常 | To-Device 消息发送正常 |
| `/_matrix/client/v3/room_keys/version` | GET | ✅ 正常 | 无备份时正确返回 404 |
| `/_matrix/client/v3/keys/signatures/upload` | POST | ⚠️ 待验证 | 交叉签名上传待深入测试 |

---

## 六、同步功能专项测试结果

| 测试项 | 状态 | 说明 |
|-------|------|------|
| 初始同步 (无 since) | ✅ 通过 | 正确返回房间列表、状态事件、账户数据 |
| next_batch token 生成 | ✅ 通过 | 格式为 `s{stream_id}_{to_device}_{device_list}` |
| 增量同步 (有 since) | ❌ 失败 | 返回全量数据而非增量数据 |
| 账户数据同步 | ✅ 通过 | pushrules 等默认账户数据正确返回 |
| Presence 同步 | ✅ 通过 | 在线状态正确返回 |
| To-Device 同步 | ⚠️ 待验证 | 发送成功但同步获取待验证 |
| Receipt 同步 | ⚠️ 待验证 | 标记已读成功但同步获取待验证 |
| Typing 同步 | ⚠️ 待验证 | 设置输入状态成功但同步获取待验证 |
| 设备列表变更同步 | ⚠️ 待验证 | keys/changes 正常但同步集成待验证 |

---

## 七、API 兼容性总览

| API 类别 | 测试端点数 | 通过 | 部分通过 | 失败 |
|---------|----------|------|---------|------|
| 认证 | 4 | 4 | 0 | 0 |
| 同步 | 3 | 1 | 1 | 1 |
| 房间 | 6 | 4 | 1 | 1 |
| E2EE | 6 | 3 | 2 | 1 |
| 媒体 | 2 | 1 | 0 | 1 |
| 设备 | 2 | 2 | 0 | 0 |
| Presence | 2 | 2 | 0 | 0 |
| 推送 | 1 | 1 | 0 | 0 |
| 联邦 | 1 | 0 | 1 | 0 |
| **总计** | **27** | **18** | **5** | **4** |

---

## 八、优先级排序与修复建议

### P0 - 立即修复 (影响核心功能)
1. **BUG-01**: 增量同步返回全量数据 → 修改 sync 查询使用 stream_ordering 过滤
2. **BUG-02**: 房间成员 API 格式错误 → 转换为标准 Matrix 事件格式

### P1 - 尽快修复 (影响重要功能)
3. **BUG-03**: 媒体上传 content_type 错误 → 正确读取查询参数
4. **BUG-04**: OTK 数量声明返回空 → 实现正确的 OTK 计数返回
5. **BUG-05**: 事件上下文 API 404 → 修复事件查询逻辑

### P2 - 计划修复 (影响体验)
6. **BUG-06**: 同步事件缺少标准字段 → 完善事件序列化
7. **BUG-07**: 签名验证过严 → 放宽 base64 解码
8. **BUG-08**: 联邦版本格式不规范 → 移除重复字段
9. **BUG-09**: 密钥备份待验证 → 完善测试
10. **BUG-10**: 事务 ID 幂等性 → 实现 txnId 缓存

### P3 - 后续优化 (锦上添花)
11. **BUG-11**: 设备列表缺 last_seen_ip → 添加字段
12. **BUG-12**: CORS 配置不完整 → 添加域名

---

## 九、技术实现建议

### 增量同步修复方案 (BUG-01)

核心问题在于 `sync_service.rs` 的 `event_since_ts` 方法将 stream_ordering 类型的 token 错误地转换为时间戳。建议修改为：

1. 在 `SyncToken` 中增加 `token_type` 字段区分 stream_ordering 和 timestamp
2. 修改 `get_room_events_batch_inner` 增加 `since_stream_ordering` 参数
3. 当 token 为 stream_ordering 类型时，使用 `stream_ordering > $since` 替代 `origin_server_ts > $since`
4. 保持对旧格式 token 的向后兼容

### 房间成员格式修复方案 (BUG-02)

需要将内部 `RoomMembership` 数据模型转换为标准 Matrix 事件格式：

```rust
fn membership_to_event(m: &RoomMembership) -> Value {
    json!({
        "type": "m.room.member",
        "state_key": m.user_id,
        "content": {
            "membership": m.membership,
            "displayname": m.display_name,
            "avatar_url": m.avatar_url,
        },
        "event_id": m.event_id,
        "origin_server_ts": m.joined_ts,
        "room_id": m.room_id,
        "sender": m.sender,
    })
}
```
