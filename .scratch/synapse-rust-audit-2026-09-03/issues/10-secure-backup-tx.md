# 10: secure_backup delete 包装事务（P2-6）

**What to build:** 修改 `synapse-services/src/e2ee/secure_backup/service.rs` 的 `delete_backup()` 函数（行 240-252），将两条 `DELETE`（`session_keys` + `backups`）包装在同一事务中，确保任一失败时整体回滚。

**Blocked by:** None（无依赖，立即可开始）

**Status:** ✅ done（cargo build -p synapse-e2ee ✅，test_e2ee_key_backup_lifecycle ✅）

- [x] 函数添加 `let mut tx = self.pool.begin().await?`
- [x] 两条 DELETE 都用 `execute(&mut *tx).await?` 执行（共享同一事务）
- [x] 任一失败时 `.await?` 传播错误，事务自动回滚
- [x] `tx.commit().await?` 在末尾提交事务
- [x] cargo build --locked 通过
- [x] api_e2ee_advanced_tests::test_e2ee_key_backup_lifecycle 验证（包含 DELETE 后状态）✅

## 修改文件

| 文件 | 改动 |
|------|------|
| `synapse-e2ee/src/secure_backup/service.rs:238-263` | `delete_backup` 添加事务包装（begin → DELETE session_keys → DELETE key_backups → commit） |