# 联邦协议审计报告（Server-Server API）

**审计日期**：2026-09-04
**审计专家**：CodeReviewExpert
**审计范围**：对照 Matrix Server-Server API 规范（v1.6/1.13）审计 synapse-rust 联邦协议实现
**代码体量**：10,676 行

---

## 一、执行摘要

| 维度 | 评分 | 关键发现 |
|------|------|----------|
| 签名验证（PDU / Request） | **A** | ed25519 完整链路、canonical JSON 严格、tamper 检测 OK |
| 服务器密钥分发 | **A-** | 唯一 spec 偏差：未做"valid_until_ts vs 7 days 取小"截断（F-01） |
| 事件幂等性 / 重放保护 | **A** | txn dedup + signature replay cache + ts tolerance 三重防线 |
| SSRF 防护（出站联邦） | **A** | check_url_and_resolve IP 黑名单 + pinned_client_for_url 钉扎 |
| DoS 防护 | **A-** | 100 PDU/100 EDU 上限、token bucket、EDU 限流 |
| 联邦权限控制 | **A** | origin + member-in-room + server_acl 三层校验、OPT-017 防信息泄漏 |
| 房间版本兼容 | **A** | 显式 federatable_room_version 检查 |
| **总体评级** | **A** | **仅 1 个 P0 spec compliance + 4 个 P3 防御强化点** |

**关键结论**：联邦协议实现严格遵守 Matrix Server-Server 规范，未发现可远程利用的高危漏洞；唯一一个 P0 finding 是 spec 明确要求的"valid_until_ts ≤ 7 days"截断缺失（key 长期有效风险窗口），其他为可选强化点。

---

## 二、审计范围与方法

### 2.1 代码清单

| 模块 | 路径 | 行数 | 角色 |
|------|------|------|------|
| Federation 路由入口 | `src/web/routes/federation/mod.rs` | 481 | 路由表、ACL、metrics |
| 事件同步路由 | `src/web/routes/federation/events.rs` | 991 | get_event/state/missing/hierarchy/backfill |
| 事务发送路由 | `src/web/routes/federation/transaction.rs` | 773 | 入口去重 + PDU 验证 + 持久化 |
| 成员变更路由 | `src/web/routes/federation/membership/` | - | make_join/send_join/leave/invite/knock |
| 密钥路由 | `src/web/routes/federation/keys.rs` | 1138 | /_matrix/key/v2/* |
| 媒体路由 | `src/web/routes/federation/media.rs` | 107 | /media/download/thumbnail |
| Federation Auth 中间件 | `src/web/middleware/federation_auth.rs` | 909 | X-Matrix 解析 + 验签 + 重放 + ts |
| Federation 限流中间件 | `src/web/middleware/federation_rate_limit.rs` | - | 50/200 token bucket / per-origin |
| 联邦客户端 | `synapse-federation/src/client.rs` | 1295 | 出站请求签名 + SSRF 钉扎 |
| 签名模块 | `synapse-federation/src/signing.rs` | 749 | ed25519 + canonical + hash + PDU limit |
| 服务器 ACL | `synapse-federation/src/server_acl.rs` | 301 | glob + IP literal + deny-precedence |
| 事件鉴权链 | `synapse-federation/src/event_auth/` | 1348 | auth chain + state resolution |
| 密钥轮换 | `synapse-federation/src/key_rotation.rs` | 1039 | 多密钥并存 + grace period |

### 2.2 入口架构

```
HTTP request
   │
   ▼
federation_auth_middleware (line 22-207)
   ├─ enabled/allow_ingress 短路
   ├─ parse_x_matrix_authorization  (X-Matrix header → origin/key/sig/destination/ts)
   ├─ SecurityValidator::validate_origin
   ├─ is_local_federation_destination (防 destination replay)
   ├─ body size limit (config.federation.max_transaction_payload, default 50KB)
   ├─ canonical_federation_request_bytes(method, uri, origin, dest, content)
   ├─ verify_federation_signature_with_cache
   │   ├─ signature cache（key=origin+key_id+sig+bytes 四元组，仅缓存成功）
   │   ├─ get_federation_verify_key (cache → local → fetch)
   │   └─ verify_federation_signature (ed25519 verify_strict)
   ├─ ts tolerance (default 24h, S1 修复)
   ├─ replay_protection cache (S1 修复)
   ├─ admission_mode check (可选)
   │
   ▼
federation_rate_limit_middleware
   └─ per-origin × endpoint-bucket token bucket (50/200 默认)
   │
   ▼
Handler
   ├─ validate_federation_origin (origin match signed body)
   ├─ room ACL check (validate_federation_origin_*_room)
   └─ 业务处理（PDU 验证、持久化、appservice 派发）
```

---

## 三、签名验证审计

### 3.1 PDU 签名链路（事件伪造防护）

**入口**：`src/web/routes/federation/transaction.rs:705-753` `verify_pdu_sender_signature`

#### 验证链（严格符合 spec §5）

1. **`sender` 字段提取** + 解析 `:server` 后缀
2. **`signatures.<sender_server>` 必填**（line 712-717）
3. **canonical JSON 重算**：去除 `signatures` / `unsigned` 后用 `synapse_common::canonical_json_bytes`
4. **ed25519 verify_strict**（多次循环支持多 key 轮换）
5. **缓存 key 含四元组**（origin+key_id+sig+bytes）—— S5 修复：之前只哈希 bytes，攻击者可以"换签名"绕过缓存

---

## 四、F-01 关键漏洞（🔴 P0）

### 缺失 spec 要求的 7-day valid_until_ts 截断

**位置**：
- `src/web/middleware/federation_auth.rs:407-410`（inbound）
- `synapse-federation/src/client.rs:518-519`（outbound）

**Matrix spec v1.6 §1.2 明确要求**：
> Servers MUST use the lesser of this field and 7 days into the future when determining if a key is valid.
> This is to avoid a situation where an attacker publishes a key which is valid for a significant amount of time without a way for the homeserver owner to revoke it.

**当前实现**：
`effective_cache_ttl_secs` 只取 `valid_until_ts - now`，未与 7 天取小。

**风险场景**：攻击者控制中间 notary，返回 `valid_until_ts = now + 365 days` 的 key → homeserver 缓存 365 天 → 即使 6 个月后合法 owner 撤销 key，攻击者仍能用它伪造签名。

**修复**：
```rust
const MAX_SERVER_KEY_VALIDITY_MS: i64 = 7 * 24 * 60 * 60 * 1000;  // 7 days

fn effective_key_validity_ms(keys: &ServerKeys, now_ms: i64) -> i64 {
    (keys.valid_until_ts.min(now_ms + MAX_SERVER_KEY_VALIDITY_MS) - now_ms).max(0)
}
```

**优先级**：🔴 P0（spec 合规 + 主动威胁场景）
**Ticket 建议**：`#T-FED-KEY-7D`
**工作量**：0.5d

---

## 五、其他发现

### 🟡 F-02 — PDU 数量上限默认 100（spec 推荐 50）

**位置**：`src/web/routes/federation/transaction.rs:180`

```rust
const MAX_PDUS_PER_TRANSACTION: usize = 100;
```

spec §4.1 推荐 50 PDU。`max_transaction_payload` 默认 50KB 已限制总体积，但建议将常量移到 config 并默认改为 50。

### 🟡 F-03 — invite 事件无本端 re-sign 链路

**位置**：`membership/invite.rs:77-122` `invite_v2`

inviter server 创建 invite 事件时，body 直接持久化，未做本端 ed25519 签名。转发出给第三方 origin 时会缺少 `signatures.<local_server_name>.<key_id>`。

### 🟡 F-04 — FederationClient resolve_server 无 inbound IP 黑名单校验

**位置**：`synapse-federation/src/client.rs:350-388`

`resolve_server` 支持 `server_name:8448` IP 字面量格式，但 `resolve_via_well_known` 发出的 HTTP 请求在 `send_signed_request` 层才走 SSRF 防护。IP 字面量路径绕过了 `check_url_and_resolve` 的 IP 黑名单校验。

### 🟡 F-05 — `admission_mode` 未实现

**位置**：`synapse-common/src/config/federation.rs:125`

```rust
pub admission_mode: bool,  // default false
```

注释表明这是"federation ACL 增强模式"，但代码中无实际使用逻辑。建议删除或实现。
---

## 六、事件同步详细审计

### 6.1 send_transaction 全链路

**位置**：`src/web/routes/federation/transaction.rs:16-675`

#### PDU 处理 12 步

| 步骤 | 函数 | 审计结论 |
|------|------|----------|
| 1 dedup | `federation_txn:{origin}:{txn_id}` cache | ✅ 命中返回空结果 |
| 2 size limit | `check_pdu_size_limits` >64KB | ✅ MAX_PDU_SIZE_BYTES (signing.rs:108) |
| 3 content hash | `verify_event_content_hash` | ✅ spec §5 hashes.sha256 必填 |
| 4 sender sig | `verify_pdu_sender_signature` | ✅ 见 §3.1 |
| 5 origin 一致 | `sender_server_name(sender) == auth.origin` | ✅ transaction.rs:698-700 |
| 6 m.federate | 非联邦房间拒绝 | ✅ transaction.rs:264-289 |
| 7 origin in room | `validate_federation_origin_in_room` | ✅ 除 create 外全路径 |
| 8 state write | `verify_state_event_write` | ✅ power level 校验 |
| 9 member transition | `authorize_inbound_member_transition` | ✅ S5 修复 ban re-join 等 |
| 10 gap fill | 从 origin 拉取 prev_events | ✅ 递归签名验证 |
| 11 persist | `create_event` + appservice | ✅ |
| 12 redact | `m.room.redaction` | ✅ redact_event_content |

**关键防御常数**：
- `MAX_PDUS_PER_TRANSACTION = 100`（建议改为 config，默认 50）
- `MAX_PDU_SIZE_BYTES = 65536`（signing.rs:108）
- EDU 每 txn 默认 100 个 + per-origin 默认 2 并发
- dedup TTL 默认 3600s（1 小时）

### 6.2 get_missing_events

**位置**：`src/web/routes/federation/events.rs:49-87`

✅ `validate_federation_origin_can_observe_room` —— 任何非 banned 成员所在房间可观察
✅ `limit.clamp(1, 100)` —— 防止 DoS

### 6.3 get_state / get_state_ids / get_event

✅ 统一权限：`validate_federation_origin_can_observe_room`
✅ `topological_sort` 保证 prev_events 顺序（events.rs:696-751）
✅ 单测：cycle 保持原序 fail-safe（events.rs:848-855）

### 6.4 backfill

**位置**：`events.rs:518-576`

✅ `limit.clamp(1, 100)`（line 771）
✅ `sort_room_events_stably`（depth 倒序 → ts 倒序 → id 升序）
✅ 5 个单测覆盖 None/正常/超出/无效/空 v

### 6.5 timestamp_to_event

**位置**：`events.rs:428-474`

✅ `validate_federation_origin_can_observe_room`
✅ 先校验 room_id 格式再观察 room —— 不会泄漏不存在房间的信息

---

## 七、联邦权限控制详细审计

### 7.1 权限模型（三层防御）

| 层 | 函数 | 校验内容 |
|----|------|----------|
| L1 房间观察权 | `validate_federation_origin_can_observe_room` | origin 有任何非 banned 成员 + room ACL |
| L2 房间参与权 | `validate_federation_origin_in_room` | origin 有 joined 成员（更严） |
| L3 用户共享权 | `validate_federation_origin_shares_user_room` | origin 与 user 在某房间交集 |

✅ **OPT-017 防信息泄漏**：先 ACL 校验再 federatable_room_version（`invite_v2:90-91`、`send_join:123-130`）
✅ **房间 ACL fail-closed**：`check_server_acl`（mod.rs:122-157）—— ACL 解析失败时拒绝

### 7.2 send_join 权限状态机

**位置**：`membership/join.rs:284-312`

```
validate_federation_join_access:
  existing_member.membership == "join"  → 允许
  existing_member.membership == "ban"    → 拒绝（403）
  existing_member.is_banned == true      → 拒绝（403）
  join_rule != "public" && not invite    → 拒绝（403）
  otherwise                             → 允许
```

✅ NULL is_banned 字段 warn + 视为 false（避免误判）
✅ 403 → 404 映射（join.rs:124-128）—— 防信息泄漏

### 7.3 make_join / send_leave / invite

✅ `validate_federation_user_origin` —— sender 服务器匹配认证 origin
✅ `validate_federation_member_event` —— 7 项必填字段 + state_key == sender + membership == expected
✅ room_id / event_id 与 path 一致性

---

## 八、SSRF / DoS 防护详细审计

### 8.1 出站 SSRF 防护

**位置**：`synapse-federation/src/client.rs`

- ✅ `check_url_and_resolve` IP 黑名单（localhost / 10.x / 172.16-31 / 192.168.x / 169.254.x）
- ✅ `pinned_client_for_url` TLS 证书钉扎（well-known 响应钉扎到首次解析的 IP）
- ✅ `resolve_server` 缓存 TTL（默认 5 分钟，DNS 变化可被检测）
- ⚠️ IP 字面量格式（如 `192.168.1.1:8448`）直接使用，不走 IP 黑名单（见 F-04）

### 8.2 入站 DoS 防护

| 机制 | 配置 | 默认值 |
|------|------|--------|
| Transaction body size | `federation.max_transaction_payload` | 50KB |
| PDU 数量 | `MAX_PDUS_PER_TRANSACTION` | 100 |
| EDU 数量 | `federation.inbound_edus_max_per_txn` | 100 |
| EDU 并发 | `federation.inbound_edu_per_origin_max_concurrency` | 2 |
| Rate limit | `federation.rate_limit.per_second` | 50 |
| Rate limit burst | `federation.rate_limit.burst_size` | 200 |
| Join 并发 | `federation.join_max_concurrency` | 10 |

✅ replay_protection（signature cache）：缓存成功验签结果，S5 修复负缓存 DoS
✅ ts tolerance（24h）：默认合理
✅ dedup TTL（1h）：足够覆盖 txn 重试窗口

---

## 九、密钥路由审计

### 9.1 GET /_matrix/key/v2/server

✅ 返回 server signing keys（含 verify_keys / old_verify_keys / signatures）
✅ `valid_until_ts` 字段存在
⚠️ **未做 7-day cap**（见 F-01 P0）

### 9.2 POST /_matrix/key/v2/query

✅ 统一入口路由（keys.rs:36-51）
✅ `QueryRequest` 支持 minimum_valid_until_ts
✅ notary 签名响应缓存 half-lifetime
✅ `parse_and_verify_signed_keys` 签名验证

### 9.3 GET /_matrix/identity/v2/query

✅ 不存在的 endpoint → 404（非 federation 范围，保留 API 兼容）

---

## 十、媒体路由审计

### 10.1 federation media

**位置**：`src/web/routes/federation/media.rs`

✅ `validate_federation_media_server_name` —— 校验 server_name == local_server（防 SSRF）
✅ 缩略图 dimension clamp 1-4096（line 55-63）
✅ `infer::get()` content-type sniffing（防 extension 欺骗）
✅ `Content-Length` header 设置（防响应走私）

---

## 十一、已修复的历史问题（审计确认）

| 问题 | 修复 commit | 审计确认 |
|------|-------------|----------|
| S1 ts tolerance 未校验 | #20250714-S1 | ✅ `verify_request_timestamp` 有 24h tolerance |
| S1 replay protection 未实现 | #20250714-S1 | ✅ `ReplayProtectionCache` 生效 |
| S5 签名缓存负缓存 DoS | #20250820-S5 | ✅ 仅缓存成功，S4-2 单元测试覆盖 |
| S4-2 单元测试矩阵不完整 | #20250820-S4-2 | ✅ 11 个用例覆盖 ed25519 矩阵 |
| OPT-017 房间存在性泄漏 | #20250801-OPT-017 | ✅ 先 ACL 后 room version |

---

## 十二、总结与建议

### 12.1 Finding 汇总

| ID | 优先级 | 类型 | 位置 | 工作量 |
|----|--------|------|------|--------|
| **F-01** | 🔴 P0 | spec 合规 | `federation_auth.rs:407-410` + `client.rs:518-519` | 0.5d |
| F-02 | 🟡 P3 | 配置优化 | `transaction.rs:180` | 0.25d |
| F-03 | 🟡 P3 | 签名完整性 | `membership/invite.rs:96-105` | 0.5d |
| F-04 | 🟡 P3 | SSRF 边界 | `client.rs:384-388` | 0.25d |
| F-05 | 💭 Nit | dead code | `config/federation.rs:125` | 0.1d |

### 12.2 下一步

1. **立即修复 F-01**：在 sprint 内处理，测试用例：
   - `valid_until_ts = now + 365 days` → 实际缓存 ≤ 7 days
   - `valid_until_ts = now + 3 days` → 实际缓存 ≤ 3 days
2. **F-02**：降低到 config，默认 50
3. **F-03**：invite/send_join/send_leave 持久化后调用 `sign_and_hash_event`
4. **F-04**：在 `resolve_server` 返回 IP 字面量前增加 IP 黑名单检查
5. **F-05**：确认 `admission_mode` 无后续实现计划后删除 dead field
