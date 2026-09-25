# 当前仍存在的问题列表（基于 docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md §22.3）

> **更新日期**: 2026-09-25
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

### 6. Admin 媒体端点族真缺口（本仓 7 vs 上游文档面 18）
#### 6.1 真缺口（仓库侧 0 命中）
- **位置**: `synapse-web/src/routes/admin/media.rs`
- **描述**:
  - `POST /_synapse/admin/v1/media/quarantine/{server_name}/{media_id}`
  - `POST /_synapse/admin/v1/media/unquarantine/{server_name}/{media_id}`
  - 房间级媒体列举/删除
- **旁证**: 鉴权白名单已为**不存在**的路由预留了路径（`utils/admin_auth.rs:386` 的 `/media/quarantine` 前缀）
- **修法**: 需新增 service 方法 + handler

### 7. 缩略图 `animated` 参数未支持
- **位置**: 全仓 `.rs` **0 命中**
- **描述**: 媒体缩略图生成不支持 `animated` 参数（如 GIF 保持动态）
- **结论**: 全仓无引用，可能是设计选择而非缺陷（待确认）

### 8. ledger `query_params` 字段无消费方
- **位置**: `synapse-web/src/routes/ledger_export.rs:156`
- **描述**: 序列化导出 fixture 时写入但无校验/断言消费

### 9. v12/v13 房间不可创建（fail-safe 设计使然）
- **位置**: `synapse-common/src/room_versions.rs:114-115`
- **描述**: 为 `stable_parse_only("12"|"13")`；注释写明理由是"避免创建无法产生合规 PDU 的房间（fail-safe）"
- **结论**: 不是缺陷，是取舍

---

## 执行计划建议

1. **中优先级**: Admin 媒体缺口 — 新增 `quarantine_media` / `unquarantine_media` service + handler
2. **待确认**: `animated` 参数 — 需产品决策是否支持
3. **低优先级**: ledger `query_params` — 可标注为已知差异
4. **设计使然**: v12/v13 房间创建限制 — 无需处理
