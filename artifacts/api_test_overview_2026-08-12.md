# API 全量测试工具 + 三缺陷修复 — 交付概览

**日期**：2026-08-12 ｜ **commit**：`961b566e` ｜ **工具目录**：`scripts/api_test/` ｜ **目标**：`https://matrix.test`（dev）

## 一、三处 API 缺陷修复（全量测试 1288→1290 通过，健康度 99.7→99.9）

### 1. keys/rotation/check 500 — EXTRACT 类型推断错误

**根因**：`rotated_at` 列是 BIGINT 毫秒时间戳，但 SQL 调用了 `EXTRACT(EPOCH FROM rotated_at)`
（仅适用于 timestamp/timestamptz），PG 报 `function pg_catalog.extract(unknown, bigint) does not exist`。

**修复**（`synapse-e2ee/src/key_rotation/service.rs`）：
- `get_last_rotation_for_key`：`SELECT EXTRACT(EPOCH FROM rotated_at)*1000` → `SELECT rotated_at`
- `get_max_rotation_ts`：`COALESCE(EXTRACT(EPOCH FROM MAX(rotated_at))*1000,0)::bigint` → `COALESCE(MAX(rotated_at),0)`

### 2. worker/v1/statistics 500 — 补 schema 迁移（用户明确要求）

**根因**：`get_statistics` 查询 `FROM worker_statistics` 引用了 15 个不存在的列
（worker_name/cpu_usage/...），`worker_statistics` 表实际只有 11 列。

**修复**（补 schema 迁移，而非改 SQL 规避）：
- 新增迁移 `20260812120000_worker_statistics_load_metrics.sql`：`ALTER TABLE worker_statistics
  ADD COLUMN` 15 列（worker_name/worker_type/status/host/port/last_heartbeat_ts/started_ts
  + 8 个实时负载指标列），并从 workers 表回填身份/生命周期字段
- 同步更新 `00000000_unified_schema_v10.sql` 基线定义
- `synapse-storage/src/worker/repository.rs`：恢复 `FROM worker_statistics` 全列查询

### 3. media/quota/check 偶发唯一约束冲突 — check-then-insert 竞态

**根因**：`get_or_create_user_quota` 先 `get_user_quota` 再 `INSERT`（check-then-insert），
并发下两个请求同时走到 INSERT → `uq_user_media_quota_user` 唯一约束冲突。

**修复**（`synapse-storage/src/media_quota/repository.rs`）：
- 改为原子 `INSERT ... ON CONFLICT (user_id) DO UPDATE SET updated_ts = user_media_quota.updated_ts RETURNING *`
- 并发压测验证：删除 admin quota 行后 20 并发首次 INSERT，全部 HTTP 200，表内仅 1 行

## 二、migrator 循环 bug 修复（重要发现）

`docker/deploy/scripts/container-migrate.sh` 的 `apply_pending_migrations` 有两个叠加 bug，
导致增量迁移**从未被应用**（schema_migrations 仅 20 条记录，29 个增量迁移静默跳过）：

1. **stdin 耗尽**：`find|sort|while` 管道中，`psql_with_retry` 的 `cat >"$stdin_file"`
   贪婪读取 while 循环的 stdin（find 输出），循环在首个调用 `psql_db` 的文件处被耗尽中断
   → 修复：改为 `for + 命令替换`（不占用 stdin，计数能正确传回）
2. **set -e 中断**：`apply_sql_file` 失败 `return 1` 在 for 循环触发 `set -e` 退出
   → 修复：用 `if apply_sql_file; then ... else ... fi` 包裹

修复后迁移正常执行：applied=22，历史记录 20→63。

## 三、全量 API 测试工具（scripts/api_test/）

| 文件 | 作用 |
| --- | --- |
| `run_api_tests.py` | 主测试执行器（自动遍历 1292 路由，匿名+认证双探测，报告+健康度评分） |
| `config.yaml` | 多环境配置（base_url/token/TLS/并发/路径参数映射） |
| `expectations.yaml` | 模块级响应期望规则 |
| `ledger.json` | Docker 同款 features 导出的 1292 条路由清单 |
| `export_ledger.sh` | 从二进制导出最新路由清单 |
| `stress_quota_check.py` | media/quota 并发竞态压测工具 |
| `README.md` | 使用文档 |

## 四、最终测试结果（2026-08-12 10:48，修复后）

| 指标 | 修复前 | **修复后** |
| --- | --- | --- |
| 通过 / 警告 / 失败 | 1288 / 1 / 3 | **1290 / 1 / 1** |
| 健康度评分 | 99.7 | **99.9 / 100（优秀）** |

### 剩余 1 条失败（非代码缺陷）

| 方法 | 路径 | 判定 | 说明 |
| --- | --- | --- | --- |
| GET | `/_matrix/client/v3/register/captcha/status` | ❌ 超时 | 并发负载下偶发（隔离复测 3/3 秒回 400），captcha 端点受全局限流影响，非代码缺陷 |

## 五、部署注意事项

- `docker compose run` 调用 migrator 必须加 `-T`（禁用 TTY）+ `< /dev/null`，
  否则 `cat >stdin_file` 因 TTY stdin 永不 EOF 而挂死
- v1/login 路由未注册，登录用 r0/login
- 运行依赖安装在受管 venv：`/Users/ljf/.workbuddy/binaries/python/envs/default`
