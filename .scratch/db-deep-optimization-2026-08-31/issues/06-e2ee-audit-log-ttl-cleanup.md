# 06: e2ee_audit_log TTL 清理策略

**What to build:** 为 `e2ee_audit_log` 和 `message_log` 表实现数据生命周期管理（TTL 清理），防止日志无限增长导致磁盘压力。

**Blocked by:** 01-v11-baseline-scaffold

**Status:** ✅ DONE

- [x] 在 v11 baseline 中定义 GUC 控制 TTL（`synapse.e2ee_audit_log_retention_days` 默认 90 天，`synapse.message_log_retention_days` 默认 30 天）
- [x] 创建 `cleanup_e2ee_audit_log()` 函数（批量 LIMIT 1000 循环避免长事务）
- [x] 创建 `cleanup_message_log()` 函数（DO $$ 条件创建，兼容无此表的实例）
- [x] 创建 `cleanup_retention_logs()` 聚合函数（一次调用清理两张表）
- [x] 验证：直接调用清理函数，表为空时返回 0，无报错
- [x] 验证：v11 baseline 重新应用后函数存在

**修复的问题**：
- 修复 `INTEGER * 86400000` 溢出 → 改为 `v_retention_days::BIGINT * 86400000`
- 修复 `cleanup_retention_logs()` 中 `table_name` 变量名与 RETURNS 列名冲突 → 用 `v_msg_exists BOOLEAN` 中间变量

**注意**：pg_cron 未启用，不在 v11 baseline 中强制调度。可由运维在外部配置 cron 调用 `cleanup_retention_logs()`。