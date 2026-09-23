# E2EE 联邦查询缺口 - 任务跟踪

## 任务概述

**任务 ID**: T2EE-001
**优先级**: P1
**状态**: 完成 ✅
**关联文档**: `docs/audit/E2EE_*` 系列

## 问题描述

联邦 `/federation/v1/user/keys/query` 端点未返回跨签名密钥字段 (`master_keys`, `self_signing_keys`, `user_signing_keys`)。虽然后端 `query_keys_internal()` 已正确计算并填充这些字段，但 HTTP 响应中未包含。

## 根因分析

- `KeyQueryResponse` 结构体已包含 `master_keys`, `self_signing_keys`, `user_signing_keys` 字段
- `query_keys_internal()` 已正确计算这些字段
- `keys_query()` 响应只返回 `device_keys` 和 `failures`

## 解决方案

修改 `keys_query()` 函数，在响应中添加跨签名密钥字段。

### 已完成工作

- [x] T1: 联邦层密钥查询端点实现 ✅
  - 修改 `synapse-web/src/routes/federation/keys.rs:243-254`
  - 添加 `master_keys`, `self_signing_keys`, `user_signing_keys` 字段到响应
- [x] T2: 添加 T2 合规测试 ✅
  - `test_federation_keys_query_routes_from_real_ledger()` 从 `declared_ledger_all()` 读取
- [x] 运行 CI 门禁 ✅
  - 指标仪表盘门禁通过
  - 格式检查通过
  - 编译通过

### 后续待办

- [ ] T2-T8: 其余优化工作（密钥轮换、签名验证链等）
  - 由 E2EE v2.0 方案 `docs/2026-09-18-synapse-rud-optimization-plan.md` 驱动

## 变更文件

| 文件 | 变更 | 行数 |
|------|------|------|
| `synapse-web/src/routes/federation/keys.rs` | 修改 `keys_query()` 响应，添加测试 | ~35 行 |

## 验收标准

- [x] 联邦密钥查询返回完整的跨签名密钥
- [ ] 所有 T1~T8 已完成
- [ ] E2EE 测试覆盖率达到目标
- [ ] 文档更新完成

## 提交记录

- commit d6c55ed8: "T2EE-001: Add cross-signing keys to federation keys query response"

## 创建时间

2026-09-23
