# 架构深化机会工程审查 — Round 2

> **日期**: 2026-07-14
> **分支**: `feat/architecture-optimization-round2`
> **审查对象**: C1 RoomContext 接口收缩、C2 SyncResponseBuilder 剩余 5/6、C4 CanonicalEvent 重复规范化、C5 ExtensionServices 抽屉

---

## C1 — RoomContext 接口收缩

### 现状

原始声称 `context.rs:60,66,79` 三个原始 storage 字段（`user_storage`/`room_storage`/`sticky_event_storage`）跨 web→storage 接缝泄漏。

**实际测量（当前代码）**:

| 字段 | 存在性 | Handler 直接引用次数 |
|------|--------|---------------------|
| `sticky_event_storage` | 已完全移除 | 0（代码库中已不存在） |
| `user_storage` | 存在于 SyncContext/AuthContext/AdminContext/FederationContext/MediaContext/SsoContext/FriendContext | **0**（全部 handler 已迁移到 service 层） |
| `room_storage` | 存在于 RoomContext(L74) + AdminContext(L424) | **1**（仅 `search/hierarchy.rs:19` 的 `ctx.room_storage.get_room(room_id)`） |

### 删除测试

| 测试 | 结果 |
|------|------|
| 删除 `user_storage` 从所有 Context | **move** — 无 handler 直接引用，仅 FromRef 构造使用，删除后复杂度集中在 AppState→Service 构造层 |
| 删除 `room_storage` 从 RoomContext | **concentrate** — 最后 1 个 handler (`search/hierarchy.rs:19`) 改为走 `ctx.room_service.state().get_room_record(room_id)`，web 层完全不再直接引用 RoomStoreApi |
| 删除 `room_storage` 从 AdminContext | 需另外分析（admin handler 使用量） |

### 接口体积

| | 修改前 | 修改后 |
|---|--------|--------|
| RoomContext 字段数 | 31 | 30（减 `room_storage`） |
| web 层直接 `use synapse_storage` 的 handler 数 | ~20（原始）→ 3（上轮审查）→ **1**（当前） | **0**（完成后） |

### Blast Radius

- `RoomContext` 结构体: 删除 1 个字段 + FromRef 中 1 行
- `search/hierarchy.rs:19`: 1 行改为走 RoomServiceApi（已有 `get_room_record` 方法）
- 编译时类型检查保证无遗漏

### 推荐: **DO**（低优先级）

杠杆已从「20 个 handler」降到「1 个 handler」，ROI 大幅降低但仍然为正。改动量约 3 行，blast radius 极小。最后一个直接引用 `ctx.room_storage` 的 handler 消除后，web 层彻底不再直接依赖 `synapse_storage::RoomStoreApi`，接口清洁度达到目标状态。

`user_storage` 的情况类似但分布在多个 Context（Sync/Auth/Admin/Federation/Media/Sso/Friend），handler 引用已清零，但从 Context 中移除需要逐个确认 admin handler 的间接使用。建议分步：先做 RoomContext 的 `room_storage`（1 handler），再做 SyncContext/AuthContext 等的 `user_storage`（0 handler，纯清理）。

---

## C2 — SyncResponseBuilder 剩余 5/6

### 现状

OPT-029 已将 `state_event_to_json` 提取到 `sync_helpers` 模块，sync_service 和 sliding_sync_service 共享。

剩余 5 类：`account_data`、`to_device`、`presence`、`device_lists`、`shared-rooms`。

### 实际测量

| 类别 | sync_service 实现 | sliding_sync_service 实现 | 是否重复 |
|------|------------------|--------------------------|---------|
| account_data | `response.rs` — `get_account_data_events()` + `get_room_account_data_events_batch()`，返回 Matrix sync v2 格式 `{type, content}` 事件数组 | `extensions.rs:38-47` — 直接调 `self.storage.get_global_account_data()` + `get_room_account_data()`，返回 MSC3575 格式 `{global, rooms}` 原始数据 | **否** — 不同 API 格式，不同存储路径 |
| to_device | `response.rs` — `get_to_device_events()`，返回事件数组 + stream_id | `extensions.rs:249-277` — `build_to_device_extension()`，支持 `since`/`limit` 分页，返回 `{events, next_batch}` MSC3575 格式 | **否** — 分页语义不同 |
| presence | `response.rs` — `get_presence_events()`，通过 `presence_storage` 查询 + sync filter 过滤 | `extensions.rs:149-186` — 手动构建 `m.presence` 事件 JSON，遍历 room members 收集 | **否** — 构建逻辑不同（sync 用 storage 查询，sliding sync 手动构建 JSON 模板） |
| device_lists | `response.rs` — `get_device_lists()` + `build_device_list_changes()` | `extensions.rs:195-247` — `build_e2ee_extension()`，含 cache 层、`shared_users` 追踪、`compute_left_shared_users()` | **否** — sliding sync 有额外的缓存和 shared_users 逻辑 |
| shared-rooms | **不存在** — sync v2 无此概念 | `extensions.rs:307-331` — `get_current_shared_users()` + `load_cached_shared_users()` + `compute_left_shared_users()` | **否** — 仅 sliding sync 独有功能 |

### 删除测试

| 测试 | 结果 |
|------|------|
| 将 5 类「共享」到一个公共模块 | **create complexity** — 需要抽象层同时支持 sync v2 和 MSC3575 两种 API 格式，引入额外的 trait/泛型/配置。当前两套代码各行其是更清晰 |
| 删除其中一套（如 sliding sync 的 account_data）改用 sync 的 | **break** — 两套 API 的 response shape 和分页语义不可互换 |

### 推荐: **SKIP**

这 5 类不是 copy-paste 重复。它们服务于两个不同的 API（Matrix sync v2 vs MSC3575 Sliding Sync），具有不同的 response shape、分页语义和过滤逻辑。`state_event_to_json` 之所以值得共享，是因为它做的是**完全相同的数据转换**（StateEvent → JSON）。而 account_data/to_device/presence 在两处的转换逻辑和输出格式都不同，强行统一需要引入不必要的抽象层。

---

## C4 — CanonicalEvent 重复规范化

### 现状

原始声称 `synapse-federation/src/signing.rs` 中 `hash(L75)` 与 `sign(L37)` 各自调 `canonical_json`，PDU 热路径重复。

### 实际测量

`sign_and_hash_event`（L188-218，PDU 热路径）:

```
1. compute_event_content_hash(event)           — L204
   └─ redact_event_for_hash(event)             — 按事件类型 redact content
   └─ 删除 hashes / signatures / unsigned      — L80-82
   └─ canonical_json(&redacted)                — L83 (FIRST canonicalization)
   └─ SHA256 hash

2. 插入 hashes 到 event                        — L205-210

3. CanonicalEvent::from_event(event)           — L213-214
   └─ event.clone()                            — 此时 event 已含 hashes，不含 signatures
   └─ 删除 signatures / unsigned
   └─ canonical_json(&stripped)                — L144 (SECOND canonicalization)
   └─ 缓存结果

4. sign_json_with_canonical(..., &canonical)   — L215
   └─ 使用步骤 3 缓存的 canonical_bytes()     — 无重复序列化
```

两次 `canonical_json` 调用操作在**不同版本的事件**上：
- 第一次：redacted content + 无 hashes + 无 signatures
- 第二次：完整 content + 有 hashes + 无 signatures

**这是两个不同的 canonical form，不能合并。**

### 删除测试

| 测试 | 结果 |
|------|------|
| 尝试用同一个 CanonicalEvent 同时做 hash 和 sign | **break** — content hash 的 canonical form 必须不含 hashes，而 sign 的 canonical form 必须含 hashes。Matrix spec 要求这两个 form 不同 |
| 缓存 hash canonical form 到 CanonicalEvent | **split concern** — CanonicalEvent 的职责是签名 canonical form（strip signatures/unsigned），hash canonical form 还要 strip hashes + redact content，是两个不同概念 |

### 推荐: **SKIP**

`sign_and_hash_event` 已经做了正确的优化：签名 canonical form 缓存在 `CanonicalEvent` 中，避免 `sign_json_with_canonical` 内部重复规范化。两次 `canonical_json` 调用是必要的，因为它们操作在不同的事件子集上。没有重复工作可消除。

`CanonicalEvent` 结构体本身的设计是正确的：只缓存签名 canonical form（strip signatures + unsigned），不混淆 hash canonical form（strip hashes + signatures + unsigned + redact content）。

---

## C5 — ExtensionServices 抽屉

### 现状

`synapse-services/src/wiring/extensions.rs:12-43`，17 个字段按 `#[cfg]` 分组：

```
Always-on (8):
  rtc_domain_service, directory_service, media_domain_service,
  identity_service, translation_service, uia_service,
  user_lock_service, user_service

Conditional (9):
  #[cfg(feature = "voice-extended")]     voice_service
  #[cfg(feature = "friends")]            friend_storage, friend_room_service, friend_federation
  #[cfg(feature = "openclaw-routes")]    ai_connection_storage
  #[cfg(feature = "server-notifications")] server_notification_storage, server_notification_service
  #[cfg(feature = "privacy-ext")]        privacy_storage
  #[cfg(feature = "widgets")]            widget_storage, widget_service
  #[cfg(feature = "burn-after-read")]    burn_after_read
```

### 变化复核（vs 上轮审查）

上轮审查判定"投机性分组"——ExtensionServices 是 catch-all 抽屉。复核发现：

1. **分类合理**: 8 个 always-on 服务确实是 "extension"（非核心房间/账户/联邦），放在 core/rooms/accounts 都不合适
2. **feature-gate 紧凑**: 9 个条件字段的 `#[cfg]` 天然形成了分组——同一 feature 的 storage + service 放在一起
3. **无新增投机项**: 自上次审查以来，未向 ExtensionServices 添加与现有分组逻辑不一致的字段
4. **size 可控**: 17 字段在单个 struct 中是可以接受的（对比 AdminContext 的 60+ 字段）

### 删除测试

| 测试 | 结果 |
|------|------|
| 删除 ExtensionServices，inline 到 container.rs | **move** — 复杂度从 extensions.rs 移到 container.rs。container.rs 已被拆成 wiring/*.rs，加回去是倒退 |
| 按 feature 拆成多个 wiring 文件 | **scatter** — 每个 feature 一个 wiring 文件（voice_extensions.rs, friend_extensions.rs...），每个只有 1-3 个字段。增加导航成本，不减少认知负荷 |
| 保持现状 | **stable** — 当前分组清晰，增量可控 |

### 推荐: **SKIP**

ExtensionServices 抽屉已经达到了合理的设计平衡点：17 个字段按 `#[cfg]` 自然分组，always-on 和 conditional 分明。进一步拆分（按 feature 拆 wiring 文件）会导致过度碎片化——每个文件只有几个字段的构造逻辑。保持现状是最佳选择。

上轮审查的"投机性"判定在当前代码状态下不再成立——新增字段都遵循了 `#[cfg(feature)]` 分组模式，没有出现随意往抽屉里扔东西的退化。

---

## 汇总

| 候选 | 删除测试 | 推荐 | 理由 |
|------|---------|------|------|
| C1 RoomContext | concentrate | **DO**（低优先级） | `sticky_event_storage` 已移除；`user_storage` handler 引用归零；`room_storage` 仅剩 1 个 handler 引用。3 行改动消除最后的 web→storage 直接依赖 |
| C2 SyncResponseBuilder 5/6 | create complexity | **SKIP** | 5 类不是代码重复而是不同 API 的不同实现。强行共享需要引入不必要的抽象层 |
| C4 CanonicalEvent | break | **SKIP** | 两次 canonical_json 操作在不同事件子集上（hash canonical form vs sign canonical form），均属必要，无重复可消除 |
| C5 ExtensionServices | stable | **SKIP** | 17 字段按 feature gate 自然分组，结构清晰。进一步拆分会造成碎片化 |
