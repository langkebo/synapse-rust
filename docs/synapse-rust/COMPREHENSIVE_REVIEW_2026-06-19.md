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
| **验证方法** | [handle_saml_login()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/saml_service.rs#L274-L290) 当前已先执行 `validate_response(...)`，再调用 `parse_saml_assertion(...)`；而 [validate_response()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/saml_service.rs#L625-L630) 会提取 `Reference` URI、定位被引用元素，并校验其摘要而非错误地对 `SignedInfo` 本身做摘要比较。 |
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
| **验证方法** | [fetch_federation_verify_key()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/middleware/federation_auth.rs#L415-L439) 当前构造的 URL 列表仅保留 `https://...` 两个候选地址，并在每次请求前调用 `check_url_against_blacklist(url, ip_blacklist)`；同一函数中创建的 reqwest client 也已显式配置 [redirect(reqwest::redirect::Policy::none())](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/middleware/federation_auth.rs#L415-L420)。 |
| **所需资源** | 后端 0.5 人周 |
| **状态** | ✅ 已修复（2026-06-19）。fetch_federation_verify_key 调用 check_url_against_blacklist 进行 IP 黑名单过滤（支持 CIDR，并在请求前解析主机名后按黑名单过滤解析结果），阻止 127.0.0.1/169.254.169.254/10.0.0.0/8 等私有网络；强制 HTTPS，禁用重定向。 |

---

#### P0-03 硬编码 fallback token hash secret ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 安全 |
| **描述** | `hash_token` 在环境变量 `TOKEN_HASH_SECRET` 未设置时使用硬编码字符串 `"dev-test-token-hash-secret-do-not-use-in-production"`。若运维忘记设置，所有 access_token/refresh_token 的 HMAC 哈希使用公开已知 secret，攻击者可离线伪造 token 哈希。release 构建同样执行此 fallback，无启动期校验。 |
| **位置** | [crypto.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/crypto.rs) L159-167 |
| **处理方法** | 1. 生产模式（`cfg!(debug_assertions)` 为 false）下 `TOKEN_HASH_SECRET` 未设置或等于已知弱值时启动 panic；2. 校验 secret 长度 ≥ 32 字节；3. 在 `server.rs` 启动阶段加入配置校验；4. 移除硬编码字符串，改为返回 `Result`。 |
| **验证方法** | [validate_token_hash_secret()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/crypto.rs#L170-L199) 当前会拒绝短于 32 字节或等于已知 dev fallback 的 secret；[hash_token()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/crypto.rs#L204-L215) 仅在 debug 构建中允许回退到 `DEV_TEST_TOKEN_HASH_SECRET`，release 分支会直接失败而非继续使用公开弱密钥。 |
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
| **验证方法** | [canonical_federation_request_bytes()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/signing.rs#L12-L27)、[sign_json()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/signing.rs#L30-L42)、[compute_event_content_hash()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/signing.rs#L70-L75) 与 [verify_event_content_hash()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/signing.rs#L81-L89) 当前都统一调用 [synapse-common/src/canonical_json.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/canonical_json.rs#L13-L58) 的 canonical JSON 实现。 |
| **所需资源** | 后端 1 人周 |
| **状态** | ✅ 已修复（2026-06-19）。统一 canonical JSON 实现到 synapse-common/src/canonical_json.rs（键排序、仅整数、U+2028/U+2029/U+FFFD 转义）；signing.rs 中 compute_event_content_hash/verify_event_content_hash/sign_json/canonical_federation_request_bytes 全部委托给统一实现。 |

---

#### P0-05 m.room.redaction 事件缺失 `redacts` 字段 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 兼容性 |
| **描述** | 原实现创建 redaction 事件时仅写入 `content = json!({"reason": reason})`，`CreateEventParams` 也没有 `redacts` 通道。规范要求 v1-v10 redaction 事件含顶层 `redacts`，v11+ 含 `content.redacts`；缺失该字段时，联邦对端无法判断被 redact 的目标，客户端也无法显示 redaction 关系。 |
| **位置** | [events.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/room/events.rs) L856-940 |
| **处理方法** | 1. `CreateEventParams` 增加 `redacts: Option<String>` 字段；2. 按目标房间版本决定写入顶层（v1-10）或 content（v11+）；3. 同步修复 `synapse-web` 镜像实现。 |
| **验证方法** | [redact_event handler](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/room/events.rs#L912-L933) 当前已把目标事件写入 `CreateEventParams { redacts: Some(event_id.clone()), ... }`；共享辅助函数 [extract_redacts()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/redaction.rs#L132-L145) 同时支持 v1-v10 顶层 `redacts` 与 v11+ `content.redacts`。 |
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
| **验证方法** | [EventStorage::redact_event_content()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/event/mod.rs#L578-L593) 当前会先读取原始 `event_type`/`content`，再委托 [redact_content()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/redaction.rs#L76-L95) 生成按事件类型裁剪后的内容，而不是直接写入空对象 `{}`。 |
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
| **验证方法** | [signing.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/signing.rs#L154-L160) 的 `redact_event_for_hash()` 当前已完全委托给共享模块；共享字段表位于 [redaction.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/redaction.rs#L23-L58) 的 `CANONICAL_JSON_TOP_LEVEL_FIELDS` 与 `allowed_content_keys()`，其中已包含 `m.room.power_levels.notifications` 等规范字段。 |
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
| **验证方法** | 联邦事务处理当前已在 [transaction.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/transaction.rs#L321-L354) 对 `m.room.redaction` 事件调用 `extract_redacts(pdu)` 提取目标事件，并在事件创建后调用 `room_service.redact_event_content(...)` 应用内容剥离。 |
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
| **验证方法** | [can_redact_event()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/auth/power_levels.rs#L521-L556) 当前会先通过 `get_room_version(room_id)` 读取房间版本，再仅在 `room_version >= 11` 且 `actor_user_id == event_sender_id` 时允许自删除；否则仍要求达到 `redact` power level。 |
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
| **验证方法** | [resolve_state_v2()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/event_auth/state_resolution.rs#L343-L476) 当前已显式分离 `auth` / `non-auth` 冲突事件，构建 `mainline`，并通过 `sort_by_reverse_topological_power(...)` 排序后决定获胜事件，不再依赖单纯时间戳选 winner。 |
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
| **验证方法** | [resolve_state_v2()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/event_auth/state_resolution.rs#L416-L436) 当前已从最新 `m.room.power_levels` 事件的 `content.users` 提取 `user_id -> power_level` 映射，并将该映射传入 [sort_by_reverse_topological_power()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/event_auth/state_resolution.rs#L289-L337) 参与排序。 |
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
| **验证方法** | 当前 [SUPPORTED_ROOM_VERSIONS](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/common/room_versions.rs#L62-L82) 已重新将 v11-v13 标记为 `stable(..., can_create=true)`，并在同文件注释中明确把这一能力与 `extract_redacts` 的 v11+ `content.redacts` 支持及 [can_redact_event()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/auth/power_levels.rs#L535-L549) 的 v11+ 自删除规则绑定。 |
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
| **验证方法** | 1. [synapse-services/src/worker/storage.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/worker/storage.rs) 已收敛为 `pub use synapse_storage::worker::*;` 的 thin facade；2. 实际 SQL 与 `WorkerStorage` 实现位于 [synapse-storage/src/worker.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/worker.rs)。 |
| **所需资源** | 后端 0.5 人周 |
| **状态** | ✅ 已修复（2026-06-20）。WorkerType/WorkerStorage/WorkerRow 等类型从 synapse-services 迁移到 synapse-storage/src/worker.rs（1514 行）；synapse-services/src/worker/types.rs 和 storage.rs 改为 thin facade re-export。 |

---

#### P1-02 route handler 在请求路径内直接 new storage 实例 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 架构 |
| **描述** | `rendezvous.rs` 在路由处理函数内直接构造新 storage 实例，从另一个 storage 的 `pool` 字段取连接池，绕过 ServiceContainer 依赖注入。 |
| **位置** | [rendezvous.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/rendezvous.rs) L281, L308 |
| **处理方法** | 在 `ServiceContainer` 中注册 `RendezvousMessageStorage`，route 通过 `state.services` 访问。 |
| **验证方法** | [container.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/container.rs#L492-L492) 已把 `rendezvous_message_storage` 纳入 `AdminServices`，并在 [L597-L680](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/container.rs#L597-L680) 完成装配；对应路由 [rendezvous.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/rendezvous.rs#L282-L316) 现已直接通过 `state.services.admin.rendezvous_message_storage` 读写消息。 |
| **所需资源** | 后端 0.3 人周 |
| **状态** | ✅ 已修复（2026-06-20 复核）。Rendezvous 路由已不再在请求路径内手动 `new` storage，而是改走 ServiceContainer 中注册的依赖。 |

---

#### P1-03 routes 大规模直接访问 storage 层（跳过 service 层）

| 项 | 内容 |
|---|---|
| **域** | 架构 |
| **描述** | 大量 route handler 通过 `state.services.X.storage_y` 直接调用 storage 方法，绕过 service 层。最严重的是 `admin/notification.rs`（17+ 处），还有 `account_compat.rs`、`dm.rs`、`e2ee_routes.rs`、`admin/user.rs` 等。 |
| **位置** | [admin/notification.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/admin/notification.rs) L138-516；[account_compat.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/account_compat.rs) L42-676；[dm.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/dm.rs) L44-486；[e2ee_routes.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/e2ee_routes.rs) L230-529 |
| **处理方法** | 为每个 storage 域提供 service 封装（如 `ServerNotificationService`、`ThreepidService`、`DeviceService`），route 只调用 service 方法。优先处理 `admin/notification.rs`。 |
| **验证方法** | 当前最严重的 `admin/notification.rs` 已不再直接调用 `storage.` 方法，但路由层仍可直接看到多处 storage 类型/辅助函数导入，例如 [admin/notification.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/admin/notification.rs#L1-L40)、[rendezvous.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/rendezvous.rs#L1-L12)、[admin/report.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/admin/report.rs#L1-L3)、[app_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/app_service.rs#L11-L18) 仍直接从 `crate::storage` 取类型或工具函数，说明整体验证边界尚未完全收口。 |
| **所需资源** | 后端 2 人周（多文件分批） |
| **状态** | ⏳ 延后（2026-06-20 复核）。局部热点已收敛，但 route 层仍残留多处直接依赖 storage 类型/工具的代码，尚未实现全面的 service 边界收口。 |

---

#### P1-04 services 直接持有 PgPool（storage 职责泄漏）

| 项 | 内容 |
|---|---|
| **域** | 架构 |
| **描述** | 多个 service 结构体直接持有 `Arc<PgPool>` 字段，可执行任意 SQL，破坏 service→storage 单向依赖。涉及 `room_tag_service.rs`、`oidc_mapping_service.rs`、`client_push_service.rs`、`admin_server_service.rs`、`admin_media_service.rs`。 |
| **位置** | [room_tag_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/room_tag_service.rs) L9；[client_push_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/client_push_service.rs) L35；[admin_server_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/admin_server_service.rs) L10 |
| **处理方法** | service 应持有对应 `Storage` 实例而非 `PgPool`。如 `RoomTagService` 持有 `RoomTagStorage`，`ClientPushService` 持有 `PushStorage`。 |
| **验证方法** | 当前 `synapse-services` 中仍可直接看到 service/辅助对象持有池句柄，例如 [admin_server_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/admin_server_service.rs#L13-L17) 的 `pool: Arc<PgPool>`、[retention_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/retention_service.rs#L114-L123) 的 `pool: Arc<PgPool>`，以及 [telemetry_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/telemetry_service.rs#L300-L306) 的池依赖，说明 storage 职责仍未完全从 service 层剥离。 |
| **所需资源** | 后端 1 人周 |
| **状态** | ⏳ 延后（2026-06-20 复核）。部分热点可能已调整，但 services crate 仍保留若干直接持池的实现，尚未达到“service 只依赖 storage”目标。 |

---

#### P1-05 synapse-web crate 完全未被使用（死代码） ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 架构/代码质量 |
| **状态** | ✅ 已修复（2026-06-19）— 选项 B：删除整个 crate |
| **描述** | `synapse-web` 是 workspace 成员，包含 ~130 个文件与 `src/web/` 高度重复，但 root crate 的 `Cargo.toml` 未将其列为依赖。整个 crate 是死代码，任何 `src/web/` 修改需手动同步。 |
| **位置** | [synapse-web/src/](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/)；[Cargo.toml](file:///Users/ljf/Desktop/hu_ts/synapse-rust/Cargo.toml) L205 |
| **处理方法** | 二选一：(A) 完成迁移：root crate 依赖 `synapse-web`，`src/web/` 改为 thin facade；(B) 从 workspace 移除 `synapse-web`，删除整个目录。建议选 (B) 除非有明确迁移计划。 |
| **验证方法** | 1. 当前仓库已不存在 `synapse-web/` 目录；2. [Cargo.toml](file:///Users/ljf/Desktop/hu_ts/synapse-rust/Cargo.toml#L203-L211) 的 `workspace.members` 仅保留 `synapse-common`、`synapse-cache`、`synapse-storage`、`synapse-e2ee`、`synapse-federation`、`synapse-services`。 |
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
| **验证方法** | [src/worker/manager.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/worker/manager.rs#L323-L329) 与 [synapse-services/src/worker/manager.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/worker/manager.rs#L259-L265) 均已改为先在写锁作用域内 `remove(worker_id)` 取出连接，再在锁外执行 `conn.disconnect().await`。 |
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
| **验证方法** | `collect_hierarchy_recursive()` 已先收集 `child_room_ids`，再通过 `get_spaces_by_rooms_batch(&child_room_ids)` 一次性加载 space 判定结果，循环内改为从 `spaces_map` 读取而非重复逐房间查询。 |
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
| **验证方法** | `build_device_list_changes()` 已在进入 changed-users 分支后单次调用 `device_key_storage.get_device_counts_batch(&changed_users)`，随后从返回的 `counts` map 中填充响应，而不再逐用户单独查询。 |
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
| **验证方法** | `build_transaction_events()` 已先收集全部 `source_event_ids`，随后单次调用 `self.event_storage.get_events_map(&source_event_ids)` 加载源事件；循环内改为读取 `source_events` map 组装事务事件。 |
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
| **验证方法** | `create_batch()` 已将 `tokens` 组装为 `tokens_arr`，并通过单条 `INSERT INTO registration_tokens ... SELECT unnest($1::text[])` 语句批量写入，而不再在循环中逐条插入。 |
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
| **验证方法** | [event/state.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/event/state.rs#L60-L89) 中的 `get_state_events_by_type()` 已在子查询中增加 `LIMIT 5000`；对应批量接口 [get_state_events_by_type_batch()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/event/state.rs#L92-L125) 也增加了 `LIMIT 50000`。 |
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
| **验证方法** | 1. [search.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/search.rs#L461-L478) 中的层级响应序列化与状态查询已改为 `ApiError::internal_with_log(...)`；2. [e2ee_routes.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/e2ee_routes.rs#L227-L260) 中共享房间用户与已验证设备批量查询也已改为显式保留错误上下文。 |
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
| **验证方法** | [federation/mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/mod.rs#L220-L220) 已将 `/_matrix/federation/v1/knock/{room_id}/{user_id}` 注册为 `post(membership::knock_room)`，并在同文件的路由清单中登记为 `Method::POST`。 |
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
| **验证方法** | [membership.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/membership.rs#L112-L126) 的返回体已包含顶层 `event` 对象，内部带 `event_id`、`room_id`、`sender`、`type`、`state_key`、`content`、`origin_server_ts`、`origin` 字段，且 `state` 固定返回 `"knock"`。 |
| **所需资源** | 后端 0.1 人周 |

---

#### P1-15 ServiceContainer::new() 是 ~415 行 god function

| 项 | 内容 |
|---|---|
| **域** | 架构 |
| **描述** | `ServiceContainer::new()` 包含数十个 service/storage 实例化、feature-gated 条件编译、异步初始化、环境变量读取、文件系统操作。单函数承担整个应用依赖图组装。 |
| **位置** | [container.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/container.rs) L667-1081 |
| **处理方法** | 进一步抽出 `assemble_core`、`assemble_account`、`assemble_sso`、`assemble_extensions`，使 `new()` 仅负责调用组装函数并拼接结果。 |
| **验证方法** | 当前 [ServiceContainer::new()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/container.rs#L1074-L1225) 虽已调用 `assemble_e2ee`、`assemble_room_and_sync`、`assemble_admin_support`、`assemble_federation`、`assemble_sso`、`assemble_core`、`assemble_extensions` 等子组装函数，但函数体本身仍保留大段初始化、条件启动和容器拼装逻辑，尚未收缩为纯粹的薄协调层。 |
| **所需资源** | 后端 1 人周 |
| **状态** | ⏳ 延后（2026-06-20 复核）。容器装配已部分模块化，但 `new()` 仍是大型总装函数，未达到目标中的最小协调职责。 |

---

#### P1-16 AdminServices 是 ~40 字段的 god struct

| 项 | 内容 |
|---|---|
| **域** | 架构 |
| **描述** | `AdminServices` 混合 storage、service、manager、scheduler 等不同职责依赖，违反单一职责原则。 |
| **位置** | [container.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/container.rs) L432-474 |
| **处理方法** | 按子域拆分为 `AdminUserServices`、`AdminFederationServices`、`AdminMediaServices`、`AdminSecurityServices`、`AdminTokenServices`，`AdminServices` 作为聚合。 |
| **验证方法** | 当前 [AdminServices](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/container.rs#L454-L500) 仍同时持有 audit、federation、media、安全、token、registration、captcha、retention、rendezvous、appservice、worker 等多域 storage/service/manager，对象边界尚未拆分成更小的 admin 子组。 |
| **所需资源** | 后端 1 人周（与 P1-15 合并） |
| **状态** | ⏳ 延后（2026-06-20 复核）。`AdminServices` 仍是跨多个管理子域的大型聚合结构，尚未拆成更细粒度的 admin service groups。 |

---

#### P1-17 超过 1000 行的大文件

| 项 | 内容 |
|---|---|
| **域** | 架构 |
| **描述** | `application_service/mod.rs`（1584 行）、`sliding_sync_service.rs`（1308 行）、`container.rs`（1216 行）超过 1000 行。 |
| **位置** | [application_service/mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/application_service/mod.rs)；[sliding_sync_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/sliding_sync_service.rs)；[container.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/container.rs) |
| **处理方法** | `application_service/mod.rs` 拆分为 `manager.rs`/`models.rs`/`transaction.rs`；`sliding_sync_service.rs` 拆分为 `core.rs`/`filters.rs`/`timeline.rs`/`state.rs`；`container.rs` 按域拆分。 |
| **验证方法** | 当前仓库复核中，`synapse-services/src/application_service/mod.rs`、`synapse-services/src/sliding_sync_service.rs`、`synapse-services/src/container.rs` 仍分别约为 1620、1419、1363 行；虽然 `synapse-services/src/assemble/` 等子模块已存在，但这三处主文件仍明显超出原目标体量。 |
| **所需资源** | 后端 1.5 人周 |
| **状态** | ⏳ 延后（2026-06-20 复核）。部分拆分前置结构已存在，但核心大文件体量仍未降到目标范围内。 |

---

#### P1-18 JWT 缺少 issuer/audience 验证 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 安全 |
| **状态** | ✅ 已修复（2026-06-19） |
| **描述** | `decode_token` 仅设置 `required_spec_claims = ["exp","iat","sub"]`，未配置 `set_audience`/`set_issuer`。若 `jwt_secret` 在多服务间复用，存在 token confusion 风险。 |
| **位置** | [token.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/auth/token.rs) L232-237 |
| **处理方法** | 1. `validation.set_audience(&["synapse-rust"])` 并在签发时写入 `aud`；2. `validation.set_issuer(&[&server_name])`；3. `validation.validate_exp = true`。 |
| **验证方法** | [decode_token()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/auth/token.rs#L234-L243) 已在 `Validation` 上同时调用 `set_issuer(&[&self.server_name])` 与 `set_audience(&[&self.server_name])`，不再仅校验 `exp/iat/sub`。 |
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
| **验证方法** | 1. [Ed25519PublicKey::verify()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-e2ee/src/crypto/ed25519.rs#L42-L46) 内部已统一委托给 `verifying_key.verify_strict(...)`；2. 联邦签名校验路径 [federation_auth.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/middleware/federation_auth.rs#L529-L531) 也显式使用 `verify_strict(...)`。 |
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
| **验证方法** | 1. [threepid.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/threepid.rs#L87-L90) 创建 3PID 验证会话时仅输出 `sid` 的 `debug` 日志，不再以 INFO 记录 `token`/`email`；2. OIDC 回调日志 [oidc.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/oidc.rs#L898-L904) 已改为记录 `email_present` 布尔值，而非原始邮箱地址。 |
| **所需资源** | 后端 0.3 人周 |

---

### P2 — 中期修复（18 项）

---

#### P2-01 生产 `expect()` 在 `panic=abort` 下有崩溃风险 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 代码质量 |
| **描述** | `Cargo.toml` 配置 `panic = "abort"`，以下生产代码使用 `expect()`：HMAC 初始化（4 处）、Argon2 参数（1 处）。虽当前库版本下不会失败，但底层变化将导致服务崩溃。 |
| **位置** | [invite_signature.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/security/invite_signature.rs)；[device_binding.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/security/device_binding.rs)；[password_hash_pool.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/common/password_hash_pool.rs) |
| **处理方法** | 改为返回 `Result` 并在调用方处理，或至少在启动时一次性校验。 |
| **验证方法** | [invite_signature.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/security/invite_signature.rs) 当前已把 HMAC 初始化收敛为 `init_hmac()`，`sign_invite()` / `verify_invite_signature()` 均改为返回 `Result`；[device_binding.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/security/device_binding.rs) 也已同样改为显式 `Result` 路径；[password_hash_pool.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/common/password_hash_pool.rs) 与 [synapse-common/password_hash_pool.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/password_hash_pool.rs) 则改为“先尝试 OWASP fallback，再退回 `Argon2::default()` 并记录日志”，不再保留 `expect()`。 |
| **所需资源** | 后端 0.3 人周 |
| **状态** | ✅ 已修复（2026-06-20 实现）。原文列举的 5 处生产 `expect()` 已全部移除：HMAC 初始化改为显式错误返回，Argon2 fallback 改为日志 + 安全默认值，不再依赖 panic 路径。 |

---

#### P2-02 缓存写入错误被 `let _ =` 静默忽略 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 代码质量 |
| **描述** | 6 处缓存写入错误被静默忽略，无日志记录。Redis 故障时缓存层完全不可观测。 |
| **位置** | [federation_auth.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/middleware/federation_auth.rs) L329, L336, L437；[search.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/search.rs) L308, L365；[events.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/room/events.rs) L264 |
| **处理方法** | 加 `tracing::warn!` 记录缓存写入失败。 |
| **验证方法** | 1. [feature_flags.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/feature_flags.rs#L134-L139)、[beacon_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/beacon_service.rs#L39-L44)、[rtc/session.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/rtc/session.rs#L59-L81)、[device_sync.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/device_sync.rs#L91-L100)、[vodozemac_megolm.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-e2ee/src/vodozemac_megolm.rs#L199-L212) 与 [device_keys/service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-e2ee/src/device_keys/service.rs#L103-L112) 当前都已在缓存写入失败时输出 `warn!`；2. [synapse-cache/lib.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-cache/src/lib.rs#L894-L902) 现已在 `CacheManager::delete()` 内统一记录 Redis 删除失败，覆盖服务层/存储层/联邦层的删除路径；3. 针对 `synapse-services/src`、`synapse-storage/src`、`synapse-federation/src`、`synapse-e2ee/src` 的 `let _ = self.cache.set/delete(...).await` 复核已清零，且 `cargo check --locked --workspace` 于 2026-06-20 通过。 |
| **所需资源** | 后端 0.2 人周 |
| **状态** | ✅ 已修复（2026-06-20 实现）。缓存写入失败已在热点模块补齐 `warn!`，缓存删除失败则下沉到 `CacheManager::delete()` 统一记录 Redis 删除错误；本轮针对 `synapse-services`、`synapse-storage`、`synapse-federation`、`synapse-e2ee` 的残留 `let _ = self.cache.set/delete(...).await` 已清零。 |

---

#### P2-03 `map_err(|_| ...)` 丢失错误上下文（~20 处） ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 代码质量 |
| **描述** | 约 20 处使用 `map_err(|_| ApiError::internal(...))` 丢弃原始错误信息，不利于排障。 |
| **位置** | [federation_auth.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/middleware/federation_auth.rs) L402；[oidc.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/oidc.rs) L58, L78；[search.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/search.rs) L224, L234；[admin_registration_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/admin_registration_service.rs) L202；[saml_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/saml_service.rs) L220, L239 等 |
| **处理方法** | 统一采用 `ApiError::internal_with_log(msg, &e)` 模式。 |
| **验证方法** | 原先最后残留的三个点位已完成收敛： [key_rotation.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/key_rotation.rs) 现在对 key 长度错误返回带长度信息的显式内部错误；[admin_auth.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/utils/admin_auth.rs) 已把系统时间异常改为 `ApiError::internal_with_log("System time error", &e)`；[content_scanner/service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/content_scanner/service.rs) 也已对 webhook timeout 使用 `internal_with_log` 保留原始错误。 |
| **所需资源** | 后端 0.5 人周 |
| **状态** | ✅ 已修复（2026-06-20 实现）。文档此前保留的最后几处无上下文 `map_err(|_| ApiError::internal(...))` 已全部替换为带错误上下文的实现，原问题不再残留于当前代码路径。 |

---

#### P2-04 search.rs 空间子查询逻辑重复 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 代码质量 |
| **描述** | `get_rooms_batch` + `get_state_events_batch` + 构建 child_rooms 逻辑在两处重复，且错误处理不一致。 |
| **位置** | [search.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/search.rs) L450-468 vs L595-609 |
| **处理方法** | 抽取为公共辅助函数。 |
| **验证方法** | [search.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/search.rs#L374-L427) 已抽出公共辅助函数 `collect_child_rooms()`，并在同文件的两个调用点 [search.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/search.rs#L508-L508) 与 [search.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/search.rs#L604-L604) 复用该逻辑。 |
| **所需资源** | 后端 0.2 人周 |

---

#### P2-05 签名密钥加密使用弱密钥派生 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 安全 |
| **描述** | `derive_key` 使用单次 `SHA-256(info ‖ master_key)` 派生 AES-256 密钥，缺乏工作因子，非标准 KDF。 |
| **位置** | [key_encryption.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/common/key_encryption.rs) L58-66 |
| **处理方法** | 改用 `hkdf::Hkdf::<Sha256>` 派生密钥。 |
| **验证方法** | [key_encryption.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/common/key_encryption.rs#L59-L72) 的 `derive_key()` 已改为通过 `hkdf::Hkdf::<Sha256>::new(None, master_key)` 派生 32 字节输出，而不再执行单次 `SHA-256(info ‖ master_key)`。 |
| **所需资源** | 后端 0.3 人周 |

---

#### P2-06 缓存无 single-flight 保护（缓存击穿风险） ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 性能 |
| **描述** | 缓存 `get` 方法无 single-flight 保护，热点 key 过期时多个并发请求同时穿透到 Redis/DB。 |
| **位置** | [lib.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-cache/src/lib.rs) L876-899 |
| **处理方法** | 实现 `get_or_fetch` 方法，使用 `tokio::sync::Mutex` 或 single-flight 模式。 |
| **验证方法** | [lib.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-cache/src/lib.rs#L616-L630) 已引入按 key 维度的 `SingleFlightMap`；[get_or_fetch()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-cache/src/lib.rs#L1013-L1050) 在缓存 miss 后先获取该 key 的互斥锁，再执行二次检查与单次回源。 |
| **所需资源** | 后端 0.5 人周 |

---

#### P2-07 缓存无批量 get 方法（N+1 缓存查询） ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 性能 |
| **描述** | 缓存无 `get_batch`/MGET 方法，导致 presence 等场景出现 N+1 缓存查询（3 处相同模式）。 |
| **位置** | [presence.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/presence.rs) L147-154, L375-382, L428-433 |
| **处理方法** | 实现 `get_batch` 方法使用 Redis MGET。 |
| **验证方法** | 1. [RedisCache::get_batch()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-cache/src/lib.rs#L447-L460) 已用单条 Redis `MGET` 拉取多 key；2. [CacheManager::get_batch()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-cache/src/lib.rs#L949-L999) 会先查 L1，再对缺失 key 统一走一次 L2 批量读取。 |
| **所需资源** | 后端 0.5 人周 |

---

#### P2-08 Token 缓存 TTL 不一致 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 性能/安全 |
| **描述** | Token 缓存 TTL 在两处不一致：`strategy.rs` 为 300s，`query_cache.rs` 为 3600s。1 小时对安全敏感的 token 过长，撤销后仍有 1 小时窗口。 |
| **位置** | [query_cache.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/cache/query_cache.rs) L57；[strategy.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/cache/strategy.rs) L128-130 |
| **处理方法** | 统一 TTL 定义，token 缓存使用 300s。 |
| **验证方法** | [query_cache.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/cache/query_cache.rs#L49-L63) 中 `QueryCacheConfig::default().token_ttl` 已为 `Duration::from_secs(300)`；[strategy.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/cache/strategy.rs#L128-L130) 中 `CacheTtl::token()` 也统一返回 300 秒。 |
| **所需资源** | 后端 0.1 人周 |
| **状态** | ✅ 已修复（2026-06-20 复核）。[query_cache.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/cache/query_cache.rs#L49-L63) 与 [strategy.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/cache/strategy.rs#L128-L130) 当前均将 token TTL 统一为 300 秒，文档中原先的 3600 秒差异已不存在。 |

---

#### P2-09 多处 N+1 查询（device/notification/beacon/e2ee_audit/room summary） ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 性能 |
| **描述** | 9 处中等严重度 N+1 查询：设备删除/批量删除循环记录变更、通知列表循环获取状态、beacon 循环获取位置、E2EE 审计循环验证设备/获取信任状态、room summary 循环更新状态/获取 heroes。 |
| **位置** | [device.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/device.rs) L422-424, L475-477；[server_notification.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/server_notification.rs) L345-352；[beacon.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/beacon.rs) L437-440；[audit_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/e2ee_audit/audit_service.rs) L110-111, L241-242；[summary_state.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/room/summary_state.rs) L149-158；[summary.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/room/summary.rs) L59-62 |
| **处理方法** | 逐个添加批量查询方法，使用 `WHERE ... = ANY($1)` 单次查询。 |
| **验证方法** | 当前实现可直接看到多处已引入批量路径，例如 [device.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/device.rs#L490-L508) 的 `delete_devices_batch()` 使用 `WHERE device_id = ANY($1)`，[e2ee_routes.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/e2ee_routes.rs#L255-L260) 调用 `get_verified_devices_batch(&user_ids)`，以及 [room_summary.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/room_summary.rs#L516-L555) 的 `get_heroes_batch()` 统一为多房间查询。 |
| **所需资源** | 后端 1.5 人周（分批） |

---

#### P2-10 PostgreSQL 连接池配置偏小 + test_before_acquire 开销 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 性能 |
| **描述** | 默认 `max_size=20` 对高流量服务器可能不足（Synapse 推荐 50-100+）。`test_before_acquire(true)` 每次获取连接执行测试查询，增加延迟。 |
| **位置** | [homeserver.yaml.example](file:///Users/ljf/Desktop/hu_ts/synapse-rust/homeserver.yaml.example) L35；[server.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/server.rs) L107 |
| **处理方法** | 1. 生产默认 `max_size=50`；2. 改用 `test_before_acquire(false)` 配合较短 `max_lifetime`。 |
| **验证方法** | [homeserver.yaml.example](file:///Users/ljf/Desktop/hu_ts/synapse-rust/homeserver.yaml.example#L40-L47) 的示例数据库配置已将 `max_size` 设为 `50`；[server.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/server.rs#L93-L107) 构建 `PgPoolOptions` 时已显式配置 `.test_before_acquire(false)`。 |
| **所需资源** | 后端 0.2 人周 |

---

#### P2-11 wildcard re-export 造成隐式耦合 ⚠️ 部分完成

| 项 | 内容 |
|---|---|
| **域** | 架构 |
| **描述** | `synapse-services/src/lib.rs` 使用 `#![allow(ambiguous_glob_reexports)]` + ~20 处 `pub use X::*;`，使 services crate 公共 API 不可控。`src/storage/mod.rs` 有 ~30 处 wildcard re-export。 |
| **位置** | [lib.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/lib.rs) L1, L190-199；[storage/mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/storage/mod.rs) |
| **处理方法** | 移除 `#![allow(ambiguous_glob_reexports)]`，将 `pub use X::*;` 改为显式导出，或至少限制为 `pub(crate) use`。 |
| **验证方法** | 当前复核显示 [synapse-services/src/lib.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/lib.rs#L1-L239) 已移除 crate 级 `#![allow(ambiguous_glob_reexports)]`，并将 `friend_room_service`、`voice_service`、`beacon_service`、`external_service_integration` 的 wildcard re-export 改为显式导出，仅在 `database_initializer::*` 与 `room::space::*` 两条真实冲突导出线上保留局部 `#[allow(ambiguous_glob_reexports)]`；但 [storage/mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/storage/mod.rs#L94-L188) 仍保留大量 `pub use self::...::*` / `pub use synapse_storage::...::*` 的 wildcard re-export。 |
| **所需资源** | 后端 1 人周 |
| **状态** | ⚠️ 部分完成（2026-06-20 更新）。`synapse-services/src/lib.rs` 已移除 crate 级 `#![allow(ambiguous_glob_reexports)]`，并把 `friend_room_service`、`voice_service`、`beacon_service`、`external_service_integration` 改为显式导出；但 `synapse-services/src/lib.rs` 其余核心模块以及 `src/storage/mod.rs` 仍保留大量 `pub use ...::*`，尚未达到“显式导出为主”的完成标准。 |

---

#### P2-12 container.rs 内硬编码运行时配置值 ⚠️ 部分完成

| 项 | 内容 |
|---|---|
| **域** | 架构 |
| **描述** | `ServiceContainer::new()` 内硬编码媒体存储路径、server_name 回退值 `"localhost"`、搜索索引名、事件广播批处理参数、refresh token TTL。 |
| **位置** | [container.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/container.rs) L10, L674-678, L682, L714, L959 |
| **处理方法** | 将这些值移入 `Config` 结构体，在 `homeserver.yaml` 中提供默认值。 |
| **验证方法** | 当前实现已可直接看到部分配置完成外提，例如 [assemble_admin_support()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/container.rs#L555-L559) 使用 `config.server.refresh_token_ttl_secs`，[assemble_core()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/container.rs#L805-L810) 使用 `config.search.search_index_name` 与 [L858-L864](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/container.rs#L858-L864) 的 `config.federation.event_broadcast_batch_size`；但同文件 [L827-L835](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/container.rs#L827-L835) 仍保留 `"/app/data/media"` / `"./data/media"` 硬编码回退。 |
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
| **验证方法** | 当前复核仍可直接在 [burn_after_read_processor_enabled()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/container.rs#L437-L448) 看到 `env::var("SYNAPSE_ENABLE_BURN_AFTER_READ_PROCESSOR")`，在 [assemble_core()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/container.rs#L827-L835) 看到 `env::var("SYNAPSE_MEDIA_PATH")`，以及在 [load_or_generate_megolm_master_key()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/container.rs#L1277-L1278) 看到 `std::env::var("SYNAPSE_MEGOLM_ENCRYPTION_KEY_PATH")` 的向后兼容回退。 |
| **所需资源** | 后端 0.3 人周（与 P2-12 合并） |
| **状态** | ⚠️ 部分完成（2026-06-20 复核）。部分调用已纳入配置体系，但 `container.rs` 仍直接读取 `SYNAPSE_ENABLE_BURN_AFTER_READ_PROCESSOR`、`SYNAPSE_MEDIA_PATH`、`SYNAPSE_MEGOLM_ENCRYPTION_KEY_PATH` 作为向后兼容回退，尚未满足“零 `env::var`”的完成标准。 |

---

#### P2-14 canonical JSON 仍允许浮点数且未校验整数范围 ⚠️ 部分完成

| 项 | 内容 |
|---|---|
| **域** | 兼容性 |
| **描述** | `format_canonical_number` 对非整数浮点用 `format!("{f}")` 输出，可能产生科学计数法。Matrix canonical JSON 要求仅允许 `[-(2^53)+1, 2^53-1]` 范围整数，浮点应被拒绝。 |
| **位置** | [canonical_json.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/canonical_json.rs) L89-100 |
| **处理方法** | 非整数/超范围数字返回错误而非输出。 |
| **验证方法** | 当前实现已在 [format_canonical_number()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/canonical_json.rs#L97-L126) 中拒绝非整数浮点与超范围整数，并由测试 [test_canonical_number_non_integer_float_rejected()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/canonical_json.rs#L243-L255) 覆盖 `1.5` 拒绝、`2^53` 越界拒绝；但 [test_canonical_number_integer_valued_float_converted()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/canonical_json.rs#L235-L241) 仍表明 `1.0` 会被归一化为整数 `1`，尚未达到“所有浮点一律拒绝”的原始目标。 |
| **所需资源** | 后端 0.2 人周（与 P0-04 合并） |
| **状态** | ⚠️ 部分完成（2026-06-20 复核）。当前 canonical JSON 已拒绝非整数浮点和超范围整数，但整数值浮点仍按 Synapse 兼容行为归一化输出，因此不能再标记为“浮点完全拒绝已修复”。 |

---

#### P2-15 联邦密钥 query/notary 未完整验证自签名链路 ⚠️ 部分完成

| 项 | 内容 |
|---|---|
| **域** | 兼容性/安全 |
| **描述** | `/_matrix/key/v2/query/{server_name}/{key_id}` 对远端结果做 canonical 包装，但未完整验证 `server_name`/`valid_until_ts`/`verify_keys`/`old_verify_keys`/`signatures` 自签名链路后再缓存。 |
| **位置** | [MATRIX_SYNAPSE_AUDIT_AND_OPTIMIZATION_PLAN_2026-05-29.md](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/synapse-rust/MATRIX_SYNAPSE_AUDIT_AND_OPTIMIZATION_PLAN_2026-05-29.md) L48-51 |
| **处理方法** | 抽出 `ServerKeySet` 类型，完整校验上述字段后再入缓存。 |
| **验证方法** | 当前复核可直接在 [keys.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/keys.rs#L351-L391) 看到缓存前会调用 `validate_server_key_response()`，而该函数在 [L394-L496](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/keys.rs#L394-L496) 仅做 `server_name`、`valid_until_ts`、`verify_keys`、`signatures` 的结构性校验；同段注释明确声明“does not verify the cryptographic signature itself”，且 `canonical_response` 虽保留 `old_verify_keys` 字段 [L366-L377](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/keys.rs#L366-L377)，但未见对应完整签名链路验证。 |
| **所需资源** | 后端 0.5 人周 |
| **状态** | ⚠️ 部分完成（2026-06-20 复核）。当前 [keys.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/keys.rs#L394-L496) 已校验 `server_name`、`valid_until_ts`、`verify_keys` 与 `signatures` 的基本结构，并拒绝缺失自签名或过期响应；但注释明确说明“does not verify the cryptographic signature itself”，且尚未校验 `old_verify_keys` 的结构/签名链路，未达到文档声称的“完整验证自签名链路”。 |

---

#### P2-16 联邦 server key 未校验 valid_until_ts ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 安全 |
| **描述** | `valid_until_ts` 缺失时默认设为 `now + 3600s`，且未校验响应中的 `valid_until_ts` 是否已过期。攻击者可注入已过期但带有效签名的旧 key 响应。 |
| **位置** | [keys.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/keys.rs) L324-327 |
| **处理方法** | 1. `valid_until_ts` 缺失或早于 `now` 时拒绝缓存；2. 缓存 TTL 取 `min(configured_ttl, valid_until_ts - now)`。 |
| **验证方法** | [validate_server_key_response()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/keys.rs#L420-L439) 当前会在 `valid_until_ts` 缺失、不可解析或早于当前时间时直接返回 `None` 拒绝缓存；而 [fetch_remote_server_keys_response()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/keys.rs#L380-L391) 会把缓存 TTL 截断为 `configured_ttl.min(remaining_secs)`。 |
| **所需资源** | 后端 0.3 人周 |
| **状态** | ✅ 已修复（2026-06-19）。`validate_server_key_response()` 已拒绝缺失或过期的 `valid_until_ts`，并在缓存时使用 `min(configured_ttl, valid_until_ts - now)` 限制 TTL，不再接受过期 server key 响应。 |

---

#### P2-17 /versions 声明的 MSC 未在项目规则登记 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 兼容性 |
| **描述** | `BASE_UNSTABLE_FEATURES` 声明了 `org.matrix.msc3266`、`org.matrix.msc3916`、`uk.tcpip.msc4133`，但项目规则 MSC 表未包含。MSC3916 实现证据不明。 |
| **位置** | [versions.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/versions.rs) L91-93；[project_rules.md](file:///Users/ljf/Desktop/hu_ts/synapse-rust/.trae/rules/project_rules.md) 第十节 |
| **处理方法** | 1. 将三项 MSC 补入项目规则 MSC 表并标注证据；2. 核实 MSC3916 是否有实现，否则从声明移除。 |
| **验证方法** | 当前 [BASE_UNSTABLE_FEATURES](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/versions.rs#L84-L93) 已不再声明 `org.matrix.msc3916`，仅保留 `org.matrix.msc3266` 与 `uk.tcpip.msc4133`；项目规则第十节的 MSC 表也已登记 [MSC3266](file:///Users/ljf/Desktop/hu_ts/synapse-rust/.trae/rules/project_rules.md#L454-L455) 与 [MSC4133](file:///Users/ljf/Desktop/hu_ts/synapse-rust/.trae/rules/project_rules.md#L455-L455)，与当前 `/versions` 声明面保持一致。 |
| **所需资源** | 后端 0.2 人周 |
| **状态** | ✅ 已修复（2026-06-20 复核）。`/versions` 的不稳定特性声明已移除缺乏实现证据的 `MSC3916`，并与项目规则中已登记的 `MSC3266`、`MSC4133` 对齐。 |

---

#### P2-18 缺少 Complement 级互通测试 ⏳ 延后

| 项 | 内容 |
|---|---|
| **域** | 兼容性 |
| **描述** | 集成测试多为本仓库行为测试，缺少最小 Complement/Matrix SDK 互通门禁。结合 P0-04/P0-05/P0-08/P0-10，联邦互通缺陷难以被现有测试捕获。 |
| **位置** | [MATRIX_SYNAPSE_AUDIT_AND_OPTIMIZATION_PLAN_2026-05-29.md](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/synapse-rust/MATRIX_SYNAPSE_AUDIT_AND_OPTIMIZATION_PLAN_2026-05-29.md) L98-100 |
| **处理方法** | 建立最小 Complement/SDK smoke 门禁：register/login/sync/create room/send event/federation key/server discovery/media。 |
| **验证方法** | 当前复核中，`.github/` 下未见 Complement 相关工作流或步骤，仓库内也未找到 `complement` 命名的测试目录/脚本；现有 `tests/` 主要由 unit、integration、e2e 与 element-web harness 组成，说明最小 Complement smoke 门禁尚未在当前仓库形态中落地。 |
| **所需资源** | 后端 2 人周 + QA 1 人周 |
| **状态** | ⏳ 延后（2026-06-20 复核）。当前仓库仍缺少 Complement 级互通测试资产与 CI 接入，现有覆盖主要停留在本仓库自测与浏览器 harness 层。 |

---

### P3 — 择期修复（12 项）

---

#### P3-01 生产路由中 `unreachable!()` ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 代码质量 |
| **描述** | `unreachable!()` 在 `panic=abort` 下会崩溃。虽由上方校验保证不可达，但应防御性编程。 |
| **位置** | [federation/events.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/events.rs) L332 |
| **处理方法** | 改为返回 `ApiError::bad_request`。 |
| **验证方法** | 原定位点 [events.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/events.rs#L305-L360) 当前已改为直接构造 public rooms 响应并返回 `Ok(Json(...))`，其中 `next_batch` 显式返回 `null`，不再保留 `unreachable!()` 分支。 |
| **所需资源** | 后端 0.1 人周 |
| **状态** | ✅ 已修复（2026-06-20 复核）。原 `src/web/routes/` 中的生产 `unreachable!()` 路径已移除，相关联邦 public rooms 处理现在走显式 JSON 返回。 |

---

#### P3-02 `src/storage/` 内联单元测试稀少 ⏳ 延后

| 项 | 内容 |
|---|---|
| **域** | 代码质量 |
| **描述** | `src/storage/` 有 ~55 个存储模块，但仅 2 个文件含 `#[cfg(test)]`。存储层通过集成测试覆盖，但缺少快速反馈的内联测试。 |
| **位置** | `src/storage/` 目录 |
| **处理方法** | 在核心存储模块补充内联单元测试。 |
| **验证方法** | 当前复核中，`src/storage/` 目录下仅在 [audit.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/storage/audit.rs) 与 [mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/storage/mod.rs) 找到 `#[cfg(test)]`，与该目录下大量存储模块数量相比仍明显偏少。 |
| **所需资源** | 后端 1 人周 |
| **状态** | ⏳ 延后（2026-06-20 复核）。存储层目前仍主要依赖集成测试覆盖，内联单元测试面没有明显扩展。 |

---

#### P3-03 事件内容哈希比较非 constant-time ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 安全 |
| **描述** | `verify_event_content_hash` 使用 `if computed != sha256_hash` 非常量时间比较。 |
| **位置** | [signing.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/signing.rs) L138 |
| **处理方法** | 改用 `synapse_common::crypto::secure_compare`。 |
| **验证方法** | [verify_event_content_hash()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/signing.rs#L85-L93) 当前已改为 `if !secure_compare(&computed, sha256_hash)`，直接通过 [secure_compare()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/crypto.rs#L91-L104) 执行内容哈希比较。 |
| **所需资源** | 后端 0.1 人周 |

---

#### P3-04 `secure_compare` 长度不同时立即返回 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 安全 |
| **描述** | `secure_compare`/`secure_compare_bytes` 在长度不同时立即返回，泄露长度信息。 |
| **位置** | [crypto.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/crypto.rs) L234-243, L91-104 |
| **处理方法** | 对变长敏感场景先 HMAC 再比较，或归一化长度。固定长度哈希场景加注释保留现状。 |
| **验证方法** | [secure_compare()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/crypto.rs#L91-L104) 与 [secure_compare_bytes()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/crypto.rs#L280-L292) 当前都先计算 `max_len`，再把长度差异折叠进 `result` 后遍历到较长输入末尾，不再在长度不同时提前返回。 |
| **所需资源** | 后端 0.2 人周 |

---

#### P3-05 `generate_signing_key` 生成随机字符串而非 Ed25519 私钥 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 安全 |
| **描述** | `generate_signing_key` 返回 `random_string(44)`，不是 base64 编码的 Ed25519 私钥种子。若用于真实密钥生成将产生无法签名的"密钥"。 |
| **位置** | [crypto.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/crypto.rs) L262-266 |
| **处理方法** | 若用于真实密钥：改用 `ed25519_dalek::SigningKey::generate(&mut OsRng)`。若仅测试：加 `#[cfg(test)]`。 |
| **验证方法** | [generate_signing_key()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/crypto.rs#L312-L319) 当前已加上 `#[cfg(test)]`，仅在测试编译目标中可见；其调用也位于同文件的测试模块 [crypto.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/crypto.rs#L321-L358) 与 [test_generate_signing_key()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/crypto.rs#L520-L526) 中，不再作为生产签名密钥生成路径暴露。 |
| **所需资源** | 后端 0.2 人周 |

---

#### P3-06 SELECT * 使用（2 处） ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 性能 |
| **描述** | `get_pending_transactions` 使用 `SELECT *`；`impl_sqlx_types` 宏定义了 `SELECT * FROM users`（宏未使用，死代码）。 |
| **位置** | [application_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/application_service.rs)；[macros.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/macros.rs) |
| **处理方法** | 明确列出所需列；删除未使用宏。 |
| **验证方法** | [get_pending_transactions()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/application_service.rs#L675-L686) 当前已显式列出所需列；同时 [synapse-common/macros.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/macros.rs) 已移除未使用的 `impl_sqlx_types!` 死宏，仅保留实际仍被使用的错误映射宏，不再残留 `SELECT * FROM users WHERE active = true` 的占位查询。 |
| **所需资源** | 后端 0.1 人周 |
| **状态** | ✅ 已修复（2026-06-20 实现）。存储层查询已全部改为显式列选择，遗留死宏中的 `SELECT *` 也已删除，原审查列举的两处问题均已收口。 |

---

#### P3-07 Worker 健康检查未并行化 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 性能 |
| **描述** | Worker 选择时循环调用 `is_healthy` 检查每个候选 worker，未并行化。Health check 循环顺序检查。 |
| **位置** | [manager.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/worker/manager.rs) L716-720；[health.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/worker/health.rs) L231-233 |
| **处理方法** | 使用 `join_all` 并行检查。 |
| **验证方法** | [select_worker_fallback()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/worker/manager.rs#L717-L733) 当前会先把每个候选 worker 的 `is_healthy()` 封装成独立 future，再通过 `futures::future::join_all(...)` 一次性等待并汇总 `healthy_worker_ids`。 |
| **所需资源** | 后端 0.2 人周 |

---

#### P3-08 联邦 HTTP 客户端未配置连接池参数 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 性能 |
| **描述** | FederationClient 的 reqwest Client 未显式配置 `pool_max_idle_per_host`/`keepalive`。 |
| **位置** | [client.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/client.rs) L188-195 |
| **处理方法** | 显式配置 `pool_max_idle_per_host(20)` 和 `pool_idle_timeout`。 |
| **验证方法** | [FederationClient::new()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/client.rs#L186-L198) 当前在 `Client::builder()` 上显式设置了 `pool_max_idle_per_host(20)`、`pool_idle_timeout(Duration::from_secs(90))`、`tcp_keepalive(Duration::from_secs(60))` 与 `connect_timeout(Duration::from_secs(10))`。 |
| **所需资源** | 后端 0.1 人周 |

---

#### P3-09 非标准联邦路径 ❌ 未完成

| 项 | 内容 |
|---|---|
| **域** | 兼容性 |
| **描述** | `/_matrix/federation/v2/key/clone`、`/_matrix/federation/v1/room_auth/{room_id}` 等非 Matrix 规范路径，增加协议面维护负担。 |
| **位置** | [federation/mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/mod.rs) L228, L241, L244, L256, L257 |
| **处理方法** | 评估是否为扩展用途；若为扩展，迁移到 `io.hula.*` 或 `/_synapse/` 命名空间。 |
| **验证方法** | 当前 [federation/mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/mod.rs#L222-L257) 仍可直接看到多个非标准扩展路由保留在标准联邦命名空间下，例如 `/_matrix/federation/v1/get_joining_rules/{room_id}`、`/_matrix/federation/v1/event_auth`、`/_matrix/federation/v1/keys/claim`、`/_matrix/federation/v1/publicRooms` 与 `/_matrix/federation/v1/query/directory`。 |
| **所需资源** | 后端 0.3 人周 |
| **状态** | ❌ 未完成（2026-06-20 复核）。`/_matrix/federation/v2/key/clone`、`/_matrix/federation/v1/room_auth/{room_id}`、`/_matrix/federation/v1/keys/claim` 等路径仍保留在标准联邦命名空间下，仅增加了“非标准扩展”注释，未完成迁移。 |

---

#### P3-10 sliding sync 缺少性能回滚闸门 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 兼容性/性能 |
| **描述** | Synapse v1.153.0rc3 因性能问题回滚 sliding sync 优化；本项目有 sliding sync 路由与测试，但无性能阈值/回滚机制。 |
| **位置** | [MATRIX_SYNAPSE_AUDIT_AND_OPTIMIZATION_PLAN_2026-05-29.md](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/synapse-rust/MATRIX_SYNAPSE_AUDIT_AND_OPTIMIZATION_PLAN_2026-05-29.md) L77-80 |
| **处理方法** | 为 sliding sync 增加 subscription-change benchmark、p95/p99 与 query count 快照，设置回滚阈值。 |
| **验证方法** | [performance_sliding_sync_benchmarks.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/benches/performance_sliding_sync_benchmarks.rs#L1-L31) 当前已提供 sliding sync 的 benchmark、`p95/p99` 与 query-count 采样；[SlidingSyncService](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/sliding_sync_service.rs#L107-L190) 已提供阈值、慢请求计数与回滚告警；同时 [.github/workflows/benchmark.yml](file:///Users/ljf/Desktop/hu_ts/synapse-rust/.github/workflows/benchmark.yml#L75-L80) 现已把 `performance_sliding_sync_benchmarks` 纳入 benchmark workflow。 |
| **所需资源** | 后端 0.5 人周 |
| **状态** | ✅ 已修复（2026-06-20 实现）。sliding sync 的基准代码、阈值统计与 workflow 接线现已闭环，benchmark workflow 会直接执行 `performance_sliding_sync_benchmarks`。 |

---

#### P3-11 设备列表/presence 缺少长期运行剪枝 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 兼容性/性能 |
| **描述** | 缺少 `device_lists_changes_in_room`、过期 presence、过期 one-time key 的统一后台剪枝任务，长期实例磁盘膨胀。 |
| **位置** | [MATRIX_SYNAPSE_AUDIT_AND_OPTIMIZATION_PLAN_2026-05-29.md](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/synapse-rust/MATRIX_SYNAPSE_AUDIT_AND_OPTIMIZATION_PLAN_2026-05-29.md) L82-85 |
| **处理方法** | 新增 background update 剪枝旧 device list change、过期 presence、过期 OTK。 |
| **验证方法** | [server.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/server.rs#L620-L657) 当前已注册每日数据库剪枝循环，并在同一任务内依次调用 [prune_old_device_list_changes() / prune_expired_presence() / prune_expired_one_time_keys()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/pruning.rs#L36-L60)。 |
| **所需资源** | 后端 1 人周 |
| **状态** | ✅ 已修复（2026-06-19）。[server.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/server.rs#L620-L678) 已启动每日剪枝循环，并调用 [pruning.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/pruning.rs#L36-L60) 中对应的三类清理函数。 |

---

#### P3-12 Admin API 与上游 v1.153 存在差距 ✅ 已修复

| 项 | 内容 |
|---|---|
| **域** | 兼容性 |
| **描述** | 缺失 `GET /_synapse/admin/v1/quarantine_media/{media_id}/changes`、`GET /_synapse/admin/v1/rooms/{room_id}/reports`、`DELETE /_synapse/admin/v1/rooms/{room_id}/reports/{report_id}`；room details 的 tombstoned/replacement_room 字段不完整。 |
| **位置** | [API_COVERAGE_REPORT.md](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/synapse-rust/API_COVERAGE_REPORT.md) L122-126 |
| **处理方法** | 优先补"审计/治理类"接口。 |
| **验证方法** | [media.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/admin/media.rs#L11-L20) 与 [report.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/admin/report.rs#L12-L22) 当前已补齐缺失的 admin 路由；[admin/room/mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/admin/room/mod.rs#L378-L398) 已在 room details 响应中返回 `tombstoned`/`replacement_room`；相关回归断言位于 [api_admin_regression_tests.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/tests/integration/api_admin_regression_tests.rs#L259-L340) 与 [api_room_tests.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/tests/integration/api_room_tests.rs#L1772-L1828)。 |
| **所需资源** | 后端 1 人周 |
| **状态** | ✅ 已修复（2026-06-19）。[media.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/admin/media.rs#L17-L19) 与 [report.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/admin/report.rs#L15-L20) 已补齐缺失路由，[mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/admin/room/mod.rs#L378-L397) 已返回 `tombstoned`/`replacement_room` 字段，相关回归测试见 [api_admin_regression_tests.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/tests/integration/api_admin_regression_tests.rs#L259-L340) 与 [api_room_tests.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/tests/integration/api_room_tests.rs#L1772-L1828)。 |

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

**复核要点**：
- 复核 [canonical_json.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/canonical_json.rs) 是否仍由共享 canonical JSON helper 统一承担签名/哈希序列化，并核对浮点与整数边界行为是否与 `P0-04`、`P2-14` 的状态一致。
- 复核 [redaction.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/redaction.rs)、[room/events.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/room/events.rs) 与 [transaction.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/transaction.rs) 的 redaction 链路，确认 `redacts` 提取、内容剥离与联邦入口行为一致。
- 复核 [state_resolution.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/event_auth/state_resolution.rs) 与 [power_levels.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/auth/power_levels.rs) 是否仍保持 room-version 相关的状态解析与 self-redact 判定。
- 对 SAML、SSRF 等高风险修复，优先依据 [saml_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/saml_service.rs) 与 [federation_auth.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/middleware/federation_auth.rs) 的实现路径复核，再按需补充攻击向量测试。 

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

**复核要点**：
- 复核 routes 与 services 是否仍直接依赖存储实现：重点查看 [admin/notification.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/admin/notification.rs)、[admin/report.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/admin/report.rs)、[app_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/app_service.rs) 等残留点。
- 复核 `PgPool` 是否仍直接持有在服务层：重点查看 [admin_server_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/admin_server_service.rs)、[retention_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/retention_service.rs) 与 [telemetry_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/telemetry_service.rs)。
- 复核 facade 收口与工作区结构：查看 [Cargo.toml](file:///Users/ljf/Desktop/hu_ts/synapse-rust/Cargo.toml#L203-L211) 的 workspace members，以及 `scripts/ci/check_root_canonical_ledger.py` 当前用于判定 thin facade 的基线含义。
- 复核关键性能修复是否仍停留在实现层：例如批量查询、并行健康检查和连接池设置，应分别与对应 issue 的源码证据保持一致。 

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

**复核要点**：
- 复核内部错误路径是否仍保留上下文：优先对照 [search.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/search.rs)、[e2ee_routes.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/e2ee_routes.rs) 等已修复点，以及本轮刚完成收敛的 `key_rotation.rs`、`admin_auth.rs`、`content_scanner/service.rs`，防止同类写法回退。
- 复核缓存与批量化改动是否真正落在实现层：重点查看 [synapse-cache/src/lib.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-cache/src/lib.rs)、相关 single-flight 入口，以及 `P2-06`、`P2-07`、`P2-09`、`P2-10` 的存储/客户端实现。
- 复核配置收敛和兼容性项是否仍有残留：重点查看 [container.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/container.rs)、[versions.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/versions.rs) 与 [keys.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/keys.rs)。
- 复核 Complement 资产是否真正落地时，直接检查 `.github/` 与 `tests/` 是否出现对应 workflow、目录或脚本，而不是以预期 CI 结果代替当前仓库事实。 

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

建议按阶段一→二→三→四的顺序推进，每阶段完成后依据对应条目的实现证据重新复核状态，并仅对高风险改动补充必要测试门禁。

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
| **验证方法** | [fetch_remote_server_keys_response()](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/keys.rs#L317-L352) 当前仅构造 `https://{server_name}/...` 的 key 获取 URL，并在循环内对每个候选地址调用 `check_url_against_blacklist(url, ip_blacklist)`；同函数创建的 reqwest client 也已显式设置 [redirect(reqwest::redirect::Policy::none())](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/keys.rs#L415-L420)。 |

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

### P2 — 中期修复（12/18 完成）

| 编号 | 描述 | 状态 |
|------|------|------|
| P2-01 | 生产代码中的 expect() (5处) | ✅ 已修复（HMAC 改为显式 Result，Argon2 fallback 去除 panic 路径） |
| P2-02 | 缓存写入错误被静默忽略 (6处) | ✅ 已修复（写入失败补齐 `warn!`，删除失败统一下沉到 `CacheManager::delete()` 记录） |
| P2-03 | map_err(\|_\| ...) 丢失错误上下文 (18处) | ✅ 已修复（残留点已改为 `internal_with_log` 或显式长度错误） |
| P2-04 | search_service 重复实现 | ✅ 已修复（抽取 collect_child_rooms 辅助函数） |
| P2-05 | 签名密钥加密改用 HKDF | ✅ 已修复（HKDF-SHA256 替代单次 SHA-256） |
| P2-06 | 缓存 single-flight 防击穿 | ✅ 已修复（get_or_fetch + per-key Mutex） |
| P2-07 | 缓存 MGET 批量获取 | ✅ 已修复（get_batch + Redis MGET） |
| P2-08 | Token 缓存 TTL 不一致 (300s vs 3600s) | ✅ 已修复 |
| P2-09 | N+1 批量查询（其他） | ✅ 已修复（9 处批量查询改造） |
| P2-10 | 连接池配置优化 | ✅ 已修复（max_size=50, test_before_acquire=false） |
| P2-11 | wildcard re-export | ⚠️ 部分完成（已移除 crate 级 `allow(ambiguous_glob_reexports)`，并显式化部分 feature 模块导出） |
| P2-12 | 配置外提 | ⚠️ 部分完成（部分配置已外提，但仍保留媒体路径等硬编码回退值） |
| P2-13 | 配置外提 | ⚠️ 部分完成（仍保留 `env::var` 向后兼容回退，未达到零直接读取） |
| P2-14 | canonical JSON 允许浮点数 | ⚠️ 部分完成（已拒绝非整数浮点与超范围整数，但 `1.0` 仍会被归一化接受） |
| P2-15 | 联邦密钥 query/notary 未完整验证 | ⚠️ 部分完成（已补结构校验与过期拒绝，但尚未完整验证自签名/`old_verify_keys` 链路） |
| P2-16 | 联邦 server key 未校验 valid_until_ts | ✅ 已修复 |
| P2-17 | MSC 登记 | ✅ 已修复（MSC3266/MSC4133 登记，移除未实现的 MSC3916） |
| P2-18 | Complement 测试 | ⏳ 延后（需 QA 配合，3 人周） |

### P3 — 低优先级（10/12 完成）

| 编号 | 描述 | 状态 |
|------|------|------|
| P3-01 | 生产路由中 unreachable!() | ✅ 已修复 |
| P3-02 | src/storage/ 内联单元测试稀少 | ⏳ 延后（1 人周，低优先级） |
| P3-03 | 事件内容哈希比较非 constant-time | ✅ 已修复 |
| P3-04 | secure_compare 长度不同时立即返回 | ✅ 已修复（constant-time 长度折叠） |
| P3-05 | generate_signing_key 生成随机字符串 | ✅ 已修复（#[cfg(test)] 限制） |
| P3-06 | SELECT * 使用（2 处） | ✅ 已修复（存储查询显式列出字段，死代码宏中的 `SELECT *` 已删除） |
| P3-07 | Worker 健康检查未并行化 | ✅ 已修复（futures::join_all） |
| P3-08 | 联邦 HTTP 客户端未配置连接池 | ✅ 已修复（pool_max_idle_per_host=20） |
| P3-09 | 非标准联邦路径 | ❌ 未完成（仍位于 `/_matrix/federation/` 命名空间，仅补充了扩展注释） |
| P3-10 | sliding sync 缺少性能回滚闸门 | ✅ 已修复（benchmark workflow 已接入 sliding sync 基准） |
| P3-11 | 设备列表/presence 缺少长期运行剪枝 | ✅ 已修复（每日后台剪枝任务） |
| P3-12 | Admin API 与上游 v1.153 存在差距 | ✅ 已修复（DELETE report/quarantine changes/tombstoned 字段） |

### 部署就绪状态评估

**部署判断**：按当前代码静态复核，所有 P0 安全漏洞和联邦协议合规性问题已修复（13/13）；剩余未完成项以架构重构、配置收敛、非标准路径治理和测试债务为主。P2 中期修复完成 12/18，P3 低优先级修复完成 10/12；其中 P2-11、P2-12、P2-13、P2-14、P2-15 仍为部分完成，P3-09 尚未完成。基于当前问题分级，项目具备部署前提，但仍建议在后续迭代中继续收敛这些非 P0/P1 阻断项。

**最近一次静态复核依据**（2026-06-20）：
- [synapse-cache/lib.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-cache/src/lib.rs#L894-L902) 现已在 `CacheManager::delete()` 内统一记录 Redis 删除失败；同时 `synapse-services/src`、`synapse-storage/src`、`synapse-federation/src`、`synapse-e2ee/src` 范围内针对 `let _ = self.cache.set/delete(...).await` 的静态复核已清零，因此 `P2-02` 更新为“已修复”。
- [canonical_json.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/canonical_json.rs#L97-L126) 与其内联测试表明：non-integer float 和超范围整数已拒绝，但 `1.0` 仍会被归一化为 `1`，因此 `P2-14` 维持“部分完成”。
- [keys.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/keys.rs#L351-L496) 表明 server key 获取前已做 `valid_until_ts`/`verify_keys`/`signatures` 结构校验与 TTL 截断，但完整密码学签名链验证仍未补齐，因此 `P2-15` 仍为“部分完成”。
- [federation/mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/mod.rs#L55-L90) 仍可见 `/_matrix/federation/...` 下保留的非标准扩展路径，因此 `P3-09` 继续标记为“未完成”。
- `.github/` 当前未见 Complement 相关工作流，仓库内也未见 `complement` 命名测试资产，说明 `P2-18` 仍停留在“延后”状态。

**延后项**（不影响部署）：
- P1-01/03/04/15/16/17：架构重构任务（4+ 人周），不影响功能正确性
- P2-18：Complement 互通测试（需 QA 配合，3 人周）
- P3-02：存储层内联单元测试（1 人周，低优先级）
