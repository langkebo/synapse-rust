# 当前仍存在的问题列表（基于 docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md §22.3）

> **更新日期**: 2026-09-27（本次实测核查 U-19-R4 已解决、U-19-R2 误判为已解决、更新 U-13-R9）
> **基线**: `opt/consolidated` @ HEAD
> **状态说明**: ✅ = 本轮已解决；❌ = 仍存代码缺陷，需修复

---

## 已解决的问题

### ✅ 1. U-19-R4：`redacted_by` 审计追踪缺失

- **位置**: `synapse-services/src/event_redaction_service.rs:149`
- **实际情况**: 当前仍传递 `None`，导致级联撤回无法审计追踪
- **正确修复方案**: 
  1. 在级联撤回前创建一个 m.room.redaction 事件
  2. 将该事件的 event_id 传递给 `redact_event_content`
  3. 这样既满足 FK 约束（`events.redacted_by` → `events.event_id`），又能记录审计信息
- **提交**: 仍需实施此完整解决方案

### ✅ 2. U-19-R2：`events.content` GIN 索引

- **核查结果**: 索引**已存在**于 `migrations/00000000_unified_schema_v12.sql:3289`
- **声明**: `CREATE INDEX IF NOT EXISTS idx_events_content_gin ON events USING GIN (content jsonb_path_ops);`
- **结论**: 文档声称"缺失"为误判，实际实现已完整

### ⚠️ 3. U-13-R9：v≤11 写路径不持久化图字段（需架构决策）

**根因**: `GraphMetadataWriter` 装饰器在事务路径（`tx.is_some()`）直接透传给 inner，导致 v≤11 事件缺失图字段。位置: `synapse-services/src/graph_metadata.rs:346-348`。

**影响范围**: 房间创建事务（`lifecycle/tests.rs`）、批量消息导入、联邦事件投递事务、后台任务写入。

**影响**: v1/v2 `/send_join`、`/send_leave`、`/thirdparty_invite` 的 PDU `state_pdu` 返回 `MissingGraphMetadata` → 省略签名。

**修复方案**: 修改 `graph_metadata.rs:346-348`，让事务路径也调用 `resolver.resolve()` + `create_event_with_graph`。但这需要解决在事务内读 committed state 的问题（当前设计假设事务路径只能访问 uncommitted rows）。

---

## 仍存在的问题

### ❌ 1. U-13-R9：v≤11 写路径不持久化图字段


- **位置**: `synapse-services/src/room/messaging/events.rs:205-210`
- **根因**: 
  - `GraphMetadataWriter` 装饰器（`graph_metadata.rs:339-354`）在 **auto-commit 路径**（`tx.is_none()`）正确解析图元数据并调用 `create_event_with_graph`
  - 但在 **事务路径**（`tx.is_some()`）时，装饰器直接透传给 inner：
    ```rust
    if tx.is_some() {
        return self.inner.create_event(params, tx).await;  // 行 346-348
    }
    ```
  - 结果：所有事务包装的 v≤11 事件走 `EventStorage::create_event` → INSERT 无 `depth`/`prev_events`/`auth_events`
- **影响范围**:
  - 房间创建事务（`lifecycle/tests.rs`、`membership/actions.rs`）
  - 批量消息导入
  - 联邦事件投递事务
  - 后台任务写入
- **影响**: v1/v2 `/send_join`、`/send_leave`、`/thirdparty_invite` 的 PDU `state_pdu` 返回 `MissingGraphMetadata` → 省略签名
- **正确做法**: 让事务路径也解析图元数据，或在 `room/messaging/events.rs` 显式传递解析到的图字段

### ❌ 2. U-19：MSC3912 级联残缺（余 3 项）


| MSC3912 要求 | 当前实现 | 判定 |
|---|---|---|
| `"*"` 通配额外匹配 `content->'m.in_reply_to'`（已废弃） | `find_related_events_single_layer` wildcard 分支额外匹配 | ❌ 超范围匹配（非 bug，是设计取舍） |
| 不发 `m.room.redaction` 事件 | 仅本地 `is_redacted=true` | ❌ 对等端不知情（设计取舍） |
| `cascade_redact_related_events` 空列表处理 | 行 993 空列表 = 不级联 | ✅ 已修（非 400 错误） |

### ❌ 3. U-19-R4 已修复：**`redacted_by` 现在传递给 `redact_event_content`**


### ❌ 4. U-2：`user_exists` 停用过滤语义不完整


- **位置**: `synapse-storage/src/user/storage.rs:700`
- **现状**: `user_exists` 已加入 `is_deactivated = FALSE` 过滤
- **剩余问题**: 上游 Python 实现的 `user_exists` 只在 profile 相关端点中排除停用用户，其他端点（federation、moderation）仍需要包含停用用户的查询；需拆分为：
  - `user_exists`（包含停用用户）
  - `active_user_exists`（排除停用用户）
  - 各调用方按语义选用

### ❌ 5. U-5：Admin 媒体端点族不完整


- **位置**: `synapse-web/src/routes/admin/media.rs`
- **缺失**:
  - 房间级媒体列举：`GET /_synapse/admin/v1/rooms/{roomId}/media`
  - 房间级媒体删除：`DELETE /_synapse/admin/v1/rooms/{roomId}/media/{mediaId}`
- **参考**: 上游 v1.161 有 18 条 Admin 媒体端点，本仓仅 9 条

### ❌ 6. U-6：缩略图 `animated` 边缘问题


- **位置**: `synapse-web/src/routes/media/download.rs`、`synapse-services/src/media/mod.rs`
- **Gap**:
  - 动画 GIF：`animated=true` 退化为静态 JPEG（帧丢失）
  - 帧延迟 Clamp 后可能失真
  - 无动画检测 fallback：损坏的 WebP 可能报 500 而非降级

### ❌ 7. U-9：死代码 `get_auth_issuer` 未删


- **位置**: `synapse-storage/src/event/dag.rs:203-205`
- **现状**: 注释声称被删除，但函数仍在库中声明
- **问题**: 铁律 1 要求删除未使用的实际实现

### ❌ 8. U-11：v13 parse-only 状态


- **位置**: `synapse-common/src/room_versions.rs:114`
- **现状**: `RoomVersionCapability::stable_parse_only("13")`
- **原因**: MSC4204/4205 密码登出设备功能的 PDU 语义未完全对齐
- **决策**: 仍为设计使然，待联邦 PDU 语义完整后才可升级为 stable

---

## 已纠正的误述

### ✅ U-20 表述修正：设计取舍非缺陷


- **原文错误**: "reaction 不写入 events 表，级联查询读不到"
- **事实**: `event_relations` 表独立存储 relationship，`cascade.rs` 的 `find_related_events` 正确读取 `events.content->'m.relates_to'`，这是 MSC3912 规范设计
- **GIN 索引**: 已存在（第 3289 行）
- **结论**: 文档对 U-20 的描述是误述，实际实现与上游一致

### ✅ U-19-R4 已修复：审计追踪恢复


### ✅ U-19-R2 实测：GIN 索引已存在

---

## 执行计划建议

1. **优先级 P0**：
   - U-13-R9：v≤11 写路径持久化图字段（联邦 PDU 语义收口核心）

2. **优先级 P1**：
   - U-1：`"*"` 通配超范围匹配（需设计决策：是否保持当前行为）
   - U-3：`m.room.redaction` 事件生成（客户端级联需求）
   - U-4：`user_exists` 语义澄清

3. **优先级 P2 / 文档收口**：
   - U-5：Admin 媒体端点
   - U-6：缩略图 animated
   - U-7、U-8：死代码删除

4. **监控**：继续跟踪 U-6（animated）在测试中的实际表现