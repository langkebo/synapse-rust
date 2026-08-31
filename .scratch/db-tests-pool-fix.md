# 数据优化 Ticket 09 — db_tests PoolTimedOut 根因定位与修复

## 问题

跑 `cargo test --workspace --features test-utils` 时，synapse-storage 的 57+ 个 db_tests 全部报：

```
Failed to connect to test database: PoolTimedOut
```

30s timeout 后连接池耗尽，所有测试失败。

## 根因

所有 `test_pool()` 函数硬编码了**错误的默认 URL**：

```
postgres://synapse:synapse@localhost:15432/synapse
```

实际情况：
| 参数 | 错误值 | 正确值 |
|------|--------|--------|
| 端口 | `15432` | `5432` |
| 数据库名 | `synapse` | `synapse_test` |

`PgPoolOptions::new().max_connections(2)` 也缺少 `acquire_timeout`，首次建 schema 时极易超时。

## 修复

### 批量替换（63 个文件，2 个 commit）

**Commit 1** `9ee3e89f` — 57 个文件
- `localhost:15432/synapse` → `localhost:5432/synapse_test`
- 添加 `.acquire_timeout(Duration::from_secs(30))`
- 导入 `use std::time::Duration;`

**Commit 2** `4289cc7a` — 6 个遗漏文件
- `filter.rs`, `login_token.rs`, `oidc_user_mapping.rs`
- `qr_login.rs`, `rate_limit.rs`, `room_tag/mod.rs`
- 原因：原始替换只匹配 `localhost:15432/synapse`（无下划线），遗漏了 `localhost:15432/synapse_test`（有下划线）

### 教训

**替换时必须精确匹配所有变体**。数据库 URL 有多种形式：
- `localhost:15432/synapse` ← 原来就错
- `localhost:15432/synapse_test` ← 也错但没匹配
- `localhost:5432/synapse` ← 需要改端口
- `localhost:5432/synapse_test` ← 正确

## 验证

```bash
# 单个测试：成功，0.03s
cargo test -p synapse-storage --lib --features test-utils \
  filter::db_tests::create_filter_then_get
→ ok. 1 passed; 0 failed

# Unit tests：1773 passed ✅
cargo test --workspace --features test-utils --test unit

# Workspace：full run in progress（NmQjnc）
```

## 提交

- `9ee3e89f` fix(test): 解决 synapse-storage db_tests PoolTimedOut 问题（57 files, +316 -122）
- `4289cc7a` fix(test): 修复剩余 6 个 db_tests 的 localhost:15432/synapse_test URL
