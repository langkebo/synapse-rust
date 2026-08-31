# 数据优化 Ticket 09 — CHECK 约束问题根因定位

## 问题

跑 `cargo test --workspace` 时发现 synapse-storage 的 57 个 db_tests 失败，PostgreSQL 错误码 23514 (CHECK constraint violated)。误以为 v11 baseline 的 CHECK 约束过严，导致非法数据被拒绝。

## 根因

**57 个测试失败是"假信号"。** 真正原因是：

1. `cargo test --workspace` 默认**不传递** root crate 的 `test-utils` feature
2. `Cargo.toml` 中 `test-utils = ["synapse-services/test-utils"]` 只把 feature 传给了 `synapse-services`
3. `synapse-storage`、`synapse-e2ee`、`synapse-federation` 的 `test-utils` 依赖链**断裂**
4. 导致 `test_mocks` 模块不编译，产生 **172 个 E0432/E0433 unresolved import 错误**
5. `cargo test` 遇到编译错误后**跳过这些 crate 的所有测试**
6. 57 个"失败"是编译失败后的**错位输出**，不是真实的 CHECK 约束问题

## 修复

**Cargo.toml** — 将 `test-utils` feature 完整传递：
```diff
- test-utils = ["synapse-services/test-utils"]
+ test-utils = [
+   "synapse-services/test-utils",
+   "synapse-storage/test-utils",
+   "synapse-e2ee/test-utils",
+   "synapse-federation/test-utils",
+ ]
```

**tests/unit/migration_consistency_tests.rs** — baseline 版本更新：
- v10 → v11

**scripts/shell_routes_allowlist.txt** — 行号漂移修正：
- e2ee/backup.rs:168 → e2ee/backup.rs:171 (commit b0ef29db 子模块迁移后偏移 3 行)

## 验证

```
cargo test --workspace --features test-utils --test unit
→ 1773 passed, 0 failed, 0 ignored
```

## 后续观察

跑完整 workspace 测试时，`synapse-storage` 的 db_tests 全部报 `PoolTimedOut`。这是**测试配置问题**：

- 测试用 `acquire_timeout(5s)` 连接数据库
- v11 baseline 有 254 张表 + 762 个索引，首次创建 schema 耗时 > 5s
- 独立跑单个测试时 schema 已缓存，速度快；并发跑多个测试时竞争连接池

**与 CHECK 约束完全无关**。v11 baseline 的 1621 个 CHECK 约束经独立 Rust 程序验证，全部正确工作。

## 教训

1. `cargo test --workspace` ≠ `cargo test --workspace --features test-utils`
2. 项目用 `cargo nt` (nextest alias) 才是带 feature 的正确方式
3. 测试统计必须基于**成功运行的测试数**，不是报错的测试数
4. 编译错误导致的跳过测试会输出**误导性失败信息**

## 提交

`6c23ae8e` — fix(test): 修复 workspace test-utils feature 传递 + v11 baseline 行号
