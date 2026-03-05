# 数据库重构优先级（第二轮）

## P0（立即）
- 应用迁移 `20260304000001_align_database_architecture.sql`
- 应用迁移 `20260304000002_fix_medium_priority_schema_gaps.sql`
- 回归验证：
  - `/_synapse/admin/v1/application_services`
  - `/_synapse/admin/v1/background_updates/cleanup_locks`
  - `/rooms/{room_id}/summary/members`
  - `/rooms/{room_id}/summary/stats`
  - `/_synapse/admin/v1/registration_tokens`

## P1（短期）
- 对 `room_summaries` 历史字段做收敛（`joined_members/invited_members` 与新字段一致性）
- 对 `rooms.created_ts/creation_ts` 制定单字段收敛窗口与兼容下线策略
- 为 `application_service_*` 子表增加查询热点索引复核

## P2（中期）
- 建立“迁移后自动校验脚本”：
  - 缺表检测
  - 缺列检测
  - 关键索引检测
- 将关键管理端点纳入数据库契约测试，避免回归
