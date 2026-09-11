# 迁移链无法在全新库重放：增量 ADD COLUMN 与 v11 基线冲突

> **日期**: 2026-09-12
> **基线提交**: `ea0e94d6`（发现时）
> **范围**: P3「数据层与持久化 — schema 双真相源、迁移可重放」
> **严重度**: **高**（全新部署 / 干净 CI / 重建测试库全部无法完成迁移）

---

## 0. 结论

把完整迁移链应用到**全新数据库**时，迁移在 74 个文件的第 30 个中止：

```
[INFO]  应用迁移: 20260906010000_add_events_soft_failed.sql
ERROR:  column "soft_failed" of relation "events" already exists
[ERROR] 迁移失败: 20260906010000_add_events_soft_failed.sql
```

根因是**同一列在两处定义、幂等语义不一致**：

| 位置 | 写法 | 幂等 |
|---|---|---|
| `00000000_unified_schema_v11.sql:373`（基线） | `ALTER TABLE events ADD COLUMN IF NOT EXISTS soft_failed ...` | ✅ |
| `20260906010000_add_events_soft_failed.sql:23`（增量） | `ALTER TABLE events ADD COLUMN soft_failed ...` | ❌ |

基线折叠（commit `ca2d65c9`）把 `soft_failed` 写进了 v11，但**没有同步**增量迁移的
幂等性。于是全新库先由基线建出该列，再被增量迁移重复添加 → 报错中止。

## 1. 为什么本地长期没有暴露（这条最值得记录）

**存量库永远不会走到这个文件**：它的 `schema_migrations` 里已有
`20260906010000` 的行，迁移器直接跳过。所以：

| 路径 | 是否受影响 |
|---|---|
| 开发者本地存量库 | ❌ 不受影响（该迁移被跳过） |
| **全新 CI 数据库** | ✅ **迁移中止** |
| **新部署** | ✅ **迁移中止** |
| **`DROP DATABASE` 后重建** | ✅ **迁移中止** |
| `docker compose` 全新起栈 | ✅ **迁移中止** |

这正是 P3 要求防的"schema 双真相源"缺陷的典型形状：**两种环境走两条不同的真相**，
而本地那条恰好是安全的，于是缺陷只会在最不该出问题的时候（上线）出现。

## 2. 复现（本轮实际执行）

本地 5432 集群处于崩溃恢复（见 `P5_test_schema_accumulation_2026-09-12.md` §8），
因此验证在临时集群上完成：

```bash
# 1. 起一个临时集群（端口 5433，避免触碰 5432）
BIN=/opt/homebrew/opt/postgresql@15/bin
initdb -D /tmp/a4f2_pgtest/pgdata -U ljf --auth-local=trust --auth-host=trust --no-sync
pg_ctl -D /tmp/a4f2_pgtest/pgdata \
  -o "-p 5433 -k /tmp/a4f2_pgtest -c listen_addresses=127.0.0.1 -c fsync=off \
      -c max_connections=100 -c max_locks_per_transaction=4096" \
  -l /tmp/a4f2_pgtest/pg.log start

psql -h 127.0.0.1 -p 5433 -U ljf -d postgres \
  -c "CREATE ROLE synapse LOGIN SUPERUSER PASSWORD '...';" \
  -c "CREATE DATABASE synapse OWNER synapse;"

# 2. 重放完整迁移链
DATABASE_URL='postgresql://synapse:<pw>@127.0.0.1:5433/synapse' \
  bash docker/db_migrate.sh migrate
```

### RED（修复前）

```
[INFO] 应用迁移: 20260906010000_add_events_soft_failed.sql
ERROR:  column "soft_failed" of relation "events" already exists
[ERROR] 迁移失败: 20260906010000_add_events_soft_failed.sql
```

### GREEN（修复后）

```
[SUCCESS] 迁移完成: 20260907000000_burn_idempotent_retry_cap.sql
[SUCCESS] 迁移完成: 20260907010000_allow_forget_membership.sql
[SUCCESS] 迁移完成: 20260909010000_drop_stale_legacy_tables.sql
[SUCCESS] 迁移完成: 20260910100000_event_relations_pagination_index.sql
[SUCCESS] 已应用 37 个迁移
```

```sql
SELECT count(*) FROM public.schema_migrations;   -- 38
```

### 迁移后可用性验证

新库不只是"迁移跑完"，还要能真正承载测试：

```
$ cargo nextest run --profile test --features test-utils -p synapse-storage --lib \
    -E 'test(/room_summary::db_tests/)'
    Summary [0.173s] 30 tests run: 30 passed, 1502 skipped
```

## 3. 修复

`migrations/20260906010000_add_events_soft_failed.sql`：

```diff
-ALTER TABLE events ADD COLUMN soft_failed BOOLEAN NOT NULL DEFAULT FALSE;
+ALTER TABLE events ADD COLUMN IF NOT EXISTS soft_failed BOOLEAN NOT NULL DEFAULT FALSE;
```

并在文件内就地说明为什么这个守卫不是装饰（记录本次实测的错误信息与影响范围）。

## 4. 防复发守卫

`tests/unit/migration_replayability_guard_tests.rs`（纯静态，无 DB）：

对每条增量迁移的 `ALTER TABLE <t> ADD COLUMN <c>`，若 v11 基线已声明 `<c>`，
则必须有守卫。**两种既有惯用法都接受**：

1. 内联 —— `ADD COLUMN IF NOT EXISTS c ...`；
2. `DO $$ ... IF NOT EXISTS (SELECT 1 FROM information_schema.columns ...)
   THEN ALTER TABLE t ADD COLUMN c ... END IF; END $$;`

守卫自检：断言从基线解析出 **>1000** 个 `table.column` 对，防止解析器静默失效
而让守卫变成永远通过。

### RED → GREEN 实测

去掉 `20260906010000` 的 `IF NOT EXISTS` 后：

```
FAIL migration_replayability_guard_tests::incremental_add_column_must_be_idempotent_when_baseline_has_the_column
      migrations/20260906010000_add_events_soft_failed.sql:38 — `ADD COLUMN events.soft_failed`
      is unguarded, but the v11 baseline already declares it. A fresh database cannot replay
      the chain past this file
```

恢复后 PASS。

> **守卫第一版误报，值得记录**：初版只认内联 `IF NOT EXISTS`，结果报出 6 个
> "违规"文件（`add_redacts_column` / `read_markers_redundant_origin_server_ts` /
> `msc4242_state_dag_prev_state_events` / `add_secret_key_to_verification_sas` /
> `sliding_sync_token_event_stream_pos` / `add_fallback_used_column`）。逐个核对后
> 发现它们**全部**使用 DO 块 + `information_schema.columns` 检查，且在本轮实测中
> 确实重放通过。谓词已按实际惯用法修正——**守卫误报会让人直接删掉守卫**，这比
> 没有守卫更糟。

## 5. 未做 / 未验证

| 项 | 状态 |
|---|---|
| 其余 73 个迁移是否**逐个**可重放 | 已验证"整链能跑通"，但未对每个做独立 RED-GREEN |
| `docker compose` 全新起栈路径 | **未验证**（本环境 Docker 栈处于停止状态） |
| 真实 CI 干净库 | **未验证**（仓库 private、`gh` token 失效，本环境不可达） |
| `.undo.sql` 回滚链的可重放性 | **未做**——本守卫刻意排除 `.undo.sql` |
| 21,748 个残留测试 schema 的清理 | **未做**，见 `P5_test_schema_accumulation_2026-09-12.md` §4.3 |

> ⚠️ 另需说明：验证所用集群是**临时**的（`/tmp`，`fsync=off`，
> `max_locks_per_transaction=4096`），因此它验证的是"迁移链的逻辑可重放性"，
> **不**代表生产参数下的性能或 fsync 行为。5432 主集群自 22:01 起仍在崩溃恢复，
> 本项未触碰它。
