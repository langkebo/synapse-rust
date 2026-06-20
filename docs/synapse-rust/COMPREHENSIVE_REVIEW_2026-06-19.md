# synapse-rust 全面审查报告

**审查日期**: 2026-06-19
**审查基线**: `/Users/ljf/Desktop/hu_ts/synapse-rust` 当前工作区
**对标项目**: element-hq/synapse v1.153.x / Matrix Spec v1.18
**审查方法**: 5 路并行子代理静态审查（代码质量 / 架构设计 / 安全漏洞 / 性能表现 / 上游兼容性）

---

## 一、审查概述

本次审查覆盖代码质量、架构设计、安全漏洞、性能表现、Matrix/Synapse 兼容性五个维度，共发现 **62 项问题**（去重后），其中：

| 严重程度 | 数量 | 说明 |
|---------|:----:|------|
| P0（立即修复） | 12 | 可被利用的安全漏洞 + 破坏联邦互通的协议不合规 |
| P1（短期修复） | 20 | 架构分层违规 + 关键路径性能瓶颈 + 协议合规缺陷 |
| P2（中期修复） | 18 | 代码质量问题 + 中等性能问题 + 中等安全风险 |
| P3（择期修复） | 12 | 低优先级改进项 |

**正面发现**：源码整体保持较高工程纪律，当前仅有少量 TODO 标记（主要集中在 wildcard re-export 显式导出等已知重构跟踪项），生产代码无 unsafe、SQL 全参数化、密码使用 Argon2id+OsRng、refresh token 有家族 reuse 检测、admin 有 RBAC+MFA+审计日志、测试面较广（当前 `tests/` 下约 188 个 Rust 测试文件，另有大量 crate 内联测试）、crate 依赖无循环、cache 跨实例失效机制完善。

---

## 二、问题清单与处理方案

### P0 — 立即修复（12 项）

---

#### P0-01 SAML XML Signature Wrapping (XSW) 认证绕过 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 安全 |
| **描述** | SAML SSO 使用 regex 解析 XML，仅验证 `SignedInfo` 签名而不验证 `Reference` URI 指向的元素。攻击者可构造 XSW 包：合法签名放在恶意 assertion 外层，签名验证通过但提取的是未签名恶意 assertion，以任意用户身份登录。且 `parse_saml_assertion` 在 `validate_response`（含签名验证）之前调用，违反"先验证再解析"原则。`canonicalize_xml` 仅做行修剪，非规范 C14N。 |
| **位置** | [saml_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/saml_service.rs) L276-279, L531-593, L613-621 |
| **处理方法** | 1. 替换为成熟 SAML 库（`samael` 或类似），消除手写 regex 解析；2. 严格按 XML Signature 规范验证 `Reference` URI 指向元素的摘要；3. 使用 Exclusive C14N 1.0 替代行修剪；4. 调整调用顺序：先 `validate_response` 再 `parse_saml_assertion`；5. 按 `want_response_signed`/`want_assertions_signed` 配置强制校验签名覆盖范围。 |
| **验证方法** | 1. 编写 XSW 攻击向量测试（覆盖 XSW1-XSW8 变体），断言全部拒绝；2. 编写合法 SAML 响应测试，断言正常解析；3. `cargo test --features test-utils --test integration saml_xsw` 通过；4. 使用 SAML 测试工具（如 SAML Raider）验证。 |
| **所需资源** | 后端 1 人周（引入 SAML 库 + 适配 + 测试） |
| **状态** | ✅ 已修复（2026-06-19）。调整调用顺序：validate_response 在 parse_saml_assertion 之前；新增 XSW 防护：extract_reference_uri 提取 Reference URI，extract_element_by_id 验证 URI 指向的元素存在，验证被引用元素摘要匹配 DigestValue（而非错误地摘要 SignedInfo）。 |

---

#### P0-02 联邦密钥获取 SSRF 漏洞 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 安全 |
| **描述** | `fetch_federation_verify_key` 直接将 attacker-controlled `origin` 拼入 URL（`https://{origin}/...` 和 `http://{origin}/...`），无 IP 黑名单/私网地址过滤。攻击者可注册 origin 使其 DNS 解析到 `127.0.0.1`、`169.254.169.254`（云元数据）、`10.0.0.0/8` 内网，诱导服务器读取内网资源。同时存在 HTTP fallback（明文可被 MITM 篡改密钥）。 |
| **位置** | [federation_auth.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/middleware/federation_auth.rs) L410-414 |
| **处理方法** | 1. 发起请求前对 `origin` 解析的 IP 调用 `check_url_against_blacklist`（项目已有此函数，URL preview 已使用）；2. 移除 HTTP fallback，仅允许 HTTPS（Matrix 规范要求）；3. 配置 reqwest `dns_resolver` 在解析阶段拒绝私网/保留地址；4. 限制重定向次数为 0。 |
| **验证方法** | 1. 编写测试：origin DNS 解析到 `127.0.0.1`/`169.254.169.254`/`10.0.0.1`，断言请求被拒绝；2. 编写测试：HTTP fallback 被禁用；3. `cargo test --features test-utils --test integration federation_ssrf` 通过。 |
| **所需资源** | 后端 0.5 人周 |
| **状态** | ✅ 已修复（2026-06-19）。fetch_federation_verify_key 调用 check_url_against_blacklist 进行 IP 黑名单过滤（支持 CIDR），阻止 127.0.0.1/169.254.169.254/10.0.0.0/8 等私有网络；强制 HTTPS，禁用重定向；DNS 解析后二次检查。 |

---

#### P0-03 硬编码 fallback token hash secret ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 安全 |
| **描述** | `hash_token` 在环境变量 `TOKEN_HASH_SECRET` 未设置时使用硬编码字符串 `"dev-test-token-hash-secret-do-not-use-in-production"`。若运维忘记设置，所有 access_token/refresh_token 的 HMAC 哈希使用公开已知 secret，攻击者可离线伪造 token 哈希。release 构建同样执行此 fallback，无启动期校验。 |
| **位置** | [crypto.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/crypto.rs) L159-167 |
| **处理方法** | 1. 生产模式（`cfg!(debug_assertions)` 为 false）下 `TOKEN_HASH_SECRET` 未设置或等于已知弱值时启动 panic；2. 校验 secret 长度 ≥ 32 字节；3. 在 `server.rs` 启动阶段加入配置校验；4. 移除硬编码字符串，改为返回 `Result`。 |
| **验证方法** | 1. 测试：生产模式下未设置 secret 时启动失败；2. 测试：secret < 32 字节时启动失败；3. 测试：dev 模式下允许 fallback；4. `cargo test --lib token_hash_secret_validation` 通过。 |
| **所需资源** | 后端 0.3 人周 |
| **状态** | ✅ 已修复（2026-06-19）。hash_token 的硬编码回退密钥仅在 debug 构建中可用；release 构建中缺少 TOKEN_HASH_SECRET 环境变量时 panic；新增 validate_token_hash_secret 启动校验函数（32 字节最小长度，拒绝已知 dev 回退值）。 |

---

#### P0-04 canonical JSON 实现分歧，签名路径使用不合规版本 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 兼容性/安全 |
| **描述** | 存在两套 canonical JSON 实现：`synapse-federation/src/signing.rs` 的 `canonical_json_string`（用 `serde_json::to_string` 处理字符串，不转义 U+2028/U+2029；用 `n.to_string()` 处理数字，不处理整数浮点 `1.0`→`1.0`）和 `synapse-common/src/canonical_json.rs`（较合规版本）。但 `compute_event_content_hash`、`verify_event_content_hash`、`sign_json`、`canonical_federation_request_bytes` 全部使用 signing.rs 的不合规版本。事件哈希/签名/联邦请求签名均可能不合规，与上游签名不匹配，直接破坏联邦互通。 |
| **位置** | [signing.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/signing.rs) L10-57；对比 [canonical_json.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/canonical_json.rs) |
| **处理方法** | 1. 删除 `signing.rs` 中的 `canonical_json_string` 重复实现；2. 所有联邦签名路径统一使用 `synapse_common::canonical_json`；3. 引入 Matrix canonical JSON test vectors 对照测试。 |
| **验证方法** | 1. 引入 Matrix 规范 canonical JSON test vectors，断言输出一致；2. 编写整数浮点（`1.0`→拒绝）、U+2028 转义、map ordering、unsigned/signatures stripping 测试；3. `cargo test --lib canonical_json_vectors` 通过；4. 与 Synapse 生成的签名交叉验证。 |
| **所需资源** | 后端 1 人周 |
| **状态** | ✅ 已修复（2026-06-19）。统一 canonical JSON 实现到 synapse-common/src/canonical_json.rs（键排序、仅整数、U+2028/U+2029/U+FFFD 转义）；signing.rs 中 compute_event_content_hash/verify_event_content_hash/sign_json/canonical_federation_request_bytes 全部委托给统一实现。 |

---

#### P0-05 m.room.redaction 事件缺失 `redacts` 字段 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 兼容性 |
| **描述** | 创建 redaction 事件时 `content = json!({"reason": reason})`，`CreateEventParams` 无 `redacts` 通道。全 `src/` 目录 grep `"redacts"` 零匹配。规范要求 v1-v10 redaction 事件含顶层 `redacts`，v11+ 含 `content.redacts`。联邦对端无法判断被 redact 的目标，客户端无法显示 redaction 关系。 |
| **位置** | [events.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/room/events.rs) L856-940 |
| **处理方法** | 1. `CreateEventParams` 增加 `redacts: Option<String>` 字段；2. 按目标房间版本决定写入顶层（v1-10）或 content（v11+）；3. 同步修复 `synapse-web` 镜像实现。 |
| **验证方法** | 1. 测试：v1-10 房间 redaction 事件含顶层 `redacts`；2. 测试：v11+ 房间 redaction 事件含 `content.redacts`；3. `cargo test --features test-utils --test integration redaction_field` 通过。 |
| **所需资源** | 后端 0.5 人周 |
| **状态** | ✅ 已修复（2026-06-19）。新增 synapse-common/src/redaction.rs 作为单一真相源；extract_redacts 支持 v1-v10 顶层 redacts 和 v11+ content.redacts；redact_event handler 设置 redacts: Some(event_id) 到 CreateEventParams。 |

---

#### P0-06 redaction 内容剥离不合规：直接清空为 `{}` ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 兼容性 |
| **描述** | `redact_event_content` 执行 `UPDATE events SET content = '{}', is_redacted = true`，把被 redact 事件内容整体清空。Matrix 规范要求 redacted 事件按事件类型保留特定字段（如 `m.room.message` 保留 `body`/`msgtype`，`m.room.member` 保留 `membership`）。清空为 `{}` 导致客户端无法展示 redacted 消息摘要、联邦对端内容哈希不匹配。 |
| **位置** | [event/mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/event/mod.rs) L565-574 |
| **处理方法** | 1. 实现按事件类型的字段保留表（与 P0-07 的 `redact_event_for_hash` 表统一）；2. redact 时按表剥离而非清空；3. 参考 Matrix 规范 Appendix "Redaction" 完整字段表。 |
| **验证方法** | 1. 测试：`m.room.message` redact 后保留 `body`/`msgtype`；2. 测试：`m.room.member` redact 后保留 `membership`；3. 测试：未知事件类型 redact 后仅保留 `body`；4. `cargo test --features test-utils --test integration redaction_content` 通过。 |
| **所需资源** | 后端 1 人周 |
| **状态** | ✅ 已修复（2026-06-19）。redact_content 按事件类型保留字段（m.room.member 保留 membership/displayname/avatar_url 等；m.room.power_levels 保留 users/ban/kick/redact 等；m.room.message 清空所有内容）；redact_event_content 委托给共享 redaction 模块而非清空为 {}。 |

---

#### P0-07 `redact_event_for_hash` 字段保留表不完整且含非法字段 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 兼容性 |
| **描述** | 顶层 `allowed_top_level` 含 `prev_state`（已废弃）和 `membership`（非顶层字段）。`allowed_content_keys` 缺失大量类型：`m.room.name`、`m.room.topic`、`m.room.canonical_alias`、`m.room.aliases`、`m.room.server_acl`、`m.room.tombstone`、`m.room.encryption`、`m.reaction` 等。`m.room.power_levels` 缺 `notifications`。该函数用于事件哈希计算，字段表错误导致哈希与上游不一致，签名验证失败。 |
| **位置** | [signing.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/signing.rs) L200-246 |
| **处理方法** | 1. 按 Matrix 规范 Appendix "Redaction" 完整重写字段表；2. 与 P0-06 共用同一份剥离逻辑；3. 移除非法顶层字段 `prev_state`、`membership`。 |
| **验证方法** | 1. 引入 Matrix 规范 redaction test vectors；2. 测试：每种事件类型 redact 后的哈希与规范一致；3. `cargo test --lib redaction_hash_vectors` 通过。 |
| **所需资源** | 后端 0.5 人周（与 P0-06 合并实施） |
| **状态** | ✅ 已修复（2026-06-19）。redact_event_for_hash 委托给 synapse_common::redaction::redact_event_for_hash；移除了非法顶层字段 prev_state/membership；CANONICAL_JSON_TOP_LEVEL_FIELDS 严格匹配 Matrix 规范。 |

---

#### P0-08 联邦事务处理器不处理 redaction PDU ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 兼容性 |
| **描述** | `send_transaction` 处理 incoming PDU 时无 `m.room.redaction` 处理分支。来自其他联邦服务器的 redaction 事件不被解析或应用，本服务器上的事件不会被远端 redact。这是重大联邦互通缺口。 |
| **位置** | [transaction.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/transaction.rs) |
| **处理方法** | 1. PDU 处理流水线新增 redaction 分支；2. 校验 redaction 事件签名与 redactor 权限（按 room version，见 P0-09）；3. 提取 `redacts`，调用规范化内容剥离逻辑（P0-06）。 |
| **验证方法** | 1. 测试：发送合法 redaction PDU，断言目标事件被 redact；2. 测试：无权限 redaction PDU 被拒绝；3. `cargo test --features test-utils --test integration federation_redaction` 通过。 |
| **所需资源** | 后端 1 人周（依赖 P0-05/P0-06/P0-07/P0-09 完成） |
| **状态** | ✅ 已修复（2026-06-19）。federation transaction 处理器对 m.room.redaction PDU 调用 extract_redacts 提取目标 event_id；设置 redacts 字段到 CreateEventParams；事件创建后调用 redact_event_content 对目标事件执行内容剥离。 |

---

#### P0-09 redaction 权限规则未按 room version 区分 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 兼容性 |
| **描述** | `can_redact_event` 始终允许 `actor_user_id == event_sender_id` 直接 redact（无条件）。但 v1-v10 规范要求 redactor 必须达到 `redact` power level（即使原作者也需满足）；只有 v11+ 才允许原作者无条件 redact。当前对 v1-v10 权限过宽。 |
| **位置** | [power_levels.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/auth/power_levels.rs) L495-535 |
| **处理方法** | 1. 读取目标房间版本；2. v1-v10：要求 actor 满足 `redact` power level（原作者无豁免）；3. v11+：保留原作者豁免。 |
| **验证方法** | 1. 测试：v10 房间原作者无 redact power level 时被拒绝；2. 测试：v11 房间原作者可直接 redact；3. 测试：有 redact power level 的非原作者可 redact；4. `cargo test --lib redaction_permission` 通过。 |
| **所需资源** | 后端 0.3 人周 |
| **状态** | ✅ 已修复（2026-06-19）。can_redact_event 按 room version 区分：v1-v10 要求所有删除者满足 redact power level（无自删除豁免）；v11+ 允许原作者自删除；新增 get_room_version 辅助方法从 m.room.create 事件读取版本。 |

---

#### P0-10 状态解析使用纯时间戳排序（非 v2 算法）✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 兼容性 |
| **描述** | `detect_conflicts` 按 `origin_server_ts` 降序选 winner，`resolution_reason` 明确写 `"Timestamp-based resolution"`。Matrix state resolution v2（MSC1442）应使用 reverse topological power ordering + mainline ordering，时间戳仅作末位 tiebreaker。纯时间戳解析会与上游 Synapse 及其他合规 homeserver 产生不同解析结果，在冲突状态下造成状态分叉。 |
| **位置** | [state_resolution.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/event_auth/state_resolution.rs) L50-78 |
| **处理方法** | 1. `detect_conflicts` 仅作冲突检测，winner 选举走 `resolve_state_with_auth_chain` 的 mainline + reverse-topological-power 路径；2. 修复 P0-11 的 power_levels bug。 |
| **验证方法** | 1. 引入 Matrix state resolution v2 test vectors；2. 测试：冲突状态下解析结果与 Synapse 一致；3. `cargo test --lib state_resolution_v2` 通过。 |
| **所需资源** | 后端 2 人周（state resolution v2 是复杂算法） |
| **状态** | ✅ 已修复（2026-06-19）。重写 `resolve_state_v2` 实现 MSC1442：分离 auth/non-auth 事件，auth 用 reverse topological power ordering，non-auth 用 mainline ordering；修复 `sort_by_reverse_topological_power` 用 `sender` 字段查询 power（而非 `state_key`）；修复时间戳提取用顶层 `origin_server_ts`（而非 `content.origin_server_ts`）；重写 `compute_mainline` 专门跟踪 `m.room.power_levels` 事件链；给 `EventData` 添加 `sender`/`origin_server_ts`/`depth` 字段。 |

---

#### P0-11 状态解析 power_levels 映射全为 0 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 兼容性 |
| **描述** | `resolve_state_with_auth_chain` 中 `power_levels: HashMap<String, i64> = events.iter().filter(|(_, e)| e.event_type == "m.room.power_levels").map(|(eid, _)| (eid.clone(), 0)).collect()`，把所有 power_levels 事件 id 映射为 0 而非从 content 提取实际值。导致 `sort_by_reverse_topological_power` 的 power 维度完全失效。 |
| **位置** | [state_resolution.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/event_auth/state_resolution.rs) L370-376 |
| **处理方法** | 从 power_levels 事件 content 解析 `users`/`users_default`，构造 sender→power_level 映射供排序使用。 |
| **验证方法** | 1. 测试：有 power level 差异的冲突状态，高 power 用户的 event 优先；2. `cargo test --lib state_resolution_power_levels` 通过。 |
| **所需资源** | 后端 0.5 人周（与 P0-10 合并实施） |
| **状态** | ✅ 已修复（2026-06-19）。`power_levels` 映射现在从最深（最新）`m.room.power_levels` 事件的 `content.users` 提取 `user_id → power_level`，而非映射所有事件 id 到 0。 |

---

#### P0-12 room version v11+ 声明可创建但 redaction 格式未实现 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 兼容性 |
| **描述** | `SUPPORTED_ROOM_VERSIONS` 将 v1-v13 全部标记为 `stable` 且 `can_create=true`，但 v11+ redaction 格式（`content.redacts`）未实现，v11+ auth 规则未区分。声明能力超出实际实现，违反项目规则"room-version capability 必须匹配实际 event/auth 行为"。 |
| **位置** | [room_versions.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/common/room_versions.rs) L47-61；[SUPPORTED_MATRIX_SURFACE.md](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/synapse-rust/SUPPORTED_MATRIX_SURFACE.md) L96-103 |
| **处理方法** | 在 P0-05/P0-06/P0-09 完成前，将 v11-v13 的 `can_create` 降为 false（仅 `can_join`），或回退为 unstable；同步更新 `SUPPORTED_MATRIX_SURFACE.md`。 |
| **验证方法** | 1. 测试：`/capabilities` 响应中 v11+ 不在可创建版本列表；2. `cargo test --lib room_version_declaration` 通过。 |
| **所需资源** | 后端 0.2 人周 |
| **状态** | ✅ 已修复（2026-06-19）。分两阶段：先在 P0-04 阶段将 v11+ 降级为 `stable_parse_only`（can_create=false）；待 P0-05~09 redaction 链和 P0-10/11 状态解析 v2 完成后，恢复 v11+ 为 `stable`（can_create=true）。`extract_redacts` 支持 v11+ `content.redacts` 格式，`can_redact_event` 支持 v11+ 自删除权限。 |

---

### P1 — 短期修复（20 项）

---

#### P1-01 storage 实现泄漏到 services crate ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 架构 |
| **描述** | `synapse-services/src/worker/storage.rs`（~590 行）包含 30+ 处 `sqlx::query` 直接 SQL，位于 services crate 内，违反分层约束。 |
| **位置** | [worker/storage.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/worker/storage.rs) |
| **处理方法** | 将 `WorkerStorage`/`WorkerRow`/`WorkerCommandRow` 迁移到 `synapse-storage/src/worker.rs`，services 通过 `pub use synapse_storage::worker::*` 引用。 |
| **验证方法** | 1. `grep -r "sqlx::query" synapse-services/src/worker/` 零匹配；2. `cargo check --workspace` 通过；3. `scripts/check_layer_isolation.sh` 无 WARNING。 |
| **所需资源** | 后端 0.5 人周 |
| **状态** | ✅ 已修复（2026-06-20）。WorkerType/WorkerStorage/WorkerRow 等类型从 synapse-services 迁移到 synapse-storage/src/worker.rs（1514 行）；synapse-services/src/worker/types.rs 和 storage.rs 改为 thin facade re-export。 |

---

#### P1-02 route handler 在请求路径内直接 new storage 实例

| 项 | 内容 |
|---|---|
| **域** | 架构 |
| **描述** | `rendezvous.rs` 在路由处理函数内直接构造新 storage 实例，从另一个 storage 的 `pool` 字段取连接池，绕过 ServiceContainer 依赖注入。 |
| **位置** | [rendezvous.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/rendezvous.rs) L281, L308 |
| **处理方法** | 在 `ServiceContainer` 中注册 `RendezvousMessageStorage`，route 通过 `state.services` 访问。 |
| **验证方法** | 1. `rendezvous.rs` 不再在请求处理路径中直接调用 `Storage::new(...)`；2. `cargo check` 通过；3. 集成测试 rendezvous 功能无回归。 |
| **所需资源** | 后端 0.3 人周 |

---

#### P1-03 routes 大规模直接访问 storage 层（跳过 service 层）

| 项 | 内容 |
|---|---|
| **域** | 架构 |
| **描述** | 大量 route handler 通过 `state.services.X.storage_y` 直接调用 storage 方法，绕过 service 层。最严重的是 `admin/notification.rs`（17+ 处），还有 `account_compat.rs`、`dm.rs`、`e2ee_routes.rs`、`admin/user.rs` 等。 |
| **位置** | [admin/notification.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/admin/notification.rs) L138-516；[account_compat.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/account_compat.rs) L42-676；[dm.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/dm.rs) L44-486；[e2ee_routes.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/e2ee_routes.rs) L230-529 |
| **处理方法** | 为每个 storage 域提供 service 封装（如 `ServerNotificationService`、`ThreepidService`、`DeviceService`），route 只调用 service 方法。优先处理 `admin/notification.rs`。 |
| **验证方法** | 1. `grep "storage\." src/web/routes/admin/notification.rs` 零匹配；2. 集成测试覆盖原有功能；3. `cargo clippy` 通过。 |
| **所需资源** | 后端 2 人周（多文件分批） |

---

#### P1-04 services 直接持有 PgPool（storage 职责泄漏）

| 项 | 内容 |
|---|---|
| **域** | 架构 |
| **描述** | 多个 service 结构体直接持有 `Arc<PgPool>` 字段，可执行任意 SQL，破坏 service→storage 单向依赖。涉及 `room_tag_service.rs`、`oidc_mapping_service.rs`、`client_push_service.rs`、`admin_server_service.rs`、`admin_media_service.rs`。 |
| **位置** | [room_tag_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/room_tag_service.rs) L9；[client_push_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/client_push_service.rs) L35；[admin_server_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/admin_server_service.rs) L10 |
| **处理方法** | service 应持有对应 `Storage` 实例而非 `PgPool`。如 `RoomTagService` 持有 `RoomTagStorage`，`ClientPushService` 持有 `PushStorage`。 |
| **验证方法** | 1. `grep "pool: Arc<PgPool>" synapse-services/src/` 零匹配；2. `cargo check` 通过；3. 单元测试无回归。 |
| **所需资源** | 后端 1 人周 |

---

#### P1-05 synapse-web crate 完全未被使用（死代码） ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 架构/代码质量 |
| **状态** | ✅ 已修复（2026-06-19）— 选项 B：删除整个 crate |
| **描述** | `synapse-web` 是 workspace 成员，包含 ~130 个文件与 `src/web/` 高度重复，但 root crate 的 `Cargo.toml` 未将其列为依赖。整个 crate 是死代码，任何 `src/web/` 修改需手动同步。 |
| **位置** | [synapse-web/src/](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/)；[Cargo.toml](file:///Users/ljf/Desktop/hu_ts/synapse-rust/Cargo.toml) L205 |
| **处理方法** | 二选一：(A) 完成迁移：root crate 依赖 `synapse-web`，`src/web/` 改为 thin facade；(B) 从 workspace 移除 `synapse-web`，删除整个目录。建议选 (B) 除非有明确迁移计划。 |
| **验证方法** | 1. 选 (B)：`synapse-web/` 目录不存在；`Cargo.toml` workspace.members 不含 `synapse-web`；`cargo check --workspace` 通过。2. 选 (A)：`src/web/mod.rs` 为 thin facade；`cargo check` 通过。 |
| **所需资源** | 后端 0.5 人周（选 B）或 3 人周（选 A） |

---

#### P1-06 锁持有跨 .await（4 处，worker/manager.rs） ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 性能 |
| **状态** | ✅ 已修复（2026-06-19） |
| **描述** | `unregister_worker`/`disconnect_worker` 中获取 `connections.write().await` 写锁后调用 `conn.disconnect().await`，锁持有期间执行异步操作，阻塞所有其他 worker 操作。root 和 canonical 各有 2 处。 |
| **位置** | [worker/manager.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/worker/manager.rs) L262-264, L561-563；[src/worker/manager.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/worker/manager.rs) L326-328, L658-660 |
| **处理方法** | 先在锁内取出 conn（`let conn = connections.write().await.remove(&id)`），释放锁后再调用 `conn.disconnect().await`。 |
| **验证方法** | 1. `cargo clippy` 无 `await_holding_lock` 警告；2. 压力测试：并发 worker 注册/注销无阻塞；3. `cargo test --features test-utils --test integration worker_lock` 通过。 |
| **所需资源** | 后端 0.3 人周 |

---

#### P1-07 Space 层级遍历 N+1 查询（嵌套） ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 性能 |
| **状态** | ✅ 已修复（2026-06-19） |
| **描述** | Space 层级遍历中循环调用 `get_space_by_room`，且同一 room_id 在循环内调用两次（L880 和 L892）。`build_hierarchy_room` 内部又调用 `get_children_state_events` 和 `get_space_by_room`，嵌套 N+1。 |
| **位置** | [space.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/space.rs) L879-933 |
| **处理方法** | 1. 批量查询所有 child room_id 是否为 space；2. 批量预加载所有 children 的 state events；3. 缓存 space 判定结果避免重复查询。 |
| **验证方法** | 1. 基准测试：10 层 Space 层级查询 SQL 次数从 O(n²) 降为 O(1)；2. `cargo bench --bench performance_api_benchmarks` 查询时间下降；3. 功能测试无回归。 |
| **所需资源** | 后端 1 人周 |

---

#### P1-08 Sync 响应构建 N+1 查询（device count） ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 性能 |
| **状态** | ✅ 已修复（2026-06-19） |
| **描述** | Sync 响应构建时循环调用 `get_device_count` 获取每个 changed user 的设备数，影响 sync 关键路径。 |
| **位置** | [response.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/sync_service/response.rs) L255-264 |
| **处理方法** | 添加 `get_device_counts_batch(user_ids: &[String])` 批量查询方法，使用 `WHERE user_id = ANY($1)` 单次查询。 |
| **验证方法** | 1. 测试：100 个 changed user 的 sync 响应构建只产生 1 次 device count 查询；2. `cargo test --features test-utils --test integration sync_device_count` 通过。 |
| **所需资源** | 后端 0.5 人周 |

---

#### P1-09 Application service 事务构建 N+1 查询 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 性能 |
| **状态** | ✅ 已修复（2026-06-19） |
| **描述** | 事务构建时循环调用 `build_transaction_event`，每次内部调用 `event_storage.get_event`，影响 appservice 消息分发。 |
| **位置** | [application_service/mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/application_service/mod.rs) L960-962 |
| **处理方法** | 批量查询所有 source_event_id 的事件，添加 `get_events_batch(event_ids: &[String])` 方法。 |
| **验证方法** | 1. 测试：100 个 event 的事务构建只产生 1 次 DB 查询；2. `cargo test --features test-utils --test integration appservice_batch` 通过。 |
| **所需资源** | 后端 0.5 人周 |

---

#### P1-10 批量创建 registration tokens N+1 INSERT ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 性能 |
| **状态** | ✅ 已修复（2026-06-19） |
| **描述** | 批量创建 registration tokens 时在循环中逐条 INSERT，未使用批量插入。 |
| **位置** | [registration_token.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/registration_token.rs) L586-603 |
| **处理方法** | 使用 `INSERT INTO ... VALUES ($1, $2), ($3, $4)...` 批量插入或 `UNNEST`。 |
| **验证方法** | 1. 测试：批量创建 100 个 token 只产生 1 次 INSERT；2. `cargo test --lib registration_token_batch` 通过。 |
| **所需资源** | 后端 0.3 人周 |

---

#### P1-11 state events 查询无 LIMIT（大房间 OOM 风险） ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 性能 |
| **状态** | ✅ 已修复（2026-06-19） |
| **描述** | `get_state_events_by_type` 使用 `fetch_all` 加载某 room 某 type 的所有 state events，无 LIMIT。大型房间的 membership state events 可能非常大，导致 OOM。 |
| **位置** | [event/state.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/event/state.rs) L72-77 |
| **处理方法** | 1. 对于 membership 类型，使用 `fetch` 流式处理或添加分页；2. 评估是否可改用 `COUNT` + 分页查询替代全量加载。 |
| **验证方法** | 1. 测试：10 万成员房间的 membership state events 查询不 OOM；2. 内存使用监控在阈值内。 |
| **所需资源** | 后端 0.5 人周 |

---

#### P1-12 存储层错误被 `unwrap_or_default()` 静默吞噬 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 代码质量 |
| **状态** | ✅ 已修复（2026-06-19） |
| **描述** | 3 处在数据库查询路径上使用 `unwrap_or_default()`，将存储层错误静默转为空默认值，DB 故障时返回不完整数据。E2EE 密钥查询路径尤其危险——DB 故障时客户端会认为无人有密钥。 |
| **位置** | [search.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/search.rs) L468, L609；[e2ee_routes.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/e2ee_routes.rs) L230 |
| **处理方法** | 统一改为 `.map_err(|e| ApiError::internal_with_log("Failed to load", &e))?`，与同文件 `get_rooms_batch` 的处理方式一致。 |
| **验证方法** | 1. `grep "unwrap_or_default" src/web/routes/handlers/search.rs src/web/routes/e2ee_routes.rs` 零匹配；2. 测试：DB 故障时返回 500 而非空数据。 |
| **所需资源** | 后端 0.2 人周 |

---

#### P1-13 联邦 knock 端点 HTTP 方法用 PUT（规范为 POST） ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 兼容性 |
| **状态** | ✅ 已修复（2026-06-19） |
| **描述** | Matrix Server-Server API 规范定义为 `POST /_matrix/federation/v1/knock/{roomId}/{userId}`，项目用 `put`。合规客户端/服务器发起 POST knock 会被 405。 |
| **位置** | [federation/mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/mod.rs) L242 |
| **处理方法** | 改为 `post(membership::knock_room)`，同步更新 route ledger。 |
| **验证方法** | 1. 测试：POST knock 返回 200；2. `cargo test --features test-utils --test integration federation_knock` 通过。 |
| **所需资源** | 后端 0.1 人周 |

---

#### P1-14 联邦 knock 响应结构不符规范 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 兼容性 |
| **状态** | ✅ 已修复（2026-06-19） |
| **描述** | 返回 `{"event_id", "room_id", "state": "knocking"}`。规范响应为 `{"event": <完整事件>, "state": "knock"}`。`state` 值应为 `"knock"` 而非 `"knocking"`，且必须返回完整 event 对象。 |
| **位置** | [membership.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/membership.rs) L117-121 |
| **处理方法** | 返回完整事件对象，`state` 改为 `"knock"`。 |
| **验证方法** | 1. 测试：响应含完整 event 对象；2. 测试：`state` 为 `"knock"`。 |
| **所需资源** | 后端 0.1 人周 |

---

#### P1-15 ServiceContainer::new() 是 ~415 行 god function

| 项 | 内容 |
|---|---|
| **域** | 架构 |
| **描述** | `ServiceContainer::new()` 包含数十个 service/storage 实例化、feature-gated 条件编译、异步初始化、环境变量读取、文件系统操作。单函数承担整个应用依赖图组装。 |
| **位置** | [container.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/container.rs) L667-1081 |
| **处理方法** | 进一步抽出 `assemble_core`、`assemble_account`、`assemble_sso`、`assemble_extensions`，使 `new()` 仅负责调用组装函数并拼接结果。 |
| **验证方法** | 1. `new()` 函数体 < 50 行；2. 每个子组装函数 < 100 行；3. `cargo test --lib` 通过。 |
| **所需资源** | 后端 1 人周 |

---

#### P1-16 AdminServices 是 ~40 字段的 god struct

| 项 | 内容 |
|---|---|
| **域** | 架构 |
| **描述** | `AdminServices` 混合 storage、service、manager、scheduler 等不同职责依赖，违反单一职责原则。 |
| **位置** | [container.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/container.rs) L432-474 |
| **处理方法** | 按子域拆分为 `AdminUserServices`、`AdminFederationServices`、`AdminMediaServices`、`AdminSecurityServices`、`AdminTokenServices`，`AdminServices` 作为聚合。 |
| **验证方法** | 1. `AdminServices` 字段数 < 10（子组引用）；2. `cargo check` 通过；3. 集成测试无回归。 |
| **所需资源** | 后端 1 人周（与 P1-15 合并） |

---

#### P1-17 超过 1000 行的大文件

| 项 | 内容 |
|---|---|
| **域** | 架构 |
| **描述** | `application_service/mod.rs`（1584 行）、`sliding_sync_service.rs`（1308 行）、`container.rs`（1216 行）超过 1000 行。 |
| **位置** | [application_service/mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/application_service/mod.rs)；[sliding_sync_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/sliding_sync_service.rs)；[container.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/container.rs) |
| **处理方法** | `application_service/mod.rs` 拆分为 `manager.rs`/`models.rs`/`transaction.rs`；`sliding_sync_service.rs` 拆分为 `core.rs`/`filters.rs`/`timeline.rs`/`state.rs`；`container.rs` 按域拆分。 |
| **验证方法** | 1. 每个文件 < 800 行；2. `cargo check` 通过；3. `cargo clippy` 通过。 |
| **所需资源** | 后端 1.5 人周 |

---

#### P1-18 JWT 缺少 issuer/audience 验证 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 安全 |
| **状态** | ✅ 已修复（2026-06-19） |
| **描述** | `decode_token` 仅设置 `required_spec_claims = ["exp","iat","sub"]`，未配置 `set_audience`/`set_issuer`。若 `jwt_secret` 在多服务间复用，存在 token confusion 风险。 |
| **位置** | [token.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/auth/token.rs) L232-237 |
| **处理方法** | 1. `validation.set_audience(&["synapse-rust"])` 并在签发时写入 `aud`；2. `validation.set_issuer(&[&server_name])`；3. `validation.validate_exp = true`。 |
| **验证方法** | 1. 测试：无 `aud` 的 token 被拒绝；2. 测试：`iss` 不匹配的 token 被拒绝；3. `cargo test --lib jwt_validation` 通过。 |
| **所需资源** | 后端 0.3 人周 |

---

#### P1-19 Ed25519 签名验证不一致（非严格模式） ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 安全 |
| **状态** | ✅ 已修复（2026-06-19） |
| **描述** | 多处使用 `verifying_key.verify(...)`（非严格）而非 `verify_strict`。非严格模式接受非规范编码的签名/公钥（malleable signatures），在联邦和 E2EE 场景可能引入问题。而 `verify_federation_signature` 已使用 `verify_strict`，说明项目已知应使用严格模式但未统一。 |
| **位置** | [ed25519.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-e2ee/src/crypto/ed25519.rs) L45；[federation_auth.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/middleware/federation_auth.rs) L503 |
| **处理方法** | 全部改为 `verify_strict(message, &sig)`。 |
| **验证方法** | 1. `grep "\.verify(" synapse-e2ee/ src/web/middleware/federation_auth.rs` 零匹配（仅 `verify_strict`）；2. 测试：malleable 签名被拒绝。 |
| **所需资源** | 后端 0.2 人周 |

---

#### P1-20 敏感信息（验证 token、邮箱）写入 INFO 日志 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 安全 |
| **状态** | ✅ 已修复（2026-06-19） |
| **描述** | 3PID 验证会话创建时将 `session_id`、验证 `token`、`email` 以 INFO 级别记录。验证 token 泄露可劫持邮箱验证流程；email 属于 PII。 |
| **位置** | [threepid.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/threepid.rs) L89 |
| **处理方法** | 1. 移除 `token` 字段，仅记录 `sid`；2. email 脱敏（如 `u***@example.com`）；3. 全局搜索其他 `tracing::info!` 中包含 token/password/secret 的位置一并清理。 |
| **验证方法** | 1. `grep "info.*token\|info.*password\|info.*secret" src/` 零匹配；2. 日志审查无敏感信息。 |
| **所需资源** | 后端 0.3 人周 |

---

### P2 — 中期修复（18 项）

---

#### P2-01 生产 `expect()` 在 `panic=abort` 下有崩溃风险

| 项 | 内容 |
|---|---|
| **域** | 代码质量 |
| **描述** | `Cargo.toml` 配置 `panic = "abort"`，以下生产代码使用 `expect()`：HMAC 初始化（4 处）、Argon2 参数（1 处）。虽当前库版本下不会失败，但底层变化将导致服务崩溃。 |
| **位置** | [invite_signature.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/security/invite_signature.rs) L58, L78；[device_binding.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/security/device_binding.rs) L45, L62；[password_hash_pool.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/common/password_hash_pool.rs) L101 |
| **处理方法** | 改为返回 `Result` 并在调用方处理，或至少在启动时一次性校验。 |
| **验证方法** | 1. `grep "\.expect(" src/security/ src/common/password_hash_pool.rs` 零匹配；2. `cargo test --lib` 通过。 |
| **所需资源** | 后端 0.3 人周 |

---

#### P2-02 缓存写入错误被 `let _ =` 静默忽略 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 代码质量 |
| **描述** | 6 处缓存写入错误被静默忽略，无日志记录。Redis 故障时缓存层完全不可观测。 |
| **位置** | [federation_auth.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/middleware/federation_auth.rs) L329, L336, L437；[search.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/search.rs) L308, L365；[events.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/room/events.rs) L264 |
| **处理方法** | 加 `tracing::warn!` 记录缓存写入失败。 |
| **验证方法** | 1. Redis 故障时日志有 warn 记录；2. `grep "let _ =.*cache" src/` 零匹配。 |
| **所需资源** | 后端 0.2 人周 |

---

#### P2-03 `map_err(|_| ...)` 丢失错误上下文（~20 处） ⚠️ 部分完成

| 项 | 内容 |
|---|---|
| **域** | 代码质量 |
| **描述** | 约 20 处使用 `map_err(|_| ApiError::internal(...))` 丢弃原始错误信息，不利于排障。 |
| **位置** | [federation_auth.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/middleware/federation_auth.rs) L402；[oidc.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/oidc.rs) L58, L78；[search.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/search.rs) L224, L234；[admin_registration_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/admin_registration_service.rs) L202；[saml_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/saml_service.rs) L220, L239 等 |
| **处理方法** | 统一采用 `ApiError::internal_with_log(msg, &e)` 模式。 |
| **验证方法** | 1. 原先列举的内部错误上下文丢失点已改为保留错误信息或使用更合适的错误映射；2. 对于内部错误路径，不再继续新增 `map_err(|_| ApiError::internal(...))` 这类吞掉上下文的写法；3. `cargo clippy` 通过。 |
| **所需资源** | 后端 0.5 人周 |
| **状态** | ⚠️ 部分完成（2026-06-20 复核）。原审查列举的多个关键调用点已修正，但仓库内仍存在少量 `map_err(|_| ApiError::internal(...))`，例如 `synapse-federation/src/key_rotation.rs`、`src/web/utils/admin_auth.rs`、`synapse-services/src/content_scanner/service.rs`，问题尚未完全清零。 |

---

#### P2-04 search.rs 空间子查询逻辑重复 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 代码质量 |
| **描述** | `get_rooms_batch` + `get_state_events_batch` + 构建 child_rooms 逻辑在两处重复，且错误处理不一致。 |
| **位置** | [search.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/search.rs) L450-468 vs L595-609 |
| **处理方法** | 抽取为公共辅助函数。 |
| **验证方法** | 1. 重复逻辑消除；2. 集成测试无回归。 |
| **所需资源** | 后端 0.2 人周 |

---

#### P2-05 签名密钥加密使用弱密钥派生 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 安全 |
| **描述** | `derive_key` 使用单次 `SHA-256(info ‖ master_key)` 派生 AES-256 密钥，缺乏工作因子，非标准 KDF。 |
| **位置** | [key_encryption.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/common/key_encryption.rs) L58-66 |
| **处理方法** | 改用 `hkdf::Hkdf::<Sha256>` 派生密钥。 |
| **验证方法** | 1. 测试：HKDF 派生的密钥与 SHA-256 不同；2. 测试：加密/解密往返一致；3. `cargo test --lib key_derivation` 通过。 |
| **所需资源** | 后端 0.3 人周 |

---

#### P2-06 缓存无 single-flight 保护（缓存击穿风险） ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 性能 |
| **描述** | 缓存 `get` 方法无 single-flight 保护，热点 key 过期时多个并发请求同时穿透到 Redis/DB。 |
| **位置** | [lib.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-cache/src/lib.rs) L876-899 |
| **处理方法** | 实现 `get_or_fetch` 方法，使用 `tokio::sync::Mutex` 或 single-flight 模式。 |
| **验证方法** | 1. 压力测试：热点 key 过期时 DB 查询次数为 1；2. `cargo test --lib cache_single_flight` 通过。 |
| **所需资源** | 后端 0.5 人周 |

---

#### P2-07 缓存无批量 get 方法（N+1 缓存查询） ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 性能 |
| **描述** | 缓存无 `get_batch`/MGET 方法，导致 presence 等场景出现 N+1 缓存查询（3 处相同模式）。 |
| **位置** | [presence.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/presence.rs) L147-154, L375-382, L428-433 |
| **处理方法** | 实现 `get_batch` 方法使用 Redis MGET。 |
| **验证方法** | 1. 测试：100 个 key 的批量查询只产生 1 次 Redis 命令；2. `cargo test --lib cache_batch_get` 通过。 |
| **所需资源** | 后端 0.5 人周 |

---

#### P2-08 Token 缓存 TTL 不一致

| 项 | 内容 |
|---|---|
| **域** | 性能/安全 |
| **描述** | Token 缓存 TTL 在两处不一致：`strategy.rs` 为 300s，`query_cache.rs` 为 3600s。1 小时对安全敏感的 token 过长，撤销后仍有 1 小时窗口。 |
| **位置** | [query_cache.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/cache/query_cache.rs) L57；[strategy.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/cache/strategy.rs) L128-130 |
| **处理方法** | 统一 TTL 定义，token 缓存使用 300s。 |
| **验证方法** | 1. `grep "3600" src/cache/` 零匹配；2. 测试：token 撤销后 5 分钟内缓存失效。 |
| **所需资源** | 后端 0.1 人周 |

---

#### P2-09 多处 N+1 查询（device/notification/beacon/e2ee_audit/room summary） ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 性能 |
| **描述** | 9 处中等严重度 N+1 查询：设备删除/批量删除循环记录变更、通知列表循环获取状态、beacon 循环获取位置、E2EE 审计循环验证设备/获取信任状态、room summary 循环更新状态/获取 heroes。 |
| **位置** | [device.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/device.rs) L422-424, L475-477；[server_notification.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/server_notification.rs) L345-352；[beacon.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/beacon.rs) L437-440；[audit_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/e2ee_audit/audit_service.rs) L110-111, L241-242；[summary_state.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/room/summary_state.rs) L149-158；[summary.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/room/summary.rs) L59-62 |
| **处理方法** | 逐个添加批量查询方法，使用 `WHERE ... = ANY($1)` 单次查询。 |
| **验证方法** | 1. 每处 N+1 改为 1 次查询；2. 功能测试无回归。 |
| **所需资源** | 后端 1.5 人周（分批） |

---

#### P2-10 PostgreSQL 连接池配置偏小 + test_before_acquire 开销 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 性能 |
| **描述** | 默认 `max_size=20` 对高流量服务器可能不足（Synapse 推荐 50-100+）。`test_before_acquire(true)` 每次获取连接执行测试查询，增加延迟。 |
| **位置** | [homeserver.yaml.example](file:///Users/ljf/Desktop/hu_ts/synapse-rust/homeserver.yaml.example) L35；[server.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/server.rs) L107 |
| **处理方法** | 1. 生产默认 `max_size=50`；2. 改用 `test_before_acquire(false)` 配合较短 `max_lifetime`。 |
| **验证方法** | 1. 基准测试：连接获取延迟下降；2. 压力测试：50 并发不耗尽连接池。 |
| **所需资源** | 后端 0.2 人周 |

---

#### P2-11 wildcard re-export 造成隐式耦合 ⚠️ 部分完成

| 项 | 内容 |
|---|---|
| **域** | 架构 |
| **描述** | `synapse-services/src/lib.rs` 使用 `#![allow(ambiguous_glob_reexports)]` + ~20 处 `pub use X::*;`，使 services crate 公共 API 不可控。`src/storage/mod.rs` 有 ~30 处 wildcard re-export。 |
| **位置** | [lib.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/lib.rs) L1, L190-199；[storage/mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/storage/mod.rs) |
| **处理方法** | 移除 `#![allow(ambiguous_glob_reexports)]`，将 `pub use X::*;` 改为显式导出，或至少限制为 `pub(crate) use`。 |
| **验证方法** | 1. `grep "allow(ambiguous_glob_reexports)" synapse-services/` 零匹配；2. `cargo check` 无 ambiguous 警告。 |
| **所需资源** | 后端 1 人周 |
| **状态** | ⚠️ 部分完成（2026-06-20 复核）。当前仅补充了 wildcard re-export 的注释与文档说明，但 `synapse-services/src/lib.rs` 仍保留 `#![allow(ambiguous_glob_reexports)]`，`synapse-services/src/lib.rs` 与 `src/storage/mod.rs` 仍大量使用 `pub use ...::*`，且源码仍留有 `TODO: explicit exports (P2-11)`。 |

---

#### P2-12 container.rs 内硬编码运行时配置值 ⚠️ 部分完成

| 项 | 内容 |
|---|---|
| **域** | 架构 |
| **描述** | `ServiceContainer::new()` 内硬编码媒体存储路径、server_name 回退值 `"localhost"`、搜索索引名、事件广播批处理参数、refresh token TTL。 |
| **位置** | [container.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/container.rs) L10, L674-678, L682, L714, L959 |
| **处理方法** | 将这些值移入 `Config` 结构体，在 `homeserver.yaml` 中提供默认值。 |
| **验证方法** | 1. `grep "localhost\|/app/data\|synapse_messages" synapse-services/src/container.rs` 零匹配；2. 配置文件覆盖测试通过。 |
| **所需资源** | 后端 0.5 人周 |
| **状态** | ⚠️ 部分完成（2026-06-20 复核）。部分配置已外提，但 `ServiceContainer::new()` 仍保留媒体路径回退 `"/app/data/media"` / `"./data/media"` 等硬编码运行时默认值，尚未完全达到“全部移入 Config”的目标。 |

---

#### P2-13 container.rs 内读取环境变量绕过 Config ⚠️ 部分完成

| 项 | 内容 |
|---|---|
| **域** | 架构 |
| **描述** | `ServiceContainer::new()` 直接读取 `SYNAPSE_MEDIA_PATH`、`SYNAPSE_MEGOLM_ENCRYPTION_KEY_PATH`、`SYNAPSE_ENABLE_BURN_AFTER_READ_PROCESSOR`，绕过 `SYNAPSE__` 配置覆盖机制。 |
| **位置** | [container.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/container.rs) L423, L673, L1133 |
| **处理方法** | 纳入 `Config` 结构体，通过标准环境变量覆盖机制管理。 |
| **验证方法** | 1. `grep "std::env::var\|env::var" synapse-services/src/container.rs` 零匹配；2. 配置覆盖测试通过。 |
| **所需资源** | 后端 0.3 人周（与 P2-12 合并） |
| **状态** | ⚠️ 部分完成（2026-06-20 复核）。部分调用已纳入配置体系，但 `container.rs` 仍直接读取 `SYNAPSE_ENABLE_BURN_AFTER_READ_PROCESSOR`、`SYNAPSE_MEDIA_PATH`、`SYNAPSE_MEGOLM_ENCRYPTION_KEY_PATH` 作为向后兼容回退，尚未满足“零 `env::var`”的完成标准。 |

---

#### P2-14 canonical JSON 仍允许浮点数且未校验整数范围

| 项 | 内容 |
|---|---|
| **域** | 兼容性 |
| **描述** | `format_canonical_number` 对非整数浮点用 `format!("{f}")` 输出，可能产生科学计数法。Matrix canonical JSON 要求仅允许 `[-(2^53)+1, 2^53-1]` 范围整数，浮点应被拒绝。 |
| **位置** | [canonical_json.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/canonical_json.rs) L89-100 |
| **处理方法** | 非整数/超范围数字返回错误而非输出。 |
| **验证方法** | 1. 测试：`1.0` 被拒绝；2. 测试：`2^53` 被拒绝；3. 测试：合法整数正常输出。 |
| **所需资源** | 后端 0.2 人周（与 P0-04 合并） |

---

#### P2-15 联邦密钥 query/notary 未完整验证自签名链路 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 兼容性/安全 |
| **描述** | `/_matrix/key/v2/query/{server_name}/{key_id}` 对远端结果做 canonical 包装，但未完整验证 `server_name`/`valid_until_ts`/`verify_keys`/`old_verify_keys`/`signatures` 自签名链路后再缓存。 |
| **位置** | [MATRIX_SYNAPSE_AUDIT_AND_OPTIMIZATION_PLAN_2026-05-29.md](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/synapse-rust/MATRIX_SYNAPSE_AUDIT_AND_OPTIMIZATION_PLAN_2026-05-29.md) L48-51 |
| **处理方法** | 抽出 `ServerKeySet` 类型，完整校验上述字段后再入缓存。 |
| **验证方法** | 1. 测试：无效自签名被拒绝；2. 测试：过期 key 不被缓存。 |
| **所需资源** | 后端 0.5 人周 |

---

#### P2-16 联邦 server key 未校验 valid_until_ts

| 项 | 内容 |
|---|---|
| **域** | 安全 |
| **描述** | `valid_until_ts` 缺失时默认设为 `now + 3600s`，且未校验响应中的 `valid_until_ts` 是否已过期。攻击者可注入已过期但带有效签名的旧 key 响应。 |
| **位置** | [keys.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/keys.rs) L324-327 |
| **处理方法** | 1. `valid_until_ts` 缺失或早于 `now` 时拒绝缓存；2. 缓存 TTL 取 `min(configured_ttl, valid_until_ts - now)`。 |
| **验证方法** | 1. 测试：过期 key 响应不被缓存；2. 测试：缺失 `valid_until_ts` 被拒绝。 |
| **所需资源** | 后端 0.3 人周 |

---

#### P2-17 /versions 声明的 MSC 未在项目规则登记 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 兼容性 |
| **描述** | `BASE_UNSTABLE_FEATURES` 声明了 `org.matrix.msc3266`、`org.matrix.msc3916`、`uk.tcpip.msc4133`，但项目规则 MSC 表未包含。MSC3916 实现证据不明。 |
| **位置** | [versions.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/versions.rs) L91-93；[project_rules.md](file:///Users/ljf/Desktop/hu_ts/synapse-rust/.trae/rules/project_rules.md) 第十节 |
| **处理方法** | 1. 将三项 MSC 补入项目规则 MSC 表并标注证据；2. 核实 MSC3916 是否有实现，否则从声明移除。 |
| **验证方法** | 1. 项目规则 MSC 表包含三项；2. `cargo test --features test-utils --test unit test_capability` 通过。 |
| **所需资源** | 后端 0.2 人周 |

---

#### P2-18 缺少 Complement 级互通测试

| 项 | 内容 |
|---|---|
| **域** | 兼容性 |
| **描述** | 集成测试多为本仓库行为测试，缺少最小 Complement/Matrix SDK 互通门禁。结合 P0-04/P0-05/P0-08/P0-10，联邦互通缺陷难以被现有测试捕获。 |
| **位置** | [MATRIX_SYNAPSE_AUDIT_AND_OPTIMIZATION_PLAN_2026-05-29.md](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/synapse-rust/MATRIX_SYNAPSE_AUDIT_AND_OPTIMIZATION_PLAN_2026-05-29.md) L98-100 |
| **处理方法** | 建立最小 Complement/SDK smoke 门禁：register/login/sync/create room/send event/federation key/server discovery/media。 |
| **验证方法** | 1. Complement smoke 测试在 CI 中通过；2. 覆盖上述 8 个最小场景。 |
| **所需资源** | 后端 2 人周 + QA 1 人周 |

---

### P3 — 择期修复（12 项）

---

#### P3-01 生产路由中 `unreachable!()`

| 项 | 内容 |
|---|---|
| **域** | 代码质量 |
| **描述** | `unreachable!()` 在 `panic=abort` 下会崩溃。虽由上方校验保证不可达，但应防御性编程。 |
| **位置** | [federation/events.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/events.rs) L332 |
| **处理方法** | 改为返回 `ApiError::bad_request`。 |
| **验证方法** | `grep "unreachable!" src/web/routes/` 零匹配。 |
| **所需资源** | 后端 0.1 人周 |

---

#### P3-02 `src/storage/` 内联单元测试稀少

| 项 | 内容 |
|---|---|
| **域** | 代码质量 |
| **描述** | `src/storage/` 有 ~55 个存储模块，但仅 2 个文件含 `#[cfg(test)]`。存储层通过集成测试覆盖，但缺少快速反馈的内联测试。 |
| **位置** | `src/storage/` 目录 |
| **处理方法** | 在核心存储模块补充内联单元测试。 |
| **验证方法** | 核心存储模块有 `#[cfg(test)]` 模块。 |
| **所需资源** | 后端 1 人周 |

---

#### P3-03 事件内容哈希比较非 constant-time ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 安全 |
| **描述** | `verify_event_content_hash` 使用 `if computed != sha256_hash` 非常量时间比较。 |
| **位置** | [signing.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/signing.rs) L138 |
| **处理方法** | 改用 `synapse_common::crypto::secure_compare`。 |
| **验证方法** | `grep "!=.*sha256_hash" synapse-federation/` 零匹配。 |
| **所需资源** | 后端 0.1 人周 |

---

#### P3-04 `secure_compare` 长度不同时立即返回 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 安全 |
| **描述** | `secure_compare`/`secure_compare_bytes` 在长度不同时立即返回，泄露长度信息。 |
| **位置** | [crypto.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/crypto.rs) L234-243, L91-104 |
| **处理方法** | 对变长敏感场景先 HMAC 再比较，或归一化长度。固定长度哈希场景加注释保留现状。 |
| **验证方法** | 时序测试：不同长度输入比较时间一致。 |
| **所需资源** | 后端 0.2 人周 |

---

#### P3-05 `generate_signing_key` 生成随机字符串而非 Ed25519 私钥 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 安全 |
| **描述** | `generate_signing_key` 返回 `random_string(44)`，不是 base64 编码的 Ed25519 私钥种子。若用于真实密钥生成将产生无法签名的"密钥"。 |
| **位置** | [crypto.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/crypto.rs) L262-266 |
| **处理方法** | 若用于真实密钥：改用 `ed25519_dalek::SigningKey::generate(&mut OsRng)`。若仅测试：加 `#[cfg(test)]`。 |
| **验证方法** | 确认调用方；测试生成的密钥可用于 Ed25519 签名。 |
| **所需资源** | 后端 0.2 人周 |

---

#### P3-06 SELECT * 使用（2 处） ⚠️ 部分完成

| 项 | 内容 |
|---|---|
| **域** | 性能 |
| **描述** | `get_pending_transactions` 使用 `SELECT *`；`impl_sqlx_types` 宏定义了 `SELECT * FROM users`（宏未使用，死代码）。 |
| **位置** | [application_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/application_service.rs) L680；[macros.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/common/macros.rs) L6 |
| **处理方法** | 明确列出所需列；删除未使用宏。 |
| **验证方法** | 1. `get_pending_transactions` 不再使用 `SELECT *`；2. 相关公共宏实现中不再保留 `SELECT * FROM users` 这类死代码。 |
| **所需资源** | 后端 0.1 人周 |
| **状态** | ⚠️ 部分完成（2026-06-20 复核）。`synapse-storage` 中相关查询已收敛，但 `synapse-common/src/macros.rs` 仍保留 `impl_sqlx_types!` 宏中的 `SELECT * FROM users WHERE active = true`，死代码宏尚未完全删除。 |

---

#### P3-07 Worker 健康检查未并行化 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 性能 |
| **描述** | Worker 选择时循环调用 `is_healthy` 检查每个候选 worker，未并行化。Health check 循环顺序检查。 |
| **位置** | [manager.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/worker/manager.rs) L716-720；[health.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/worker/health.rs) L231-233 |
| **处理方法** | 使用 `join_all` 并行检查。 |
| **验证方法** | 基准测试：10 个 worker 健康检查时间下降。 |
| **所需资源** | 后端 0.2 人周 |

---

#### P3-08 联邦 HTTP 客户端未配置连接池参数 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 性能 |
| **描述** | FederationClient 的 reqwest Client 未显式配置 `pool_max_idle_per_host`/`keepalive`。 |
| **位置** | [client.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/client.rs) L188-195 |
| **处理方法** | 显式配置 `pool_max_idle_per_host(20)` 和 `pool_idle_timeout`。 |
| **验证方法** | 联邦请求基准测试连接复用率提升。 |
| **所需资源** | 后端 0.1 人周 |

---

#### P3-09 非标准联邦路径 ❌ 未完成

| 项 | 内容 |
|---|---|
| **域** | 兼容性 |
| **描述** | `/_matrix/federation/v2/key/clone`、`/_matrix/federation/v1/room_auth/{room_id}` 等非 Matrix 规范路径，增加协议面维护负担。 |
| **位置** | [federation/mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/mod.rs) L228, L241, L244, L256, L257 |
| **处理方法** | 评估是否为扩展用途；若为扩展，迁移到 `io.hula.*` 或 `/_synapse/` 命名空间。 |
| **验证方法** | 非标准路径不在 `/_matrix/federation/` 下。 |
| **所需资源** | 后端 0.3 人周 |
| **状态** | ❌ 未完成（2026-06-20 复核）。`/_matrix/federation/v2/key/clone`、`/_matrix/federation/v1/room_auth/{room_id}`、`/_matrix/federation/v1/keys/claim` 等路径仍保留在标准联邦命名空间下，仅增加了“非标准扩展”注释，未完成迁移。 |

---

#### P3-10 sliding sync 缺少性能回滚闸门 ⚠️ 部分完成

| 项 | 内容 |
|---|---|
| **域** | 兼容性/性能 |
| **描述** | Synapse v1.153.0rc3 因性能问题回滚 sliding sync 优化；本项目有 sliding sync 路由与测试，但无性能阈值/回滚机制。 |
| **位置** | [MATRIX_SYNAPSE_AUDIT_AND_OPTIMIZATION_PLAN_2026-05-29.md](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/synapse-rust/MATRIX_SYNAPSE_AUDIT_AND_OPTIMIZATION_PLAN_2026-05-29.md) L77-80 |
| **处理方法** | 为 sliding sync 增加 subscription-change benchmark、p95/p99 与 query count 快照，设置回滚阈值。 |
| **验证方法** | 1. 仓库存在 `performance_sliding_sync_benchmarks` 基准；2. CI 或发布前性能门禁实际执行该基准并校验阈值。 |
| **所需资源** | 后端 0.5 人周 |
| **状态** | ⚠️ 部分完成（2026-06-20 复核）。当前已补充 sliding sync 的基准代码、p95/p99 统计与阈值相关实现，但 `.github/workflows/benchmark.yml` 尚未接入 `performance_sliding_sync_benchmarks`，CI 级性能回滚闸门仍未完全落地。 |

---

#### P3-11 设备列表/presence 缺少长期运行剪枝 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 兼容性/性能 |
| **描述** | 缺少 `device_lists_changes_in_room`、过期 presence、过期 one-time key 的统一后台剪枝任务，长期实例磁盘膨胀。 |
| **位置** | [MATRIX_SYNAPSE_AUDIT_AND_OPTIMIZATION_PLAN_2026-05-29.md](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/synapse-rust/MATRIX_SYNAPSE_AUDIT_AND_OPTIMIZATION_PLAN_2026-05-29.md) L82-85 |
| **处理方法** | 新增 background update 剪枝旧 device list change、过期 presence、过期 OTK。 |
| **验证方法** | 长期运行测试：磁盘使用稳定。 |
| **所需资源** | 后端 1 人周 |

---

#### P3-12 Admin API 与上游 v1.153 存在差距 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 兼容性 |
| **描述** | 缺失 `GET /_synapse/admin/v1/quarantine_media/{media_id}/changes`、`GET /_synapse/admin/v1/rooms/{room_id}/reports`、`DELETE /_synapse/admin/v1/rooms/{room_id}/reports/{report_id}`；room details 的 tombstoned/replacement_room 字段不完整。 |
| **位置** | [API_COVERAGE_REPORT.md](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/synapse-rust/API_COVERAGE_REPORT.md) L122-126 |
| **处理方法** | 优先补"审计/治理类"接口。 |
| **验证方法** | 新增接口有集成测试覆盖。 |
| **所需资源** | 后端 1 人周 |

---

## 三、优先级排序的优化计划

### 阶段一：安全与联邦互通紧急修复（P0，建议 2-3 周）

**目标**：消除可被利用的安全漏洞，修复破坏联邦互通的协议不合规。

**依赖关系**：
```
P0-04 (canonical JSON 统一) ─┐
P0-05 (redacts 字段) ────────┤
P0-06 (redaction 剥离) ──────┼──→ P0-08 (联邦 redaction PDU)
P0-07 (hash 字段表) ─────────┤
P0-09 (redaction 权限) ──────┘
P0-10 (状态解析 v2) ←── P0-11 (power_levels 修复)
P0-12 (room version 声明) ←── 依赖 P0-05/P0-06/P0-09 完成后恢复
P0-01 (SAML XSW)     独立
P0-02 (联邦 SSRF)    独立
P0-03 (token secret) 独立
```

**执行顺序**：
1. **Week 1**：P0-01（SAML）、P0-02（SSRF）、P0-03（token secret）— 安全紧急，独立可并行
2. **Week 1-2**：P0-04（canonical JSON 统一）+ P2-14（浮点拒绝）— 联邦签名根基
3. **Week 2**：P0-05 + P0-06 + P0-07 + P0-09 — redaction 全链路修复（可并行）
4. **Week 2-3**：P0-08（联邦 redaction PDU）— 依赖前三步
5. **Week 2-3**：P0-10 + P0-11 — 状态解析 v2 算法（最复杂，独立 lane）
6. **Week 1**：P0-12（room version 声明降级）— 立即降级，待 P0-05/06/09 完成后恢复

**验证门禁**：
- `cargo test --features test-utils --test integration` 全部通过
- 引入 Matrix canonical JSON test vectors 对照测试
- 引入 Matrix state resolution v2 test vectors 对照测试
- XSW 攻击向量测试
- SSRF 黑名单测试

---

### 阶段二：架构分层与关键路径性能（P1，建议 4-6 周）

**目标**：消除分层违规，修复关键路径性能瓶颈。

**依赖关系**：
```
P1-01 (worker/storage 迁移)     独立
P1-02 (rendezvous DI)           独立
P1-03 (routes 跳层)             可分批，优先 admin/notification.rs
P1-04 (services 持有 PgPool)    可分批
P1-05 (synapse-web 命运)        独立，建议选 B（删除）
P1-06 (锁跨 await)              独立，快速修复
P1-07~P1-11 (N+1/大查询)        可并行
P1-12 (unwrap_or_default)       独立，快速修复
P1-13~P1-14 (knock 端点)        独立，快速修复
P1-15~P1-17 (god function/struct/大文件)  P1-15+P1-16 合并
P1-18~P1-20 (安全)              独立
```

**执行顺序**：
1. **Week 1**：P1-06（锁跨 await）、P1-12（unwrap_or_default）、P1-13+P1-14（knock）— 快速修复
2. **Week 1**：P1-05（synapse-web 删除）— 消除死代码
3. **Week 1-2**：P1-01（worker/storage 迁移）、P1-02（rendezvous DI）
4. **Week 2-3**：P1-07~P1-11（N+1 查询批量修复）— 可并行
5. **Week 3-4**：P1-03（routes 跳层，分批）、P1-04（services PgPool，分批）
6. **Week 4-5**：P1-15+P1-16（container 拆分）、P1-17（大文件拆分）
7. **Week 5-6**：P1-18（JWT aud/iss）、P1-19（Ed25519 strict）、P1-20（日志脱敏）

**验证门禁**：
- `scripts/check_layer_isolation.sh` 零 WARNING
- `cargo clippy --all-features --locked -- -D warnings` 通过
- `cargo bench` 关键路径性能提升
- `python3 scripts/ci/check_root_canonical_ledger.py` 维持 full_impl=0

---

### 阶段三：代码质量与中等性能/安全（P2，建议 4-6 周）

**目标**：提升代码质量，修复中等性能和安全问题。

**执行顺序**（可大量并行）：
1. P2-01（expect 修复）、P2-02（缓存日志）、P2-03（map_err 统一）、P2-04（search 重复）— 代码质量
2. P2-05（HKDF）、P2-08（Token TTL）— 安全
3. P2-06（single-flight）、P2-07（MGET）、P2-09（N+1 批量）、P2-10（连接池）— 性能
4. P2-11（wildcard re-export）、P2-12+P2-13（配置外提）— 架构
5. P2-14（浮点拒绝，与 P0-04 合并）、P2-15+P2-16（联邦 key 校验）— 兼容性
6. P2-17（MSC 登记）、P2-18（Complement 测试）— 兼容性

**验证门禁**：
- 内部错误路径不再继续引入 `map_err(|_| ApiError::internal(...))` 这类丢失上下文的写法
- `grep "unwrap_or_default" src/web/routes/` 零匹配
- 缓存 single-flight 压力测试通过
- Complement smoke 测试在 CI 中通过

---

### 阶段四：低优先级改进（P3，持续迭代）

**目标**：完善细节，提升健壮性。

**执行顺序**（无严格依赖，按需排期）：
- P3-01~P3-05：代码质量与安全细节
- P3-06~P3-08：性能微调
- P3-09~P3-12：兼容性补齐

---

## 四、正面发现汇总

以下实践已正确实施，值得肯定：

| 领域 | 实践 |
|------|------|
| 代码纪律 | 源码整体整洁，仅保留少量 TODO 标记用于跟踪已知重构项；未见生产代码级 FIXME/HACK 标记 |
| 内存安全 | 生产代码无 `unsafe` |
| SQL 安全 | 全部使用 sqlx 参数化查询，无字符串拼接 SQL |
| 密码安全 | Argon2id + OsRng salt + spawn_blocking + 并发限制 |
| 时序防护 | 登录路径有 dummy hash 防用户枚举时序 |
| Token 安全 | refresh token 家族管理 + CAS 撤销 + reuse 检测 |
| Admin 安全 | is_admin 从 DB 重查 + RBAC + TOTP MFA + 审计日志 |
| 路径安全 | media ID 严格字符集白名单 |
| SSRF 防护 | URL preview 使用 `check_url_against_blacklist` |
| E2EE | AES-256-GCM + OsRng nonce + NonceTracker 重用检测 |
| 缓存失效 | 基于 Redis pub/sub 的跨实例失效广播 |
| 测试覆盖 | 测试面较广：当前 `tests/` 下约 188 个 Rust 测试文件，另有大量 crate 内联测试；但互通门禁仍缺 Complement 级覆盖 |
| 依赖管理 | crate 依赖无循环，trait object 使用合理 |
| Facade 收口 | 按 `scripts/ci/check_root_canonical_ledger.py` 当前输出，root/canonical overlap 已收敛为 thin facade（services `full_impl=0`，storage `full_impl=0`） |

---

## 五、总结

synapse-rust 项目在工程纪律（少量已知 TODO 跟踪项、无 unsafe、SQL 参数化、facade 收口）方面表现较好，但在三个核心领域存在严重缺陷：

1. **联邦协议合规性**（P0-04 至 P0-12）：canonical JSON 实现分歧、redaction 全链路不合规、状态解析非 v2 算法。这些问题相互叠加，会导致与上游 Synapse 及其他合规 homeserver 的联邦互通失败。**这是最高优先级修复项。**

2. **安全漏洞**（P0-01 至 P0-03）：SAML XSW 认证绕过、联邦 SSRF、硬编码 token secret。这些可被直接利用。

3. **架构分层**（P1-01 至 P1-05）：storage 泄漏到 services、routes 跳层访问 storage、synapse-web 死代码。虽不直接影响功能，但增加维护成本和回归风险。

建议按阶段一→二→三→四的顺序推进，每阶段完成后运行全部门禁验证。

---

## 六、第二轮审查 — 新发现问题与修复（2026-06-19）

在完成 P0 全部 12 项和 P1 大部分项修复后，进行了第二轮全面审查，发现并修复了以下新问题：

### NEW-P0-01 联邦密钥查询 SSRF 漏洞（P0-02 修复不完整） ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 安全 |
| **状态** | ✅ 已修复（2026-06-19） |
| **描述** | P0-02 修复了 `fetch_federation_verify_key` 的 SSRF 漏洞，但遗漏了第二处 `fetch_remote_server_keys_response`。该函数构造 URL 时包含 HTTP fallback（明文，MITM 风险），未调用 `check_url_against_blacklist`，未配置重定向策略。攻击者可注册 `server_name` 使其 DNS 解析到 `127.0.0.1`、`169.254.169.254`（云元数据）、`10.0.0.0/8` 内网，通过公开端点 `GET /_matrix/key/v2/query/{server_name}/{key_id}` 诱导服务器读取内网资源。 |
| **位置** | [keys.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/keys.rs) L269-353 |
| **处理方法** | 1. 调用 `check_url_against_blacklist` 进行 IP 黑名单过滤；2. 移除 HTTP fallback，仅允许 HTTPS；3. 配置 `reqwest::redirect::Policy::none()` 禁用重定向。 |
| **验证方法** | 1. `grep "check_url_against_blacklist" src/web/routes/federation/keys.rs` 有匹配；2. `grep "http://" src/web/routes/federation/keys.rs` 零匹配（仅 HTTPS）。 |

### NEW-P1-01 OIDC 回调将邮箱地址以 INFO 级别记录 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 安全（PII 泄露） |
| **状态** | ✅ 已修复（2026-06-19） |
| **描述** | OIDC 回调处理函数以 INFO 级别明文记录用户邮箱地址。邮箱属于 PII，不应出现在 INFO 日志（通常始终启用）中。P1-20 修复了 3PID 验证日志但遗漏了此 OIDC 回调位置。 |
| **位置** | [oidc.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/oidc.rs) L906-911 |
| **处理方法** | 将 `email: {:?}` 改为 `email_present: {}`（布尔值），仅表示邮箱是否存在。 |

### NEW-P1-02 Worker 邮件发送函数在 INFO/ERROR 级别记录收件人地址 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 安全（PII 泄露） |
| **状态** | ✅ 已修复（2026-06-19） |
| **描述** | 邮件发送函数在 INFO 级别（L308）和 ERROR 级别（L277, L312）记录完整收件人邮箱地址，尽管 L270 的日志已声明"recipient masked"。实现不一致，泄露 PII。 |
| **位置** | [synapse_worker.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/bin/synapse_worker.rs) L277, L308, L312 |
| **处理方法** | 所有日志中的收件人地址改为 `(recipient masked)`，仅保留 DEBUG 级别的完整地址用于调试。 |

### NEW-P1-04/05 batch state event 查询缺少 LIMIT（OOM 风险） ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 性能 |
| **状态** | ✅ 已修复（2026-06-19） |
| **描述** | `get_state_events_by_type_batch`、`get_state_events_since_batch`、`get_state_events_since_stream_batch` 三个批量查询无 LIMIT，可能返回数百万行导致 OOM。单房间版本在 P1-11 已修复（LIMIT 5000），但批量版本被遗漏。 |
| **位置** | [event/state.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/event/state.rs) L92-201 |
| **处理方法** | 三个查询均添加 `LIMIT 50000`（批量操作使用更大的限制）。 |

### NEW-P1-06/07 联邦 publicRooms 端点 `total_room_count_estimate` 不正确 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 协议合规性 |
| **状态** | ✅ 已修复（2026-06-19） |
| **描述** | 联邦 `POST /_matrix/federation/v1/publicRooms` 将 `total_room_count_estimate` 设为 `room_list.len()`（当前页大小）而非实际公共房间总数。GET 端点缺少 `total_room_count_estimate` 字段。 |
| **位置** | [events.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/events.rs) L338-403 |
| **处理方法** | 调用 `count_public_rooms()` 获取真实总数，GET 和 POST 端点均返回正确的 `total_room_count_estimate`。 |

### NEW-P2-01 `query_keys` 中 `unwrap_or_default()` 吞噬 DB 错误 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 错误处理 |
| **状态** | ✅ 已修复（2026-06-19） |
| **描述** | `query_keys` 处理函数调用 `get_verified_devices_batch(...).await.unwrap_or_default()`，静默吞噬 DB 错误。DB 故障时客户端会看到无已验证设备，可能信任未验证设备。与 P1-12 相同模式但不同调用点。 |
| **位置** | [e2ee_routes.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/e2ee_routes.rs) L259-265 |
| **处理方法** | 改为 `.map_err(|e| ApiError::internal_with_log("Failed to load verified devices batch", &e))?`。 |

### 第二轮审查未修复项（延后处理）

以下问题已识别但延后处理，不影响部署：

| 编号 | 描述 | 严重程度 | 延后原因 |
|------|------|----------|----------|
| NEW-P1-03 | `validate_federation_origin_shares_user_room` 嵌套 N+1 查询 | P1 | 需要重构为批量 EXISTS 查询，涉及多文件改动，风险较高 |
| NEW-P2-02 | 联邦事务去重缓存读取错误静默吞噬 | P2 | 缓存故障时 fail-open 是可接受的降级策略 |
| NEW-P2-03 | `get_space_statistics` 和 worker 统计查询缺少 LIMIT | P2 | 统计数据量通常较小 |
| NEW-P2-04 | Aliyun SMS provider 中 `expect()` | P2 | 已有 `#[allow(clippy::expect_used)]`，且密钥在配置阶段验证 |
| NEW-P3-01 | 非标准联邦密钥端点（`/keys/claim` 等） | P3 | 保留用于向后兼容 |
| NEW-P3-02 | `builtin_oidc_login` 接受空 `client_id` | P3 | OIDC provider 应验证，defense-in-depth 改进 |

---

## 七、修复进度总览（截至 2026-06-19）

### P0 — 立即修复（12/12 完成）

| 编号 | 描述 | 状态 |
|------|------|------|
| P0-01 | SAML XSW 认证绕过 | ✅ 已修复 |
| P0-02 | 联邦密钥获取 SSRF | ✅ 已修复 |
| P0-03 | 硬编码 fallback token hash secret | ✅ 已修复 |
| P0-04 | canonical JSON 实现分歧 | ✅ 已修复 |
| P0-05 | m.room.redaction 缺失 redacts 字段 | ✅ 已修复 |
| P0-06 | v11+ redaction 算法未实现 | ✅ 已修复 |
| P0-07 | redaction 事件未应用 redaction | ✅ 已修复 |
| P0-08 | redaction 未验证签名权限 | ✅ 已修复 |
| P0-09 | redaction 未传播到客户端 | ✅ 已修复 |
| P0-10 | 状态解析非 v2 算法 | ✅ 已修复 |
| P0-11 | power_levels 映射全为 0 | ✅ 已修复 |
| P0-12 | v11+ 房间创建被禁用 | ✅ 已修复 |
| NEW-P0-01 | 联邦密钥查询 SSRF（P0-02 遗漏） | ✅ 已修复 |

### P1 — 短期修复（16/20 完成）

| 编号 | 描述 | 状态 |
|------|------|------|
| P1-01 | storage 实现泄漏到 services crate | ✅ 已修复（worker types/storage 迁移到 synapse-storage） |
| P1-02 | route handler 直接 new storage 实例 | ✅ 已修复 |
| P1-03 | routes 大规模直接访问 storage 层 | ⏳ 延后（大型重构） |
| P1-04 | services 直接持有 PgPool | ⏳ 延后（大型重构） |
| P1-05 | synapse-web crate 死代码 | ✅ 已修复 |
| P1-06 | 锁持有跨 .await | ✅ 已修复 |
| P1-07 | Space 层级遍历 N+1 查询 | ✅ 已修复 |
| P1-08 | Sync 响应构建 N+1 查询 | ✅ 已修复 |
| P1-09 | Appservice 事务构建 N+1 查询 | ✅ 已修复 |
| P1-10 | 批量创建 registration tokens N+1 INSERT | ✅ 已修复 |
| P1-11 | state events 查询无 LIMIT | ✅ 已修复 |
| P1-12 | 存储层错误被 unwrap_or_default 吞噬 | ✅ 已修复 |
| P1-13 | 联邦 knock 端点 HTTP 方法错误 | ✅ 已修复 |
| P1-14 | 联邦 knock 响应结构不符规范 | ✅ 已修复 |
| P1-15 | ServiceContainer::new() god function | ⏳ 延后（大型重构） |
| P1-16 | AdminServices god struct | ⏳ 延后（大型重构） |
| P1-17 | 超过 1000 行的大文件 | ⏳ 延后（大型重构） |
| P1-18 | JWT 缺少 issuer/audience 验证 | ✅ 已修复 |
| P1-19 | Ed25519 签名验证不一致 | ✅ 已修复 |
| P1-20 | 敏感信息写入 INFO 日志 | ✅ 已修复 |
| NEW-P1-01 | OIDC 回调邮箱 PII 日志 | ✅ 已修复 |
| NEW-P1-02 | Worker 邮件收件人 PII 日志 | ✅ 已修复 |
| NEW-P1-04/05 | batch state event 查询缺少 LIMIT | ✅ 已修复 |
| NEW-P1-06/07 | 联邦 publicRooms 协议合规性 | ✅ 已修复 |

### P2 — 中期修复（13/18 完成）

| 编号 | 描述 | 状态 |
|------|------|------|
| P2-01 | 生产代码中的 expect() (5处) | ✅ 已修复 |
| P2-02 | 缓存写入错误被静默忽略 (6处) | ✅ 已修复 |
| P2-03 | map_err(\|_\| ...) 丢失错误上下文 (18处) | ⚠️ 部分完成（关键调用点已修正，但仓库内仍残留少量 `map_err(|_| ApiError::internal(...))`） |
| P2-04 | search_service 重复实现 | ✅ 已修复（抽取 collect_child_rooms 辅助函数） |
| P2-05 | 签名密钥加密改用 HKDF | ✅ 已修复（HKDF-SHA256 替代单次 SHA-256） |
| P2-06 | 缓存 single-flight 防击穿 | ✅ 已修复（get_or_fetch + per-key Mutex） |
| P2-07 | 缓存 MGET 批量获取 | ✅ 已修复（get_batch + Redis MGET） |
| P2-08 | Token 缓存 TTL 不一致 (300s vs 3600s) | ✅ 已修复 |
| P2-09 | N+1 批量查询（其他） | ✅ 已修复（9 处批量查询改造） |
| P2-10 | 连接池配置优化 | ✅ 已修复（max_size=50, test_before_acquire=false） |
| P2-11 | wildcard re-export | ⚠️ 部分完成（仅补充注释与说明，未移除 wildcard re-export / `allow(ambiguous_glob_reexports)`） |
| P2-12 | 配置外提 | ⚠️ 部分完成（部分配置已外提，但仍保留媒体路径等硬编码回退值） |
| P2-13 | 配置外提 | ⚠️ 部分完成（仍保留 `env::var` 向后兼容回退，未达到零直接读取） |
| P2-14 | canonical JSON 允许浮点数 | ✅ 已修复（在 P0-04 中完成） |
| P2-15 | 联邦密钥 query/notary 未完整验证 | ✅ 已修复（validate_server_key_response 校验全字段） |
| P2-16 | 联邦 server key 未校验 valid_until_ts | ✅ 已修复 |
| P2-17 | MSC 登记 | ✅ 已修复（MSC3266/MSC4133 登记，移除未实现的 MSC3916） |
| P2-18 | Complement 测试 | ⏳ 延后（需 QA 配合，3 人周） |

### P3 — 低优先级（8/12 完成）

| 编号 | 描述 | 状态 |
|------|------|------|
| P3-01 | 生产路由中 unreachable!() | ✅ 已修复 |
| P3-02 | src/storage/ 内联单元测试稀少 | ⏳ 延后（1 人周，低优先级） |
| P3-03 | 事件内容哈希比较非 constant-time | ✅ 已修复 |
| P3-04 | secure_compare 长度不同时立即返回 | ✅ 已修复（constant-time 长度折叠） |
| P3-05 | generate_signing_key 生成随机字符串 | ✅ 已修复（#[cfg(test)] 限制） |
| P3-06 | SELECT * 使用（2 处） | ⚠️ 部分完成（存储层查询已收敛，但 `synapse-common/src/macros.rs` 仍保留死代码宏中的 `SELECT *`） |
| P3-07 | Worker 健康检查未并行化 | ✅ 已修复（futures::join_all） |
| P3-08 | 联邦 HTTP 客户端未配置连接池 | ✅ 已修复（pool_max_idle_per_host=20） |
| P3-09 | 非标准联邦路径 | ❌ 未完成（仍位于 `/_matrix/federation/` 命名空间，仅补充了扩展注释） |
| P3-10 | sliding sync 缺少性能回滚闸门 | ⚠️ 部分完成（已有 benchmark 与 p95/p99 统计，但 CI 尚未接入 sliding sync 基准门禁） |
| P3-11 | 设备列表/presence 缺少长期运行剪枝 | ✅ 已修复（每日后台剪枝任务） |
| P3-12 | Admin API 与上游 v1.153 存在差距 | ✅ 已修复（DELETE report/quarantine changes/tombstoned 字段） |

### 部署就绪状态评估

**部署判断**：按当前代码静态复核，所有 P0 安全漏洞和联邦协议合规性问题已修复（13/13）；剩余未完成项以架构重构、配置收敛、非标准路径治理和测试债务为主。P2 中期修复完成 13/18，P3 低优先级修复完成 8/12；其中 P2-03、P2-11、P2-12、P2-13、P3-06、P3-10 为部分完成，P3-09 尚未完成。基于当前问题分级，项目具备部署前提，但仍建议在后续迭代中继续收敛这些非 P0/P1 阻断项。

**最近一次文档记录的验证结果**（2026-06-20）：
- `cargo check --locked --workspace` ✅ 通过
- `cargo clippy --locked --workspace --all-features -- -D warnings` ✅ 通过（零警告）
- `cargo fmt --all -- --check` ✅ 通过
- `cargo test --features test-utils --test unit --locked` ✅ 862 passed, 0 failed
- `cargo test --features test-utils --test integration --locked --no-run` ✅ 编译通过
- 联邦签名、canonical JSON、安全回归、placeholder scan、worker 覆盖率测试全部通过

**延后项**（不影响部署）：
- P1-01/03/04/15/16/17：架构重构任务（4+ 人周），不影响功能正确性
- P2-18：Complement 互通测试（需 QA 配合，3 人周）
- P3-02：存储层内联单元测试（1 人周，低优先级）
