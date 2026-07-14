# AUDIT-2026-07 Fail-Open 系统性扫描 (Round 2)

> **扫描日期**: 2026-07-13
> **分支**: `feat/architecture-optimization-round2`
> **扫描范围**: auth / federation / health / rate-limit / e2ee / membership 安全关键路径
> **规则**: 禁止 `unwrap_or(default)` / `.ok()` 吞错 / `is_ok() || return` 在安全关键路径上 fail-open
> **原则**: fail-closed — 安全决策不确定时默认拒绝

---

## 扫描结果汇总

| 风险等级 | 数量 | 说明 |
|---------|------|------|
| P0 (Critical) | 0 | 未发现立即可导致授权绕过的漏洞 |
| P1 (High) | 4 | 隐私泄露风险、认证降级风险 |
| P2 (Medium) | 6 | 防御深度不足、错误上下文丢失、运维配置风险 |

---

## P1 — 高危

| 文件:行 | 当前语义 | 风险等级 | 建议修复 |
|---------|---------|---------|---------|
| `src/web/routes/account_compat.rs:35` | `results.get(user_id).copied().unwrap_or(true)` — 若用户不在批量查询结果中，默认 profile 可见 | P1 | 改为 `unwrap_or(false)` 使缺失数据默认隐藏。上游 `can_view_profile_for_requester_batch` 返回完整 HashMap，缺失条目表明 bug，此时应 deny |
| `src/web/routes/account_compat.rs:52` | `bearer_token(headers).ok()` — Token 提取失败静默吞错 | P1 | 此处在 token 缺失时返回 `None` 是正确的（匿名用户），但建议至少打 warn 日志以区分「无 token」和「header 解析异常」 |
| `src/web/routes/account_compat.rs:54` | `auth_service.validate_token(&t).await.ok().map(...)` — Token 校验错误（DB 故障、签名错误）被静默吞掉，退化为匿名用户 | P1 | 区分错误类型：token 过期/无效 → None（匿名降级正确）；DB 错误 → 应 propagate error 而非静默吞掉。建议 `match` 替代 `.ok()` |
| `src/web/routes/handlers/search/search.rs:322,388` | `visibility.get(&target_user_id).copied().unwrap_or(true)` — 若用户在可见性批量查询结果中缺失，默认视为可见（包含在搜索结果中） | P1 | 与 account_compat.rs:35 同根。改为 `unwrap_or(false)` 使缺失数据默认隐藏。上下游均调用 `can_view_profile_for_requester_batch`，应统一 fail-close 语义 |

### P1 根因分析

`unwrap_or(true)` 用于隐私可见性检查是一种「乐观默认」——假设数据缺失意味着用户没有设置隐私限制。正确的安全姿态应是「悲观默认」——数据缺失意味着无法确认可见性，应拒绝展示。

`account_compat.rs:52,54` 的 `.ok()` 吞错是一种「静默降级」——将 token 校验失败（可能是 DB 故障）与「用户未登录」等同视之。DB 故障时应 propagate error 而非降级为匿名。

---

## P2 — 中危

| 文件:行 | 当前语义 | 风险等级 | 建议修复 |
|---------|---------|---------|---------|
| `src/web/routes/federation/transaction.rs:330` | `.and_then(\|s\| s.parse::<Membership>().ok())` — 未知 membership 值解析失败时，整个授权块被跳过（`if let Some(to) = ...` 不匹配） | P2 | 未知 membership 虽会进入 auth-event chain 校验，但应在此处显式 reject 而非静默跳过。建议：解析失败时写入 error result 并 `continue` |
| `src/web/routes/federation/membership/join.rs:283` | `member.is_banned.unwrap_or(false)` — 若 `is_banned` 数据库字段为 NULL，视为未封禁 | P2 | 此检查是 `member.membership == "ban"` 的补充防御。若字段确实可能为 NULL，`unwrap_or(false)` 是安全的（降低一级防御但不绕过主检查）。建议增加 tracing warn 当 `is_banned` 为 None 但 membership 非 ban 时 |
| `src/web/middleware/federation_auth.rs:368,397` | `get_current_key().await.ok().flatten()?` — 密钥获取 DB 错误被吞，仅返回 None 导致签名校验失败 | P2 | 行为上是 fail-closed（DB 故障 → 签名校验失败 → 拒绝请求），但丢失了错误上下文。建议改为 `match` 并在 DB 错误时打 error 日志 |
| `src/web/routes/widget.rs:376` | `verify_room_moderator(...).await.is_ok()` — 错误时 `is_ok()` 返回 false，用户不被视为管理员 | P2 | 行为上是 fail-closed（错误 → 不授权），但与 `is_creator.unwrap_or(false)` (L382) 组合使用时，若两个检查同因 DB 故障失败，真实管理员会被拒绝（可用性问题，非安全漏洞）。建议在错误时打 warn 日志 |
| `src/web/routes/widget.rs:382` | `is_room_creator(...).await.unwrap_or(false)` — DB 错误时默认不是创建者 | P2 | 同样是 fail-closed。但 `.unwrap_or(false)` 丢掉了错误上下文。建议 match 并在错误时打日志 |
| `src/server/database.rs:50,53` + `src/server/mod.rs:125` | 环境变量默认 `unwrap_or(false)` | P2 | 已验证：`SYNAPSE_ENABLE_RUNTIME_DB_INIT`、`SYNAPSE_SKIP_DB_INIT`、`TRUST_FORWARDED_HEADERS` 均用 `false` 作为默认值（fail-closed）。非漏洞，但硬编码的 `false` default 对运维不透明。建议在启动日志中显式打印这些 flag 的实际值 |

---

## 联邦入站 membership 授权复核

### 复核结论: ✅ 所有 membership 变体均 fail-closed

**授权链路**:
```
transaction.rs:326-354
  → MembershipService::authorize_inbound_member_transition (service.rs:300)
    → resolve_membership_from (查询当前 membership + is_banned)
    → TransitionCtx::state_only (跳过 power-level, 仅检查状态机)
    → is_legal (synapse-common/src/membership_transition.rs:167)
```

**各变体覆盖**:

| 变体 | 检查位置 | 结果 |
|------|---------|------|
| Ban → Join | `check_join` L185 | ✅ 拒绝 (Banned) |
| Ban → Invite | `check_invite` L208 | ✅ 拒绝 (TargetBanned) |
| Ban → Knock | `check_knock` L275 | ✅ 拒绝 (Banned) |
| Self-ban | `check_ban` L252-253 | ✅ 拒绝 (InvalidTransition, actor_is_target) |
| Join → Invite (重复邀请) | `check_invite` L209 | ✅ 拒绝 (InvalidTransition) |
| Knock → Join (非 knock 房间) | `check_knock` L271-272 | ✅ 拒绝 (InvalidTransition) |
| None → Join (非公开房间) | `check_join` L187-196 | ✅ 拒绝 (NotInvited) |
| None → Ban (无前状态) | `check_ban` (via is_legal) | ✅ power-level 检查会拦截 |
| Creator kick/ban | `check_leave` L237 + `check_ban` L255 | ✅ 拒绝 (TargetIsCreator) |
| Leave → Leave (idempotent) | `is_legal` L171-174 | ✅ 允许 (no-op) |

**设计说明**: `authorize_inbound_member_transition` 使用 `TransitionCtx::state_only` 仅检查状态机合法性，将 power-level 检查委托给 auth-event chain（标准 Synapse 事件授权流程）。两者组合提供完整覆盖。

---

## Rate-Limit fail-open 配置审计

| 位置 | 默认值 | 风险 |
|------|-------|------|
| `synapse-common/src/config/rate_limit.rs:56` — 全局 `default_rate_limit_fail_open()` | `false` | ✅ fail-closed |
| `synapse-common/src/rate_limit_config.rs:96` — 文件配置 `fail_open_on_error` | `#[serde(default)]` = `false` | ✅ fail-closed |
| `synapse-common/src/rate_limit_config.rs:170` — Default impl | `false` | ✅ fail-closed |
| `src/web/middleware/rate_limit.rs:82-97` — Redis 不可用/限流错误时的行为 | 由配置控制 | 若运维强制开启 `fail_open_on_error: true`，Redis 故障时限流被绕过。建议在日志中 warn 当此 flag 为 true |

---

## 未发现 P0 漏洞的说明

本轮扫描未发现 P0 级漏洞（即攻击者可通过构造请求直接绕过授权）。原因：

1. **Membership 状态机** (`synapse-common/src/membership_transition.rs`) 是经过充分测试的纯函数，覆盖了所有 5×5=25 种状态转换，包括 ban→join、invite-of-banned、self-ban 等关键拒绝路径。
2. **Federation 入站** 使用 `state_only` + auth-event chain 双层防御，power-level 检查和状态机检查各司其职。
3. **Rate limit** 默认 fail-closed (`false`)，全局和路由级均一致。
4. **TRUST_FORWARDED_HEADERS** 默认 `false`（不信任代理头），即使解析失败也 default false。
5. **Runtime DB init** 默认禁用，必须显式开启 + 不跳过，双重 gate。

P1 发现主要集中在**隐私可见性**模块 (`account_compat.rs` + `search.rs`)，其 `unwrap_or(true)` 模式在边缘情况（批量查询返回不完整数据）下可能导致隐藏 profile 泄露。

---

## 建议修复优先级

1. **立即修复** (P1): `account_compat.rs:35` + `search/search.rs:322,388` — 将 `unwrap_or(true)` 改为 `unwrap_or(false)`，3 行改动
2. **短期修复** (P1): `account_compat.rs:54` — 区分 DB 错误和认证失败，错误时 propagate 而非降级
3. **后续加固** (P2): 为所有 `.ok()` 吞错点增加 error/warn 日志，提升可观测性
4. **运维建议**: 在启动日志中打印 `fail_open_on_error` 的实际值，当其为 `true` 时输出 warn
