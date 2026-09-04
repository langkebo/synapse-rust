# F-02 🟡 P3 — PDU 数量上限默认 100（spec 推荐 50）

**审计报告**：`/Users/ljf/Desktop/hu_ts/synapse-rust/.scratch/federation-audit-2026-09-04/report.md`

## 问题描述

`src/web/routes/federation/transaction.rs:180` 写死 `MAX_PDUS_PER_TRANSACTION = 100`，但 spec §4.1 推荐 50。

## 当前影响

`max_transaction_payload` 默认 50KB 实际上把 100 个 PDU 限制在 ~500B/PDU 小事件。理论上是 6.4MB body 解析成本风险，但限流中间件 + body size 限制已覆盖。

## 建议修复

1. 移到 `federation.inbound_max_pdus_per_txn` config
2. 默认值改为 50
3. 通过 `once_cell` 或 `&'static` 注入

## 验证

- 单元测试 `test_max_pdu_config_default_50`
- 集成测试：发送 60 个 PDU 的 txn → 拒绝 51+

## 优先级

🟡 P3

## 工作量

0.25d
