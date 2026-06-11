# Synapse-Rust 技术债务优化方案

> 版本: v2.1.0
> 日期: 2026-06-11
> 基于: 2026-06-11 本地代码复核 + unwrap/expect 精确扫描 + 现有审计报告交叉校验

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
| P2 | `unwrap/expect` clippy lint 门禁 | 核心路径已清零，仅需门禁 | 预防性治理 | 低 |
| P1 | `tests/unit/` 中 DB 依赖测试迁移/重分类 | 明显失真，需系统治理 | 测试架构/CI 可靠性 | 高 |
| P1 | 根 crate 与 `synapse-*` 子 crate 镜像模块漂移 | 新识别，结构性债务 | 架构可维护性 | 高 |
| P2 | `config/mod.rs` 半拆分状态收尾 | 已部分落地 | 可维护性/配置安全 | 中 |
| P2 | `room/service.rs` 及关联模块继续拆分 | 已部分落地 | 可维护性/房间域演进 | 中 |
| P3 | `DMService` 兼容模块收尾 | 已降级为低优先级 | 代码整洁性 | 低 |
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
- [ ] 仓内启用 crate 级 `clippy::unwrap_used` / `clippy::expect_used` 警告
- [ ] 新增代码默认不得引入裸 `unwrap()/expect()`

---

## 四、P1 — `tests/unit/` 中 DB 依赖测试重分类

### 4.1 现状

原方案中的 “5 个需要数据库的测试被 `#[ignore]` 放在 `tests/unit/` 下” 已不成立：

- `tests/unit/` 下未再检出 `#[ignore]`
- 旧例中的 [sticky_event_tests.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/tests/unit/sticky_event_tests.rs) 已经是纯结构体/纯逻辑测试
- 但当前 `tests/unit/` 下仍有大量文件通过 `setup_test_database()` 或 “database is unavailable” 分支运行数据库相关逻辑

本轮复核口径：

- 命中 `setup_test_database(` 的 unit 测试文件：31 个
- 相关调用总数：647 次

这意味着当前问题不是 “少量遗漏”，而是：

> 大量依赖 Postgres fixture 的测试仍以 unit 目录形态组织，只是从 `#[ignore]` 改成了“数据库不可用时跳过/提前返回”。

### 4.2 风险

- “unit” 命名与真实测试类型不一致，误导开发者和 CI 设计
- 本地跑 unit 时结果不稳定，可能出现“看似通过，实际大面积跳过”
- 测试并发、模板库克隆、schema 清理策略难以统一

### 4.3 修复策略

**方案 A（推荐）— 迁移到 `tests/integration/`**

迁移原则：

- 只要依赖 `PgPool`、`TestDatabase`、迁移、真实 schema，即归入 integration
- `tests/unit/` 只保留纯函数、纯结构体、无 I/O、无 DB、无网络测试

迁移批次建议：

1. storage 测试：`*_storage_tests.rs`
2. service 测试：`room_service_tests.rs`、`sync_service_tests.rs` 等
3. auth/admin 注册链路测试

**方案 B — 保留目录但改名分层**

如果短期不愿大迁移，则至少：

- 新增 `tests/db/` 或 `tests/service_db/`
- 将 DB fixture 依赖测试迁出 `tests/unit/`
- 在 `tests/unit/mod.rs` 只保留真正 unit 测试

### 4.4 验收标准

- [ ] `tests/unit/` 中不再出现 `setup_test_database()` / `PgPool` / `TestDatabase`
- [ ] DB 依赖测试统一迁入 `tests/integration/` 或 `tests/db/`
- [ ] CI 中 `unit` 与 `integration` 的执行矩阵清晰分离

---

## 五、P1 — 根 crate 与 `synapse-*` 子 crate 镜像模块漂移

### 5.1 问题描述

本轮复核新增识别到更高价值的结构性债务：根 crate 的 `src/` 与多个 workspace 子 crate 之间存在并行实现或近镜像实现。

典型证据：

| 根 crate | 子 crate | 现状 |
|----------|----------|------|
| `src/common/config/mod.rs` 1997 行 | `synapse-common/src/config/mod.rs` 1999 行 | 高度相似，长期并行维护风险高 |
| `src/services/room/service.rs` 1998 行 | `synapse-services/src/room/service.rs` 1428 行 | 职责重叠，存在分叉风险 |
| `src/services/room/space.rs` 748 行 | `synapse-services/src/room/space.rs` 712 行 | 近镜像 |
| `src/services/room/summary.rs` 704 行 | `synapse-services/src/room/summary.rs` 604 行 | 近镜像 |
| `src/web/routes/route_ledger.rs` | `synapse-web/src/routes/route_ledger.rs` | 已通过 re-export 方式收口，是可复用模式 |

### 5.2 风险

- 修一个 crate，另一个 crate 未同步，出现行为漂移
- 审计统计与真实运行路径难以统一
- 后续拆分/重构成本被放大一倍

### 5.3 治理方向

为每个领域先定义唯一事实来源：

- config：以 `synapse-common` 为唯一实现，根 crate 仅做 re-export / glue
- room service：以 `synapse-services` 为唯一实现，根 crate 保持组装
- web route ledger：当前模式可作为模板

### 5.4 验收标准

- [ ] 对 config / room / common 等领域完成 canonical crate 决策
- [ ] 根 crate 不再保留大体量镜像实现
- [ ] 审计时不再需要同时统计两套近似模块

---

## 六、P2 — 巨型文件拆分（已部分落地，需收尾）

### 6.1 `config/mod.rs`：从“未拆分”改为“半拆分”

当前状态：

- [src/common/config/](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/common/config) 下已存在 20+ 个子模块
- 但 [src/common/config/mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/common/config/mod.rs) 仍有 1997 行
- 个别子模块本身也开始膨胀，如 `voip.rs` 332 行

结论：

- 该项不能再按 “从零开始拆分” 来写
- 真正待办是：把聚合入口瘦身到只保留 `Config` 聚合、`load/validate` 和必要 re-export

建议动作：

1. 盘点 `mod.rs` 中仍残留的结构体/默认值/校验逻辑
2. 将领域校验分别下沉到 `auth.rs`、`server.rs`、`federation.rs`、`security.rs`
3. 将 `mod.rs` 控制到 500 行以内

### 6.2 `room/`：已拆分目录，但大文件仍成群存在

当前状态：

- [src/services/room/service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/services/room/service.rs) 1998 行
- [src/services/room/create.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/services/room/create.rs) 626 行
- [src/services/room/space.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/services/room/space.rs) 748 行
- [src/services/room/summary.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/services/room/summary.rs) 704 行

结论：

- 该项不是“是否拆目录”的问题，而是目录拆了，但功能域切分仍过粗。

建议目标结构：

```text
src/services/room/
├── mod.rs
├── service.rs          # 只保留门面与组装
├── create.rs
├── membership.rs
├── state.rs
├── power_levels.rs
├── aliases.rs
├── visibility.rs
├── upgrade.rs
├── summary.rs
├── space.rs
└── utils.rs
```

### 6.3 验收标准

- [ ] `src/common/config/mod.rs` < 500 行
- [ ] `src/services/room/service.rs` < 500 行
- [ ] `src/services/room/` 下无 > 500 行文件
- [ ] workspace 镜像版本同步收口，而不是双边同时继续膨胀

---

## 七、P3 — `DMService` 兼容模块收尾

### 7.1 当前定位

[synapse-services/src/dm_service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/dm_service.rs) 现在更像：

- 测试辅助
- 旧接口兼容
- 纯内存语义验证

而不是运行时主路径。

### 7.2 建议

不再把它当作 P1 主任务删除，而是先做一轮价值判断：

1. 确认是否仍有 `test-utils` 用户真实依赖
2. 若仅剩内部测试，可迁到专用 `test_utils` / `compat` 目录
3. 若没有外部价值，再删除

### 7.3 验收标准

- [ ] 明确 `DMService` 的唯一用途（兼容 or 测试）
- [ ] 若保留，则移动到更准确的命名空间
- [ ] 若无价值，则在 workspace 中彻底删除

---

## 八、执行路线图（修订版）

| 阶段 | 周次 | 任务 | 产出 |
|------|------|------|------|
| Phase 1 | W1-W2 | P1 测试重分类 + P1 canonical crate 决策 | 先止住测试/架构继续漂移 |
| Phase 2 | W3-W4 | P0 clippy lint 门禁建立 | 预防新增裸 unwrap/expect |
| Phase 3 | W5-W6 | P1 workspace 镜像收口 | config/room 等领域明确唯一事实来源 |
| Phase 4 | W7-W8 | P2 `config/mod.rs` 收尾瘦身 | 配置模块真正完成拆分 |
| Phase 5 | W9-W10 | P2 `room/` 继续细分 | 房间域大文件收敛 |
| Phase 6 | W11+ | P3 兼容模块清理 + P0 lint 由 warn 升级为 deny | 长期治理机制稳定化 |

---

## 九、成功指标（修订版）

| 指标 | 当前复核值 | 目标 |
|------|------------|------|
| `src/` 下生产代码 `unwrap/expect` | **~2 处**（核心路径已清零） | 核心路径保持 0 |
| 测试代码 `unwrap/expect` | ~820 处 | 正常（测试允许） |
| `tests/unit/` 中 DB 依赖文件 | 31 文件 | 0 |
| `tests/unit/` 中 `setup_test_database()` 调用 | 647 次 | 0 |
| `config/mod.rs` 行数 | 1997 | < 500 |
| `room/service.rs` 行数 | 1998 | < 500 |
| `route_ledger` 去重状态 | 已完成 | 完成并固化 |
| 路由层直引存储层 | 0 处 | 保持 0 |
| `DMService` 运行时暴露 | 已移除 | 明确归属后清理或保留 |

---

## 十、风险评估（修订版）

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|----------|
| `unwrap/expect` panic 导致服务中断 | **低**（核心路径已清零） | 高 | clippy lint 门禁防止新增 |
| DB 依赖测试仍伪装为 unit，造成假阳性 | 高 | 高 | 优先重分类并拆 CI 执行矩阵 |
| workspace 镜像模块继续分叉 | 高 | 高 | 先做 canonical crate 决策，再做文件级收口 |
| config 拆分破坏反序列化 | 中 | 高 | 每步拆分后验证 `homeserver.yaml` 加载与默认值行为 |
| room 域继续拆分引入回归 | 中 | 中 | 每次拆分后运行针对性 service/storage 测试 |
| 过早删除兼容模块影响 `test-utils` | 低 | 中 | 先确认调用方，再删除 `DMService` |
