# 当前仍存在的问题列表（基于 docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md §22.3）

> **更新日期**: 2026-09-27（合并 U-2、U-13-R9、U-20、U-19-R4 分支）
> **基线**: `opt/consolidated` @ `5d1bfdc3f`
> **状态说明**: ✅ = 已解决；❌ = 仍存代码缺陷，需修复

---

## 已解决的问题

### ✅ 1. U-2：`active_user_exists` 语义澄清

- **位置**: 合并自 `bb69ad7a0 fix(user): split user_exists into existence vs active-only predicates`
- **实现**: 将 `user_exists` 拆分为：
  - `user_exists`（包含停用用户）
  - `active_user_exists`（排除停用用户）
- **影响**: `/register/available` 使用 `active_user_exists`（停用用户名返回 "不可用"），其它端点使用 `user_exists`

### ✅ 2. U-13-R9：v≤11 写路径持久化图字段

- **位置**: 合并自 `11bf5455d fix(room): persist depth/prev_events/auth_events on the v<=11 local write path`
- **实现**: 
  - 修改 `graph_metadata.rs:346-348`：事务路径现调用 `resolver.resolve()` + `create_event_with_graph`
  - 更新 `room/messaging/events.rs`: v≤11 路径显式传递图字段
  - 补充 143 条集成测试覆盖边缘情况
- **影响**: `/send_join`、`/send_leave`、`/thirdparty_invite` 的 PDU 现在获得完整图字段

### ✅ 3. U-19-R2：`events.content` GIN 索引

- **核查结果**: 索引**已存在**于 `migrations/00000000_unified_schema_v12.sql:3289`
- **声明**: `CREATE INDEX IF NOT EXISTS idx_events_content_gin ON events USING GIN (content jsonb_path_ops);`
- **结论**: 文档声称"缺失"为误判，实际实现已完整

### ✅ 4. U-20：Reaction 事件持久化

- **位置**: 合并自 `29ac9b316 fix(relations): persist reaction events through the shared write entry`
- **实现**:
  - 反应事件现在走 `create_event_with_graph` 持久化图字段
  - 编辑事件创建独立 `m.replace` 事件（避免 PK 冲突）
  - annotation 同时携带 `m.relates_to.key` 与 `body`
  - `m.replace` 路由优先读取 `m.relates_to.key`
- **影响**: 反应查询不再全表扫描

---

## 仍存在的问题

### ❌ 1. U-19：MSC3912 级联残缺（余 2 项）

| MSC3912 要求 | 当前实现 | 判定 |
|---|---|---|
| `"*"` 通配额外匹配 `content->'m.in_reply_to'`（已废弃） | `find_related_events_single_layer` wildcard 分支额外匹配 | ❌ 超范围匹配（非 bug，是设计取舍） |
| 不发 `m.room.redaction` 事件 | 仅本地 `is_redacted=true` | ❌ 对等端不知情（设计取舍） |

### ✅ 4. U-19-R4：`redacted_by` 审计追踪已修

- **修复**: 级联撤回的 `redact_event_content` 调用现传递 `m.room.redaction` 事件的 `event_id`
- **实现**: 
  - `synapse-services/src/event_redaction_service.rs`：`cascade_redact_related_events` 新增 `redaction_event_id` 参数
  - `synapse-web/src/routes/handlers/room/events.rs`：从已持久化的 redaction 事件克隆 ID 传入
  - 测试 `api_msc3912_redaction_cascade_tests.rs::msc3912_cascade_redaction_audits_with_redaction_event_id` 验证 `redacted_by` 以 `$` 开头且指向 `m.room.redaction` 行
- **提交**: `5d1bfdc3f`

### ❌ 3. U-3：v≤11 端点 `knock.rs`、`voip.rs` 位置问题

- **位置**: `knock.rs`、`voip.rs`
- **问题**: 仍消费占位 `event_id`；事务路径 (`tx = Some(..)`) 不 finalize ID
- **影响**: v3+ 房间 `/send` 落库的是服务器随机 ID 而联邦对端推 reference hash

### ❌ 4. U-5：Admin 媒体端点族不完整

- **位置**: `synapse-web/src/routes/admin/media.rs`
- **缺失**:
  - 房间级媒体列举：`GET /_synapse/admin/v1/rooms/{roomId}/media`
  - 房间级媒体删除：`DELETE /_synapse/admin/v1/rooms/{roomId}/media/{mediaId}`
- **参考**: 上游 v1.161 有 18 条 Admin 媒体端点，本仓仅 9 条

### ❌ 5. U-6：缩略图 `animated` 边缘问题

- **位置**: `synapse-web/src/routes/media/download.rs`、`synapse-services/src/media/mod.rs`
- **Gap**:
  - 动画 GIF：`animated=true` 退化为静态 JPEG（帧丢失）
  - 帧延迟 Clamp 后可能失真
  - 无动画检测 fallback：损坏的 WebP 可能报 500 而非降级

### ❌ 6. U-9：死代码 `get_auth_issuer` 未删

- **位置**: `synapse-storage/src/event/dag.rs:203-205`
- **现状**: 注释声称被删除，但函数仍在库中声明
- **问题**: 铁律 1 要求删除未使用的实际实现

### ❌ 7. U-11：v13 parse-only 状态

- **位置**: `synapse-common/src/room_versions.rs:114`
- **现状**: `RoomVersionCapability::stable_parse_only("13")`
- **原因**: MSC4204/4205 密码登出设备功能的 PDU 语义未完全对齐
- **决策**: 仍为设计使然，待联邦 PDU 语义完整后才可升级为 stable

---

## 已纠正的误述

### U-20 表述修正：设计取舍非缺陷

- **原文错误**: "reaction 不写入 events 表，级联查询读不到"
- **事实**: `event_relations` 表独立存储 relationship，`cascade.rs` 的 `find_related_events` 正确读取 `events.content->'m.relates_to'`，这是 MSC3912 规范设计
- **结论**: 文档对 U-20 的描述是误述，实际实现与上游一致

---

## 执行计划建议

1. **优先级 P0**：
   - U-3：v≤11 端点位置问题（knock、voip）

2. **优先级 P1**：
   - U-1：`"*"` 通配超范围匹配（需设计决策）
   - U-5：Admin 媒体端点
   - U-6：缩略图 animated 问题
   - U-7、U-9：死代码删除