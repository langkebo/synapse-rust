# M3-ISSUE-1: 全仓孤儿模块审计

**Status**: 🟡 open
**Severity**: 中
**Discovered**: M-3 阶段 A (2026-06-06)
**Origin**: [M3_BATCH1_EXECUTION_PLAN.md §7.2](../../M3_BATCH1_EXECUTION_PLAN.md#72-关键发现5-个-sqlxquery-宏实为死代码)
**Blocks**: 不阻塞 M-3 Batch 1；可能阻塞未来的 sqlx-cli 查询收集完整性

---

## 1. 背景

M-3 阶段 A 审计发现 `cargo sqlx prepare` 报告 `no queries found`，但项目代码内有 5 个 `sqlx::query!` 宏。根因是 2 个父模块（`src/services/mod.rs` 和 `src/cache/mod.rs`）没有 `pub mod` 声明对应子模块，导致 `sqlx-cli` 的查询收集器从 crate public API 入口追踪时**完全不可见**这 2 个文件。

阶段 A 已清理 2 个孤儿模块（`guest_service.rs` 167 行 + `warmup.rs` 393 行），但**未做全仓审计**。本 issue 跟踪全仓 ~400 个 .rs 文件的孤儿模块识别工作。

## 2. 影响

| 影响维度 | 详情 |
|----------|------|
| sqlx-cli 查询收集 | 孤儿模块中的 `query!` / `query_as!` 不会被 `cargo sqlx prepare` 收集 |
| 二进制体积 | 孤儿模块在父模块缺失 `pub mod` 时，**不会进入生产二进制**（无运行时影响） |
| 代码维护 | 孤儿代码仍占用仓库空间、IDE 索引、代码审查心智负担 |
| 测试覆盖 | 孤儿模块的 `#[cfg(test)]` 不被执行 |

## 3. 审计方法

```bash
# Step 1: 获取 crate public API 入口
cargo metadata --format-version 1 | jq '.workspace_members[]'

# Step 2: 对比父模块的 pub mod 列表
for f in src/services/mod.rs src/cache/mod.rs src/storage/mod.rs \
         src/common/mod.rs src/web/mod.rs; do
  echo "=== $f ==="
  grep -E "pub mod " "$f" || echo "(none)"
done

# Step 3: 反向验证可达性
git ls-files src/ | while read f; do
  if [[ "$f" =~ src/([^/]+)/mod\.rs$ ]]; then continue; fi  # 跳过 mod.rs
  parent_module=$(dirname "$f")
  if [[ ! -f "$parent_module/mod.rs" ]]; then continue; fi
  module_name=$(basename "$f" .rs)
  if ! grep -q "pub mod $module_name" "$parent_module/mod.rs"; then
    echo "ORPHAN: $f"
  fi
done
```

## 4. 已知孤儿模块（阶段 A 已清理）

| 文件 | 行数 | 处理 |
|------|------|------|
| `src/services/guest_service.rs` | 167 | ✅ 已删 (2026-06-06) |
| `src/cache/warmup.rs` | 393 | ✅ 已删 (2026-06-06) |

## 5. 待审计范围

- 全仓 ~400 个 .rs 文件
- 重点目录：`src/services/`（90+ 模块） / `src/cache/`（5 模块） / `src/storage/`（~50 文件） / `src/common/`
- 重点关注：包含 `query!` / `query_as!` 宏但父模块未 `pub mod` 注册的文件

## 6. 风险与缓解

| 风险 | 概率 | 影响 | 缓解 |
|------|------|------|------|
| 删除有用的占位代码 | 低 | 中（可能后续需要复活） | 保留 git 历史；如需可 `git revert` |
| 漏判真孤儿 | 中 | 低（不进入二进制） | 通过 `cargo build` 验证；漏判无运行时影响 |
| 误判可达模块 | 低 | 高（破坏功能） | 步骤 3 之后用 `cargo build --bin synapse-rust` 端到端验证 |

## 7. 验收

- [ ] 全仓孤儿模块清单已生成
- [ ] 每个孤儿模块有「保留 / 删除 / 重构」三选一决策
- [ ] `cargo build --bin synapse-rust` 退出码 0（保留决策后）
- [ ] `cargo test --lib` 全绿
- [ ] `cargo sqlx prepare --workspace` 与代码量匹配（孤儿模块删除后 queries 数与代码量同步）

## 8. 工时估计

| 工作量 | 时间 |
|--------|------|
| 审计 + 清单生成 | 0.3 天 |
| 决策 + 清理 | 0.3 天 |
| 验证 + 回归 | 0.2 天 |
| **总计** | **0.8 天** |
