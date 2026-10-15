# 数据库优化方案（2026-10-15）

> **依据**：`docs/audit/DB_REVIEW_2026-10-15.md`（实测取证；其中 §1 已于同日更正为"备份存在但无 PITR"）。
> **不重复**：`DB_REVIEW_2026-09-17.md` 已修项（28 组重复索引、18 个外键前导索引、7 个 int4 主键、
> 连接预算成文、`widgets`/`server_notices` NOT NULL 等）——本方案只处理**仍然开着**的问题。
>
> **执行纪律（沿用本仓既有约定）**：
> 1. **每批一个原子提交**，批内自洽；**每批需显式授权后**才实施，未授权不动代码。
> 2. **新增门禁必须 RED 自证**：先证明"构造缺陷 ⇒ 门禁红"，再证明"移除缺陷 ⇒ 门禁绿"。
>    **没有 RED 自证的守卫等于没有守卫**（本仓已有先例：54 个未格式化文件在"计数恒为 0"的门禁下跑过 21 次 CI）。
> 3. **不引入静默余量**：棘轮/断言一旦收紧就按实测值钉死；放宽必须写明理由并留痕。
> 4. **凡"不存在/必然失败"类结论，先跑一遍再写**（本报告 §1 的更正就是没做到这点的代价）。
> 5. 涉及 `DROP` / 数据目录 / 参数重启的操作，**必须前置备份 + 回滚路径 + 演练**。

---

## 0. 先要拍的四个数（阻塞项）

| # | 决策 | 选项 | 影响批次 |
|---|---|---|---|
| D1 | **RPO / RTO 目标** | 建议自建单机 **RPO ≤ 15min / RTO ≤ 30min**；若要 RPO≈0 需同步复制（代价见 §7） | B1、B2 |
| D2 | **异地备份落点** | ① 另一台机/NAS（SSH+rsync）② 对象存储（S3 兼容 + WAL-G/pgBackRest）③ 暂不做（**则 B1 只能算半成品**） | B1 |
| D3 | **`pids_limit` 与 `max_connections` 二选一** | A：`pids_limit ≥ 300`（保留 250 连接）B：`max_connections ≤ 240`（保留 256 pids） | B3 |
| D4 | **`work_mem` 取值** | 需生产并发排序数实测；缺数据时先保守取 **8MB**，并同步下调 `max_connections` 或上调 `mem_limit` | B3 |

> 这四个数定不下来，B1/B3 都会退化成"猜"。**建议 D1/D3 当天拍板**（都是二选一且有默认值）。

---

## 1. 批次总览

| 批次 | 主题 | 对应问题 | 风险 | 依赖 | 预估 |
|---|---|---|---|---|---|
| **B1** | 备份可用化（WAL 归档 + 异地 + 脚本修复 + 校验 + 演练） | §1 | 低（改配置 + 脚本） | D1、D2 | 1–2 天 |
| **B2** | 观测面补齐（postgres_exporter + 告警） | §3 | 低（新增容器） | D1 | 0.5–1 天 |
| **B3** | 资源预算一致（pids / 内存 → 门禁） | §4、§5、§10 | 中（重启 + 压测） | D3、D4 | 1 天 |
| **B4** | 迁移可靠性（记成功语义 + 锁 + INVALID 索引 + CHECK 改 `NOT VALID`） | §6、§6.5 | 中（迁移路径） | B1（备份就位） | 2 天 |
| **B5** | 对象卫生与守卫（孤儿表 + 守卫盲区） | §7、§8 | 中（含 DROP） | B1 | 1 天 |
| **B6** | 待裁定项（中文检索 / JSONB / 前缀冗余登记） | §9、§11、§12 | 低（测量为主） | — | 1 天 |

**可并行**：B1 与 B2 不同文件、B6 纯测量 ⇒ 三者可并行；B4/B5 串行在 B1 之后（因为都要"备份就位"作为回滚前提）。

---

## 2. B1 · 备份可用化

### B1.1 补 WAL 归档（唯一能把 RPO 压到分钟级的手段）

- **改动**：`docker/config/postgres.conf`
  ```ini
  # 新增（示例：本地目录 + 外部同步；对象存储则改用 archive_library/WAL-G）
  archive_mode = on
  archive_command = 'test ! -f /var/lib/postgresql/wal_archive/%f && cp %p /var/lib/postgresql/wal_archive/%f'
  archive_timeout = 60s          # 低写入时段也保证分钟级 RPO
  wal_keep_size = 512MB          # 备库/归档异常时的短期缓冲（非长期方案）
  ```
  > ⚠️ **需在你的 PG 版本/OS 上验证**：`archive_library`（PG16 可用）与 `archive_command` 的取舍；
  > 用 WAL-G/pgBackRest 时**不要**再写 `archive_command`，二选一。
- **重启**：`archive_mode` 需重启（`docker compose up -d --force-recreate postgres`），
  **必须先做 B1.3 的脚本修复 + 一次全量备份**，否则重启窗口内无可回退点。
- **判据**：
  ```bash
  psql -c "SELECT archived_count, failed_count, last_archived_time FROM pg_stat_archiver"
  # 要求：failed_count = 0；archived_count 在 5 分钟内持续增长
  ```
- **回滚**：`archive_mode = off` + 重启；归档产物独立目录，不写进 PGDATA（避免磁盘打满 ⇒ **必须另设卷**并监控剩余空间）。

### B1.2 备份异地

- **改动**：
  - `docker/deploy/scripts/backup.sh`：`BACKUP_DIR` 已是 env 可配（默认 `./backups`）⇒ 部署文档里改为**数据卷之外**的挂载点；
  - `scripts/backup_database.sh`：**改掉默认 `/tmp/backups`**（易失），默认改 `./backups` 并要求显式指定才用 `/tmp`；
  - 新增可选的 offsite 步骤（`rclone copy` / `aws s3 cp`），**以 env 开关控制**，未配置时**明确打印 SKIP**（不允许静默）。
- **判据**：备份包所在挂载点 `df` 显示的设备 **≠** `postgres_data` 卷所在设备；offsite 开关打开后 `rclone ls` 能看到当日对象。

### B1.3 修 dev 备份脚本的"静默空备份"（P2 · 独立可交付）

- **改动**：`scripts/backup_database.sh`
  ```bash
  set -euo pipefail        # ← 加 pipefail（同时兼容 set -u）
  # 压缩分支改为重定向，或显式检查管道首段
  if docker exec "$PG_CONTAINER" pg_dump -U synapse -d synapse -Fc -b >"$PG_BACKUP_COMPRESSED"; then
  # 之后加最小体积断言
  size=$(stat -f%z "$PG_BACKUP_COMPRESSED" 2>/dev/null || stat -c%s "$PG_BACKUP_COMPRESSED")
  [ "$size" -gt 1024 ] || { log_error "备份体积异常（${size}B）"; exit 1; }
  # 廉价校验
  docker exec -i "$PG_CONTAINER" pg_restore --list < "$PG_BACKUP_COMPRESSED" >/dev/null
  ```
  > 注：`pg_dump -Fc` 输出**已是压缩格式**，外层再 `gzip` 属重复压缩；一并简化（体积/耗时双收益）。
- **变异自证（本批必做）**：
  1. 把 `PG_CONTAINER` 指向一个不存在的容器 ⇒ 脚本**必须非零退出**（修复前会 exit 0 并记 SUCCESS）；
  2. 恢复正常 ⇒ exit 0 且备份体积 > 阈值；
  3. 故意把库名改错 ⇒ 同样必须非零退出。
- **回滚**：脚本是独立文件，`git checkout --` 即回。

### B1.4 加调度

- **改动**：提供 `docker/deploy/systemd/synapse-backup.{service,timer}`（或 `deploy/backups/crontab`）；
  文档步骤里 service **显式**传 `BACKUP_DIR`、`KEEP_DAYS`。
- **判据**：`systemctl list-timers | grep synapse-backup` 有下次触发；隔日检查备份目录出现新包。
- **备份策略**（对齐 D1）：每日全量 + WAL 连续归档 ⇒ 可 PITR 到任意时刻。

### B1.5 恢复演练（"没演练过的备份等于没有备份"）

- **产出**：`docs/runbook/DB_RESTORE_DRILL.md`（逐条命令 + 判定 + 异常分支），
  内容至少：恢复到临时实例 → `pg_restore --list` → 恢复 → **行数比对**（`events`/`rooms`/`users`）
  → 抽样一致性 → 记录表（日期 / 执行人 / 实测 RTO / 差异项）。
- **判据**：首次演练**实测** RTO ≤ 目标值（D1）；若超出 ⇒ 记录差距并调整方案（不是"演练通过"就完事）。
- **频率**：每季度一次；结果写进 `docs/audit/` 或 runbook 的"演练记录"小节。

---

## 3. B2 · 观测面补齐

### B2.1 postgres_exporter

- **改动**：`docker/docker-compose.monitoring.yml` 新增服务（与既有 `node-exporter` 同风格）：
  ```yaml
  postgres-exporter:
    # 沿用本仓 O2-7 惯例：镜像按 digest 钉版，不用浮动 tag
    image: prometheuscommunity/postgres-exporter@sha256:<PIN>
    environment:
      DATA_SOURCE_NAME: "postgresql://exporter:***@postgres:5432/synapse?sslmode=disable"
    # 只走 compose 内网，不映射宿主端口（Prometheus 同网络直连 :9187）
  ```
  安全：**只读专用账号**（`CONNECT` + `pg_monitor` 角色），凭据走 `.env`，不写死；
  同步取消 `monitoring/prometheus/prometheus.yml:51` 那行的注释并加 `job_name: postgres`。
- **判据**：
  ```bash
  curl -s localhost:9090/api/v1/targets | grep -c '"job":"postgres"'      # ≥1 且 health=up
  curl -s localhost:9187/metrics | grep -E '^(pg_stat_replication|pg_stat_archiver|pg_up)' | head
  ```

### B2.2 告警（本批核心：只加"能红"的）

| 告警 | 表达式（示意） | 备注 |
|---|---|---|
| `PostgresBackupStale` | `time() - synapse_backup_last_success_timestamp > 93600` | **§1 的兜底**。指标由备份脚本**主动上报**到栈内已有的 `pushgateway`（`docker-compose.monitoring.yml:151`）—— 备份新鲜度没有天然导出器，必须自己喂；脚本失败时不推送 ⇒ 时间戳自然变旧 |
| `PostgresArchiveFailing` | `pg_stat_archiver_failed_count > 0` 或 `rate(pg_stat_archiver_archived_count[10m]) == 0` | 归档断档比"备份失败"更隐蔽 |
| `PostgresConnectionsSaturating` | `sum(pg_stat_activity_count) / on() pg_settings_max_connections > 0.8` | **PG 侧**的连接数（区别于现有应用池告警） |
| `PostgresLongRunningTransaction` | `max(pg_stat_activity_max_tx_duration{state!="idle"}) > 300` | 长事务是 VACUUM/bloat 的头号杀手 |
| `PostgresLockWait` | `pg_locks_count{mode="ExclusiveLock"} > 0` 持续 2m | 部署期排他锁（§6.5）可见化 |

> **复制类告警（replication lag / slot backlog）留空**：当前 `pg_replication_slots = 0`，加了也只会"永远无数据"。
> 一旦上备库（§7 触发条件）再补 —— **不要为不存在的组件配告警**。
- **门禁（可自证）**：`promtool test rules monitoring/promtool/*.test.yml` 纳入 CI；
  **RED 自证**：把阈值改到"当前必然超"的值 ⇒ 测试必须红；恢复 ⇒ 绿。
- **判据**：人为 `systemctl stop synapse-backup.timer` + 改旧 mtime ⇒ `PostgresBackupStale` 进入 `firing`。

---

## 4. B3 · 资源预算一致（pids / 内存）

### B3.1 参数改动（按 D3 选一条）

- **A 路线**：`docker/deploy/docker-compose.yml` + `docker/docker-compose.yml`
  `pids_limit: ${POSTGRES_PIDS_LIMIT:-300}`（≥ 250 连接 + 7 常驻 + 8 并行 + 余量）
- **B 路线**：`docker/config/postgres.conf` `max_connections = 240`，并同步 `docker/deploy/README.md` 预算表
  （否则 `check_connection_budget.py` 会红 —— 这正是它该做的事）。
- **验收**（B 路线）：压到 240 个并发连接，无 `could not fork new process`；
  ```bash
  # 造 240 个空闲连接（示例，用 pgbench 更稳）
  for i in $(seq 1 240); do psql "…" -c "SELECT pg_sleep(30)" & done; wait
  docker exec synapse-postgres psql -U synapse -c "SELECT count(*) FROM pg_stat_activity"   # 应为 240+常驻
  ```

### B3.2 把 pids 与内存纳入既有门禁（**扩 `check_connection_budget.py`，不新建脚本**）

- **改动**：在 `scripts/ci/check_connection_budget.py` 现有四条（代码默认 / yaml / compose / 文档四方对齐）
  之后加两条断言，**读真实运行配置**、缺数即 `exit 2`（沿用该脚本既有风格）：
  1. `pids_limit ≥ max_connections + 常驻进程数(≥7) + max_worker_processes`；
  2. `shared_buffers + work_mem × 并发上限 + maintenance_work_mem ≤ 0.8 × mem_limit`
     （并发上限 = `max_connections`，或显式登记一个更小的"预算并发数"并写明理由）。
- **挂载**：已在 `ci.yml:174`（无需新增 workflow）。
- **RED 自证**：把 `pids_limit` 从 256 改成 200 ⇒ 门禁必须红并打印超预算算式；恢复 ⇒ 绿。

### B3.3 `work_mem` 重算

- 先采样真实并发（`pg_stat_activity` 采样 + `log_temp_files` 统计），再定值；
  缺数据时用保守组合：`work_mem = 8MB` **且** 保留 §B3.2 的内存断言（防止有人只改一半）。

---

## 5. B4 · 迁移可靠性

### B4.1 容错模式按**实际退出码**记录成败（P2 最高价值项）

> ⚠️ **2026-10-15 落地时的更正：这个标题本身就是错的判据。** 实测（`psql < 含 SELECT 1/0 的脚本`）：
> 不带 `ON_ERROR_STOP` 时**语句报错 psql 仍返回 0**；只有加 `ON_ERROR_STOP=1` 才返回 3。
> 而容错模式恰恰不能用 `ON_ERROR_STOP`（它要"个别语句失败不中断"）⇒ **退出码在这里恒为 0，
> 按退出码判成败等于没判**。实际采用的判据是 **stderr 里的 `ERROR:` 行数**（见下）。
> 另外 `STOP_ON_MIGRATION_FAILURE` **早已存在**（`docker/entrypoint.sh` / `docker/docker-entrypoint.sh`，
> 默认 `true`）—— 不需要新开关，只要让 `db_migrate.sh` **如实返回非零**即可。

- **现状**：`docker/db_migrate.sh`，`psql … || true` 之后**无条件** `record_migration … TRUE`；
  叠加 `init_database()` 的"checksum 相同即 early return" ⇒ 半迁移可被**永久固化**为已应用。
- **实际改动（已实施）**：
  ```bash
  # 容错模式：捕获 stderr，按其中的 ERROR 行数判成败
  psql_db <"$file" >/dev/null 2>"$err_file" || psql_rc=$?
  error_count="$(grep -c '^ERROR' "$err_file" 2>/dev/null || true)"
  if [ "$psql_rc" -eq 0 ] && [ "$error_count" -eq 0 ]; then
      record_migration … TRUE  …
  else
      record_migration … FALSE …      # ← 记实话
      # 打印失败语句，并 return 1；由 entrypoint 的 STOP_ON_MIGRATION_FAILURE 决定是否阻断
  fi
  ```
  同时修掉 `init_database()` 里"调用后 `return 0`"的吞错（它让失败在**记账路径**上被吃掉）。
- **判据（已自证）**：见 §13.8 —— 旧脚本 `exit=0 / is_success=true`（缺陷复现），
  新脚本 `exit=1 / is_success=false`，且**出错后的语句仍被执行**（容错语义保留）。
- **回滚**：单文件改动。

### B4.2 shell 迁移路径补 advisory lock（与 Rust 侧**同一把 key**）

> ⚠️ **2026-10-15 落地时的两处更正**：
> 1. **不能只在"应用迁移文件"那次会话里取锁**：一次 `migrate` 会开好几个 psql 会话
>    （记账建表 → 应用文件 → 写账本），只锁中间那段**覆盖不到记账阶段**。实测并发两次
>    `migrate` 时，后到的会在 `ensure_schema_migrations_table` 撞目录竞态：
>    `ERROR: duplicate key value violates unique constraint "pg_type_typname_nsp_index"`。
>    ⇒ 改为**一个长驻会话在整次命令期间持锁**（FIFO + 哨兵，见 §13.8）。
> 2. 顺带发现一个**自死锁陷阱**：既有长驻持有者，又在应用文件的会话里再取同一把锁，
>    两个会话会互相等待（持有者只在本命令结束才释放）⇒ 落地时必须**只保留一处取锁**。
>
> 另一处实现细节：macOS 自带 `/bin/bash` 是 **3.2**，不能用 `exec {fd}>` 动态 fd 语法，
> 必须用固定 fd（本脚本 shebang 就是 `#!/bin/bash`）。

- **改动**：`docker/db_migrate.sh` 在主流程外层取/放 `pg_advisory_lock(<同 key>)`；
  key **从 Rust 常量派生同一数值**（`synapse-services/src/database_initializer/mod.rs`），
  并在两处加注释互相指认，避免两套协议各自漂移。
- **实际做法**：两侧都用**同一条表达式**
  `hashtext(COALESCE(current_database() || ':' || current_schema(), current_database() || ':'))::bigint`
  派生 key（不是各自硬编码魔数 —— 魔数会漂移，表达式不会）；
  shell 侧由 `acquire_migration_lock()` 起一个后台 psql 从 FIFO 读语句并持有会话级锁，
  等到哨兵出现才继续；`trap … EXIT INT TERM` 保证任何退出路径都释放。
- **判据（已自证，见 §13.8）**：并发两次 `migrate` ⇒ **两次都成功、零目录竞态**（带锁）；
  把取锁调用变异掉 ⇒ **复现目录竞态**（`exit=(0,3)`）；外部先持同一把锁 ⇒ migrate **阻塞等待**
  （墙钟 ≥ 持有时长）⇒ 证明与 Rust 侧是同一把 key。

### B4.3 让 `CREATE INDEX CONCURRENTLY` 失败**可见**

> ✅ **2026-10-15 已实施**。落点与本节原写的位置不同：schema 健康检查的**真正实现**在
> `synapse-storage/src/schema_health_check.rs`（`src/server/database.rs` 只是启动期的调用方与日志）。

- **改动（已实施）**：
  - `schema_health_check.rs`：`HealthCheckResult` 新增 `invalid_indexes`，新增
    `check_invalid_indexes()`（`pg_index` 上 `NOT indisvalid OR NOT indisready`，限定 `current_schema()`），
    非空即 `passed = false` + `error!`（**critical**，与缺表/缺列同级）；
  - `src/server/database.rs`：启动期把它与缺表/缺列一起视为致命（fail closed，`SKIP_SCHEMA_CHECK`
    仍是逃生门），并在修复指引里给出 `DROP INDEX CONCURRENTLY` 的处置线索；
  - `src/bin/schema_health_check.rs`：汇总里加计数，并**逐条列出索引名**（处置要按名字删，
    只看计数不知道该删哪个）。
- **为什么按 critical 处理**：`indisvalid = false` 时优化器不会用它（退化为顺序扫描），
  而若是 **UNIQUE** 索引，**唯一性根本没有被强制** —— 这是正确性问题，不只是性能问题。
- **判据（已自证，见 §13.8）**：造一个必失败的 `CREATE UNIQUE INDEX CONCURRENTLY`（撞重复值）
  ⇒ PG 侧留下 `indisvalid=false indisready=false` 的索引；健康检查从 `Invalid indexes: 0`
  变为 `Status: FAILED / Invalid indexes: 1` 并列出名字；删掉后回到 0。
- ⚠️ 新增的 `query_scalar!` 会改变静态计数 ⇒ 已同步 `BASELINE_STATIC` 1353 → **1354** 与
  `SQLX_CACHE_ENTRIES` 1319 → **1320**（见 SQLx 方案文档的数字块）。

### B4.4 3 条 CHECK 改 `NOT VALID` + 独立 `VALIDATE CONSTRAINT`（§6.5）—— ✅ 已实施

- **改动**：基线中 `ck_rooms_room_id_format` / `ck_events_event_id_format` / `ck_room_memberships_valid`
  的 `ADD CONSTRAINT … CHECK (…)` 改为 `ADD CONSTRAINT … CHECK (…) NOT VALID`，
  再在**同文件后段**（新增的 "P1-2b" 段，独立语句）`ALTER TABLE … VALIDATE CONSTRAINT …`。
- ⚠️ **落地时更正了本方案里锁语义写反的地方**（原文说"`NOT VALID` 只取 `ShareUpdateExclusive`"）：
  实测**两种写法的 `ADD` 都取 `AccessExclusiveLock`**，区别是**扫不扫表** ——
  `NOT VALID` 不扫 ⇒ 排他锁窗口 ≈ 0；`VALIDATE` 才取 `ShareUpdateExclusiveLock`（不阻塞读写）承载扫描。
- **判据（实测，4,000,000 行表 + 关闭 autovacuum）**：

  | 写法 | `AccessExclusiveLock` 持有 | `ShareUpdateExclusiveLock` 持有 | 并发 INSERT 30 笔（`lock_timeout=150ms`） |
  |---|---|---|---|
  | 原：`ADD … CHECK (…)` | **603 ms** | 0 | **27 成功 / 3 被锁阻塞** |
  | 新：`ADD … NOT VALID` + `VALIDATE` | **6 ms** | 785 ms（扫描在这里） | **30 成功 / 0 被锁阻塞** |

  ⇒ 排他锁窗口缩短约 **100×**，被阻塞的写事务 **3 → 0**。两种写法的**总工作量相同**
  （约 1.2 s DDL 墙钟），差异只在"持什么锁"。
- **指纹**：基线 +2570 字节（226557 → 229127）⇒ `EXPECTED_BASELINE_FINGERPRINT`
  `306314fd1316fa2d` → **`9e954f0c9c50755e`**（按 R10 先自检旧值）。
- ⚠️ **发现的一处守卫盲区 —— 已同日补上守卫**：模板路线**看不到**这种遗漏 ——
  隔离机制克隆用 `CREATE TABLE … (LIKE … INCLUDING ALL)`（`synapse-common/src/test_isolation.rs:897`），
  CHECK 约束在**空克隆**上被重建为 `convalidated = true`，所以 `monitoring::db_tests`
  无法察觉模板里的 `NOT VALID`（已实测：变异模板 `convalidated=false`，但测试仍通过）。
  ⇒ 判据只能落在**真实库**上。**处置（同日完成）**：给 `scripts/report_orphan_db_objects.py`
  增了「`NOT VALID`（未被验证）的 CHECK 约束」一节（该脚本本就有"VALIDATE 死引用"一节），
  并把 `--strict` 的退出条件扩到覆盖它；`room_sticky_events` 那两条**有意**的 `NOT VALID` CHECK
  按约束名进白名单（依据见基线 5431-5432 的原注释）。自证：真库/全新库 ✅0、`--strict` exit 0；
  变异（把 `ck_rooms_room_id_format` 改回 `NOT VALID`）⇒ ⚠️**1** 并点名该约束、`--strict` **exit 1**；
  还原 ⇒ 0。运维说明见 `docs/runbook/DB_ORPHAN_OBJECT_AUDIT.md` §5.1。

---

## 6. B5 · 对象卫生与守卫

### B5.1 孤儿对象可见化（两条路，**推荐 ①+②**）

- **① 报告脚本（低风险，先上）**：`scripts/db_orphan_objects.sh`（对**目标库**只读执行）：
  比对"基线解析出的对象集合"与"库里实际对象集合"，输出双向差集
  （库多出来的 = 孤儿；基线有而库里没有 = 未应用）。运维按周期跑，输出归档到 `docs/audit/`。
- **② 让基线收敛（需备份就位）**：把 4 张废弃表以 `DROP TABLE IF EXISTS` 折入基线
  （基线本就是"幂等重放"语义，`CREATE`/`CREATE INDEX` 都已 `IF NOT EXISTS`，加 `DROP` 是自然延伸）。
  - **风险**：老库上这些表可能**有数据**（本机实测 0 行），DROP 是破坏性操作 ⇒
    **必须**：a) 先跑 ① 出清单；b) 备份就位（B1）；c) 逐表 `pg_dump -t` 存证；d) 再 DROP。
  - 门禁：新增 `scripts/ci/check_baseline_drops_are_declared.sh`（基线里的 `DROP TABLE` 必须逐条登记在
    白名单文件里，防"顺手 DROP 一张有数据的表"）。

### B5.2 修重复索引守卫的覆盖盲区（§8）

- **改动**：`scripts/check_baseline_consolidation.py::_INDEX_DEF_RE`
  ① 支持嵌套括号（表达式索引：`ON users(lower(username))`）；
  ② 支持 `INCLUDE (…)` 子句（把 INCLUDE 也纳入 key 或显式忽略并**在重复判定中标注**）。
- **RED 自证（必做）**：
  1. 在基线临时复制一条**带 `INCLUDE`** 的索引 ⇒ `duplicate_indexes()` 必须报告重复（当前报 0）；
  2. 临时复制一条表达式索引 ⇒ 必须报告；
  3. 删除临时副本 ⇒ 恢复绿。
- **注意**：`idx_events_room_stream_ordering` 与 `idx_events_sync_covering` **不要因此删掉任何一个**
  （2026-09-18 真实计划实测两者都被选中）。本项只修**检测能力**，不改索引集合；
  若守卫修好后报出这一对，按"已裁定保留"登记进白名单并附 §14.3 的链接。

### B5.3 清 4 张废弃表

依赖 B1（备份）+ B5.1①（清单）：`device_lists_outbound_pokes`、`room_state_events`
（代码里只剩注释）、`leak_alerts`、`push_devices`（全仓 0 引用）。全部 0 行。

---

## 7. B6 · 待裁定项（测量先行，不猜）

| 项 | 动作 | 判据 |
|---|---|---|
| 中文检索（§12） | `SELECT show_trgm('北京')` + `EXPLAIN (ANALYZE, BUFFERS) … ILIKE '%北京%'`；用真实中文关键词跑 2 字/3 字对照组 | 2 字查询是否走索引；若不走 ⇒ 选型（`pg_bigm` / `zhparser` / 应用层分词），**产出选型对比表**（含 alpine 镜像可用性、引入成本） |
| JSONB 索引覆盖（§11） | 从 `pg_stat_statements` + 代码里提取"真正出现在 WHERE 中的 JSONB 列"清单 | 102 列全覆盖标注：被过滤 ⇒ 必须有 GIN 或书面豁免；仅存储 ⇒ 无需索引 |
| 7 组前缀冗余索引（§9） | 复用 `DB_REVIEW_2026-09-17.md` §12.1 的测量口径（`pg_relation_size` + `EXPLAIN (ANALYZE, BUFFERS)`） | 每组给出"删/留"裁定 + 依据；**留**的要写进索引台账，**删**的走 B5 的 DROP 流程 |

---

## 8. 门禁矩阵（新增/修改一览）

| 门禁 | 类型 | 挂载点 | RED 自证 |
|---|---|---|---|
| `check_connection_budget.py`（扩 pids + 内存两条断言） | 修改 | `ci.yml:174`（已挂） | `pids_limit` 改小 ⇒ 红 |
| `promtool test rules`（告警单元测试） | 新增 | 新 workflow 或 `ci.yml` repo-sanity | 阈值改到必然超 ⇒ 红 |
| `check_baseline_consolidation.py::duplicate_indexes`（修 INCLUDE/表达式） | 修改 | `preflight.sh`（已由 `run` 调用）/ CI | 复制 INCLUDE 重复索引 ⇒ 红 |
| `check_baseline_drops_are_declared.sh` | 新增 | `preflight.sh` | 加一条未登记的 `DROP TABLE` ⇒ 红 |
| `db_orphan_objects.sh` | 新增（**运维周期跑**，非 CI） | 手工/定时 | 对当前老库跑 ⇒ 必须列出 4 张 |
| schema 健康检查（INVALID 索引） | 修改（代码） | 启动路径 | 造 INVALID 索引 ⇒ 启动报错 |
| `sync`：备份脚本"空备份"回归 | 修改（脚本） | B1.3 变异自证（三步） | 停容器跑备份 ⇒ 非零退出 |

> **接线原则**：能在 `preflight.sh` 里以 `run "<名>" <命令>` 低成本跑（~ms 级）的就放它，
> 并注意该脚本的既有教训——**"只在改动集命中时才跑"会绕过真故障形态**（2026-10-14 已改成每次都跑）。
> 新增门禁若依赖外部服务（promtool / DB），放 CI，不要放 preflight。

---

## 9. 明确「不做」的项 + 触发条件

| 不做 | 理由（沿用 2026-09-17 §12.5） | 触发条件 |
|---|---|---|
| `events` 等追加型大表分区 | 会牵动主键语义（分区键须进 PK）、应用侧需逐热点查询审分区裁剪；当前无生产量级数据 ⇒ 收益兑现不了，风险先兑现 | 单表 > 50GB 或 `DROP PARTITION` 的收益明确 > 改造成本 |
| 备库 + 自动切换（Patroni 等） | 单机 + 备份可满足 RTO ≤ 30min；**无 fencing 的自动切换风险大于收益**（脑裂双写） | D1 要求 **RTO < 5min**，或出现读扩展/离线分析需求 |
| 同步复制（`synchronous_standby_names`） | 当前无备库；即便有备库，同步复制会把备库抖动放大为主库提交延迟 | 上备库 **且** D1 要求 RPO ≈ 0 |
| 拆分 5408 行 baseline | 单一真相源的价值大于拆分的收益（无从零收益中获益的证据） | 迁移清单突破单文件可维护规模，或需要"增量迁移"工作流 |

> **已有基础设施提示**：`.github/workflows/db-replica-consistency.yml` 已存在（PRIMARY/REPLICA 逻辑校验和比对），
> 但依赖未配置的 secret ⇒ 当前按 design **SKIP**。一旦上备库，配好 secret 即可启用 —— 无需新建。

---

## 10. 排期建议

```
周内（D1/D3 拍板后）
  第 1 天   B1.3（脚本修复 + 变异自证）→ B1.2（异地）→ B1.1（WAL 归档，重启前先全量备份）
  第 2 天   B1.4 调度 + B1.5 首次恢复演练（实测 RTO）
  并行      B2.1/B2.2 观测面（不同文件，独立提交）
  第 3 天   B3（参数 + 门禁扩展 + 变异自证）
两周内
  第 4–5 天 B4.1 → B4.2 → B4.3（B4.4 可后置）
  第 6 天   B5.2（守卫修复 + RED 自证）→ B5.1① （报告）→ B5.1②/B5.3（备份就位后清表）
  第 7 天   B6 三项测量 + 裁定
```

---

## 11. 验收总表（"做完"的定义）

| # | 验收项 | 命令/判据 | 通过标准 |
|---|---|---|---|
| 1 | 归档可用 | `SELECT failed_count FROM pg_stat_archiver` | `0`，且 `archived_count` 持续增长 |
| 2 | 备份非空且可读 | B1.3 的三步变异自证 | 停容器 ⇒ 非零退出；正常 ⇒ 体积达标且 `pg_restore --list` 成功 |
| 3 | 备份异地 | `df` 对比 + offsite 清单 | 备份不在 `postgres_data` 同一设备 |
| 4 | 恢复演练 | runbook 记录 | 实测 RTO ≤ D1 目标，差异项已登记 |
| 5 | PG 指标可见 | `/api/v1/targets` | `job=postgres` up |
| 6 | 告警可红 | `promtool test rules` + 人为触发备份过期 | 变红并进入 firing |
| 7 | 资源预算 | 扩展后的 `check_connection_budget.py` | 绿；且改坏任一项即红 |
| 8 | 迁移不撒谎 | 注入必错 DDL | `is_success=false` 且部署中止 |
| 9 | 半建索引可见 | 造 INVALID 索引 | 启动日志报错 |
| 10 | 孤儿对象可见 | 跑 `db_orphan_objects.sh` | 先列出 4 张（清理前）；清理后双向差集为空 |
| 11 | 守卫无盲区 | 复制 INCLUDE/表达式重复索引 | 守卫红；移除后绿 |
| 12 | 中文检索有结论 | `EXPLAIN` 对照 | 2 字查询的走向已实测并给出选型裁定 |

---

## 12. 风险与未决

1. **重启窗口**：`archive_mode` 与 `max_connections` 都需重启 PG ⇒ 必须**串到同一次窗口**，
   且窗口前完成一次全量备份（B1.3 先落地）。
2. **归档目录磁盘打满**：WAL 归档若与 PGDATA 同盘，会把数据库拖死 —— 必须独立卷 + 剩余空间告警
   （B2.2 建议追加一条 `node_filesystem_avail_bytes` 的 PG 卷专用告警）。
3. **`NOT VALID` 的行为差异**：`ADD CONSTRAINT … NOT VALID` 对新增数据的约束**仍然生效**，
   只跳过存量校验 —— 但若存量确有脏数据，`VALIDATE` 会失败并**保持 NOT VALID**（此时约束对新数据有效、
   对旧数据未验证）。需在 B4.4 里明确"VALIDATE 失败怎么办"（修数据 vs 保留 NOT VALID + 登记）。
4. **未验证**（需使用方实测）：宿主机/Docker 卷的 fsync 语义（`wal_sync_method=fdatasync` 在 VM 叠加层
   的持久性）；`pids_limit` 的 cgroup 计数口径（是否含内核线程）。**这两条直接决定 RPO 的真实性与 §4 的取值**。
5. **本方案的测量依赖生产数据**：B3.3、B4.4、B6 的最终裁定必须在**有量级数据**的库上复测；
   本机 dev 栈（56k events）只能验证"方法可跑"。
6. **并发会话**：本仓长期有多个会话并行编辑，**只 `git add` 自己的文件**（显式 pathspec），
   提交后核 `git status`；文档类改动注意历史上有"陈旧快照回写"的记录（内容以 git 为准）。

---

## 13. 执行记录（2026-10-15）

### 13.1 四个待拍板数的采纳值

| # | 决策 | 采纳值 | 理由 |
|---|---|---|---|
| D1 | RPO / RTO | **RPO ≤ 15min / RTO ≤ 30min** | 单机自建规模；PITR 落地后"数据损坏/误删"类可到分钟级，"整机损毁"类取决于异地同步频率（每日 ⇒ ≤24h），**两个数要分开说，不要用一个 RPO 糊过去** |
| D2 | 异地落点 | **先把落点移出数据卷 + rclone 钩子**（`BACKUP_OFFSITE_REMOTE`，未配置时**显式 SKIP**、失败**非零退出**） | 不引入尚未就绪的对象存储依赖，但把"异地"变成一条命令；等有了存储只填 env |
| D3 | pids vs max_connections | **A：`pids_limit` 256 → 300** | 保住已文档化的 `50×4+20=220 ≤ 250` 连接预算，改动面最小（只动 compose）；走 B 路线要同步 conf + 文档 + 门禁三处 |
| D4 | `work_mem` | **16MB → 4MB**，预算表登记"预期并发 100" | `256 + 4×100 + 64 = 720MB ≤ 0.8×1536 = 1228MB`；按 `max_connections` 算最坏情况不是预算而是灾难复述 |

### 13.2 已执行

| 批次 | 提交 | 内容 | 自证 |
|---|---|---|---|
| **B1.3 / B1.2** | `013f9e9dd` | 修 dev 备份脚本「静默空备份」（`set -eo pipefail`、去管道用重定向、**体积下限** + `pg_restore --list` 校验）、`BACKUP_DIR` 默认移出 `/tmp`、日志目录前置创建、可选异地同步（rclone，未配置显式 SKIP） | **三阶段变异自证**：老脚本 + 桩（pg_dump 必败）→ `exit 0` + **20 字节空归档**（缺陷复现）；新脚本 + 同一桩 → `exit 1` 且打印 pg_dump 原始报错；真环境 → `exit 0`、18MB、`pg_restore --list` 通过 |
| **B1.1 / B1.4 / B1.5** | （本批） | `archive_mode=on` + `archive_timeout=900s` + `wal_keep_size=256MB`；归档落点 `./wal_archive` **在 PGDATA 之外**（两个 compose 都挂了卷、`deploy.sh` 建目录、`.gitignore` 登记）；systemd unit/timer 模板 + crontab 备选；`docs/runbook/DB_RESTORE_DRILL.md`（含 PITR 步骤与异常分支） | **实测**：① 重建容器 4s 内 healthy，数据完好（events 56246 / rooms 7868）；② `pg_switch_wal()` 后 `archived=1 failed=0`，宿主目录出现 16MB 归档段 ⇒ 归档链路真通；③ **恢复演练 PASS**：5s 恢复，6 张表行数 + `public` 索引数（693）**逐项相等**，演练库已清理 |
| **B3** | `cd59b60fc` | `pids_limit` 256→300、`work_mem` 16MB→4MB、预算表补两行、`check_connection_budget.py` **新增 pids 与内存两条不变式** | **RED 自证**：pids 300→200 ⇒ 红；work_mem→16MB ⇒ 红；README pids 300→301 ⇒ 命中"文档与运行配置漂移"；还原 ⇒ 绿 |
| **B5.2** | （本批） | `_INDEX_DEF_RE` 改为整条语句 + **配平括号**解析键列（覆盖 343/343 条定义）；新增 `covering_pair_indexes()` 让同键「覆盖 vs 普通」对**可见**，已裁定项登记 `scripts/index_covering_pair_allowlist.txt` | **RED 自证**：复制一条带 `INCLUDE` 的索引 ⇒ 旧守卫 `exit 0`（盲区）/ 新守卫 `exit 1`；复制一条表达式索引 ⇒ 同样 0/1；还原 ⇒ 绿 |
| **B2** | （本批） | 只读角色脚本 + `synapse-postgres-exporter` + `postgres`/`pushgateway` 两个 job + 新告警组 `postgres-server-health`（6 条）+ 备份脚本上报 pushgateway + `promtool` 门禁进 CI | **端到端实测**：角色正向可读/负向不可写；两目标 `health=up`；6 条规则 `state=inactive`+`health=ok` 持续求值；备份指标已入 Prometheus。**RED 自证**：改坏归档阈值/连接阈值/删除 `state!="idle"` ⇒ 测试分别判红，还原 ⇒ 绿 |
| 文档 | `7997e700d`、`c6a2d0cd8` | 审查报告 + 本方案 + 执行记录 | preflight 全绿（aspell / markdownlint / 章节棘轮 / 数字随访 / 契约 gate / 可信度守卫） |

推送：`426e82999..7997e700d`（pre-push：clippy `--all-features` + cargo-deny + cargo-machete 全绿）。

### 13.3 未执行（及原因）——下一步

| 项 | 未执行原因 | 解除条件 |
|---|---|---|
| ~~B1.1 WAL 归档~~ | ✅ **已落地并实测**（本批）：归档已开启、`archived=1 failed=0`、落点在数据卷之外 | — |
| ~~B1.4 调度~~ | ✅ 已提供 systemd unit/timer + crontab 备选（安装说明见 `docker/deploy/systemd/README.md`） | — |
| ~~B1.5 恢复演练~~ | ✅ 首次演练已完成（**实测 RTO = 5s**，行数逐项一致），记录在 `docs/runbook/DB_RESTORE_DRILL.md` §2.3 | — |
| B1.2 异地副本 | 脚本钩子已就绪，但**落点未定**（需一个真实的异地存储） | D2 定下落点后填 `BACKUP_OFFSITE_REMOTE` |
| **B2 观测面** | ✅ **已落地**：exporter + 两 job + 6 条告警 + 备份新鲜度上报 + promtool 门禁。**已补齐（§13.10）**：Grafana 补了 `postgresql.json`（20 个面板，23 个指标名逐个在 Prometheus 里验过有数据），并修掉 `check_dashboard_metrics.py` 的一处**误报**（它把动态模板注册的指标记成违规 ⇒ HEAD 的 CI 由此变红）。**仍未做**：复制延迟/槽积压告警（**故意不做**：当前无备库，配了只会永远无数据） | 上备库后补复制类告警 |
| ~~B4 迁移可靠性~~ | ✅ **全部落地**（§13.8 + §13.9）：B4.1 容错模式按 `ERROR` 行数记成败 + 传播失败；B4.2 整次命令持**与 Rust 同一表达式**的 advisory lock；B4.3 半建索引（`indisvalid=false`）列为 critical；**B4.4** 三条 CHECK 改 `NOT VALID` + 独立 `VALIDATE`（排他锁窗口 603 ms → 6 ms、被阻塞写 3 → 0；并发现"模板路线看不到 `NOT VALID`"的守卫盲区，已登记） | 无遗留子项 |
| ~~B5.1① 孤儿对象报告~~ | ✅ **已落地**（§13.4）：`scripts/report_orphan_db_objects.py` + `docs/runbook/DB_ORPHAN_OBJECT_AUDIT.md` | — |
| ~~B5.3 清 4 张废弃表~~ | ✅ **已落地**（§13.5）：全流程走完并复核归零 | — |
| B6.2 / B6.3 | 需要真实查询清单 / 测量数据（JSONB 清单、7 组前缀冗余索引登记） | 使用方数据 |
| ~~B2+. 监控配置"双份副本收敛"~~ | ⚠️ **我的判断错了，已收回**：`monitoring/` 不是陈旧副本，而是 **dev 栈**的配置（被 `docker/docker-compose.monitoring.yml` 真实挂载、`scripts/deploy-monitoring.sh` 校验）；`docker/deploy/prometheus/` 归 **deploy 栈**。两者**属于两个栈，不能删**。真问题是 dev 那份带 6 处损坏模板 ⇒ **已修**；门禁口径也改为"用**所有已部署版本里最新的** promtool 校验**全部**配置"（§13.4） | 无需决策；若将来真要合并两栈配置，那是架构决定 |
| **B6.1 中文检索** | 已实测完成（见下）；**迁移决策未定**（改 ctype 要新建库 + dump/restore + **REINDEX 3 个 `lower()` 表达式索引**） | 业务方确认"中文搜索"是否为当前交付项 |

**B6.1 实测结论（本轮已完成测量部分）**：`datctype = C` ⇒ `show_trgm('北京') = {}` ⇒ 中文模式
`%北京%` 走 Seq Scan、ASCII 模式走 Bitmap Index Scan；探针库（`LC_CTYPE='C.UTF-8'`）实测
`show_trgm('北京')` 恢复非空 ⇒ **修复路径明确且不需要换基础镜像**（alpine/musl 支持 `C.UTF-8`）。

### 13.4 本批（监控配置纠偏 + B5.1①）

本批起因是**复核上一批**，结果查出三件事，其中第一件是**我自己引入的 CI 红**：

| # | 问题 | 性质 | 处置 |
|---|---|---|---|
| 1 | **B2 引入的 `postgres` job 触发既有「死目标守卫」**：`synapse-postgres-exporter:9187` 未在 compose 里声明端口 | 🔴 **我的回归**（GitHub CI 当时是红的） | 给 exporter 加 `expose: ["9187"]`（该守卫第 286 行确实同时读 `ports` 与 `expose`）。**教训**：这道门禁**只在 CI 与 `scripts/deploy-monitoring.sh` 里跑，不在 preflight 里** ⇒ 改监控配置后必须手动跑 `python3 scripts/ci/check_prometheus_config.py` |
| 2 | **promtool 版本盲区**：门禁硬编码 `prom/prometheus:v2.47.0`，而 deploy 栈实际运行 **v2.48.1**；`promtool check rules` **自 v2.48 起才校验 annotation 模板** | 🟡 既有盲区（不是我引入） | 门禁改为**从两个监控 compose 派生钉版、取最新**用它校验全部配置；升版会被立刻吸收 |
| 3 | **dev 套 6 处损坏模板**（`{{ $values.X }}` + 对字符串做 `range` ⇒ 触发时必渲染失败） | 🟡 真缺陷 | 最小修复（改为直接引用 `$labels.X`）；live 那份本来就干净 |
| 4 | **B5.1① 孤儿对象报告** | ✅ 新交付 | `scripts/report_orphan_db_objects.py`（只读）+ `docs/runbook/DB_ORPHAN_OBJECT_AUDIT.md` |

**自证 1（版本盲区）—— 同一变异分别喂给"修复前"与"修复后"的门禁**：

| 变异 | 旧门禁（硬编码 v2.47.0） | 新门禁（派生最新） |
|---|---|---|
| 往规则文件注入 `{{ $values.stage }}` | `rc=0` —— **PASS（盲区）** | `rc=1` —— FAIL（抓住，报 3 项） |
| 删掉 exporter 的 `expose: ["9187"]` | `rc=1` | `rc=1` |
| 把 deploy 钉版改成 v2.99.0 | — | 派生值 = `prom/prometheus:v2.99.0`（升版被吸收） |
| 还原 | `rc=0` | `rc=0` |

**自证 2（孤儿报告的四道过滤都真的在挡误报）—— 同一份库、只开关过滤器**：

| 配置 | 孤儿关系 | 孤儿索引(存活表) | 孤儿约束 | VALIDATE 死引用 |
|---|---|---|---|---|
| **全部过滤开启（真值）** | **4** | **1** | **5** | **1** |
| 关掉「扩展对象」过滤 | 6 | 1 | 5 | 1 |
| 关掉「生成式模板」过滤 | 4 | 1 | **119** | 1 |
| 关掉「自动命名」过滤 | 4 | 1 | **138** | 1 |
| 不摘除 `VALIDATE` 再解析 | 4 | 1 | 5 | **0（被掩盖）** |

> 报告类工具的失败模式与门禁相反：**门禁怕漏报，报告怕误报**。这四道过滤若不生效，
> 这份报告第一眼就会给出 6/347/119/138 的噪声 ⇒ 没人会再看它。故四道都做了开关对照自证。

**只读实测结论**（详表见 runbook §3）：孤儿表 **4** 张（`device_lists_outbound_pokes`、
`room_state_events`、`leak_alerts`、`push_devices`，**均 0 行**）、孤儿索引 **1** 条
（`idx_schema_migrations_version` —— 由 `init-db.sql`/`db_migrate.sh`/Rust 引导代码
**三处**分别创建，基线里没有）、未应用约束 **1** 条（`pk_schema_migrations` 命名漂移：
基线叫这个名字、库里是 `schema_migrations_pkey`）、基线内陈旧引用 **1** 条
（`VALIDATE CONSTRAINT fk_cross_signing_keys_user`，而基线创建的是 `..._user_id`
⇒ 该守卫分支**永不触发**；实测库里该外键 `convalidated=true`，故**无实际影响**，
但名字应对齐）。

### 13.5 本批（B5.3：清理 4 张废弃表）

按 runbook §4 的五步流程执行，**每一步都有可复核的产物**：

| 步 | 动作 | 实测产物 / 结论 |
|---|---|---|
| ① | **新的全量备份**（硬前置，成功前不许动库） | `synapse_pg_20261015_161059.dump`（17 MB；`pg_restore --list` **1990 条目**校验通过）＋ Redis RDB |
| ② | **逐表存证**（唯一恢复路径，落**仓库之外**） | `pre-drop-evidence-2026-10-15/{4 张表}.dump`（`pg_dump -Fc`，`TABLE DATA` 段齐、均 0 行）＋ `SNAPSHOT.txt`（行/列/索引数：4/7 列等见文件） |
| ③ | **依赖核对 + 零副作用演练** | 无外部外键指向这 4 张；`BEGIN; DROP TABLE <4 张>; ROLLBACK;` ⇒ **DROP 成功、回滚后 4 张完好** ⇒ **无需 `CASCADE`** |
| ④ | **折入基线 + 同步指纹** | baseline 末尾新增「废弃表清理」段（4 条 `DROP TABLE IF EXISTS`，附证据注释与"为何要有这段"的说明，**+2030 字节 / 5438 行**）；指纹 `5654afa2ea622aff` → **`48e404226830f749`**（按 R10 第①条：先用**改动前实际常量**自检哈希实现 —— 改动前 221331 字节逐字节吻合 —— 再取新值） |
| ⑤ | **执行 + 复核** | dev 库执行这 4 条 ⇒ 孤儿对象 **4 表 / 7 索引 / 5 约束 → 0 / 0 / 0**；报告结论变为「表/视图与基线一致」 |

**自证**：

- **指纹守哨单测** `cargo test --test unit --features test-utils
  baseline_fingerprint_is_the_single_v12_source` ⇒ **1 passed**。它同时证明两件事：
  ① 新常量与基线字节逐字节一致；② **fixture 侧 `concat!` 的基线字符串仍哈希到同一值**
  （否则会静默铸出**第二份**全量模板 —— 这正是该测试存在的理由）。
- **基线固化守卫** `check_baseline_consolidation.py` ⇒ 通过（未被 `DROP` 段误判为重复对象）。
- **表覆盖门禁** `check_schema_table_coverage.py` ⇒ 通过（4 张表在代码侧只剩注释，无查询引用）。
- **章节棘轮 / 数字随访** ⇒ 通过。

**为什么这一步不能省成"只清本机库"**：那样换一台机器/新库还会长出同样的残留（或反过来缺表）。
折入基线后，**老库与新库收敛到同一形态**；对全新库这些 `DROP` 是 no-op。

**本批刻意不做（属 B5.4，需先定口径）**：报告残留的 3 项。最值得注意的是
`idx_schema_migrations_version` —— 它有**三个创建者、两种定义**（`db_migrate.sh:339` 建 **UNIQUE**、
`init-db.sql:34` 与 `database_initializer/mod.rs:365` 建**非唯一**），`IF NOT EXISTS` 下
**谁先跑谁赢**（实测库里是非唯一版）；而表上已有 `uq_schema_migrations_version` 保证唯一性，
UNIQUE 版属冗余。**先把"哪一种是对的口径"定下来再折入基线**，否则等于把随机性写进单一来源。
（→ **同日已在 §13.6 的 B5.4 中定下口径并收敛**：该索引被判为**纯冗余**，三方一律不建。）

### 13.6 B5.4：账本表口径收敛 + 报告全绿（2026-10-15）

**起因**：B5.3 之后报告剩 3 项。它们不是"该删的表"，而是"**同一张表的多个定义方互相漂移**"，
必须先定口径。**现已全部收敛，报告 8/8 分节全绿。**

#### 成因定论：**谁先创建，决定形态**

`schema_migrations` 有**四个**定义方（canonical 基线 ＋ `docker/db_migrate.sh` ＋
`docker/deploy/scripts/init-db.sql` ＋ `synapse-services/src/database_initializer/mod.rs`），
而**引导路径必然先建它**（迁移器要先有账本表才能记录迁移）⇒ `CREATE TABLE IF NOT EXISTS`
让"引导那份"赢，**基线的定义在实践上从不生效** —— 四处不一致长期无人发现的原因即此。

实测（用 `report_orphan_db_objects.py --dsn-env` 扫各库）：

| 库 | 创建者 | 形态 |
|---|---|---|
| `synapse_sqlx_scratch`、`synapse_sqlx_prep_v12` | **基线**直接创建 | **规范**：PK 名 `pk_schema_migrations`、无冗余索引（报告全 0） |
| 容器 `synapse`、`synapse_ci_verify`、宿主 `synapse` | **引导**先创建 | **漂移**：PK 自动名 `schema_migrations_pkey` ＋ 多一棵冗余索引 |

#### 四项处置

| # | 项 | 定论 | 处置 |
|---|---|---|---|
| ① | `idx_schema_migrations_version` | **纯冗余**（`uq_schema_migrations_version UNIQUE(version)` 已提供同列唯一 btree）＋三处定义不一致（`db_migrate.sh` 建 **UNIQUE**、另两处非唯一 ⇒ `IF NOT EXISTS` 下"谁先跑谁赢"） | **三方一律不建**；基线清理段 `DROP INDEX IF EXISTS` 收敛老库 |
| ② | `pk_schema_migrations` 未应用 | 引导用内联 `PRIMARY KEY` ⇒ 实际名 `schema_migrations_pkey`（列相同） | **三方改命名式**；基线幂等 `RENAME CONSTRAINT` 收敛老库 |
| ③ | `VALIDATE … fk_cross_signing_keys_user` | 基线查/校验旧名，实际创建的是 `..._user_id` ⇒ 整块**死代码**（实测该外键本就 VALIDATED） | 改为实际名（**纯一致性清理、零行为变化**） |
| ④ | 基线 `executed_at` 默认值 | `::BIGINT * 1000`（**先取整再乘**）⇒ 毫秒恒为 `000`（实测 `…011000` vs 另三处 `…011206`）—— **又一处"不生效的定义"里藏着的 bug** | 对齐为先乘再取整 |

#### 验证

- 容器库执行基线里那两条收敛语句 ⇒ 约束集合变为
  `pk_schema_migrations, uq_schema_migrations_version`（**与基线完全一致**）、冗余索引消失、
  `version` 的唯一保护仍在。
- 报告 **8/8 分节 ✅**（孤儿关系/未应用关系/孤儿索引×2/孤儿约束×2/未应用约束/VALIDATE 死引用 全 0）。
- 指纹 `48e404226830f749` → **`306314fd1316fa2d`**（+3196 字节；按 R10 约定**先用改动前实际常量
  自检哈希实现**——改动前 223361 字节逐字节吻合——再取新值）。
- `.sqlx`：**变更前**对 `synapse_sqlx_scratch` 跑 `--check` 得
  `OK: .sqlx 与数据库元数据一致`（→ 建立了可信基线，重生成的差异只可能来自本次改动）；
  经唯一入口 `scripts/ci/sqlx_prepare.sh` 重生成（缩容需显式 `ALLOW_CACHE_SHRINK=1`）⇒
  **缓存 1320 → 1319，被删条目恰好就是那条冗余索引的 DDL 查询**（`git status .sqlx/` 仅 1 个 `D`）。

#### 附：一处环境发现（影响"该对着哪个库操作"）

本机有**两个 PostgreSQL 实例**：容器 `synapse-postgres`（PG **16.15**，deploy 栈真数据，
**5432 未发布到宿主**）与宿主 Homebrew（PG **15.18**，`127.0.0.1:5432`，**28 个库**，含
`.sqlx` 重生成源 `synapse_sqlx_scratch`）。**`localhost:5432` 不是容器** ——
此前若误把它当容器库，会对着另一套数据下结论。判据与命令见
`.workbuddy/memory/local-env-pitfalls.md` §8。

> ✅ **当天晚些时候已按决策收敛（详见报告 §16）**：宿主两台 Homebrew PG **全部停用**
> （根因是 `brew services` 对 `postgresql@18` 的 ~10 秒重试循环，日志时间线见 §16.2），
> **容器 5432 发布到宿主回环** ⇒ `localhost:5432` 重新等价于"项目库"；
> 并在**容器内**重建 `synapse_sqlx_scratch`（`.sqlx --check` = OK）与 `synapse_test`
> （`prepare_test_db.sh` 跑通，`public` 218 表 + 模板 218 表）。
> 本节曾记的两条"暂时失效"Workflow 均已恢复。

### 13.7 口令强化：部署时生成强随机口令（2026-10-15，收官）

**起因**：§13.6 之后仍有一处**主动欠下的技术债** —— 为了恢复"`localhost:5432` = 项目库"
这一被多处硬编码的假设，当时把库口令对齐到了 `synapse`（弱值）。

**改造四步**（全部落在**部署脚本**里，不再是"记住要手工做"）：

| # | 改造 | 落点 |
|---|---|---|
| ① | 生成：**弱口令也当成"缺失"** | `docker/deploy/scripts/generate-secrets.sh` 新增 `is_weak_password()`（弱口令名单 ＋ 长度下限 16）与 `maybe_set_password()` |
| ② | 闸门：**弱口令拒绝部署** | `docker/deploy/deploy.sh` 的 `check_env_file` 调 `check-password postgres`（判据**单一实现**，不另抄名单） |
| ③ | 收敛：**口令落到已有数据卷** | 新增 `converge_db_password()`（postgres 就绪后、迁移**之前**：非 loopback 探测 ⇒ 不一致则 `ALTER ROLE` ⇒ 复验） |
| ④ | 校验：**验证必须真的经过口令** | `verify_database()` 原走容器内 `trust`（**口令错了也报"连接正常"**），改为容器 IP ＋ 显式口令 |

**工具链同步**：新增 `scripts/lib/db_credentials.sh` 作为口令的**单一来源**，接入 11 个脚本与
`Makefile`（逐项明细见 `docs/audit/DB_REVIEW_2026-10-15.md` **§16.7**）。
**CI 零影响**：CI 没有 `docker/deploy/.env` ⇒ 解析落到兜底 `synapse`，与历史行为一致。

**实测**：库口令 32 字符随机；`synapse-app` healthy 且日志 **0 条 ERROR/WARN**；
`/health` **200 ＋ `status: ok`**（该端点内部就是 `SELECT 1`）；`prepare_test_db.sh` **EXIT=0**；
约定守卫 `test_db_url_convention_tests` **9/9**。

**顺带修掉的三处既有缺陷**：① `generate-secrets.sh` 的 `update_env_file` 由 `sed -i` 改为纯 bash
（BSD/GNU 的 `-i` 语义不同，某些 shell 会把 `-i ''` 当文件名吃掉 ⇒ "密钥已生成"的日志为真但
**并没有写回 `.env`**）；② `scripts/generate_env.sh` 生成随机口令却把 `DATABASE_URL` 写死成
`synapse:synapse`（同一份输出里自相矛盾）；③ **`deploy.sh` 的日志门禁是"假绿"** ——
`grep -Ei '\b(ERROR)\b'` 的 `\b` 不属于 POSIX ERE，在 toybox/busybox 一类实现下**静默失配**，
于是 `verify_logs_clean` "分析容器日志"什么都没匹配到却打印 `SUCCESS`。已改 POSIX 字符类
＋ 新增 `verify_log_filters_are_live()` **过滤器自检**（变异自证：正则改回 `\b` ⇒ 自检立刻判红）。
自检后真栈实测：**已知=1（唯一那条是白名单内的预期 Megolm 告警）、未知=0、无 ERROR ⇒ rc=0**。

**未做**：`TURN_SHARED_SECRET`（15 字符）与 `coturn` 的门配置耦合，轮换需同时改两处并重启 ⇒
属独立变更，登记为待办而非顺手改掉。

### 13.8 B4：迁移可靠性（2026-10-15，授权后执行）

**范围**：B4.1 / B4.2 / B4.3 已做；**B4.4（3 条 CHECK 改 `NOT VALID`）** 方案 §5 自己
标注"可选、后置"（改基线要同步指纹，且当前无大表上线压力），本批不顺手改 ——
**用户授权后已于同日补做，见 §13.9**。

#### 落地时对方案的三处**更正**（照字面实现会出错）

| # | 方案原写法 | 实测/落地结论 |
|---|---|---|
| 1 | B4.1「**按实际退出码**记录成败」 | ❌ **该判据不成立**：不带 `ON_ERROR_STOP` 时 psql **语句报错仍返回 0**（`SELECT 1/0` 实测 `exit=0`；加 `ON_ERROR_STOP=1` 才 `exit=3`），而容错模式恰恰不能用 `ON_ERROR_STOP` ⇒ 退出码恒为 0。**改用 stderr 的 `ERROR:` 行数** |
| 2 | B4.1「依 `STOP_ON_MIGRATION_FAILURE` 决定是否 exit 1」 | 该开关**早已存在**并已接在 entrypoint（`docker/entrypoint.sh:130`、`docker/docker-entrypoint.sh:130`，默认 true）⇒ 只需让 `db_migrate.sh` **如实返回非零**，不必新增开关 |
| 3 | B4.2「在主流程外层取/放 `pg_advisory_lock`」 | 若只在"应用迁移文件"那次会话取锁，**覆盖不到记账阶段** —— 实测并发时后到者会撞 `pg_type_typname_nsp_index`（目录竞态）。⇒ 改为**长驻会话整次命令持锁**；并注意**不能两处同时取锁**（会自死锁）；且 macOS `/bin/bash` 是 3.2，不能用 `export {fd}>` 动态 fd |

#### 四项改动与自证

| 项 | 改动 | 自证（实测） |
|---|---|---|
| **B4.1** | `apply_sql_file` 容错分支：捕获 stderr、按 `^ERROR` 行数判成败；失败则 `record_migration … FALSE` + 打印失败语句 + `return 1`。并修掉 `init_database` 里"调用后无条件 `return 0`"的**吞错**（在"基线内容已变化 ⇒ 重放"这条路径上尤其致命） | 自证库预建 `users` 以强制走容错分支：**旧脚本**（HEAD 版）`exit=0 / is_success=true`（**缺陷复现**）；**新脚本** `exit=1 / is_success=false`；且 `proof_after_error` 表仍被创建 ⇒ **容错语义保留**（出错不中断）；干净基线 ⇒ `exit=0 / is_success=true`（不误伤） |
| **B4.2** | 新增 `migration_lock_statement()`（**与 Rust 同一表达式**，互加指认注释）+ `acquire_migration_lock()` / `release_migration_lock()`（后台 psql 经 FIFO 持有会话级锁，哨兵确认到手，`trap … EXIT INT TERM` 释放）；`main` 的 `init`/`migrate` 分支整次命令持锁 | 并发两次 `migrate`：**带锁 `exit=(0,0)`、目录竞态 ERROR=0**；把取锁调用变性掉 ⇒ **`exit=(0,3)`、复现 `pg_type_typname_nsp_index`**；外部先持**同一表达式**的锁 5s ⇒ migrate **阻塞**（墙钟 10s ≥ 5s）⇒ 证明与 Rust 侧同 key |
| **B4.3** | `schema_health_check.rs` 新增 `invalid_indexes` + `check_invalid_indexes()`（`NOT indisvalid OR NOT indisready`），非空即 `passed=false`；`src/server/database.rs` 列入启动致命项并给 `DROP INDEX CONCURRENTLY` 线索；`src/bin/schema_health_check.rs` 加计数 + 逐条列名 | 造 `CREATE UNIQUE INDEX CONCURRENTLY` 撞重复值 ⇒ PG 侧 `indisvalid=false indisready=false`；健康检查由 `Invalid indexes: 0` 变为 **`Status: FAILED` / `Invalid indexes: 1`** 并列出 `b43_bad_idx (valid=false, ready=false)`；删表后回到 0（自证残留已清理，scratch 库孤儿报告仍 8/8 ✅） |
| **附带** | `load_env` 现在也**认调用方的分项 `DB_*`**（此前只有 `DATABASE_URL` 算"调用方指定"，其余被 `.env` 覆盖） | 差分自证：同目录 `.env` 写 `DB_NAME=clobbered_by_env`、调用方传 `DB_NAME=explicit_target` ⇒ **旧脚本显示 `clobbered_by_env`**（覆盖成立）、**新脚本显示 `explicit_target`**（显式优先）。**动因是一次真实事故**：只传 `DB_NAME` 时目标被 `.env` 改成 `synapse`，"给临时库跑迁移"实际打到了在跑的应用库（已核实活库**未受影响**：`applied_ts` 未变、无 `proof_*` 表残留）。CI 零影响（`docker/.env` 是 gitignored，CI 无此文件） |

#### 数字同步

新增一条 `query_scalar!` ⇒ `BASELINE_STATIC` 1353 → **1354**、`SQLX_CACHE_ENTRIES` 1319 → **1320**
（`.sqlx` 经唯一入口重生成，仅新增 1 条）；复核时另发现 `BASELINE_DYNAMIC_TEST_INFRA` 实测 768
而基线写 769（余量 1）⇒ 按"零余量"一并收紧为 **768**。三处已同步到 SQLx 方案文档的数字块与棘轮基线文件。

---

### 13.9 B4.4：三条 CHECK 改 `NOT VALID` + 独立 `VALIDATE`（2026-10-15，同日补做）

方案 §5 原把它标为"可选、后置"。用户授权后执行，使 B4 **无遗留子项**。

#### 改动（`migrations/00000000_unified_schema_v12.sql`，+36/-3）

| # | 改动 |
|---|---|
| ① | `ck_rooms_room_id_format` / `ck_events_event_id_format` / `ck_room_memberships_valid` 的 `ADD CONSTRAINT … CHECK (…)` 末尾加 `NOT VALID`（前两条在 `DO $$` 块内，第三条是裸语句） |
| ② | 新增 "P1-2b" 段：三行 `ALTER TABLE … VALIDATE CONSTRAINT …`（独立语句，指向 `rooms` / `events` / `room_memberships`） |
| ③ | 两处交叉指认注释（"改这里必须同步 P1-2b"）+ 该段的锁语义说明块 |
| ④ | `tests/unit/test_isolation_unification_tests.rs`：`EXPECTED_BASELINE_FINGERPRINT` `306314fd1316fa2d` → **`9e954f0c9c50755e`**（基线 226557 → 229127 字节；按 R10 **先用改动前的实际常量自检哈希实现**，逐字节吻合后才取新值） |

#### 落地时更正方案里**写反**的一处

方案 §6.5 原文说 `ADD … NOT VALID`「只取 `ShareUpdateExclusive`，不阻塞读写」——**写反了**。
实测：**`ADD CONSTRAINT`（无论带不带 `NOT VALID`）都取 `AccessExclusiveLock`**；
`NOT VALID` 的作用是**不扫表**（于是排他锁窗口 ≈ 0），**扫描**才由 `VALIDATE CONSTRAINT`
在 `ShareUpdateExclusiveLock` 下做 —— 那一步才是"不阻塞读写"的那一步。

#### 判据（方案 §5 要求的两项，实测）

4,000,000 行表、`autovacuum_enabled = false`（消除 autovacuum 持 SUE 污染采样）：

| 写法 | `AccessExclusiveLock` 持有 | `ShareUpdateExclusiveLock` 持有 | 并发 INSERT 30 笔（`lock_timeout=150ms`） |
|---|---|---|---|
| 原：`ADD … CHECK (…)` | **603 ms** | 0 | 27 成功 / **3 被锁阻塞** |
| 新：`ADD … NOT VALID` + `VALIDATE` | **6 ms** | 785 ms（扫描在此） | **30 成功 / 0 被锁阻塞** |

⇒ 排他锁窗口缩短约 **100×**、被阻塞的写事务 **3 → 0**，而**总工作量不变**
（两种写法的 DDL 墙钟都约 1.2 s，差异只在"持什么锁"）。

#### 端到端与自证

| 项 | 做法 | 结果 |
|---|---|---|
| 设计前置 | 4 项语义确认 | ① `ADD … NOT VALID` 后 `convalidated=false`、`VALIDATE` 后 `true`；② **对已 valid 的约束重复 `VALIDATE` 不报错**（⇒ 裸写法安全，无需 `IF NOT convalidated` 守卫）；③ `NOT VALID` **仍拦新写**（既有违规行可通过 ADD，新增违规行仍 23514）；④ `VALIDATE` 遇既有违规行**如实失败** |
| 真实迁移路径 | `db_migrate.sh migrate` 到一次性库（fresh + replay 各一次） | 两次均 `exit=0`、stderr `^ERROR` = **0**；三条约束终态 **`convalidated=true`**；账本 `is_success=true`；孤儿报告 **8/8 ✅**（含"VALIDATE 死分支 = 0"） |
| 解析面回归 | 用 `report_orphan_db_objects.py` 的解析器对比新旧基线 | `table 218` / `index 343` / `constraint 334` / `view 4` **全部不变**；`VALIDATE 引用 1 → 4`，新增的三条正是本批三条、**无丢失** ⇒ 无文档计数需同步 |
| **变异自证** | 删掉 3 行 `VALIDATE` 后走**真实迁移路径**（`MIGRATIONS_DIR` 指向变异副本，不碰仓库基线） | 变异 ⇒ 三条 `convalidated=**false**`，且 `verify_data_integrity` 第 2 段那条查询精确列出 `rooms.…` / `events.…` / `room_memberships.…`；还原 ⇒ 三条 `true`、该查询返回"(无)" |
| `.sqlx` | 本批不新增/删除任何 SQL 查询 | `.sqlx` **零改动** ⇒ `BASELINE_STATIC` / `SQLX_CACHE_ENTRIES` 不变 |

#### ⚠️ 顺带发现的一处**守卫盲区** —— 同日补上守卫

**模板路线看不到 `NOT VALID`。** `monitoring::integrity_violation_tests` 第 1 行断言"干净模板上
没有任何发现"，看着像本项的天然守卫；但**实测：删掉 3 行 `VALIDATE` 后该测试仍然通过**。
根因已定位：隔离机制克隆走 `CREATE TABLE … (LIKE … INCLUDING ALL)`
（`synapse-common/src/test_isolation.rs:897`），CHECK 约束在**空克隆**上被重建
⇒ `convalidated = true`。实测同一约束在两个模板与克隆里的取值：

| 载体 | `ck_rooms_room_id_format` 等三条 |
|---|---|
| 合法指纹模板 `…9e954f0c9c50755e` | `convalidated = true` |
| 变异指纹模板 `…13c6b61b484ae697` | `convalidated = false` |
| 任一 per-test 克隆（`LIKE … INCLUDING ALL`） | **`convalidated = true`（不变）** ⇒ 测试看不到模板的 NOT VALID |

**处置（同日完成，不再是待办）**：给 `scripts/report_orphan_db_objects.py` 增了
「`NOT VALID`（未被验证）的 CHECK 约束」一节（只取 `contype = 'c'`，避免基线里十几条
`NOT VALID` 外键淹没信号），并把 `--strict` 退出条件扩到覆盖它；
`ck_room_sticky_events_expiry_pair` / `…_duration_positive` 按**约束名**进白名单
（依据 = 基线 5431-5432 的原注释：同块刚 `ADD COLUMN expires_at`，存量行两列同为 `NULL`
⇒ 必然满足，`NOT VALID` 只为省一次全量扫描；且它们只在**老库**上才是 `NOT VALID`）。

**该守卫的判据（实测）**：

| 场景 | 新节 | `--strict` |
|---|---|---|
| 真库 `synapse` | ✅ 0（白名单点名 2 条） | exit 0 |
| 全新库（应用当前基线） | ✅ 0 | exit 0 |
| 变异：`ck_rooms_room_id_format` 改回 `NOT VALID` | ⚠️ **1**，点名该约束 | **exit 1** |
| 变异后 `VALIDATE` 还原 | ✅ 0 | exit 0 |

运维说明见 `docs/runbook/DB_ORPHAN_OBJECT_AUDIT.md` **§5.1**（该报告分节数因此 8 → 9）。

### 13.10 B2 收尾：PostgreSQL 面板 + 修掉面板门禁的一处误报（2026-10-15）

#### 起因与两个发现

§13.3 把 B2 的「Grafana 面板」标为"可后补"。补之前先做**可达性核对**（这正是
`scripts/ci/check_dashboard_metrics.py` 存在的理由：面板引用不存在的指标名时只会安静显示
"No data"，甚至标题与语义错位后显示一个自信的错数字），于是查出两件事：

1. **PG 观测面此前零可视化**：`docker/deploy/grafana/dashboards/*.json` 里 `pg_*` 命中 **0**，
   白名单与棘轮基线也没有任何 PG 条目 —— 数据齐全（exporter + 2 job + 6 条告警）但没人看得到。
2. **门禁本身在 HEAD 上误报**：`storage-performance.json` 引用的
   `synapse_storage_stream_current_position` 被记为"三处都查不到" ⇒ 门禁 exit 1 ⇒ **CI 红**。实查：
   - 它由 `collector.create_dynamic_gauge_template("synapse_storage_stream_current_position", vec!["stream"])`
     注册（`synapse-common/src/server_metrics.rs:347`），而 `DynamicGaugeTemplate::new` **原样持有**
     传入名（`synapse-common/src/metrics.rs:523`）；
   - 运行中的 Prometheus 里 `count(synapse_storage_stream_current_position)` = **1 条序列（值 6）**
     ⇒ **面板引用是正确的**；症结是门禁的 `REGISTER_RE` 只认 `register_*` / `get_*`，扫不到动态模板。
   - 隔离验证：在 pin 到 HEAD 的临时 worktree 里跑同一门禁 ⇒ **同样 exit 1、同一条** ⇒ 与本次改动无关。

#### 交付

| # | 内容 |
|---|---|
| ① | **`docker/deploy/grafana/dashboards/postgresql.json`** —— 20 个面板，四组：可用性与容量（`pg_up` / 库体积 / WAL 体积 / 实例信息）、连接与锁（占用率、按 `state` 的连接、按 `mode` 的锁、最长事务）、WAL 归档与备份（归档段累计与失败、归档年龄、备份新鲜度 —— 阈值与 4 条告警**逐个对齐**）、事务与表级健康（提交/回滚、缓存命中率、死锁、临时文件、死元组 Top10、表体积 Top10、最久未 autovacuum、exporter 自监控） |
| ② | `scripts/ci/dashboard_metrics_allowlist` 增 postgres_exporter **22 条** + pushgateway **1 条**（`synapse_backup_*`，来源 `scripts/backup_database.sh`），每条写清来源；并注明这 22 个名字是**逐字取自运行中 exporter 的 `/metrics`**、且在 Prometheus 里逐个验过有数据 |
| ③ | `scripts/ci/check_dashboard_metrics.py` 增 `DYNAMIC_TEMPLATE_RE`，把 `create_dynamic_{gauge,counter,histogram}_template("name", …)` 也当注册点（与 `register_*` 同口径展开 `_total/_bucket/_count/_sum`），并同步模块 docstring 的"判据"段 |

#### 判据（实测）

- **指标可达性**：23 个指标名**逐个**在运行中的 Prometheus 验过有数据
  （`pg_up=1`、`sum(pg_stat_activity_count)=11`、`pg_settings_max_connections=250`、
  `synapse_backup_last_success_timestamp_seconds=1792046614`、`pg_stat_user_tables_n_dead_tup` 220 条序列）；
  `postgres` 与 `pushgateway` 两个 scrape job 均 `health=up`。
- **门禁**：修复前 `违规 1 / exit 1`（HEAD 上同样，已用隔离 worktree 复现）；修复后
  **`违规 0 / exit 0`**（已知名 713 → **756** —— 其中 23 条来自我的白名单、20 条 = 4 个动态模板名 × 5 种展开）。
  **变异自证**：把 `DYNAMIC_TEMPLATE_RE` 改成永不匹配 ⇒ `违规 1 / exit 1` ⇒ 该正则**承重**。
  新正则全仓实际命中 **4 个**名（`cache_operations_total`、`room_operations_total`、
  `stream_position`、`synapse_storage_stream_current_position`）⇒ **不是按前缀放行**。
  ⚠️ 与 `REGISTER_RE` 一致，它会扫到 doc 示例与测试里的调用（其中 2 处属此类），
  属**既有口径**，本批未收窄。
- **Grafana 端到端**：`GET /api/search?query=PostgreSQL` ⇒ `uid=postgresql`、`folder=synapse-rust`；
  再走 Grafana 的**数据源代理**跑代表性面板查询，全部返回序列：`pg_up=1`、连接占用率 `0.044`、
  备份新鲜度 `23824 s`（≈6.6 h，阈值 93600 s 之内）、缓存命中率 `1`、
  `topk(10, time() - pg_stat_user_tables_last_autovacuum)` **10 条序列** ⇒ 面板渲染时**真有数据**。
- **归属判定**：dev 栈（`monitoring/`）的 Prometheus **没有** `postgres` job
  （只有 `synapse-rust`/`node-exporter`/`prometheus`/`alertmanager`）⇒ 面板**只**放 deploy 栈，
  避免造出"必然 No data"的面板（正是该门禁要防的形态）。

---

*方案生成：2026-10-15 · 依据 `docs/audit/DB_REVIEW_2026-10-15.md`（实测取证版）*
