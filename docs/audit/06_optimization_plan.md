# synapse-rust 审核修复实施计划（Optimization Plan）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 7 份审核报告（`docs/audit/01..07`）里的全部「必须修复(P0/P1/HIGH)」与「建议修复(P2/MEDIUM/LOW)」拆成可原子提交、可独立 `cargo test` 验证的任务。

**Architecture:** 遵守三层架构（web → services → storage，真实逻辑在 workspace crates），TDD Red-Green-Refactor，字段命名规范（毫秒时间戳、禁用字段名清单），insta 快照冗余字段 redact。安全关键项（OIDC/联邦密钥/E2EE）配套 spec 向量或语义回归测试；DB 迁移配套 rule 六.3 回滚脚本。

**Tech Stack:** Rust 2024 / Axum / sqlx(PostgreSQL) / criterion / insta / nextest；crates: synapse-common, synapse-storage, synapse-e2ee, synapse-federation, synapse-services。

## Global Constraints

- 时间戳统一毫秒 `chrono::Utc::now().timestamp_millis()`（BIGINT）；JWT exp/iat 例外为秒（规则五.3）。
- 禁用字段名：`invalidated/invalidated_ts/created_at/updated_at/expires_ts/revoked_ts/enabled`（规则二.3）。
- 三层边界：service 不建裸 SQL，全委托 storage；handler 不取 `State<AppState>`，用 domain context extractor（规则七/八）。
- 错误统一 `ApiError`，禁 `anyhow`/`Box<dyn Error>`（规则五.4）。
- 每个任务：`SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings` 必须零告警；关联测试必须先 Red 后 Green。
- DB 迁移：正向脚本用 `IF NOT EXISTS` 幂等，配套 `六.3` 回滚脚本，放 `migrations/<UTC时间戳>_<name>.sql` + `.down.sql`。
- E2EE/canonical JSON/事件签名任务：必须附 spec 向量或语义回归测试（Matrix v1.18 / sytest）。
- 排序原则：P0 安全 → 独立原子 → 不阻塞他人。命名 `OPT-NNN`。

> 说明：本文件按用户要求存 `docs/audit/06_optimization_plan.md`（与同前缀的 `06_e2ee_federation_review.md` 共存，后缀不同）。

---

## 任务索引（按修复优先级）

| ID | 标题 | 审核发现 | 风险 | 原子 |
|----|------|---------|------|------|
| OPT-001 | OIDC 签名绕过：删除 claim-only 回退 | 07 #1 CRITICAL | 高 | ✅ |
| OPT-002 | 联邦签名私钥日志脱敏 | 07 #4 HIGH | 低 | ✅ |
| OPT-003 | healthcheck 只信 /health | 07 #3 HIGH | 低 | ✅ |
| OPT-004 | 远程 server key 缓存按 valid_until_ts 收缩 TTL | 06 §9 P1 | 中 | ✅ |
| OPT-005 | 敏感端点限流紧规则 | 05 §7 / 07 #5 | 低 | ✅ |
| OPT-006 | megolm 成员离开触发轮换（接线） | 06 §2 P1 | 中 | ✅ |
| OPT-007 | 密钥备份恢复校验版本新鲜度 | 06 §4 P1 | 中 | ✅ |
| OPT-008 | 联邦密钥默认要求 master key | 07 #6 MEDIUM | 中 | ✅ |
| OPT-009 | account_data 秒→毫秒时间戳 | 04 §4 P2 | 低 | ✅ |
| OPT-010 | to_device 去重原子化（消 TOCTOU） | 06 §5 P2 | 中 | ✅ |
| OPT-011 | device 写操作包事务 | 03 §4 P1 | 中 | ✅ |
| OPT-012 | delete_devices_batch 消 N+1 | 03 §1 P1 | 中 | ✅ |
| OPT-013 | NULLABLE 列裸 i64 → Option<i64> | 03 §7 P1 | 中 | ✅ 19字段 013a–013r + lib-test 修复 |
| OPT-014 | 后台任务统一 CancellationToken 停机 | 04 §7 P1 | 中 | ✅ 014-0/a/b/c/e/d |
| OPT-015 | SyncService 注入缓存热读 | 04 §5 P1 | 中 | ✅ 015-0/a/b/c/d |
| OPT-016 | XFF 可信代理校验（限流防绕过） | 07 #2 HIGH | 中 | ✅ |
| OPT-017 | 联邦存在性 404/403 统一 | 06 §10 P1 | 中 | ✅ 逐端点 |
| OPT-018 | Box<dyn Error> → ApiError | 04 §3 P2 | 低 | ✅ |
| OPT-019 | Ed25519SecretKey ZeroizeOnDrop | 07 #12 LOW | 低 | ✅ |
| OPT-020 | url_preview expires_ts → expires_at（迁移） | 03 §6 P2 | 中 | ✅ +回滚 |
| OPT-021 | OIDC nonce 校验 | 07 #7 MEDIUM | 中 | ✅ |
| OPT-022 | SAML 强制至少一方签名 | 07 #8 MEDIUM | 中 | ✅ |
| OPT-023 | 认证提取器统一（手动 token→AuthenticatedUser） | 05 §1 P2 | 低 | ✅ 逐 handler |
| OPT-024 | 审计日志防篡改（限制 delete） | 07 #11 LOW | 低 | ✅ |

---

## Tier 0 — P0 安全（先做，独立原子）

### OPT-001: OIDC id_token 签名绕过 —— 删除 claim-only 回退

**审核发现:** 07 #1 CRITICAL（A07）。**风险:** 高（认证绕过/账号接管，OIDC 启用时）。

**Files:**
- Modify: `/Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/oidc_service.rs:400-418`
- Test: `/Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/oidc_service.rs`（同文件 `#[cfg(test)]`）

**Interfaces:**
- Consumes: `validate_id_token(&self, id_token)` 内部 JWKS 分支。
- Produces: JWKS 无匹配 kid / fetch 失败时返回 `Err`，不再降级 claim-only。

- [ ] **Step 1: 写失败测试（Red）** —— 追加到 `oidc_service.rs` 的 `#[cfg(test)] mod tests`：

```rust
#[tokio::test]
async fn unknown_kid_must_not_fall_back_to_claim_only() {
    // 构造一个 issuer/aud/exp 均合法但 kid 不在 JWKS 的 id_token（未签名段随意）。
    let svc = test_oidc_service_with_empty_jwks();
    let forged = forge_unsigned_id_token(&svc.config.issuer, &svc.config.client_id);
    let res = svc.validate_id_token(&forged).await;
    assert!(res.is_err(), "unknown kid must be rejected, not claim-only accepted");
}
```

- [ ] **Step 2: 运行看失败** —— `cargo test -p synapse-services unknown_kid_must_not_fall_back -- --nocapture`；Expected: FAIL（当前回退 claim-only 返回 Ok）。

- [ ] **Step 3: 最小实现（Green）** —— 改 `:400-418`：

Before:
```rust
                } else {
                    tracing::warn!(kid = ?kid, issuer = %self.config.issuer, client_id = %self.config.client_id,
                        "No matching JWKS key found, falling back to claim-only validation");
                    self.validate_id_token_claims(id_token)?;
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, issuer = %self.config.issuer, client_id = %self.config.client_id,
                    "Failed to fetch JWKS, falling back to claim-only validation");
                self.validate_id_token_claims(id_token)?;
            }
```

After:
```rust
                } else {
                    tracing::error!(kid = ?kid, issuer = %self.config.issuer, client_id = %self.config.client_id,
                        "No matching JWKS key for id_token kid; rejecting (no claim-only fallback)");
                    return Err("id_token signature key (kid) not found in JWKS".to_string());
                }
            }
            Err(e) => {
                tracing::error!(error = %e, issuer = %self.config.issuer, client_id = %self.config.client_id,
                    "JWKS fetch failed; rejecting id_token (no claim-only fallback)");
                return Err(format!("JWKS unavailable, cannot verify id_token signature: {e}"));
            }
```

- [ ] **Step 4: 运行看通过** —— `cargo test -p synapse-services -- --nocapture`；Expected: PASS。`SQLX_OFFLINE=true cargo clippy -p synapse-services --all-features --locked -- -D warnings` 零告警。

- [ ] **Step 5: 提交** ——
```bash
git add synapse-services/src/oidc_service.rs
git commit -m "fix(oidc): reject id_token on JWKS miss/fetch-fail instead of claim-only fallback (OPT-001, audit 07#1)"
```

---

### OPT-002: 联邦签名私钥日志脱敏

**审核发现:** 07 #4 HIGH（A09/A02）。**风险:** 低（改动一行，但消除私钥泄露）。

**Files:**
- Modify: `/Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/keys.rs:229`

- [ ] **Step 1: 写失败测试（Red）** —— 若该函数可单元测，断言日志不含 key；否则用代码审查断言（grep gate）。最小可执行断言：新增测试确认脱敏 helper：

```rust
#[test]
fn signing_key_never_logged_verbatim() {
    let src = include_str!("keys.rs");
    assert!(!src.contains("from signing_key: {}\", k"),
        "raw signing key must not be interpolated into logs");
}
```

- [ ] **Step 2: 运行看失败** —— `cargo test -p synapse-rust signing_key_never_logged -- --exact`；Expected: FAIL。

- [ ] **Step 3: 最小实现（Green）** —— 改 `:229`：

Before:
```rust
            ::tracing::error!("Failed to derive verify key from signing_key: {}", k);
```

After:
```rust
            ::tracing::error!("Failed to derive verify key from configured signing_key ([REDACTED], {} chars)", k.len());
```

- [ ] **Step 4: 运行看通过** —— `cargo test -p synapse-rust signing_key_never_logged -- --exact`；Expected: PASS。clippy 零告警。

- [ ] **Step 5: 提交** ——
```bash
git add src/web/routes/federation/keys.rs
git commit -m "fix(federation): redact signing_key in derive-failure log (OPT-002, audit 07#4)"
```

---

### OPT-003: Docker healthcheck 只信 /health（不回退 /versions）

**审核发现:** 07 #3 HIGH（A05，硬约束违反）。**风险:** 低。

**Files:**
- Modify: `/Users/ljf/Desktop/hu_ts/synapse-rust/src/bin/healthcheck.rs:22`
- Modify(可选): `/Users/ljf/Desktop/hu_ts/synapse-rust/docker/docker-compose.yml:54`

- [ ] **Step 1: 写失败测试（Red）** —— 在 `healthcheck.rs` 加 `#[cfg(test)]` 断言探测路径集不含 /versions：

```rust
#[test]
fn health_probe_must_not_trust_versions() {
    // 把 paths 提成 const HEALTH_PATHS 以便测试。
    assert!(!HEALTH_PATHS.contains(&"/_matrix/client/versions"),
        "/versions returns 200 without DB; must not be a health signal");
}
```

- [ ] **Step 2: 运行看失败** —— `cargo test --bin healthcheck health_probe_must_not_trust_versions`；Expected: FAIL。

- [ ] **Step 3: 最小实现（Green）** —— 改 `:22`：

Before:
```rust
        let paths = ["/health", "/_matrix/client/versions", "/_matrix/federation/v1/version"];
```

After:
```rust
        const HEALTH_PATHS: [&str; 1] = ["/health"];
        let paths = HEALTH_PATHS;
```

（`/health` 已在 DB 宕时返 503，是唯一可信健康信号。TCP fallback 保留。）

- [ ] **Step 4: 运行看通过** —— `cargo test --bin healthcheck`；Expected: PASS。clippy 零告警。

- [ ] **Step 5: 提交** ——
```bash
git add src/bin/healthcheck.rs
git commit -m "fix(healthcheck): trust only /health, drop /versions fallback that masks DB outage (OPT-003, audit 07#3)"
```

---

## Tier 1 — P1 / HIGH（安全含义或合规，独立原子优先）

### OPT-004: 远程 server key 缓存按 valid_until_ts 收缩 TTL

**审核发现:** 06 §9 P1（可能用已撤销密钥验签）。**风险:** 中（联邦验签正确性）。**Spec:** Matrix server-server key `valid_until_ts`。

**Files:**
- Modify: `/Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/client.rs:402-421`

**Interfaces:**
- Consumes: `ServerKeys.valid_until_ts: i64`（毫秒）、`KEY_CACHE_TTL_SECS`。
- Produces: 缓存过期取 `min(KEY_CACHE_TTL_SECS, (valid_until_ts-now)/1000)`。

- [ ] **Step 1: 写失败测试（Red，spec 向量）** —— 追加：

```rust
#[tokio::test]
async fn cache_respects_valid_until_ts_shorter_than_default() {
    // 远端密钥 600s 后到期，默认 TTL 3600s。缓存应在 ~600s 后失效，而非 3600s。
    let now_ms = chrono::Utc::now().timestamp_millis();
    let keys = ServerKeys { valid_until_ts: now_ms + 600_000, ..sample_keys() };
    let effective = effective_cache_ttl_secs(&keys, now_ms); // 待抽出的纯函数
    assert!(effective <= 600 && effective >= 590, "TTL must shrink to valid_until_ts window, got {effective}");
}
```

- [ ] **Step 2: 运行看失败** —— `cargo test -p synapse-federation cache_respects_valid_until_ts`；Expected: FAIL（函数不存在）。

- [ ] **Step 3: 最小实现（Green）** —— 抽纯函数 + 改缓存判定：

```rust
fn effective_cache_ttl_secs(keys: &ServerKeys, now_ms: i64) -> u64 {
    let remaining = ((keys.valid_until_ts - now_ms) / 1000).max(0) as u64;
    KEY_CACHE_TTL_SECS.min(remaining)
}
```

改 `:405-409`：
```rust
            if let Some(cached) = cache.get(destination) {
                let now_ms = chrono::Utc::now().timestamp_millis();
                let ttl = effective_cache_ttl_secs(&cached.keys, cached.cached_at_ms.max(now_ms - i64::MAX/2) as _); // 见下
                if cached.cached_at.elapsed().as_secs() < ttl {
                    return Ok(cached.keys.clone());
                }
            }
```
（若 `CachedKeys` 只有 `cached_at: Instant`，用 `effective_cache_ttl_secs(&cached.keys, now_ms)` 计算上限，再与 `elapsed()` 比较即可，无需 `cached_at_ms`。）

- [ ] **Step 4: 运行看通过** —— `cargo test -p synapse-federation`；Expected: PASS。clippy 零告警。

- [ ] **Step 5: 提交** ——
```bash
git add synapse-federation/src/client.rs
git commit -m "fix(federation): shrink server-key cache TTL to valid_until_ts window (OPT-004, audit 06 §9)"
```

---

### OPT-005: 敏感端点限流紧规则

**审核发现:** 05 §7 / 07 #5。**风险:** 低（配置默认值）。

**Files:**
- Modify: `/Users/ljf/Desktop/hu_ts/synapse-rust/docker/config/rate_limit.yaml`

- [ ] **Step 1: 写失败测试（Red）** —— 若有配置加载测试则加断言；否则 YAML 解析断言：

```rust
#[test]
fn sensitive_endpoints_have_tight_limits() {
    let cfg: RateLimitConfigFile = serde_yaml::from_str(include_str!("../../docker/config/rate_limit.yaml")).unwrap();
    for p in ["/_matrix/client/v3/register", "/_matrix/client/v3/account/password"] {
        let rule = cfg.rule_for(p).expect("must have explicit rule");
        assert!(rule.per_second <= 5, "{p} must be tightly limited");
    }
}
```

- [ ] **Step 2: 运行看失败** —— `cargo test -p synapse-rust sensitive_endpoints_have_tight_limits`；Expected: FAIL（落默认 50/s）。

- [ ] **Step 3: 最小实现（Green）** —— 在 `endpoints:` 下追加（参照 login 5/s）：

```yaml
  - path: "/_matrix/client/v3/register"
    match_type: "prefix"
    rule: { per_second: 1, burst_size: 3 }
  - path: "/_matrix/client/v3/account/password"
    match_type: "prefix"
    rule: { per_second: 1, burst_size: 3 }
  - path: "/_matrix/client/v3/register/email/requestToken"
    match_type: "prefix"
    rule: { per_second: 1, burst_size: 3 }
  - path: "/_matrix/client/v3/refresh"
    match_type: "prefix"
    rule: { per_second: 2, burst_size: 5 }
  - path: "/_matrix/client/v3/keys/upload"
    match_type: "prefix"
    rule: { per_second: 5, burst_size: 10 }
```

- [ ] **Step 4: 运行看通过** —— `cargo test -p synapse-rust sensitive_endpoints_have_tight_limits`；Expected: PASS。

- [ ] **Step 5: 提交** ——
```bash
git add docker/config/rate_limit.yaml
git commit -m "fix(ratelimit): tight rules for register/password/refresh/keys-upload/3pid (OPT-005, audit 05 §7 / 07#5)"
```

---

### OPT-006: megolm 成员离开触发轮换（接线零调用方函数）

**审核发现:** 06 §2 P1（前向保密）。**风险:** 中。**Spec:** E2EE outbound megolm 轮换（服务端辅助）。

**Files:**
- Modify: 调用方（成员离开事件处理处，`synapse-services/.../room` 成员变更路径）
- Reference: `/Users/ljf/Desktop/hu_ts/synapse-rust/synapse-e2ee/src/key_rotation/service.rs:221` `notify_member_left_encrypted_room`

**Interfaces:**
- Consumes: `KeyRotationService::notify_member_left_encrypted_room(room_id, user_id)`。
- Produces: 成员 leave（且房加密）时写 `key_rotation_pending`。

- [ ] **Step 1: 定位调用点** —— `rg -n "membership.*leave|handle_leave|on_member_left" synapse-services/src | head`；确认成员离开事件落地函数（记其绝对路径:行）。

- [ ] **Step 2: 写失败测试（Red，语义回归）** ——

```rust
#[tokio::test]
async fn member_leave_encrypted_room_marks_rotation_pending() {
    let (svc, rot) = room_service_with_spy_key_rotation();
    svc.process_membership_change("!enc:localhost", "@bob:localhost", "leave").await.unwrap();
    assert_eq!(rot.pending_calls().await, vec![("!enc:localhost".into(), "@bob:localhost".into())]);
}
```

- [ ] **Step 3: 运行看失败** —— `cargo test -p synapse-services member_leave_encrypted_room_marks_rotation_pending`；Expected: FAIL（零调用方）。

- [ ] **Step 4: 最小实现（Green）** —— 在成员离开处理内、房间为加密时调用：

```rust
if membership == "leave" && self.is_room_encrypted(room_id).await? {
    self.key_rotation.notify_member_left_encrypted_room(room_id, &user_id).await?;
}
```

- [ ] **Step 5: 运行看通过 + 提交** ——
```bash
cargo test -p synapse-services -- --nocapture
git add synapse-services/
git commit -m "fix(e2ee): trigger megolm rotation on member leave of encrypted room (OPT-006, audit 06 §2)"
```

---

### OPT-007: 密钥备份恢复校验版本新鲜度

**审核发现:** 06 §4 P1（旧备份回滚）。**风险:** 中。**Spec:** E2EE key backup version。

**Files:**
- Modify: `/Users/ljf/Desktop/hu_ts/synapse-rust/synapse-e2ee/src/backup/service.rs:446-456`

**Interfaces:**
- Consumes: `storage.get_current_backup_version(user_id) -> Option<String>`（若无则需新增该 storage 方法）。
- Produces: `recover_keys` 对非最新 version 返回 `ApiError::bad_request`。

- [ ] **Step 1: 写失败测试（Red）** ——

```rust
#[tokio::test]
async fn recover_rejects_stale_version() {
    let svc = backup_service_with_versions(&["1", "2"]); // current = "2"
    let err = svc.recover_keys("@a:localhost", "1", None).await.unwrap_err();
    assert_eq!(err.errcode(), "M_INVALID_PARAM");
}
```

- [ ] **Step 2: 运行看失败** —— `cargo test -p synapse-e2ee recover_rejects_stale_version`；Expected: FAIL。

- [ ] **Step 3: 最小实现（Green）** —— 在 `:456` 之后插入：

```rust
        let current = self
            .storage
            .get_current_backup_version(user_id)
            .await?
            .ok_or_else(|| ApiError::not_found("No backup version".to_string()))?;
        if current != version {
            return Err(ApiError::bad_request(format!(
                "Refusing to recover non-current backup version {version} (current: {current})"
            )));
        }
```

- [ ] **Step 4: 运行看通过 + 提交** ——
```bash
cargo test -p synapse-e2ee -- --nocapture
git add synapse-e2ee/src/backup/service.rs
git commit -m "fix(e2ee): reject key-backup recovery of stale version (OPT-007, audit 06 §4)"
```

---

### OPT-008: 联邦密钥默认要求 master key（拒绝明文入库）

**审核发现:** 07 #6 MEDIUM（A02）。**风险:** 中。

**Files:**
- Modify: `/Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/key_rotation.rs:115-119`

- [ ] **Step 1: 读现状** —— Read `key_rotation.rs:108-160`，确认 `signing_key_master_key` 缺失时的 warn+明文分支。
- [ ] **Step 2: 写失败测试（Red）** ——
```rust
#[tokio::test]
async fn refuses_plaintext_persistence_without_master_key() {
    let svc = key_rotation_without_master_key();
    let err = svc.persist_rotated_key(sample_key()).await.unwrap_err();
    assert!(err.to_string().contains("master key"));
}
```
- [ ] **Step 3: 运行看失败** —— `cargo test -p synapse-federation refuses_plaintext_persistence`；Expected: FAIL。
- [ ] **Step 4: 最小实现（Green）** —— 把 warn+明文改为：master key 缺失时返回 `Err`（除非显式 `allow_plaintext_signing_keys: true` opt-in）。
- [ ] **Step 5: 提交** ——
```bash
git add synapse-federation/src/key_rotation.rs
git commit -m "fix(federation): require master key for signing-key persistence, refuse plaintext by default (OPT-008, audit 07#6)"
```

---

### OPT-009: account_data 秒→毫秒时间戳

**审核发现:** 04 §4 P2（真 bug，跨端排序错位）。**风险:** 低。

**Files:**
- Modify: `/Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/account_data_service.rs:100`

- [ ] **Step 1: 写失败测试（Red）** ——
```rust
#[tokio::test]
async fn room_account_data_uses_millis() {
    let (svc, spy) = account_data_service_with_spy_storage();
    let before = chrono::Utc::now().timestamp_millis();
    svc.set_room_account_data("@a:localhost", "!r:localhost", "m.tag", &serde_json::json!({})).await.unwrap();
    let ts = spy.last_room_ts().await;
    assert!(ts >= before, "expected millis timestamp, got {ts}");
}
```
- [ ] **Step 2: 运行看失败** —— `cargo test -p synapse-services room_account_data_uses_millis`；Expected: FAIL（秒级 < 毫秒基准）。
- [ ] **Step 3: 最小实现（Green）** ——

Before: `let now = chrono::Utc::now().timestamp();`
After: `let now = chrono::Utc::now().timestamp_millis();`

- [ ] **Step 4: 运行看通过 + 提交** ——
```bash
cargo test -p synapse-services room_account_data_uses_millis
git add synapse-services/src/account_data_service.rs
git commit -m "fix(account-data): use millis timestamp for room account data (OPT-009, audit 04 §4)"
```

---

### OPT-010: to_device 去重原子化（消 TOCTOU）

**审核发现:** 06 §5 P2（重复投递竞态）。**风险:** 中。

**Files:**
- Modify: `/Users/ljf/Desktop/hu_ts/synapse-rust/synapse-e2ee/src/to_device/service.rs:33-42`
- Modify: 对应 storage `record_transaction` 返回是否首次插入（`ON CONFLICT DO NOTHING ... RETURNING`）

**Interfaces:**
- Produces: `record_transaction(...) -> Result<bool, ApiError>`（true=首次）。

- [ ] **Step 1: 写失败测试（Red）** —— 并发两次同 message_id 只投递一次：
```rust
#[tokio::test]
async fn duplicate_message_id_delivered_once_under_race() {
    let svc = to_device_service_inmem();
    let f1 = svc.send("@a", "DEV", "m.type", Some("mid1"), &two_device_payload());
    let f2 = svc.send("@a", "DEV", "m.type", Some("mid1"), &two_device_payload());
    let _ = tokio::join!(f1, f2);
    assert_eq!(svc.delivered_count("@b", "DEVB").await, 1);
}
```
- [ ] **Step 2: 运行看失败** —— `cargo test -p synapse-e2ee duplicate_message_id_delivered_once`；Expected: FAIL。
- [ ] **Step 3: 最小实现（Green）** —— 用 `record_transaction` 的插入结果做闸门，删掉先 SELECT：

Before:
```rust
        if let Some(mid) = message_id {
            if self.storage.is_duplicate_transaction(sender_user_id, sender_device_id, mid).await? {
                return Ok(());
            }
            self.storage.record_transaction(sender_user_id, sender_device_id, mid).await?;
            let _ = self.storage.cleanup_old_transactions(TRANSACTION_MAX_AGE_MS).await;
        }
```
After:
```rust
        if let Some(mid) = message_id {
            let is_first = self.storage.record_transaction(sender_user_id, sender_device_id, mid).await?;
            if !is_first {
                tracing::debug!("Duplicate to-device transaction {} from {}:{}", mid, sender_user_id, sender_device_id);
                return Ok(());
            }
            let _ = self.storage.cleanup_old_transactions(TRANSACTION_MAX_AGE_MS).await;
        }
```
storage 侧：`INSERT ... ON CONFLICT (sender, device, message_id) DO NOTHING RETURNING 1`，`Ok(row.is_some())`。

- [ ] **Step 4: 运行看通过 + 提交** ——
```bash
cargo test -p synapse-e2ee -- --nocapture
git add synapse-e2ee/ synapse-storage/
git commit -m "fix(to-device): atomic dedup via INSERT..RETURNING, remove TOCTOU (OPT-010, audit 06 §5)"
```

---

### OPT-011: device 写操作包事务（create/delete）

**审核发现:** 03 §4 P1（设备已建但变更流丢，E2EE 对端不同步）。**风险:** 中。

**Files:**
- Modify: `/Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/device/mod.rs:378-404`（create_device）、`:570-588`（delete_user_device）

- [ ] **Step 1: 读现状** —— Read `device/mod.rs:370-410` 与 `:560-595`，记录三步写的确切 SQL 与参数。
- [ ] **Step 2: 写失败测试（Red）** —— 注入中途失败的假 pool，断言首步不落库（原子性）：
```rust
#[tokio::test]
async fn create_device_is_atomic_on_midway_failure() {
    let store = device_store_failing_on_second_write();
    let _ = store.create_device("@a:localhost", "DEV", "disp").await;
    assert!(store.get_device("@a:localhost", "DEV").await.unwrap().is_none(),
        "partial device row must be rolled back");
}
```
- [ ] **Step 3: 运行看失败** —— `cargo test -p synapse-storage create_device_is_atomic`；Expected: FAIL。
- [ ] **Step 4: 最小实现（Green）** —— 三步写包 `let mut tx = self.pool.begin().await?; ... tx.commit().await?;`，全部走 `&mut *tx`。delete_user_device 同法。
- [ ] **Step 5: 提交** ——
```bash
cargo test -p synapse-storage -- --nocapture
git add synapse-storage/src/device/mod.rs
git commit -m "fix(storage): wrap create_device/delete_user_device multi-write in a transaction (OPT-011, audit 03 §4)"
```

---

### OPT-012: delete_devices_batch 消 N+1

**审核发现:** 03 §1 P1（删 100 台=300 往返；batch 方法已存在未用）。**风险:** 中。

**Files:**
- Modify: `/Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/device/mod.rs:643-646`
- Reuse: `delete_lazy_loaded_members_for_devices_batch`(:339)、`record_device_list_changes_batch`(:192)

- [ ] **Step 1: 写失败测试（Red，计数往返）** —— 用可计数假 executor 断言往返 = O(1) 而非 O(n)：
```rust
#[tokio::test]
async fn delete_devices_batch_is_constant_roundtrips() {
    let (store, counter) = device_store_counting_queries();
    store.delete_devices_batch("@a:localhost", &vec!["D1".into(),"D2".into(),"D3".into()]).await.unwrap();
    assert!(counter.roundtrips() <= 3, "batch must not loop per device, got {}", counter.roundtrips());
}
```
- [ ] **Step 2: 运行看失败** —— `cargo test -p synapse-storage delete_devices_batch_is_constant`；Expected: FAIL（9 往返）。
- [ ] **Step 3: 最小实现（Green）** —— 循环内只收集 id，循环外用已有 batch 方法一次 `= ANY($1)`，全程单事务。
- [ ] **Step 4: 提交** ——
```bash
cargo test -p synapse-storage -- --nocapture
git add synapse-storage/src/device/mod.rs
git commit -m "perf(storage): use batch deletes in delete_devices_batch, remove N+1 (OPT-012, audit 03 §1)"
```

---

### OPT-013: NULLABLE 列裸 i64 → Option<i64>（逐 struct 原子）

**审核发现:** 03 §7 P1（sqlx `UnexpectedNullError` 运行时崩溃）。**风险:** 中。**做法:** 每个 struct 一个提交，先 per-struct 确认表列可空性。

**Files（逐个）:**
- `synapse-storage/src/oidc_session_storage.rs:32,61`（expires_at）
- `synapse-storage/src/matrixrtc.rs:15,31`（updated_ts）
- `synapse-storage/src/feature_flags.rs:22,44`（updated_ts）
- `synapse-storage/src/federation_blacklist.rs:37,71,85`（updated_ts）
- `synapse-storage/src/threepid.rs:43`（expires_at）
- `synapse-storage/src/qr_login.rs:146`、`rendezvous.rs:18`（expires_at）
- `synapse-storage/src/{moderation.rs:16,dehydrated_device.rs:15,thread.rs:49/61/93/109,sticky_event.rs:189,privacy.rs:16}`（updated_ts）

**每个 struct 的模板（以 feature_flags 为例）:**
- [ ] **Step 1: 确认列可空性** —— `docker exec docker-postgres psql -U synapse -d synapse -tc "SELECT column_name,is_nullable FROM information_schema.columns WHERE table_name='feature_flags' AND column_name='updated_ts';"`；Expected: `YES`。
- [ ] **Step 2: 写失败测试（Red）** —— 查询一条 `updated_ts IS NULL` 的行，断言不 panic：
```rust
#[tokio::test]
async fn feature_flag_reads_null_updated_ts() {
    let store = seed_feature_flag_with_null_updated_ts().await;
    let ff = store.get_flag("test").await.unwrap();
    assert!(ff.updated_ts.is_none());
}
```
- [ ] **Step 3: 运行看失败** —— `cargo test -p synapse-storage feature_flag_reads_null_updated_ts`；Expected: FAIL（`UnexpectedNullError`）。
- [ ] **Step 4: 最小实现（Green）** —— struct 字段 `updated_ts: i64` → `updated_ts: Option<i64>`，`query_as` 与下游读取相应改 `Option`。
- [ ] **Step 5: 提交** —— `git commit -m "fix(storage): feature_flags.updated_ts nullable -> Option<i64> (OPT-013a, audit 03 §7)"`。
> 其余 struct 重复本模板，命名 OPT-013b/c/...，每 struct 独立提交。

---

## Tier 2 — P1 结构性工作流（拆多子任务；每子任务独立可测）

### OPT-014: 后台任务统一 CancellationToken 停机

**审核发现:** 04 §7 P1（5 个裸 loop 无停机，违反硬约束）。**风险:** 中。**注:** 非 2-5 min 单任务，拆为 1 框架 + 5 接线子任务。

- **OPT-014-0（框架）:** 在 `container` 层创建 `tokio_util::sync::CancellationToken`，注入各后台服务；SIGTERM → `token.cancel()` 后 join。测试：`shutdown_signal_cancels_token`。
- **OPT-014-a..e（逐 loop 接线，各独立提交）:** 参照已合规 `worker/health.rs:219` 的 `select!` 模式，给以下每个 loop 加 `_ = token.cancelled() => break`：
  - `synapse-services/src/event_notifier.rs:152`
  - `synapse-services/src/burn_after_read_service.rs:301`
  - `synapse-services/src/room/service.rs:184`
  - `synapse-services/src/application_service/scheduler.rs:404`
  - `src/worker/tcp.rs:39`
- **每子任务 TDD:** Red = 启动 loop，cancel token，断言 join 在超时内返回；Green = 加 select! 分支；`cargo test -p <crate> <name>`。
- **附带清理:** 删死代码 `admin_registration_service.rs:61 start_nonce_cleanup_task`（无调用方）或接线之。

---

### OPT-015: SyncService 注入缓存热读

**审核发现:** 04 §5 P1（/sync 每次 4 项未缓存直打 PG）。**风险:** 中。拆子任务，每项一个提交。

- **OPT-015-0:** 给 `SyncService` 加 `cache: Arc<dyn CacheApi>` 字段（`sync_service/mod.rs:45`），container 注入 `SharedInfra.cache`。
- **OPT-015-a:** `filter.rs:24 get_filter` → key `filter:{user}:{id}` 长 TTL；set 时失效。
- **OPT-015-b:** `data_fetch.rs:223 list_account_data` → key `account_data:{user}`；写 account data 时失效（与 OPT-009 同文件协同）。
- **OPT-015-c:** `data_fetch.rs:308 get_max_device_list_stream_id` → 全局短 TTL(5s)。
- **OPT-015-d:** `sliding_sync_service/state.rs:18 get_state_events(room)` → key `room_state:{room}`；state 变更失效。
- **每子任务 TDD:** Red = 命中两次只查 storage 一次（计数假 storage）；Green = 包一层 cache get/set；`cargo test -p synapse-services <name>`。缓存失效路径必须配套测试，避免读到陈旧数据。

---

### OPT-016: XFF 可信代理校验（限流防绕过）

**审核发现:** 07 #2 HIGH（限流全绕过）。**风险:** 中。拆子任务。

- **OPT-016-0:** `RateLimitConfig` 增 `trusted_proxies: Vec<IpNet>` 与 `trust_forwarded: bool`（默认 false）。
- **OPT-016-a:** 改 `src/web/utils/ip.rs:extract_client_ip`：仅当 `ConnectInfo` peer 属 `trusted_proxies` 才解析 XFF，且取**最右可信跳**；否则用 peer_addr。签名改为接收 peer_addr。
- **OPT-016-b:** 改调用点 `rate_limit.rs:43` 传入 `ConnectInfo` peer。
- **TDD:** Red = 不可信 peer + 伪造 XFF → bucket key 应为 peer 而非 XFF；Green = 实现可信链校验。`cargo test -p synapse-rust xff_not_trusted_uses_peer`。

---

### OPT-017: 联邦存在性 404/403 统一（逐端点原子）

**审核发现:** 06 §10 P1（泄露私有房间/用户存在性）。**风险:** 中。范式：对齐 `events.rs:57 validate_federation_origin_can_observe_room` 统一 404。

**Files（逐端点）:** `membership.rs:435/464`（send_join）、`:399/518`（make/send_leave）、`:569/282`（invite）、`events.rs:115/124/127`（get_event）、`events.rs:198/366/460`（directory/hierarchy）。

- **每端点 TDD:** Red = 私有存在房 vs 不存在房，断言两者返回同一状态码（不可区分）；Green = 把访问控制前置到 `federatable_room_version` 之前，或统一错误码；`cargo test --test integration federation_no_existence_leak_<endpoint>`。make_join 定级轻微，可留。

---

## Tier 3 — P2 / 建议修复（低风险，独立原子）

### OPT-018: Box<dyn Error> → ApiError

**审核发现:** 04 §3 P2。**风险:** 低。
**Files:** `synapse-services/src/auth/trait.rs:75`、`auth/mod.rs:204`、`auth/account.rs:264`（`generate_email_verification_token`）。
- [ ] Red: 断言返回类型编译为 `Result<String, ApiError>`（改签名后重编）。
- [ ] Green: trait + 2 实现签名改 `Result<String, ApiError>`，失败走 `ApiError::internal_with_log`。
- [ ] `cargo build -p synapse-services` + clippy 零告警；`git commit -m "refactor(auth): generate_email_verification_token returns ApiError (OPT-018, audit 04 §3)"`。

### OPT-019: Ed25519SecretKey ZeroizeOnDrop

**审核发现:** 07 #12 LOW。**风险:** 低。
**Files:** `/Users/ljf/Desktop/hu_ts/synapse-rust/synapse-e2ee/src/crypto/ed25519.rs:49`。
- [ ] Red: `#[test] fn secret_key_zeroizes_on_drop`（drop 后底层 buffer 归零，用裸指针读或 `ZeroizeOnDrop` trait 存在性断言）。
- [ ] Green: Before `#[derive(Debug, Zeroize)]` → After `#[derive(Debug, Zeroize, ZeroizeOnDrop)]`（引入 `zeroize::ZeroizeOnDrop`）。
- [ ] `cargo test -p synapse-e2ee`；`git commit -m "fix(e2ee): ZeroizeOnDrop for Ed25519SecretKey (OPT-019, audit 07#12)"`。

### OPT-020: url_preview `expires_ts` → `expires_at`（DB 迁移，含回滚）

**审核发现:** 03 §6 P2（禁用字段名）。**风险:** 中。**规则六.3 回滚必附。**
**Files:** `synapse-storage/src/url_preview_storage.rs:17/43/45/60/73/96`；新增迁移。

- [ ] **Step 1: 正向迁移** —— `migrations/20260710120000_rename_url_preview_expires.sql`：
```sql
DO $$ BEGIN
  IF EXISTS (SELECT 1 FROM information_schema.columns
             WHERE table_name='url_preview_cache' AND column_name='expires_ts') THEN
    ALTER TABLE url_preview_cache RENAME COLUMN expires_ts TO expires_at;
  END IF;
END $$;
ALTER INDEX IF EXISTS idx_url_preview_cache_expires RENAME TO idx_url_preview_cache_expires_at;
```
- [ ] **Step 2: 回滚脚本（六.3）** —— `migrations/20260710120000_rename_url_preview_expires.down.sql`：
```sql
ALTER INDEX IF EXISTS idx_url_preview_cache_expires_at RENAME TO idx_url_preview_cache_expires;
DO $$ BEGIN
  IF EXISTS (SELECT 1 FROM information_schema.columns
             WHERE table_name='url_preview_cache' AND column_name='expires_at') THEN
    ALTER TABLE url_preview_cache RENAME COLUMN expires_at TO expires_ts;
  END IF;
END $$;
```
- [ ] **Step 3:** 同步 struct/SQL 字段名 `expires_ts`→`expires_at`；`bash docker/db_migrate.sh validate`。
- [ ] **Step 4:** `cargo test -p synapse-storage url_preview`；提交（迁移 + 代码同一提交）。

### OPT-021: OIDC nonce 校验

**审核发现:** 07 #7 MEDIUM。**风险:** 中。
**Files:** `src/web/routes/oidc.rs:500-593` + `synapse-services/src/oidc_service.rs`（`validate_id_token` 增 nonce 比对参数）。
- [ ] Red: id_token nonce ≠ 存储 nonce → 拒绝。Green: 交换路径取回存储 nonce，与 id_token `nonce` claim 比对。`cargo test -p synapse-rust oidc_nonce_mismatch_rejected`。

### OPT-022: SAML 强制至少一方签名

**审核发现:** 07 #8 MEDIUM。**风险:** 中。
**Files:** `synapse-services/src/saml_service.rs:562-576`。
- [ ] Red: `want_response_signed=false && want_assertions_signed=false` 且断言未签 → 必须拒绝。Green: `validate_response` 无条件要求响应或断言至少一方有有效签名。`cargo test -p synapse-services saml_requires_signature`。

### OPT-023: 认证提取器统一（逐 handler）

**审核发现:** 05 §1 P2。**风险:** 低。
**Files:** `src/web/routes/handlers/sync.rs:55`、`handlers/room/members.rs:22`、`directory_reporting.rs:56`。
- [x] 每 handler：把手动 `bearer_token+validate_token` 改为 `AuthenticatedUser` 提取器参数。Red = 无 token 返 401 快照不变；Green = 换提取器。`cargo test --test integration <handler>_requires_auth`。快照若涉及用 insta 并 redact 动态字段。 — 完成 dc769ddb（sync.rs×2, members.rs×6, directory_reporting.rs×3）

### OPT-024: 审计日志防篡改（限制 delete）

**审核发现:** 07 #11 LOW。**风险:** 低。
**Files:** `synapse-storage/src/audit.rs:197-209 delete_events_before`。
- [ ] Red: 断言 `delete_events_before` 仅在保留策略路径可调（加调用方 gate 或 feature）。Green: 收窄可见性/加显式 retention-only 守卫；或加 append-only 触发器迁移（附六.3 回滚）。`cargo test -p synapse-storage audit_delete_guarded`。

---

## Self-Review

**1. Spec coverage:** 07 CRITICAL→OPT-001；07 HIGH×3→OPT-002/003/016；06 P1×4→OPT-004/006/007/017；06 P2→OPT-010；04 P1×2→OPT-014/015；04 P2×2→OPT-009/018；03 P1×3→OPT-011/012/013；03 P2（字段名）→OPT-020；05 P2×2→OPT-005/023；07 MEDIUM/LOW→OPT-008/019/021/022/024。GDPR/导出/CAPTCHA/JWT 轮换（07 #9/#10/#13/#14）为大特性，列入 backlog（见下），非本轮原子修复。

**2. Placeholder scan:** 已读源码的任务（001/002/003/004/009/010/019/020）给出精确 before/after；未读周边代码的任务（006/008/011/012/013/016/017 等）首步显式要求 `读现状/定位` 再改，非占位。

**3. Type consistency:** `notify_member_left_encrypted_room(room_id, user_id)`、`record_transaction(...)->Result<bool>`、`effective_cache_ttl_secs(&ServerKeys,i64)->u64`、`get_current_backup_version(user_id)->Option<String>` 在引用处一致。

**Backlog（超出 2-5min 原子、需独立 spec）:** 07 #9 GDPR erase 级联、#10 CAPTCHA provider 接入、#13 JWT kid 轮换、#14 用户数据导出、04 §6 SyncReadHelpers 下沉、06 §2 megolm 若需真正 outbound 轮换编排。建议各起 `/spec`。

---

## 执行方式

计划已存 `docs/audit/06_optimization_plan.md`。建议：**先做 Tier 0（OPT-001/002/003）**——三项独立、低风险、消除 CRITICAL+2 HIGH；再按表顺序推进。E2EE/联邦任务（004/006/007/010）执行时补 spec 向量；迁移任务（020）执行时验证 `.down.sql` 可回滚。
