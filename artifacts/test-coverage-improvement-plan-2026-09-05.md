# 测试覆盖提升行动方案

> 目标：系统提升 synapse-rust 测试覆盖率，识别并消除核心逻辑的测试盲区。
> 日期：2026-09-05
> 状态：已调查，方案制定中

---

## 一、现状盘点

### 1.1 覆盖率数字（**2026-09-05 真实基线**）

| 维度 | 数字 | 说明 |
|------|------|------|
| **根 crate 实际行覆盖率** | **32.53%** | `cargo llvm-cov report --summary-only`（**已验证**）|
| **总 Lines** | 40,443 | 根 crate 代码 |
| **未覆盖 Lines** | 26,273 | 根 crate |
| **AGENTS.md 记录的 workspace 数字** | ~68% | 2026-08 完整 workspace（synapse-services/storage/e2ee/federation 全部）|
| **阶段目标** | 未在正式文档明确 | 历史上 archive 有 85% 目标（已过期），当前需要重新定义 |

> **⚠️ lcov 基线（26.82%）≠ 完整 workspace 覆盖率（~68%）**
>
> `coverage/lcov.info` 是 2026-09-04 部分跑的 lcov（仅跑了根 crate + 部分测试），
> 不是完整 `bash scripts/run_local_coverage.sh` 的结果。**AGENTS.md 记录的 ~68% 才是
> 真实的当前完整基线**，来自完整的 workspace llvm-cov 跑。
>
> 方案执行前需重跑 `bash scripts/run_local_coverage.sh` 建立新的准确基线。

### 1.2 测试层分布

| 层级 | 数量 | 说明 |
|------|------|------|
| 集成测试（`tests/integration/`） | **115 个文件** | 最厚，覆盖 route 层 |
| 单元测试（`tests/unit/`） | **108 个文件** | 覆盖 service/storage 层逻辑 |
| E2E 测试（`tests/e2e/`） | 少量 | 场景级 |
| Complement 测试（`tests/complement/`） | 有 | 联邦互操作 |
| **总测试函数** | ~1396 个 | AGENTS 记录 |

### 1.3 测试盲区（lcov 分析，2026-09-04）

以下文件**零覆盖或极低覆盖**，是重点补测目标：

| 文件 | 未覆盖行 | 覆盖率 | 现状分析 |
|------|---------|--------|---------|
| `web/routes/handlers/room/events.rs` | 821/821 | **0%** | 核心事件处理，987 行无测试 |
| `web/routes/friend_room.rs` | 722/874 | 17.4% | 好友房间路由 |
| `routes/admin/user.rs` | 697/731 | 4.7% | 管理后台用户路由 |
| `web/routes/key_backup.rs` | 672/763 | 11.9% | 密钥备份路由 |
| `routes/extractors/auth.rs` | 606/702 | 13.7% | 认证提取器 |
| `room/management/metadata.rs` | 602/620 | 2.9% | 房间元数据管理 |
| `web/routes/account_compat.rs` | 554/554 | **0%** | 账户兼容性路由 |
| `admin/room/mod.rs` | 552/660 | 16.4% | 房间管理 |
| `routes/federation/events.rs` | 504/819 | 38.5% | 联邦事件 |
| `web/routes/auth_compat.rs` | 489/489 | **0%** | 认证兼容性路由 |
| `routes/federation/transaction.rs` | 487/487 | **0%** | 联邦事务处理 |
| `handlers/room/members.rs` | 489/489 | **0%** | 房间成员处理 |
| `web/routes/module.rs` | 461/492 | 6.3% | 模块化路由 |
| `routes/e2ee/devices.rs` | 422/490 | 13.9% | 设备管理 |
| `routes/e2ee/keys.rs` | 418/488 | 14.3% | 密钥路由 |
| `routes/media/download.rs` | 334/345 | 3.2% | 媒体下载 |
| `web/routes/verification_routes.rs` | 332/510 | 34.9% | 验证路由 |

**0% 覆盖的关键文件（7 个）**：
1. `web/routes/handlers/room/events.rs` — 821 行，核心消息事件处理
2. `web/routes/account_compat.rs` — 554 行，账户兼容性（已有 unit 测试但未编译）
3. `web/routes/auth_compat.rs` — 489 行，认证兼容性
4. `routes/federation/transaction.rs` — 487 行，联邦事务（最关键）
5. `handlers/room/members.rs` — 489 行，房间成员处理
6. `admin/room/management.rs` — 449 行，房间管理
7. `routes/extractors/auth.rs` — 局部区块未覆盖

### 1.4 已 100% 覆盖文件（仅 3 个）
- `extractors/pagination.rs` (44 行)
- `routes/mod.rs` (226 行)
- `utils/auth.rs` (176 行)

### 1.5 已知低质测试

`tests/integration/coverage_tests.rs` 存在 **30+ 个无意义 no-op 测试**：

```rust
#[tokio::test]
async fn test_send_message() {
    let message = json!({ /* ... */ });
    assert!(message.get("content").is_some()); // 永远为 true，无任何业务价值
}
```

这些测试编译通过但不覆盖任何生产代码，是覆盖率数据的"噪音"来源。

### 1.6 测试 feature gate 风险

unit 测试的 `tests/unit/mod.rs` 中有 `#[cfg(feature = "beacons")]` 门控。
`test-utils` 是跑 unit 的必要 feature。部分测试可能因为 feature 组合不完整而未被编译。

---

## 二、目标设定建议

| 阶段 | 覆盖率目标 | 关键指标 | 说明 |
|------|-----------|---------|------|
| **Phase A（立即）** | 重跑基线，确认差距 | llvm-cov workspace 数字 | 消除 lcov 不一致 |
| **Phase B（短期）** | 50% → 65% | workspace 行覆盖率 | 聚焦 0% 文件补测 |
| **Phase C（中期）** | 65% → 75% | + 消除高价值盲区 | 聚焦 handlers/routes |
| **Phase D（目标）** | 75% | 综合判断 | 核心逻辑全部覆盖 |

> **目标合理性说明**：synapse-rust 是 Matrix homeserver，包含大量路由胶水代码和
> feature-gated 冷路径，100% 覆盖不现实也不经济。75% 是业界合理水平
> （Synapse Python 官方覆盖率约 60-70%）。

---

## 三、执行方案

### Phase A：建立准确基线（1 步，~15 分钟）

```bash
# 跑完整 workspace llvm-cov（需要 postgres 运行）
bash scripts/run_local_coverage.sh

# 生成新 lcov 后分析
python3 .workbuddy/tmp/cov_analysis.py
```

**验收标准**：
- `coverage/lcov.info` 更新，Rust 文件覆盖率明确
- 输出所有 <30% 覆盖的模块列表
- 确认与 AGENTS.md ~68% 基线的偏差

---

### Phase B：消除零覆盖关键文件（优先级排序）

按影响面和补测成本排序：

#### B-1：Federation 事务路由（最高优先）
**文件**：`routes/federation/transaction.rs` — 487 行，0% 覆盖
**为什么**：联邦事务是 Matrix 互操作的核心，P-0 安全路径
**方案**：
1. 写 `tests/integration/api_federation_transaction_coverage_tests.rs`
2. 覆盖场景：
   - 合法 transaction 提交（`/federation/v1 transaction/...`）
   - 事务去重（idempotency）
   - 过期事务拒绝
   - 签名验证失败
   - 缺少 room 上下文拒绝
3. 用现有的 `TestAppBuilder` 和 federation test helpers

#### B-2：认证兼容性路由
**文件**：`web/routes/auth_compat.rs`（489 行，0%）+ `account_compat.rs`（554 行，0%）
**为什么**：账户/认证兼容路径是安全边界
**方案**：
1. 检查 `tests/unit/auth_compat_route_tests.rs` / `account_compat_route_tests.rs` 为什么不编译
2. 修复编译错误后重跑 llvm-cov 确认覆盖
3. 补充缺失的边界场景

#### B-3：事件处理 handlers
**文件**：`web/routes/handlers/room/events.rs`（821 行，0%）+ `handlers/room/members.rs`（489 行，0%）
**为什么**：消息发送/事件处理是核心业务路径
**方案**：
1. 在 `tests/integration/` 中扩展 `api_room_sync_tests.rs` / 新建 `api_event_coverage_tests.rs`
2. 覆盖场景：
   - 普通消息发送
   - 事件重写（replacement）
   - 事件重新（redaction）
   - 消息编辑（edit）
   - 房间成员邀请/加入/离开

#### B-4：Admin 后台路由
**文件**：`routes/admin/user.rs`（697 行，4.7%）+ `admin/room/management.rs`（449 行，0%）
**方案**：
1. 扩展 `tests/integration/api_admin_user_lifecycle_tests.rs`
2. 新增 `tests/integration/api_admin_room_management_coverage_tests.rs`
3. 覆盖 admin 用户的批量操作、权限校验失败路径

#### B-5：Key Backup 路由
**文件**：`web/routes/key_backup.rs`（672 行，11.9%）
**方案**：扩展 `tests/integration/api_e2ee_advanced_tests.rs` 中 key_backup 相关测试

---

### Phase C：清理低质测试（降低噪音）

**文件**：`tests/integration/coverage_tests.rs`
**问题**：30+ 无意义 no-op 测试
**方案**：
1. 识别哪些是真正有用的（未来可能有用）
2. 删除其余无意义的 `assert!(json!().get().is_some())` 测试
3. **不要**盲目全部删除——其中有正确的测试结构保留

---

### Phase D：Feature Gate 覆盖补全

**问题**：`tests/unit/mod.rs` 中 `#[cfg(feature = "beacons")]` 等门控导致部分测试不编译
**方案**：
```bash
# 以扩展 feature 集跑 unit tests，确认所有门控测试编译
cargo test --test unit --features "test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications,cas-sso,saml-sso,external-services,builtin-oidc" --all-targets
```

---

### Phase E：建立覆盖率门禁（防止回退）

1. **在 `scripts/ci/` 中加入 llvm-cov 检查**（如果尚未）：
   ```bash
   # CI 失败当覆盖率低于基线 -5pp
   cargo llvm-cov --workspace --fail-under-lines 60
   ```

2. **覆盖率快照**：`coverage/` 中保存 `lcov.info` 作为 git-tracked 基线

3. **PR 门槛**：每次 PR 要求覆盖率不降低（可以用 `UPDATE_LCOV_SNAPSHOTS=1` 自动更新）

---

## 四、工时估算

| 阶段 | 任务 | 估算工时 | 依赖 |
|------|------|---------|------|
| A | 重跑 llvm-cov 基线 | ~20 分钟 | postgres |
| B-1 | Federation 事务补测 | 4-6h | Phase A 结果 |
| B-2 | 认证兼容路由 | 2-3h | Phase A 结果 |
| B-3 | 事件 handlers | 6-8h | Phase A 结果 |
| B-4 | Admin 路由 | 3-4h | Phase A 结果 |
| B-5 | Key Backup | 2h | Phase A 结果 |
| C | 清理低质测试 | 1h | 人工审查 |
| D | Feature gate 补全 | 1h | Phase A |
| E | 覆盖率门禁 | 2h | CI 配置 |

**总计：约 20 小时（3-5 人天）**

---

## 五、下一步行动（立即可做）

**在跑完 Phase A 之前**，可以并行开始：

1. **修复 `tests/integration/coverage_tests.rs`** —— 人工审查，删除无意义测试
2. **修复 unit 测试编译错误** —— 检查 auth_compat / account_compat 为什么 0%
3. **问：目标覆盖率是多少？** —— 确认 Phase D 目标值（建议 75%）

---

## 六、风险与注意事项

| 风险 | 说明 | 缓解 |
|------|------|------|
| 覆盖率基线不准 | lcov 数字 ≠ 真实完整 workspace 数字 | 先跑 Phase A |
| 测试编写成本高 | handlers 层需要 setup 大量 fixture | 复用现有 test helpers |
| Feature gate 复杂 | 部分代码是 debug-only 或 feature-gated | 用 `--all-features` 跑但限制并发 |
| 测试不稳定 | db 测试并发竞争 | 单线程跑 storage (`RUST_TEST_THREADS=1`) |
| 快照 drift | 覆盖率数字自然波动 | 用 git diff 审查 lcov 变化 |

---

*本文档基于 2026-09-04 调查，后续更新以 lcov 基线为准。*
