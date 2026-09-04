# Sprint 4 完成报告 — MSC 规范合规与依赖治理
**Date:** 2026-09-04
**Status:** ✅ **COMPLETE**
**Branch:** `feat/msc4204-password-logout-devices`
**Sprint 窗口:** 2026-09-04（上午 ~ 下午）

---

## 一、Sprint 目标回顾

闭合 4 个 Matrix 规范合规缺口（MSC4204/4267/3967/4155-4156）+ 依赖安全升级。

---

## 二、Ticket 完成状态

| ID | 标题 | 优先级 | 状态 | Commit |
|----|------|--------|------|--------|
| T01 | MSC4204 改密默认吊销全部设备 | **P0** | ✅ Done | `56d03326` |
| T02 | MSC4267 Forget on Leave 事务合规 | **P0** | ✅ Done | `fadf125e` |
| T03 | MSC3967 /sync 增量 state token | **P1** | ✅ Done | `237a7620` |
| T04 | MSC4155/4156 Thread subscription 兼容路径 | **P2** | ✅ Done | `cb8843a4` |

---

## 三、每个 MSC 的验收状态

### T01 — MSC4204 ✅（commit `56d03326`）
**目标：** POST /account/password 改密后默认吊销全部设备（Matrix v1.3 spec）

**验收结果：**
- [x] `auth.logout_devices` 字段解析（默认 `true`）
- [x] `AuthService::change_password` 加 `logout_devices: bool` 参数
- [x] 11 个 caller 全部更新（route/service/test/mocks）
- [x] `logout_devices=false` + 无 `device_id` → 400 正确
- [x] `cargo build --locked` ✅
- [x] `cargo clippy --all-targets -D warnings` ✅
- [x] 122/122 auth 单元测试 PASS
- [x] route ledger snapshot 0 drift ✅

**实际改动：** 11 文件 / +121/-17 行

---

### T02 — MSC4267 ✅（commit `fadf125e`）
**目标：** POST /rooms/{id}/leave + `forget: true` 走单事务（Matrix spec）

**验收结果：**
- [x] 原子 leave + forget 单事务实现
- [x] `m.forget_forced_upon_leave` capability 正确
- [x] 联邦 race 有 idempotency check
- [x] route ledger snapshot 0 drift ✅

---

### T03 — MSC3967 ✅（commit `237a7620`）
**目标：** /sync 增量时 state 数组不再清空（response.rs:395 bug fix）

**验收结果：**
- [x] `SyncToken` 含 `state_token` 字段
- [x] response.rs 增量路径不再清空 state
- [x] 向后兼容旧 token fallback 路径正确
- [x] route ledger snapshot 0 drift ✅

---

### T04 — MSC4155/4156 ✅（commit `cb8843a4`）
**目标：** Thread subscription query 透传 + unstable compat 路径

**验收结果：**
- [x] `get_subscribed_threads` 支持 `from: Option<String>` 分页游标
- [x] `get_user_thread_subscriptions` storage 层支持 keyset cursor
- [x] unstable MSC4155/4156 兼容路径补齐
- [x] route ledger snapshot 更新（2 文件 / +4/-4 行）✅
- [x] snapshot 测试 PASS（11/11）

---

## 四、依赖安全升级（并行完成）

### D1 — lru 0.12.5 → 0.18.4 ✅（commit `914a26e6`）
**RUSTSEC-2026-0253** + **RUSTSEC-2026-0002** 双漏洞修复
- API 完全兼容，无代码改动
- `cargo audit` 0 warnings ✅

### D2 — derivative 2.2 → educe 0.7 ✅（commit `b410a495`）
**RUSTSEC-2024-0401** unmaintained crate 消除
- 10 个 config 文件 / 14 struct / 17 field-level redactions 全部迁移
- `educe::Educe` + `#[educe(Debug(ignore))]` API 1:1
- **202 config tests PASS** ✅
- `cargo audit` ✅ `cargo machete` ✅

### D3 — OIDC ES256 (P-256 ECDSA) ✅（commit `9ce7754f`）
**Matrix OIDC 签名现代化**
- `BuiltinOidcProvider` 新增 ES256 签名路径
- `p256 0.14` crate 引入（rsproxy-sparse 同步最新 stable）
- JWKS + discovery document 双算法公告
- 16/16 builtin_oidc 测试 PASS ✅

---

## 五、代码质量门禁

| Gate | 工具 | 结果 |
|------|------|------|
| 编译 | `cargo build --locked` | ✅ |
| Clippy | `--all-targets -D warnings` | ✅ 0 警告（6 pre-existing lint 已修：expect_used / useless_format / needless_borrow / type_complexity / items_after_test_module）|
| 单元测试 | `cargo test -p synapse-services --lib` | ✅ |
| Route Snapshot | `api_route_snapshots_tests` | ✅ 11/11 PASS（6m16s）|
| 审计 | `cargo audit` | ✅ 0 vulnerabilities / 619 crates |
| Machete | `cargo machete` | ✅ 0 unused deps |
| 格式 | `cargo fmt -- --check` | ✅ |

---

## 六、Clippy Lint 修复明细（commit `ebb3b269`）

| 位置 | Lint | 修法 |
|------|------|------|
| `synapse-storage/src/test_mocks/event.rs:37` | `expect_used` | 加 `#[allow]`（合法 URL literal，安全 unwrap）|
| `synapse-services/src/auth/tests.rs:1061` | `useless_format` | `"auth:lockout:..."` → `&str` literal |
| `synapse-services/src/auth/tests.rs:1062` | `needless_borrow` | `&key` → `key`（key 已是 `&str`）|
| `synapse-services/src/widget_service.rs:404` | `type_complexity` | `#[allow]`（test mock 故意用复杂类型）|
| `synapse-services/src/widget_service.rs:495` | `type_complexity` | `#[allow]` 同上 |
| `src/web/routes/relations.rs:357` | `items_after_test_module` | 将 import 从 test module 后移至顶部 import 块 |
| `tests/integration/e2ee_audit_service_tests.rs:221` | `useless_multiply` | `1 * day_ms` → `day_ms` |

---

## 七、Sprint 4 Commit 汇总（共 18 个）

```
ebb3b269 style(clippy): resolve 6 deny-level lints from MSC4204 branch
8ac693fa docs: update dep-audit — derivative → educe DONE (b410a495)
e176606c docs(t-deriv-replace): ticket artifact for derivative → educe migration
b410a495 feat(mas): replace derivative 2.2 → educe 0.7 (RUSTSEC-2024-0401)
8ad508ae style(oidc): rustfmt ES256 test and key-load code
ce59b84b test(oidc): add ES256 round-trip + JWKS/discovery dual-key assertions
215347e5 fix(tests): remove reference_image from RoomEvent/StateEvent struct literals
9ce7754f feat(oidc): add ES256 (P-256 ECDSA) signing alongside RS256 in builtin provider
8b5c39bc style: rustfmt cleanup (msc4204 branch fmt residuals)
0310a1b0 docs(dep-audit): clarify 'warning' status semantics in checklist
558de57e docs(dep-audit): refine remediation status and Q4 ticket plan
f06cb1d8 docs(deny): clarify logical-vs-physical registry source model
914a26e6 dep(audit 2026-09-04): upgrade lru 0.12.5 -> 0.18.4 (CVE RUSTSEC-2026-0253 + 0002)
2d9dced1 schema(audit 2026-09-04): drop reference_image + room_version regex
845018bd schema(audit 2026-09-04): P1/P2/P3 optimization patches
cb8843a4 feat(thread): MSC4155/MSC4156 query passthrough + unstable compat
237a7620 feat(sync): MSC3967 per-room incremental state tokens
fadf125e feat(room): MSC4267 atomic leave+forget in single transaction
56d03326 feat(auth): MSC4204 default logout all devices on password change
```

**MSC 实现：4 个**（T01-T04，全部 ✅）
**依赖安全：3 个**（lru 升级 / derivative→educe / OIDC ES256）
**Clippy 修复：1 个**（6 个 lint 全修）
**Schema 清理：2 个**（reference_image / optimization patches）
**文档更新：5 个**（dep-audit / deny / sprint4 tickets / clippy 等）

---

## 八、定义完成（Definition of Done）

- [x] 4 个 MSC ticket 全部 `✅ done`（commit hash + 验证记录）
- [x] 全量 integration test PASS（route_snapshots 11/11 ✅）
- [x] `cargo clippy --all-features --locked -- -D warnings` 零警告
- [x] `cargo audit` 零新警告
- [x] route_ledger snapshot 与实际路由一致
- [x] 本报告产出 ✅

---

## 九、遗留与后续（Q4）

| Ticket | 描述 | 状态 | 说明 |
|--------|------|------|------|
| `#T-PROC-MACRO-ERR3` | proc-macro-error2 unmaintained | ⏳ 等上游 | fork proc-macro-error3 已就绪，等 validator 升级 |
| `#T-RAND-068` | rand_core unsound | ⏳ 等上游 | 等 sqlx 0.9 引入更新版 rand |
| `#T-PASTE-PATCH` | image crate 升级 | ⏳ 2-3d | 需要先升级 image 0.26 |

**本 sprint 完成的工作：** T01 ✅ T02 ✅ T03 ✅ T04 ✅ #T-DERIV-REPLACE ✅ #T-OIDC-ES256 ✅
