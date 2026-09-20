# 后端路由问题清单审计与根治方案（修订版）

**审计日期**: 2026-09-13  
**修订版本**: v2（基于 codegen 源码与实际 manager 调用点深度核查）  
**审计对象**: synapse-rust Route Ledger + SDK @langkebo/matrix-js-sdk + Tjg 前端  
**核心发现**: 7 个问题真实存在，但 B-3/B-4 原始审计结论有误，已修正。

---

## 关键修正说明

### 1. SDK route-table **不代表实际调用**
SDK codegen 使用「既有条目 ∪ ledger 清单 ∪ ROUTE_CONTRACT.md」策略，route-table 中的条目只增不减，累积了大量死账。**必须检查 manager 源码实际调用的路径前缀**。

### 2. B-3 friend 双前缀：client 前缀是死账
- Ledger: client/v1 29 + client/r0 28 + client/v3 7 = **64 条 client** vs vendor/v1 29 条
- SDK route-table: 93 条（含全部四个前缀）
- **实际调用**：`src/friend/paths.ts` 明确声明 `src/friend/**` 全部通过 `VendorPrefix` 发送请求，使用 `friendPath()` 类型约束。
- **结论**：64 条 client 前缀是 codegen 累积死账，**非实际调用**。

### 3. B-4 rendezvous 路径族：SDK 实际使用 unstable
- Ledger: unstable 4 条 (`msc4108_rendezvous`), v1 6 条 (`rendezvous`)
- SDK 实际调用：`src/rendezvous/transports/MSC4108RendezvousSession.ts:106`
  ```typescript
  getUrl("/org.matrix.msc4108/rendezvous", undefined, ClientPrefix.Unstable)
  ```
- **结论**：SDK **使用 unstable** 而非 v1。原始审计假设 SDK 已迁移到 v1 是错误的。

---

## B-1 vendor_route_manifest() 遗留分组器 ✅ 确认

**证据链**
```
src/web/routes/assembly.rs:301-308
fn vendor_route_manifest() -> Vec<RouteEntry> {
    expand_under_prefixes(
        "vendor",
        &["/_matrix/vendor/v1"],
        &[(Method::GET, "/my_rooms"), (Method::POST, "/search_rooms"), (Method::POST, "/search_recipients")],
    )
}
```
Ledger 确认：**3 条孤立路由**，`registered_by="vendor"`，功能上属于 room/search 模块但被归为 vendor。

**根因**：`registered_by` 记录的是注册器文件名 `"vendor"`，而非功能域（room/search）。SDK 按功能分目录（`rooms/`, `search/`, `friends/`），ledger 按注册器分组 → 映射失败。

**根治方案**
1. 在 `RouteEntry` 增加 `module: String` 字段，显式声明功能域：
   ```rust
   pub struct RouteEntry {
       pub method: Method,
       pub path: &'static str,
       pub module: String,          // "rooms", "search", "friends", "push", "rendezvous"
       pub registered_by: &'static str,
       pub status: RouteStatus,
       pub query_params: &'static [&'static str],
       pub auth: Option<&'static str>,
       pub rate_limit_exempt: bool,
   }
   ```
2. 在 `vendor_route_manifest()` 中为这 3 条路由显式设置 `module: "search"` / `module: "rooms"`。
3. SDK codegen 时按 `module` 聚合，而非 `registered_by`。

---

## B-2 push_notification legacy 重叠 ✅ 确认（修正细节）

**Ledger 数据**
```
r0/push/* (7 条，push_notification):
  GET     /_matrix/client/r0/push/devices
  POST    /_matrix/client/r0/push/devices
  DELETE  /_matrix/client/r0/push/devices/{device_id}
  GET     /_matrix/client/r0/push/rules
  POST    /_matrix/client/r0/push/rules
  DELETE  /_matrix/client/r0/push/rules/{scope}/{kind}/{rule_id}
  POST    /_matrix/client/r0/push/send

spec-compliant pushers/pushrules (28 条，push):
  r0/v3 前缀的 pushers/pushrules
```

**SDK 实际调用**
- `src/notifications/index.ts`: 只调用 `/notifications` 和 `/notifications/{id}/ack`，**从不碰 `r0/push/*`**
- `src/push/index.ts`: 调用 `pushers/pushrules` 系列路径
- **结论**：7 条 legacy 是死账，**未被任何 manager 使用**。

**根因**：legacy 路由提供类似功能但路径不同的实现，且全栈无调用点。ledger 未标记其为 deprecated。codegen 的「既有条目 ∪」策略让 route-table 累积死账。

**根治方案**
1. 在 `RouteEntry` 增加 `status: RouteStatus` 枚举：
   ```rust
   pub enum RouteStatus {
       Stable,
       Deprecated { replacement: &'static str, sunset_at: Option<&'static str> },
       Removed,
   }
   ```
2. 为 `push_notification.rs` 的 7 条路由标记 `Deprecated`：
   ```rust
   status: RouteStatus::Deprecated {
       replacement: "/_matrix/client/v3/pushers",
       sunset_at: None,
   }
   ```
3. SDK codegen 时 `status != Stable` 的路由应被标记但不一定删除（保留向后兼容），但 CI 应该警告。
4. 在 CI 中添加 `ledger-deprecated` 任务，列出所有 deprecated 路由并追踪清理时间线。

---

## B-3 friend_room 双前缀 ✅ 确认（**严重修正**）

**Ledger 数据**
- client/v1: 29 条
- client/r0: 28 条
- client/v3: 7 条
- vendor/v1: 29 条

**原始审计错误**："SDK 只引用 vendor 前缀" —— **完全错误**。

**真相**：
- SDK route-table: **93 条**（client/v1+r0+v3=64 + vendor/v1=29）
- 但 `src/friend/paths.ts` 明确声明：
  ```typescript
  /** 相对 `/_matrix/vendor/v1` 的 friends 路径（必须是 ledger 声明过的形态）。 */
  export type FriendRelativePath = StripVendor<FriendPathPattern>;
  export function friendPath<P extends FriendRelativePath>(path: P): P { return path; }
  ```
- **实际调用**：所有 `src/friend/**` 代码通过 `VendorPrefix` 发送请求。
- **结论**：64 条 client 前缀是 codegen 「既有条目」累积的死账，**非实际调用**。

**根因**：
1. SDK codegen 采用「既有条目 ∪」策略，route-table 只增不减
2. 后端未标记 client 前缀为 deprecated
3. codegen 注释说 "保留向后兼容"，但实际并未有向后兼容的需求

**根治方案**
1. 后端：标记 client 前缀路由为 `Deprecated`，并指向 vendor 前缀路径
2. 后端：在 `friend_room.rs` 的 `create_router` 中添加 `deprecated_since="2026-Q1"` 注释
3. SDK codegen: 将 `status == Deprecated` 的路由标记为 `@deprecated` JSDoc，但保留在 route-table 中
4. CI 添加 `ledger-unused` 任务：对比 ledger 声明 vs SDK manager 实际调用（通过 AST 分析 `request()` 字面路径），报告 unused routes
5. 3 个月后若无报警，删除后端 client 前缀路由

---

## B-4 MSC4108_rendezvous 路径族分裂 ✅ 确认（**结论修正**）

**Ledger 数据**
- `unstable/org.matrix.msc4108/rendezvous/*` (4 条，by `msc4108_rendezvous`)
- `/v1/rendezvous/*` (6 条，by `rendezvous`)

**原始审计错误**："SDK codegen 必须读取 `status == Stable` 的路径，建议统一为 `/v1/rendezvous`" —— **SDK 实际用的是 unstable！**

**真相**：
```typescript
// src/rendezvous/transports/MSC4108RendezvousSession.ts:106
this.client.getUrl("/org.matrix.msc4108/rendezvous", undefined, ClientPrefix.Unstable)
```
SDK **直接使用 unstable** 而非 v1。

**根因**：MSC4108 规范尚未稳定，后端同时提供 unstable（MSC 特定路径）和 v1（通用路径）两条路线。SDK 选择了 unstable。

**根治方案**
1. 后端：明确 MSC4108 路径族的在线契约优先级：
   - **MSC4108 特有路径** (`unstable/org.matrix.msc4108`) → 用于实验性功能
   - **通用路径** (`v1/rendezvous`) → 用于标准功能
   - 两者可以并存，但需要在 `RouteEntry.status` 中标注差异
2. SDK codegen: 按 `module` + `tags=["msc4108"]` 区分，允许 TypeScript 层面标注 `@experimental`
3. CI 校验：对于 `unstable` 路径必须带有 `mscXXXX` 标签，否则拒绝合并

---

## B-5 RouteEntry 缺元数据 ✅ 确认

**当前结构**
```rust
pub struct RouteEntry {
    pub method: Method,
    pub path: &'static str,
    pub registered_by: &'static str,
    pub query_params: &'static [&'static str],
    pub auth: Option<&'static str>,
    pub rate_limit_exempt: bool,
}
```

**缺失字段**：`module`, `status`, `since_version`, `tags`, `replacement`, `deprecated_at`。

**根治方案**（见下方「综合根治架构」）

---

## B-6 ROUTE_CONTRACT.md 人工文档漂移 ✅ 确认

**证据**：
- `cargo ledger-export` → 1295 条（ledger 实际）
- `ROUTE_CONTRACT.md` → ~900 条（手工维护）
- codegen 脚本 `sdk-contract-codegen.mjs` 同时读取 ROUTE_CONTRACT.md 和 ledger 作为补充

**根因**：文档手工维护，与代码生成分离。没有 CI 强制对齐。

**根治方案**
1. **单一真相源原则**：以 `route_ledger.rs` 导出为**唯一真相源**，`ROUTE_CONTRACT.md` 由代码自动生成
2. 在 CI 中增加 `ledger-sync-check`：比对 ledger 输出与 `ROUTE_CONTRACT.md`，不一致则 CI 失败
3. codegen 脚本调整为仅从 ledger JSON 读取，移除 ROUTE_CONTRACT.md 依赖

---

## B-7 registered_by 语义错位 ✅ 确认

**当前语义**
```rust
pub registered_by: &'static str, // 例如 "vendor_route_manifest", "push_notification"
```

**SDK 假设**：SDK 按功能分目录：`rooms/`, `search/`, `friends/`, `push/`。

**错位后果**：无法自动映射功能域。

**根治方案**
1. 增加显式 `module` 字段（见 B-1）
2. `registered_by` 改名为 `registered_at`（或保留原名但在文档中重新定义语义为"代码溯源"）
3. SDK codegen 按 `module` 聚合，生成 `__generated__/route-table.ts`
4. CI 校验：`module` 必须存在且符合规范

---

## 综合根治架构

### 1. 数据模型重构（优先级 P0）

```rust
// src/web/routes/route_ledger.rs

/// 路由生命周期状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteStatus {
    /// 当前活跃的生产契约
    Stable,
    /// 已弃用但仍可用（标记替换路径和时间线）
    Deprecated {
        replacement: &'static str,
        sunset_at: Option<&'static str>, // ISO date or "2026-Q4"
    },
    /// 已移除，仅留文档
    Removed,
}

pub struct RouteEntry {
    pub method: Method,
    pub path: &'static str,
    
    // ========== 新增字段 ==========
    pub module: &'static str,        // 功能域：rooms, search, friends, push, rendezvous
    pub status: RouteStatus,         // 路由生命周期
    pub since_version: Option<&'static str>, // 引入版本
    pub tags: &'static [&'static str],      // ["msc4108", "experimental", "legacy"]
    
    // 保留现有字段
    pub registered_by: &'static str,
    pub query_params: &'static [&'static str],
    pub auth: Option<&'static str>,
    pub rate_limit_exempt: bool,
}
```

### 2. 单一真相源流水线

```
src/web/**/route_*.rs (register_router + RouteEntry { module: "...", ... })
       ↓ (启动时自动注入 + RouteLedger::validate)
route_ledger.rs  (唯一真相源)
       ↓ (cargo run -p synapse-services --bin ledger-export)
artifacts/route_contract.json (或 docs/api-contract/ledger.json)
       ↓ (SDK 端 SDK codegen 读取此 JSON)
SDK __generated__/route-table.ts
       ↓ (可选)
ROUTE_CONTRACT.md (由 ledger JSON 自动生成，不手工编辑)
```

### 3. CI 校验矩阵

```yaml
# .github/workflows/route-ledger.yml
jobs:
  ledger-validate:
    # 校验所有 RouteEntry.module 存在且符合规范
    # 校验所有 Stable 路由在 SDK 中有对应调用（AST 扫描）
    # 校验所有 Deprecated 路由有 replacement 字段
  
  ledger-sync-check:
    # 比对 ledger JSON 与 ROUTE_CONTRACT.md（如果启用自动生成）
    # 如果不一致，CI 失败
  
  ledger-deprecate-tracker:
    # 列出所有 Deprecated 路由
    # 检查 sunset_at 是否过期
    # 生成报表发送至 #api-deprecation 频道
  
  ledger-unused-detector:
    # AST 扫描 SDK manager 实际调用的路径
    # 对比 ledger 声明，找出 unused routes
    # 告警提示可能可以删除
```

### 4. 迁移时间表

| 问题 | 短期 (1 周) | 中期 (1 月) | 长期 (3 月) |
|------|-----------|-----------|-----------|
| B-1 vendor 分组 | 增加 `module` 字段，在 `vendor_route_manifest()` 中设置 | 迁移 3 条路由到 `search.rs`/`rooms.rs` | 删除 `vendor_route_manifest()` |
| B-2 push legacy | 标记 7 条 `Deprecated` | SDK codegen 标注 `@deprecated` | 物理删除，若无人报警 |
| B-3 friend 双前缀 | 标记 client 前缀 `Deprecated` | CI 加入 `unused-detector` 验证 | 删除 client 路由 |
| B-4 rendezvous | 在 ledger 中标注 `tags=["msc4108"]` | 明确不稳定路径优先级文档 | 若 MSC 稳定，统一到 v1 |
| B-5 元数据缺失 | 扩展 `RouteEntry` + `RouteStatus` | 补全所有路由的 `status`/`module` | CI 强制校验必填字段 |
| B-6 文档漂移 | 自动生成 `ROUTE_CONTRACT.md` | CI `ledger-sync-check` | 废弃手工文档 |
| B-7 registered_by | 增加 `module` 字段 | SDK 按 `module` codegen | 文档重新定义 `registered_by` |

---

## 待执行任务清单

### Phase 1: 数据模型重构（1-2 天）
- [ ] 在 `route_ledger.rs` 中定义 `RouteStatus` enum
- [ ] 扩展 `RouteEntry` 结构体，新增 `module`, `status`, `tags`, `since_version`
- [ ] 修改 `expand_under_prefixes()` 签名以支持 `module` 参数
- [ ] 更新所有现有的 `RouteEntry::new()` 调用点

### Phase 2: 存量路由标注（2-3 天）
- [ ] `push_notification.rs`: 标记 7 条 legacy 路由为 `Deprecated`
- [ ] `friend_room.rs`: 标记 client 前缀为 `Deprecated`，vendor 为 `Stable`
- [ ] `vendor_route_manifest()`: 设置 `module: "search"`
- [ ] `msc4108_rendezvous.rs`: 设置 `tags=["msc4108", "experimental"]`

### Phase 3: SDK codegen 改造（1-2 天）
- [ ] 修改 `sdk-contract-codegen.mjs`: 增加对 `module` 字段的解析
- [ ] 修改 `sdk-contract-codegen.mjs`: 根据 `status` 标注 `@deprecated`
- [ ] 调整 route-table 输出格式，在 header 中反映 `module` 和 `status`

### Phase 4: CI 门禁建设（1 周）
- [ ] 编写 `ledger-check` CLI 工具（对比 ledger vs 文档）
- [ ] 编写 `ledger-unused-detector` (AST 扫描 SDK)
- [ ] 在 `.github/workflows` 中集成
- [ ] 设置告警通道

### Phase 5: 渐进式清理（3 个月）
- [ ] 每月检查 `deprecated-tracker` 报表
- [ ] 若 `Deprecated` 路由无人报警，安排删除
- [ ] 每季回顾 `unused-detector` 输出，清理 dead code

---

## 结论

**根本原因**：Route Ledger 设计缺失元数据 + 双源维护 + 注册器语义错位 + SDK codegen 累积策略。

**根治路径**：
1. 数据模型增强（`module`, `status`, `tags`）→ 使语义显式化
2. 单一真相源（ledger → artifacts → SDK）→ 消除文档漂移
3. CI 门禁矩阵（validate + unused + deprecate tracker）→ 防止回归

**预期收益**：
- SDK 与后端语义一致性提升（module 聚合）
- 路由生命周期可视化（status）
- 实验性/稳定性区分（tags）
- Dead code 自动发现（unused-detector）

---

**修订人**: glm-5.3 (via Audit Correction Turn)
**修订日期**: 2026-09-13 16:45
**修订内容**:
- B-3: 推翻「SDK 只引用 vendor」结论，确认 64 条 client 前缀是 codegen 死账，实际全走 vendor
- B-4: 推翻「SDK 已迁移到 v1」结论，确认 SDK 实际使用 unstable 路径
- 新增「核心修正说明」章节，解释 route-table 不代表实际调用的判定逻辑
