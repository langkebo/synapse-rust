# 2026-09-06 B-tickets 执行总结

## 完成概览

| Commit | Ticket | 类型 | 改动 |
|--------|--------|------|------|
| `c60f388f` | B-1.1 | feat | admin 踢人并发化 + 分页游标 + 修复 `let _` 吞错 |
| `0e0f0b70` | B-2.2 | feat | EventNotifier `idle_timeout_secs` 配置化 |
| `9344544a` | B-1.4 | feat | friend fan-out 双层循环 → 单次 batch SQL |
| `7d40053f` | B-3.1-a | docs | missing-docs ratchet 度量基础设施 |
| `49245667` | B-3.1-b (step1) | docs | per-crate 真实 missing docs 摸底 |
| `6e0ff637` | — | chore | 历史遗留修改收尾 (Phase 14 / DB-02 / SSO / docker) |

## 关键技术结论

### B-1.4 核心澄清
- **ticket 原文**："外层 link 串行循环需要改并发"
- **真实瓶颈**：N-1 循环中每次调用 `get_room_ids_for_user` 是独立 DB 往返
- **borrow checker 约束**：`&self` 持有时无法 `Arc::clone` 出多个可变形借用
- **修复路径**：双层循环降为单次 `room_id = ANY($1)` batch SQL 查询
- **启示**：Rust 优化类 ticket 需先"看一层"确认真实热点再定方案

### B-3.1-b 规模重估
- **Ticket 估计**：80-150 missing docs
- **实测**：14,148 missing docs（超 94 倍）
- **分解**：
  - `synapse-storage` 5,587 | `synapse-services` 3,358 | `synapse-rust` 2,392
  - `synapse-e2ee` 1,151 | `synapse-common` 1,007 | `federation` 331 | `cache` 322
- **工期**：按 1-2 分钟/项 ≈ 236-473 小时 ≈ 30-60 人天
- **建议**：拆为 5 个子 ticket 团队并行

## Ratchet 基础设施

`scripts/quality/check_missing_docs_ratchet.sh`：
- 默认 measure `--workspace --all-features`
- 支持 `--lib`/`-p <crate>` 本地增量
- 三个分支：regression (exit 1) / no-change / progress (auto-update baseline)
- 初始 baseline = 0（allow 模式）

`scripts/quality/audit_per_crate_missing_docs.sh`：
- 临时切换 allow→warn，测真实 missing 数
- 回退，保持 lint 等级不变

## 待处理问题

| 优先级 | 问题 | 来源 | 备注 |
|--------|------|------|------|
| P2 | 7× `unused variable: user_store` (moderation.rs) | commit `1f5f38c3` Phase 14 | 每人加 `_` 前缀 |
| P3 | `rust-optimization-workaround` skill 已创建 | 本次 session | 捕获 borrow-checker 绕过工作流 |

## 环境状态

- 分支：`feat/msc4204-password-logout-devices`
- Working tree：干净（无 staged/modified 文件）
- 编译：`cargo check --workspace --locked` ✓
- Clippy：`cargo clippy -D warnings` ⚠️ 7 pre-existing unused variable
- 未跟踪文件：`.scratch/` 下的多个审计/ticket 文档（无需 commit）
