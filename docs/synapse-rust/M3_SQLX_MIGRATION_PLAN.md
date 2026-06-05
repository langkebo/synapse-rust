# M-3 `sqlx::query!` 编译期校验迁移计划

> 起始基线: 2026-06-03 快照
> 现状: 1408 处 `sqlx::query` 调用（100 文件），595 处 `sqlx::query_as`，19 处 `QueryBuilder`；编译期校验调用 0 处
> 目标: 把动态 SQL 调用迁移到 `sqlx::query!` / `query_as!` / `query_scalar!`，CI 强制 `.sqlx/` 入仓
> 关联: [COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md](./COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md) M-3
> 关联: [ROUTE_STORAGE_MIGRATION_PLAN.md](./ROUTE_STORAGE_MIGRATION_PLAN.md)
> 门禁: `scripts/ci/check_sqlx_offline_cache.sh`（新增）

## 一、迁移原则

1. **优先静态**：能写 `query!` / `query_as!` 的，就不要用动态 `query()`。
2. **可读性优先**：宏内 SQL 与结构体字段一一对应，禁止出现拼字符串。
3. **不强求 100%**：动态 `QueryBuilder`、条件分支、复杂 IN 子句等场景可保留动态 SQL，但必须用 `#[sqlx::query(name = "...")]` 命名 query 走迁移文件。
4. **离线缓存为准**：CI 在没有 PostgreSQL 的环境也能编译（`SQLX_OFFLINE=true`）。
5. **缓存必须入仓**：`.sqlx/` 目录随代码一起提交，PR 必须包含 `cargo sqlx prepare` 后的差异。
6. **逐文件验证**：每个文件迁移后跑 `cargo check` + `cargo test --lib` 确认无误。

## 二、范围与数据

### 2.1 当前基线（2026-06-03）

| 维度 | 数量 | 占比 |
|---|---|---|
| 动态 `sqlx::query()` / `query_as()` | 1408 + 595 = 2003 | 99.6% |
| 编译期 `sqlx::query!` 等宏 | 3 + 1 = 4 | 0.4% |
| `QueryBuilder` 动态构建 | 19 | 难迁移 |
| `#[sqlx::query(name=...)]` 命名 query | 0 | 0 |

### 2.2 验收目标

| 指标 | 当前 | 目标 |
|---|---|---|
| 动态 SQL 占比 | 99.6% | ≤ 30% |
| 编译期宏占比 | 0.4% | ≥ 70% |
| `.sqlx/` 缓存完整性 | N/A | 100% 入仓 |
| `cargo sqlx prepare --check` 通过率 | N/A | 100% |

## 三、基础设施（Phase 0）

### 3.1 准备工作

1. 安装 `sqlx-cli`（已具备，0.8.6）。
2. 在 CI 与本地 `.env.example` 中固化 `DATABASE_URL`。
3. 准备本地 PostgreSQL（CI 已在 `db-migration-gate` job 中提供 Postgres 15）。
4. 把所有 2003 处 `sqlx::query` 调用按"文件 + 函数"统计到 `scripts/ci/sqlx_migration_inventory.json`。

### 3.2 离线缓存目录

1. 创建 `.sqlx/` 目录（首次 `cargo sqlx prepare` 自动生成）。
2. 更新 `.gitignore`：允许 `.sqlx/` 入仓（`!/.sqlx` 排除规则修改为提交）。
3. 在 CI 中加入 `cargo sqlx prepare --check` 步骤（在 `db-migration-gate.yml` 之后）。
4. 本地开发：使用 `SQLX_OFFLINE=true` 编译。

### 3.3 新增 CI 门禁脚本

`scripts/ci/check_sqlx_offline_cache.sh`：

```bash
#!/usr/bin/env bash
# 检查 .sqlx/ 缓存与代码的一致性
set -euo pipefail
cd "$(dirname "$0")/../.."

# 1) 必须存在 .sqlx 目录
test -d .sqlx || { echo "ERROR: .sqlx/ 缓存目录不存在"; exit 1; }

# 2) 至少有一个 .json
json_count=$(find .sqlx -name 'query-*.json' | wc -l)
test "$json_count" -gt 0 || { echo "ERROR: .sqlx/ 为空"; exit 1; }
echo "OK: $json_count 个 query 缓存"

# 3) 编译期校验
SQLX_OFFLINE=true cargo check --all-features --locked --quiet

# 4) 禁止新增动态 query（按 PR delta 评估，初始允许存量）
echo "OK: 编译期校验通过"
```

## 四、批次计划

### Batch 1（Phase 1）— 基础结构 + 简单 storage（4 文件，~30 处）

> 目标：建立 `.sqlx/` 缓存 + CI 门禁；让第一批文件 100% 编译期校验。

| 文件 | 动态 query 数 | 备注 |
|---|---|---|
| `src/storage/audit.rs` | 5 | 含 `QueryBuilder`，需拆分为多个 `query!` |
| `src/storage/feature_flags.rs` | 12 | 含 `QueryBuilder` |
| `src/storage/ai_connection.rs` | 6 | 简单表 |
| `src/storage/matrixrtc.rs` | 11 | 简单表 |

**步骤**：
1. 启动本地 Postgres，应用最新 schema。
2. 一次性生成基线 `.sqlx/`：
   ```bash
   cargo sqlx prepare --workspace --all-features
   ```
3. 提交 `.sqlx/`（按文件分片，避免单 commit 过大）。
4. 改造这 4 个文件，逐一验证：
   - 简单 `SELECT` → `query_as!` / `query!`
   - `QueryBuilder` 条件分支 → 拆分为多个 `query!`（每个组合一份）或保留为 `query_unchecked!` + 命名 query
5. 每个文件跑 `cargo check` + `cargo test --lib storage::`。
6. PR 标题 `refactor(storage): M-3 batch1 — sqlx::query! compile-time validation`。

**验收**：
- `.sqlx/` 入仓且文件 ≥ 4。
- `cargo sqlx prepare --check` 通过。
- 4 文件动态 query 数从 34 → 0。

### Batch 2（Phase 1）— Auth/Token 域（5 文件，~80 处）

| 文件 | 动态 query 数 |
|---|---|
| `src/storage/token.rs` | 15 |
| `src/storage/threepid.rs` | 17 |
| `src/storage/refresh_token.rs` | 27 |
| `src/storage/registration_token.rs` | 21 |
| `src/storage/email_verification.rs` | 9 |

### Batch 3（Phase 1）— User/Device 域（4 文件，~124 处）

| 文件 | 动态 query 数 |
|---|---|
| `src/storage/user.rs` | 27 |
| `src/storage/device.rs` | 42 |
| `src/storage/dehydrated_device.rs` | 10 |
| `src/storage/captcha.rs` | 15 |
| `src/storage/cas.rs` | 20 |
| `src/storage/openid_token.rs` | 7 |

### Batch 4（Phase 2）— 事件/房间核心（5 文件，~226 处）

| 文件 | 动态 query 数 |
|---|---|
| `src/storage/event.rs` | 53 |
| `src/storage/room.rs` | 76 |
| `src/storage/membership.rs` | 29 |
| `src/storage/space.rs` | 36 |
| `src/storage/room_summary.rs` | 28 |

> 这一批有较多 `QueryBuilder` 和事务，需要先建 `migration_queries/` 目录放命名 query。

### Batch 5（Phase 2）— 联邦/路由核心（6 文件，~150 处）

| 文件 | 动态 query 数 |
|---|---|
| `src/storage/sliding_sync.rs` | 24 |
| `src/storage/state_groups.rs` | 17 |
| `src/storage/federation_queue.rs` | 8 |
| `src/storage/federation_blacklist.rs` | 12 |
| `src/storage/relations.rs` | 14 |
| `src/storage/thread.rs` | 36 |

### Batch 6（Phase 3）— E2EE 域（11 文件，~80 处）

> 包含 `e2ee/*/storage.rs`、`services/identity/storage.rs` 等。

### Batch 7（Phase 3）— Service/Worker/Web 域（剩余 ~700 处）

> 涵盖 `services/*` 中的内联 SQL、`worker/storage.rs`、`web/routes/*` 残留、federation 等。

## 五、迁移模式手册

### 5.1 简单 `query_as!` 改造

```rust
// BEFORE
let row = sqlx::query_as::<_, UserRecord>(
    "SELECT user_id, username FROM users WHERE user_id = $1"
)
.bind(user_id)
.fetch_optional(pool)
.await?;

// AFTER
let row = sqlx::query_as!(
    UserRecord,
    r#"SELECT user_id, username FROM users WHERE user_id = $1"#,
    user_id
)
.fetch_optional(pool)
.await?;
```

> 字段名必须与 SELECT 一致，否则需要 AS 别名。

### 5.2 INSERT/UPDATE/DELETE 用 `query!`

```rust
// AFTER
let result = sqlx::query!(
    r#"UPDATE users SET last_seen_ts = $2 WHERE user_id = $1"#,
    user_id,
    last_seen_ts
)
.execute(pool)
.await?;
let affected = result.rows_affected();
```

### 5.3 `QueryBuilder` 条件分支处理

策略 A — 拆为多个 `query!`：

```rust
// BEFORE
let mut q = QueryBuilder::new("SELECT * FROM events WHERE 1=1");
if let Some(room) = room_id { q.push(" AND room_id = ").push_bind(room); }
if let Some(limit) = limit { q.push(" LIMIT ").push_bind(limit); }

// AFTER
let rows = match (room_id, limit) {
    (Some(room), Some(lim)) => {
        sqlx::query_as!(EventRow,
            "SELECT * FROM events WHERE room_id = $1 ORDER BY ts DESC LIMIT $2",
            room, lim
        ).fetch_all(pool).await?
    }
    (Some(room), None) => {
        sqlx::query_as!(EventRow,
            "SELECT * FROM events WHERE room_id = $1 ORDER BY ts DESC",
            room
        ).fetch_all(pool).await?
    }
    // ... 其它组合
};
```

策略 B — 命名 query（在 `migrations/sqlx_named_queries/` 目录）：

```sql
-- migrations/sqlx_named_queries/list_events_by_filter.sql
-- name: list_events_by_filter
SELECT * FROM events
WHERE ($1::text IS NULL OR room_id = $1)
ORDER BY ts DESC
LIMIT $2;
```

```rust
// 使用
sqlx::query_as!(EventRow, "list_events_by_filter", room_id, limit)
    .fetch_all(pool).await?;
```

### 5.4 事务中宏

```rust
let mut tx = pool.begin().await?;
let rows = sqlx::query_as!(UserRecord,
    "SELECT user_id, username FROM users WHERE user_id = $1",
    user_id
)
.fetch_all(&mut *tx)
.await?;
tx.commit().await?;
```

### 5.5 复杂 JSON/数组返回

`query!` 会推断类型。如果是 `serde_json::Value`：

```rust
sqlx::query!(
    r#"SELECT data FROM events WHERE id = $1"#,
    id
)
.fetch_optional(pool)
.await?
.map(|row| row.data);  // row.data: Option<serde_json::Value>
```

## 六、CI 门禁设计

### 6.1 `db-migration-gate.yml` 增加新 job

```yaml
sqlx-offline-cache-gate:
  name: sqlx Offline Cache Gate
  runs-on: ubuntu-latest
  services:
    postgres:
      image: postgres:15
      env:
        POSTGRES_USER: synapse
        POSTGRES_PASSWORD: synapse
        POSTGRES_DB: synapse
      ports: ["5432:5432"]
  steps:
    - uses: actions/checkout@v4
    - name: Install sqlx-cli
      run: cargo install sqlx-cli --locked --no-default-features --features postgres
    - name: Apply migrations
      env:
        DATABASE_URL: postgresql://synapse:synapse@localhost:5432/synapse
      run: |
        sqlx database create
        sqlx migrate run
    - name: Check .sqlx cache consistency
      env:
        DATABASE_URL: postgresql://synapse:synapse@localhost:5432/synapse
      run: |
        cargo sqlx prepare --check
        bash scripts/ci/check_sqlx_offline_cache.sh
```

### 6.2 `drift-detection.yml` 增加动态 query 占比门禁

```yaml
sqlx-dynamic-query-ratio:
  name: Dynamic SQL Query Ratio
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - name: Compute ratio
      run: |
        dynamic=$(grep -rE 'sqlx::query\(|\bsqlx::query_as\(' src/ | wc -l)
        static=$(grep -rE 'sqlx::query!|sqlx::query_as!|sqlx::query_scalar!|sqlx::query_file!' src/ | wc -l)
        total=$((dynamic + static))
        ratio=$(awk "BEGIN { printf \"%.4f\", $dynamic / $total }")
        echo "dynamic=$dynamic static=$static ratio=$ratio"
        awk -v r="$ratio" 'BEGIN { exit !(r <= 0.30) }' \
          || { echo "::error::dynamic ratio $ratio exceeds 0.30"; exit 1; }
```

## 七、追踪看板

每完成一个文件，在 `scripts/ci/sqlx_migration_inventory.json` 中标记 `done: true`。

初始化时生成基线：

```json
{
  "baseline_date": "2026-06-03",
  "total_dynamic": 2003,
  "total_static": 4,
  "files": [
    { "path": "src/storage/audit.rs", "dynamic": 5, "static": 0, "done": false },
    { "path": "src/storage/feature_flags.rs", "dynamic": 12, "static": 0, "done": false },
    ...
  ]
}
```

每完成一个 batch 更新一次 `M3_PROGRESS.md`，格式：

```markdown
## Batch 1 (2026-06-10) — 4 文件，34 → 0 动态
- [x] src/storage/audit.rs (5 → 0)
- [x] src/storage/feature_flags.rs (12 → 0)
- [x] src/storage/ai_connection.rs (6 → 0)
- [x] src/storage/matrixrtc.rs (11 → 0)
- 累计: 2003 → 1969 (-34)
```

## 八、风险与缓解

| 风险 | 缓解 |
|---|---|
| `QueryBuilder` 拆为多个 `query!` 后代码可读性下降 | 抽到 helper 函数 `list_events_filtered(room_id, limit)` |
| `.sqlx/` 缓存体积大 | 接受（一般 < 5MB），用 gitattributes 标记为文本 |
| CI 缓存漂移 | `cargo sqlx prepare --check` 在每次 PR 必跑 |
| 列名/类型不匹配导致编译失败 | 这是目标；一次性修复比运行时错误好 |
| 命名 query 文件需 import 路径 | 放在 `migrations/sqlx_named_queries/`，与 schema 同源管理 |

## 九、验收标准（终极）

1. `cargo sqlx prepare --check` 在 CI 100% 通过。
2. `bash scripts/ci/check_sqlx_offline_cache.sh` 通过。
3. `bash scripts/ci/check_sqlx_dynamic_ratio.sh` 显示 ratio ≤ 0.30。
4. `cargo test --all-features --locked` 全绿。
5. `.sqlx/` 完整入仓。
6. 所有 `route → service → storage` 路径的 storage 层全部走编译期宏。
