# synapse-rust HTTP 边界层结构性代码审查

- 日期：2026-07-10
- 范围：`src/web/routes/`（119 个 `.rs` 文件，含 handlers/ admin/ e2ee/ extractors/ 等子目录，Axum 路由装配层）
- 依据规则：`.trae/rules/project_rules.md` §八（API 设计规范）、`AGENTS.md`（route_ledger 同步要求）
- 方法：机械扫描（认证提取器 / State<AppState> / DefaultBodyLimit / 中间件链）+ 2 个 Explore agent 深挖认证覆盖率、限流覆盖与 ledger 完整性，均带 file:line 证据
- 一句话结论：**这是四份审查里最干净的一层。认证提取器模式系统化落地（零 State<AppState> 越权、零认证缺口）、错误响应集中在 ApiError 统一产出 `{errcode, error}`、route_ledger 全量登记且启动时强校验、CORS/CSRF/安全头/限流全局挂载——唯一真债是限流默认值：登录/注册/验证码有紧限流，但改密/刷新令牌/密钥上传/媒体上传/3pid 全部落到 10/s 的默认桶，对敏感操作偏松（P2，可配置）。无 P0/P1。**

## 总览（按严重度）

| 级别 | 数量 | 类别 |
|------|------|------|
| P0 | 0 | 无 |
| P1 | 0 | 无 |
| P2 | 2 类 | 敏感端点限流默认值偏松、~5% 路由用手动 token 校验而非提取器 |
| ✅ 合规 | 6 项 | 认证中间件、错误格式、ledger 登记、context 提取器、上传限制、CORS/CSRF/安全头 |
| N/A | 1 项 | WebSocket（本服务无 WS 端点） |

---

## 1. 认证中间件覆盖（关注点 1 / 规则 8.2）—— ✅ 合规

结论：**认证提取器模式系统化，无缺口。** 本项目不用 blanket `.route_layer(auth_middleware)`，而是**逐 handler 提取器**：handler 签名带 `AuthenticatedUser`（`extractors/auth.rs:73-108` 的 `FromRequestParts` 在进入 handler 前校验 Matrix access token，失败即 401）。domain context 提取器（`RoomContext`/`AdminContext`/`MediaContext`/`FederationContext`/`DeviceContext`/`E2eeRoomContext`，`context.rs`）也实现 `AuthenticatedUser`，取其一即已认证。

- **模式 A（~95%）**：handler 取 `AuthenticatedUser` 参数。例：`account_data.rs:131 get_account_data`、`device.rs:197 get_device`、`e2ee/keys.rs:129 claim_keys`、`key_backup.rs`（33 handler 全覆盖）、`admin/*`（更强的 `AdminUser`）。
- **模式 B（~5%）**：handler 体内显式 `bearer_token(&headers)? + validate_token()?`，`?` 保证无效 token 在调 service 前被拒。例：`handlers/sync.rs:55-56`、`handlers/room/members.rs:22-23`（join）、`directory_reporting.rs:56-57`。

正确放行的公开端点（符合 Matrix spec）：login/register/refresh/logout、`/versions`、`/capabilities`（用 `OptionalAuthenticatedUser`）、`.well-known`、health、captcha、3pid 校验、公开房间目录。

| 文件:行 | 级别 | 问题 | 修复建议 |
|---------|------|------|---------|
| `handlers/sync.rs:55`、`handlers/room/members.rs:22`、`directory_reporting.rs:56` | P2（风格） | ~5% handler 用手动 `bearer_token+validate_token` 而非 `AuthenticatedUser` 提取器。功能等价（`?` 保证拒绝），但不统一，容易漏 | 重构为 `AuthenticatedUser` 提取器，与其余 95% 一致，降低未来漏加的风险 |

## 2. 错误响应格式 {errcode, error}（关注点 2 / 规则 8.3）—— ✅ 合规

结论：**集中产出，天然合规。** 全部错误走 `ApiError` → `IntoResponse`，在 `synapse-common/src/error.rs:1011-1013` 统一产出 `{"errcode": ..., "error": ...}`（并按需附 `retry_after_ms`，:1017），正是 Matrix 标准格式。路由 handler 只需 `return Err(ApiError::...)`，响应体自动正确。全目录仅 3 个文件手动构造 `errcode`（都属特殊场景），无 handler 自造非标准错误体。

## 3. route_ledger 登记（关注点 3 / AGENTS.md:130）—— ✅ 合规

结论：**全量登记 + 启动强校验，无漂移。** `assembly.rs` 中每个 `.merge()`/`.nest()` 都有对应 manifest：
- 内联路由 → `top_level_inline_manifest()`（:103）；compat 子路由 → `assembly_compat_manifest()`（:147）
- ~30 个独立路由模块各导出 `*_route_manifest()`，汇入 `base_route_manifest()`（:39）
- 状态感知/feature-gated 模块（room/federation/oidc/saml/cas/burn_after_read/widget/friend/voice…）经 `RouteModule::manifest_for_profile`（`route_module.rs:177-367`）

校验**实活**：`create_router` 在 `assembly.rs:328` 调 `declared_route_manifest_for(&state)`，:329 `ledger.validate()`，发现重复 `(method, path)` 即 `std::process::exit(1)`（:349），每次启动无条件跑。反向检查由 `tests/integration/api_route_ledger_tests.rs`（PATCH 探测每条 manifest 是否有活路由）兜底。无 mounted-but-unmanifested 路由。

## 4. domain context 提取器 vs State<AppState>（关注点 4 / 硬约束）—— ✅ 合规

结论：**零越权，已全量迁移。** 全目录 `State<AppState>` 仅 2 处命中，**均在注释里**（`room_access.rs:21`、`extractors/auth.rs:181` 描述迁移的注释），**没有任何 handler 直接取 `State<AppState>`**。domain context 提取器齐备（`context.rs:42 RoomContext`、`:317 AdminContext`，均 `impl FromRef<AppState>`），67 个文件用 `AuthenticatedUser`。硬约束满足得很干净。

## 5. WebSocket 连接泄漏（关注点 5）—— N/A

结论：**无 WebSocket 端点。** 全目录零 `WebSocketUpgrade`/`on_upgrade`。Matrix 客户端-服务器 API 用长轮询 `/sync`（`handlers/sync.rs`）而非 WS，没有 WS 连接可泄漏。此关注点不适用。

## 6. 文件上传大小限制（关注点 6）—— ✅ 合规

结论：**上传路由有显式 body 限制。** `media.rs:32` `/upload` 挂 `DefaultBodyLimit::max(50 * 1024 * 1024)`（50MB），:52 另一路由 50MB，:58 缩略图 10MB。联邦事务体限制 `middleware/federation_auth.rs:63` `max_transaction_payload.max(64KB)`。配置侧 `server.max_upload_size`（`media.rs:546` 通过 `m.upload.size` 回报客户端）。上传面有限制。

## 7. 限流覆盖敏感端点（关注点 7）—— P2

结论：**全局挂载，但敏感端点默认值偏松。** `rate_limit_middleware` 在 `assembly.rs:487` 作为全局 `.layer()` 挂在装配完成的 router 上，覆盖 :481 之前所有 merge/nest 的路由（仅 :490 之后的 Swagger UI 逃逸——非敏感）。采用两源配置：运行时 `RateLimitConfig` + 可热重载的文件 `RateLimitConfigFile`，按最长前缀匹配选规则。

| 端点 | 覆盖 | 限流 |
|------|------|------|
| `/login` | ✅ 紧 | 1/s，burst 3 |
| `/register` | ✅ 紧 | 1/s，burst 2 |
| `/register/captcha` | ✅ 紧 | 1/s，burst 1 |
| `/account/password` 改密 | ⚠ 仅默认 | 10/s，burst 20 |
| token `/refresh` | ⚠ 仅默认 | 10/s，burst 20 |
| `/keys/upload` | ⚠ 仅默认 | 10/s，burst 20 |
| media `/upload` | ⚠ 仅默认 | 10/s，burst 20 |
| 3pid/email requestToken | ⚠ 仅默认 | 10/s，burst 20 |

联邦端点额外过 per-origin `federation_rate_limit_middleware`（`federation/mod.rs:306`）。**注意**：所有 `/sync` 无条件豁免限流（`middleware/rate_limit.rs:11-20`）——长轮询语义决定，合理但需知悉。

| 级别 | 问题 | 修复建议 |
|------|------|---------|
| **P2** | 改密/刷新令牌/密钥上传/媒体上传/3pid 落到默认 10/s 桶，对敏感操作偏松（撞库改密、令牌刷新滥用、3pid 轰炸风险） | 在 `RateLimitConfigFile` 默认里给这 5 类加专属紧规则（参照 login 的 1/s burst 3）。已可运维配置，但默认值应把它们当安全关键项 |

## 8. CORS / CSRF / 安全头（关注点 8）—— ✅ 合规

结论：**全局中间件链完整。** `assembly.rs:482-489` 顺序挂载：`cors_middleware` → `security_headers_middleware` → `method_not_allowed_middleware` → `CompressionLayer`(SizeAbove 1024) → `csrf_middleware`(带 state) → `rate_limit_middleware`(带 state) → `shadow_ban_middleware` → `request_id_middleware`。CORS、CSRF、安全响应头、限流、请求 ID 全部全局覆盖。

---

## 修复优先级建议

1. **P2 限流默认值**（§7）——给改密/刷新/密钥上传/媒体上传/3pid 加专属紧规则。唯一有安全含义的项，且改动只在配置默认。
2. **P2 认证模式统一**（§1）——把 ~5% 手动 `bearer_token+validate_token` 的 handler（sync/join/directory_reporting）重构为 `AuthenticatedUser` 提取器，消除未来漏加认证的风险面。

## 合规亮点（这一层做得好）

- ✅ 认证提取器系统化，零 State<AppState> 越权，零认证缺口（agent 逐模块核 room/e2ee/key_backup/device/admin 全覆盖）。
- ✅ 错误格式集中 ApiError 统一产出 `{errcode, error}`，路由无需手写。
- ✅ route_ledger 全量登记 + 启动 `exit(1)` 强校验 + 集成测试反向兜底，无静默 merge。
- ✅ 上传有 DefaultBodyLimit，CORS/CSRF/安全头/限流全局挂载。

产出：docs/audit/05_web_routes_review.md
