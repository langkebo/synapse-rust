# F-03 🟡 P3 — invite 事件无本端 re-sign 链路

**审计报告**：`/Users/ljf/Desktop/hu_ts/synapse-rust/.scratch/federation-audit-2026-09-04/report.md`

## 问题描述

`membership/invite.rs:96-105` `invite_v2` handler 将 body 字段填入 `CreateEventParams` 后直接 `create_event` 持久化，**未对事件做本端 ed25519 签名**。

类似问题在 `send_join`、`send_join_v2`、`send_leave` 也可能存在（需逐个审查）。

## 风险

如果 invite 事件后续被 federate 给第三方 origin，将缺少本端 `signatures.<local_server_name>.<key_id>` 字段。第三方 origin 在 `verify_pdu_sender_signature` 阶段会失败。

## 修复

在 `invite_v2` / `send_join*` / `send_leave*` 持久化后增加 `sign_and_hash_event` 调用，添加本地 server signature。

参考 `synapse-federation/src/signing.rs:188-218` `sign_and_hash_event` 函数。

## 验证

- 单元测试：invite 事件持久化后检查 `signatures.<local_server_name>.<key_id>` 存在
- 集成测试：跨服务器 invite 流程中，第三方 origin 能成功验签

## 优先级

🟡 P3

## 工作量

0.5d
