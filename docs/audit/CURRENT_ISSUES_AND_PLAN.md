# 当前仍存在的问题列表（基于 docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md §22.3）

> **更新日期**: 2026-09-25
> **基线**: `opt/consolidated` @ HEAD
> **状态说明**: ✅ = 本轮已解决；❌ = 仍存在

---

## 已完成的问题

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

---

## 仍存在的问题

### 1. 客户端撤回不级联（MSC3912 级联仅管理端可达）
- **位置**：`synapse-web/src/routes/handlers/room/events.rs:990`（`redact_event_content`）
- **描述**：客户端路径只调用 `redact_event_content` 撤单条；MSC3912 级联入口只有管理端
- **管理端入口**：`admin/room/mod.rs:244,788`（`/_synapse/admin/v1/rooms/{room_id}/cascade_redact`）

### 2. Content Scanner 零生产调用点（配置可开却不扫描）
- **位置**：`synapse-services/src/wiring/core.rs:179`（构造） + 无消费者
- **描述**：`ContentScanner` 有 `scan`/`scan_text`/`scan_media` 三个公开方法，但全仓只有它自己的单测在调用；生产侧仅构造，无消费者

## 中优先级问题

### 3. `dag.rs` 注释声称的调用点不存在
- **位置**：`synapse-storage/src/event/dag.rs:203-205`
- **描述**：注释写明 "Used by `/send_join` (federation) … and by `/get_missing_events`"，但 `get_state_dag_edges` 的引用只有 `db_tests.rs`（生产 0 调用点）
- **确认的生产调用点**：`federation/transaction.rs:322`、`federation/events.rs:66`（但注释对这两个没说错）

### 4. `msc2965/auth_issuer` 仍在册（上游 1.161 已删该端点）
- **位置**：`routes/assembly.rs:197`（+ `derived_route_table_always.inc.rs:114`）
- **上游证据**：1.161 `CHANGES.md:48`：`Drop GET /_matrix/client/unstable/org.matrix.msc2965/auth_issuer endpoint which never ended up being used. (#20163)`
- **口径须收窄**：上游只删了 `auth_issuer`，`auth_metadata` 未删（同文件 `:612` 还专门为它加了缓存）

### 5. Profile 三处偏差（2/3 未修）
#### 5.1 停用但存在用户写自定义字段应成功
- **位置**：`user/storage.rs:698`（`user_exists` 的 SQL 带 `AND is_deactivated = FALSE`）
- **描述**：停用用户被判"不存在"，写路径 `extended_profile.rs:124` 直接 404
- **上游期望**：1.161 `CHANGES.md:36`（#20172）："this now **succeeds for existing (e.g. deactivated) users** and returns a 404 error if the user does not exist"

#### 5.2 稳定 `/{keyName}` 未注册
- **位置**：派生路由表里缺少泛化 `{key_name}` 路由
- **描述**：`/_matrix/client/v3/profile/{user_id}`、`/avatar_url`、`/displayname` 都在，**没有**泛化 `{key_name}`；泛化版只在 `unstable/uk.tcpip.msc4133` 下
- **声明与注册面不一致**：`/versions` 已声明 `m.profile_fields`（`capability_governance.rs:507`）

### 6. Admin 媒体端点族真缺口（本仓 7 vs 上游文档面 18）
#### 6.1 真缺口（仓库侧 0 命中）
- **位置**：
  - `POST .../media/quarantine/{server_name}/{media_id}`
  - `POST .../media/unquarantine/{server_name}/{media_id}`
  - 房间级媒体列举/删除
- **旁证**：鉴权白名单已为**不存在**的路由预留了路径（`utils/admin_auth.rs:386` 的 `/media/quarantine` 前缀）

### 7. 缩略图 `animated` 参数未支持
- **位置**：业务层 0 命中
- **描述**：全仓 `.rs` **0 命中** ⇒ **仍存在**（未支持）

### 8. ledger `query_params` 字段无消费方
- **位置**：
  - 定义：`synapse-web/src/routes/route_ledger.rs:84`
  - Builder：`:107` 有 builder `with_query_params`
  - 消费：零调用点（全仓仅它自己的定义）；`ledger_export.rs:156` 只把它序列化进导出 fixture，无任何校验/断言消费

## 低优先级问题（建议从问题清单移入"已知取舍"）

### 9. v12/v13 房间不可创建（fail-safe 设计使然）
- **位置**：`synapse-common/src/room_versions.rs:114-115`
- **描述**：为 `stable_parse_only("12"\|"13")`；注释写明理由是"避免创建无法产生合规 PDU 的房间（fail-safe）"
- **结论**：不是缺陷，是取舍

### 10. `search_index` 遗留表（D-39）
- **位置**：baseline 仍有该表；而 `synapse-storage/src/search_index.rs` 文件已不存在
- **描述**：表无代码消费

---

## 执行计划建议（参考 §22.5）

1. **高优先级**：Content Scanner 接线（**已完成**）- 在媒体/消息落库前调 `scan_media`/`scan_text`
2. **高优先级**：客户端撤回接级联
3. **中优先级**：Profile 两条（停用用户写自定义字段 + 稳定 `/{keyName}` 注册）
4. **中优先级**：Admin 媒体缺口（补 `media/quarantine|unquarantine` POST 端点）
5. **中优先级**：`dag.rs` 注释（改注释或补调用点）
6. **中优先级**：`msc2965/auth_issuer` 直接摘除（上游已删）
7. **低优先级**：`search_index` 表删或标注废弃（需同步 baseline 指纹）