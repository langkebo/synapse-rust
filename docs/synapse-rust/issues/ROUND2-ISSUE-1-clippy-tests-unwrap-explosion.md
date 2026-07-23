# ROUND2-ISSUE-1: 测试代码 clippy unwrap/expect 爆炸（3416+ errors）

**Status**: 🟢 resolved (unwrap/expect/panic errors eliminated in test code)
**Severity**: 中（不阻塞功能，但阻碍 CI 严格模式启用）
**Discovered**: 三项目协同优化 阶段 B 验收 (2026-07-17)
**Origin**: `cargo clippy --workspace --tests` 在严格 lint 配置下报错
**Blocks**: 不阻塞阶段 A/B/C；阻碍 `cargo clippy --all-targets -- -D warnings` 作为 CI 门禁

---

## 1. 背景

根 `Cargo.toml` 配置了严格 clippy lint：

```toml
[lints.clippy]
unwrap_used = "deny"
expect_used = "warn"
panic = "allow"
```

生产 lib 代码完全通过 clippy 检查（`cargo clippy --workspace --lib` 退出码 0），但 `--tests` 模式下报 3416+ 个错误，绝大多数来自测试代码中的 `.unwrap()` / `.expect()` 调用。

## 2. 错误分布

| Crate | 错误数 | 占比 |
|-------|--------|------|
| synapse-storage | ~3155 | 83% |
| synapse-common | ~317 | 8% |
| synapse-e2ee | ~201 | 5% |
| synapse-federation | ~84 | 2% |
| synapse-cache | ~19 | 1% |
| synapse-services | (lib 已通过) | - |
| **合计** | **~3776** | 100% |

错误类型细分：

| Clippy 规则 | 出现次数 | 严重级别 |
|-------------|----------|----------|
| `expect_used` on Result | 2070 | warn |
| `unwrap_used` on Result | 939 | **deny** |
| `unwrap_used` on Option | 292 | **deny** |
| `expect_used` on Option | 89 | warn |
| `unwrap_err_used` on Result | 25 | deny |
| **deny 总计** | **1256** | 阻断 |

## 3. 根因分析

| 原因 | 影响 | 说明 |
|------|------|------|
| 测试代码习惯性使用 `.unwrap()` | 主要 | Rust 测试惯用法倾向 unwrap/expect 简洁断言 |
| `[lints.clippy]` 配置未区分 lib/test | 主要 | 配置同时应用于 lib 和 tests，未使用 `cfg_attr(test, ...)` 豁免 |
| synapse-storage 测试基数大 | 次要 | 该 crate 测试用例数最多（数据库相关），unwrap 使用密度高 |

## 4. 推荐修复方案

### 方案 A：豁免测试代码（推荐，1 行修改）

在根 `Cargo.toml` 中为测试代码放宽 lint：

```toml
[lints.clippy]
unwrap_used = "deny"
expect_used = "warn"
panic = "allow"
# ... 其他规则保持不变

# 测试代码允许 unwrap/expect（Rust 测试惯用法）
[lints.clippy.workspace]
# 通过 cfg_attr 在 test 模式下豁免
```

或在 `src/lib.rs` 顶部：

```rust
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::unwrap_err_used))]
```

**优点**：1 行修改，立即解锁 CI 严格模式
**缺点**：测试代码失去 unwrap 滥用防护

### 方案 B：迁移到 `?` + `Result` 返回类型（高质量）

将所有测试函数签名改为 `fn foo() -> Result<(), Box<dyn std::error::Error>>`，用 `?` 替代 `.unwrap()`。

**优点**：错误信息更清晰（包含上下文），符合现代 Rust 测试最佳实践
**缺点**：3776 处修改，工作量大；需逐个评估是否真的需要 panic

### 方案 C：分 crate 渐进迁移 + ratchet 机制

1. 在 `[lints.clippy]` 中将 `unwrap_used` / `expect_used` 改为 `allow`
2. 创建 `scripts/clippy-tests-ratchet.sh` 记录当前基线（3776）
3. CI 中检查 clippy 错误数不超过基线
4. 后续 PR 必须降低基线或保持不变
5. 按 crate 顺序逐个迁移（先 synapse-cache 19 个，再 synapse-federation 84 个...）

**优点**：渐进改进，不阻塞当前 CI；强制基线下降
**缺点**：需要长期跟踪

## 当前状态（2026-07-23 验证）

- ✅ `unwrap_used` / `expect_used` / `unwrap_err_used` 错误：7 个 lib.rs 已添加 `#![cfg_attr(test, allow(...))]` 豁免，测试代码不再报错
- ✅ `panic` lint 错误：已在 `synapse-common`/`synapse-e2ee`/`synapse-services`/`synapse-rust` 的 lib.rs 及 bin 文件中添加 `clippy::panic` 豁免，测试代码不再报错
- `scripts/check_clippy_ratchet.sh` 已被归档到 `docs/archive/unused-scripts/`，因 ratchet 机制不再需要

## 剩余 10 个 panic 错误详情

| # | 文件 | 行 | 说明 |
|---|------|----|------|
| 1 | `synapse-common/src/background_job.rs:88` | panic!("Expected SendEmail variant") | 测试代码中的 match unreachable |
| 2 | `synapse-common/src/task_queue.rs:407` | panic::panic_any(...) | 测试通道发送失败 |
| 3-6 | `synapse-e2ee/src/vodozemac_interop_tests.rs` | 4 处 panic!("expected pre-key...") | vodozemac 测试断言 |
| 7-8 | `synapse-services/src/cas_service.rs:424,435` | panic!("Expected success/failure") | CAS 测试断言 |
| 9-10 | `synapse-services/src/worker/protocol.rs:395,406` | panic!("Expected Ping/Pong command") | Worker 协议测试断言 |

> 这些 `panic` 错误与原始的 `unwrap_used`/`expect_used` 问题不同，属于 `panic = "allow"` 配置下的预存在项。由于 `panic` lint 在生产代码中设为 `allow`，但在测试代码中未豁免，因此报错。建议：在相关 lib.rs 的 `#![cfg_attr(test, allow(...))]` 中添加 `clippy::panic`。

- [x] 选定修复方案并实施：7 个 lib.rs + 2 个 bin 文件已添加 `#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::unwrap_err_used, clippy::panic))]` 豁免
- [x] `cargo clippy --workspace --lib -- -D warnings` 退出码 0（生产代码完全通过）
- [x] `unwrap_used`/`expect_used`/`unwrap_err_used`/`panic` 测试代码错误已消除（从 3416+ 降至 0）

## 6. 工时估计

| 方案 | 时间 |
|------|------|
| A（豁免测试代码） | 0.1 天 |
| B（全量迁移到 `?`） | 5-8 天 |
| C（ratchet + 渐进迁移） | 0.5 天（初始）+ 持续 |

## 7. 备注

- 本 issue 不阻塞三项目协同优化的后端阶段
- 生产代码已 100% 通过 clippy，说明 lint 配置本身合理
- 推荐采用**方案 A**快速解锁 CI，同时开**方案 C**作为长期改进路径
- synapse-storage 占 83% 错误，应作为方案 C 的首要目标
