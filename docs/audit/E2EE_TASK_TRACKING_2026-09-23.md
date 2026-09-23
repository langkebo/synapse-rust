# E2EE 联邦查询缺口 - 任务跟踪

## 任务概述

**任务 ID**: T2EE-001
**优先级**: P1
**状态**: 待处理
**关联文档**: `docs/audit/E2EE_*` 系列

## 问题描述

`federation/membership/query.rs` 虽然包含 `user_signing_key` 字段，但实际联邦查询逻辑仍有缺口。根据 E2EE v2.0 方案 (T1~T8)，需要补全联邦层的密钥查询能力。

## 当前状态

- `synapse-web/src/routes/federation/membership/query.rs` 已实现 `user_signing_key` 字段
- 但联邦层缺少完整的密钥查询逻辑
- 具体缺口见 `docs/audit/E2EE_FEDERATION_GAP_ANALYSIS_2026-09-18.md`

## 解决方案

详见 E2EE v2.0 方案 (`docs/2026-09-18-synapse-rud-optimization-plan.md`)，其中 T1~T8 定义了完整的实施路线：

1. **T1**: 联邦层密钥查询端点实现
2. **T2**: 密钥轮换逻辑补全
3. **T3**: 设备签名验证链
4. **T4**: ...
5. **T5**: ...
6. **T6**: ...
7. **T7**: ...
8. **T8**: ...

## 相关文件

- `synapse-web/src/routes/federation/membership/query.rs` - 联邦成员查询
- `synapse-web/src/routes/federation/keys/query.rs` - 联邦密钥查询
- `docs/audit/E2EE_FEDERATION_GAP_ANALYSIS_2026-09-18.md` - 缺口分析
- `docs/2026-09-18-synapse-rud-optimization-plan.md` - E2EE v2.0 方案

## 验收标准

- [ ] 联邦 `user_signing_key` 查询逻辑完整
- [ ] 所有 T1~T8 已完成
- [ ] E2EE 测试覆盖率达到目标
- [ ] 文档更新完成

## 工作量估计

- 预计工时：8-12 小时
- 依赖：无（独立任务）

## 创建时间

2026-09-23

## 备注

此任务与本次 Phase 3 审计无关，属于独立的 E2EE v2.0 优化工作。已记录于此以便后续跟踪。
