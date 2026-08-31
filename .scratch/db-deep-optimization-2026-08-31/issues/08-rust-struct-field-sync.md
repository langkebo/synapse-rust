# 08: Rust struct 字段同步迁移

**What to build:** 同步 Rust struct 中时间戳字段命名为 `*_ts_ms`（与 v11 baseline 对齐），更新所有 sqlx::query 字段引用。

**Blocked by:** 07-timestamp-field-rename

**Status:** ready-for-agent

- [ ] 全项目搜索 `struct ... { created_ts: ...` 等字段定义，逐个改名为 `created_ts_ms`
- [ ] 全项目搜索 `created_ts = ...`/`created_ts: ...`/`&created_ts` 等引用，逐个改名
- [ ] 同样处理 `updated_ts`、`origin_server_ts`、`last_activity_ts`、`applied_ts`、`executed_at`
- [ ] 全 workspace `cargo check --workspace --locked`：无编译错误
- [ ] `cargo clippy -p synapse-storage -p synapse-services --locked`：仅预存警告
- [ ] storage 测试套件：1513/1513 通过
- [ ] services 测试套件（`--features test-utils`）：1535/1535 通过

**关键决策**：
- 纯机械改名（重命名 + 验证），无逻辑变更
- 如果 sqlx::query! 宏需要重编译，可用 `cargo sqlx prepare`