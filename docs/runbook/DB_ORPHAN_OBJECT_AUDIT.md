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
# `127.0.0.1:5432` 现在**就是本项目库**：容器已把 5432 发布到宿主回环
# （2026-10-15「环境收敛」，宿主自装 PG 已全部停用）。所以下面这条可直接用：
DATABASE_URL="postgresql://synapse:synapse@127.0.0.1:5432/synapse" \
    python3 scripts/report_orphan_db_objects.py --dsn-env DATABASE_URL
# 也可以用其审查**任意库**（如 CI/临时库）：把库名换掉即可。环境沿革见报告 §16。

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

### 3.2 清理执行记录（2026-10-15，方案 B5.3，已执行）

按 §4 的流程对上述 **4 张废弃表**执行完毕：

| 步骤 | 证据 |
|---|---|
| ① 新的全量备份 | `…/hu_ts/backups/synapse_pg_20261015_161059.dump`（17 MB；`pg_restore --list` **1990 条目**校验通过） |
| ② 逐表存证 | `…/hu_ts/backups/pre-drop-evidence-2026-10-15/{device_lists_outbound_pokes,room_state_events,leak_alerts,push_devices}.dump`（`pg_dump -Fc`，均 `TABLE DATA` 段存在、0 行）＋ `SNAPSHOT.txt`（行/列/索引数） |
| ③ 依赖核对 | 无外部外键指向这 4 张；事务内 `DROP`+`ROLLBACK` 演练通过（**无需 `CASCADE`**） |
| ④ 折入基线 | baseline 末尾新增幂等 `DROP TABLE IF EXISTS` 段（含证据注释）；指纹 `5654afa2ea622aff` → **`48e404226830f749`**（+2030 字节，按 R10 约定先自检旧值再取值） |
| ⑤ 执行 + 复核 | dev 库执行这 4 条 ⇒ 孤儿对象 **4 表 / 7 索引 / 5 约束 → 0 / 0 / 0**；报告结论变为"表/视图与基线一致" |

### 3.3 B5.4：报告上剩余 3 项**已全部收敛**（2026-10-15）

B5.3 之后报告剩 3 项。它们**不是"该删的表"**，而是"**同一张表的多个定义方互相漂移**"——
处置前必须先定口径。现已全部收敛，报告 **8 个分节全绿**。

#### 成因定论：**谁先创建，决定形态**（这解释了整类现象）

`schema_migrations` 这张账本表有**四个**定义方：canonical 基线、`docker/db_migrate.sh`、
`docker/deploy/scripts/init-db.sql`、`synapse-services/src/database_initializer/mod.rs`。
而**引导路径必然先建它**（迁移器要先有账本表才能记录迁移）⇒ `CREATE TABLE IF NOT EXISTS`
让"引导那份"赢，**基线的定义在实践上从不生效** —— 这正是四处不一致长期无人发现的原因。

| 库 | 创建者 | 实测形态 |
|---|---|---|
| `synapse_sqlx_scratch`、`synapse_sqlx_prep_v12` | **基线**直接创建 | 规范形态：PK 名 `pk_schema_migrations`、无冗余索引（报告全 0） |
| 容器 `synapse`、`synapse_ci_verify`、宿主 `synapse` | **引导**先创建 | 漂移形态：PK 自动命名 `schema_migrations_pkey`、多一棵 `idx_schema_migrations_version` |

判据（用本工具即可区分任意库）：

```bash
DATABASE_URL="postgresql://synapse:synapse@127.0.0.1:5432/<库>" \
  python3 scripts/report_orphan_db_objects.py --dsn-env DATABASE_URL
```

#### 三项处置

| 项 | 定论 | 处置 |
|---|---|---|
| `idx_schema_migrations_version` | **纯冗余**：`uq_schema_migrations_version UNIQUE (version)` 已提供 (version) 上的唯一 btree；这棵同列 btree 只带来双倍写放大＋双倍空间。且三处引导定义不一致（`db_migrate.sh` 建 **UNIQUE**、另两处非唯一）⇒ `IF NOT EXISTS` 下"谁先跑谁赢" | **三方一律不建**（`db_migrate.sh` / `init-db.sql` / Rust 均删该语句）；基线清理段加 `DROP INDEX IF EXISTS …` 使老库收敛 |
| `pk_schema_migrations` | 基线声明此名、引导建的是自动名 `schema_migrations_pkey`（**列相同、名字不同**） | **三方改为命名式** `CONSTRAINT pk_schema_migrations PRIMARY KEY (id)`；基线清理段加幂等 `RENAME CONSTRAINT` 使老库收敛 |
| `VALIDATE … fk_cross_signing_keys_user`（死引用） | 基线查/校验的是**旧名**，而它创建的是 `fk_cross_signing_keys_user_id` ⇒ 整块**死代码**。实测库里该外键本就 `convalidated=true`（创建时没写 `NOT VALID`） | 修正为实际名（**纯一致性清理、零行为变化**；保留了防御性：若某个老库上它是 `NOT VALID`，现在会真的被校验） |

#### 顺带修掉的第四处不一致：`executed_at` 精度

实测同一时刻两种写法的结果：

| 写法 | 结果 |
|---|---|
| 基线原写法 `EXTRACT(EPOCH FROM NOW())::BIGINT * 1000`（**先取整再乘**） | `…011000` —— 毫秒恒为 `000`，**丢精度** |
| 另三处写法 `(EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT`（先乘再取整） | `…011206` |

已把基线改为后者（与另三处一致）。因为引导总是先建表，基线那句在实践上从不生效 ——
**又一处"不生效的定义"里藏着的 bug**。

#### 验证

- 容器库执行两条收敛语句（即基线里那两条）⇒ 约束集合变为
  `pk_schema_migrations, uq_schema_migrations_version`（**与基线完全一致**）、冗余索引消失、
  `version` 的唯一保护仍在。
- 报告 **8/8 分节 ✅**：孤儿关系 0、未应用关系 0、孤儿索引 0（两类）、孤儿约束 0（两类）、
  未应用约束 0、VALIDATE 死引用 0。
- 指纹 `48e404226830f749` → **`306314fd1316fa2d`**（+3196 字节，按 R10 约定先自检旧值再取值）。
- `.sqlx` 经唯一入口 `scripts/ci/sqlx_prepare.sh` 重生成：**变更前 `--check` PASS**
  （`OK: .sqlx 与数据库元数据一致`，对 `synapse_sqlx_scratch`）⇒ 重生成后的差异只可能是本次
  删掉的那条 DDL 查询；缩容需显式 `ALLOW_CACHE_SHRINK=1`。
  ⚠️ 该源库当天晚些时候从宿主 PG 迁到了**容器内**（宿主自装 PG 全部停用）：
  已在容器里重建 `synapse_sqlx_scratch`（用 `init_test_public_schema.sh` 应用基线）并复测
  `--check` = **OK**。现在的调用方式是
  `DATABASE_URL="postgresql://synapse:synapse@127.0.0.1:5432/synapse_sqlx_scratch" \
   [ALLOW_CACHE_SHRINK=1] bash scripts/ci/sqlx_prepare.sh`。沿革见报告 §16。

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
