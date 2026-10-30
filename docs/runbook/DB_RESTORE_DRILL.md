# 数据库恢复演练与恢复 Runbook（synapse-rust）

> 依据：`docs/audit/DB_REVIEW_2026-10-15.md` §1、`docs/audit/DB_OPTIMIZATION_PLAN_2026-10-15.md` §B1。
> **本文件是"备份是否可信"的唯一判据来源** —— 没有演练记录的备份一律按"未验证"处理。

---

## 0. 一句话判据

**备份不是"有文件"，而是"能恢复出与源库一致的数据，且这个结论有实测记录"。**

---

## 1. 现状（2026-10-15 起）

| 能力 | 状态 | 落点 |
|---|---|---|
| 每日逻辑备份 | 有脚本（`scripts/backup_database.sh`）+ 定时模板（`docker/deploy/systemd/`） | `BACKUP_DIR`（**必须在数据卷之外**） |
| Redis 备份 | 同上（RDB） | 同上 |
| **WAL 连续归档（PITR 前提）** | ❌ **已关闭**（2026-10-30 起 `archive_mode=off`，`archive_command`/`archive_timeout` 一并注销） | 无落点：bind mount 已移除，历史目录（59 GB）已删除 |
| 异地副本 | 脚本支持（`BACKUP_OFFSITE_REMOTE`），**未配置时显式 WARNING** | 待定（决策 D2） |
| 备份可读性校验 | ✅ 每次备份自动做（`pg_restore --list` + 体积下限） | — |
| **备份可恢复性（自动化演练）** | ✅ 每周一 + 手动 + 备份/迁移脚本变更时自动跑 | `.github/workflows/backup-restore-drill.yml` → `scripts/ci/backup_restore_drill.sh` |

**RPO 的两种口径（必须分开说）**

- ~~**数据损坏 / 误删 / 误迁移**：靠 **PITR** 恢复到任意时刻~~ —— **2026-10-30 起不成立**：
  归档已关闭（无消费方，见下），这类事故的 RPO = 距**上次逻辑备份**（≤24h），只能恢复到备份点，
  备份点之后的写入**不可挽回**。
- **整机 / 磁盘损毁**：RPO = 距**上次异地同步**的时间（异地未配置前，等于"全丢"）。

**为什么关掉归档（2026-10-30）**：本部署的备份走 `pg_dump -Fc`，全仓没有 pg_basebackup /
`restore_command` / PITR 恢复流程 ⇒ 归档没有消费方。而它反而在制造故障：目录 9 天涨到
59 GB（约 6.5 GB/天、只写不删），且 `archive_command` 的 `test ! -f …/%f` 守卫遇到已存在
的同名段后恒假 ⇒ 归档自 2026-10-24 起永久失败（`failed_count=26979`、995 个 `.ready` 积压、
`pg_wal` 涨到 15.5 GB 无法回收）。关闭后 `pg_wal` 立即回落到 320 MB。
重新启用的前提条件（5 条，缺一不可）写在 `docker/config/postgres.conf` 的「WAL 归档」段。

**RTO**：以本文件 §2.3 的演练实测值为准（当前 **5s**，276MB 库、逻辑备份路径）。

---

## 2. 恢复演练（每季度一次，**必须留记录**）

### 2.1 演练步骤

```bash
# ① 取最新备份
DUMP=$(ls -t "$BACKUP_DIR"/synapse_pg_*.dump | head -1)

# ② 可读性（廉价，秒级 —— 备份"能不能被工具读"和"数据对不对"是两件事，都要查）
docker exec -i synapse-postgres pg_restore --list <"$DUMP" | head

# ③ 恢复到**临时库**（绝不覆盖生产库）
docker exec -u postgres synapse-postgres psql -U synapse -d postgres \
  -c "DROP DATABASE IF EXISTS synapse_restore_drill; CREATE DATABASE synapse_restore_drill;"

# ④ 计时恢复
time docker exec -i synapse-postgres pg_restore -U synapse -d synapse_restore_drill \
  --no-owner <"$DUMP"

# ⑤ 一致性核对：**行数逐项比对**（只比一个表不算数）
for t in events rooms users room_memberships event_edges audit_events; do
  a=$(docker exec -u postgres synapse-postgres psql -U synapse -d synapse   -tAc "SELECT count(*) FROM $t;")
  b=$(docker exec -u postgres synapse-postgres psql -U synapse -d synapse_restore_drill -tAc "SELECT count(*) FROM $t;")
  [ "$a" = "$b" ] && echo "OK   $t $a" || echo "FAIL $t 源=$a 恢复=$b"
done
# 索引/约束也要比（数据对了但索引丢了，性能会塌）
docker exec -u postgres synapse-postgres psql -U synapse -d synapse_restore_drill -tAc \
  "SELECT count(*) FROM pg_indexes WHERE schemaname='public';"

# ⑥ 清理（演练库不留痕）
docker exec -u postgres synapse-postgres psql -U synapse -d postgres -c "DROP DATABASE synapse_restore_drill;"
```

### 2.2 验收标准

| 项 | 标准 |
|---|---|
| 备份可读 | `pg_restore --list` 成功（非 0 即判失败，**不要**用"文件大小看着对"代替） |
| 数据一致 | 关键表 + `public` 索引数**逐项相等** |
| 实测 RTO | ≤ 目标值（当前目标 30min；实测已远低于） |
| 差异项 | 任何不一致都必须记录原因并定后续动作，不允许"大概差不多" |

### 2.3 演练记录

| 日期 | 执行人 | 备份文件 | 库大小 | 实测 RTO | 一致性 | 备注 |
|---|---|---|---|---|---|---|
| **2026-10-15** | AI（自动） | `synapse_pg_20261015_135329.dump`（17MB） | 276MB | **5s** | ✅ events 56246 / rooms 7868 / users 116 / room_memberships 7910 / event_edges 48380 / audit_events 110329 / public 索引 693 —— **全部相等** | 首次演练；同日启用 WAL 归档（`archived=1 failed=0`），归档段已落宿主目录 |

### 2.4 自动化演练车道（CI，2026-10-30 起）

`scripts/ci/backup_restore_drill.sh` 在单个 job 内做完整闭环：应用统一基线 schema →
灌种子数据（`restore_drill_probe`，默认 5000 行 + 索引）→ `pg_dump -Fc` →
`pg_restore` 到临时库 → **逐项比对**表行数 / 索引定义 / 约束 / 触发器 / 序列值 / 扩展 →
探针表行数与内容校验和 → 清理。任一项不一致即非零退出，差异文件（`fingerprint.diff`）
作为 artifact 保留。

**它证明什么、不证明什么**（两种口径不要混用）：

- 证明：备份格式与恢复链路可用，且结构与数据**逐项**一致；schema 改动若破坏可恢复性会当场红。
- 不证明：生产数据量下的 **RTO**，以及"备份内容与当前线上库一致" —— CI 用的是空 schema +
  合成种子数据。这两点仍以 §2.1 的人工演练（真实备份 + 真实库）为准，每季度一次、留记录。
- 异地副本仍未纳入本车道（§5.1 的遗留风险不变）。

---

## 3. PITR：恢复到"某个时刻"（**当前不可用**）

> ⛔ **2026-10-30 起归档已关闭（`archive_mode=off`），本节流程无法执行** —— 没有 WAL 段可重放。
> 本节保留作为「重新启用归档后」的操作模板；启用前提见 `docker/config/postgres.conf`。
> 当前的恢复能力只有 §2 的逻辑备份路径（RPO ≤ 24h，RTO 见 §2.2）。

```bash
TARGET_TIME='2026-10-15 14:30:00+08'          # 要回到的时刻（事故前）
DRILL_DIR=/tmp/pitr_drill                      # 演练目录（不要在 PGDATA 内）

# ① 取一个基础备份（逻辑备份不能做 PITR 的基座；这里用 pg_basebackup）
docker exec -u postgres synapse-postgres pg_basebackup -D "$DRILL_DIR" -Ft -z -X none

# ② 准备 recovery 配置（PG12+ 用 recovery.signal + restore_command）
#    restore_command 指向归档目录，PG 会按需拉取 WAL 段
cat >"$DRILL_DIR/recovery.signal" <<EOF
EOF
# postgresql.conf 里加：
#   restore_command = 'cp /var/lib/postgresql/wal_archive/%f %p'
#   recovery_target_time = '$TARGET_TIME'
#   recovery_target_action = 'promote'

# ③ 起一个**独立端口**的实例做验证（不要停生产库）
#    验证：目标时刻之后的事件不存在、之前的存在，且没有 2PC/时间线异常

# ④ 演练通过再谈真实恢复；真实恢复同样先备份当前状态（"恢复"本身也会写数据）
```

**恢复成功后的必做动作**：确认时间线（`pg_controldata` 的 `Latest checkpoint's TimeLineID`）、
重做一次全量备份（新基座）、把这次事故与恢复过程补进上面的记录表。

---

## 4. 异常分支

| 症状 | 处理 |
|---|---|
| 备份脚本退出非零 | 看 stderr：修复前版本会把 pg_dump 的报错吞掉（`2>/dev/null` + 管道取 gzip 退出码），现已改为透传 pg_dump 原始报错。若报"体积过小"⇒ 检查 `PG_CONTAINER` / 库名 / 是否空库（空库需显式 `BACKUP_MIN_BYTES=1`） |
| `pg_restore --list` 失败 | 备份文件损坏 ⇒ 用**上一天**的备份；同时查磁盘错误与备份目录所在卷余量 |
| 归档 `failed_count > 0` | **当前不适用**：归档已关闭（`pg_stat_archiver` 计数会冻结在停机前的值，属正常，不是故障）。重新启用归档后才有此分支：目录不可写/满盘时 PG 会保留 `pg_wal` 等重试，`pg_wal` 会增长，注意磁盘 |
| `wal_archive` 挂载丢失 | **当前不适用**：bind mount 已移除、目录已删除。重新启用归档时若忘记挂载，归档会写到容器内路径并随容器重建丢失 |
| 恢复后行数不一致 | **不要**直接上生产：保留现场，先查是否归档/备份跨越了某次 DDL 变更（schema 与数据版本错配） |

---

## 5. 遗留风险（明示）

1. **无异地副本**（B2 未落地前）：主机/磁盘损毁 = 备份与数据同时丢失。`BACKUP_OFFSITE_REMOTE` 一旦配置即刻生效。
2. **无备份新鲜度告警**：定时任务失败目前只能靠 `systemctl --failed` 或日志发现（方案 B2.2）。
3. **无 PITR**（2026-10-30 起归档关闭）：任何事故最多只能回到最近一次逻辑备份，备份点之后的写入不可挽回。
