# Phase B Remediation — OPT-029~OPT-033 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**目标:** 基于 Phase B（布尔列 `is_` 前缀命名规范）审核发现及 AUDIT-2026-07 硬约束，修复剩余的列名不一致、桥接代码清理、schema_migrations 表名 bug 及禁用字段名违规。

**架构:** TDD Red-Green-Refactor，逐表原子迁移（含回滚脚本），Rust struct/sqlx query 同步更新，`.sqlx/` 离线缓存重生成。

**技术栈:** Rust + sqlx + PostgreSQL v10 schema + Axum web framework

## 全局约束

- 时间戳毫秒 BIGINT（JWT exp/iat 例外为秒）
- 禁用字段名：`invalidated`/`created_at`/`updated_at`/`expires_ts`/`revoked_ts`/`enabled`
- 三层边界：service 不建裸 SQL，handler 不取 `State<AppState>`
- 错误统一 `ApiError`，禁 `anyhow`/`Box<dyn Error>`
- `SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings` 必须零告警
- 安全失败必须 fail-closed（硬约束 1）
- 声明"绿"前 fmt/clippy/cargo test --no-run 必须各在 `--all-features` 下跑一遍（硬约束 2）
- 改动进程级全局状态的测试必须模块级 Mutex 串行化（硬约束 3）
- 共享池/destructive 测试需隔离 schema 或 SerialGuard（硬约束 4）
- 联邦端点对"存在但无权"与"不存在"统一返 404（硬约束 5）
- 每个 DB 列重命名迁移必须附带 `.undo.sql` 回滚脚本
- 迁移命名格式：`YYYYMMDDHHMMSS_descriptive_name.sql`

---

## 任务索引

| ID | 优先级 | 发现来源 | 描述 | 风险 | 预估时间 |
|----|--------|----------|------|------|---------|
| OPT-029 | P0 | B-phase §schema_migrations | 修复 `ensure_schema_migrations_table()` CREATE TABLE 列名 `success`→`is_success` | 高（运行时 DB 初始化崩溃） | 5min |
| OPT-030 | P0 | B-phase §sliding_sync | 迁移 `sliding_sync_rooms.invited`→`is_invited` + 移除 `#[sqlx(rename)]` 桥接 | 中（列名桥接在运行时脆弱） | 5min |
| OPT-031 | P1 | B-phase §schema_migrations | 修复剩余迁移脚本中 `schema_migrations` 表的 `success`→`is_success` INSERT 引用 | 中（与 OPT-029 表定义不一致） | 3min |
| OPT-032 | P1 | AUDIT 03 §6 | 审计并重命名 `url_preview_cache.expires_ts`→`expires_at`（禁用字段名 `expires_ts`） | 中 | 5min |
| OPT-033 | P2 | B-phase 扩展 | 重命名 `key_rotation_history.revoked`→`is_revoked`（布尔列缺 `is_` 前缀，当前无 Rust 查询引用） | 低 | 3min |

---

### Task 1 (OPT-029): 修复 `ensure_schema_migrations_table` — 列名 `success`→`is_success`

**审核发现:** Phase B audit（`is_` 前缀规范）发现 `schema_migrations` 表的 `success BOOLEAN` 列未使用 `is_success`。更严重的是，Rust 代码 `is_migration_executed()` 和 `record_migration()` 查询 `is_success` 列，但 `ensure_schema_migrations_table()` CREATE TABLE 创建的是 `success` 列。在启用 `SYNAPSE_ENABLE_RUNTIME_DB_INIT` 的新数据库上会导致运行时 SQL 错误：`column "is_success" does not exist`。

**受影响的代码:**
- Modify: `synapse-services/src/database_initializer/mod.rs:345`（CREATE TABLE 列名）
- Verify: `synapse-services/src/database_initializer/mod.rs:367`（SELECT is_success — 无需改动）
- Verify: `synapse-services/src/database_initializer/mod.rs:384`（INSERT is_success — 无需改动）

**接口契约:**
- `ensure_schema_migrations_table()` 创建 `schema_migrations` 表，列 `is_success BOOLEAN NOT NULL DEFAULT TRUE`
- `is_migration_executed(version)` → `Result<bool>` 查询 `SELECT is_success FROM schema_migrations WHERE version = $1`
- `record_migration(...)` → `Result<()>` 写入 `INSERT INTO schema_migrations (..., is_success) VALUES (...)`

**Red 测试代码:**

```rust
// synapse-services/src/database_initializer/mod.rs — #[cfg(test)] module

#[tokio::test]
async fn test_schema_migrations_table_has_is_success_column() {
    // Setup: create an isolated test schema
    let config = TestDbConfig::new("opt029_test");
    let pool = config.create_pool().await;

    // Create the table via the actual function under test
    let init = DatabaseInitializer::new(pool.clone());
    init.ensure_schema_migrations_table().await.expect("create table");

    // Verify: INSERT and SELECT using is_success (the name Rust queries use)
    sqlx::query(
        "INSERT INTO schema_migrations (version, is_success) VALUES ($1, $2)"
    )
    .bind("test_version")
    .bind(true)
    .execute(&*pool)
    .await
    .expect("INSERT with is_success should work");

    let (success,): (bool,) = sqlx::query_as(
        "SELECT is_success FROM schema_migrations WHERE version = $1"
    )
    .bind("test_version")
    .fetch_one(&*pool)
    .await
    .expect("SELECT with is_success should work");

    assert!(success, "is_success should be true");
}
```

**Green 实现要点:**

1. 将 `ensure_schema_migrations_table()` 第 345 行的 `success BOOLEAN NOT NULL DEFAULT TRUE` 改为 `is_success BOOLEAN NOT NULL DEFAULT TRUE`
2. 唯一的代码变更（1 行，1 个单词）

**验证命令:**

```bash
SQLX_OFFLINE=true cargo test --all-features -- --exact --nocapture \
  test_schema_migrations_table_has_is_success_column
  
SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings
cargo fmt --all -- --check
cargo test --all-features --no-run
```

---

### Task 2 (OPT-030): 迁移 `sliding_sync_rooms.invited`→`is_invited` + 移除 `#[sqlx(rename)]` 桥接

**审核发现:** Phase B audit 扩展发现 `sliding_sync_rooms.invited`（布尔列）未重命名为 `is_invited`。Rust 代码使用 `is_invited` 字段名但通过 `#[sqlx(rename = "invited")]` 桥接到旧列名。这违背了 `is_` 命名规范，且桥接代码是脆弱的技术负债。

**受影响的代码:**
- Create: `docker/deploy/migrations/20260713000001_rename_sliding_sync_invited_is_prefix.sql`
- Create: `docker/deploy/migrations/20260713000001_rename_sliding_sync_invited_is_prefix.undo.sql`
- Modify: `synapse-storage/src/sliding_sync/models.rs:47-49`（移除 `#[sqlx(rename = "invited")]` 和 `#[serde(rename = "invited")]`）
- Modify: `synapse-storage/src/sliding_sync/models.rs:84-86`（第二个 struct，同样处理）
- Modify: `synapse-storage/src/sliding_sync/repository.rs:208,218,237,254,316,428,578,631`（SQL 列名 `invited`→`is_invited`）

**接口契约:**
- `SlidingSyncRoom.is_invited: bool` — 字段名保持不变，仅移除 rename 属性
- `SlidingSyncRoomStripped.is_invited: bool` — 同上
- SQL 查询中将 `invited` 列引用替换为 `is_invited`

**Red 测试代码:**

```rust
// synapse-storage/src/sliding_sync/tests.rs — 新增测试

#[tokio::test]
async fn test_sliding_sync_room_is_invited_column() {
    let pool = test_pool("opt030_test").await;
    let repo = SlidingSyncRepository::new(&pool);

    // Insert a room with is_invited = true
    sqlx::query(
        "INSERT INTO sliding_sync_rooms (user_id, device_id, room_id, conn_id, \
         list_key, bump_stamp, is_dm, is_encrypted, is_tombstoned, is_invited) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
    )
    .bind("user") .bind("device") .bind("!room:localhost") .bind("conn")
    .bind("key") .bind(100i64) .bind(false) .bind(false) .bind(false) .bind(true)
    .execute(&*pool).await.expect("INSERT with is_invited");

    let row = sqlx::query_as::<_, SlidingSyncRoom>(
        "SELECT id, user_id, device_id, room_id, conn_id, list_key, bump_stamp, \
         highlight_count, notification_count, is_dm, is_encrypted, is_tombstoned, \
         is_invited, name, avatar, timestamp, created_ts, updated_ts \
         FROM sliding_sync_rooms WHERE room_id = $1"
    )
    .bind("!room:localhost")
    .fetch_one(&*pool).await.expect("SELECT with is_invited");

    assert!(row.is_invited, "is_invited should be true");
}
```

**Green 实现要点:**

1. 创建正向迁移：
```sql
-- docker/deploy/migrations/20260713000001_rename_sliding_sync_invited_is_prefix.sql
DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'sliding_sync_rooms'
          AND column_name = 'invited'
    ) THEN
        ALTER TABLE sliding_sync_rooms RENAME COLUMN invited TO is_invited;
    END IF;
END $$;
```

2. 创建回滚迁移（`.undo.sql`）

3. 更新 `models.rs`：移除两个 struct 中的 `#[serde(rename = "invited")]` 和 `#[sqlx(rename = "invited")]`

4. 更新 `repository.rs`：将所有 SQL 查询中的 `invited` 列引用替换为 `is_invited`（共约 8 处，在 SELECT/INSERT/UPDATE 语句中）

**验证命令:**

```bash
# 验证迁移可接受（在测试 DB 上）
psql -h localhost -p 15432 -U synapse -d synapse \
  -f docker/deploy/migrations/20260713000001_rename_sliding_sync_invited_is_prefix.sql

# 编译和测试
SQLX_OFFLINE=true cargo test -p synapse-storage -- sliding_sync -- --nocapture

# 三重门
cargo fmt --all -- --check
SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings
cargo test --all-features --no-run
```

---

### Task 3 (OPT-031): 修复迁移脚本中 `schema_migrations` 的 `success`→`is_success` INSERT 引用

**审核发现:** 现有的迁移 SQL 脚本在向 `schema_migrations` 表插入记录时使用列名 `success`，但在 OPT-029 修复后该列名将变为 `is_success`。需审计所有迁移文件并更新引用。

**受影响的代码:**
- Scan: `docker/deploy/migrations/` 下所有 `.sql` 文件（`INSERT INTO schema_migrations ... success`）
- Modify: 所有引用 `success` 列的迁移 INSERT 语句

**接口契约:**
- 迁移记录 INSERT 格式：`INSERT INTO schema_migrations (version, name, success, ...)` → `INSERT INTO schema_migrations (version, name, is_success, ...)`

**实现要点:**

1. 扫描所有迁移文件中 `INSERT INTO schema_migrations` 语句内使用 `success` 列名的行
2. 将 `success` 替换为 `is_success`（仅限 `schema_migrations` 的 INSERT 列清单中）
3. 当前数据库（通过迁移系统创建）已有 `success` 列——OPT-029 修复仅影响新数据库的运行时初始化路径。现有迁移文件的 INSERT 语句中列名需要与表定义一致

**扫描命令:**
```bash
grep -rn "INSERT INTO schema_migrations" docker/deploy/migrations/ \
  --include="*.sql" | grep -v "is_success" | grep "success"
```

**验证命令:**
```bash
# 确认零匹配（所有迁移 INSERT 已使用 is_success）
! grep -rn "INSERT INTO schema_migrations" docker/deploy/migrations/ \
  --include="*.sql" | grep -v "is_success" | grep -v ".undo.sql" | grep "success"

cargo test --all-features --no-run
SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings
```

---

### Task 4 (OPT-032): 重命名 `url_preview_cache.expires_ts`→`expires_at`（禁用字段名修复）

**审核发现:** AUDIT 03 §6 P2 — `url_preview_cache.expires_ts` 使用禁用字段名 `expires_ts`，应改为 `expires_at`（原 OPT-020 任务标记完成但迁移仅覆盖了 9 个其他表，未包含 `url_preview_cache`）。Rust 代码仍使用 `expires_ts` 作为 struct 字段名和 SQL 列名。

**受影响的代码:**
- Create: `docker/deploy/migrations/20260713000002_rename_url_preview_expires.sql`
- Create: `docker/deploy/migrations/20260713000002_rename_url_preview_expires.undo.sql`
- Modify: `synapse-storage/src/url_preview_storage.rs:17`（`expires_ts: i64`→`expires_at: i64`）
- Modify: `synapse-storage/src/url_preview_storage.rs:43,45,60,73,86,96,135,152,177,189,191,218,230,252,269,298,302,339,355,386,395-397,438,458`（所有 SQL 查询和测试中的 `expires_ts`）

**接口契约:**
- `UrlPreviewCache.expires_at: i64` — 字段类型不变，仅改名
- SQL 查询：`expires_ts`→`expires_at` 在 SELECT/INSERT/UPDATE/WHERE 子句中
- 索引（若存在）`idx_url_preview_cache_expires` → 重建为 `idx_url_preview_cache_expires_at`

**Red 测试代码:**

```rust
#[tokio::test]
async fn test_url_preview_cache_uses_expires_at_column() {
    let pool = test_pool("opt032_test").await;
    let storage = UrlPreviewStorage::new(&pool);

    let preview = UrlPreviewCache {
        url: "https://test.example.com".into(),
        created_ts: 1700000000000,
        expires_at: 1700003600000,  // 使用新字段名
        ..Default::default()
    };

    storage.save_preview(&preview).await.expect("save with expires_at");

    let found = storage.get_cached_preview(
        "https://test.example.com", 1700001800000
    ).await.expect("get").expect("found");

    assert_eq!(found.expires_at, 1700003600000);
}
```

**Green 实现要点:**

1. 创建正向迁移：
```sql
-- docker/deploy/migrations/20260713000002_rename_url_preview_expires.sql
DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'url_preview_cache'
          AND column_name = 'expires_ts'
    ) THEN
        ALTER TABLE url_preview_cache RENAME COLUMN expires_ts TO expires_at;
    END IF;
END $$;
```

2. 若存在索引，删除并重建：
```sql
DROP INDEX IF EXISTS idx_url_preview_cache_expires;
CREATE INDEX IF NOT EXISTS idx_url_preview_cache_expires_at
    ON url_preview_cache(expires_at) WHERE expires_at IS NOT NULL;
```

3. Rust struct：`pub expires_ts: i64` → `pub expires_at: i64`
4. 所有 SQL 查询字符串中 `expires_ts`→`expires_at`
5. 所有测试代码中 `.expires_ts`→`.expires_at`

**验证命令:**

```bash
SQLX_OFFLINE=true cargo test -p synapse-storage -- url_preview -- --nocapture
cargo fmt --all -- --check
SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings
cargo test --all-features --no-run
```

---

### Task 5 (OPT-033): 重命名 `key_rotation_history.revoked`→`is_revoked`

**审核发现:** Phase B 扩展审计 — `key_rotation_history.revoked BOOLEAN DEFAULT FALSE`（统一 schema v7）未使用 `is_` 前缀。当前无 Rust 代码直接查询此列，因此风险最低。

**受影响的代码:**
- Create: `docker/deploy/migrations/20260713000003_rename_key_rotation_revoked_is_prefix.sql`
- Create: `docker/deploy/migrations/20260713000003_rename_key_rotation_revoked_is_prefix.undo.sql`
- Verify: `synapse-storage/src/key_rotation.rs` 和 `synapse-federation/src/key_rotation.rs`（确认无 `revoked` 列的直接 SQL 引用）

**Green 实现要点:**

1. 创建正向迁移：
```sql
DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'key_rotation_history'
          AND column_name = 'revoked'
    ) THEN
        ALTER TABLE key_rotation_history RENAME COLUMN revoked TO is_revoked;
    END IF;
END $$;
```

2. 创建回滚迁移
3. 验证 Rust 代码中无 `key_rotation_history` 的 SELECT/INSERT 查询包含 `revoked`
4. 不存在需更新的 Rust 代码（迁移仅限 SQL）

**验证命令:**

```bash
# 确认无 Rust 代码引用旧列名
! grep -rn "key_rotation_history" --include="*.rs" synapse-storage/src/ synapse-federation/src/ | grep "revoked" | grep -v "is_revoked"

# 验证迁移可执行
psql -h localhost -p 15432 -U synapse -d synapse \
  -f docker/deploy/migrations/20260713000003_rename_key_rotation_revoked_is_prefix.sql

cargo test --all-features --no-run
SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings
```

---

## 依赖关系

```
OPT-029 (schema_migrations 表修复)
  |
  +---> OPT-031 (迁移脚本 INSERT 列名同步，依赖 OPT-029 先完成)
  
OPT-030 (sliding_sync_rooms.invited) — 独立，无依赖

OPT-032 (url_preview_cache.expires_ts) — 独立，无依赖

OPT-033 (key_rotation_history.revoked) — 独立，无依赖
```

## 建议执行顺序

1. **OPT-029** → OPT-031（先修表定义，再对齐迁移脚本）
2. **OPT-030**（清理最明显的桥接代码）
3. **OPT-032**（修复禁用字段名）
4. **OPT-033**（低风险迁移，最后做）
