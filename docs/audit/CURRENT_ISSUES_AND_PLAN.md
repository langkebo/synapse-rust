# 当前仍存在的问题列表（基于 docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md §22.3）

> **更新日期**: 2026-09-26（本次审查确认 `query_params` 有消费方、Admin 媒体端点已实施）
> **基线**: `opt/consolidated` @ HEAD
> **状态说明**: ✅ = 本轮已解决；❌ = 仍存在

---

## 已完成的问题（不再追踪）

### ✅ 1. 客户端撤回不级联（MSC3912）—— 已修复
- **位置**: `synapse-web/src/routes/handlers/room/events.rs:993`
- **修法**: 添加 `cascade_redact_event` 调用，使用 fail-closed 策略
- **提交**: `76e5f9136`

### ✅ 2. Content Scanner 零生产调用点 —— 已修复
- **位置**: `synapse-web/src/routes/media/upload.rs` + `synapse-web/src/routes/handlers/room/events.rs`
- **修法**: 接入 `scan_media` / `scan_text` 到媒体上传和消息发送路径
- **集成测试**: `tests/integration/api_content_scanner_integration_tests.rs`
- **提交**: `8cd21a87a`

### ✅ 3. `dag.rs` 注释失真 —— 已修复
- **位置**: `synapse-storage/src/event/dag.rs:203-205`
- **修法**: 修正注释，移除错误的 `/send_join` 调用点声明
- **提交**: `76e5f9136`

### ✅ 4. `msc2965/auth_issuer` 仍在册 —— 已修复
- **位置**: `synapse-web/src/routes/assembly.rs:197`
- **修法**: 删除路由注册，标记 `get_auth_issuer` 为 `#[deprecated]`
- **提交**: `76e5f9136`

### ✅ 5.1 Profile 停用用户写自定义字段 404 —— 已修复
- **位置**: `synapse-storage/src/user/storage.rs:698`
- **修法**: 移除 `user_exists` 查询中的 `AND is_deactivated = FALSE` 过滤
- **提交**: `9e5ca99b5`

### ✅ 5.2 Profile 稳定版 `/{keyName}` 未注册 —— 已修复
- **位置**: `synapse-web/src/routes/assembly.rs`
- **修法**: 添加稳定版路由 `/_matrix/client/v3/profile/{user_id}/{key_name}` (GET/PUT/DELETE)
- **配套**: 重生成派生路由表 + 更新 ledger fixtures + 集成快照
- **提交**: `eeb99cef8` + `9e6741511` + `ba7aa103e`

### ✅ 10. `search_index` 遗留表 —— 已修复
- **位置**: `migrations/00000000_unified_schema_v12.sql:2750-2761` + 4 条索引
- **修法**: 删除表定义 + 4 条索引（`idx_search_index_content_trgm`、`room`、`user`、`type`）
- **配套**: 更新 baseline fingerprint `a20182b71fb77e7e` → `793304d36eee7917`
- **状态**: 已提交，测试待验证

---

## 仍存在的问题

### 6. Admin 媒体端点族缺口（本仓 9 vs 上游文档面 18）
#### 6.1 已修复（`337318c86`）
- **位置**: `synapse-web/src/routes/admin/media.rs`
- **修复内容**:
  - ✅ `POST /_synapse/admin/v1/media/quarantine/{server_name}/{media_id}`
  - ✅ `POST /_synapse/admin/v1/media/unquarantine/{server_name}/{media_id}`
  - 委托链：`AdminMediaService::quarantine_media/unquarantine_media` → `QuarantinedMediaChangeStoreApi`
- **仍未实施**：房间级媒体列举/删除（低优先级，需设计决策）

#### 6.2 鉴权白名单残留（已过期）
- **位置**: `utils/admin_auth.rs:386`
- **说明**: `/media/quarantine` 前缀原本为不存在的路由预留，现该路由已实施，白名单前缀有效

### ✅ 6.3 缩略图 `animated` 参数支持
- **位置**: `synapse-web/src/routes/media/download.rs` + `synapse-services/src/media_service.rs` + `synapse-services/src/media/mod.rs`
- **说明**: Phase 1 & Phase 2 已完成：
  - ✅ `animated=true/false` 请求参数解析（`thumbnail_request_params` 返回四元组）
  - ✅ GIF/WebP 动画检测（魔数字节检测，安全快速）
  - ✅ 首帧提取降级策略（`AnimationDecoder::into_frames().next()`）
  - ✅ Content-Type 正确返回：动画 `image/webp`，静态 `image/jpeg`
  - ✅ Phase 2: 完整动画 WebP 编码输出（`webp-animation 0.10.0` crate）
    - `generate_animated_thumbnail`: 解码所有帧 → 处理 → 编码
    - `decode_all_frames`: 辅助函数提取 GIF/WebP 帧序列
    - 帧延迟保留并 clamp(10, 5000)ms 防止极端值
- **提交**: 
  - Phase 1: `f71c574f8` feat(media): add animated thumbnail parameter support (Phase 1)
  - Phase 2: `2bbe172d8` feat(media): Phase 2 - complete animated WebP encoding with webp-animation
- **门禁**: Clippy `-D warnings` ✅ 通过 | 编译 ✅ 干净

### ✅ 8. ledger `query_params` 字段 — 已确认有消费方
- **位置**: `synapse-web/src/routes/ledger_export.rs:156`
- **描述**: 原认为"无消费方"，经核查实际有 4 个下游消费者：
  1. `scripts/api_test/generate_openapi.py:336` — 将 `query_params` 转化为 OpenAPI parameters
  2. `scripts/api_test/gen_route_table.py:64` — 透传到 `route-table.json`
  3. `scripts/contract/extract_registered.py` — 校验并提取（第 58、971、1061 行）
  4. `scripts/contract/gen_derived_routes.py:257` — 生成 `.with_query_params()` 调用
- **结论**: 字段被正确使用，非缺陷

### ✅ 9. v12/v13 房间版本状态 — 已核查（v12 已实现，v13 为 parse-only）
- **位置**: `synapse-common/src/room_versions.rs:112-114`
- **当前状态**：
  - Line 113: `RoomVersionCapability::stable("12")` → **v12 已完整实现**（`can_create=true, can_parse=true`）
  - Line 114: `RoomVersionCapability::stable_parse_only("13")` → **v13 为 parse-only**（`can_create=false, can_parse=true`）
- **v13 parse-only 原因**：MSC4204 密码登出设备功能依赖的 PDU 语义（特别是 `event_id` 的 v4+ reference hash）尚未完全对齐上游 Synapse，为避免创建无法产生合规 PDU 的房间，暂时设为 parse-only（fail-safe）
- **结论**: v12 已可用；v13 的 parse-only 是设计取舍，待联邦 PDU 语义完整后升级为 stable

---

## 执行计划建议

1. **已完成**: Admin 媒体缺口 — `quarantine_media` / `unquarantine_media` service + handler 实施完成
   - Commit `337318c86`：完整实现 + 路由同步 + 单元测试通过
   - 房间级媒体列举/删除：已在规划中（低优先级）
2. **已完成**: `animated` 参数 Phase 1 + Phase 2（完整动画 WebP 输出）
3. **已结论**: ledger `query_params` — 已核查为有下游消费方，非缺陷
4. **设计使然**: v13 parse-only — 待联邦 PDU 语义完整后升级为 stable；v12 已可用（`stable("12")`）
