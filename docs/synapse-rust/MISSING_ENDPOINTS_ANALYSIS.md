# 91个Missing端点深度分析与优化方案

> **生成日期**: 2026-04-02
> **分析目的**: 区分真正未实现 vs 测试问题，评估实现必要性

---

## 一、测试结果总览

| 指标 | 数值 |
|------|------|
| **Passed** | 422 |
| **Failed** | 0 |
| **Missing** | 91 |
| **Skipped** | 39 |

---

## 二、Missing端点分类（91个）

### 2.1 已实现但测试脚本问题（约25个）

这些API在项目中已实现，但测试脚本使用了错误的URL路径或HTTP方法：

| 测试名称 | 实际状态 | 测试问题 |
|----------|----------|----------|
| Room Receipts | ✅ 已实现 | 测试使用GET而非POST |
| Room Redact | ✅ 已实现 | 测试路径问题 |
| Room Invite | ✅ 已实现 | 测试使用GET而非POST |
| Room Typing | ✅ 已实现 | 测试路径问题 |
| Room Members | ✅ 已实现 | 测试问题 |
| Profile | ✅ 已实现 | 测试问题 |
| Room Search | ✅ 已实现 | 测试使用GET而非POST |
| Account Data | ✅ 部分实现 | 测试问题 |
| Room Sync | ✅ 已实现 | 测试路径问题 |
| Room Timeline | ✅ 已实现 | 测试路径问题 |

### 2.2 真正未实现的API（约66个）

#### Room核心功能（约20个）

| 端点 | Matrix规范 | 实现必要性 | 说明 |
|------|-----------|-----------|------|
| `/_matrix/client/v3/rooms/{room_id}/timeline` | MSC2015 | **P0** | Sliding Sync核心 |
| `/_matrix/client/v3/rooms/{room_id}/sync` | r0.6.0 | **P0** | Room同步 |
| `/_matrix/client/v3/rooms/{room_id}/threads` | MSC3440 | **P1** | 线程支持 |
| `/_matrix/client/v3/rooms/{room_id}/keys/claim` | r0.6.0 | **P2** | E2EE密钥申领 |
| `/_matrix/client/v3/rooms/{room_id}/keys/forward` | r0.6.0 | **P2** | E2EE密钥转发 |
| `/_matrix/client/v3/rooms/{room_id}/retention` | r0.6.0 | **P2** | 消息保留 |
| `/_matrix/client/v3/rooms/{room_id}/report/{event_id}` | r1.1 | **P1** | 事件举报 |
| `/_matrix/client/v3/rooms/{room_id}/relations` | r0.6.0 | **P1** | 事件关系 |
| `/_matrix/client/v3/rooms/{room_id}/unread` | r0.6.0 | **P1** | 未读状态 |
| `/_matrix/client/v3/rooms/{room_id}/read_markers` | r0.6.0 | **P1** | 已读标记 |
| `/_matrix/client/v3/rooms/{room_id}/render` | r0.6.0 | **P2** | 消息渲染 |
| `/_matrix/client/v3/rooms/{room_id}/service_types` | Synapse特有 | **P3** | 服务类型 |
| `/_matrix/client/v3/rooms/{room_id}/vault` | Synapse特有 | **P3** | 保险库 |
| `/_matrix/client/v3/rooms/{room_id}/metadata` | Synapse特有 | **P3** | 元数据 |
| `/_matrix/client/v3/rooms/{room_id}/resolve` | Synapse特有 | **P3** | 冲突解决 |
| `/_matrix/client/v3/rooms/{room_id}/reduced` | Synapse特有 | **P3** | 简化视图 |
| `/_matrix/client/v3/rooms/{room_id}/encrypted` | r0.6.0 | **P2** | 加密事件 |
| `/_matrix/client/v3/rooms/{room_id}/external_ids` | r0.6.0 | **P3** | 外部ID |
| `/_matrix/client/v3/rooms/{room_id}/tags` | r0.6.0 | **P2** | 房间标签 |
| `/_matrix/client/v3/rooms/{room_id}/message_queue` | WebSocket | **P3** | 消息队列 |

#### E2EE密钥管理（约10个）

| 端点 | Matrix规范 | 实现必要性 | 说明 |
|------|-----------|-----------|------|
| `/_matrix/client/v3/keys/claim` | r0.6.0 | **P2** | 密钥申领 |
| `/_matrix/client/v3/keys/query` | r0.6.0 | **P2** | 密钥查询 |
| `/_matrix/client/v3/keys/upload` | r0.6.0 | **P2** | 密钥上传 |
| `/_matrix/client/v3/keys/signatures/upload` | r0.6.0 | **P2** | 签名上传 |
| `/_matrix/client/v3/keys/forward` | r0.6.0 | **P2** | 密钥转发 |
| `/_matrix/client/v3/rooms/{room_id}/keys/keys` | r0.6.0 | **P2** | 房间密钥 |
| `/_matrix/client/v3/rooms/{room_id}/keys/thread` | MSC3440 | **P2** | 线程密钥 |
| `/_matrix/client/v3/rooms/{room_id}/keys/sign` | r0.6.0 | **P2** | 密钥签名 |
| `/_matrix/client/v3/rooms/{room_id}/keys/verify` | r0.6.0 | **P2** | 密钥验证 |
| `/_matrix/client/v3/rooms/{room_id}/keys/perspective` | r0.6.0 | **P3** | 视角密钥 |

#### Admin API（约5个）

| 端点 | Matrix规范 | 实现必要性 | 说明 |
|------|-----------|-----------|------|
| `/_synapse/admin/v1/register` | Synapse特有 | **P2** | 管理员注册 |
| `/_synapse/admin/v1/rooms/{room_id}/event` | Synapse特有 | **P2** | 房间事件 |
| `/_synapse/admin/v1/rooms/{room_id}/aliases` | r0.6.0 | **P2** | 房间别名 |
| `/_synapse/admin/v1/federation/` | Synapse特有 | **P3** | 联邦重写 |

#### Thirdparty/SSO/Identity（约10个）

| 端点 | Matrix规范 | 实现必要性 | 说明 |
|------|-----------|-----------|------|
| `/_matrix/client/v1/sso` | r0.6.0 | **P2** | SSO认证 |
| `/_matrix/identity/v1/` | r0.6.0 | **P2** | 身份服务 |
| `/_matrix/client/v3/thirdparty/protocols` | r0.6.0 | **P2** | 第三方协议 |
| `/_matrix/client/v3/thirdparty/protocol/{protocol}` | r0.6.0 | **P2** | 特定协议 |
| `/_openid/userinfo` | OpenID Connect | **P2** | OpenID |

#### User/Profile/Device（约10个）

| 端点 | Matrix规范 | 实现必要性 | 说明 |
|------|-----------|-----------|------|
| `/_matrix/client/v3/profile/{user_id}` | r0.6.0 | **P1** | 用户资料(部分实现) |
| `/_matrix/client/v3/account_data/{type}` | r0.6.0 | **P1** | 账户数据 |
| `/_matrix/client/v3/user/{user_id}/filter` | r0.6.0 | **P2** | 用户过滤器 |
| `/_matrix/client/v3/presence/{user_id}/status` | r0.6.0 | **P1** | 在线状态 |
| `/_matrix/client/v3/devices/{device_id}` | r0.6.0 | **P1** | 设备管理 |
| `/_matrix/client/v3/user_directory` | r0.6.0 | **P2** | 用户目录 |
| `/_matrix/client/v3/user/appservice` | r0.6.0 | **P3** | AS用户 |
| `/_matrix/client/v3/push/rules/` | r0.6.0 | **P2** | 推送规则 |

#### Federation（约10个）

| 端点 | Matrix规范 | 实现必要性 | 说明 |
|------|-----------|-----------|------|
| `/_matrix/federation/v1/backfill` | r0.6.0 | **P0** | 联邦回填 |
| `/_matrix/federation/v1/state/` | r0.6.0 | **P1** | 联邦状态 |
| `/_matrix/federation/v1/user/devices/` | r0.6.0 | **P2** | 用户设备 |
| `/_matrix/federation/v1/keys/` | r0.6.0 | **P1** | 密钥查询 |
| `/_matrix/federation/v1/send/` | r0.6.0 | **P1** | 事件发送 |
| `/_matrix/federation/v1/invite/` | r0.6.0 | **P1** | 邀请发送 |

#### Search（约3个）

| 端点 | Matrix规范 | 实现必要性 | 说明 |
|------|-----------|-----------|------|
| `/_matrix/client/v3/search` | r0.6.0 | **P1** | 搜索功能(部分实现) |

#### Other（约8个）

| 端点 | Matrix规范 | 实现必要性 | 说明 |
|------|-----------|-----------|------|
| `/_matrix/client/v3/events` | r0.6.0 | **P2** | 事件流 |
| `/_matrix/client/v3/voip/turnServer` | r0.6.0 | **P2** | TURN服务 |
| `/_matrix/client/v3/rooms/{room_id}/client_config` | r0.6.0 | **P2** | 客户端配置 |
| `/_matrix/client/v1/evict` | Synapse特有 | **P3** | 驱逐用户 |
| `/_matrix/client/v3/rooms/{room_id}/aliases` | r0.6.0 | **P1** | 房间别名 |

---

## 三、已实现但测试脚本问题的端点详情

### 3.1 Room Receipts

**实际实现**: ✅ [room.rs:29](/Users/ljf/Desktop/hu/synapse-rust/src/web/routes/room.rs#L29)
```rust
"/rooms/{room_id}/receipt/{receipt_type}/{event_id}",
post(send_receipt),
```

**测试问题**: 测试使用GET而非POST

### 3.2 Room Redact

**实际实现**: ✅ [room.rs:63](/Users/ljf/Desktop/hu/synapse-rust/src/web/routes/room.rs#L63)
```rust
"/rooms/{room_id}/redact/{event_id}/{txn_id}",
post(redact_event),
```

**测试问题**: 测试路径或方法问题

### 3.3 Room Typing

**实际实现**: ✅ [typing.rs](/Users/ljf/Desktop/hu/synapse-rust/src/web/routes/typing.rs)
```rust
"/_matrix/client/v3/rooms/{room_id}/typing/{user_id}",
put(set_typing).get(get_user_typing),
```

**测试问题**: 扩展测试使用错误用户ID

### 3.4 Room Search

**实际实现**: ✅ [search.rs:26](/Users/ljf/Desktop/hu/synapse-rust/src/web/routes/handlers/search.rs#L26)
```rust
.route("/search_rooms", post(search_rooms)),
```

**测试问题**: 测试使用GET而非POST

---

## 四、优化方案

### 4.1 P0优先级（核心功能，必须实现）

| 端点 | 工作量 | 依赖 |
|------|--------|------|
| Room Timeline | 高 | Sliding Sync基础 |
| Room Sync | 高 | Sliding Sync基础 |
| Federation Backfill | 中 | 联邦通信 |

### 4.2 P1优先级（重要功能）

| 端点 | 工作量 | 说明 |
|------|--------|------|
| Room Threads | 中 | MSC3440支持 |
| Room Relations | 中 | 事件关系 |
| Room Read Markers | 低 | 已读标记 |
| Room Report | 低 | 事件举报 |
| Profile | 低 | 用户资料 |
| Presence | 中 | 在线状态 |
| Room Aliases | 低 | 房间别名 |
| Push Rules | 中 | 推送规则 |

### 4.3 P2优先级（E2EE和安全）

| 端点 | 工作量 | 说明 |
|------|--------|------|
| E2EE Keys | 高 | 完整密钥管理 |
| SSO/Identity | 高 | 认证集成 |
| Thirdparty | 中 | 第三方协议 |
| Device Management | 中 | 设备管理 |
| User Filter | 低 | 过滤器 |

### 4.4 P3优先级（可选功能）

| 端点 | 说明 |
|------|------|
| Room Vault | Synapse特有 |
| Room Metadata | Synapse特有 |
| User Appservice | AS专用 |
| Federation Rewrite | 联邦重写 |

---

## 五、实施建议

### 5.1 短期（1-2周）

1. **修复测试脚本问题** - 约25个端点可立即通过
2. **实现Room Timeline** - Sliding Sync核心
3. **实现Room Relations** - 事件关系支持

### 5.2 中期（1个月）

1. **实现Room Threads** - MSC3440
2. **实现E2EE Keys** - 完整密钥管理
3. **实现Profile/Presence** - 用户功能

### 5.3 长期（持续）

1. **Sliding Sync** - 完整同步方案
2. **Federation** - 联邦通信
3. **SSO/Identity** - 认证集成

---

## 六、参考原Synapse

根据 [element-hq/synapse](https://github.com/element-hq/synapse) 分析：

### 6.1 Synapse已实现的Room API

| 功能 | 状态 |
|------|------|
| Room State | ✅ 完全实现 |
| Room Messages | ✅ 完全实现 |
| Room Timeline | ✅ sliding_sync支持 |
| Room Sync | ✅ sliding_sync |
| Room Typing | ✅ 完全实现 |
| Room Receipt | ✅ 完全实现 |
| Room Redact | ✅ 完全实现 |
| Room Threads | ✅ MSC3440 |

### 6.2 synapse-rust当前状态

| 功能 | 状态 | 说明 |
|------|------|------|
| Room State | ✅ 已实现 | 完整 |
| Room Messages | ✅ 已实现 | 完整 |
| Room Typing | ✅ 已实现 | 需要测试修复 |
| Room Receipt | ✅ 已实现 | 需要测试修复 |
| Room Redact | ✅ 已实现 | 需要测试修复 |
| Room Timeline | ⚠️ 部分实现 | 测试问题 |
| Room Sync | ⚠️ 部分实现 | 需要完整实现 |
| Room Threads | ❌ 未实现 | 需要MSC3440 |

---

## 七、结论

1. **91个Missing中约25个已实现**，只是测试脚本问题
2. **约66个是真正未实现的功能**
3. **P0优先级**：Room Timeline、Room Sync、Federation Backfill
4. **建议先修复测试脚本**，然后逐步实现P1/P2优先级功能
