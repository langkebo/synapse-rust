# Task 11 - 空壳接口豁免清单

> 说明：仅用于记录“短期必须保留，但暂未完成真实实现”的空壳/占位接口。  
> 原则：只有在端点已经改为明确错误（优先 `M_UNRECOGNIZED`）且不再返回 200 假成功时，才允许登记豁免。  
> 要求：每条豁免都必须带 `owner_role`、`expires_at`、`replacement_plan`，到期后若未清理，应由 CI 阻断继续保留。

## 当前状态

- 当前无已批准的豁免项。
- 若未来出现短期保留的占位端点，必须先更新本文件，再更新 `task11_scan_and_ci_gate.md` 与 `docs/synapse-rust/UNSUPPORTED_ENDPOINTS.md`。

## 字段说明

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `endpoint` | 是 | 对外暴露的 HTTP 方法 + 路径 |
| `current_behavior` | 是 | 当前返回行为，必须说明错误码 |
| `reason` | 是 | 为什么短期内必须保留该路由 |
| `owner_role` | 是 | 负责清理的角色，而不是个人姓名 |
| `expires_at` | 是 | 清理截止日期，格式 `YYYY-MM-DD` |
| `replacement_plan` | 是 | 计划改为真实实现，或改为明确不支持的方式 |
| `tracking_doc` | 否 | 关联的设计文档、任务文档或测试文档 |
| `notes` | 否 | 额外背景说明 |

## 模板

| endpoint | current_behavior | reason | owner_role | expires_at | replacement_plan | tracking_doc | notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `GET /_matrix/client/v3/example` | `400 + M_UNRECOGNIZED` | 客户端探测依赖路由存在 | API Maintainer | `2026-04-30` | 接入真实 `service/storage` 或移除暴露 | `task11_scan_and_ci_gate.md` | 示例模板，提交前删除 |

## 使用规则

1. 若端点仍返回 `200` 假成功，不允许登记豁免，必须先修正为明确错误。
2. 若端点已被确认“项目明确不支持”，应同时登记到 `docs/synapse-rust/UNSUPPORTED_ENDPOINTS.md`。
3. 每次新增、修改或删除豁免项时，必须在相关任务文档中记录变更原因。
4. 过期未清理的豁免项应视为阻断项，不应继续延期而不留痕。
