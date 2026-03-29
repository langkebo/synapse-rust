# incremental

该目录用于存放按时间戳命名的常规增量迁移脚本：

- 命名：`YYYYMMDDHHMMSS_description.sql`
- 约束：每个脚本应配套 `../rollback/` 下同名回滚脚本或明确不可逆

当前仓库仍以 `migrations/` 根目录为 `sqlx migrate run` 的默认入口；目录治理按批次渐进推进。
