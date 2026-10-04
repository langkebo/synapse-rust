# 当前仍存在的问题列表（基于 docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md §22.3）

> **⚠️ ❌7 / U-22 状态更正（2026-09-28）**：本条所记"v12 构成 MSC 未实现、room ID 为随机"
> 已部分收口 —— room ID 现由 create 事件 id 派生。MSC4291（创建侧 C-1/C-2、入站 D-1、升级 C-4）、MSC4289（E-1/E-2/E-3）、MSC4307（B-2）均已落地；**仅 MSC4297（State Resolution v2.1）未落**（本仓当前无状态决议路径）。
> 另：版本 13 已按裁定 Q5(b) 从能力表移除；v1–v11 已不可创建（G-1），仅 v12 可创建。逐项状态与证据以 `docs/audit/ROOM_V12_PLAN_STATUS_2026-09-27.md` 为**唯一现状来源**。

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
| `"*"` 通配额外匹配 `content->'m.in_reply_to'`（已废弃） | `find_related_events_single_layer` wildcard 分支额外匹配 | ✅ **U-1 已修**：wildcard 现仅匹配 MSC3912 标准的 `m.relates_to`（见「✅ 5. U-1」） |
| 不发 `m.room.redaction` 事件 | 仅本地 `is_redacted=true` | ❌ 对等端不知情（设计取舍） |

### ✅ 4. U-19-R4：`redacted_by` 审计追踪已修

- **修复**: 级联撤回的 `redact_event_content` 调用现传递 `m.room.redaction` 事件的 `event_id`
- **实现**:
  - `synapse-services/src/event_redaction_service.rs`：`cascade_redact_related_events` 新增 `redaction_event_id` 参数
  - `synapse-web/src/routes/handlers/room/events.rs`：从已持久化的 redaction 事件克隆 ID 传入
  - 测试 `api_msc3912_redaction_cascade_tests.rs::msc3912_cascade_redaction_audits_with_redaction_event_id` 验证 `redacted_by` 以 `$` 开头且指向 `m.room.redaction` 行
- **提交**: `5d1bfdc3f`

### ✅ 3. U-13-R9：v≤11 写路径持久化图字段

- **位置**: 合并自 `11bf5455d fix(room): persist depth/prev_events/auth_events on the v<=11 local write path`
- **实现**:
  - 修改 `synapse-services/src/room/messaging/events.rs:185-232`：v≥1 路径走 `create_event_with_pdu`
  - 未知/不可解析版本才回退到 `create_event`（无图字段）
  - 143 条集成测试覆盖
- **影响**: `/send_join`、`/send_leave`、`/thirdparty_invite`、`/knock`、`/voip` 事件都持久化图字段

### ✅ 4. U-3：knock.rs/voip.rs 占位 event_id 问题

- **位置**: 合并自 `31b475710 fix(api): consume the write entry's event id in knock/voip`
- **问题**: `knock_room`、`call_invite`、`call_answer` 生成占位符 `$$...`，但回应了占位符而非持久化后的 ID
- **修复**:
  - `knock.rs`: 返回 `stored.event_id` 而非占位符
  - `voip.rs`: `call_invite`/`call_answer` 同上  
  - `room_membership.rs`: `add_member` 投影不再存储冲突的 event_id
- **提交**: `31b475710`
- **测试**: 新增 `federation_existence_leak_tests::knock_room_returns_the_id_of_the_persisted_row`

### ✅ 5. U-5：Admin 媒体端点族补全（18 条）

- **位置**: `synapse-web/src/routes/admin/media.rs`
- **已实现**（2026-09-27 补全）:
  - 基础媒体管理：`GET /_synapse/admin/v1/media` ✅
  - 媒体详情：`GET /_synapse/admin/v1/media/{mediaId}` ✅
  - 媒体删除：`DELETE /_synapse/admin/v1/media/{mediaId}` ✅
  - 媒体配额：`GET /_synapse/admin/v1/media/quota` ✅
  - 用户媒体列表：`GET /_synapse/admin/v1/users/{userId}/media` ✅
  - 用户媒体删除：`DELETE /_synapse/admin/v1/users/{userId}/media` ✅
  - 房间级媒体列举：`GET /_synapse/admin/v1/rooms/{roomId}/media` ✅
  - 房间级媒体删除：`DELETE /_synapse/admin/v1/rooms/{roomId}/media/{mediaId}` ✅
  - 媒体隔离查询：`GET /_synapse/admin/v1/quarantine_media/{mediaId}/changes` ✅
  - 媒体隔离：`POST /_synapse/admin/v1/media/quarantine/{serverName}/{mediaId}` ✅
  - 解除隔离：`POST /_synapse/admin/v1/media/unquarantine/{serverName}/{mediaId}` ✅
  - 房间隔离：`POST /_synapse/admin/v1/rooms/{roomId}/media/quarantine` ✅
  - 房间解除隔离：`POST /_synapse/admin/v1/rooms/{roomId}/media/unquarantine` ✅
  - 媒体保护：`POST /_synapse/admin/v1/media/protect/{serverName}/{mediaId}` ✅
  - **用户级隔离**：`POST /_synapse/admin/v1/user/{userId}/media/quarantine` ✅ (U-5 新增)
  - **按策略删除**：`POST /_synapse/admin/v1/media/delete` ✅ (U-5 新增)
  - **清除缓存**：`POST /_synapse/admin/v1/purge_media_cache` ✅ (U-5 新增)
  - **解除保护**：`POST /_synapse/admin/v1/media/unprotect/{mediaId}` ✅ (U-5 新增)
- **参考**: 上游 v1.161 有 18 条 Admin 媒体端点，本仓现 18 条 ✅

### ✅ 5. U-1：`"*"` 通配 MSC3912 合规性

- **位置**: `synapse-storage/src/event/cascade.rs:70-85`
- **问题**: wildcard (`["*"]`) 同时匹配 `m.relates_to` 和已废弃的 `m.in_reply_to`
- **修复**: wildcard 现仅匹配 MSC3912 标准的 `m.relates_to`
- **影响**:
  - 符合 MSC3912 规范
  - 老旧事件（仅有 `m.in_reply_to` 而无 `m.relates_to`）在 wildcard 下将不再被匹配
  - 如需匹配 legacy 格式，调用方需单独传入 `"m.in_reply_to"`（按文档说明，此类事件无 `rel_type` 字段，故实际不匹配）
- **提交**: 本次提交

### ❌ 6. U-6：缩略图 `animated` 边缘问题

- **位置**: `synapse-services/src/media_service.rs:496-498`
- **Gap**:
  - ✅ **动画支持**：`generate_animated_thumbnail` 完整实现（解码所有帧 → WebP 编码）
  - ❌ **缓存文件名误导**：`{media_id}_{width}x{height}_{method}_animated.jpg` 后缀恒为 `.jpg`，但实际内容是 WebP 字节
  - ✅ **content_type 正确**：返回头 `Content-Type: image/webp` 是正确的
- **修复**: 动画缩略图缓存文件后缀改为 `.webp`，静态保持 `.jpg`
- **提交**: 本次提交

### ✅ 6. U-9：死代码 `get_auth_issuer` 已清理

- **位置**: `synapse-web/src/routes/handlers/auth_discovery.rs:66`（原文档误指 `synapse-storage/src/event/dag.rs:203-205`）
- **现状**:
  - `get_auth_issuer` 已删除（该函数注解为 `#[deprecated]`，上游在 v1.161 中移除）
  - 该函数从未被注册为路由，`assembly.rs` 也未引用
  - 仅 `auth_metadata` 存留，已单独维护
- **删除提交**: 本次提交
- **后续**: 模块 `auth_discovery` 仍保留 `get_auth_metadata` 供参考

### ✅ 7. U-22：v12 被声明为 stable/默认，但其构成 MSC 未实现；"v13" 不存在

- **位置**: `synapse-common/src/room_versions.rs`（`stable("12")` + `DEFAULT_ROOM_VERSION = "12"` + `stable_parse_only("13")`）
- **现状（2026-09-27 核实并更正原文）**:
  - 原文写"**原因**: MSC4204/4205 密码登出设备功能的 PDU 语义未完全对齐" —— **不实**。
    MSC4204 是"改密时登出设备"（见本仓 `routes/account_compat.rs:353` 的自身引用），与房间版本无关。
  - 原文把 **v13** 当作待升级的真实版本 —— **不实**：**上游不存在 v13**。规范稳定列表止于 v12
    （matrix-spec `content/rooms/_index.md`），上游 Synapse 1.161.0 只识别 `1..12` + 三个 unstable
    （`org.matrix.hydra.11`、`org.matrix.msc3757.10/11`），MSC4304 的 prior-art 链也止于 v12。
  - **真实问题**：v12 由 **MSC4304** 定义（= v11 + MSC4289 创建者特权 + MSC4291 room ID = create 事件哈希
    - MSC4297 State Res v2.1 + MSC4307 `auth_events` 同房间校验），本仓四个都**未实现**，
    却已把 v12 标为 `stable`（可创建）并设为默认版本。
- **影响**: 本机创建的 v12 房间用**随机** room ID（`crypto.rs:145`），而 v12 对端要求 room ID 等于
  create 事件哈希 ⇒ 互操作破坏；且 `redaction.rs:210` 已置 `room_ids_as_hashes: true`，
  撤回 `m.room.create` 后 room ID 不可恢复。
- **决策**: 二选一 —— ① 实现四个 MSC 后再放开；② **降回 `stable_parse_only("12")` 且默认版本回 v11**
  （与 `65f70e33` 的 fail-safe 一致），并同步 `/versions`、`/capabilities` 与快照。
- **依据**: `docs/audit/V12_ROOM_VERSION_AND_ANIMATED_THUMBNAIL_IMPLEMENTATION_PLAN.md` §1.1；
  上游 MSC4304 原文；本机 `synapse 1.161.0` 的 `KNOWN_ROOM_VERSIONS` 实测。

---

## 已纠正的误述

### U-20 表述修正：设计取舍非缺陷

- **原文错误**: "reaction 不写入 events 表，级联查询读不到"
- **事实**: `event_relations` 表独立存储 relationship，`cascade.rs` 的 `find_related_events` 正确读取 `events.content->'m.relates_to'`，这是 MSC3912 规范设计
- **结论**: 文档对 U-20 的描述是误述，实际实现与上游一致

---

## 执行计划建议

1. **优先级 P0**：
   - ~~U-3：v≤11 端点位置问题（knock、voip）~~ ✅ 已完成（`31b475710`）

2. **优先级 P1**：
   - ~~U-1：`"*"` 通配超范围匹配~~ ✅ 已完成（wildcard 现仅匹配 MSC3912 标准的 `m.relates_to`）
   - ~~U-5：Admin 媒体端点~~ ✅ 已完成（`555a21b75`，18/18 条端点全覆盖）
   - ~~U-6：缩略图 animated 问题~~ ✅ 已完成（缓存文件名后缀纠正：动画 `.webp` / 静态 `.jpg`）
   - ~~U-9：死代码 `get_auth_issuer`~~ ✅ 已完成
