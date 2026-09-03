# 09: cross_signing_keys delete 包装事务（P2-5）

**What to build:** 修改 `synapse-federation/src/cross_signing/storage.rs` 的 `delete_cross_signing_keys()` 函数（行 398-415），将两条 `DELETE`（`cross_signing_keys` + `device_signatures`）包装在同一事务中，确保任一失败时整体回滚。

**Blocked by:** None（无依赖，立即可开始）

**Status:** ✅ done（cargo build -p synapse-e2ee ✅，cross_signing 5/5 integration tests ✅）

- [x] 函数添加 `let mut tx = self.pool.begin().await?`
- [x] 两条 DELETE 都用 `execute(&mut *tx).await?` 执行（共享同一事务）
- [x] 任一失败时 `.await?` 传播错误，事务自动回滚
- [x] `tx.commit().await?` 在末尾提交事务
- [x] cargo build --locked 通过
- [x] cross_signing 集成测试 5/5 ✅（test_e2ee_cross_signing_flow 等）

## 修改文件

| 文件 | 改动 |
|------|------|
| `synapse-e2ee/src/cross_signing/storage.rs:397-425` | `delete_cross_signing_keys` 添加事务包装（begin → DELETE keys → DELETE signatures → commit） |