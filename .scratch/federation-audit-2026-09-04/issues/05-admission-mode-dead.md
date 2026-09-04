# F-05 💭 Nit — admission_mode dead field

**审计报告**：`/Users/ljf/Desktop/hu_ts/synapse-rust/.scratch/federation-audit-2026-09-04/report.md`

## 问题描述

`synapse-common/src/config/federation.rs:125`：

```rust
pub admission_mode: bool,  // default false
```

该字段在 `federation_auth.rs:179-200` 实际被使用（检查 origin 状态），但功能上仅是简单的"active"/"pending" 状态机，无持久化策略。属于"半实现"状态。

## 风险

无直接风险，但 dead/nearly-dead config 字段增加维护负担。

## 修复

二选一：
- **A. 完整实现** admission_mode：增加持久化的 `federation_admission` 表 + admin 审批 UI
- **B. 删除字段** + 移除 `federation_auth.rs:179-200` 的检查逻辑

## 建议

选 B。当前部署未启用该模式（default false），删除可减少 100+ 行死代码。

## 验证

- 移除后 `cargo build` 零警告
- `cargo test` 全部通过

## 优先级

💭 Nit

## 工作量

0.1d（删除）/ 2-3d（完整实现）
