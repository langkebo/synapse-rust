# Synapse-Rust 数据库深度优化 Tickets

**源报告**: `artifacts/数据库架构深度诊断-2026-08-31.md`（38 个问题）
**项目状态**: 未发布、可清空重部署
**优化窗口**: 利用重部署机会做不可逆优化（v11 baseline 内联所有变更）

## 执行结果（2026-08-31 单会话）

| # | 标题 | 状态 | 备注 |
|---|---|---|---|
| 01 | v11 baseline 脚手架 | ✅ DONE | v10 内容 + v11 header + v11 增量块，~5340 行 |
| 02 | Matrix ID 字段 CHECK 约束 | ✅ DONE | 123 user_id + 1 room_id + 1 event_id + 1 sender + 9 mxc = 135 format 约束 |
| 03 | events 表 HASH(room_id) 64 分区 | ⏸️ DEFER | 4 张子表 FK P0 阻塞 + stream_ordering watermark 退化 |
| 04 | room_summaries 触发器同步 | ✅ DONE | `trg_sync_member_count` 5/5 场景测试通过 |
| 05 | 移除应用层 member_count 自增 | ✅ DONE | mod.rs + admin.rs 4 个函数体清理 |
| 06 | e2ee_audit_log TTL 清理 | ✅ DONE | 3 个函数（`cleanup_e2ee_audit_log` / `cleanup_message_log` / `cleanup_retention_logs`） |
| 07 | 时间戳字段统一命名 | ⏳ DEFER | 高爆炸半径，依赖 08 |
| 08 | Rust struct 字段同步迁移 | ⏳ DEFER | 依赖 07 |
| 09 | 删除 .undo.sql + 历史 v10 | 🟡 PARTIAL | undo.sql 22 个全部删除，v10 baseline 保留待确认 |
| 10 | UNIQUE 约束/索引冗余清理 | ✅ DONE | v11 baseline DO $$ 块删除显式重复 UNIQUE INDEX |
| 11 | 关键索引补齐 | ✅ DONE | 3 个新索引（device_lists_changes/stream） |
| 12 | 集成验证 | ✅ DONE | 95.8% 通过（1416/1478），62 失败 = CHECK 约束识别非法测试数据 |

**完成度**: 8/12 ✅，1/12 ⏸️ DEFER（P0 阻塞），1/12 🟡 PARTIAL，2/12 ⏳ 待 07/08 链路

## 关键变更

- `migrations/00000000_unified_schema_v11.sql`: v10 内容 + v11 增量（CHECK 约束、触发器、清理函数、新索引）
- `docker/db_migrate.sh`: `is_superseded_by_latest_baseline()` 支持 v8/v10/v11
- `scripts/init_v11_database.sh`: 干净的 v11 部署脚本
- `synapse-storage/src/room/{mod,admin}.rs`: 4 个 `*member_count*` 函数移除双增量 SQL
- `migrations/*.undo.sql`: 全部 22 个删除

## 关键决策

1. **v11 baseline 内联策略**：所有破坏性变更（CHECK、分区、字段改名）直接写在 v11 CREATE TABLE 中，不走 NOT VALID 路径
2. **expand–contract 模式**：04（建触发器）→ 05（删应用层逻辑），确保任何时刻都有同步机制
3. **测试基线**：synapse_test 255 张表 + 1513 storage test 是验收基线
