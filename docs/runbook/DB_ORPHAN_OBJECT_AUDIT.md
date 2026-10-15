# 数据库孤儿对象审计 runbook

> 工具：`scripts/report_orphan_db_objects.py`（**只读**，不执行任何 DDL/DML）
> 首次实测：2026-10-15（dev/deploy 栈真实库 PG 16.15）

## 1. 为什么需要它

仓库里已有的 `scripts/check_schema_table_coverage.py` 只做**一个方向**：

```
代码里引用的表  ->  基线 migrations/ 里必须定义      （代码 -> 基线）
```

于是"**基线里没有、老库里却真实存在**"的对象**没有任何门禁看得见**。根因是基线
`migrations/00000000_unified_schema_v12.sql` 里 `DROP TABLE` 计数为 **0**（只增不减）：
它靠 `CREATE TABLE IF NOT EXISTS` 幂等重放，但**从不删除**已经废弃的表。
⇒ 老库永久保留早期迁移链的残留，全新库不会有 —— **同一个版本号、两种库形态**。

本工具补的就是这个缺失方向（**基线 -> 库**），并且是**报告**而非门禁
（CI 里没有数据库，且"老库有残留"本身未必是缺陷，需人判断）。

## 2. 怎么跑

```bash
# 默认走本机容器 synapse-postgres（dev 与 deploy 栈的容器名相同、库名相同）
python3 scripts/report_orphan_db_objects.py

# 换容器/账号/库/schema
python3 scripts/report_orphan_db_objects.py --container synapse-postgres \
    --user synapse --db synapse --schema public

# 容器不可用时用连接串直连（需要本机 psql）
DATABASE_URL=postgresql://synapse@127.0.0.1:5432/synapse \
    python3 scripts/report_orphan_db_objects.py --dsn-env DATABASE_URL

# 机器可读（供趋势对比/留存）
python3 scripts/report_orphan_db_objects.py --json-report /tmp/orphans.json

# 巡检模式：存在孤儿表/视图或未应用对象时 exit 1
python3 scripts/report_orphan_db_objects.py --strict
```

**它是只读的**：只跑 `pg_class` / `pg_index` / `pg_constraint` / `pg_depend` 的 `SELECT`。
任何"清理"动作都必须**另行**执行，且遵守 §4 的流程。

## 3. 首次实测结果（2026-10-15，dev 栈真实库）

```
基线关系 222 个；实时库关系 226 个（另有 346 个索引由 PK/UNIQUE 承载、已排除）
```

| 分类 | 数量 | 明细 |
|---|---|---|
| 孤儿表/视图（库有、基线无） | **4** | `device_lists_outbound_pokes`、`room_state_events`、`leak_alerts`、`push_devices`（**均 0 行**） |
| 未应用表/视图（基线有、库无） | **0** | — |
| 孤儿索引 · 挂孤儿表上 | 7 | 上述 4 张表的索引（随表一起处理） |
| 孤儿索引 · 挂存活表上 | **1** | `idx_schema_migrations_version` |
| 孤儿约束 · 挂孤儿表上 | 5 | 上述 4 张表的 PK/FK/UQ |
| 孤儿约束 · 挂存活表上 | **0** | — |
| 未应用约束（基线有、库无） | **1** | `pk_schema_migrations` |
| VALIDATE 死引用 | **1** | `fk_cross_signing_keys_user` |

### 3.1 三条需要单独解释的发现

**(a) `idx_schema_migrations_version` —— 同一个索引由三处分别创建，基线里没有。**
`docker/deploy/scripts/init-db.sql:34`、`docker/db_migrate.sh:339`、
`synapse-services/src/database_initializer/mod.rs:365` 都建它，唯独
`migrations/` 里没有 ⇒ 新库/老库在这张表上形态不一致。
**处置**：把它折入基线，让基线成为唯一来源（其余三处可保留 `IF NOT EXISTS` 作为引导兜底）。

**(b) `pk_schema_migrations` 未应用 —— 命名漂移。**
基线声明 `CONSTRAINT pk_schema_migrations PRIMARY KEY (id)`，
而库里实际叫 `schema_migrations_pkey`（**列相同、名字不同**）。
因为表是用 `CREATE TABLE IF NOT EXISTS` 建的，老库先建成的那份"赢"了，基线的名字从未生效。
**影响**：低（主键仍存在），但属于"同一版本、两种形态"的证据。

**(c) `fk_cross_signing_keys_user` —— 基线内陈旧引用（死分支）。**
基线第 4003 行创建的是 `fk_cross_signing_keys_user_id`，
但第 5221/5225 行的守卫却查/校验**旧名** `fk_cross_signing_keys_user`：

```sql
IF EXISTS (SELECT 1 FROM pg_constraint
           WHERE conname = 'fk_cross_signing_keys_user' AND conrelid = 'cross_signing_keys'::regclass
           AND NOT convalidated) THEN
    ALTER TABLE cross_signing_keys VALIDATE CONSTRAINT fk_cross_signing_keys_user;
END IF;
```

⇒ 该分支**永不触发**。实测库里 `fk_cross_signing_keys_user_id` 的 `convalidated = true`
（创建语句本身就是 VALIDATED），**所以这处没有造成实际影响**；
但它是一条应当对齐名字的陈旧引用 —— 今天无害不代表明天无害。

## 4. 处置流程（清理孤儿表之前必读）

1. **先备份**（前置条件，不可跳过）：`docs/runbook/DB_RESTORE_DRILL.md` 的流程 + `scripts/backup_database.sh`
   产出一份**新的**全量备份并校验（`pg_restore --list`）。备份没成功就不许动库。
2. **逐表存证**：对每张待删表做 `pg_dump -t <表>` 存成归档（守"删除前有取证"的规矩）：

   ```bash
   docker exec -u postgres synapse-postgres pg_dump -U synapse -d synapse \
       -Fc -t '<表名>' > /path/outside/repo/<表名>.dump
   ```
3. **核对引用**：`grep -rn "<表名>" --include=*.rs --include=*.sql`。
   若只剩注释（如本仓 `device_lists_outbound_pokes` / `room_state_events`），说明代码侧已无依赖。
4. **折入基线**：在基线末尾追加 `DROP TABLE IF EXISTS <表>;`（并同步
   `EXPECTED_BASELINE_FINGERPRINT`、`.sqlx`、`test_template_ci` —— 见项目记忆）。
   这样**新库与老库都会收敛到同一形态**，而不是只清一台机器。
5. **执行与复核**：应用基线 → 重跑本工具，确认孤儿数归零、且"未应用对象"仍为 0。

> ⚠️ 只删本机库、不改基线 = **假收敛**：换一台机器/新库还会长出同样的残留（或反过来缺表）。
> 本工具的 `--strict` 就是给这种巡检用的。

## 5. 工具刻意规避的假阳性（改脚本前先读）

报告类工具的失败模式与门禁相反：**门禁怕漏报，报告怕误报**（狼来了就没人看了）。
因此有四道过滤，全部有实测支撑（关掉任一都会爆出已知误报量）：

| 过滤 | 关掉后的误报 | 依据 |
|---|---|---|
| 排除**扩展自带对象**（`pg_depend.deptype='e'`） | 孤儿关系 4 → **6**（多出 `pg_stat_statements`、`pg_stat_statements_info`） | 实测 |
| 排除**由 PK/UNIQUE 隐式创建的索引**（`pg_constraint.conindid`） | 孤儿索引 1 → **347** | 实测 |
| 排除**基线用 `%I` 动态生成**的约束名（模板从基线派生） | 孤儿约束 5 → **119**（106 条 `*_user_id_format` + 8 条 `*_mxc_format` + …） | 实测 |
| 排除**自动命名**约束（`_pkey`/`_key`/`_fkey`/`_check` 结尾） | 孤儿约束 5 → **138** | 实测 |
| 摘除 `VALIDATE CONSTRAINT` 再解析"定义" | 死引用 1 → **0**（被掩盖） | 实测 |

这套对照由 `--` 变异自证脚本验证过（关掉每个过滤器 → 数字按上表膨胀；开启 → 回到真值 4/1/5/1）。

## 6. 相关文件

| 文件 | 作用 |
|---|---|
| `scripts/report_orphan_db_objects.py` | 本工具（只读） |
| `scripts/check_schema_table_coverage.py` | 反方向门禁（代码 → 基线） |
| `scripts/check_baseline_consolidation.py` | 基线内重复对象守卫（含同键覆盖对白名单） |
| `docs/audit/DB_REVIEW_2026-10-15.md` | 本轮数据库审查报告（分级与证据） |
| `docs/audit/DB_OPTIMIZATION_PLAN_2026-10-15.md` | 批次化优化方案与执行记录 |
| `docs/runbook/DB_RESTORE_DRILL.md` | 备份/恢复演练（清理前的备份依据） |
