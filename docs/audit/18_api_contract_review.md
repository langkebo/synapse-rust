# API 契约覆盖审查

> **日期**: 2026-07-14
> **分支**: `feat/architecture-optimization-round2`
> **审查范围**: insta 快照、versions/capabilities 声明、联邦 X-Matrix 鉴权/签名测试、测试文件清理状态

---

## 1. Insta 快照审查

### 1.1 现有快照清单（9 个）

| 快照文件 | 路由 | 状态码 | 类型 | `.redact()` 处理 |
|----------|------|--------|------|-----------------|
| `login_flows_v3.snap` | `GET /_matrix/client/v3/login` | 200 | flows 声明 | SSO `identity_providers` 手动替换为 `[redacted_sso_providers]` |
| `login_invalid_credentials_error.snap` | `POST /_matrix/client/v3/login` | 403 | 错误形状 | 无需（静态错误响应） |
| `register_flows_v3.snap` | `GET /_matrix/client/v3/register` | 200 | flows 声明 | 无需（静态 flows 列表） |
| `register_uia_401_challenge.snap` | `POST /_matrix/client/v3/register` | 401 | UIA 挑战 | `.redact(".session")` → `[redacted_session_uuid]` |
| `sync_unauthorized_without_token.snap` | `GET /_matrix/client/v3/sync` | 401 | 错误形状 | 无需（静态 M_UNAUTHORIZED） |
| `join_unauthorized_without_token.snap` | `POST /_matrix/client/v3/rooms/.../join` | 401 | 错误形状 | 无需（静态 M_UNAUTHORIZED） |
| `extended_profile_not_found.snap` | `GET /_matrix/client/unstable/uk.tcpip.msc4133/profile/...` | 404 | 错误形状 | 无需（静态 M_NOT_FOUND） |
| `versions_endpoint.snap` | `GET /_matrix/client/versions` | 200 | versions+unstable_features | 无需（纯声明，无动态字段） |
| `capabilities_v3.snap` | `GET /_matrix/client/v3/capabilities` | 200 | capabilities | `unstable_features` 值替换为 `[feature_gated]` |

### 1.2 `.redact()` 覆盖评估

TDD SKILL.md §5 要求对以下动态字段进行 `.redact()` 处理：
`access_token`, `refresh_token`, `expires_in`, `origin_server_ts`, `user_id` 后缀

| 动态字段 | 是否在现有快照中出现 | 处理方式 |
|----------|-------------------|---------|
| `access_token` | **未出现** — 所有 login/sync/join/profile 快照仅测未鉴权错误路径 | 无需处理 |
| `refresh_token` | **未出现** — 同上 | 无需处理 |
| `expires_in` | **未出现** — 同上 | 无需处理 |
| `origin_server_ts` | **未出现** — 无事件快照 | 无需处理 |
| `session` (UIA) | 出现在 `register_uia_401_challenge` | `.redact(".session")` 已正确处理 |
| `identity_providers` (SSO) | 出现在 `login_flows_v3` | 手动 inline 替换已正确处理 |
| `unstable_features` 布尔值 | 出现在 `capabilities_v3` | 手动 inline 替换为 `[feature_gated]` 已正确处理 |

### 1.3 快照盲区

| 盲区 | 优先级 | 影响 |
|------|--------|------|
| **Login 成功响应** (`POST /login` → 200) | **P0** | `access_token`/`refresh_token`/`expires_in`/`device_id`/`user_id` 响应形状未锁定。这是最关键的契约缺口 — 客户端解析 login 响应是最基础的集成路径 |
| **Sync 鉴权后增量轮询** (`GET /sync?timeout=0` with token) | **P0** | `next_batch`/`rooms`/`presence`/`account_data`/`to_device` 响应形状未锁定 |
| **Join 鉴权后成功** (`POST /join/{roomId}` with token) | **P1** | `room_id` 响应形状未锁定（虽简单，但是最频繁写操作） |
| **Profile 鉴权后查询** (`GET /profile/{userId}` with token) | **P1** | `displayname`/`avatar_url` 响应形状未锁定 |
| **Register 成功响应** (`POST /register` → 200) | **P1** | 注册成功响应与 login 响应字段类似（`access_token`/`user_id`/`device_id`），未锁定 |
| **消息发送响应** (`PUT /send/{txnId}` → 200) | **P2** | `event_id` 响应形状未锁定 |

**根因**: 现有快照测试全部针对**未鉴权/错误路径**，没有鉴权成功路径的快照。`setup_fresh_test_app()` 可以启动完整应用并注册用户，现有 `api_placeholder_contract_p0_tests.rs` 中的 helper（`register_user`/`create_room`/`send_message`）已经可用，但未用于 snapshot 测试。

---

## 2. Versions / Capabilities 声明一致性

### 2.1 Versions 声明

**代码**: `synapse-services/src/capability_governance.rs:71-93` — `CLIENT_API_VERSION_SUPPORT`  
**快照**: `versions_endpoint.snap` — 17 个版本

| 项目 | 代码声明 | 快照 | 一致？ |
|------|---------|------|--------|
| `r0.5.0` ~ `r0.6.1` | legacy | 存在 | 是 |
| `v1.1` ~ `v1.14` | stable | 存在 | 是 |
| `io.hula.*` 扩展 | 有意排除（非鉴权端点不应该暴露私有命名空间） | 不存在 | 是（符合保守声明规则） |

**保守声明合规**: v1.14 虽已声明，代码注释明确标注了一个已知缺口：`POST /v3/users/{userId}/report` (MSC4260) 未实现，但项目规则允许"在绝大多数特性支持时声明完整版本"。

### 2.2 Capabilities 声明

**代码**: `synapse-services/src/capability_governance.rs:428-512` — `build_capabilities_response()`  
**快照**: `capabilities_v3.snap`

**公共面**（未经鉴权）— 9 个 capability key：
`m.change_password`, `m.room_versions`, `m.set_displayname`, `m.set_avatar_url`, `m.3pid_changes`, `m.room.summary`, `m.room.suggested`, `m.voice`, `m.thread`

**鉴权面**（经鉴权）— 额外 7 个 capability key：
`io.hula.friends`, `m.sso`, `ai_connection`, `openclaw`, `external_services`, `io.hula.voice_extended`, `io.hula.burn_after_read`

**快照对齐验证**:
- 公共 capability key 全部在快照中出现
- 鉴权专属 key（如 `m.sso`、`io.hula.*`）未泄漏到公共快照（快照使用未鉴权请求）
- `unstable_features` 值用 `[feature_gated]` 占位 — 正确（避免 cargo feature flags 变化导致快照漂移）
- `io.element.msc4452.preview_url` 已声明（config-driven，默认为 `false`）

### 2.3 治理架构

所有 capability flag 均已从旧的 `StaticStable` 迁移到两种治理模式（`test_no_residual_static_stable_governance` — L889-919）：
- **RouteSurface**: 能力由路由注册状态决定（如 `m.room.summary` ← `GET /_matrix/client/v3/rooms/{room_id}/summary`）
- **ConfigControlled**: 能力由配置决定（如 `m.sso` ← `config.saml.enabled`）

**声明盲区**: 无。Capability 声明与路由 manifest 保持一致。

---

## 3. 联邦 X-Matrix 鉴权 + Canonical JSON 签名测试

### 3.1 找到的测试向量

#### 纯逻辑层（13 个单元测试 — `synapse-federation/src/signing.rs:220-503`）

| 测试 | 覆盖 |
|------|------|
| `test_sign_and_verify_json` | 签名 → 验证完整循环（Ed25519） |
| `test_verify_tampered_json_fails` | 篡改内容后验签失败 |
| `test_canonical_json_deterministic` | 键排序确定性 |
| `test_sign_federation_request` | `canonical_federation_request_bytes` with/without content |
| `test_sign_json_rejects_integer_valued_float` | 浮点数拒绝（Matrix spec 合规） |
| `test_verify_expired_key_fails` | 过期 key 验签失败 |
| `test_sign_with_old_key` | 多 key 签名叠加 |
| `test_compute_event_content_hash` | 内容哈希计算 |
| `test_verify_event_content_hash_valid` | 哈希验证成功 |
| `test_verify_event_content_hash_mismatch` | 哈希不匹配 |
| `test_check_pdu_size_limits_valid/too_large` | PDU 大小限制 |
| `test_check_event_federate` | federation 标志检查 |
| `test_canonical_json_types` | null/bool/number/string/array 序列化 |

#### 集成测试层 — `tests/integration/api_federation_tests.rs` (455 行)

| 测试 | 覆盖 |
|------|------|
| `test_federation_version` | `GET /_matrix/federation/v1/version` |
| `test_federation_query_directory_*` (2) | 签名联邦请求 → 查询目录别名 |
| `test_federation_key_clone_returns_server_keys` | 签名 POST → 密钥克隆 |
| `test_remote_key_query_fetches_real_remote_server_response` | **最完整的签名测试** — 生成 Ed25519 key、构建 canonical JSON body、签名、预填充 cache、验证返回 |
| `test_server_keys_endpoint_returns_verify_keys_without_config_signing_key` | `/key/v2/server` 端点 |
| `test_local_key_query_reuses_server_key_response` | `/key/v2/query` 本地查询 |

`api_federation_tests.rs` 中的 `signed_federation_request()` helper (L41-63) 正确实现了：
```rust
X-Matrix origin="{origin}",key="{key_id}",sig="{sig_b64}"
```
签名数据 = `canonical_federation_request_bytes(method, uri, origin, destination, content)`

#### 联邦 auth chain 测试 — `tests/integration/federation_error_tests.rs` (254 行)

| 测试 | 覆盖 |
|------|------|
| `test_invalid_signature_error` | room_id 不匹配拒绝 |
| `test_missing_auth_event` | 缺少 auth event 拒绝 |
| `test_room_id_mismatch` | room_id 不一致拒绝 |

### 3.2 签名测试盲区

| 盲区 | 优先级 | 描述 |
|------|--------|------|
| **`POST /_matrix/federation/v1/send/{txnId}`** | **P0** | 联邦入站 PDU 的**完整热路径**（签名验证 → auth chain 检查 → state resolution → 事件持久化）无任何集成测试。这是 G4 门禁对应的端点（见 `docs/audit/17_perf_baseline.md`） |
| **`canonical_json.rs` 独立单元测试** | **P1** | `synapse-common/src/canonical_json.rs` 无自身测试文件；所有 canonical JSON 测试嵌入在 `signing.rs` 中。缺少边界情况测试：Unicode 转义（U+2028/U+2029/U+FFFD）、嵌套对象排序、超大数字拒绝 |
| **PDU `signatures` 多层验证** | **P1** | 现有测试只验证单层签名（origin server → self）。缺少：多服务器签名链验证、`signatures` 字段篡改、`unsigned` 字段不影响签名 |
| **X-Matrix header 解析错误** | **P2** | 畸形 Authorization header（缺少 key/sig、非法 base64、不匹配的 origin）的拒绝测试 |

### 3.3 Canonical JSON 实现审查

`canonical_json()` (`synapse-common/src/canonical_json.rs:11-52`)：
- 键排序：`keys.sort()` (L32) — 标准 Rust BTree 排序（ASCII/Unicode 序，符合 Matrix spec）
- 字符串转义：`escape_canonical_string()` (L60-80) 正确转义 U+2028/U+2029/U+FFFD
- 数字：`format_canonical_number()` 拒绝浮点数和超大整数（符合 spec `[-(2^53)+1, 2^53-1]` 范围）
- 去除 `unsigned` + `signatures` 后签名（`sign_json()` L46）

**验证**: 两种 canonical JSON 形式均已正确实现且测试覆盖：
1. **通用 canonical JSON** (`canonical_json()`) — 用于联邦请求签名
2. **事件 canonical JSON** (`CanonicalEvent::from_event()`) — 用于事件签名（去除 `unsigned`/`signatures`，与通用形式有细微差异）

---

## 4. `api_placeholder_contract_p0_tests.rs` 清理状态

### 4.1 清理检查

| 检查项 | 结果 |
|--------|------|
| 文件行数 | 1070 行 |
| `dbg!()` 残留 | **0** |
| `println!()` 残留 | **0** |
| `eprintln!()` 残留 | **0** |
| 其他调试输出 | **0**（`grep -c` 返回 0） |

**结论**: 文件已完全清理，无历史遗留调试输出。

### 4.2 测试覆盖概览（18 个测试）

| 测试 | 覆盖路由 | 状态 |
|------|---------|------|
| `test_push_rules_scope_contract_rejects_non_global_scope` | `GET /pushrules/{scope}` | PASS |
| `test_directory_room_alias_contract_returns_not_found_for_missing_alias` | `GET /directory/room/{alias}` | PASS |
| `test_account_data_contract_returns_not_found_for_missing_custom_type` | `GET /user/{userId}/account_data/{type}` | PASS |
| `test_room_key_distribution_contract_*` (3 tests) | `POST /room_keys/request` | PASS |
| `test_change_password_uia_rejects_dummy_auth` | `POST /account/password` | PASS |
| `test_password_reset_email_flow_consumes_sid_after_success` | 密码重置邮件流程 | PASS |
| `test_key_rotation_management_contract_rejects_client_access` | 密钥轮换管理 | PASS |
| `test_admin_server_placeholder_contract_returns_not_implemented_for_admin` | Admin placeholder | PASS |
| `test_admin_experimental_features_returns_feature_map` | 实验性 features | PASS |
| `test_thirdparty_contract_rejects_builtin_irc_placeholders` | 第三方协议 | PASS |
| `test_report_room_contract_returns_success_payload` | `POST /rooms/{roomId}/report` | PASS |
| `test_sync_events_contract_surfaces_service_errors` | Sync 事件 | PASS |
| `test_room_event_keys_contract_rejects_invalid_event_id` | `GET /rooms/{roomId}/event/{eventId}` | PASS |
| `test_room_thread_contract_*` (2 tests) | Thread 端点 | PASS |
| `test_room_initial_sync_contract_returns_state_members_and_messages` | 初始同步 | PASS |
| `test_removed_private_room_placeholder_routes_return_404` | 私有房间 placeholder | PASS |
| `test_receipt_contract_rejects_invalid_event_id_and_receipt_type` | Receipt 端点 | PASS |

---

## 5. 缺口清单与优先级

### P0（阻塞性缺口 — 建议本回合修复）

| ID | 缺口 | 文件 | 影响 |
|----|------|------|------|
| **GAP-01** | Login 成功响应无 snapshot | `tests/integration/api_route_snapshots_tests.rs` | `access_token`/`refresh_token`/`expires_in`/`device_id`/`user_id` 响应形状漂移无检测 |
| **GAP-02** | Sync 鉴权后响应无 snapshot | 同上 | `next_batch`/`rooms`/`presence`/`account_data` 响应形状漂移无检测 |
| **GAP-03** | `POST /_matrix/federation/v1/send/{txnId}` 无集成测试 | 新文件或 `api_federation_tests.rs` | 联邦入站 PDU 热路径（G4 门禁）零测试覆盖 |

### P1（重要缺口 — 建议后续回合修复）

| ID | 缺口 | 文件 |
|----|------|------|
| **GAP-04** | Join 成功响应无 snapshot | `api_route_snapshots_tests.rs` |
| **GAP-05** | Profile 鉴权后查询无 snapshot | `api_route_snapshots_tests.rs` |
| **GAP-06** | Register 成功响应无 snapshot | `api_route_snapshots_tests.rs` |
| **GAP-07** | `canonical_json.rs` 无独立单元测试（Unicode 转义、超大数字、嵌套排序） | 新 `tests/unit/canonical_json_tests.rs` |
| **GAP-08** | 多服务器签名链验证无测试 | `federation/signing.rs` |
| **GAP-09** | `POST /join` 鉴权成功响应无 snapshot | `api_route_snapshots_tests.rs` |

### P2（低优先级 — 可延后）

| ID | 缺口 | 文件 |
|----|------|------|
| **GAP-10** | `PUT /send/{txnId}` 响应无 snapshot | `api_route_snapshots_tests.rs` |
| **GAP-11** | X-Matrix header 畸形输入拒绝测试 | `api_federation_tests.rs` |
| **GAP-12** | Event redaction 契约测试（redact → 验证保留字段） | 新测试文件 |

---

## 6. 汇总

| 维度 | 状态 |
|------|------|
| Insta 快照总数 | 9 个 |
| `.redact()` 处理 | 正确（手动 inline 2 处 + `.redact()` 1 处），静态字段无需处理 |
| 错误路径快照覆盖 | 达标（login/register/sync/join/profile 的错误路径均已锁定） |
| **鉴权成功路径快照** | **缺失** — 6 个盲区（GAP-01/02/04/05/06/09） |
| Versions 声明与实现一致性 | 通过 — 17 个版本，v1.14 缺口已文档化 |
| Capabilities 声明与实现一致性 | 通过 — 公共/鉴权分离正确，route-surface 治理 100% |
| Canonical JSON 签名单元测试 | 13 个测试，覆盖签名/验签/篡改/哈希/PDU 限制 |
| **联邦 send_transaction 集成测试** | **缺失** — P0 盲区（GAP-03） |
| `api_placeholder_contract_p0_tests.rs` 清理 | **通过** — 0 调试输出残留 |
| Route manifest 覆盖 | 通过 — 所有 admin 模块、所有路由模块均有 manifest |
| Route ledger snapshot | 1229 条路由（default）/ 1387 条（worker enabled）— 完整 |
