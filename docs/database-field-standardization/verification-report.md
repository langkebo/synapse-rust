# 字段标准化验证报告

## 验证范围

- 数据完整性检查
- 迁移链路可执行性检查
- 业务接口回归检查
- 查询性能影响抽样检查

## 数据完整性

- `application_service_state/users/events` 的 `as_id` 空值计数为 0
- `appservice_id` 与 `as_id` 已完成回填并保持一致
- 外键补充后，命名空间与统计类表可追踪到 `application_services`

## 迁移链路

- `db_migrate.sh migrate` 已完成执行
- `db_migrate.sh status` 可在不同 `schema_migrations` 结构下正常输出
- 历史迁移 `20260302000003` 已支持多结构迁移记录方式

## 业务回归

- 4.22 应用服务 API 回归通过：21/21
- 报告文件：`/home/tzd/api-test/reports/regress_4.22_after_field_audit_opt2.json`

## 性能抽样

- `application_service_events` 按 `as_id` 查询执行时间约 0.118ms
- `application_service_state` 按 `as_id` 查询执行时间约 0.020ms
- `application_service_users` 按 `as_id` 查询执行时间约 0.025ms

## 结论

- 本轮已完成审计体系、标准规范、映射清单和兼容性重构一期落地
- 关键业务链路已验证可用
- 剩余全库字段统一项已纳入后续分阶段治理
