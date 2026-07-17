# M-3 / Round-2 Issue Tracker — Dead code, schema drift, test hygiene

> Scope: items surfaced during M-3 Batch 1 (sqlx macro migration)
> and Round-2 (三项目协同优化) that are intentionally **not blocking**
> the mainline but need dedicated follow-up.
>
> Origin: M3_BATCH1_EXECUTION_PLAN.md §13.6 / 三项目协同优化方案.md
>
> Last update: 2026-07-17

## Issue index

### M-3 Batch 1（sqlx 迁移副产物）

| ID | Title | Severity | Discovered in | Owner | Status |
|----|-------|----------|---------------|-------|--------|
| [M3-ISSUE-1](./M3-ISSUE-1-orphan-module-audit.md) | 全仓孤儿模块审计 | 中 | phase A | unassigned | 🟡 open |
| [M3-ISSUE-2](./M3-ISSUE-2-federation-blacklist-drift.md) | federation_blacklist 7 个 schema-drift 查询 | 中 | phase C | unassigned | 🟡 open |
| [M3-ISSUE-3](./M3-ISSUE-3-e2ee-nullable-drift.md) | E2EE 多表 nullable 性审计 | 中 | phase D | unassigned | 🟡 open |
| [M3-ISSUE-4](./M3-ISSUE-4-media-link-signer-drift.md) | media_service::link_signer 字段缺失 | 低 | phase B-Round 3 | unassigned | 🟢 open |

### Round-2（三项目协同优化 阶段 A+B+C 验收副产物）

| ID | Title | Severity | Discovered in | Owner | Status |
|----|-------|----------|---------------|-------|--------|
| [ROUND2-ISSUE-1](./ROUND2-ISSUE-1-clippy-tests-unwrap-explosion.md) | 测试代码 clippy unwrap/expect 爆炸（3416+ errors） | 中 | 阶段 B 验收 | unassigned | 🟡 open |
| [ROUND2-ISSUE-2](./ROUND2-ISSUE-2-test-config-env-race.md) | test_config 环境变量并行竞争（flaky test） | 中 | 阶段 A+B+C 验收 | unassigned | 🟡 open |

## Why these are not in the M-3 mainline

- M-3 Batch 1's contract is to migrate high-sensitivity SQL to
  compile-time macros and add `.sqlx/` cache coverage. It is not a
  schema-cleanup or dead-code-removal pass.
- The 4 items above all have the same shape: a mismatch between the
  public Rust model and either (a) the database schema, or (b) the
  module's reachability from `pub mod` declarations. Resolving them
  requires design decisions (Option vs NOT NULL, keep-vs-delete) and
  the right place is a separate tracking wave.
- The user explicitly asked for the dead-code work to be tracked as
  standalone issues and not to block M-3. This file is the answer.

## Out of scope for M-3 (re-confirmed)

| Decision | Rationale |
|----------|-----------|
| Keep all 4 issues out of M-3 Batch 1 | Each needs its own design + test cycle |
| Do not auto-fix in follow-up commits | Tracked items must be picked up by a dedicated wave |
| Do not delete `ssss` schema-drift fix from M-3 phase D | That fix **prevents** an immediate runtime failure and is part of phase D, not a future item |
