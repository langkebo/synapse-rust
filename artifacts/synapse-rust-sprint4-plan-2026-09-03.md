# Sprint 4 实施规划 — MSC 规范合规与 /sync 性能优化

**Date:** 2026-09-03
**Status:** ready-for-review
**Sprint 窗口:** 2026-09-04 ~ 2026-09-18（2 周，10 个工作日）
**Owner:** CodeReviewExpert

---

## 一、Sprint 4 目标

闭合 2 个 P0 规范合规缺口（MSC4204、MSC4267）+ 1 个 P1 性能优化（MSC3967）+ 1 个 P2 兼容性补全（MSC4155/4156）。Sprint 4 结束时，synapse-rust 在 Matrix 规范合规度上对齐 element-hq/synapse v1.156+。

**总工作量:** 7.5 人天（约 1.5 周单人 / 1 周双人并行）

---

## 二、Ticket 依赖图

```
[T01 MSC4204]  1d   ─┐
                     ├─ Week 1 (P0 并行)
[T02 MSC4267]  2-3d ─┘
                     │
                     ├── [T03 MSC3967]  3d   ── Week 2 (P1, 依赖 T02 完成后启动以免 schema 冲突)
                     │
[T04 MSC4155/4156] 1.5d ── Week 1 后半 / Week 3 早期 (P2, 独立)
```

**关键依赖:**
- T01 / T02 互不依赖，可并行（两人各 1d + 2-3d）
- T03 依赖 T02 完成（避免 schema migration 冲突期）
- T04 独立，可任意时点插入

**Week 3 buffer:** 留 5 天 buffer（5d）用于 code review、回归测试、CI 修复、突发集成问题。

---

## 三、详细 ticket 列表

| ID | 标题 | 优先级 | 工作量 | 依赖 | 路径 |
|----|------|--------|--------|------|------|
| **T01** | MSC4204 改密默认吊销全部设备 | **P0** | **1.0d** | 无 | `.scratch/sprint4-2026-09-03/issues/01-msc4204-password-logout-devices.md` |
| **T02** | MSC4267 Forget on Leave（事务内 leave+forget） | **P0** | **2-3d** | 无 | `.scratch/sprint4-2026-09-03/issues/02-msc4267-forget-on-leave.md` |
| **T03** | MSC3967 /sync 增量 state token | **P1** | **3.0d** | T02 schema | `.scratch/sprint4-2026-09-03/issues/03-msc3967-incremental-state-tokens.md` |
| **T04** | MSC4155/4156 thread 兼容路径 + query 透传 | **P2** | **1.5d** | 无 | `.scratch/sprint4-2026-09-03/issues/04-msc4155-msc4156-thread-compat.md` |

**总工作量:** 7.5 ~ 8.5 人天（视 T02 范围 2d vs 3d 而定）

---

## 四、推荐排期（单人视角）

### Week 1（2026-09-04 ~ 09-08，5 个工作日）

| Day | 工作 | 产出 |
|-----|------|------|
| **D1（周一）** | T01 MSC4204 完整实现 + 单测 | commit `feat(auth): MSC4204 ...` |
| **D1（周一）下午** | T02 MSC4204 集成测试补齐 | test pass |
| **D2-D3（周二-周三）** | T02 MSC4267 PoC + service/storage 实现 | commit `feat(room): MSC4267 ...` |
| **D4（周四）** | T02 MSC4267 集成测试 + capability 翻转 | capability `m.forget_forced_upon_leave: true` |
| **D5（周五）** | Sprint 1 收尾: T01+T02 集成回归 + route ledger snapshot 刷新 | 1396 tests PASS, 2 个新 commit |

**Week 1 验收:**
- [ ] POST /account/password 默认吊销全部（MSC4204 spec 合规）
- [ ] POST /rooms/{id}/leave + `forget: true` 走单事务清理
- [ ] 2 个新 commit 通过 `cargo clippy --all-features --locked -- -D warnings`
- [ ] 集成测试全 PASS（1396 + 新增 6-8 个）

### Week 2（2026-09-09 ~ 09-11，3 个工作日 + buffer）

| Day | 工作 | 产出 |
|-----|------|------|
| **D6（周一）** | T03 MSC3967 PoC（storage 索引验证 + benchmark baseline） | bench report |
| **D7（周二）** | T03 storage `get_state_events_changed_since` 实现 | storage commit |
| **D8（周三）** | T03 SyncToken + response.rs 改造 + feature flag 接入 | service commit |
| **D9-D10（周四-周五）** | T03 集成测试 + feature flag 灰度验证 + Sprint 2 收尾 | 灰度报告 |

**Week 2 验收:**
- [ ] /sync 增量 state delta 正常返回（feature flag 灰度）
- [ ] Payload 减少 ≥ 30%（benchmark 验证）
- [ ] 旧 since token 不破坏（fallback 全量 state）

### Week 3（2026-09-12 ~ 09-18，buffer + 收尾）

| Day | 工作 | 产出 |
|-----|------|------|
| **D11-D12** | T04 MSC4155/4156 兼容路径 + query 透传 | commit `feat(thread): ...` |
| **D13** | T04 集成测试 + E2E 验证 | 2 unstable 路由 + 2 个新 e2e |
| **D14-D15** | 全 sprint 回归: clippy + 1396 tests + route ledger snapshot 刷新 | 1400+ tests PASS |
| **D16-D17（buffer）** | Code review 修复、CI 修复、文档更新 | artifacts/sprint4-2026-09-18.md 总结 |

---

## 五、风险登记表

| 风险 | 概率 | 影响 | Mitigation |
|------|------|------|------------|
| **T01 行为反转引发客户端兼容性投诉** | 中 | 中 | 默认 `true` 是 v1.3 spec，旧客户端用 `logout_devices: false` 兼容 |
| **T02 联邦 leave 事件 race（远程 server 重建 membership）** | 高 | 中 | event_idempotency check + `synapse-common` task queue saga |
| **T02 大房间 forget 事务超时** | 中 | 中 | 批量 delete + 超时保护，参考 Sprint 3 `multi-DELETE 事务`模式 |
| **T03 旧 since token 解析失败** | 中 | 高 | 向后兼容: 不带 state_token 字段的旧 token 走全量 state fallback |
| **T03 storage 索引缺失** | 中 | 中 | PoC 阶段（半天）先验证，必要时 migration 加索引 |
| **T04 route ledger snapshot drift** | 低 | 低 | 沿用 Sprint 3 教训: `UPDATE_ROUTE_LEDGER_SNAPSHOTS=1` + 同 feature 集 |
| **CI 全量回归 51 分钟** | 高 | 中 | 受影响子集先跑 + 全量 nightly |

---

## 六、测试策略

### 单元测试（必做）
- T01: `synapse-services/src/auth/tests.rs` 新增 `change_password_logout_devices` 系列用例
- T02: `synapse-services/src/room_service.rs` 新增 `leave_with_forget` 单元测试
- T03: `synapse-services/src/sync_service/tests.rs` 新增 `state_token_round_trip` 用例
- T04: `synapse-services/src/thread_service.rs` 新增 `get_subscribed_threads_pagination`

### 集成测试（必做，feature flag 全开）
- T01: `tests/integration/auth_service_coverage_tests.rs` 新增 3 个用例
- T02: `tests/integration/room_membership_tests.rs` 新增 4 个用例
- T03: `tests/integration/sync_service_tests_migrated.rs` 新增 4 个用例
- T04: `tests/integration/thread_service_tests.rs` 新增 2 个用例

### E2E 测试（必做）
- T01: `tests/e2e/e2e_scenarios.rs:test_user_password_change_flow` 增强
- T02: 新增 `test_leave_with_forget` E2E
- T03: 新增 `test_incremental_sync_state_delta` E2E
- T04: 新增 2 个 thread 兼容路径 E2E

### 回归测试（sprint 结束）
- 全量 integration test（baseline 51 分钟）: 目标 1400+ tests PASS
- route ledger snapshot 刷新（沿用 `UPDATE_ROUTE_LEDGER_SNAPSHOTS=1`）
- `cargo clippy --all-features --locked -- -D warnings` 零警告
- `cargo audit` + `cargo machete` 通过

---

## 七、CI / 工程基线

每个 ticket 完成时立即跑（**禁止批量 commit 前跳过验证**）：
```bash
# 1. 编译
cargo build --locked

# 2. 静态检查
cargo clippy --features "test-utils privacy-ext voice-extended voip-tracking beacons server-notifications" --locked -- -D warnings

# 3. 受影响 integration test 子集
cargo test --features "test-utils ..." --test integration <affected_test_file>

# 4. 提交
git add -A && git commit -m "<conventional commit>"
```

Sprint 结束验收（必须全过）：
```bash
# 全量 integration test
SQLX_OFFLINE=true cargo test --features "test-utils privacy-ext voice-extended voip-tracking beacons server-notifications" --test integration

# 依赖治理
cargo audit
cargo machete
```

---

## 八、文档产出

| 文件 | 内容 | 时点 |
|------|------|------|
| `artifacts/synapse-rust-sprint4-impl-2026-09-18.md` | Sprint 4 实施完成总结 + 4 个 commit hash + 性能对比 | Sprint 结束 D17 |
| 更新 `artifacts/synapse-rust-code-review-2026-09-03.md` | 把 Sprint 4 候选章节标"已实施"或更新预估 | Sprint 结束 D17 |
| 4 个 ticket 文件标 `✅ done` | 实现完成后填 commit hash + 测试记录 | 每个 ticket 完成后 |

---

## 九、Sprint 4 完成定义（Definition of Done）

- [ ] 4 个 ticket 全部 `✅ done`（commit hash + 验证记录）
- [ ] 全量 integration test PASS（≥ 1400 个）
- [ ] `cargo clippy --all-features --locked -- -D warnings` 零警告
- [ ] `cargo audit` 零新警告
- [ ] route_ledger snapshot 与实际路由一致
- [ ] `artifacts/synapse-rust-sprint4-impl-2026-09-18.md` 产出
- [ ] Sprint 5 候选（性能基准 + federation interop）建议列表

---

## 附：与 Sprint 3 的连贯性

| 维度 | Sprint 3 收尾状态 | Sprint 4 切入点 |
|------|------------------|----------------|
| 路由层架构 | route_ledger snapshot 一致 | 沿用，新增路由刷新 snapshot（commit 模板参考 `f42aadaf`） |
| 测试基线 | 1396 PASS | 1400+ PASS（新增 11-15 个用例） |
| DB 迁移 | schema v11 | 视 T02/T03 需求新增 migration（沿用 Sprint 3 idempotent 模式） |
| 工程纪律 | fmt + clippy + audit + machete | 沿用，每个 ticket 完成立即跑 |
| 文档模板 | `artifacts/synapse-rust-code-review-2026-09-03.md` 8 个章节 | 沿用 + 新增 Sprint 4 实施报告 |

---

_本规划基于 Sprint 3 审计文档的"Sprint 4 候选"章节、MSC 子代理差距分析结果、以及 3 个深度调研产出的代码现状（详见 4 个 ticket 文件）。_
