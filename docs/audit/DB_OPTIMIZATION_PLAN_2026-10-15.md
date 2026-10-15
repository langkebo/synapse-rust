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

- **现状**：`docker/db_migrate.sh:452-458`，`psql … || true` 之后**无条件** `record_migration … TRUE`；
  叠加 `init_database()` 的"checksum 相同即 early return" ⇒ 半迁移可被**永久固化**为已应用。
- **改动**：
  ```bash
  if psql_db <"$file" >/dev/null 2>&1; then
      record_migration "$version" "$filename" "…" TRUE "$checksum"
  else
      record_migration "$version" "$filename" "…" FALSE "$checksum"   # ← 记实话
      log_error "容错模式仍有语句失败：$filename（已记为失败，需人工核对）"
      # 依 STOP_ON_MIGRATION_FAILURE 决定是否 exit 1（deploy 栈默认 true ⇒ 阻断部署）
  fi
  ```
- **判据（变异自证）**：在基线里临时插一条必错 DDL（如 `CREATE TABLE users (x int);`）⇒
  `schema_migrations.is_success = false` **且** `STOP_ON_MIGRATION_FAILURE=true` 时部署中止；移除后恢复绿。
- **回滚**：单文件改动。

### B4.2 shell 迁移路径补 advisory lock（与 Rust 侧**同一把 key**）

- **改动**：`docker/db_migrate.sh` 在主流程外层取/放 `pg_advisory_lock(<同 key>)`；
  key **从 Rust 常量派生同一数值**（`synapse-services/src/database_initializer/mod.rs:293` 附近），
  并在两处加注释互相指认，避免两套协议各自漂移。
- **判据**：并发跑两次 `db_migrate.sh migrate` ⇒ 一个阻塞等待、无交错 DDL；日志出现"已获取迁移 advisory lock"。

### B4.3 让 `CREATE INDEX CONCURRENTLY` 失败**可见**

- **改动**：`src/server/database.rs` 的 schema 健康检查（或 `run_schema_health_check`）增加一项：
  ```sql
  SELECT indexrelid::regclass FROM pg_index WHERE NOT indisvalid;
  ```
  非空 ⇒ 记 `ERROR` 并列入"critical"（当前只查缺表/缺列，**半建索引完全不可见**）。
- **判据（变异自证）**：手工制造一个 INVALID 索引（`CREATE INDEX CONCURRENTLY` 中途取消）⇒
  启动日志报错；删除 ⇒ 通过。

### B4.4 3 条 CHECK 改 `NOT VALID` + 独立 `VALIDATE CONSTRAINT`（§6.5）

- **改动**：基线中 `ck_rooms_room_id_format` / `ck_events_event_id_format` / `ck_room_memberships_valid`
  的 `ADD CONSTRAINT … CHECK (…)` 改为 `ADD CONSTRAINT … CHECK (…) NOT VALID`，
  再在**同文件后段**（独立语句）`ALTER TABLE … VALIDATE CONSTRAINT …`。
- **收益**：`DROP` 仍取 `AccessExclusiveLock`（瞬时），但**全表校验不再阻塞写入**；
  `VALIDATE` 可另择窗口执行。
- **判据**：造百万行 `events` 副本，对比两种写法在 `pg_locks` 中的持锁时长与阻塞的写事务数。
- ⚠️ **本项可选、后置**：改动会重写基线（`EXPECTED_BASELINE_FINGERPRINT` 必须同步）；
  若近期无大表上线，可先只做 B4.1–B4.3。

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
| **B2 观测面** | ✅ **已落地**（本批）：exporter + 两 job + 6 条告警 + 备份新鲜度上报 + promtool 门禁。**未做**：① 复制延迟/槽积压告警（**故意不做**：当前无备库，配了只会永远无数据）；② Grafana 面板（告警已能叫醒人，面板属体验优化，可后补） | 上备库后补复制类告警；需要面板时再补 |
| B4 迁移可靠性 | 改动迁移路径（`db_migrate.sh` 语义 + advisory lock + INVALID 索引检测），影响面大，需单独的变异自证 | 单独一批授权 |
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

---

*方案生成：2026-10-15 · 依据 `docs/audit/DB_REVIEW_2026-10-15.md`（实测取证版）*
