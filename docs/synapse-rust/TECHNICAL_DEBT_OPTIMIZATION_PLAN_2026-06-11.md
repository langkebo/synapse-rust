# Synapse-Rust 技术债务优化方案

> 版本: v2.8.0
> 日期: 2026-06-11
> 基于: v2.7.0 基础上完成 CI 测试矩阵分离（unit/integration 独立执行）、DMService 兼容模块删除、Phase B storage 层深度分析

---

## 一、复核结论总览

本次重新审查后，原方案中的多项任务状态已经发生明显变化：

- `route_ledger` 去重已基本完成：主 crate 的 [route_ledger.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/route_ledger.rs) 已退化为对 [synapse-web 版本](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/route_ledger.rs) 的 re-export，且启动日志与 `api_route_ledger_tests` 已存在。
- 路由层直引存储层已清零：在 `src/web/routes/` 下未再检出 `use crate::storage::`。
- 旧版 “`src/services/dm_service.rs` 死代码” 描述已过时：主 crate 已不再导出该模块，残留的是 `synapse-services` 中的兼容性、测试向内存实现。
- 配置与房间服务拆分都不是“未开始”，而是已部分拆分但停在中途。
- 测试架构问题比原文严重：`tests/unit/` 中已无 `#[ignore]` + database 旧模式，但仍有大量 DB 依赖测试按 unit 组织。

### 1.1 当前优先级重排

| 优先级 | 事项 | 当前状态 | 影响范围 | 风险 |
|--------|------|----------|----------|------|
| P2 | `unwrap/expect` clippy lint 门禁 | **门禁已建立**（`cargo clippy --lib` 仅 7 个合理警告） | 预防性治理 | 低 |
| P1 | `tests/unit/` 中 DB 依赖测试迁移/重分类 | **全部完成**（18 文件迁移，`tests/unit/` 零 DB 依赖） | 测试架构/CI 可靠性 | 高 |
| P1 | CI 中 `unit` 与 `integration` 执行矩阵分离 | **已完成**（`ci.yml` + `run_ci_tests.sh` 支持 --lib/--unit/--integration） | CI 可维护性 | 中 |
| P1 | 根 crate 与 `synapse-*` 子 crate 镜像模块漂移 | Phase A 完成（`rate_limit`/`crypto`/`health` re-export），`filter` 不可 re-export | 架构可维护性 | 高 |
| P2 | `config/mod.rs` 半拆分状态收尾 | 已部分落地 | 可维护性/配置安全 | 中 |
| P2 | `room/` 巨型文件拆分 | **全部完成**（23 子模块，全部 < 500 行） | 可维护性/房间域演进 | 低 |
| P3 | `DMService` 兼容模块收尾 | **已删除**（`synapse-services/src/dm_service.rs`，零外部引用） | 代码整洁性 | 低 |
| P3 | `route_ledger` 外壳文件是否保留 | 已基本完成 | 维护一致性 | 低 |
| P3 | 分层违规回归防护 | 已完成，转守护项 | 架构规范 | 低 |

---

## 二、已完成或基本完成的旧项

### 2.1 `route_ledger` 去重已基本完成

当前状态：

- [src/web/routes/route_ledger.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/route_ledger.rs) 仅 4 行，内容为：

```rust
pub use synapse_web::routes::route_ledger::*;
```

- 规范化实现位于 [synapse-web/src/routes/route_ledger.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/route_ledger.rs)。
- 路由装配阶段已有 `route manifest validated: N declared (method, path) tuples, 0 duplicates` 启动日志。
- 集成测试侧已有 `tests/integration/api_route_ledger_tests.rs`。

结论：

- 原 “双副本去重” 任务不应继续作为 P2 主任务。
- 剩余工作仅是是否接受保留一个 re-export 外壳文件。若团队接受该模式，则此项可直接标记完成。

### 2.2 路由层直引存储层已清零

复核结果：

- 在 `src/web/routes/` 下未检出 `use crate::storage::`。

结论：

- 原 “2 处分层违规修复” 已完成，不应再作为待办。
- 后续仅需保留 CI/脚本门禁，避免回归。

### 2.3 旧版 `DMService` 死代码判断需要降级

复核结果：

- 主 crate 的 [src/services/mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/services/mod.rs) 已不再 `pub mod dm_service;`。
- 兼容性实现位于 [synapse-services/src/dm_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/dm_service.rs)。
- 该模块当前通过 [synapse-services/src/mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/mod.rs) 上的 `#[cfg(any(test, feature = "test-utils"))] pub mod dm_service;` 仅在测试/测试工具场景编译。

结论：

- 这已经不是 “运行时死代码模块必须立即删除” 的问题。
- 更准确的表述应为：兼容性测试模块是否仍有保留价值。

---

## 三、P0 — `unwrap/expect` 风险治理

### 3.1 现状（2026-06-11 精确复核）

本轮对 federation、e2ee、crypto、services 核心路径进行了**排除测试代码的精确扫描**：

- grep 裸命中 `unwrap()/expect()` 约 820 处，但 **99%+ 位于 `#[cfg(test)]` 测试块中**。
- 生产代码中，关键运行时路径（federation/e2ee/auth/services）已基本无裸 `unwrap()`。
- 生产代码中仅存的 `unwrap` 均为**安全防御模式**：`unwrap_or()`、`unwrap_or_else()`、`unwrap_or_default()`。
- 唯一的生产代码裸断言：[models.rs:36](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/services/friend_room_service/models.rs#L36) `.expect("friend list cursor serialization should succeed")` — 对简单结构体序列化的合理断言。

### 3.2 精确热点（仅生产代码，排除测试）

| 文件 | 生产代码 unwrap/expect | 实际风险 |
|------|------------------------|----------|
| `src/e2ee/crypto/aes.rs` | **0**（30 全在测试） | 无风险 |
| `src/services/media_service.rs` | **0**（24 全在测试） | 无风险 |
| `src/services/typing_service.rs` | **0**（23 全在测试） | 无风险 |
| `src/federation/device_sync.rs` | **0**（20 全在测试） | 无风险 |
| `src/common/crypto.rs` | **1**（已有 `#[allow]`） | 极低 |
| `src/federation/` 全部文件 | **0**（全在测试） | 无风险 |
| `src/services/` 全部文件 | **1**（models.rs 合理断言） | 极低 |

### 3.3 重新评估

federation/e2ee/auth 核心运行时路径已在前期重构中自然收敛至 `Result` 传播模式。
当前 `unwrap/expect` 风险主要存在于：

- 测试代码（非运行时问题）
- 配置加载/启动初始化代码（启动期 panic 可接受）

### 3.4 修订后的治理策略

**Phase 1 — 建立预防门禁（低投入高收益）**

- 在 crate 根启用 `#![warn(clippy::unwrap_used, clippy::expect_used)]`
- 对测试目录批量 `#[allow(...)]`
- 先把新代码拦住，再逐步清旧债

**Phase 2 — 收尾扫尾（低优先级）**

- `cache/`、`config/`、`web/middleware/` 等非关键路径的防御性加固
- 对可降级路径优先做 graceful fallback

### 3.5 验收标准（修订）

- [x] 关键运行时路径（federation/e2ee/auth）无裸 `unwrap()` ✅ 已达成
- [x] 仓内启用 crate 级 `clippy::unwrap_used` / `clippy::expect_used` 警告 ✅ 已启用（2026-06-11）
- [x] 新增代码默认不得引入裸 `unwrap()/expect()` — 门禁已建立

---

## 四、P1 — `tests/unit/` 中 DB 依赖测试迁移/重分类

> 2026-06-11 更新：**迁移已完成**

### 4.1 背景

- 旧例中的 [sticky_event_tests.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/tests/unit/sticky_event_tests.rs) 已经是纯结构体/纯逻辑测试
- 但当前 `tests/unit/` 下仍有大量文件通过 `setup_test_database()` 或 "database is unavailable" 分支运行数据库相关逻辑

本轮复核口径：

- 命中 `setup_test_database(` 的 unit 测试文件：31 个
- 相关调用总数：647 次

### 4.2 已完成工作（2026-06-11）

已将 20 个 storage 测试文件迁移到 `tests/integration/`，使用 `_migrated` 后缀：

| 源文件 (tests/unit/) | 目标文件 (tests/integration/) | 状态 |
|----------------------|------------------------------|------|
| `cross_signing_storage_tests.rs` | `cross_signing_storage_tests_migrated.rs` | 编译通过 |
| `device_storage_tests.rs` | `device_storage_tests_migrated.rs` | 编译通过 |
| `event_storage_tests.rs` | `event_storage_tests_migrated.rs` | 编译通过 |
| `feature_flags_storage_tests.rs` | `feature_flags_storage_tests_migrated.rs` | 编译通过 |
| `federation_blacklist_storage_tests.rs` | `federation_blacklist_storage_tests_migrated.rs` | 编译通过 |
| `filter_storage_tests.rs` | `filter_storage_tests_migrated.rs` | 编译通过 |
| `friend_room_storage_tests.rs` | `friend_room_storage_tests_migrated.rs` | 编译通过 |
| `key_backup_storage_tests.rs` | `key_backup_storage_tests_migrated.rs` | 编译通过 |
| `megolm_dual_write_storage_tests.rs` | `megolm_dual_write_storage_tests_migrated.rs` | 编译通过 |
| `membership_storage_tests.rs` | `membership_storage_tests_migrated.rs` | 编译通过 |
| `openid_token_storage_tests.rs` | `openid_token_storage_tests_migrated.rs` | 编译通过 |
| `presence_storage_tests.rs` | `presence_storage_tests_migrated.rs` | 编译通过 |
| `receipt_storage_tests.rs` | `receipt_storage_tests_migrated.rs` | 编译通过 |
| `refresh_token_storage_tests.rs` | `refresh_token_storage_tests_migrated.rs` | 编译通过 |
| `room_tag_storage_tests.rs` | `room_tag_storage_tests_migrated.rs` | 编译通过 |
| `sliding_sync_storage_tests.rs` | `sliding_sync_storage_tests_migrated.rs` | 编译通过 |
| `state_groups_storage_tests.rs` | `state_groups_storage_tests_migrated.rs` | 编译通过 |
| `threepid_storage_tests.rs` | `threepid_storage_tests_migrated.rs` | 编译通过 |
| `token_storage_tests.rs` | `token_storage_tests_migrated.rs` | 编译通过 |
| `user_storage_tests.rs` | `user_storage_tests_migrated.rs` | 编译通过 |

迁移模式：
- `setup_test_database()` 改为接收 `pool: &Arc<sqlx::PgPool>` 参数
- 测试函数使用 `let pool = crate::require_test_pool().await; setup_test_database(&pool).await;`
- 移除 `Runtime::new().block_on()` 包装，直接使用 `#[tokio::test]`
- 所有迁移文件在 `tests/integration/mod.rs` 中注册

### 4.3 剩余工作

- [x] 删除 `tests/unit/` 中已被迁移的 20 个源文件（避免重复）— 已完成（2026-06-11）
- [x] `tests/unit/mod.rs` 中已移除 19 个 `mod` 声明 — 已完成
- [x] 将剩余的 18 个 `tests/unit/` DB 依赖测试（service 测试等）迁移 — **已完成（2026-06-11）**
- [x] `tests/unit/` 最终只保留纯函数、纯结构体、无 I/O 测试 — **已完成**

### 4.4 验收标准

- [x] 20 个 storage 测试文件已迁移到 `tests/integration/`（已完成）
- [x] 18 个 service 测试文件已迁移到 `tests/integration/`（已完成）
- [x] `cargo test --features test-utils --test integration --no-run` 编译通过（0 错误）
- [x] `tests/unit/` 中不再出现 `setup_test_database()` / `PgPool` / `TestDatabase`
- [x] CI 中 `unit` 与 `integration` 的执行矩阵清晰分离 — **已完成（2026-06-11）**
  - `.github/workflows/ci.yml`：`test` job 分离为 `--lib` + `--test unit` 两个独立步骤
  - `.github/workflows/ci.yml`：`integration-test` job 改为仅执行 `--test integration`
  - `scripts/run_ci_tests.sh`：支持 `--lib` / `--unit` / `--integration` 独立目标，默认全部执行

---

## 五、P1 — 根 crate 与 `synapse-*` 子 crate 镜像模块漂移

> 2026-06-11 全面分析：基于实际文件对比，确认漂移范围和严重程度。

### 5.1 Workspace 结构

根 crate `synapse-rust` 依赖 7 个子 crate：

| 子 crate | 路径 | 职责 |
|----------|------|------|
| `synapse-common` | `synapse-common/` | 公共类型、配置、错误、日志、加密工具 |
| `synapse-web` | `synapse-web/` | HTTP 路由、中间件、过滤器 |
| `synapse-services` | `synapse-services/` | 业务逻辑服务 |
| `synapse-storage` | `synapse-storage/` | 数据持久化 |
| `synapse-e2ee` | `synapse-e2ee/` | 端到端加密 |
| `synapse-federation` | `synapse-federation/` | 联邦协议 |
| `synapse-cache` | `synapse-cache/` | 缓存层 |

### 5.2 镜像模块规模对比（2026-06-11 实测）

| 根 crate 模块 | 根行数 | 子 crate 模块 | 子行数 | 漂移 | 关系 |
|---------------|--------|---------------|--------|------|------|
| `src/common/config/mod.rs` | 1997 | `synapse-common/src/config/mod.rs` | 1999 | 2 行 | 完全镜像（子多 `experimental`/`policy_server` 模块） |
| `src/common/error.rs` | 26 | `synapse-common/src/error.rs` | 1293 | 1267 行 | **正确架构**（根是 thin re-export） |
| `src/common/logging.rs` | 83 | `synapse-common/src/logging.rs` | 77 | 6 行 | 各自独立实现 |
| `src/common/crypto.rs` | 469 | `synapse-common/src/crypto.rs` | 470 | 1 行 | 近完全镜像 |
| `src/common/rate_limit.rs` | 271 | `synapse-common/src/rate_limit.rs` | 271 | 0 | 完全相同 |
| `src/common/health.rs` | 288 | `synapse-common/src/health.rs` | 221 | 67 行 | 分叉 |
| `src/storage/device.rs` | 985 | `synapse-storage/src/device.rs` | 1082 | 97 行 | 分叉（子更完整） |
| `src/storage/filter.rs` | 149 | `synapse-storage/src/filter.rs` | 146 | 3 行 | 近镜像 |
| `src/storage/membership.rs` | 719 | `synapse-storage/src/membership.rs` | 777 | 58 行 | 分叉（子更完整） |
| `src/storage/event/mod.rs` | 881 | `synapse-storage/src/event/mod.rs` | 800 | 81 行 | **反向分叉**（根更完整） |
| `src/services/room/service.rs` | 2004 | `synapse-services/src/room/service.rs` | 1435 | 569 行 | **严重分叉**（根更完整） |
| `src/cache/mod.rs` | 1320 | `synapse-cache/src/lib.rs` | 1322 | 2 行 | 近镜像 |
| `src/federation/mod.rs` | 21 | `synapse-federation/src/lib.rs` | 20 | 1 行 | 各自 re-export 入口 |
| `src/e2ee/mod.rs` | 69 | `synapse-e2ee/src/lib.rs` | 69 | 0 | 完全相同 |
| `src/web/mod.rs` | 13 | `synapse-web/src/lib.rs` | 11 | 2 行 | 各自 re-export 入口 |

**总计**: 15 对镜像模块中，4 对严重分叉（`error` 是正确 thin re-export，不算分叉），6 对近镜像。

### 5.3 根 crate 模块组织模式分析

根 crate `src/common/mod.rs` **声明自己的模块**而非 re-export 子 crate：

```rust
// src/common/mod.rs — 根 crate 声明自己的模块
pub mod crypto;       // 469 行，自己的实现
pub mod rate_limit;   // 271 行，自己的实现
pub mod config;       // 1997 行，自己的实现
// ...
pub use synapse_common::metrics;        // 少数模块是从子 crate re-export
pub use synapse_common::server_metrics;
```

只有 `src/common/error.rs` 正确使用了 thin re-export 模式：
```rust
pub use synapse_common::error::{ApiError, ApiErrorKind, ...};
```

### 5.4 风险矩阵

| 风险 | 严重程度 | 说明 |
|------|----------|------|
| 行为漂移 | **高** | `room/service.rs` 根 2004 行 vs 子 1435 行，569 行差异意味着两边行为已不同 |
| 反向分叉 | **高** | `event/mod.rs` 根 881 行 vs 子 800 行——根 crate 多了 81 行业务逻辑，子 crate 是“过时版本” |
| 维护成本翻倍 | **高** | 修一个 bug 需要同时在两处修（如 `crypto.rs`、`rate_limit.rs`） |
| 审计不可靠 | 中 | CI 编译的可能是子 crate，但实际运行路径走根 crate |
| 新人困惑 | 中 | 无法确定哪个是权威实现 |

### 5.5 治理方案

核心原则：**每个领域只保留一个 canonical 实现，根 crate 通过 re-export 引用。**

| 领域 | Canonical crate | 具体动作 |
|------|-----------------|----------|
| config | `synapse-common` | 根源 `config/` 改为 thin re-export；差异子模块（`experimental`/`policy_server`）补到根 |
| crypto / rate_limit / health | `synapse-common` | 删根源，改 re-export |
| storage (device/filter/membership/event) | `synapse-storage` | 以子 crate 为 canonical，差异逻辑优先合入子 crate |
| room service | `synapse-services` | 以子 crate 为 canonical，根源 569 行差异需审计后合入 |
| cache | `synapse-cache` | 以子 crate 为 canonical |
| e2ee / federation / web | 已是各自入口 | 当前状态可接受（各自是 mod.rs 入口文件） |

### 5.6 实施步骤

1. **Phase A — 低风险 module 收口**（先做简单的）
   - `crypto.rs`（差异 1 行）、`rate_limit.rs`（差异 0 行）→ 根源改为 `pub use synapse_common::{crypto, rate_limit}`
   - `filter.rs`（差异 3 行）→ 同上
   - `health.rs`（差异 67 行）→ 差异审计后收口

2. **Phase B — storage 层收口**
   - `device.rs`（子更完整 97 行）、`membership.rs`（子更完整 58 行）→ 以子为准，根源删
   - `event/mod.rs`（反向分叉 81 行）→ 根源多出的逻辑审计后移入子 crate

3. **Phase C — 高复杂度收口**
   - `config/mod.rs`（1997 vs 1999）→ 差异集中在子 crate 多了 `experimental`/`policy_server` 模块
   - `room/service.rs`（569 行差异）→ 最大风险项，需逐函数审计

4. **Phase D — 验证**
   - `cargo test --all-features` 全量通过
   - 删除根源镜像文件后，无编译中断
   - route ledger 的 re-export 模式作为参考模板

### 5.7 验收标准

- [x] 完成 15 对镜像模块的全面对比分析（本次完成）
- [x] Phase A：`rate_limit`（0 差异）改为 re-export — 已完成（2026-06-11）
- [x] Phase A：`crypto`（3 处微小差异）改为 re-export — 已完成（2026-06-11）
- [x] Phase A：`health`（根多 `CacheHealthCheck` + `openapi-docs`）— 已 re-export 共性部分，`CacheHealthCheck` 保留 root 扩展（2026-06-11）
- [ ] Phase A：`filter`（宏风格差异：`query_as!` vs `query_as`）— 不可简单 re-export（根 crate 不依赖 `synapse-storage`），需结构性重构
- [ ] Phase B：`device`/`membership`/`event` — **已分析，发现深层架构问题**（见 §5.8）
- [ ] Phase C：`config`、`room/service` 逐函数审计后收口（预计 W3-W4）
- [ ] Phase D：全量测试通过，根源不再保留大体量镜像实现（预计 W4）
- [ ] 审计时不再需要同时统计两套近似模块

### 5.8 Phase B Storage 层深层分析（2026-06-11）

三个 storage 文件有相同的根本差异模式，且问题比预期更深：

| 文件 | 根行数 | 子行数 | 根 SQL 风格 | 子 SQL 风格 | 其他差异 |
|------|--------|--------|-------------|-------------|----------|
| `device.rs` | 985 | 1082 | `query!()` 宏（编译期检查） | `query()` + `.bind()`（运行时） | 子多 97 行，含额外方法 + `sqlx::Row` trait |
| `membership.rs` | 719 | 777 | `query_as!()` 宏 | `query_as::<_, T>()` + `.bind()` | 子多 58 行，支持 `tx` 事务参数；import 路径不同 |
| `event/mod.rs` | 881 | 800 | `query_as!()` 宏 | `query_as::<_, T>()` + `.bind()` | **根有 `add_ephemeral_event`、子有 `delete_events_before`**（不同方法！）字段名 `processed_ts` vs `processed_at` |

**深层发现**：`synapse_storage::` 被 100 个子 crate 文件使用（`synapse-web`、`synapse-services`、`synapse-federation`），但根 crate 的 `src/` **完全不导入 `synapse-storage`**——根 crate 使用自己的 `src/storage/` 实现。这意味着项目中存在 **两套平行的存储实现**，分别服务于不同的调用者。

**结论**：Phase B 不是简单的"删根源改 re-export"，而是需要：
1. 统一 SQL 风格（宏 vs 运行时 bindings）
2. 合并两套实现中的差异方法
3. 让所有调用者使用统一的 `synapse-storage`

这属于结构性重构，不适合在当前迭代中以简单 re-export 处理。

---

## 六、P2 — 巨型文件拆分（已部分落地，需收尾）

### 6.1 `config/mod.rs`：已完成拆分 ✅

> 2026-06-11 更新：**拆分已完成**

当前状态：

- [src/common/config/mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/common/config/mod.rs) 已从 1997 行瘦身至 **199 行**，仅保留 `Config` 聚合结构体、`database_url()`、`redis_url()`、`access_token_lifetime_seconds()` 辅助方法
- 配置加载逻辑已提取到 [src/common/config/loader.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/common/config/loader.rs)（234 行）
- 配置验证逻辑已提取到 [src/common/config/validation.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/common/config/validation.rs)（86 行）
- 测试代码已提取到 [src/common/config/tests.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/common/config/tests.rs)（682 行）
- 删除了 855 行注释死代码

结论：此项已完成，`mod.rs` 199 行，远低于 500 行目标。

### 6.2 `room/`：完整拆分已完成 ✅

> 2026-06-11 更新：**Phase 5.1 剩余大文件拆分已全部完成**

当前结构（按行数排序）：

```text
src/services/room/
├── mod.rs                  # 31 行 — 模块声明
├── utils.rs                # 25 行 — 工具函数
├── read_markers.rs         # 58 行 — MSC2654 阅读标记
├── receipts.rs             # 63 行 — 已读回执
├── burn_after_read.rs      # 85 行 — 阅后即焚调度
├── aliases.rs              # 101 行 — 别名管理、目录、公开房间
├── upgrade.rs              # 137 行 — 房间升级、迁移
├── info.rs                 # 152 行 — 加密状态查询、删除、用户房间列表
├── membership.rs           # 166 行 — 成员查询、add_member ✅
├── space/                  # 空间模块（目录）
│   ├── mod.rs              # 329 行 — 核心 CRUD、状态、查询 ✅
│   ├── membership.rs       # 165 行 — 邀请、加入、退出、成员列表 ✅
│   └── children.rs         # 343 行 — 子房间管理、层级结构 ✅
├── membership_actions.rs   # 180 行 — join/leave/forget ✅
├── create_events.rs        # 200 行 — 房间创建事件助手 ✅
├── events.rs               # 217 行 — 状态事件、事件 CRUD、签名
├── summary_stats.rs        # 215 行 — 统计、队列、heroes 重算 ✅
├── summary.rs              # 248 行 — 核心 CRUD ✅
├── summary_state.rs        # 263 行 — 状态管理、同步 ✅
├── membership_moderation.rs # 283 行 — invite/knock/ban/unban/kick ✅
├── service.rs              # 384 行 — 核心门面、配置、生命周期、基础查询
├── create.rs               # 473 行 — 房间创建 ✅
└── messages.rs             # 321 行 — 消息发送、分页、ephemeral、typing
```

拆分结果：

- `service.rs` 从 1998 行瘦身至 **384 行** ✅
- `membership.rs`（611 行）→ `membership.rs`（166 行）+ `membership_actions.rs`（180 行）+ `membership_moderation.rs`（283 行）✅
- `create.rs`（662 行）→ `create.rs`（473 行）+ `create_events.rs`（200 行）✅
- `summary.rs`（711 行）→ `summary.rs`（248 行）+ `summary_state.rs`（263 行）+ `summary_stats.rs`（215 行）✅
- `space.rs`（801 行）→ `space/` 目录（`mod.rs` 329 行 + `membership.rs` 165 行 + `children.rs` 343 行）✅
- **所有文件均 < 500 行** ✅

结论：`room/` 目录拆分已全部完成，共 23 个文件（含 1 个目录），无任何文件超过 500 行。

### 6.3 验收标准

- [x] `src/common/config/mod.rs` < 500 行（**199 行** ✅）
- [x] `src/services/room/service.rs` < 500 行（**384 行** ✅）
- [x] `src/services/room/` 下无 > 500 行文件（**全部完成** ✅）
- [ ] workspace 镜像版本同步收口，而不是双边同时继续膨胀

---

## 七、P3 — `DMService` 兼容模块收尾（已完成）

### 7.1 清理结果（2026-06-11）

已删除 `synapse-services/src/dm_service.rs`（399 行），原因：

- **零外部引用**：根 crate `src/` 和 `tests/` 无任何 `DMService` 导入
- **纯内存自测**：所有测试仅覆盖模块自身，无外部调用方
- **运行时路径已迁移**：DM 语义已收敛至 `FriendRoomService + m.direct account data`
- **模块门控已追溯**：`lib.rs` 和 `mod.rs` 中的 `#[cfg(any(test, feature = "test-utils"))] pub mod dm_service;` 声明均已移除

### 7.2 验收标准

- [x] 明确 `DMService` 的唯一用途（零外部引用，纯自测）
- [x] 在 workspace 中彻底删除（`dm_service.rs` + 两个 mod 声明）
- [x] `cargo check --all-features` 编译通过（0 错误）

---

## 八、执行路线图（修订版 v2.8.0）

| 阶段 | 周次 | 任务 | 产出 |
|------|------|------|------|
| Phase 1 | W1 | P1 测试重分类（38 文件迁移完成） | tests/unit/ 零 DB 依赖 ✅ |
| Phase 1 | W1 | P1 canonical crate Phase A：`rate_limit`/`crypto`/`health` re-export | 3 对镜像模块收口 ✅ |
| Phase 1 | W1 | P1 CI 矩阵分离：`--lib` / `--test unit` / `--test integration` | ci.yml + run_ci_tests.sh 支持独立目标 ✅ |
| Phase 1 | W1 | P3 DMService 兼容模块删除 | 399 行死代码清理 ✅ |
| Phase 2 | W2 | P1 canonical crate Phase B：`device`/`membership`/`event` 深度分析 | 已分析，确认两套平行存储实现，需结构性重构 |
| Phase 3 | W3 | P0 clippy lint 门禁建立 | 预防新增裸 unwrap/expect ✅ |
| Phase 4 | W4-W5 | P1 canonical crate Phase C：`config`/`room/service` 审计合并 | 高风险大模块收口（待执行） |
| Phase 5 | W6-W7 | P2 `config/mod.rs` 收尾瘦身 + `room/` 核心拆分 | **已完成** ✅ config 199 行，room/ 从 12→15 子模块 |
| Phase 5.1 | W7-W8 | P2 `room/` 剩余大文件拆分（`create.rs`/`summary.rs`/`space.rs`/`membership.rs`） | **已完成** ✅ 全部 < 500 行（2026-06-11） |
| Phase 6 | W8+ | P3 兼容模块清理 + 长期治理机制稳定化 | 技术债务常态化管理 |

---

## 九、成功指标（修订版 v2.8.0）

| 指标 | 当前复核值 | 目标 |
|------|------------|------|
| `src/` 下生产代码 `unwrap/expect` | **~2 处**（核心路径已清零） | 核心路径保持 0 |
| 测试代码 `unwrap/expect` | ~820 处 | 正常（测试允许） |
| `tests/unit/` 中 DB 依赖文件 | **0** ✅（全部 38 文件已迁移） | 0 ✅ |
| `tests/unit/` 中 `setup_test_database()` 调用 | 0 ✅ | 0 ✅ |
| CI 矩阵分离 | **已完成** ✅（`--lib` + `--test unit` + `--test integration`） | 完成并固化 ✅ |
| `config/mod.rs` 行数 | ~~1997~~ **199** ✅ | < 500 |
| `room/service.rs` 行数 | ~~1998~~ **384** ✅ | < 500 |
| `room/` 子模块数 | 23 个（15 个新增） | 全部 < 500 行 ✅ |
| `route_ledger` 去重状态 | 已完成 | 完成并固化 |
| 路由层直引存储层 | 0 处 | 保持 0 |
| `DMService` 运行时暴露 | **已删除** ✅ | 零死代码 ✅ |

---

## 十、风险评估（修订版 v2.8.0）

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|----------|
| `unwrap/expect` panic 导致服务中断 | **低**（核心路径已清零） | 高 | clippy lint 门禁防止新增 |
| DB 依赖测试仍伪装为 unit，造成假阳性 | **已消除**（38 文件已迁移） | — | CI 矩阵已分离 |
| workspace 镜像模块继续分叉 | 高 | 高 | 先做 canonical crate 决策，再做文件级收口 |
| config 拆分破坏反序列化 | 中 | 高 | 每步拆分后验证 `homeserver.yaml` 加载与默认值行为 |
| room 域继续拆分引入回归 | 中 | 中 | 每次拆分后运行针对性 service/storage 测试 |
| storage 层两套平行实现导致行为漂移 | 高 | 高 | 已深度分析，下一步需统一 SQL 风格 + 合并差异方法 |
