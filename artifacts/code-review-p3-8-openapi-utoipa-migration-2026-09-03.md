# Code Review: P3-8 OpenAPI utoipa derive 迁移

**文件**: `src/web/api_doc/{client_server,admin,auth}.rs` + `schemas.rs`
**评审时间**: 2026-09-03 18:30
**评审人**: CodeReviewExpert

---

## 现状摘要

| 文件 | 行数 | untyped body (`body = serde_json::Value`) |
|------|------|----------------------------------------|
| `client_server.rs` | 4837 | 278 |
| `admin.rs` | 2413 | 106 |
| `auth.rs` | 1573 | 88 |
| **合计** | **8823** | **472** |

PoC（commit `4ab3703b`）已建立完整框架：
- `schemas.rs` 定义 `ApiPusher` + `ApiPushersResponse`（`#[derive(utoipa::ToSchema)]`）
- `mod.rs` 的 `#[openapi(...)]` 已注册 ~300 条 `paths(...)` + 4 个 `schemas(...)`
- doc handler 用 `#[utoipa::path(...)]` + `unreachable!()` — **纯文档用途，不绑定真实路由**

---

## 关键架构约束

**`utoipa` 仅在 root crate 的 `openapi-docs` feature 下可选引入**：

```toml
# Cargo.toml
openapi-docs = ["dep:utoipa", "dep:utoipa-swagger-ui"]
utoipa = { version = "5", features = ["axum_extras"], optional = true }
```

`synapse-storage` 和 `synapse-services` 没有 utoipa 依赖。**不能直接在 storage/service struct 上加 `ToSchema`**。

**设计选择**：在 `schemas.rs` 创建**独立的 `Api*` wrapper struct**（如 `ApiPusher`），
从 handler 的 `json!()` 块或 Matrix spec 示例中提取字段，加上 `#[derive(utoipa::ToSchema, serde::Serialize)]`。

---

## 迁移价值分层

### 🔴 高价值（客户端开发强依赖，建议优先迁移）

**Device endpoints** (`client_server.rs:141-193`)
```rust
// 当前：body = serde_json::Value（已有 example，含 device_id/display_name/last_seen）
// 建议迁移到：
body = ApiDeviceListResponse  // { devices: Vec<ApiDevice> }
body = ApiDeviceResponse      // { device: ApiDevice, device_id, display_name, last_seen_ts }
```
真实 handler 用 `json!()` 构造，字段形状完全已知，迁移成本低。

**Profile endpoints** (`client_server.rs`)
- `get_profile_info` / `get_profile_displayname` / `get_profile_avatar_url` / `update_avatar_url_doc`
- Matrix spec 定义清晰，字段固定（avatar_url / displayname / mxc:// URI）
```rust
body = ApiProfileResponse   // { avatar_url: Option<String>, displayname: Option<String> }
```

**Presence endpoints** (`client_server.rs`)
- `get_presence_status` / `set_presence_status_v1_doc`
- 固定字段：`presence` (online/away/offline), `status_msg`, `last_active_ago`

**Room creation** (`client_server.rs`)
- `create_room_doc` — Matrix spec 有完整 response schema（room_id / room_version /征战...）

### 🟡 中等价值（spec 有定义，但字段多或变体多）

**Sync** — `sync_doc` / `get_events_doc`
- 强烈建议保持 `Value`：sync 响应字段极多（rooms/presence/to_device/chunk），且版本间变化
- 可选：拆出 `ApiSyncResponse` 但 ROI 低

**Search** — `search_room_events_doc` / `search_rooms_doc`
- 保持 `Value`：search_results 内部结构复杂，客户端通常自己解析

**Push rules** (`pushrules`)
- `get_push_rules` → `Result<Vec<PushRule>>`，`PushRule` 是 synapse-services 已定义的结构
- 迁移代价：需要给 synapse-services 的 `PushRule` 加 ToSchema（需加 utoipa 依赖），ROI 不高

**Media upload** — `upload_media_v3_doc`
- Matrix spec 定义了 `content_uri` (mxc://...) 返回，字段简单
```rust
body = ApiMediaUploadResponse  // { content_uri: String }
```

### 💭 低价值（内部/调试端点，客户端不直接调用）

**Admin endpoints**（大部分）
- `admin_cleanup_abnormal_rooms_doc` / `admin_info_doc` / `admin_statistics_doc`
- `admin_jitsi_config_doc` — 内部运营用，OpenAPI 文档价值低

**Federation endpoints** — 在 `federation.rs`，不在本次 11h 范围内

**OIDC/SAML callbacks** — 重定向端点，无 response body

---

## 迁移模式（供执行参考）

每个 endpoint 的迁移步骤：
1. 从 `#[utoipa::path(...)]` 的 `example = json!({...})` 中提取字段
2. 在 `schemas.rs` 创建 `Api<ResponseName>` struct，加上 `#[derive(utoipa::ToSchema, serde::Serialize, Debug)]`
3. 在 handler 的 `responses(...)` 中把 `body = serde_json::Value` 替换为 `body = Api<ResponseName>`
4. 在 `mod.rs` 的 `components(schemas(...))` 注册新类型（如果 utoipa 不自动收集）

**PoC 已验证**：注册路径是 `schemas.rs` 定义 → doc handler 引用 → `mod.rs` 的 schemas 列表（目前 utoipa 5 的 derive 需要显式注册到 components，auto 模式见下方）

### utoipa 5 模式选择

`utoipa` v5 支持两种 schema 收集模式：
- **显式**（当前 PoC）：`components(schemas(...))` 手动列出
- **自动**：`#[openapi(components)]` 让 derive 自动收集

建议用**显式模式**（保持 PoC 一致），每次新增 struct 后加一行到 `components(schemas(...))`。

---

## 代码质量观察

### ✅ 做得好的地方
1. `#[cfg(feature = "openapi-docs")]` 隔离，避免生产二进制膨胀
2. `unreachable!()` doc handler 模式清晰——文档路由与业务路由完全解耦
3. `example = json!({...})` 已在许多 endpoint 提供了字段形状，降低迁移成本
4. `tag = "Client-Server"` 等 tag 分类合理，与 OpenAPI tags 对齐

### ⚠️ 风险点

**1. Doc handler 与真实 handler 脱节**
- `get_devices()` doc 返回 `ApiDeviceListResponse`，但真实 `src/web/routes/device.rs` 的 `get_devices()` 返回 `Json<Value>`
- **这是设计意图**（doc-only），但意味着 OpenAPI 文档可能与实际 API 行为产生 drift
- 建议：加 CI 检查——定期用 OpenAPI validator 对比实际 handler 签名（超出 P3-8 范围，记录为 tech debt）

**2. `serde_json::Value` 在 request_body 同样存在**
- `client_server.rs:205` 等处有 `request_body = serde_json::Value`
- 这些也值得评估：对于 `PUT /devices/{device_id}` 这类 request body，Matrix spec 有明确 schema

**3. Federation endpoints 完全未评估**
- `federation.rs` 不在本次 scope，但 Federation API 也是 Matrix 重要部分

---

## 推荐执行计划（11h 预算分配）

| 优先级 | 工作项 | 预估工时 | 产出 |
|--------|--------|----------|------|
| P1 | Device endpoints (4 个)：get_devices, get_device, update_device, delete_device | 1.5h | ~6 个新 schema |
| P1 | Profile endpoints (4 个)：info, displayname, avatar_url, update_avatar_url | 1h | ~4 个新 schema |
| P1 | Presence endpoints (6 个)：get/set presence v1/v3/r0 | 1h | ~4 个新 schema |
| P2 | Room endpoints (6 个)：create_room, join, leave, invite, get_joined_members, forget | 1.5h | ~6 个新 schema |
| P2 | Media endpoints (4 个)：upload, download, thumbnail, preview_url | 1h | ~4 个新 schema |
| P2 | Admin 高价值 (8 个)：whois, whois_device, user_stats, room_stats, purge_history, purge_room, block_room, shadow_ban | 2h | ~8 个新 schema |
| P3 | 其他剩余 admin/auth 端点 | 2h | ~15 个新 schema |
| - | 验证：cargo build --features openapi-docs + route_ledger snapshot | 1h | 确保无编译错误 |
| **合计** | | **11h** | **~50-70 个新 typed schema** |

---

## 遗留项（超出 P3-8 范围）

1. **UTOIPA-REQUEST-BODY**：request_body 的 `serde_json::Value` 替换（~100+ 处）
2. **FEDERATION-DOCS**：`federation.rs` 的 80+ endpoint 文档化
3. **DOC-DRIFT-CI**：CI 检查 doc handler 与实际 handler 的类型一致性
4. **PUSH-RULE-SCHEMA**：给 synapse-services 的 `PushRule` 加 utoipa（需改 synapse-services 的 Cargo.toml 加可选依赖）

---

## 结论

P3-8 是一个**高价值但机械**的工作包。472 个 endpoint 中，约 80-100 个（客户端常用、高 spec 稳定性）值得优先迁移；其余保持 `Value`（内部/调试/高变化 API）。

建议先执行 P1 批次（Device/Profile/Presence，~3.5h），验证模式稳定后批量推进 P2/P3。

---

*评审完成。推荐按上述优先级表执行，而非盲目全量迁移 472 个 endpoint。*
