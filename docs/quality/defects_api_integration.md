# 质量缺陷清单（来自 API 集成测试）

## 复现方式（统一）

- 运行：`SERVER_URL=http://localhost:8008 TEST_ENV=dev bash scripts/test/api-integration_test.sh`
- 产物：`test-results/api-integration.failed.txt` / `test-results/api-integration.missing.txt`

## 分级说明

- P0：阻塞/崩溃/500/FAILED
- P1：核心 Matrix Client 路径缺失或 4xx/不兼容
- P2：非核心但重要能力缺失（admin/federation/sso/第三方等）
- P3：优化与增强项（不阻塞主流程）

## P0（0）

- 无

## P1（3）

### P1-API-001: Device List

- 来源：MISSING
- 现象：not found
- 影响范围：device
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P1-API-002: Account Data

- 来源：MISSING
- 现象：endpoint not available
- 影响范围：unknown
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P1-API-003: Account Data

- 来源：MISSING
- 现象：endpoint not available
- 影响范围：unknown
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

## P2（36）

### P2-API-001: OpenID Userinfo

- 来源：MISSING
- 现象：endpoint not available
- 影响范围：sso
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-002: Events

- 来源：MISSING
- 现象：endpoint not available
- 影响范围：unknown
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-003: VoIP TURN Server

- 来源：MISSING
- 现象：endpoint not available
- 影响范围：unknown
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-004: Get Room Alias

- 来源：MISSING
- 现象：endpoint not available
- 影响范围：room
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-005: Federation Backfill

- 来源：MISSING
- 现象：endpoint not available
- 影响范围：federation
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-006: Admin Room Event

- 来源：MISSING
- 现象：not found
- 影响范围：admin
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-007: Space

- 来源：MISSING
- 现象：not found
- 影响范围：space
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-008: Room Key Share

- 来源：MISSING
- 现象：not found
- 影响范围：e2ee
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-009: Room Key Share

- 来源：MISSING
- 现象：not found
- 影响范围：e2ee
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-010: SSO

- 来源：MISSING
- 现象：not found
- 影响范围：sso
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-011: SSO

- 来源：MISSING
- 现象：not found
- 影响范围：sso
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-012: Room Alias Admin

- 来源：MISSING
- 现象：not found
- 影响范围：admin
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-013: Room Invite

- 来源：MISSING
- 现象：not found
- 影响范围：room
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-014: Room Vault

- 来源：MISSING
- 现象：not found
- 影响范围：room
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-015: Room Vault

- 来源：MISSING
- 现象：not found
- 影响范围：room
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-016: Room Retention

- 来源：MISSING
- 现象：not found
- 影响范围：room
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-017: Room Key Backward

- 来源：MISSING
- 现象：not found
- 影响范围：e2ee
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-018: Evict User

- 来源：MISSING
- 现象：not found
- 影响范围：unknown
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-019: Room Search

- 来源：MISSING
- 现象：not found
- 影响范围：room
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-020: Room Global Tags

- 来源：MISSING
- 现象：not found
- 影响范围：room
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-021: Room Redact

- 来源：MISSING
- 现象：not found
- 影响范围：room
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-022: Room External IDs

- 来源：MISSING
- 现象：not found
- 影响范围：room
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-023: Key Forward

- 来源：MISSING
- 现象：not found
- 影响范围：e2ee
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-024: Room Search

- 来源：MISSING
- 现象：not found
- 影响范围：room
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-025: Room Event Perspective

- 来源：MISSING
- 现象：not found
- 影响范围：room
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-026: User Appservice

- 来源：MISSING
- 现象：not found
- 影响范围：unknown
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-027: Room Event Report

- 来源：MISSING
- 现象：not found
- 影响范围：room
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-028: Room Report

- 来源：MISSING
- 现象：not found
- 影响范围：room
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-029: Room Search

- 来源：MISSING
- 现象：not found
- 影响范围：room
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-030: Federation

- 来源：MISSING
- 现象：not found
- 影响范围：federation
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-031: Federation

- 来源：MISSING
- 现象：not found
- 影响范围：federation
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-032: Identity

- 来源：MISSING
- 现象：not found
- 影响范围：unknown
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-033: Identity

- 来源：MISSING
- 现象：not found
- 影响范围：unknown
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-034: Admin Register

- 来源：MISSING
- 现象：not found
- 影响范围：admin
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-035: Room Resolve

- 来源：MISSING
- 现象：not found
- 影响范围：room
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

### P2-API-036: Room Report v3

- 来源：MISSING
- 现象：not found
- 影响范围：room
- 重现步骤：运行集成测试脚本，定位产物中同名条目
- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）
- 修复建议：
  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐
  - 补充回归用例（单元/集成）覆盖成功与失败路径
- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归

## P3（0）

- 无

