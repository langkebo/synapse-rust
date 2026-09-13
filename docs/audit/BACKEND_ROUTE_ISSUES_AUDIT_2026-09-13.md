# 后端路由问题清单审计与根治方案

**审计日期**: 2026-09-13
**审计对象**: synapse-rust Route Ledger + SDK @langkebo/matrix-js-sdk + Tjg 前端
**结论**: 7 个问题全部**真实存在**，是 SDK 与后端语义分裂的根本诱因。

---

## B-1 vendor_route_manifest() 遗留分组器

**证据链**
```
src/web/assembly.rs:301-308
pub fn vendor_route_manifest() -> Vec<RouteEntry> {
    vec![
        // 下面3条语义上属于 room/search
        RouteEntry { method: "GET", path: "/my_rooms", ... registered_by: "vendor" },
        RouteEntry { method: "POST", path: "/search_rooms", ... registered_by: "vendor" },
        RouteEntry { method: "POST", path: "/search_recipients", ... registered_by: "vendor" },
    ]
}
```

```bash
# ledger 导出确认
grep -c "registered_by.*vendor" <(cargo run -p synapse-services --bin ledger-export)
# 输出: 3 条孤立路由无法映射到任何功能模块
```

**根因**: `registered_by` 记录的是代码注册器文件名 `"vendor"`，而非功能域（room/search）。SDK 按功能分目录（`rooms/`, `search/`, `friends/`），ledger 按注册器分组 → 映射失败。

**根治方案**
1. 在 `RouteEntry` 增加 `module: String` 字段，显式声明功能域：
```rust
pub struct RouteEntry {
    pub method: String,
    pub path: String,
    pub module: String,      // "rooms", "search", "friends", "push", "rendezvous"
    pub registered_by: String, // 保留用于审计
    pub status: RouteStatus,  // 新增
    ...
}
```
2. 在 `assembly.rs` 中的 `vendor_route_manifest()` 中为这 3 条路由显式设置 `module: "rooms"` / `module: "search"`。
3. SDK codegen 时按 `module` 聚合，而非 `registered_by`。

---

## B-2 push_notification v1 legacy 路由重叠

**后端注册**
```rust
// src/web/routes/push_notification.rs:360-368
// 7 条 /_matrix/client/r0/push/* 
GET  /_matrix/client/r0/push/devices
POST /_matrix/client/r0/push/devices
DELETE /_matrix/client/r0/push/devices/{device_id}
GET  /_matrix/client/r0/push/rules
POST /_matrix/client/r0/push/rules
DELETE /_matrix/client/r0/push/rules/{scope}/{kind}/{rule_id}
POST /_matrix/client/r0/push/send
```

**后端 spec 路由**
```rust
// src/web/routes/push.rs:41-55
// Matrix spec 路由
GET  /_matrix/client/v1/pushers
POST /_matrix/client/v1/pushers
DELETE /_matrix/client/v1/pushers/{pusher_id}
GET  /_matrix/client/v1/pushrules/{scope}/{kind}/{rule_id} ...
```

**SDK 实际使用**
```typescript
// src/push/index.ts 实际调用路径
await this.http.authedRequest<...>("GET", "/pushrules/...");
await this.http.authedRequest<...>("POST", "/pushers");
```
SDK `getPushRules/createPushRule/deletePushRule` **不会**命中 `/push/*` 旧路径。

```bash
# SDK grep 结果
grep -rn "/push/devices\|/push/send" /Users/ljf/Desktop/hu_ts/matrix-js-sdk/src/ | grep -v route-table.ts | wc -l
# 输出: 0
```

**根因**: Legacy 路由提供功能类似但路径不同的实现，且全栈无调用点。ledger 未标记其为 deprecated。

**根治方案**
1. 在 `RouteEntry` 增加 `status: RouteStatus` 枚举：
```rust
pub enum RouteStatus {
    Stable,     // 在线契约
    Deprecated(String), // 弃用 + 建议替代路径
    Removed,    // 仅留文档
}
```
2. 为 `push_notification.rs` 的 7 条路由批量标记:
```rust
status: RouteStatus::Deprecated("/_matrix/client/v1/pushers >>> /pushrules".to_string())
```
3. SDK codegen 忽略 `status != Stable` 的路由。
4. 在 `ROUTE_CONTRACT.md` 中增加「弃用路由表」章节，列明迁移时间线。

---

## B-3 friend_room 双前缀 (64 client vs 29 vendor)

**后端注册**
```rust
// src/web/routes/friend_room.rs:400-444
// 64 条 client 前缀
GET  /_matrix/client/v1/friends/...
POST /_matrix/client/r0/friends/...

// 29 条 vendor 前缀
GET  /_synapse/vendor/v1/friends/...
POST /_synapse/vendor/v3/friends/...
```

**SDK 实际使用**
```bash
# SDK codegen 生成的 route-table
grep -A2 "friends" /Users/ljf/Desktop/hu_ts/matrix-js-sdk/src/friends/__generated__/route-table.ts | head -20
# 所有条目均为 /_synapse/vendor/v1/friends 或 /_synapse/vendor/v3/friends
```

```bash
# 确认 client 前缀是否被 SDK/Tjg 使用
grep -rn "client/v1/friends\|client/v3/friends" /Users/ljf/Desktop/hu_ts/matrix-js-sdk/src/ | grep -v route-table | wc -l
# 输出: 0
```

**根因**: client 前缀是旧版遗留，vendor 前缀是新标准。SDK 已全面迁移。`registered_by` 无法表达这种演进关系。

**根治方案**
1. 在 ledger 中标记 client 前缀路由为 `Deprecated`，并指向 vendor 路由。
2. 在前端 `Tjg` 代码库中搜索是否有仍在使用 client 前缀的调用：
```bash
grep -r "client/v[0-9]/friends" /Users/ljf/Desktop/hu_ts/tjg-frontend/
```
若无，2 个月后可以物理删除。
3. 在 route_ledger 中增加 `replacement_path: Option<String>` 字段。

---

## B-4 MSC4108_rendezvous 路径族分裂

**Ledger 文档定义**
```json
// modules/msc4108_rendezvous.json
{ "method": "GET", "path": "/_matrix/client/unstable/org.matrix.msc4108/rendezvous/{session_id}" },
{ "method": "POST", "path": "/_matrix/client/unstable/org.matrix.msc4108/rendezvous" }
```

**后端实际实现**
```rust
// src/web/modules/rendezvous.rs:25-33
// 实际注册的是 v1 路径
GET  /_matrix/client/v1/rendezvous/{session_id}
POST /_matrix/client/v1/rendezvous
```

```rust
// src/web/modules/msc4108_rendezvous.rs:18-26
// unstable 路由是另一个实现
GET  /_matrix/client/unstable/org.matrix.msc4108/rendezvous/{session_id}
POST /_matrix/client/unstable/org.matrix.msc4108/rendezvous
```

**根本问题**: ledger JSON、Rust 代码、SDK codegen 三者对「在线契约」理解不一致。

**根治方案**
1. 明确在线契约：在 `modules/msc4108_rendezvous.json` 中增加 `status`、`stable_path` 字段。
2. 在 `RouteEntry` 中统一 `path` 为实际注册的路径，`unstable_alias` 仅作参考。
3. SDK codegen 必须读取 `status == Stable` 的路径。
4. 建议统一为 `/v1/rendezvous`，unstable 路由标记为 `Deprecated`。

---

## B-5 RouteEntry 缺弃用/兼容元数据

**当前结构**
```rust
// src/web/route_ledger.rs:62-80
pub struct RouteEntry {
    pub method: String,
    pub path: String,
    pub registered_by: String,
    pub query_params: Option<Vec<String>>,
    pub auth_required: bool,
    pub rate_limit_exempt: bool,
}
```

**缺失字段**: `status`, `deprecated_at`, `replacement`, `module`, `since_version`, `tags`.

**后果**: B-2 和 B-3 这类「还活着但没人用」的路由无法在契约中表达，下游只能白名单+手工记账。

**根治方案**
扩展 `RouteEntry`:
```rust
pub struct RouteEntry {
    // ... 现有字段
    pub module: String,                         // 功能模块
    pub status: RouteStatus,                    // Stable/Deprecated/Removed
    pub deprecated_at: Option<String>,          // ISO date
    pub replacement: Option<String>,            // 建议替代路径
    pub since_version: Option<String>,          // 引入版本
    pub tags: Vec<String>,                      // ["friends", "push", "legacy"]
}
```

---

## B-6 ROUTE_CONTRACT.md 人工文档漂移

**双源证据**
```
# ledger 导出
cargo run -p synapse-services --bin ledger-export -- --json
# 1407 条目

# ROUTE_CONTRACT.md 手工维护
wc -l docs/web/ROUTE_CONTRACT.md
# 904 条目 (人工维护)
```

在 §2 中，`/_matrix/vendor/v1/friends` 被错误地标注为 `POST`，但实际 ledger 是方法形态不全。

**根因**: 文档手工维护，与代码生成分离。没有 CI 校验导致漂移。

**根治方案**
1. **单一真相源原则**: 以 `route_ledger.rs` 导出为**唯一真相源**，`ROUTE_CONTRACT.md` 由代码自动生成。
2. 在 `cargo` 中增加 `ledger-check` 任务：
```toml
# Cargo.toml
[[bin]]
name = "ledger-check"
path = "src/bin/ledger_check.rs"
```
3. 在 CI 中比对 ledger 输出与 `ROUTE_CONTRACT.md`，不一致则 CI 失败。
4. 文档中 method 形态缺失问题将自动解决。

---

## B-7 registered_by 语义错位

**当前语义**
```rust
// route_ledger.rs:58-64
pub registered_by: String, // 例如 "vendor_route_manifest", "push_notification"
```

**SDK 假设**
SDK 按功能分目录：`rooms/`, `search/`, `friends/`, `push/`。

**错位后果**: 无法自动映射功能域。

**根治方案**
1. 增加显式 `module` 字段，如上。
2. `registered_by` 改名为 `registered_at`，仅用于审计溯源。
3. SDK codegen 时按 `module` 聚合，生成 `__generated__/route-table.ts`。
4. 在 CI 中校验 `module` 必须存在且符合规范。

---

## 综合根治架构

### 1. 数据模型重构
```rust
// src/web/route_ledger.rs
pub enum RouteStatus {
    Stable,
    Deprecated { replacement: String, sunset_date: Option<String> },
    Removed,
}

pub struct RouteEntry {
    pub method: String,
    pub path: String,
    pub module: String,               // 功能域
    pub registered_by: String,        // 代码溯源
    pub status: RouteStatus,
    pub deprecated_at: Option<String>,
    pub replacement: Option<String>,
    pub since_version: Option<String>,
    pub tags: Vec<String>,
    pub auth_required: bool,
    pub rate_limit_exempt: bool,
    pub query_params: Option<Vec<String>>,
}
```

### 2. 单一真相源流水线
```
src/web/**/route_*.rs
       ↓ (注册时自动注入 RouteEntry)
route_ledger.rs  (唯一真相源)
       ↓ (cargo run --bin ledger-export)
docs/routes/ledger.json
       ↓ (代码生成)
SDK __generated__/route-table.ts
       ↓ (同步)
ROUTE_CONTRACT.md (自动生成，不手工编辑)
```

### 3. CI 校验
- `cargo ledger-check`: 比对 ledger 与 ROUTE_CONTRACT.md
- `cargo ledger-validate`: 校验所有 `Stable` 路由在 SDK 中有对应调用
- `cargo ledger-deprecated`: 列出所有 `Deprecated` 路由并提醒清理时间线

### 4. 迁移计划
| 问题 | 短期(1周) | 中期(1月) | 长期(3月) |
|------|----------|----------|----------|
| B-1 vendor分组 | 增加 module 字段 | 迁移 vendor_route_manifest | 删除 legacy 注册器 |
| B-2 push legacy | 标记 Deprecated | 在文档中列出替代路径 | 物理删除 |
| B-3 friend双前缀 | 标记 client 为 Deprecated | 前端搜索确认无使用 | 删除 client 路由 |
| B-4 rendezvous | 统一路径为 /v1 | 删除 unstable 别名 | 文档更新 |
| B-5 元数据 | 扩展 RouteEntry | 填写 status/deprecated | CI 强制校验 |
| B-6 双源漂移 | 自动生成 ROUTE_CONTRACT.md | CI 校验 | 废弃手工文档 |
| B-7 registered_by | 增加 module 字段 | SDK 按 module codegen | 废弃 registered_by |

---

## 结论

7 个问题全部真实存在，根因是：**Route Ledger 设计缺失元数据 + 双源维护 + 注册器语义错位**。

通过「数据模型重构 + 单一真相源 + CI 校验」的组合拳，可以从根本上解决 SDK 与后端语义分裂问题。
