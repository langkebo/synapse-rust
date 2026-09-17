# 项目实际存在问题总清单（docs/audit 全量主张 × 代码复核）

- **复核基准**：`main` @ `4e8764c7`（复核期间 HEAD 因并发会话前进；本批修复提交为 `fba2aac6`、`86fc6cd0`）
- **被复核对象**：`docs/audit/` 下 33 篇文档、11,450 行主张
- **方法**：逐文档抽取"断言"，再用 `read`/`grep`/`git`/`psql`/脚本实跑判定：
  **仍存在** / **已修复** / **证伪**
- **判定原则**：文档自身的 ✅/🔴 标记**不可信**（本次发现多处假 ✅、假 🔴 与过期计数）。
  一切以代码/命令输出为准，每条尽量给 `file:line` 或可复现命令。

> ⚠️ **并发变更说明**：复核期间另一个会话在同一 worktree 上推进"配置单一真相源"重构
> （删除 `docker/deploy/config/`、移除 `scripts/check_config_consistency.py` 等）。
> 因此本文 §6（配置/仓库卫生）与 §7.1 中涉及配置门禁的条目**可能已随该重构改变**，
> 引用前请以当前 HEAD 复核。

> 🆕 **本文件已做三轮复核，读第一轮结论前先看最新一轮：**
>
> | 轮次 | 日期 | 复核基线 | 章节 |
> |---|---|---|---|
> | 第一轮 | 2026-09-14 | `4e8764c7` | §1–§8（多为当时快照） |
> | 第二轮 | 2026-09-15 | `66339069` | §18（新增 N-1…N-7 + 合并清单） |
> | **第三轮** | **2026-09-17** | **`dd625003`** | **§19（新增 M-1…M-6 + 合并清单，最新）** |
>
> §0.2 与 §19.3 是唯一"当前状态"口径；§1–§8 与 §18 中与之冲突的表述一律作废。

---

## 0. 结论摘要

### 0.1 最严重的问题：`migrations/` 的"已全量吸收"前提不成立

72 个时间戳增量迁移在 `a0f2819d`（"72 个已被 v11 baseline 全量吸收的增量迁移"）中被整体删除，
但**前提不成立**：

- 用**有序活集模拟**（按文件名顺序对 36 个被删正向迁移执行 `CREATE/ADD` 与 `DROP`）得到 78 个最终对象，
  其中 **23 个在 `00000000_unified_schema_v11.sql` + `00000001_extensions_v10.sql` 中完全不存在**。
- 23 个中 **2 个造成真实功能故障**（其中 1 个已用 SQL 现场复现 `42P10`）。
- 其余为数据完整性约束、合规强制（append-only 触发器）与热点索引。
- `scripts/check_baseline_consolidation.py` 现在报"吸收全部 **0** 个增量迁移"且 `EXIT=0`
  ——它已**空转**，且**未接入任何 CI**，因此发现不了这个缺口。

> **本批已修复其中 3 个**（§8），剩余 20 个待折入。

> ✅ **2026-09-17 第三轮复核：本节描述的缺口已全部关闭**（§19.2）。
> 23 个对象里 8 个约束 + 9 个索引已折入 v12 baseline 并实灌验证，其余为改名/等价覆盖。
> 另注意：`migrations/` 现在只剩 `00000000_unified_schema_v12.sql` 一个文件，v11 与
> `00000001_extensions_v10.sql` 均已内联/移出 git ⇒ 本节依赖的"有序活集模拟"判据**已结构性退役**，
> `check_baseline_consolidation.py` 的"吸收 0 个"恒值由此而来，不再是缺陷证据（见 §19.1 M-2）。

### 0.2 严重度分布

> **2026-09-17 第三轮复核后重写**（HEAD `dd625003`）。本节只保留**当前**状态；
> 逐条证据见 §19。§1–§18 的严重度表是历史快照，凡与本表冲突的以本表 + §19 为准。

| 级别 | 数量 | 状态 |
|---|---|---|
| 🔴 P0 功能/数据正确性 | **0** | 第一轮 3 项（F-1 burn `42P10`、F-2 v12/v13 CHECK、F-3 audit append-only）✅；第二轮 N-2（`events.room_id` 双 FK 中 CASCADE 存活）**本轮已修**——v12 baseline 只保留 `fk_events_room ... NO ACTION`，实灌后 `events` 上 `room_id` 恰一个 FK。**§1 整章（schema 吸收缺口）在本轮全部关闭** |
| 🔴 P0 门禁诚信 | **1 仍存在 · 1 新发现** | 已修：路由契约、覆盖率基线、perf 门禁、db-migration-gate、集成 fail-closed、T-6；**本轮新修**：clippy 在 HEAD 全绿（`EXIT=0`）、fmt/派生表**互锁解除**（生成器已过 rustfmt）。仍存在：§2.4 供应链门禁在 `repo-sanity` 恒绿。**新 M-1**：同一门禁在 `security-audit`（真装工具）路径上**转红**——`rustls 0.23.43` 命中 RUSTSEC-2026-0285 |
| 🟠 P1 schema/迁移缺口 | **0**（§1.4/§1.5 全部折入） | 8 个完整性约束 + 9/10 个热点索引已写入 v12 baseline 并实灌验证；第 10 个（`idx_federation_queue_dest_created`）由 `idx_federation_queue_pending (destination, created_ts) WHERE status='pending'` 等价覆盖。§1.6 重复索引由 5 组真实重复 → **0** |
| 🟠 P1 架构冗余 | 12 项中 **2 修复 · 8 降级/仍存 · 2 部分** | 已修：A-1（穿透 48 文件 → **0**，双门禁）、A-11（`/r0/push` 0）。大幅改善：A-2（69 manifest/935 `.route(` → **1 派生 manifest / 933**）、A-9（`src/web` 3,631 行 → `synapse-web/src` 60,368 行，根 crate 仅 4,326 行）、A-4（70 → **65** trait）。仍存在：A-3（r0=0 / **v1=223 / v3=412**）、A-5（176 → **149** glob）、A-6（22 → **26**）、A-7、A-8（287 → **249**）、A-10（假投递仍在）、A-12（storage 66 行壳保留） |
| 🟠 P1 测试隔离/模板构建 | **5 仍存在 · 3 改善** | 改善：T-1（`public` 实测 **230** 表，不再是 0——但**代码仍无条件 DROP+CREATE 不回填**，靠 CI seed 断言 `≥200` 兜底）、T-7（`synapse_test` 残留 **25 → 0**；但残留迁移到了 `synapse_ship_ci` **231** 个）、H-11（member.rs 已 clamp 两处）。仍存在：T-2（`error_count>0` 仍返回 `Ok`，仅新增"运行时初始化默认关闭"缓解）、T-3/T-8（仍有 5 个隔离实现文件）、T-4（`search_path …, public` 回退遍布）、T-5（public-wipe 逃生门仍在代码里） |
| 🟡 P2 安全/协议残留 | 11 → **4 未决** | 已修：S-1/S-2/S-4/S-5/S-9/S-12/S-13/S-14/S-15。未决：S-6（理论）、S-7（速率限制碎片化）、S-8（cache 读写不对称）、S-11（SDK 侧死条目） |
| 🟡 P2 配置/仓库/文档卫生 | **8 已修 · 2 仍存在** | 已修：H-1（`pool_size` 移除）/H-6（`server` feature 删除）/H-8（serde 反序列化测试）/H-9/H-12/H-14/H-15/H-16/H-17/H-18。仍存在：H-10（god-file 拆分前一轮已修）、H-13（计数过期：实测 **29/265/27/12/0**） |
| 🟡 P2 本机环境漂移 | **1 新发现** | **M-4**：本机 `synapse_test.public` 是 **e210c72e 之前的旧基线**——仍带 `fk_events_room_id` CASCADE 与 5 组重复索引。直接查库会读到"已修复的缺陷"，结论失真 |

---

## 1. 🔴 P0：schema 吸收缺口（由 `a0f2819d` 引入）

### 1.1 根因链

1. 项目有**两条 schema 构建路径**：
   - **测试路径**：`scripts/build_sqlx_migration_source.py` 只选 `baseline + extensions + V*`，
     并**显式丢弃所有时间戳迁移**（注释：`# All timestamp-based migrations are superseded by v8 baseline`）。
   - **部署路径**：`docker/db_migrate.sh:434` `find "$MIGRATIONS_DIR" -maxdepth 1 -type f -name '*.sql' | sort`
     **应用目录下全部正向迁移**（含时间戳迁移）。
2. 因此时间戳迁移里的对象**只存在于部署路径构建出的库**；测试路径从来没有它们。
3. `a0f2819d` 把 72 个时间戳迁移（36 forward + 36 undo）整体删除，但 baseline 生成时
   并未把其中 23 个对象折进去 → **部署路径 schema 静默退化**。
4. **该删除在测试路径不产生任何红灯**（测试路径本就没有这些对象），所以 `cargo test` 全绿
   完全不能说明这次删除是安全的。

```bash
# 证据：23 个对象在 baseline 中不存在（有序活集模拟，脚本见附录 A.1）
# 输出: ABSENT FROM BASELINE: 23
for n in fk_event_edges_prev fk_events_redacted_by fk_backup_keys_room \
         ck_events_depth_nonneg ck_events_not_before_nonneg \
         uq_backup_keys_room_session uq_device_keys_user_device_algorithm_keyid; do
  printf '%-46s %s\n' "$n" "$(grep -c "$n" migrations/00000000_unified_schema_v11.sql)"
done              # 全部输出 0
git log --all --oneline -S fk_event_edges_prev -- migrations/
# a0f2819d (删除) / 845018bd (新增，2026-09-04 DB schema 审计补丁)
```

### 1.2 🔴 功能性破坏（P0，§8 已修）

#### F-1 `burn_after_read` 批量写日志**必然报错**（已复现，已修）

- 代码：`synapse-storage/src/burn_after_read.rs:400-413`
  ```sql
  INSERT INTO burn_after_read_log (user_id, room_id, event_id, burned_ts)
  SELECT u, r, e, t FROM UNNEST(...) AS x(u, r, e, t)
  ON CONFLICT (user_id, event_id) DO NOTHING
  ```
- 唯一支撑唯一索引 `ux_burn_log_user_event`（由已删迁移 `20260901000001_burn_log_unique_index.sql`
  创建，其注释明确写着"为支持 `ON CONFLICT` 而建"）**不在 baseline 中**；
  baseline 的 `burn_after_read_log` 只有 `id BIGSERIAL PRIMARY KEY`。
- **复现**（baseline 原样 DDL）：
  ```
  ERROR:  there is no unique or exclusion constraint matching the ON CONFLICT specification
  ```
- **生产可达**：`synapse-services/src/burn_after_read_service.rs:395` 在 burn 处理主流程调用
  `storage.log_burned_event_batch(&log_entries)`。
- **为什么测试没抓到**：`burn_after_read.rs` 的 11 个 `db_tests` **全部只走单行版
  `log_burned_event`**（无 `ON CONFLICT`），批量版本零覆盖。
- 状态：测试路径**此前就已损坏**；部署路径**因 `a0f2819d` 新增损坏**。

#### F-2 v12/v13 房间的联邦加入在写库时被 CHECK 拒绝（已修）

- 能力表把 v12/v13 声明为"可 join + 可 federate"：
  `synapse-common/src/room_versions.rs:127-128` `stable_parse_only("12"/"13")`，
  定义为 `can_create:false, can_join:true, can_parse:true, can_federate:true`（`:57-70`）。
- 加入流程把远端房间版本**原样写进本库**：
  `synapse-services/src/room/membership/federation.rs:60`
  `let room_version = make_join_response.room_version.unwrap_or_else(|| "10".to_string());`
  随后 `:106` `.create_room(..., &room_version, is_public)`
  → `synapse-storage/src/room/mod.rs:124` `INSERT INTO rooms (..., room_version, ...)`。
- baseline 原约束是**硬编码 1..11 白名单**（`v11:242-247`），而被删迁移
  `20260904050000_extend_room_version_check.sql` 正是把它改成 `room_version ~ '^[0-9]+(\.[0-9]+)*$'`。
- 结果：远端 v12/v13 房间的 join 在 `INSERT INTO rooms` 处抛 CHECK 违反。

#### F-3 `audit_events` 的 append-only 触发器丢失（已修）

- 被删迁移 `20260710190001_audit_log_append_only.sql` 创建 `prevent_audit_delete()` +
  `trg_prevent_audit_delete BEFORE DELETE ON audit_events`；未显式设置
  `synapse.allow_audit_delete='true'` 时禁止删除。
- baseline 中 `audit_events` 原本无任何触发器。
- 残留证据：`synapse-storage/src/audit.rs:248` 仍在事务里
  `SELECT set_config('synapse.allow_audit_delete','true',true)`。
- 原有"守卫测试" `test_delete_events_before_bypasses_append_only_guard` **是自证的**：
  触发器不存在时它照样通过。
- **修复期间又发现同族缺陷**：`pg_trigger` 是 cluster-wide catalog，
  `WHERE tgname = 'trg_prevent_audit_delete'` 会命中**其他 schema**（如残留模板）里的同名触发器，
  导致新 schema 上跳过安装。已改为 `AND tgrelid = 'audit_events'::regclass`（提交 `86fc6cd0`）。
  这是"schema-blind 守卫"缺陷类的又一实例（与上一轮修的 5 处 `pg_constraint` 守卫同型）。

### 1.4 🟠 数据完整性约束丢失（**待折入**）

| 丢失对象 | 语义 | baseline 现状 |
|---|---|---|
| `fk_event_edges_prev` | `event_edges.prev_event_id → events(event_id) ON DELETE SET NULL`（DAG 完整性） | 只有 `pk_event_edges` 与 `fk_event_edges_event` |
| `fk_events_redacted_by` | `events.redacted_by → events(event_id) ON DELETE SET NULL`（孤儿引用） | `events` 表无此 FK |
| `fk_backup_keys_room` | `backup_keys.room_id → rooms(room_id)` | 只有 `fk_backup_keys_backup` |
| `uq_backup_keys_room_session` | `UNIQUE(backup_id, room_id, session_id)` | 无（应用层先 DELETE 再 INSERT 兜底，仅完整性缺口） |
| `ck_events_depth_nonneg` / `ck_events_not_before_nonneg` | `events.depth >= 0` / `not_before >= 0` | 无任何相关 CHECK |
| `ck_room_memberships_valid` | `membership IN (…, 'forget')` | `room_memberships` **无 membership CHECK** |
| `uq_device_keys_user_device_algorithm_keyid` | `UNIQUE(user_id, device_id, algorithm, key_id)` | baseline 保留**更严**的 `uq_device_keys_user_device_key UNIQUE(user_id, device_id, key_id)` → 低风险 |

### 1.5 🟡 热点索引丢失（仅性能，**待折入**）

> **2026-09-15 复核**：以下 10 个索引名在 v11 baseline 与 extensions 中 `grep -c` 均为 **0**，全部仍未折入
> （本行末尾原名 `idx_federated` 是笔误，被删迁移里的真名是 `idx_rooms_federated`）。

`idx_device_signatures_user_device`、`idx_device_signatures_target`、`idx_event_edges_prev_room`、
`idx_e2ee_audit_log_device`、`idx_e2ee_audit_log_room_event`、`idx_push_queue_user_pending`、
`idx_push_queue_retry`、`idx_federation_queue_dest_created`、`idx_federation_queue_retry`、`idx_rooms_federated`。

部分由等价索引覆盖（`idx_device_signatures_unique`、`idx_federation_queue_dest_status`、
`idx_event_edges_prev`），但目标查找/重试扫描/部分索引（`WHERE is_processed=false`）无等价物。
`migrations/INDEXES.md:67,123,124` 仍在描述其中两个索引 → 文档与 baseline 不一致（已知待办）。

### 1.6 🟡 附带发现：baseline 自身违反"单一真相源"

> **2026-09-15 更新**（HEAD `66339069`，详见 §18）：行号已漂移，硬编码 `'public'` 已从 3 处降到 1 处。

- **5 对完全重复的索引定义**（`IF NOT EXISTS` 使运行期无害）—— 现为
  `idx_rooms_name_trgm`(3331/3814)、`idx_rooms_canonical_alias_trgm`(3332/3815)、
  `idx_users_email_trgm`(3302/3812)、`idx_users_user_id_trgm`(3301/3813)、
  `idx_users_lower_email`(3298/3827)。
- **schema-blind 硬编码 `'public'` 剩 1 处**：`v11:4983`（`AND n.nspname = 'public'`，`uq_%` 去重循环）——
  两处 `message_log` 守卫已在 `aba8275d` 改为 `current_schema()`（现 `:5047`、`:5098`）。
  测试隔离按 `search_path = <test_schema>, public` 应用 baseline，故这一处只清 `public`、不清目标 schema。
- **另有一类更隐蔽的 schema-blind 缺陷**：`v11:4117-4120` 的"确保约束存在"块按 `conname` 判幂等，
  对同名重复无害、对**不同名同列**的 FK 完全失效 —— 见 §18 N-2 / N-3。
- 其他硬编码 `public`：`docker/db_migrate.sh:192,203,501,507`；
  `scripts/generate_logical_checksum_report.py:129,143`（无 env 覆盖）。

---

## 2. 🔴 P0：门禁诚信问题

> **§2.0–§2.8 是 2026-09-14 的快照。** 2026-09-15 复核（HEAD `66339069`）结论：
> **已修复** §2.0 路由契约（EXIT=0）、§2.2 perf 门禁（改为真断言）、§2.3 db-migration-gate（`P1-8`=0、34 step 全有 `run`/`uses`）；
> **部分** §2.1 覆盖率基线（`.gitignore:44-45` 例外已加、`git add` 实测可成功）、§2.5 集成静默跳过（18 处已 fail-closed，
> 余 `tests/unit/test_schema_housekeeping_tests.rs:41-42,86-87` 仍裸 `return`）、§2.6 sqlx 棘轮（`OK: 1484 <= 1484`，
> 但正则漏计 turbofish —— 见 §18 N-5）；
> **仍存在** §2.4 供应链门禁在 repo-sanity 恒绿、§2.7 迁移守卫 0 候选、§2.8 baseline 日志缺失，
> 以及 `check_baseline_consolidation.py` 空转未接线。**新增 P0：§18 N-1（fmt 与 clippy 在 HEAD 同时红 + 两道门禁互锁）。**

### 2.0 ✅ 路由契约门禁曾为红（§8 已修）

`bash scripts/contract/check_route_contract.sh` 曾 `EXIT=1`：committed `ROUTE_CONTRACT.md` 写 **921** 条，
重新生成为 **918**（`push_notification.rs` 9 → 6），因为 `2e9bbd71` 删除 3 条 `/r0/push/rules*` 路由时
**没有重新生成文档**。该门禁**确实被 CI 调用**（`drift-detection.yml:359`；`Makefile:203/223` 已并入 `make check`）。
注意该脚本会**就地重写** `docs/` 下的文件，是副作用式门禁。

### 2.1 覆盖率基线**永远无法提交**（棘轮失去跨运行记忆）

- `ci.yml:862` 执行 `git add artifacts/coverage_baseline.json`；但 `.gitignore:44` 是 `artifacts/`（无 `!` 例外）：
  ```bash
  $ git check-ignore -v artifacts/coverage_baseline.json
  .gitignore:44:artifacts/	artifacts/coverage_baseline.json
  $ git add --dry-run artifacts/cargo-deny.txt
  The following paths are ignored by one of your .gitignore files: artifacts   # exit=1
  ```
- 且该文件当前**不存在**，而 `ci.yml:826` 注释称"Baseline at artifacts/coverage_baseline.json"。
- 后果：per-file 覆盖率棘轮（`ci.yml:830-841`）每次拿"本次新生成的基线"跟自己比 → 不可能发现跨运行回退。

### 2.2 `drift-detection.yml` 的性能门禁是纯 echo

`performance-baseline` job：`:316-334` 仍真实 `generate_series(1, 10000000)` 造 1000 万行并跑迁移；
但 `:336-345` 的内容是 `echo "::warning::Performance-baseline gate removed: …"` + `failed=0` +
`if [ "$failed" -ne 0 ]; then exit 1; fi` → **永不可能变红**，付出 10M 行代价却零断言。

### 2.3 `db-migration-gate.yml` 仍含占位空转步骤

该文件仍有 **22 处** `P1-8 placeholder` / 纯 `echo` 步骤（`grep -c "P1-8\|placeholder\|echo "`）。

### 2.4 `repo-sanity` 的供应链门禁恒绿（但同脚本在 `security-audit` 里是真的）

`scripts/ci/supply_chain_gate.sh` 在工具缺失时只打印 warning 并继续（`:57-58`、`:99-100`），最后 `exit 0`。
`ci.yml:140`（`repo-sanity`）**不安装** `cargo-deny`/`cargo-audit`；安装只在 `ci.yml:644-648`（`security-audit`）。
→ **修正一个文档误判**：不是"死门禁"，而是"同一门禁有一处恒绿的重复实例"。

### 2.5 集成测试存在 **18 处**静默跳过（假绿）

```bash
$ grep -rn "integration test database is not available" --include=*.rs tests/ | wc -l
18          # api_route_ledger_tests.rs:16 处 + api_key_backup_route_table_tests.rs:2 处
```
`api_route_ledger_tests.rs` 的**全部 14 个测试**（含
`declared_route_manifest_full_snapshot_matches_default_state` 与 `..._worker_enabled_state`）
都用 `let Some(ledger) = … else { eprintln!("Skipping: …"); return; }` → **不建库即"通过"**。
这是 ledger 契约链（`route_manifest → router_ledger → ledger_export → SDK`）唯一的端到端守卫，
因此该链在无 DB 环境下**从未被验证**。

同类问题：`tests/unit/test_schema_housekeeping_tests.rs:40-43,85-88` 在未设
`TEST_DATABASE_URL/DATABASE_URL` 时静默 `return`，其 2 条"RED-GREEN 回归"默认空转。

### 2.6 SQLx 动态查询棘轮当前 **FAIL**

```bash
$ bash scripts/ci/check_sqlx_dynamic_ratio.sh
dynamic=1476 static=61      # 基线 dynamic<=1443 → 超 33
```
其中 **32** 处是复核前既有欠账，**1** 处是本批新增的 test-only 动态查询
（`synapse-storage/src/audit.rs` 的 append-only 反证 DELETE，必须走真实 DB 往返；
sqlx 离线元数据无法为 `#[cfg(test)]` 专用查询生成）。burn 侧已改用既有
`get_user_stats()` API，未新增计数。文档 `P5C §6`/`C7` 记的"1432→1436 OK / 已修复"已过期。

### 2.7 迁移守卫当前**无候选可检**

`tests/unit/migration_replayability_guard_tests.rs`、`migration_search_path_tests.rs` 仍注册并自检
（断言 `>1000`/`>100` 对），但当前非 baseline 的唯一 SQL 是 `00000001_extensions_v10.sql`，
其 `ADD COLUMN`/`ADD CONSTRAINT` 计数为 **0** → 守卫只能恒绿，只对未来新增迁移生效。

### 2.8 baseline 日志文件缺失

CLAUDE.md 约定的 `docs/audit/00_test_baseline.log`、`00_clippy_baseline.log`、`05_performance_baseline.log`、
`11_performance_after.log` **均不存在**；`scripts/.missing-docs-baseline` 仍有 6 条。

---

## 3. 🟠 P1：仍然存在的架构性冗余 / 过度开发

> **2026-09-15 复测（HEAD `66339069`）**：A-11 已修复（`/r0/push/*` 全清、r0 路由注册数 0）；
> A-10 主项已修复（`initialize_providers` 已在 `wiring/admin.rs:291` 调用），但 `push_gateway` 字段与
> `send_upstream` 仍是死状态/假投递；A-12 的 `src/services/` 壳已删除（`src/storage/mod.rs` 66 行纯壳保留）。
> 其余条目数字更新为：A-1 **47**、A-2 **69 manifest / 935 `.route(`**、A-3 **r0=0 / v1=181 / v3=337**
> （HEAD `66339069` 实测；工作树 WIP 删 manifest 后为 60 个）、
> A-4 **70 trait（~67 单实现）**、A-5 **176 glob / 压制 7**、A-6 **22** 错误枚举、
> A-7 **~4812–4950** 条样板注释、A-8 **287** 个上下文 pub 字段、A-9 **3,631** 行。

| ID | 问题 | 实测证据 |
|---|---|---|
| A-1 | 路由层穿透分层直连 storage | `grep -rl 'synapse_storage' src/web/routes` = **48** 个文件 |
| A-2 | 路由清单函数与路由数庞大 | `*_route_manifest()` **72** 个；`.route(` **977** 处 |
| A-3 | 同一 API 面三套前缀并存 | `/_matrix/client/r0/` **202**、`/v1/` **246**、`/v3/` **290** |
| A-4 | storage trait 爆炸且几乎都是单实现 | 70 个 storage trait，**68 个 ≤1 个真实实现**（为 mock 而存在） |
| A-5 | 通配 re-export 泛滥 | `pub use …::*` **176** 处；`ambiguous_glob_reexports` 抑制 **14** 处 |
| A-6 | 错误枚举重复定义 | 22 个独立错误枚举 |
| A-7 | rustdoc 样板/同义反复 | `/// See [\`…\`]` **4,960** 处，占 rustdoc 总行（**25,873**）~19% |
| A-8 | 上下文对象过胖 | `AppState`/`ServiceContainer`/`RoomContext` 等累计 **256** 个字段 |
| A-9 | 单 crate 模块装配巨大 | `src/web/` 模块系统 **3,360** 行 |
| A-10 | 自建推送能力整条链无人调用 | `PushNotificationService::initialize_providers()`（`synapse-services/src/push/service.rs:137-138`）**0 个调用方** → providers 恒为 `None`，`send_notification` 恒走 `send_fcm_fallback` |
| A-11 | 遗留 `/r0/push/*` 路由未清 | `src/web/routes/push_notification.rs:220-223` 仍有 4 条 legacy 路由 |
| A-12 | 根 crate 薄壳仍存 | `src/{services,storage,common}/mod.rs` 纯 re-export（铁律 6 要求有实际使用者） |

> A-10/A-11 关联：既然已裁定**保留自建推送能力**，则 `initialize_providers()` 必须在
> `synapse-services/src/wiring/admin.rs:280`（构造 `PushNotificationService` 处）被真正调用。

---

## 4. 🟠 P1：测试隔离与模板构建

> **2026-09-15 复测（HEAD `66339069`）**：**T-6 已修复**（`ci.yml` 12 处 `TEST_DATABASE_URL` 全指 `synapse_test`、
> 0 处应用库，各测试步骤 pin `TEST_DB_TEMPLATE_SCHEMA: test_template_ci`，并有守卫
> `tests/unit/test_db_url_convention_tests.rs`）。其余 **T-1 / T-2 / T-3 / T-4 / T-5 / T-7 / T-8 全部仍存在**：
> T-1 实测 `public` 的 BASE TABLE 数 = **0**（跑一次根模板构建即清空且不回填）；T-2 `run_runtime_migrations`
> 在 `error_count>0` 时仍返回 `Ok`（`database_initializer/mod.rs:531`，`executed_at` 类型已修但错误未传播）；
> T-7 live DB 残留 **25** 个 `test_*` schema。

| ID | 问题 | 证据 |
|---|---|---|
| T-1 | **根模板构建无条件 `DROP SCHEMA public CASCADE` 且不回填**（HIGH） | `src/test_utils.rs:563` `DROP SCHEMA IF EXISTS public CASCADE`、`:564` 重建，然后只迁移进模板 schema。**本批复核期间现场复现**：跑完 `cargo test --test unit` 后 `public` 表数从 232 → **0**，随后所有 `connect_shared_test_pool` 套件（22/22 `push::db_tests`、burn/room/audit）全部 `42P01` |
| T-2 | **模板构建吞错**（HIGH） | `synapse-services/src/database_initializer/mod.rs:439-487` 语句失败仅 `warn!` + `break`；`:527` `run_runtime_migrations` **无视 `error_count` 返回 `Ok`**；`:99-117` `initialize()` 仅在 `Err` 分支清 `is_success`；调用方 `src/test_utils.rs:367,639`、`synapse-services/src/test_utils.rs:411` 只查该字段 → **半成品模板被标记 ready** |
| T-3 | 测试隔离仍多头 | 两份 `prepare_isolated_test_pool`（`src/test_utils.rs:303` vs `synapse-services/src/test_utils.rs:237`）；两份 `init_template_schema`（`:478` vs `:350`）；Guard 4 只扫 services（`tests/unit/test_isolation_unification_tests.rs:531-533`） |
| T-4 | `search_path = <schema>, public` 回退仍在 | `src/test_utils.rs:334/606`、`synapse-services/src/test_utils.rs:257/270`、`synapse-storage/src/test_isolation.rs:107` |
| T-5 | "清 public schema"逃生门仍在 | `SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE`（`src/test_utils.rs:540` 附近）；CI 未开，代码路径仍在 |
| T-6 | **CI 把集成测试指向应用库**（HIGH） | `.github/workflows/ci.yml:569/587/599/608/799` 五步用 `TEST_DATABASE_URL=…:5432/synapse` 且**未** pin `TEST_DB_TEMPLATE_SCHEMA`；seed 在 `:297/:309` 用 `…:5432/synapse_test`。已实测：这些步骤里 `prepare_shared_test_pool` 进入 `init_template_schema` 被 DB 名守卫拒绝（`src/server/mod.rs:1206` panic），而 `--test integration` 有 **775** 处 `require_test_pool()/get_test_pool()` → 整批集成测试失败。文档 `§1.4` 声称"修复后 `TEST_DATABASE_URL` 只出现在 `:16`"是**假的**（实为 10 处命中，5 处指向应用库） |
| T-7 | 6 个 `test_*` schema 未被 janitor 回收 | `synapse_test` 中创建于 `2026-09-14T12:15Z` 的 6 个 schema 仍在 |
| T-8 | 模板构建仍有 3 条路径 | 根/services 走 `DatabaseInitService`，共享路径走 `synapse-common/src/test_isolation.rs::build_template`；`synapse_test_template_*` 的创建方**未定位** |

**已修复的隔离项**（不要再改）：`clone_schema_from_template` 唯一真实实现已在
`synapse-common/src/test_isolation.rs:1225`（另两处是薄包装）；`SeedSource` 允许列表已参数化；
序列 `OWNED BY` + `advance_schema_sequences` 已落地；Guard 1b/7 已补；清理脚本 4 类 schema + dry-run 默认；
`prune_stale_template_schemas` 已实现并测试；`cleanup_test_schemas.sh` 的 4 个缺陷已修。

---

## 5. 🟡 P2：安全 / 协议残留

| ID | 问题 | 证据 / 影响评估 |
|---|---|---|
| S-1 | `query_server_keys` **不校验**返回密钥自签名 | ✅ **已修复（B5-1）**。原缺陷：`client.rs:773-785` 直接 `return`，同一份文档经 `/key/v2/server` 会被拒（`get_server_keys` 有 `verify_server_keys_self_signature`）、经 `/key/v2/query` 却被接受 —— 校验缺口取决于调用的是哪个端点。现 `query_server_keys` 与 `get_server_keys` 收敛到唯一信任门禁 `validate_remote_server_keys(&keys, server_name)`；期望值取 `server_name`（"要的是谁的密钥"）而非 `destination`（承载请求的传输对端）。守卫：`admit_server_keys_rejects_forged_signature_without_caching` + `query_server_keys_path_rejects_mismatched_document_over_http`（经真实 HTTP 响应字节走 `handle_response` → 门禁，非就地构造 `ServerKeys`） |
| S-2 | `get_server_keys` **不校验** `server_name == destination` | ✅ **已修复（B5-1）**。原缺陷：`client.rs:746-770` 仅在缓存前验签，未校验文档自称的身份。攻击者可用**自己的**密钥合法自签一份文档并声称 `server_name = victim`，单独的自签名校验必然放行（这正是自签名抓不住的那一类），于是 `destination → 错误身份的公钥` 被写入 `key_cache`（跨身份缓存投毒）。现顺序改为**先名字、后自签名**：自签名是在 `keys.server_name` 下查找的，先钉住名字才使该查找等价于"destination 签的"而非"文档自称是谁签的"。比较严格相等（不做大小写折叠 / 不去端口），fail-closed。守卫：`server_keys_wrong_server_name_rejected`、`admit_server_keys_rejects_wrong_name_without_caching`（断言缓存仍为空）、正向对照 `admit_server_keys_caches_valid_document_under_expected_server` |
| S-4 | `quarantined_media_changes` 无界增长 | ✅ **已修复（B5-2）**。原缺陷：全仓 `DELETE` 针对该表 **0** 条 → 纯 append-only，违反 AGENTS.md"Long-running deployments need pruning"。修复复用 `synapse-storage/src/pruning.rs` 既有骨架（该模块本就是"append-only 表的 `DELETE ... WHERE ts < cutoff`，由 `src/server/mod.rs` 定时任务调度"）：新增 `QUARANTINED_MEDIA_CHANGES_RETENTION_DAYS = 30` + `prune_old_quarantined_media_changes`，并**接入调度循环**——只加函数不接线等于假修复。取舍已文档化：`GET /_synapse/admin/v1/quarantine_media/{media_id}/changes?since=N` 无法回放早于窗口的位置，但该缺口**可被检测**（返回的最低 `stream_id` 会高于请求的 `since`），与 device-list 流已接受的取舍一致；写入速率由**管理员动作**而非流量决定，故 30 天足够宽松。**有意不加 `created_ts` 索引**：既有 `idx_..._stream` 已服务按位置读取路径，小表上的顺序谓词比在写路径多维护一个索引更便宜（有证据再议）。守卫：`prune_quarantined_media_changes_respects_retention_window`（除计数外还断言存活行 `MIN/MAX(created_ts)`，堵住"删对数量但删错行"）+ 常量一致性测试 `test_quarantine_retention_matches_device_list_window`。变异自证：比较符 `<` 反转为 `>` → 转红 |
| S-5 | federation knock **丢弃 `via`** | ✅ **已修复（2026-09-15 复核确认）**。原缺陷：join 只读非标准 body 键 `via_servers`、knock 完全忽略 `via`。现 `src/web/routes/handlers/room/members.rs:69 extract_via_servers(query, legacy_body)` 同时支持规范形状（重复查询参数 `?via=srv1&via=srv2`，`via` 胜出）与旧 body 形状（兜底），join(`:117`) 与 knock(`:236`) 均已接入；单测 `:775-805` 覆盖查询解码、端口保留、旧形状兜底。`percent_decode_component` 一并处理了 URL 编码 |
| S-6 | X-Matrix 头解析用朴素 `split(',')` | `src/web/middleware/federation_auth.rs:299`。**理论问题**：所有字段值都不会含逗号，且解析结果参与签名校验，误解析 fail-closed |
| S-7 | 速率限制碎片化 | 已核实 `src/web/routes/friend_room.rs:631-632` 自建 key 并直接 `ctx.cache.rate_limit_token_bucket_take(...)`；同类模式另见 `handlers/search/search.rs`、`auth_compat.rs`（子代理复核） |
| S-8 | cache 读写不对称（结构性陷阱，代码内已标注） | `synapse-cache/src/manager.rs:398` `set_raw` 写 L1+L2（异步）；`:408` 同步 `get_raw` **只读 L1**；L2 回退需显式 `:421 get_raw_shared().await`。当前无生产误用 |
| S-9 | threepid 路由**孤儿**（且已进契约文档/未进 ledger） | ✅ **已修复（B5-4）**。方案裁定为**删除**而非补装配点（理由与证据见 §11）：`create_threepid_router` 在本仓历史中**从未**被任何装配位置调用（`git log -S 'create_threepid_router()'` 在 `assembly.rs`/`mod.rs`/`src/server` 全为空），路径为裸 `/requestToken`、`/submitToken`（非 Matrix 规范形状，任何 Matrix 客户端都找不到），且是**未完成桩**（`request_token` 从不发信；注释称"为测试返回 token"但响应结构体没有 token 字段）。真实 3PID 端点在 `account_compat.rs`（`/account/3pid/...`，已在 `assembly.rs` 装配并使用同一 `threepid_storage`）。已删除 `src/web/routes/threepid.rs`、`mod.rs` 的死 re-export、以及仅服务于它的 `AuthContext::threepid_storage`（DI 手工复制的净减）。契约提取器实测：模块 67→66、路由 1148→1146、"前缀之外"桶 16→**14 条纯有意根级注册**，两份 oracle 仍 **0 缺口**。守卫从「钉死缺陷」改为「钉死不变量」：`check_non_namespace_bucket` 精确断言该桶 == 14 条已知有意注册 |
| S-12 | MSC4108 `DELETE` 204 响应缺 3 个 required 头 | ✅ **已修复（B5-3）**。原缺陷：`msc4108_rendezvous.rs:275` 返回 `(StatusCode::NO_CONTENT, Body::empty())`，无 header tuple；POST/GET/PUT 有 `Last-Modified`/`Cache-Control: no-store`/`Pragma: no-cache` 而 DELETE 缺失。守卫见 S-15 行（改为调用真实 handler） |
| S-13 | 契约提取器结构性缺陷 | ✅ **已修复（B5-5）**。原缺陷三项全部复现并消除：①链式方法只记录第一个 method（msc4108 `get().put().delete()` 只出 `GET`）；②`nest_map` 收集后从未使用；③`ROUTE_CONTRACT.md` 有 **15** 条相对 `/spaces/...`、**0** 条带前缀。修复后实测：MSC4108 出全 4 条（POST/GET/PUT/DELETE）；spaces 相对路径 **15→0**、带前缀 **0→48**（24 路由 × v1/v3 两前缀，B1 已删 r0 故为 2 而非 §17 预估的 3×15=45）。详见 §9 |
| S-14 | "真实 router == ledger" 只有单向校验，**漏报方向无任何门禁** | ✅ **已修复（B2-4a）**，但原判定需修正：`tests/integration/api_route_ledger_tests.rs::declared_route_manifest_entries_are_actually_wired` **确实**在探针真实 router（对每条声明发 PATCH，断言 405 且 `Allow` 含该方法）——所以"声明不谎报"这一向**有**覆盖。真正缺的是**反向**：ledger 漏掉多少真实路由，无任何断言。实测漏 **22** 条（提取器原实现把这组差集只打印不拦截，注释自认 "manifests are hand-written and incomplete"）。已逐条核实为真并补进 manifest，双向闭合为 0。详见 §12 |
| S-15 | MSC4108 响应头测试**自证** | ✅ **已修复（B5-3）**。原缺陷：`tests/unit/msc4108_rendezvous_route_tests.rs:222-338` 构造本地 header 数组再对其断言，从不调用 handler → DELETE 缺头也会通过。现新增 `:421 delete_session_returns_204_with_required_headers` 等用例走**真实 handler 调用**再断言响应头 |

> **降权/未复核**：`S-3`（文档 `CFG-5/CFG-6` 称 51/27 处 legacy hash 引用，按所给符号名只能命中 9/1 → 符号名不确定，
> 不作为结论）；`S-10`（MSC4108 双路径族）、`S-11`（SDK 93 条中 64 条死条目）为子代理复核，未由主复核独立确认。

---

## 6. 🟡 P2：配置 / 仓库 / 文档卫生

> **2026-09-15 复测（HEAD `66339069`）**：**已修复** H-2（`.scratch` 0 文件）、H-3（`coverage/` 0 文件）、
> H-4（`docker/deploy` **2.0 MB**，原 18 GB）、H-5（worktree 仅 main）、H-12（端口链只剩 5432/synapse_test + 守卫）、
> H-14（`db_migrate.sh` 拒绝隐式 loopback 宿主 psql + 守卫）、H-15/H-16（`Makefile` 只剩 `migrate-status`/`migrate-audit`，
> 且改为 `$(DC) exec -T $(DB_SERVICE) psql` 查 `schema_migrations`）。
> **仍存在** H-1（`pool_size` 零引用废弃字段）、H-6（`server` feature 0 门）、H-9（~3.5k doc 警告无棘轮）、
> H-10（god-file 1833 行）、H-11（mock 漂移 2 处）、H-13（计数过期：实测 30/262/29/11/0）。
> **部分** H-8：单一配置树漂移已结构性消除，但**仍无测试把 `docker/config/homeserver.yaml` 反序列化进 Rust `Config`**。

| ID | 问题 | 证据 |
|---|---|---|
| H-1 | ~~`pool_size` 废弃字段仍在两份配置~~ | 复核时为 `docker/config/homeserver.yaml:70,88` + `docker/deploy/config/homeserver.yaml:69,87`；**并发会话正在删除 `docker/deploy/config/`**，请以当前 HEAD 复核 |
| H-2 | `.scratch` 入库 | `git ls-files .scratch \| wc -l` = **97** |
| H-3 | `coverage/` 派生结果入库 | `git ls-files coverage \| wc -l` = **3** |
| H-4 | `docker/deploy` 目录 18 GB | `du -sh docker/deploy` = **18G**（19 份备份） |
| H-5 | git worktree 常驻 | 复核时 3 个；本批已删除自己遗留的 `.worktrees/redundancy-cleanup`（其 3 个提交均已并入 main），现剩 `.claude/worktrees/optimization+audit-2026-07` |
| H-6 | `server` feature 有名无实 | `Cargo.toml:31,39` 声明；`#[cfg(feature = "server")]` Rust 门 **0** 个 |
| H-8 | 配置一致性门禁不覆盖 Rust config struct | `scripts/check_config_consistency.py:59` 硬编码 `COMPARED_FILES` 三文件；`:184-189` 用 `is_file()` 忽略目录。**并发会话正在删除该脚本**，请以当前 HEAD 复核 |
| H-9 | `cargo doc` 存量警告 | `~3,500` 条 intra-doc-link 警告（未重新测量；`ci.yml:270-274` 自认该局限） |
| H-10 | god-file 未拆 | `friend_room_service/mod.rs` = **1,833** 行 |
| H-11 | mock 与 PG 语义漂移（2 处） | `test_mocks/member.rs:596-597` 负 `limit` 直接 `take(limit as usize)` 未 clamp（生产 `managers/query.rs:157` 已 clamp）；`test_mocks/device_list.rs:112` 仍是忽略 `from/to/requester` 的 stub |
| H-12 | `IsolatedTestPool` 默认 URL 过期 | `synapse-common/src/test_isolation.rs:42-52` env-first，但 fallback 仍首选 `localhost:15432` |
| H-13 | 允许列表计数过期 | `#[allow(dead_code)]`=**31**、`allow(clippy::`=**262**、`#[ignore]`=**26**、TODO/FIXME/XXX/HACK=**11–12**、`#[deprecated]`=**0**（文档记 149/170/26/8） |
| H-14 | ~~🔴 `docker/db_migrate.sh` 优先宿主 `psql`，会操作**非 compose 栈**的 PostgreSQL 并**改它**~~ | 2026-09-15 已加护栏（`db_migrate.sh` 的 `host_psql_target_is_implicit_loopback`）：宿主 psql + loopback + **调用方未显式给出目标**（`DATABASE_URL` 与 `DB_HOST` 都为空）+ 不在容器内 → 拒绝执行。原复现形态 `bash docker/db_migrate.sh validate` 现在打印 6 行 `[ERROR] 拒绝执行…` 并以 1 退出（实测 exit=1，不再建库）；确实要打宿主实例时 `SYNAPSE_DB_MIGRATE_ALLOW_HOST_PSQL=1` 显式放行。调用侧不变式已由 `tests/unit/migration_consistency_tests.rs::every_ci_db_migrate_call_supplies_an_explicit_target` 守住：CI 每一步调用都必须显式给出 `DATABASE_URL` 或 `DB_HOST` |
| H-15 | ~~🟠 Makefile 里并存第二、三条迁移路径~~ | 2026-09-15 已删：`flyway-info`/`flyway-migrate` + `FLYWAY_URL`（挂载的 `scripts/db/` 目录根本不存在，目标必死）、`migrate`/`migrate-check`/`migrate-undo`/`migrate-baseline`（写 `_sqlx_migrations`，与 `schema_migrations` 两套账且绕过扩展门控）。Makefile 只保留只读查询目标 `migrate-status`/`migrate-audit`；执行入口唯一为 `docker/db_migrate.sh {init\|migrate\|status\|validate}`。刻意**不**提供 `make migrate` 转发 —— Makefile 自带的 `DATABASE_URL` 默认值会让脚本误判"调用方显式指定了目标"，正好绕过 H-14 护栏。守卫：`makefile_exposes_a_single_migration_execution_path` |
| H-16 | ~~Makefile `migrate-status`/`migrate-audit` 用宿主 `psql` 连 `FLYWAY_URL`~~ | 2026-09-15 已改为容器 exec（默认 dev 栈 `db` 服务，`COMPOSE_DIR`/`DB_SERVICE`/`COMPOSE_FILES` 可覆盖指向 deploy 栈的 `postgres`），不再依赖宿主端口；栈未启动时打印可操作提示。验证：dev 栈给出空表结果，deploy 栈列出 2 条记录，旧路径复现为 `FATAL: database "synapse" does not exist` |

---

## 7. ✅ 已修复 / ❌ 已证伪（避免重复劳动）

### 7.1 已确认修复（文档已过期）

- **删除类**：`src/web/api_doc/` + utoipa 双源；runtime DDL 路径；23 个零引用表；
  `push_rules.priority`/`priority_class` + 依赖索引；`docker/deploy/migrations` 死副本。
- **门禁类**：`ci.yml:217` clippy 已扩为 `--workspace --all-targets --features test-utils … -D warnings`
  （全仓 `cargo clippy` 仅 1 处）；`ci.yml:277` doc-test 已加 `--workspace`；
  四处 CI 修复生效（`ci.yml:738 --bin synapse_worker`、`schema-health-check.yml:73 …_v11.sql`、
  `drift-detection.yml:346` 显式 warning、`test.yml`/`format-drift-tracking.yml` 已删）；
  `ci.yml:303` 已跑 `--workspace --lib`；`ci.yml:358-362` 已接 schema 清理。
- **隔离/测试类**：`clone_schema_from_template` 唯一实现 + 2 薄包装；`SeedSource` 参数化；
  `prune_stale_template_schemas` + marker 播种/刷新（`synapse-common/src/test_isolation.rs:473/427/511/592-596/644`）；
  Guard 1b/7（`tests/unit/test_isolation_unification_tests.rs:418/723`）；
  `cleanup_test_schemas.sh` 4 类 + dry-run + stderr；
  `media/mod.rs:697` 委派隔离池；`ci_test_scope_tests` 4/4；
  `test_fixture_error_handling_tests` 2/2；`e2e_honesty_tests` 6/6；`schema_lifecycle_guard_tests` 2/2；
  54 个 storage 文件改用 `connect_shared_test_pool`；`map_err` 化的 `delete_room_cascade`。
- **ledger/协议类**：`RouteStatus`/`with_status`/`RouteEntry.status` 已删；
  `SCHEMA_VERSION = "4"`（`ledger_export.rs:45`）+ 6 fixture + `LEDGER_EXPORT_SCHEMA.md` 同步；
  `schema_doc_version_matches_code` 守卫存在（`tests/unit/ledger_export_tests.rs:284`）；
  `friend_room` 双前缀**证伪**（router 93 / manifest 93 / symdiff 0）；
  MSC4108 manifest 4 条与 router 一致（`msc4108_rendezvous.rs:46-49`）；
  MSC4108 其余 7 项规范细节（202/204/412+errcode/413/Content-Type/Expose-Headers/304 五头）已实现。
- **其他**：`allow(dead_code)` 151 → 22/31；`strategy.rs` 312 → 76 行；fmt 棘轮绿；
  `migrations/` 单真相源；`TIMESTAMPTZ` 仅剩 1 处注释；
  `/sync` invite N+1 改批量预取（`sync_service/response.rs:140/564/571`）；
  admin 批量踢人 `buffer_unordered` + failures 上报；`20260831070000` search_path 误绑结构性消失；
  `time.rs:189 assert!(age <= 50)`。

### 7.2 证伪（断言与代码事实不符）

- **A2/A3/A7/B1–B4/C1–C5（P3 迁移文档）**：断言对象（`20260906010000`、undo 链、36↔36）**已整体删除**；
  `migrations/` 只有 **2** 个 SQL、**0** 个 `.undo.sql`；文档称"applied=38 / public 表 253"不成立。
- **`AUDIT_SUMMARY` SUM-14/19/20**：73 增量 → 0；nextest retention 分组已移除；149 → 22/31。
- **`sdk-encapsulation-audit` §12.3 S-6/S-7/S-8**：代码事实反驳。
- **`synapse-rust-a-plus-roadmap`**："0 missing docs" vs `scripts/.missing-docs-baseline` = **6**；"快照通过" vs 快照测试静默跳过。
- **`P5C §6` / `C7`**：`sqlx dynamic 1432→1436 OK` → 实测 **FAIL**。
- **`P5C §6` "baseline consolidation 是门禁"**：未接入 CI。
- **`PROJECT_REMAINING §1.4`**："修复后 `TEST_DATABASE_URL` 只出现在 `:16`" → 实为 10 处命中、5 处指向应用库。
- **`PROJECT_REMAINING §16`**："10/10 已补齐" → 实为 7/10（DELETE 缺头）。
- **`PROJECT_REMAINING §18`**："main 上唯一未动问题是 §17 与 §7" → 还应加 §8（门禁红）与 §16（DELETE 头）。
- **`PROJECT_REMAINING §12.3`**："0 个 registered-but-not-in-manifest" → extractor 实为 918，且 threepid 2 条"已注册但不在 ledger"。
- **`PROJECT_REMAINING §15.3b-3`**：引用不存在的 §15.4，且存在两个 §15.3 标题。
- **`PROJECT_REMAINING §19-2/§20-1`**：`clone_schema_from_template` 有 3 个 `fn` 定义，但唯一真实实现在 `test_isolation.rs:1225`。
- **B6/C 系列"守卫 RED→GREEN 实测"**：当前树无法制造 RED（唯一非 baseline SQL 的 `ADD COLUMN`/`ADD CONSTRAINT`=0），历史结论不可复现。
- **`push::db_tests` 22/22 `42P01` 不是产品缺陷**：根因是本地 `public` schema 被根模板构建清空（`src/test_utils.rs:563`），
  CI 通过 `TEST_DB_TEMPLATE_SCHEMA=test_template_ci` + `scripts/ci/prepare_test_db.sh` 规避。**这是环境问题，不是 schema 缺陷。**

---

## 8. 本批已实施修复（P0 三项 + 契约门禁）

| 修复 | 变更 | RED→GREEN 证据 |
|---|---|---|
| F-1 | baseline 增 `CREATE UNIQUE INDEX IF NOT EXISTS ux_burn_log_user_event ON burn_after_read_log(user_id, event_id)` | 新测试 `burn_after_read::db_tests::test_log_burned_event_batch_is_idempotent_on_conflict_target`：删索引后 **FAILED（42P10）**，恢复后 **ok** |
| F-2 | `ck_rooms_room_version_valid` 由 1..11 白名单改为 `room_version ~ '^[0-9]+(\.[0-9]+)*$'`（不留 `_v2` 兼容名） | 新测试 `room::db_tests::test_room_version_check_accepts_versions_beyond_eleven`：换回白名单后 **FAILED**，恢复后 **ok**；并断言 `not-a-version` 仍被拒（CHECK 有牙齿） |
| F-3 | baseline 恢复 `prevent_audit_delete()` + `trg_prevent_audit_delete`；守卫加 `tgrelid = 'audit_events'::regclass`（见 F-3 补充） | 新测试 `audit::db_tests::test_audit_events_reject_unflagged_delete`：删触发器后 **FAILED**，恢复后 **ok**；schema-blind 版本在重建 schema 上实测 **FAILED**（`rows_affected: 1`），加 `tgrelid` 后 **ok** |
| G-1 | 重新生成 `docs/synapse-rust/ROUTE_CONTRACT.md`（921 → 918） | `bash scripts/contract/check_route_contract.sh` 由 `EXIT=1` → 提交后 `EXIT=0` |
| 附带 | `migrations/README.md`、`migrations/INDEXES.md` 更新为与 baseline 一致（删除对已删迁移文件的引用，写明 20 项待折入） | — |
| 附带 | `EXPECTED_BASELINE_FINGERPRINT` 两次更新：`8737d9a5f8413d51` → `f6e7a2093ed526e8` → `696ff078f537a0c8` | `baseline_fingerprint_is_v11_then_extensions_with_no_separator` 通过 |
| 附带 | 删除自己遗留的 `.worktrees/redundancy-cleanup`（3 个提交均已并入 main） | 修复 `exactly_one_place_builds_the_schema_clone` 因陈旧副本产生的假失败（8/8 通过） |

提交：`fba2aac6`（基线三项 + 契约文档 + 测试）、`86fc6cd0`（schema-aware 触发器守卫 + fingerprint）、
`95af403d`（把两个新 db_test 留在 sqlx 动态路径，修复 `SQLX_OFFLINE` 编译失败）、
`931da04f`（本报告）。

**验证**：`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → **0 警告 / EXIT=0**；
`./scripts/check_fmt_ratchet.sh` → **绿**；`python3 scripts/check_migration_consistency.py` → `status=ok, issues=0, warnings=0`；
三个 P0 测试 + 全部 `burn_after_read::db_tests`/`audit::db_tests`/`e2ee_audit::db_tests` → **22 passed, 0 failed**。

**明确未做**（按裁定留待后续批次）：剩余 **20** 个未折入对象（§1.4/§1.5）、5 对重复索引、
3 处硬编码 `public`、§2 的 8 项门禁诚信问题、§4 的模板构建吞错与 CI 环境库指向、§5/§6 的全部条目。

### 8.1 自建推送投递链路此前**完全不可用**（本批修复并端到端验证）

复核「确认真实投递路径可用」时发现：**这条链从未跑通过**，共 4 个独立缺陷叠加。

| ID | 缺陷 | 证据 / 影响 |
|---|---|---|
| P-1 | `initialize_providers()` **0 个调用点** | `synapse-services/src/push/service.rs:138` 定义、全仓无调用 → `fcm/apns/webpush_provider` 恒为 `None`，`send_to_provider` 永远走不到 `send_with_retry`。现已在 `wiring/admin.rs` 构造后调用（`if let Err` 记录 error 而非中止启动，因为 `ServiceContainer::new` 无返回值） |
| P-2 | 三条 "fallback" 路径**伪造成功** | 旧 `send_{fcm,apns,webpush}_fallback` 在 provider 未初始化时打印 "Sending fallback push notification" 并返回 `PushResult::success_with_response("FCM accepted (fallback)")` → 通知被标记 `sent`、`last_used` 被更新，**实际一个字节都没发出去**。已替换为 `provider_unavailable()`：配置禁用 → 跳过（避免无意义重试）；配置启用但 provider 缺失 → **返回错误**（fail-closed，落进通知日志与重试队列） |
| P-3 | `create_notification_log` **漏写 NOT NULL 列** | `push_notification_log.created_ts` 是 `BIGINT NOT NULL` 且无默认值，而 INSERT 的列清单里没有它 → 每次投递后写日志必报 `23502`。已复现：`psql … INSERT INTO push_notification_log (user_id, device_id, …, response_time_ms) VALUES (…)` → `ERROR: null value in column "created_ts" … violates not-null constraint`。已在 `synapse-storage/src/push_notification.rs` 补上 `created_ts` |
| P-4 | 投递后记账失败**反噬投递结论** | 旧代码把 `create_notification_log` / `update_device_last_used` / `record_device_error` 的 `?` 直接冒泡，而 `process_pending_notifications` 把 `Err` 映射为 `mark_notification_failed` → **已经投递成功的推送被当成失败重试**（重复推送，最多 `max_attempts` 次）。已改为 best-effort + `warn!`，结论只由 provider 返回决定 |

**修复后验证（5 个 db_test，wiremock + 真实 DB，全部 RED→GREEN 实测）**：

| 测试 | 断言 | RED 证据 |
|---|---|---|
| `initialize_providers_builds_every_enabled_provider` | 启用的 fcm/apns/webpush 全部被构建 | — |
| `initialize_providers_leaves_disabled_providers_unset` | 禁用者不构建（陈旧凭据不生效） | — |
| `process_pending_notifications_delivers_through_the_provider` | wiremock 收到**恰好 1 次** POST（`Authorization: key=…`、body `to` = token），队列行 `status='sent'`，**且 `push_notification_log.provider_response` 落库** | 回退 `created_ts` 修复 → 该用例 `processed=0` 且行变 `pending` + `Failed to create notification log` |
| `enabled_but_uninitialized_provider_fails_instead_of_faking_success` | 启用但 provider 缺失 → `processed=0`、非 `sent`、`error_message` 含 "not initialized" | 临时恢复旧的"假成功"fallback → 用例 FAILED |
| `disabled_provider_skips_without_retrying` | 禁用 → 跳过记为成功（不产生重试风暴） | — |

**另加 3 条静态守卫** `tests/unit/push_provider_wiring_tests.rs`（防止再次丢失接线）：
`admin_wiring_calls_initialize_providers`（并要求放在 `Arc::new` **之前**）、
`admin_wiring_handles_initialization_failure_explicitly`（必须显式处理结果，且不得 `.await?`/`.unwrap()` 中止启动）、
`notification_log_insert_supplies_created_ts`。
**RED 实测**：临时删除 `wiring/admin.rs` 的调用 → 前两条 FAILED（`1 passed; 2 failed`），恢复后 3/3 通过。

**仍存在（未修，留待后续）**：`push_config` **没有任何写入 API/管理端点**（`PushNotificationStoreApi` 只有
`get_config*` 读取器），运维只能用 SQL 手工插入 —— 且在生成的 `ck_push_config_user_id_format` 约束下必须伪装成合法 user_id；
`PushQueue`（`synapse-services/src/push/queue.rs`，14 KB）是**只写不读**的死状态（`self.queue` 仅在 `with_queue`/`initialize_providers`
赋值，全仓无读取点），属铁律 2 的重复实现；`config.push.enabled`（`docker/config/homeserver.yaml:205`）与 provider 初始化**无关联**。
本批新增 2 处测试夹具动态查询，已按基线文件既有惯例把 `BASELINE_DYNAMIC` 1476 → 1478 并记录理由。

### 8.2 推送配置可运维化 + 删除死队列（本批完成）

**① `push_config` 没有写入 API/管理端点 → 已补齐，并把表结构改对**

| 项 | 变更 |
|---|---|
| 表结构 | `push_config` 由"per-user 伪配置表"改为**全局键值表**：`config_key TEXT PRIMARY KEY, config_value TEXT NOT NULL, created_ts, updated_ts`。原 `id/user_id/device_id/config_type/config_data` **全部删除**——没有任何读取方使用它们，而 `user_id` 上的生成约束 `ck_push_config_user_id_format` 迫使运维**伪造一个合法 user_id** 才能存一条全局 provider 凭据。连带删除失效索引 `idx_push_config_user`（baseline 索引数 365 → 364） |
| 存储 API | 新增 `list_config()` / `set_config(key, value)`（UPSERT，返回 `PushConfigEntry`）/ `delete_config(key)`，与既有 `get_config*` 读取器配套 |
| 管理端点 | `GET /_synapse/admin/v1/push/config`（列出配置 + 当前已初始化的 provider + 支持的键；**密钥类字段脱敏**为 `****last4`）与 `PUT /_synapse/admin/v1/push/config`（`config_key -> value`，`null` 表示删除该键） |
| 立即生效 | provider 改为 `Arc<RwLock<PushProviders>>`，`initialize_providers(&self)` 原地重建 → **PUT 后无需重启**（`container` 以 `Arc` 共享服务，原本无法替换） |
| 防配置垃圾场 | PUT 的键必须命中 `SUPPORTED_PUSH_CONFIG_KEYS`（= `initialize_providers` 真正读取的 7 个键），`*.enabled` 必须为 `true`/`false`，写入前整批校验（避免半更新）；空/纯空白凭据视为未配置，不会构造出"已禁用但恒返回成功"的 provider |

**验证**：`push_config_round_trips_through_the_storage_api`（set/list/delete/UPSERT 不重复）、
`config_changes_reload_providers_without_a_restart`（改配置 → 原地生效；禁用 → provider 消失）、
`blank_credentials_do_not_build_a_provider`；路由层新增 6 条无 DB 单测（body 反序列化含 `null` 删除、未知字段拒绝、
7 键全通过校验、未知键/空 patch/非布尔 enabled 被拒、SECRET ⊆ SUPPORTED）。
Ledger 契约链同步：golden + SDK 两条车道的 6 个 fixture、`ROUTE_CONTRACT.md`（918 → 920 条路由）、baseline 指纹。

**② `PushQueue` 死状态 → 已删除**

`synapse-services/src/push/queue.rs`（481 行）整体删除，并移除 `push/mod.rs` 的模块声明、`PushNotificationService::queue` 字段、
`with_queue()` 及 `initialize_providers` 里的建队列代码。真实队列是 **DB 表 `push_notification_queue`**
（`queue_notifications_batch` 写、`get_pending_notifications` 读，由 `POST /_synapse/admin/v1/push/process` 消费），
内存队列从来没有任何读取点。

**同类问题（本批未动，建议下一批处理）**：`PushNotificationService::push_gateway` 字段同样**只写不读**
（仅 `with_push_gateway` 赋值、无读取点），而它依托的 `PushGateway` 结构体（`push/gateway.rs`，441 行中约 340 行）
除自身单测外**无任何构造点**；唯一存活的是 `validate_push_gateway_url`（被 `src/web/routes/push.rs:222` 使用）。
此外 `send_to_provider` 的 `"upstream"` 分支（`send_upstream`）同样是"打日志 + 返回成功"的**假投递路径**。
建议二选一：**(a)** 删除该字段/结构体并把 `upstream` 从 `register_device` 的合法 `push_type` 中移除；
**(b)** 用新的 `push_config` 写入 API 配置 upstream gateway 端点并真正实现投递（fail-closed）。
在裁定前保留现状，但不应继续以"假成功"姿态留在投递链里。

---

## 9. S-13 契约提取器——已根治（2026-09-15，B5-5）

原缺陷是**结构性**的：解析器只看文本里每个 `.route(...)` 的**第一个**方法、完全忽略
`.nest()` 前缀，因此 `ROUTE_CONTRACT.md` 既漏方法、又把相对路径当 serve 路径。
§17 已指出"要做完整审计，必须先有一个能解析链式方法与 `.nest()` 前缀的解析器"——
本节点即该前置修复的落地。

### 9.1 三项子缺陷的复核与消除

| # | 原缺陷 | 复核结果 | 修复后实测 |
|---|---|---|---|
| 1 | 链式方法只取第一个 | ✅ 复现：MSC4108 实有 4 条，只提取到 2 条 | `msc4108_rendezvous.rs` 出全 POST/GET/PUT/DELETE |
| 2 | `nest_map` 收集后从未使用 | ✅ 复现：`out[mod] = full`，前缀从未拼接 | 死代码删除；改为沿 router 构造递归传播前缀 |
| 3 | 15 条相对 `/spaces/...`、0 条带前缀 | ✅ 精确复现：恰为 15 / 0 | 相对 **15→0**，带前缀 **0→48** |

> 前缀数说明：§17 预估 45 是按 **3** 个前缀（v1/r0/v3）推算的；B1 已拆 r0，
> `SPACE_NEST_PREFIXES` 现为 `["/_matrix/client/v1","/_matrix/client/v3"]`，
> 故 24 条路由 × 2 = **48**。数字变化源自 r0 拆除，不是修复不完整。

### 9.2 修复方式

解析器从"正则扫行"升级为**沿真实 router 构造求值**：

1. **链式全取**：`.route(p, get(a).put(b).delete(c))` 提取链上每个方法，且支持多行写法
   （本仓主流格式是 `.route(\n  "/p",\n  get(h).put(h2),\n)`）。
2. **前缀传播**：递归求值 `.nest("/prefix", expr)` / `.merge(expr)`，支持局部变量、
   同文件与**跨文件** router 构造函数（`space.rs` 的 nest 作用于 `space/*.rs` 定义的路由），
   以及声明式的 `expand_under_prefixes("m", PREFIXES, &relative_routes())`。
3. **异构前缀**：不同子 router 可用不同前缀集（e2ee 的 compat→v1+v3、v3-only→v3；
   search 的 v1_router→v1、v3_router→v3），故做的是**逐 router 精确解析**而非文件级启发。
4. **只求值根**：把一个 router 被另一 router 静态调用的关系建成图，仅求值入度为 0 的根，
   避免"子 router 单独求值产出相对路径 + 父级再产出绝对路径"的双份条目。
   经 `RouteModule::merge_into` 动态装配的 feature 模块天然无静态调用者，因此自然成为根。
5. **归属到字面量所在文件**：路径归属取**定义**文件而非调用方文件，
   于是 `space/*.rs` 各自列出自己的绝对路径，`space.rs` 自身不再重复。
6. **测试块块级剔除**：不再"在第一个 `#[cfg(test)]` 处截断"——`space/lifecycle_query.rs`
   在第 19 行就有 `mod cursor_tests`，截断法会连第 200 行的 router 构造一起删掉。

### 9.3 验证：两份独立 oracle，而非自证

| 对照源 | 性质 | 结果 |
|---|---|---|
| 各模块 `*_route_manifest()` 声明集 | 手写**绝对**路径，不经过本解析器的前缀推导 | **声明而未解析出 = 0** |
| `tests/unit/fixtures/ledger_export/*.json`（1044 条） | 由真实 Rust 装配 `synapse_ledger_export` 导出、golden 测试守护 | **ledger 有而清单缺 = 0** |

第二条是关键：它保证新清单**不会漏掉任何真实对外服务的路由**（零漏报）。
反向差额 104 条来自源码扫描可见、而默认 feature 构建不注册的 gated 路由
（SAML / CAS / Voice / ExternalServices）以及 manifest 的漏声明——已抽样逐条确认为真实注册。

**守卫**（`scripts/contract/test_extract_registered.py`，18 项）：
链式方法、前缀传播、跨文件 nest、异构前缀、测试块剔除、两份 oracle、孤儿识别、unresolved 棘轮。
并以 `--mutation-check` **自证能变红**：分别注入"只取第一个方法"与"取消 nest 传播"两个变异，
要求测试转红——均已通过。

### 9.4 顺带产出

- **新门禁**：`check_route_contract.sh` 以 `EXTRACT_STRICT=1` 运行提取器，
  任一 oracle 出现缺口或出现**新的**无法解析构造即失败；
  `extract_unresolved_allowlist.txt` 对 19 处已知良性构造（同名重载、动态装配链方法）做棘轮。
- **孤儿可达性证据（S-9）**：新增"前缀之外 / 未装配的注册"表。
  解析器沿整条装配链递归后仍未获得任何前缀的注册只有 16 条：
  3 条根级探活（`/`、`/health`、`/_health`）+ 11 条 CAS 根协议/遗留 admin 端点（有意为之）
  + **2 条 threepid 孤儿**（`/requestToken`、`/submitToken`）——机器上可直接区分
  "有意根级" 与 "从未装配"。
  **（B5-4 已处置 threepid 孤儿，此表现为 14 条纯有意根级注册；见 §11。）**
- **附带发现**：`push.rs` 的 `/pushers/` 是 `get().post()` 链，旧解析器只报 GET；
  `presence.rs` 的 `/presence/list` GET 分支未被任何 manifest 声明（manifest 漏声明）。

---

## 10. S-1 / S-2 联邦密钥校验缺口——已修复（2026-09-15，B5-1）

两处缺口单独看都"像"是一行校验的补丁，但真正的问题是**信任门禁被端点数整除成了两份**：
`/key/v2/server` 走验签、`/key/v2/query` 直接返回；名字匹配两边都没有。

### 10.1 复核与消除

| # | 原缺陷 | 复核结果 | 修复后 |
|---|---|---|---|
| S-1 | `query_server_keys` 不校验自签名 | ✅ 复现：`client.rs` 里该方法直接 `return`，同一份文档换端点即绕过校验 | 与 `get_server_keys` 收敛到唯一门禁 `validate_remote_server_keys` |
| S-2 | `get_server_keys` 不校验 `server_name == destination` | ✅ 复现：仅验签，未校验文档自称身份 | 门禁内**先名字、后自签名**，且位于写缓存之前 |

### 10.2 为什么 S-2 不是"多余的一行"

单靠自签名抓不住 S-2 描述的攻击：攻击者用**自己的**私钥把自己的公钥自签一份文档、
在 `server_name` 字段里声称自己是受害者 —— 自签名校验**必然通过**（签名与公钥自洽）。
放行后写入的是 `destination → 攻击者公钥`，即**跨身份缓存投毒**：此后针对 destination 的
签名校验会拿一份无关服务器的 `verify_keys` 去比，且真实 destination 轮换密钥时会静默失效。

顺序也不是随意的：自签名是在 `keys.server_name` 这个 key 下查找的
（`keys.signatures.get(&keys.server_name)`）。**先钉住名字**，该查找才等价于
"destination 签的"；不钉名字，它就退化成"文档自称是谁签的"。

比较采用严格相等，不做大小写折叠、不去端口 —— 理由是 Matrix server name 会**逐字**出现在
ID、`signatures` 的 key、以及 `m.server` 委派响应三处，拼写不一致本身就是身份替换或误配，fail-closed。

### 10.3 验证：断言"缓存未被污染"，而不是断言"调用了校验器"

`validate_remote_server_keys` 是纯函数，单测它只证明"校验器能拒"。而
AGENTS.md 要求的性质是"**缓存远程密钥之前**必须校验"——只有断言**缓存状态**才能覆盖它。
因此把"校验 + 写缓存"合并为一个单元 `admit_server_keys(expected_server, keys)`，
使失败路径下的缓存为空成为可直接断言的事实。

**守卫**（`synapse-federation/src/client.rs`，5 条新增）：

| 测试 | 断言 |
|---|---|
| `server_keys_wrong_server_name_rejected` | 攻击者自签文档对受害命名 → 拒绝；对自身命名 → 接受（防"名字校验过严"） |
| `admit_server_keys_rejects_wrong_name_without_caching` | 名字不符 → `Err` **且 `key_cache` 为空** |
| `admit_server_keys_rejects_forged_signature_without_caching` | 伪造签名 → `Err` **且 `key_cache` 为空** |
| `admit_server_keys_caches_valid_document_under_expected_server` | 正向对照：证明上面两条不是"校验器恒返回 Err" |
| `query_server_keys_path_rejects_mismatched_document_over_http` | 经 wiremock 的**真实 HTTP 响应字节**走 `handle_response` → 门禁；覆盖 JSON→`ServerKeys`→校验这一层，避免 S-15 那种就地构造数据的自证测试 |

**变异自证（铁律 8）**：把 `admit_server_keys` 内的门禁调用摘掉后，
`admit_server_keys_rejects_wrong_name_without_caching` 与
`admit_server_keys_rejects_forged_signature_without_caching` 两条**转红**（正向对照仍绿）——
证明这两条守卫守的是"缓存写入被门禁挡住"，而不是同义反复。

**命令**：`cargo test -p synapse-federation --lib` → **187 passed**；
`cargo clippy -p synapse-federation --all-targets` → 无告警。

---

## 11. S-9 threepid 孤儿路由——已处置（2026-09-15，B5-4）

方案给了两个选项："要么补装配点，要么从 `ROUTE_CONTRACT.md` 删除"。
裁定为**删除代码本身**，即二者中对契约影响更彻底的那个。

### 11.1 为什么不是"补装配点"

| 证据 | 命令 / 位置 | 结论 |
|---|---|---|
| 历史从未装配 | `git log --all -S 'create_threepid_router()' -- src/web/routes/assembly.rs src/web/routes/mod.rs src/server` → **空** | 出生即死，不是"曾经接好后掉了" |
| 路径非规范形状 | `Router::new().route("/requestToken", …)` | Matrix 3PID 端点是 `/_matrix/client/v3/register/email/requestToken`、`/account/3pid/email/requestToken` 等；裸根路径无客户端可达 |
| 自身是未完成桩 | `threepid.rs` 原文注释 "In a full implementation, send email here / For now, return the token in the response for testing" | 但 `RequestTokenResponse` **只有 `sid` 与 `submit_url`，没有 token 字段**——连注释声称的测试用途都不成立 |
| 功能并不缺失 | `assembly.rs:572-606` 已装配 `/register/email/*`、`/account/password/email/*`、`/account/3pid/email/*`，并经 `auth_compat::request_email_verification_with_submit_path` 使用同一 `threepid_storage` | 补装配点会新增一个非规范、无鉴权语义的根级面，纯属净负 |

### 11.2 删除范围（含被它拖着的死管线）

- `src/web/routes/threepid.rs`（整模块）
- `src/web/routes/mod.rs`：`pub mod threepid;` 与 `pub use threepid::create_threepid_router;`（后者是无人消费的 re-export）
- `src/web/routes/context.rs`：`AuthContext::threepid_storage` 字段及其 `FromRef` 初始化 —— 唯一消费者是刚被删的模块，留着就是把死管线从一处搬到另一处
- `tests/unit/context_route_tests.rs`：对应的字段存活断言
- `scripts/api_test/handler_schemas.json`：重新生成（原文件含指向已删文件的悬空引用）

### 11.3 守卫从"钉死缺陷"改为"钉死不变量"

原 `check_orphan_detection` 断言 `/requestToken`、`/submitToken` **必须**出现在孤儿桶里。
把缺陷当成契约钉死，是让异常永久化的标准做法——它记录问题，却从不逼出决定。
现改为 `check_non_namespace_bucket`：**精确**断言该桶 == 14 条已知有意根级注册
（3 条探活 + 11 条 CAS 根协议端点）。桶里出现任何新成员都会转红，强制回答
"这是有意新增的根级端点，还是又一个没人装配的 router"。

### 11.4 验证

| 项 | 结果 |
|---|---|
| 契约提取器 | 模块 67→**66**，路由 1148→**1146**，"前缀之外" 16→**14** |
| 两份独立 oracle | manifest 声明集 **0 缺口**；`ledger_export` fixture **0 缺口** |
| 守卫测试 | 18 项全过（含新的精确桶断言）+ 变异自证仍能变红 |
| `cargo test --test unit --features test-utils` | **1815 passed / 0 failed** |
| `cargo clippy --workspace --all-targets --features test-utils -D warnings` | 干净 |
| `cargo check --workspace` | 干净 |
| `./scripts/check_fmt_ratchet.sh` | OK（0 debt） |

**顺带处置**：`scripts/ci/sqlx_dynamic_ratio_baseline` 1478 → 1484（+6）。
归因经逐提交实测确认全部来自 B5-2 的保留期清理：1 处生产 `DELETE` + 5 处
`pruning::db_tests` 夹具。生产那处不静态化的理由（`pruning.rs` 8 个同类函数全为动态、
`.sqlx/` 是部分缓存、单条 `query!` 需整体重刷缓存）已写入基线文件。

**已识别但未处理**：`docs/openapi/client.yaml` 相对源码树**已过期**，且其漂移
**全部**是 B1 拆除 r0 造成的（与 B5-4 无关，`git diff` 中 0 行涉及 threepid、18 行涉及 `/client/r0`）。
它需要走完整的 `scripts/api_test/refresh_openapi_specs.py` 流程（依赖按 Docker features 导出的
ledger），且**不在任何 CI 门禁中**，故本轮不夹带半吊子重生成。建议作为独立条目处理。

---

## 12. S-14 契约的单向性——已修复正向（2026-09-15，B2-4a）

### 12.1 原判定需修正：不是"无任何测试"，而是"只有单向"

S-14 原文是"无任何测试校验真实 router == ledger"，依据是
`grep -rn '\.routes()\|into_make_service' src/ tests/` 只有 3 处命中。这个依据找的是**枚举型** API。

但 `tests/integration/api_route_ledger_tests.rs::declared_route_manifest_entries_are_actually_wired`
用的是**探针型**：对 ledger 里每一条声明发 `PATCH`，断言收到 405 且 `Allow` 头含声明的方法。
（选 `PATCH` 是因为 RFC 5789 保留、本仓无端点使用，axum 必然回 405 + `Allow`。）
这条测试证明的是 `声明 ⊆ 真实`——**manifest 不会谎报**。

缺的是**反向**：`真实 ⊆ 声明`。ledger 漏掉的路由没有任何断言，
而漏报比谎报更隐蔽：端点能正常工作，运行时零报错，
只是 SDK 永不生成对应客户端、`ROUTE_CONTRACT.md` 永不列出，
**直到某个客户端去调一个服务端明明在服务的端点**才被发现。

### 12.2 缺口量化：为什么它一直没被发现

提取器**本来就计算了**这组差集，只是**只打印、不拦截**。原注释写得很清楚：

> The residual `derived-but-not-declared` set is expected (manifests are hand-written and
> incomplete) and is **reported rather than enforced**.

把"manifest 是手写且不完整"当成了容忍它的理由——而这正是要修的状态。

更要命的是**量错了基准**，导致真缺口被噪声淹没：

| 对照基准 | `derived \ ledger` |
|---|---|
| 仅 golden 泳道（默认特征编译） | **102** |
| golden ∪ sdk 泳道并集（sdk = `all-extensions` 编译） | **22** |

差的那 80 条全是**特征门控**造成的假阳性——`voice`(voice-extended)、`cas`(cas-sso)、`saml`(saml-sso)、
`admin::notification`(server-notifications)、`assembly::voip_tracking`(voip-tracking)、
`oidc`(builtin-oidc) 在默认特征下根本没编译，`saml_enabled` 等 profile flag 也随之是 false。
102 条里 102 条看起来都"合理"，于是没人去看。**用并集之后噪声消失，剩下 22 条条条是真缺口。**

### 12.3 22 条缺口的形态：都是"manifest 写漏了兄弟形态"

全部 22 条经逐条打开注册点核实，无一是提取器伪影，形态高度一致：

| 模块 | 漏掉的声明 | 真实注册处 | 漏掉的原因 |
|---|---|---|---|
| `room.rs` | `POST /v3/rooms/{room_id}/redact/{event_id}/{txn_id}` | `room.rs:67` `put(redact_event).post(redact_event)` | 清单只写了 PUT |
| `room.rs` | `GET`+`PUT /v3/rooms/{room_id}/anti_screenshot` | `room.rs:139` `get(...).put(...)` | 整条路径没进清单 |
| `push.rs` | `GET`+`POST /v3/pushers/`（带尾斜杠） | `push.rs:16` | 清单只有不带斜杠的 `/pushers` |
| `e2ee/keys.rs` | `POST /v1`+`/v3 /keys/upload/{device_id}` | `keys.rs:24` | 清单只有 `/keys/upload` |
| `e2ee/keys.rs` | `GET /v3/keys/history` | `keys.rs:51` | 整条漏 |
| `presence.rs` | `GET /v3/presence/list` | `presence.rs:15` 同路径挂了 `get()` | 清单只有 POST |
| `account_data.rs` | `POST` × 2（user / room 两条路径） | `account_data.rs:18,22` 挂了 `.post(...)` | 清单只有 GET/PUT/DELETE |
| `media/mod.rs` | 4 条 `/_matrix/media/r0/{download×2,preview_url,delete}` | `create_media_r0_router` 合并了 legacy download + preview/delete 子路由 | 清单只列了 `/upload`+`/config` |
| `assembly.rs` | `POST /v3/upload/token`、`GET /v3/upload/provider` | `assembly.rs:518` nest `create_upload_provider_router()` | `assembly_compat_manifest` 未列 |
| `admin/server.rs` | `GET /_synapse/admin/v1/server`、`/whoami` | `server.rs:16,18` | 清单列了 13 个兄弟却漏这 2 个 |
| `admin/room/mod.rs` | `POST /v3/admin/room/{room_id}/redact` | `admin/room/mod.rs:154` | 整条漏 |
| `cas.rs` | `GET /v3/login/sso/redirect/cas` | `cas.rs:147` | 整条漏 |
| `room_summary.rs` | `GET /v1/rooms/{room_id}/summary` | `room_summary.rs:660`（v1 路由器） | 清单把读集只按 v3 展开 |

`room.rs` 与 `push.rs` 那几条尤其说明问题：**handler 明明在同一个 `.route()` 表达式里链式挂了多个方法，
清单却只抄了其中一个**——这正是 S-13 提取器那个"链式只取第一个 method"缺陷的**人工对照物**。

### 12.4 修复与门禁

1. **22 条全部补进所属 manifest**（不删路由——它们在生产上是被服务的，删了才是行为变更）。
2. **提取器改为两泳道并集**，并把原本"只打印"的差集**提升为 `EXTRACT_STRICT=1` 下的硬失败**，
   且**不设 allowlist**：这条属性说的是"契约没漏东西"，"基本没漏"就是它要防的失效模式本身
   （对比 `extract_unresolved_allowlist.txt` 那条是解析器已知良性不解析，性质不同）。
3. **守卫测试新增 `check_positive_contract`**，并带一条**"谓词非空转"自检**
   （注入一条伪造路由，断言谓词确实能标出它）——否则 `return []` 也能让断言通过。
4. 实测闭合：`derived \ ledger = 0` 且 `ledger \ derived = 0`；golden 泳道 `all` 1044→1065，sdk 泳道 `all` 1124→1146，两者并集 **1146** = `ROUTE_CONTRACT.md` 的路由总数。

### 12.5 一处执行事故（可复用教训）

补 `room.rs` 的两处声明时，我在**同一条消息里对同一文件发了两次 Edit**。
两次都基于同一份原始快照，后一次写入覆盖了前一次——`POST redact` 的声明被**静默丢弃**
（两次工具调用都报成功）。提取器把它精确抓了出来，才发现源码里根本没有那行。

**教训**：同一文件的多次编辑必须**串行**；批量编辑后必须用提取器/编译器这类**外部判据**回验，
不能信任"工具报成功"。这次是提取器救了场——这恰好也证明了 S-14 门禁的价值。

### 12.6 剩余（B2-4b）

B2-4 的另一半"断言**派生 ledger ⊇ SDK 声明消费的全部端点**"仍未做：
需要从 `matrix-js-sdk` fork 的 **manager 源码**里提取 URL 字面量作判据
（**不得**用 `route-table.ts`——那是 codegen「既有条目 ∪ ledger」的产物，只增不减，
本项目已两次确认它不能作为实际调用判据）。已另立为 B2-4b。

---

## 13. S-16 集成快照被**手改而非重生成**——已修复（2026-09-15，B2-4a 附带）

**症状**：在 `main` 上带齐环境变量跑 `cargo test --test integration --all-features api_route_ledger_tests`，
`declared_route_manifest_full_snapshot_matches_default_state` **失败**：

```
left  (实际)  count: 1127
right (快照)  count: 1378
```

即这条契约快照在仓库里**已经是红的**。它是 S-14 的同族问题（守卫看起来在、其实不在），
但成因不同：S-14 是"方向少了一半"，这一条是"金标准文件本身是假的"。

### 13.1 判定依据：那个 1378 不可能被生成出来

`tests/integration/api_route_ledger_tests.rs::render_ledger_snapshot` 里
`count` 是**算出来的**（`lines.len()`），不是手填的。所以只要真跑过 UPDATE，count 必然等于当次真实条目数。

而在 `52b59c8f`（A7：拆 r0 兼容嵌套）这个提交上：

| 观测 | 值 | 含义 |
|---|---|---|
| 该提交对 `route_ledger_default.snapshot` 的改动 | **582 增 / 582 删** | 行被搬动（重排序），**总数没变** |
| 该提交后文件表头 count | **1378** | 与父提交 `52b59c8f^` 的 1378 **完全一致** |
| 同一提交 commit message 自述 | `ledger −489`（`all.json` 1319→830） | 路由面**大幅收缩** |

三者互相矛盾：路由面缩了 489 条，快照行数却有 582 行被搬动而 count 一动不动。
**唯一自洽的解释是：这个文件是被脚本/手工重排的，没有走 `UPDATE_ROUTE_LEDGER_SNAPSHOTS=1` 重生成。**

### 13.2 为什么能长期不被发现

重生成需要真实数据库（`default_ledger()` → `setup_fresh_test_app_with_config`）。
缺 `TEST_DATABASE_URL` / `TEST_DB_TEMPLATE_SCHEMA` 时用例走 `skip_or_fail_without_db()` 直接 `return`。
于是**能重生成的人只有装了库的人，而没装库的人只能手改**——这正是 §2.5 那 18 处静默跳过的复现路径。

### 13.3 该文件同时携带的另外两类腐坏

| 腐坏 | 量化 | 说明 |
|---|---|---|
| 重复条目 | 1378 行里只有 **1128 唯一行**（250 条完全重复） | 当前 ledger 已不产出重复（`declared_route_manifest_validates_with_no_duplicates` 绿） |
| 已不存在的声明 | **28 条** | 22 条 `/_matrix/client/v3/friends/*` + 6 条 `assembly::{account,directory}_r0_only`（已改名 `compat`/`extra`） |

22 条 v3 friends 值得单独说：`create_friend_router()` 只为 `/friends`、`/friends/search`、
`/friends/requests/{incoming,outgoing}`、`/friends/check/{user_id}` 注册 v3；
`/{user_id}/info`、`/{user_id}/groups`、`/groups`、`/dm/{user_id}`、`/request*`、`/suggestions`
**只注册在 `/_matrix/client/v1/friends/…`**。也就是说快照声称存在的这些 v3 端点在生产上**是 404**。
—— 补声明之前必须确认这一层，否则就是把"快照过报"误修成"删除真实端点"。

### 13.4 修复与验证

1. 带齐 `TEST_DATABASE_URL` + `TEST_DB_TEMPLATE_SCHEMA=test_template_ci` **重生成**两张快照：
   `default` 1378（含 250 重复）→ **1127**（唯一 1127），`worker_enabled` → **1138**（唯一 1138）。
2. **差异分解逐项闭合**，证明除了"去重 + 改名 + 本批 22 条新声明"之外没有其它变化：

   ```
   旧唯一 1128  −  旧有新无 28  +  新有旧无 27  =  1127   ✓
   旧有新无 28 = 22 条陈旧 v3 friends + 6 条 r0_only 改名
   新有旧无 27 = 22 条本批新声明 + 3 条 directory_extra 改名 + 2 条既有漂移(push/config)
   ```

3. 只读复跑通过（**30.96s**，真跑；对比静默跳过时是 1.1s 且 0 断言）。
4. **非空转自证**：这条守卫在修复前对陈旧数据**确实失败**过——它自己就是自己的变异测试。

### 13.5 残留风险（未关闭）

快照的**重生成仍需数据库**。CI 步骤已 fail-closed，但本地开发者若没有库，
仍然只能手改这个文件、且手改后无任何东西会拦住他。彻底关闭需要把
"快照 == `synapse_ledger_export` 对同一 profile 的导出"做成**无库可比**的断言
（sdk `default` 泳道 1127 与集成快照 `default` 1127 目前**数值相等**，是可利用的锚点）。
本案不扩大改动范围，仅记录。

---

## 14. H-12 测试库端口约定已经烂掉——已收敛（2026-09-15，B0-8）

### 14.1 原描述与其不足

计划里 H-12 只有一行：「`IsolatedTestPool` fallback 仍首选 `localhost:15432`」。
实际打开源码后发现这不是"一个文件的端口写错了"，而是**同一套约定散成 5 份、其中 2 份走的是另一个端口**。

### 14.2 实测证据（为什么 `15432` 是死端口）

```
$ nc -z localhost 15432        → 无监听（连接被拒）
$ docker/docker-compose.dev-host-access.yml:4
    - "${DB_EXPOSE_PORT:-5432}:5432"      ← compose 现在默认发布 5432，不是 15432
$ PGPASSWORD=synapse psql -h localhost -p 5432 -U synapse -d postgres -tAc \
      "SELECT datname FROM pg_database"
  ext_none / postgres / synapse_bench / synapse_ship_ci / synapse_test /
  synapse_test_p1p2 / synapse_v10 / template0 / template1
$ psql -h localhost -p 5432 -U synapse -d synapse
  FATAL:  database "synapse" does not exist   ← 旧链里的应用库兜底也是死的
```

即：两条 fallback 都是**打不通的**，而且其中一条指向的是**应用库**。

### 14.3 五份链的真实分布（修复前）

| 位置 | 顺序 | 问题 |
|---|---|---|
| `src/test_utils.rs` | 5432/synapse → 5432/synapse_test → 5432/secret | 应用库排第一 |
| `synapse-services/src/test_utils.rs` | 同上 | 同上 |
| `tests/common/mod.rs` | 同上 | 同上 |
| `synapse-storage/src/test_utils.rs` | **15432**/test → **15432**/synapse → 5432/test → 5432/secret | 死端口排第一 |
| `synapse-storage/src/test_isolation.rs` | **15432**/test → **15432**/synapse → 5432/test → 5432/synapse | 死端口排第一 |

`synapse-storage` 那份的注释还写着"callers do not agree on a fallback chain"——把分歧当成了设计，
而分歧的另一半（4 份 5432 链）里根本没人提 15432。

### 14.4 两类真实代价

1. **每次冷启动都要先撞一次墙**：`15432` 无监听，所以每个 DB-backed 测试进程在拿到正确 URL 之前
   都要先在死端口上失败一次。这正是"集成测试看起来在跳过"的历史成因之一。
2. **可能静默落到应用库**：兜底链里带 `…:5432/synapse`。`db_tests`（51 个文件）直连 `TEST_DATABASE_URL`
   的 `public` schema、**不走模板**，所以一旦落到应用库，就没有 `init_template_schema` 那层
   "库名含 test 才允许重建"的守卫拦着。这与 `P0-4`（CI 指向应用库）是同一类缺陷，只是本地版。

### 14.5 修复

- **统一链**：5 份 Rust resolver 一律 `TEST_DATABASE_URL` → `DATABASE_URL` → `5432/synapse_test`
  （另一条 `5432/synapse_test` + `secret` 密码留给旧本地环境）。**应用库与 15432 全部删除**。
- **脚本**：`init_test_public_schema.sh`（`TEST_DB_PORT`）、`cleanup_test_schemas.sh`（`PGPORT`）、
  `tune_test_db.sh`（`PGPORT`）、`seed_test_db.sh`（`DB_PORT`）、`run_bench_server.sh`（`BENCH_DB_PORT`）
  的默认端口 15432 → 5432；`run_local_coverage.sh` 的 `DATABASE_URL` 默认值同理。
- **测试内硬编码**：`synapse-e2ee/…/verification/service.rs`、`synapse-services/…/account_identity_service.rs`（×2）
  的 `…:15432/synapse_test` → `…:5432/synapse_test`；`synapse-services/…/saml_service.rs`（×3）
  的 `…:5432/synapse` → `…:5432/synapse_test`。

### 14.6 守卫（可回归）

新增 `tests/unit/test_db_url_convention_tests.rs`，6 项静态断言，无需数据库：

| 断言 | 作用 |
|---|---|
| `rust_resolvers_share_one_fallback_chain` | 5 份链**逐元素相等**（顺序也相等） |
| `every_resolver_is_scanned_by_the_guard` | 防"链被删空 → 空链也能通过"的空转 |
| `no_target_uses_the_dead_port_or_the_application_database` | 代码行不得出现 `:15432` 或 `:5432/synapse"`（**注释可以**，否则会逼人删掉解释而不是修链） |
| `every_script_carries_the_5432_default` | 6 个脚本各自的端口默认值钉死 |
| `environment_variables_are_consulted_before_any_fallback` | `TEST_DATABASE_URL` 必须出现在首条 fallback 之前 |
| `the_checker_rejects_the_old_chain` | 谓词非空转自检：喂进修复前的旧链，断言**两类**违规都被标出 |

**变异自证**：把 `synapse-storage/src/test_isolation.rs` 的链改回旧形态 → 2 项转红，
并精确报出 `test_isolation.rs:56 targets the dead port 15432` 与 `:57 falls back to the application database`；
复位后 6/6 绿。

### 14.7 方法论教训

`git check-ignore -v <path>` **不能**用来判断"例外是否生效"：命中负向规则（`!…`）时它照样打印
该行并返回 **exit 0**。B0-2 的原始验证判据就错在这里（会得出"仍被忽略"的假结论）。
唯一可靠判据是 `git add --dry-run <path>`。同类教训：`grep -c placeholder` = 0 这种"字面量归零"
判据会误伤真测试名（`api_placeholder_contract_p0_tests`），判据必须落在**语义**上而不是字符串上。

---

## 15. B1-3 的遗留：一个"只写"的抑制旋钮 + 8 处会骗人的陈旧注释（2026-09-15）

### 15.1 为什么查到这里

B1-3（拆 r0 兼容链）的验收判据是 `grep -rn '"/_matrix/client/r0"' src/` = 0 —— 这条**早就满足**。
但同一条 B1-3 还附带要求"删除 `suppress_r0_deprecation_warning` / `suppress_vendor_endpoint_warning`
配置字段 + `homeserver.yaml` 条目"。复检时发现后半句没做完，而且顺着它翻出了两个更值得记的东西。

### 15.2 `suppress_vendor_endpoint_warning`：声明了，从没被读过

| 出现位置 | 性质 |
|---|---|
| `synapse-common/src/config/server.rs:286-298` | 字段声明 + 12 行文档 |
| `docker/config/homeserver.yaml:21` | 部署配置写 `true` |
| `docker/deploy/docker-compose.yml:217` | `SYNAPSE__SERVER__SUPPRESS_VENDOR_ENDPOINT_WARNING: ${…:-true}` 透传 |
| `src/web/routes/assembly.rs:387` | **读的是 env var，不是这个字段** |
| `src/web/routes/assembly.rs:397` | 只出现在告警文案字符串里（"Set `server.…: true` to suppress"） |

即：**4 件产物**（字段 + 文档 + yaml 键 + compose 透传）围着一个每次启动必发的 `tracing::warn!` 转，
而决定"要不要警告"的代码**根本没读这个字段**。全仓零测试。

### 15.3 那条告警本身也不该存在

告警内容是"ISSUE-13 的旧 `/v3` 别名已弃用"。但：

1. 它在**每次启动**都发，而触发条件是"manifest 校验通过"——与运营者能做的任何事无关，
   是个永不消失的 WARN；
2. 这条信息**本来就在每个别名的声明处**（`key_rotation.rs:295`、`external_service.rs:447`、
   `friend_room.rs:146`、`burn_after_read.rs:54`、`voice.rs:64` 都有 ISSUE-13 注释）——
   只是 `sync.rs` 的 `/my_rooms` 与 `handlers/search/mod.rs` 的 `/search_rooms`、
   `/search_recipients` 这三处漏了。
3. 于是它属于典型的"**先给自己加告警，再加配置静音**"（REDUNDANCY 审计 §158 已点名）。

### 15.4 处置

- 删除 `assembly.rs` 里的整段告警（连同它的 env 读取），在原处留一条
  **"此处刻意不告警"** 的说明，讲明理由，防后人再加回来；
- 删除 `ServerConfig::suppress_vendor_endpoint_warning` 字段、`homeserver.yaml` 键、
  compose env 透传；
- 把弃用信息**补到缺失的三处声明点**（`sync.rs`、`handlers/search/mod.rs`），
  这样删掉的是噪音、留下的是信息。

### 15.5 附带发现：注释声称的契约 ≠ 代码注册的契约

`grep '"/_matrix/client/r0"'` = 0 之后，**8 处注释仍在声称 r0**：

| 位置 | 注释原文 | 紧随其后的实际前缀 |
|---|---|---|
| `assembly.rs:155` | `/capabilities — under r0 + v3` | 只有 `/_matrix/client/v3` |
| `assembly.rs:162` | `/media/config — under v1 + r0 + v3` | `v1`、`v3` |
| `assembly.rs:179` | `Base VoIP compat surface — under r0 + v3` | `v3` |
| `assembly.rs:203` | `Auth compat — under r0 + v3` | `v3` |
| `assembly.rs:232` | `Account compat — under v1 + r0 + v3` | `v1`、`v3` |
| `friend_room.rs:39` | `v1 和 r0 路径 - 主路由` | 只有 `/_matrix/client/v1/…` |
| `friend_room.rs:67` | `r0 兼容路由` | `/_matrix/client/v1/…` |
| `space.rs:172` | `Apply the same routes to v1, r0, and v3` | `v1`、`v3` |

**这次复核一度据此判断"r0 仍在服务"**——差点得出 SDK 会被打断的错误结论（真实结论是
SDK 已在 `039c2b2ec` 迁到 v3）。教训：**删除一个前缀时注释不会自己跟着改**；复核的判据
只能取机器生成的 `ROUTE_CONTRACT.md` / ledger 快照，不能取注释。注释只能作为"去寻找证据"的线索。

### 15.6 守卫

新增 `tests/unit/self_silencing_config_tests.rs`，4 项静态断言：

| 断言 | 作用 |
|---|---|
| `no_endpoint_suppression_knob_remains` | 递归扫 `src/` + 6 个 crate + `docker/`，**代码行**不得出现 `suppress_…(endpoint\|r0\|alias)…` 形状的旋钮（注释允许，否则会逼人删解释而非删旋钮）。谓词刻意收窄，不会误伤上游 Synapse 的 `suppress_key_server_warning` |
| `startup_validation_does_not_warn_about_unchangeable_state` | 从 `ledger.validate()` 到 `Err(err) =>` 之间不得出现 `warn!` / `SUPPRESS_` |
| `the_removed_warning_left_its_deprecation_notice_behind` | `sync.rs` 与 `handlers/search/mod.rs` 必须同时含端点与其 `ISSUE-13` 说明——把"删噪音但别丢信息"钉死 |
| `the_predicate_rejects_a_reintroduced_knob` | 谓词非空转自检：字段、env 透传都必须被标出；注释行与 `suppress_key_server_warning` 必须**不**被标出 |

**变异自证**（三处同时注入）：yaml 加回 `suppress_vendor_endpoint_warning: true`、
校验块加回一条 `tracing::warn!`、`sync.rs` 的 ISSUE-13 改成 `SO-13`
→ **3/3 转红**且各自报出精确位置；复位后 4/4 绿。

### 15.7 另立的记录（不在本批修）

`suppress_key_server_warning` **也是只写字段**，且更铺张：声明在 `ServerConfig`（`server.rs:103`）
与 `FederationConfig`（`federation.rs:58`）**两处**，`homeserver.yaml:113` 有键，
**22 处结构体字面量**在写它，全仓无读取。同批测出 `ServerConfig` 另有 12 个字段从不以
`.字段` 形式被读取（`expire_access_token`、`serve_server_wellknown`、`soft_file_limit`、
`max_image_resolution` 等），成因未判（可能是间接消费）。已登记为 **H-15**，
先判"间接消费 vs 真死字段"再统一处置——本批只做 B1-3 点名的那个。

---

## 16. S-14 的反方向：SDK 实际调用的端点 ⊆ ledger（2026-09-15，B2-4b）

§12 修的是「真实 router ⊆ ledger」（后端别偷偷多出端点）。本条修的是**反方向**：
「SDK manager 源码里真正调用的端点 ⊆ ledger」（后端别欠 SDK 端点）。两者合起来才闭环 ——
B2-4a 之后，一个"SDK 在打、后端没有"的端点仍然是无人看守的。

### 16.1 为什么不能用 SDK 的 `route-table.ts` 当判据

fork 里每个模块都有 `__generated__/route-table.ts`，看起来就是现成的"SDK 调用了什么"
清单。但 H-5 与 B1-2 都实测过它**只增不减**：r0 拆除后它仍留着 r0 条目。它是"曾经登记过"，
不是"现在在调"。用它做判据，会在前缀迁移后**静默通过**——正是守卫最该防的失效模式。
所以本项只读 manager **源码里的 `encodeUri("…")` 字面量**（实测 117 处，全部双引号；
单引号 0、模板字符串 0）。

### 16.2 匹配谓词：为什么"猜前缀"不行、最后用"首段锚定"

SDK 有三种构造风格：

| 风格 | 形态 | 出现模块 |
|---|---|---|
| 1 | `await this.request({ method: Method.X, path, prefix: ClientPrefix.V3 })` | `room-member` / `reporting` / `room` / `directory` … |
| 2 | `buildXPath()` 只返回相对路径，prefix 由调用方给 | `client-*-requests.ts` 一族（48 处） |
| 3 | `http.getUrl(path)`，前缀来自 client 的默认 `opts.prefix` | `account/index.ts` 的 fallback 认证 URL |

原型期先试了"取 `encodeUri` 附近几行里的 `prefix:` 来拼完整路径"，结果**串味**：把后一个
请求的 `VendorPrefix` 配到了前一个 v3 路径上（两者相距 3 行）。假阴假阳都有。而且实测
`prefix:` 的取值形态多达十几种 —— `ClientPrefix.V3`×262、`AdminPrefix.V1`×48、`VendorPrefix`×44、
`""`×37、`THREAD_PREFIX_V1`×20、动态函数 `verificationPrefix(version)`×9、以及
`ClientPrefix.Unstable + "/org.matrix.msc2965"` 这类**拼接**（拼接会漏 MSC 段）。

最终判据改为「首段锚定对齐」`path_match(relative, backend_path)`：

1. 取 SDK 相对路径的**首段**，在 ledger 路径里找它出现的落点；
2. 落点之后剩余段数必须**恰好等于** SDK 段数；
3. 逐段比对：`{}` 是通配（`{name}` 与 `$name` 都归一成它），两个字面量必须相同。

第 1、2 条一起把假阳堵死。反例：`/rooms/{}/guest_access` vs
`/_matrix/client/v3/rooms/{room_id}/state/{event_type}` —— `rooms` 落点之后后端还剩 4 段、
SDK 只有 3 段，于是**拒绝**，而不是让 `state`/`guest_access` 去撞通配符。正例：
`/rooms/$roomId/messages` 正常命中 `/_matrix/client/v3/rooms/{room_id}/messages`。

改判据后，"未被覆盖"从宽松匹配的 **8 条**收敛到 **2 条** —— 少掉的 6 条全是假阴
（3 条 `keys/backup/secure/$backupId[...]`、3 条裸路径构造器）。

### 16.3 实测结果

```
SDK 站点 117（其中 35 个解出唯一 method，做了方法校验）
后端 ledger 路径 1146
allowlist 条目 2，命中 2
✅ 所有 SDK 调用的端点都在 ledger 中（且 method 一致）
```

- 117 站点里 **35 个**能解出唯一 method（窗口内恰好一个 `method:`），据此额外做了
  「路径存在但后端不服务这个 method」的收紧校验 → **0 条不匹配**。其余 48 处是风格 2
  的纯路径构造器，method 由调用方给，脚本**放弃**推断而不是猜（窗口内出现多个 method、
  或 `prefix:` 带 `+` 拼接时一律跳过）。
- 2 条真实缺口（见 §16.5、§16.6）。**没有第 3 条。**

### 16.4 守卫的非空转自证（5 项变异）

| # | 注入 | 期望 | 实测 |
|---|---|---|---|
| 1 | allowlist 加一条后端不存在的假端点 `DELETE /rooms/$roomId/B2_4B_FAKE_PROBE` | 转红（stale） | ✅ 报出该条 stale |
| 2 | allowlist 条目去掉 `# 理由` | 解析期报错 | ✅ `条目缺少理由` |
| 3 | 在 SDK 副本里注入 `encodeUri("/rooms/$roomId/B2_4B_SDK_PROBE_MISSING")` | 报未覆盖 | ✅ `src/reporting/index.ts:50` |
| 4 | 把 `Method.Post` 改成 `Method.Delete`（`/rooms/$roomId/invite`） | 报方法不匹配 | ✅ `DELETE … ← 后端只服务 [POST]`，`room-member/index.ts:64` |
| 5 | 把谓词退化回"段数相等的整段比较" | 自检转红 | ✅ `谓词认不出真实存在的相对路径` |

自检还含一条归一化断言：`canonical_shape` 必须把 `$var` 与 `{var}` 同时折成 `{}`，
否则 SDK 把 `$roomId` 改名成 `$id` 会让豁免条目**静默失效**。

### 16.5 SDK 侧死代码：`AccountManager.setGuestAccess`（H-16）

`src/account/index.ts:351` 请求 `PUT /rooms/$roomId/guest_access` —— 后端无此路由。
Matrix 规范里访客准入只有 state event `m.room.guest_access`。同一 SDK 里另有三条**正确**实现：

| 位置 | 实现 | 状态 |
|---|---|---|
| `RoomManager.ts:1106` `setGuestAccess` | `sendStateEvent(roomId, m.room.guest_access, …, "")` | ✅ 后端支持 |
| `client.ts:2914` `setGuestAccess` | 委托 `RoomManager` | ✅ 指向正确实现 |
| `client-room-access.ts:15` `setGuestAccessRequest` | 委托 `sendGuestAccessState` | ✅ 同上 |
| `account/index.ts:351` `AccountManager.setGuestAccess` | `PUT /rooms/$roomId/guest_access` | ❌ 非标 + 不可达 |

结论：`client` 的**门面已指向正确实现**，AccountManager 那份是历史遗留的重复实现、
不可达的死代码。**正确处置是在 SDK fork 侧删掉它，而不是为它开后端路由** ——
后者等于往 v3 命名空间里塞私有路径，正与 ISSUE-13 相悖。

### 16.6 后端缺规范端点：fallback 认证页面（H-17）✅ **2026-09-17 已修复**

`src/account/index.ts:290` `getFallbackAuthUrl` 拼 `/auth/$loginType/fallback/web` 交给
`http.getUrl()`；后者用 `prefix ?? this.opts.prefix`，而 http opts 的默认 prefix 是
`ClientPrefix.V3`（`src/client.ts:841`）。所以实际 URL 是
`/_matrix/client/v3/auth/{loginType}/fallback/web` —— 正是 C-S 规范定义的 fallback 认证页面。

> **修复记录（2026-09-17）**：已在 `synapse-web/src/routes/auth_compat.rs` 中实现
> `auth_fallback_web` handler（`GET /_matrix/client/v3/auth/{auth_type}/fallback/web`），
> 在 `assembly.rs` 中注册路由，更新 `mod.rs` re-export，重新生成 `gen_derived_routes.py`
> （1,149 行派生表），并更新 ledger fixtures（default 1,048 / worker 1,059 / all 1,066）。
> `scripts/contract/sdk_uncovered_allowlist.txt` 中 SDK-BE-2 豁免已移除，
> 仅保留 SDK-SDK-1 一条豁免。`check_sdk_route_coverage.py` 验证通过。

补它需要一张 HTML 页 + session 承接逻辑，属**功能开发**而非契约守卫范围；且 Tjg 前端
登录流程目前不走 fallback 认证（`grep -rn 'getFallbackAuthUrl' Tjg/src` 无命中）。
故登记为 H-17，不阻塞。**不要**反过来让 SDK 改调 `/_matrix/static/client/login/` ——
那是静态页位置，不是客户端该硬编码的契约路径。

### 16.7 门禁接线：为什么"不依赖 SDK 的部分"要前置

`scripts/contract/check_route_contract.sh` 新增本步骤。但 CI 只 checkout 本仓、
**拿不到 SDK fork**，于是站点逐条比对在 CI 里只能跳过。为不把 CI 变成空转，脚本把
**不依赖 SDK 的两块刻意放在 SDK 存在性判断之前**，任何环境都会跑：

1. **谓词自检**（4 项断言，同时覆盖漏报与误报两个方向）；
2. **豁免清单卫生检查**：被豁免的形状必须**仍然不被 ledger 服务** —— 后端补上端点后
   豁免若不删，此后真实的不一致会被它静默吃掉。

实测四种组合：

| 环境 | 结果 |
|---|---|
| SDK 在场、清单干净 | EXIT=0，117 站点全绿 |
| SDK 缺席、清单干净 | EXIT=0，但横幅明写「**未跑**：SDK 站点逐条比对 —— 本次结论不覆盖 SDK ⊆ ledger」 |
| SDK 缺席、清单里有一条后端其实已服务（`GET /rooms/$roomId/messages`） | **EXIT=1**，报出「后端现在已服务 GET /_matrix/client/v3/rooms/{room_id}/messages，豁免应删除」 |
| SDK 缺席、`SDK_CONTRACT_STRICT=1` | **EXIT=1** |

关键点：SDK 缺席时输出的是 SKIPPED 横幅 + 明确列出"已跑/未跑"分别是哪一半，
**不会让人误当成通过**。

### 16.8 豁免清单不是垃圾桶

`scripts/contract/sdk_uncovered_allowlist.txt` 现 2 条，格式 `<METHOD|*> <形状> # 理由`，
**理由必填**（缺则解析期直接报错）。文件头写死四条规矩：每条必须写清"为什么现在可以不修"
与"什么时候必须修"；形状按 SDK 源码原文写（`$var`/`{var}` 会归一比较）；后端一旦补上
端点条目会变 stale 并让检查失败；新增一条 = 承认一处不一致，必须同时登记 follow-up。

---

## 17. B2-1 第一步：契约真值源的"并集视角"被拆成 lane × profile（2026-09-15）

### 17.1 问题的形状

契约提取器（`scripts/contract/extract_registered.py`）一直只有一个视角：**并集**。
`#[cfg(feature = "…")]` 被当噪声丢掉（`strip_leading_attrs` 直接略过属性文本），
运行时 `ProfileFlags` 压根没建模。于是它只能回答"源码里一共注册了多少条路由"（1146），
回答不了"`default` 特性编译、`worker` 档配置下到底服务多少条"。

后果是两类不一致在**数学上**不可见：

- 一条路由被**承诺在一个编译不出它的泳道里**（例如某条只在 `all-extensions` 下存在的端点，
  在 golden fixture 里也声明着 —— 单看并集，两边一样多）；
- 一条路由被**承诺在一个永不合并它的 profile 里**（`worker_enabled = false` 时
  `create_worker_body_router` 根本不合并，但并集里它有 11 条）。

这不是理论担忧。见 17.4 的实测：改一处 `mod` 的 cfg 门控，union 门禁四项指标
**全部保持干净**（`router-derived 1146` / `declared-not-derived 0` / `ledger NOT derived 0` /
`derived-not-in-ledger 0`），而真实语义已经错了 27 条。

### 17.2 两条轴分别怎么读

| 轴 | 载体 | 读法 |
|---|---|---|
| **lane**（编译期） | `#[cfg(feature = "…")]`，出现在 ① `mod` 声明 ② `fn` 定义 ③ 语句/块 | `Cargo.toml` 的 `[features]` 求传递闭包：`golden` = `default`；`sdk` = `default + all-extensions`（cargo 的语义是**叠加**而非替换） |
| **profile**（运行时） | `route_module.rs::*::merge_into` 里 `if <flag> { router.merge(…) }` | 解析 `merge_into` 的 `if` 分支，识别被门控的 router 构造根 —— **不是手写清单** |

三个容易踩空的地方：

1. **lane 必须在 `mod` 声明上把关，不能只在 `fn` 上。** 试过按 `fn` 级 cfg 过滤：
   `#[cfg(feature = "voice-extended")] impl RouteModule for VoiceModule` 被丢掉后，
   `voice::create_voice_router` 就**没有静态调用方**了，于是被提升为新的 root，
   把 27 条路由**加进**了根本编译不出它们的 golden 泳道 —— 过滤器反而变成放大器。
   `mod` 声明是唯一覆盖整个条件面的位置。

2. **profile guard 必须归属到"产生路由的那个构造根"，不能挂在 root 上。**
   `route_module.rs::merge_into` 本身也是一个 root（`create_router` 通过 trait 动态派发，
   静态看不见调用方）。实测：把 11 个 `merge_into` root 整个排除会丢掉 **194 条**路由 ——
   `friend_room` / `voice` / `cas` / `saml` / `widgets` / `burn_after_read` /
   `external_service` / `worker` / `oidc` 的构造器**只**经由它被静态调用到。
   所以 guard 记在"每次生成路由元组"的现场，且允许多个 guard 并存：
   `/.well-known/openid-configuration` 同时被总是合并的 fallback router 与 OIDC-only
   router 服务，两条 guard 都在 → 判定为 Always（正确）。

3. **受 flag 影响的不是整模块，而是两个 router 构造根。**
   实测归属（`registered_by` × profile 签名）：

   | 模块 | Always | `RequiresWorker` | `RequiresOidc` |
   |---|---|---|---|
   | `worker.rs` | 15（`create_worker_admin_router`，经 `create_router` 合并） | 11（`create_worker_body_router`） | — |
   | `oidc/mod.rs` | 2（`/.well-known/{jwks.json,openid-configuration}`） | — | 8（`create_oidc_router`） |
   | 其余 64 个模块 | 全部 | — | — |

   三个 profile 实测**单调**（`default ⊆ worker ⊆ all`，违反数 = 0），于是"这条路由在哪个
   profile"能压成一个 `{Always, RequiresWorker, RequiresOidc}` 三值标注，
   不需要对 `manifest_for_profile` 的分支做通用求值。

### 17.3 判据：六组集合逐条精确相等，无豁免清单

```
ledger_export      default 1047   worker 1058   all 1065
ledger_export_sdk  default 1127   worker 1138   all 1146
```

对面是**手写 `*_route_manifest()`** 产出的 fixture，所以这是两套独立实现互证，不是自证。
已接进 `EXTRACT_STRICT=1`，`check_route_contract.sh` 每轮都跑。整套门禁耗时 1.5s。

### 17.4 源码级变异：新门禁抓到旧门禁看不见的东西

| 变异（改真实 Rust 源码） | union 门禁 | 新 lane/profile 门禁 |
|---|---|---|
| 把 `mod voice` 的 `#[cfg(feature = "voice-extended")]` 改成 `widgets`（`default` 里有） | **四项指标全绿**：`router-derived 1146`、`declared-not-derived 0`、`ledger NOT derived 0`、`derived-not-in-ledger 0` | **EXIT=1**，三条 profile 全部 FAIL（`golden/worker 1085 vs 1058`、`all 1092 vs 1065`、`default 1074 vs 1047`） |
| 把 `OidcModule::merge_into` 的条件 `oidc::oidc_enabled(&sso_ctx)` 改成 `true` | 无感（并集不变） | **EXIT=1**，4 组集合 FAIL，并点名多出 `GET /_matrix/client/v3/oidc/authorize` 等 8 条 |

### 17.5 新增守卫

`test_extract_registered.py`：22 项 → **39 项检查**，变异自证 2 项 → **4 项**。

- cfg 谓词双向可判别：`feature = "voice-extended"` 在 golden 关 / sdk 开；
  `not(feature = "friends")` 两泳道都关（`friends` 在 `default` 里）；`all`/`any`/`not`
  组合与 Rust 语义一致；未知裸 flag（`test`）判为关；`features=None` 的并集模式恒真。
- `merge_into` 读出的 guard 集合**恰为**
  `{create_oidc_router: oidc_enabled, create_worker_body_router: worker_enabled}`。
- **每条派生路由都必须带 guard 记录**（漏记会被静默当成 Always，所以单列一项）。
- 每个泳道 `default ⊆ worker ⊆ all`。
- 变异 #3（无视 cfg）→ `golden lane reports 1146 vs 1065` 转红；
  变异 #4（丢掉 profile guard）→ `default reports 1146 vs 1127` 转红。

### 17.6 副作用：零

union 侧输出**逐字未变**：`router-derived 1146` / `manifest-declared 1077` /
`declared-but-not-derived 0` / `derived-but-not-declared 69` / `unresolved 19` /
非 Matrix 命名空间 14。`ROUTE_CONTRACT.md` 无漂移，SDK 覆盖检查（B2-4b）2 条豁免全命中。

### 17.7 顺带发现：`ProfileFlags::saml_enabled` 从不被读取（H-18）

三个 field 里 `oidc_enabled`（`route_module.rs:199`）与 `worker_enabled`（`:236`）都在
`manifest_for_profile` 里被读；`saml_enabled`（`:37` 声明、`:52` 由 `from_state` 写入、
`ledger_export.rs:113-114` 用于构造 profile、`:273-274` 有断言）**没有任何
`manifest_for_profile` 读它**。原因是 SAML 路由本就经
`oidc::oidc_enabled()`（`oidc/mod.rs:154`：`oidc_service.is_some() || builtin_oidc_provider.is_some() || saml_enabled`）
折进了 `oidc_enabled`。已登记 H-18，与 H-15 一批处置。

---

```bash
# A.1 23 个"应被吸收但缺失"的对象（有序活集模拟）
cd /Users/ljf/Desktop/hu_ts/synapse-rust
git diff-tree -r --diff-filter=D --name-only a0f2819d | grep -E '^migrations/.*\.sql$' > /tmp/deleted_migs.txt
python3 - <<'PY'
import re,subprocess,os
paths=[l.strip() for l in open('/tmp/deleted_migs.txt')
       if l.strip().endswith('.sql') and '.undo.sql' not in l and '/archive/' not in l]
paths.sort()
universe=open('migrations/00000000_unified_schema_v11.sql').read()+open('migrations/00000001_extensions_v10.sql').read()
sc=lambda s: re.sub(r'--[^\n]*','',re.sub(r'/\*.*?\*/','',s,flags=re.S))
live={}
for p in paths:
    s=sc(subprocess.check_output(['git','show','a0f2819d^:'+p],text=True))
    for m in re.finditer(r'DROP\s+(?:CONSTRAINT|INDEX|TRIGGER|FUNCTION|TABLE|MATERIALIZED\s+VIEW|VIEW|POLICY|RULE)\s+(?:CONCURRENTLY\s+)?(?:IF\s+EXISTS\s+)?([A-Za-z_]\w*)',s,re.I): live.pop(m.group(1),None)
    for m in re.finditer(r'DROP\s+COLUMN\s+(?:IF\s+EXISTS\s+)?([A-Za-z_]\w*)',s,re.I): live.pop(m.group(1),None)
    found=[]
    for pat in (r'CREATE\s+(?:UNIQUE\s+)?INDEX\s+(?:CONCURRENTLY\s+)?(?:IF\s+NOT\s+EXISTS\s+)?([A-Za-z_]\w*)',
                r'ADD\s+CONSTRAINT\s+(?:IF\s+NOT\s+EXISTS\s+)?([A-Za-z_]\w*)',
                r'ADD\s+COLUMN\s+(?:IF\s+NOT\s+EXISTS\s+)?([A-Za-z_]\w*)',
                r'CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?([A-Za-z_]\w*)',
                r'CREATE\s+(?:OR\s+REPLACE\s+)?(?:TRIGGER|FUNCTION)\s+(?:IF\s+NOT\s+EXISTS\s+)?([A-Za-z_]\w*)',
                r'CREATE\s+MATERIALIZED\s+VIEW\s+(?:IF\s+NOT\s+EXISTS\s+)?([A-Za-z_]\w*)'):
        for m in re.finditer(pat,s,re.I): found.append(m.group(1))
    for n in found: live[n]=p
miss=[(n,p) for n,p in live.items() if not re.search(r'\b'+re.escape(n)+r'\b',universe)]
print('ABSENT FROM BASELINE:',len(miss))
for n,p in sorted(miss,key=lambda x:x[1]): print(' ',n,'<-',os.path.basename(p))
PY

# A.2 burn ON CONFLICT 必然失败（42P10）
psql "$TEST_DATABASE_URL" -c "CREATE TEMP TABLE burn_after_read_log (id BIGSERIAL PRIMARY KEY, user_id TEXT NOT NULL, room_id TEXT NOT NULL, event_id TEXT NOT NULL, burned_ts BIGINT NOT NULL); CREATE INDEX idx_burn_log_user ON burn_after_read_log(user_id); INSERT INTO burn_after_read_log (user_id,room_id,event_id,burned_ts) SELECT u,r,e,t FROM UNNEST(ARRAY['@a:t']::text[],ARRAY['!r:t']::text[],ARRAY['\$e1']::text[],ARRAY[1]::bigint[]) AS x(u,r,e,t) ON CONFLICT (user_id,event_id) DO NOTHING;"

# A.3 门禁现状
bash scripts/contract/check_route_contract.sh          # 提交后 EXIT=0
bash scripts/ci/check_sqlx_dynamic_ratio.sh            # FAIL: dynamic=1476 > 1443（32 处既有 + 1 处本批 test-only）
bash scripts/check_fmt_ratchet.sh                      # 绿
python3 scripts/check_migration_consistency.py         # status=ok issues=0 warnings=0
python3 scripts/check_baseline_consolidation.py        # "吸收 0 个" 仍 EXIT=0
grep -rn "integration test database is not available" --include=*.rs tests/ | wc -l   # 18
git check-ignore -v artifacts/coverage_baseline.json   # .gitignore:44
ls migrations/*.sql | wc -l; ls migrations/*.undo.sql 2>/dev/null | wc -l              # 2 / 0

# A.4 本批 P0 测试（RED←删对象 / GREEN←恢复）
P="postgresql://synapse:synapse@localhost:5432/synapse_test"
psql "$P" -q -c "DROP SCHEMA IF EXISTS public CASCADE; CREATE SCHEMA public;"
psql "$P" -q -f migrations/00000000_unified_schema_v11.sql
psql "$P" -q -f migrations/00000001_extensions_v10.sql
SQLX_OFFLINE=true TEST_DATABASE_URL="$P" cargo test -p synapse-storage --all-features --lib -- \
  burn_after_read::db_tests room::db_tests::test_room_version_check_accepts_versions_beyond_eleven audit::db_tests
# 注意：跑 tests/unit 会经 T-1 清空 public，之后必须重新灌 baseline 才能跑 storage db_tests
```

> ⚠️ A.3 是 2026-09-14 那批的**当时快照**（sqlx 当时确实红、跳过点当时确实 18 处）。
> 其中 `git check-ignore -v artifacts/coverage_baseline.json` 这一行的**判据是错的**——
> 见 §14.7：负向规则命中时它照样打印并 exit 0。当前门禁状态请看 §14 与
> `OPTIMIZATION_EXECUTION_PLAN_2026-09-15.md` §3 各批次行。

## 18. 第二轮全量复核（2026-09-15，HEAD `66339069`）

### 18.0 方法与基线

- **复核对象**：本文档 §1–§17 的全部条目 + 期间新增的 B0/B1/B2/B5 系列改动（约 40 个提交）。
- **方法**：4 路并行只读复核（门禁组 / 架构与隔离组 / schema 与卫生组 / 回归猎捕组）+ 主复核独立复跑关键命令。
  所有 grep 均排除 `target/`、`.claude/worktrees/`、`.worktrees/`、`docker/deploy/`、`tests/element-web-harness/artifacts/`。
- **数据库**：`postgresql://synapse:synapse@localhost:5432/synapse_test`（只读 + 一个用完即删的 scratch schema）。
- **工具链**：`rust-toolchain.toml` pin `1.93.0`；本机 `rustfmt 1.8.0-stable (254b59607d 2026-01-19)`、`cargo 1.93.0` —— 与 pin 一致，故下面的 fmt 结论不是版本差异造成的。
- **工作树状态**：复核期间有另一个会话在改（`M src/web/middleware/rate_limit.rs`、`M src/web/routes/assembly.rs`、
  `M src/web/routes/delayed_events.rs`，以及未跟踪的 `optimize-route-manifests.py`、`scripts/replace_manifest_wrappers.py`）。
  凡"HEAD 自身"的结论都用 `git hash-object` 与 `git rev-parse HEAD:<path>` 比对确认工作树内容 == HEAD blob。

### 18.1 🔴 本轮新发现（此前报告没有的）

#### N-1 [P0] HEAD 上 **fmt 与 clippy 两道门禁同时为红**，且 fmt 与 derived-table 漂移门禁**互锁**

**实测**：

```bash
$ ./scripts/check_fmt_ratchet.sh
fmt debt: current=4010 baseline=0
::error::fmt debt increased: 4010 > 0        # EXIT=1
$ for f in src/web/routes/mod.rs src/web/routes/derived_routes.rs; do
    printf '%s  wt=%s head=%s\n' "$f" "$(git hash-object $f)" "$(git rev-parse HEAD:$f)"; done
# 两个 hash 各自相等 -> 这两个文件与 HEAD 完全一致，fmt 红不是工作树 WIP 造成的
$ rustfmt --edition 2021 --check src/web/routes/derived_routes.rs | grep -c '^Diff in'
997
```

- `src/web/routes/derived_routes.rs`（**10,757 行**）首行自述
  `//! GENERATED by scripts/contract/gen_derived_routes.py — DO NOT EDIT.`；
  生成器 `emit()` **不调用 rustfmt**（`grep -n rustfmt scripts/contract/gen_derived_routes.py` 无命中）。
- 漂移门禁是**逐字节**比较：`gen_derived_routes.py:457`
  `if not os.path.exists(OUT) or open(OUT).read() != text: STALE; exit 1`；当前 `--check` **EXIT=0**（即已提交文件 == 生成器输出）。
- 该门禁被 `scripts/contract/check_route_contract.sh:48` 调用，进而并入 `make check` 与
  `.github/workflows/drift-detection.yml` 的 `route-contract-drift` job。
- ⇒ **互锁**：留着未格式化的生成文件 → fmt 棘轮红；按 AGENTS.md 要求跑 `cargo fmt --all` →
  文件被改写 → `--check` 报 `STALE` → 漂移门禁红。**在改动生成器或排除该文件之前，不存在同时满足两道门禁的状态。**
- 另：HEAD 还因 `66339069` 遗留的死代码 clippy 红 —— `src/web/routes/assembly.rs:39`
  `fn base_route_manifest()`（私有、全仓无调用点，仅 :36/:91 注释提及；`assembly_compat_manifest`:149 /
  `vendor_route_manifest`:302 同样孤儿），仓库无 `[lints.rust] dead_code` 放行、无 crate 级
  `#![allow(dead_code)]` → `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  （CI 门禁 `ci.yml:218`）在 `dead_code` 上转红。当前工作树的 WIP 恰好删掉了这三个函数。

**建议修法**：① 让 `gen_derived_routes.py` 把 `emit()` 的输出过一遍 `rustfmt` 再写盘，并重新生成提交（优于在
配置里 `ignore` 掉它 —— 生成物本身应该是格式化的）；② 删掉三个孤儿 manifest 函数（WIP 已在做）。
两条都做完后两道门禁才可能同时绿。

#### N-2 [P0] baseline 的 DB-04-b "去级联"修复**并未真正生效**：`events.room_id` 上有两个 FK，CASCADE 那个仍在

`migrations/00000000_unified_schema_v11.sql` 里同一列被定义了两次：

| 行 | 约束 | ON DELETE | 出处 |
|---|---|---|---|
| `:343` | `fk_events_room` | `NO ACTION`（内联 CREATE TABLE，注释写着 "DB-04-b: replaced CASCADE with NO ACTION"） | 表定义 |
| `:4117-4120` | `fk_events_room_id` | **`CASCADE`** | 后面的 "确保约束存在" DO 块 |

在 scratch schema 里实灌 baseline 后实测：

```sql
SELECT conname, confdeltype FROM pg_constraint
 WHERE conrelid='<schema>.events'::regclass AND contype='f';
-- fk_events_room    | a   (NO ACTION)
-- fk_events_room_id | c   (CASCADE)     <- 两个都在
```

**为什么一直没人发现**：PostgreSQL 对同一事件按 RI 触发器名（含约束 OID）排序执行，
`fk_events_room` 的 OID `232146664` < `fk_events_room_id` 的 `232149836`，于是 NO ACTION 的检查触发器
（`RI_ConstraintTrigger_c_232146667`）先于 CASCADE 触发器（`...232149839`）执行。实测后果是**删房直接失败**：

```
$ INSERT 房 + 1 条 event; DELETE FROM rooms WHERE room_id='!r:audit';
ERROR:  update or delete on table "rooms" violates foreign key constraint "fk_events_room" on table "events"
DETAIL:  Key (room_id)=(!r:audit) is still referenced from table "events".
```

也就是说：**表面上"NO ACTION 生效"只是触发器命名顺序的巧合**，而 DB-04-b 明确要删掉的那个
CASCADE 约束**仍然装着**。风险：(a) 一旦该约束的 OID/名字序反过来（例如在某个既有库上
`fk_events_room_id` 先被创建），删房就会**静默级联**并重新引入 DB-04-b 要消除的
`events` AccessExclusiveLock 卡顿；(b) 每次 `events` 写入/删除都要多跑一对 RI 触发器。

**历史证据**（被删迁移 `20260831060000_events_no_cascade.sql` 的原文）：

> There are two constraints covering the same relationship in the v10 baseline:
> `fk_events_room` (inline CREATE TABLE, line 338): already NO ACTION in v10
> `fk_events_room_id` (IF NOT EXISTS block, line 4272): still CASCADE — **THIS is the problem**

该迁移同时 `DROP` 了两个约束并重建单一 `fk_events_room_no_action`。但 v11 折入时**只丢了
`fk_events_room_no_action` 没建**（§1.4 已记录它缺失），**却没注意 baseline 自己又把 CASCADE 那个造了回来**。
⇒ 正确的修复不是"补一个 `fk_events_room_no_action`"（那会变成第三个 FK），而是
**在 `:4117-4120` 的 DO 块里 DROP `fk_events_room_id`**，并加一条"`events.room_id` 上恰好一个 FK 且为 NO ACTION"的守卫。

#### N-3 [P1] baseline 另有 **5 组"不同名、同语义"的重复 FK**

同一份 baseline 的"确保约束存在"DO 块用 `<name>_id` 后缀重造了内联 FK，
但守卫只查 `conname`，名字不同就查不到，于是**同列出现两个 FK**（运行期实测，方法：
按 `conkey[1]` 聚合 `pg_constraint`）：

| 列 | 两个约束 | ON DELETE |
|---|---|---|
| `access_tokens.user_id` | `fk_access_tokens_user` + `fk_access_tokens_user_id` | CASCADE / CASCADE |
| `devices.user_id` | `fk_devices_user` + `fk_devices_user_id` | CASCADE / CASCADE |
| `refresh_tokens.user_id` | `fk_refresh_tokens_user` + `fk_refresh_tokens_user_id` | CASCADE / CASCADE |
| `room_memberships.room_id` | `fk_room_memberships_room` + `fk_room_memberships_room_id` | CASCADE / CASCADE |
| `room_memberships.user_id` | `fk_room_memberships_user` + `fk_room_memberships_user_id` | CASCADE / CASCADE |

语义等价（不改变行为），代价是每组多一个 RI 触发器对 + 一份冗余 catalog 条目，
且和 N-2 同源 —— **DO 块的幂等判据应当按 `(conrelid, conkey)` 而不是 `conname`**。
（另有 14 组"同名重复"是真正无害的：DO 块的 `IF NOT EXISTS` 直接跳过。）

#### N-4 [P1] `shutdown_room` 违反 phase-0 刚确立的 `room_directory` 不变量

`33854505` 把 `room_directory` 定义为"**有行 == 公开**"（`set_room_directory` 转私有即 `DELETE`，
`remove_room_directory` 复用），但漏了一个写者：

`synapse-storage/src/room/mod.rs:966-976`：

```rust
// 注释：we delete it from directory ...
sqlx::query("UPDATE rooms SET is_public = false, name = COALESCE(name,'') || ' (SHUTDOWN)' WHERE room_id = $1")
```

只改 `rooms.is_public`，**没删 `room_directory` 行**。可达路径：
`POST /_synapse/admin/v1/shutdown_room`（admin 鉴权）→ `synapse-services/src/room/state/info.rs:233-235` → `shutdown_room`。
后果：`EXISTS (SELECT 1 FROM room_directory ...)` 类读取（`admin.rs:437/986/1015`）与
`GET /directory/list/room/{id}` 仍把该房当作**公开**。既有测试
`synapse-storage/src/room/mod.rs:1897` 播种了 directory 行却从不断言其状态，所以没拦住。

同族的两个 P2：(a) `is_room_in_directory`（`room/mod.rs:1035`）读的仍是已失去意义的
`is_public` 列（应为 `SELECT EXISTS(...)`）；(b) `room_directory.is_public` 列与
`idx_room_directory_public ... WHERE is_public = TRUE` 索引在 baseline（`:489-499`、`:3392`）里已成死列/死索引；
另有第二个可见性写者 `set_room_visibility`（`room/mod.rs:768-789`）完全绕过该表（当前仅测试调用）。

#### N-5 [P1] sqlx 棘轮正则漏计 turbofish（实测 **652** 处未计入，少报 ~30%）

`scripts/ci/check_sqlx_dynamic_ratio.sh` 的两个正则
（dynamic `sqlx::query(_as|_scalar)?\(`、static `sqlx::query(_as|_scalar|_file)?!`）
**匹配不到** `sqlx::query_as::<_, T>(...)` / `sqlx::query_scalar::<_, T>(...)`
（`<` 出现在 `(` 之前，`query_as(` 不成立）。而 turbofish 写法在本仓很常见：

```bash
$ grep -rE --include='*.rs' 'sqlx::query_as::<|sqlx::query_scalar::<' \
    src synapse-*/src | wc -l
652          # 579 query_as::<  +  73 query_scalar::<
$ bash scripts/ci/check_sqlx_dynamic_ratio.sh | grep dynamic=
check_sqlx_dynamic_ratio: dynamic=1484 static=61 total=1545 ratio=0.9605   # OK（<=1484）
```

即棘轮报告的 1484 只是**下界**，真实动态调用约 2136；新增的 turbofish 动态查询
**完全不会触发棘轮**。基线文件 `:99-102` 自述过这个盲区，但没有修。

#### N-6 [P1] 两个"守卫/修复"脚本本身不可信

- `scripts/check_schema_blind_guards.py`（`aba8275d` 新增）：**未接入任何 workflow / Makefile / 其他脚本**
  （`grep -rn check_schema_blind_guards .github Makefile scripts/contract` 无命中），且**只 warning 不失败**：
  实跑 `exit=0`，输出 `Summary: 0 errors, 15 warnings`。即使接线也不会变红 —— 违反"门禁必须自证能变红"。
  更糟的是它的**判据本身是错的**：baseline 里被它点名的 3 处（`v11:4522`、`:4754`、`:5047`）写的都是
  `table_schema = current_schema()` —— 也就是**已经 schema-aware 的正确写法**，却被它判成
  "missing schema qualification in condition"（false positive）；而 baseline 里**唯一**真正硬编码的
  `n.nspname = 'public'`（`v11:4983`）**它一条都没报**（false negative）。⇒ 该脚本需要重写判据后才值得接线。
- `scripts/replace_manifest_wrappers.py`（未跟踪）：会重写 `src/web/routes/**` 里每个
  `pub fn *_route_manifest()`，但它生成的 wrapper **无参调用** `derived_route_manifest()`
  （真签名是 `derived_route_manifest(flags: &ProfileFlags)`，`derived_routes.rs:10630`）→ 一旦运行即 E0061，
  且按"文件名 stem"过滤 `registered_by`、丢弃 `with_auth`/`with_rate_limit_exempt` 注解。
  `optimize-route-manifests.py`（未跟踪，**仓库根目录**）是带硬编码绝对路径与未定义变量（`:132` `end_fn`）的打印脚本。
  两者都应删除或移入正式工具链并接线。

#### N-7 [P2] 一批文档/守卫漂移（都不改变行为，但会误导）

- `docs/synapse-rust/LEDGER_EXPORT_SCHEMA.md:91-92` 与 `scripts/generate_sdk_ledger_fixtures.sh:14-15`
  仍写 `default=1320 / SDK=1407`，**实测 1065 / 1146**。降幅有据可查
  （`git show <c>:tests/unit/fixtures/ledger_export/all.json` 逐提交读 `entry_count`）：
  `1320 → 1317`（删 3 条 push rules）`→ 1319`（push config +2）`→ 1044`
  （`52b59c8f` B1 删除 r0 兼容嵌套 —— 每路由的 r0 孪生条目一并消失）`→ 1065`
  （`9ba8b6b1` 补 S-14 正向缺口）。即**计数变更是有意的**，但文档（含 `:46` 的"1320 条只有 3 条"）
  没跟着改，且除 `schema_version` 行外没有任何门禁校验这些数字。
- `migrations/00000000_unified_schema_v11.sql:2945` 的默认值是
  `DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)`（**先取整再乘**，截断到秒），与
  `db_migrate.sh:329` / `database_initializer/mod.rs:298` 的毫秒写法不一致（仍是同一秒内迁移会撞值）；
  而守卫 `tests/unit/migration_consistency_tests.rs:79-87` 只断言 `contains("executed_at BIGINT")`
  且不含 `TIMESTAMPTZ`/`NOW()`，**两种写法都能通过** → 看不见这类漂移。
- `docs/audit/*` 里仍有过时 runbook：`P4_performance_baseline_2026-09-11.md:222-244,625-627`
  给出对已删 `docker/deploy/config/` 的 `cp/sed -i`；`P4_ci_gate_integrity_2026-09-11.md:188`、
  `P4_perf_gate_honesty_2026-09-11.md:205`、`P5_migration_search_path_shadowing_2026-09-12.md:232`
  仍在调用已删的 `scripts/check_config_consistency.py`。
- 工作树的 `assembly.rs` 删函数后遗留悬挂 `///` 文档块（`:30-113`，注释仍称 "Composes `base_route_manifest()`"）；
  严格提取器提示 `1 allowlist entries are no longer produced`（`scripts/contract/extract_unresolved_allowlist.txt` 陈旧项，
  只 note 不失败）。

### 18.2 已修复（本轮实测确认，不要再重测）

| 原条目 | 判定 | 证据 |
|---|---|---|
| §2.0 路由契约门禁红 | ✅ 已修复 | pristine clone @ `66339069` 跑 `check_route_contract.sh` → **EXIT=0**；fix `fba2aac6`（注意：脚本会就地重写 `ROUTE_CONTRACT.md`） |
| §2.1 覆盖率基线永远无法提交 | ✅ 已修复 | `.gitignore:44` `artifacts/*` + `:45` `!artifacts/coverage_baseline.json`（`78e59b64`）；clone 内 `git add --dry-run artifacts/coverage_baseline.json` → exit 0 |
| §2.2 perf 门禁纯 echo | ✅ 已修复 | `drift-detection.yml:324-349` 改为真断言（tables/indexes ≥200、elapsed ≤600，否则 exit 1），无 `generate_series`（`33854505`） |
| §2.3 db-migration-gate 占位步骤 | ✅ 已修复 | `grep -c P1-8` = 0；34 step 全部带 `run:`/`uses:`；`check_workflow_steps.py` OK（`78e59b64` + `33854505`） |
| §2.5 集成测试 18 处静默跳过 | ⚠️ 部分 | 18 处全部改为 `super::skip_or_fail_without_db()`（`tests/integration/mod.rs:170-180`，`33854505`）→ CI 下 panic；**残余** `tests/unit/test_schema_housekeeping_tests.rs:41-42,86-87` 仍裸 `return` |
| §2.6 sqlx 棘轮 FAIL | ⚠️ 部分 | 现 `OK: 1484 <= 1484`；但 N-5 的 turbofish 盲区使该数字不可信 |
| T-6 CI 指向应用库 `synapse` | ✅ 已修复 | `ci.yml` 12 处 `TEST_DATABASE_URL` 全指 `synapse_test`、0 处应用库，各步骤 pin `TEST_DB_TEMPLATE_SCHEMA: test_template_ci`；守卫 `tests/unit/test_db_url_convention_tests.rs`（`5e595605`） |
| S-5 knock 丢弃 `via` | ✅ 已修复（本轮新核实） | `handlers/room/members.rs:69` `extract_via_servers(query, legacy_body)`：标准 `?via=` 重复查询参数优先，旧 body `via_servers` 仅作兜底；join(`:117`) 与 knock(`:236`) 都已接入；单测 `:775-805` |
| S-12 / S-15 MSC4108 DELETE 头缺失 + 自证测试 | ✅ 已修复 | 新增 `tests/unit/msc4108_rendezvous_route_tests.rs:421 delete_session_returns_204_with_required_headers`（调用真实 handler） |
| A-11 `/r0/push/*` legacy 路由 | ✅ 已修复 | `push_notification.rs:336-339` 全为 `/_matrix/client/v3/push/*`；`r0` 路由注册数 = 0 |
| A-12 `src/services/` 薄壳 | ✅ 已修复（services 部分） | `git ls-tree HEAD src/services` 为空（已删除）；`src/storage/mod.rs` 仍是 66 行纯 re-export（有使用者，非死代码） |
| H-2 / H-3 `.scratch` 97 文件、`coverage/` 3 文件入库 | ✅ 已修复 | `git ls-files .scratch` = 0、`git ls-files coverage` = 0（`33343379`） |
| H-4 `docker/deploy` 18 GB | ✅ 已修复 | `du -sh docker/deploy` = **2.0M**，backups 目录已空 |
| H-5 常驻 worktree | ✅ 已修复 | `git worktree list` 仅 main |
| H-8 配置单一真相源 | ⚠️ 部分 | 第二个配置树已删除（`86fc6cd0`），替换为 `config_mount_tests.rs`(7) / `sync_rate_limit_config_tests.rs`(4) / `migration_consistency_tests.rs`(8)；**但仍无测试把 `docker/config/homeserver.yaml` 反序列化进 Rust `Config`** |
| H-12 `15432` 死端口 | ✅ 已修复 | 回退链只剩 `5432/synapse_test`；守卫 `tests/unit/test_db_url_convention_tests.rs`(6 tests) |
| H-14 `db_migrate.sh` 宿主 psql 误伤 | ✅ 已修复 | `db_migrate.sh:162 host_psql_target_is_implicit_loopback()` + `:186-201` 拒绝；守卫 `every_ci_db_migrate_call_supplies_an_explicit_target` |
| H-15 / H-16 Makefile 重复迁移路径 | ✅ 已修复 | `flyway-*`/`sqlx migrate`/`scripts/db` 引用全部消失；`Makefile:92,96` 改为 `$(DC) exec -T $(DB_SERVICE) psql` 查 `schema_migrations` |
| §16 反向契约（SDK ⊆ ledger） | ✅ 已修复 | `check_sdk_route_coverage.py` EXIT=0；`EXTRACT_STRICT=1 extract_registered.py` 双向 0/0 |
| §9 S-13 契约提取器 | ✅ 已修复 | 链式方法全出、`spaces` 相对路径 15→0；`gen_derived_routes --check` 复现全部 6 份 fixture |

### 18.3 仍然存在（合并清单，按严重度）

**P0**

1. **N-1** HEAD fmt 红（`current=4010`）+ clippy 红（`base_route_manifest` 死代码），且 fmt 与
   `derived_routes.rs` 字节级漂移门禁**互锁**。
2. **N-2** `migrations/00000000_unified_schema_v11.sql:4117-4120` 的 CASCADE 双 FK 使 DB-04-b 形同虚设。

**P1**

3. **§1.4/§1.5 的 18 个命名对象仍缺失**（8 个约束 + 10 个索引；有序活集模拟 `ABSENT = 20`，
   其中 `ck_rooms_room_version_valid_v2` 是改名误报、`fk_events_room_no_action` 实为 N-2）。
   已确认恢复的是 `ux_burn_log_user_event`(v11:3670)、正则 `ck_rooms_room_version_valid`(v11:251)、
   `trg_prevent_audit_delete`(v11:2318)。
4. **N-3** 5 组不同名重复 FK。
5. **N-4** `shutdown_room` 违反 `room_directory` 不变量（房间已关停却仍在公开目录）。
6. **N-5** sqlx 棘轮漏计 652 处 turbofish。
7. **N-6** `check_schema_blind_guards.py` 未接线且 warning-only；`replace_manifest_wrappers.py` 是会破坏编译的危险脚本。
8. **T-1** `src/test_utils.rs:563-564` 无条件 `DROP SCHEMA public CASCADE` 且不回填 —— 实测当前
   `public` 的 BASE TABLE 数 = **0**，故 `synapse-storage/src/test_utils.rs:201 connect_shared_test_pool`
   一类直连 `public` 的套件必然 `42P01`。
9. **T-2** `synapse-services/src/database_initializer/mod.rs:531` 在 `error_count>0` 时仍返回 `Ok`，
   `initialize()` 保持 `is_success=true` → 半成品模板被标记 ready。
10. **T-7** live DB 有 **25** 个 `test_*` schema 未回收（14 个空 `test_<pid>_…`、9 个
    `test_isolation_template_*`、`test_template_v2_*`、`test_template_ci`）；janitor 只覆盖进程内正常退出。
11. **§2.4** `ci.yml:141`（repo-sanity）调用 `supply_chain_gate.sh` 但不安装 `cargo-deny`/`cargo-audit`
    → 恒绿；真安装只在 `security-audit`（`ci.yml:656-660`）。
12. **§2.7** 迁移守卫 0 候选恒绿（`migrations/` 仅 2 个 `.sql`，extensions 的 `ADD COLUMN`/`ADD CONSTRAINT` = 0）。
13. **§2.8** 四个 baseline log 仍缺失；`scripts/.missing-docs-baseline` 仍为 6。
14. **§0.1 追加** `scripts/check_baseline_consolidation.py` 空转（"已吸收 0 个"、EXIT=0）且未接入任何 CI
    → 23 对象缺口无自动守卫。
15. **T-3/T-8/T-4/T-5** 隔离/模板构建仍有 2–3 份实现（Guard 4 只扫 services 副本）；`search_path …, public` 回退与
    public-wipe 逃生门仍在代码中。
16. **A-10 残留** `PushNotificationService::push_gateway` 只写不读（`service.rs:30`，`with_push_gateway` 0 调用方）；
    `send_upstream`（`service.rs:486-495`）对 `"upstream"` 仍伪造成功而不发送。

**P2**

17. **N-7** 文档/守卫漂移（ledger 计数 1320/1407、`executed_at` 秒级默认值 + 守卫盲区、过时 runbook 引用、
    悬挂注释、陈旧 allowlist 项、两个散落脚本）。
18. **H-1** `docker/config/homeserver.yaml:68,86` 的 `pool_size` 零引用废弃字段仍在发布配置中。
19. **H-6** `Cargo.toml:32` `server` feature 声明但 0 个 `#[cfg(feature = "server")]` 门。
20. **H-9** `ci.yml:274` 自认 ~3.5k intra-doc-link 警告，但无对应棘轮/baseline。
21. **H-10** `synapse-services/src/friend_room_service/mod.rs` = 1833 行。
22. **H-11** `test_mocks/member.rs:596-597` 负 `limit` 未 clamp；`test_mocks/device_list.rs:100-125` 仍是忽略
    `from/to/requester` 的 stub。
23. **H-13** 允许列表计数过期：实测 `#[allow(dead_code)]`=30、`allow(clippy::`=262、`#[ignore]`=29、
    TODO/FIXME/XXX/HACK=11、`#[deprecated]`=0（文档记 149/170/26/8）。
24. **§1.6** baseline 仍有 5 对逐字节重复索引（现 `:3331/3814`、`:3332/3815`、`:3302/3812`、`:3301/3813`、`:3298/3827`）；
    硬编码 `'public'` 从 3 处降到 **1 处**（`v11:4983` 的 `n.nspname = 'public'`，两处 `message_log` 已改 `current_schema()`）。
25. **长期债（A 系列）**：47 个路由文件穿透分层、69 个 manifest / 935 个 `.route(`、70 个 storage trait、
    176 处 glob re-export、22 个错误枚举、~4812–4950 条 rustdoc 样板、287 个上下文 pub 字段、
    `src/web/**/mod.rs` 合计 3,631 行；`/_matrix/client/v1/`(181) 与 `/v3/`(337) 双前缀并存（`r0` 已清零）。

### 18.4 证伪 / 降级 / 口径修正

- **A-3 "r0/v1/v3 三前缀并存"**：`r0` 路由注册数已为 **0**（只剩 15 处非路由引用），降级为"v1/v3 双前缀"。
- **S-5**：不再是"已文档化的功能缺口"，本轮核实**已实现**（见 §18.2）。
- **§1.5 `idx_federated`**：名字写错，被删迁移里的真名是 `idx_rooms_federated`。
- **H-4 的"18 GB grep 陷阱"**：已消失（现 2.0 MB）；但 `docker/deploy/` 之外的 `tests/element-web-harness/artifacts/`
  （266 MB，已 gitignore）仍会让 `grep -r .` 变慢。
- **口径修正（premise correction）**：删除 `docker/deploy/config/` 的提交是 **`86fc6cd0`**（`git log --diff-filter=D -1` 可验），
  不是 `33854505`；`33854505` 修的是随之悬空的 deploy 挂载与旧一致性脚本/测试。
- **A-4 / A-7 / A-8** 的原始统计口径无法完全复现（"非 mock 真实实现数"、doc 注释文件范围、字段集合的定义不同），
  本轮给出的是**同一意图下最接近的口径**，数字有 ±3 的噪声：67/70、~4812–4950、287。
- **Area 2 的契约链本身是干净的**：`EXTRACT_STRICT=1` 下"manifest 声明但未注册" = 0、
  "ledger ↔ derived" 双向 0/0、`gen_derived_routes --check` 复现 6 份 fixture、SDK ⊆ ledger 通过。
  "注册但无 manifest 条目" 的 140 条（all-extensions 车道，其中 `assembly.rs` 68、`federation/mod.rs` 39）
  是 B2 迁移进行中的**预期状态**，不是缺陷。

---

## 19. 第三轮全量复核（2026-09-17，HEAD `dd625003`）

### 19.0 方法与基线

- **复核基线**：`main` @ `dd625003`（2026-09-17 06:29，"B3-5 Step 1: friend_room_service error convergence complete"）。
  上一轮基线 `66339069`（2026-09-15 22:21）。区间 **51 个提交 / 491 个文件 / +21,863 −36,721 行**。
- **被复核对象**：本文档 §1–§18 的全部"仍存在"条目 + 区间内新增的 B2/B3/B4 系列改动。
- **工作树状态**：**17 个脏路径**，全部是并发会话的 B3-5 错误收敛 WIP（暂存 3 个新 `error.rs`
  + 6 个 `synapse-web/src/routes/*`）。**凡"HEAD 自身"的结论都在 `git archive HEAD` 的纯净快照上复跑**
  （快照与 target 目录用完即删，已确认工作树未残留）。
- **数据库**：`postgresql://synapse:synapse@localhost:5432/synapse_test`（PG 15.18 Homebrew）。
  除只读查询外，另建一个用完即删的 scratch schema `audit_v12` **实灌当前 baseline** 取证。
- **本轮最重要的方法论修正**：**"查库"不等于"查基线"**。本机 `synapse_test.public` 是旧快照
  （见 M-4），照它下结论会把已修的缺陷报成"仍存在"。凡 schema 结论一律以"baseline 文本 + 全新 schema 实灌"为准。
- **提醒**：`docs/audit/DB_REVIEW_2026-09-17.md` 是本区间新增的**数据库专项审查**，已接管
  §1.4/§1.5/§1.6 之外的 DB 维度（冗余索引深化、FK 无索引列、int4 主键、分区、连接预算）。
  本 §19 只做"上一轮条目是否关闭"的判定，**不重复那边的工作**；两处结论已交叉对齐。

### 19.1 🔴 本轮新发现（此前报告没有的）

#### M-1 [P0] 供应链门禁在"真装工具"的路径上**转红**：`rustls 0.23.43` 命中 RUSTSEC-2026-0285

第二轮把 §2.4 记为"恒绿"，本轮状态**变了**——不是修好了，而是**真的红了**：

```bash
$ bash scripts/ci/supply_chain_gate.sh     # 本机已装 cargo-deny / cargo-audit
==> cargo-deny check
error[vulnerability]: TLS 1.3 handshake messages incorrectly accepted across encryption level boundaries
  Cargo.lock:387  rustls 0.23.43
  ID: RUSTSEC-2026-0285
  Solution: Upgrade to >=0.23.45 (try `cargo update -p rustls`)
 advisories FAILED: 1 errors, 3 warnings, 2 notes
supply_chain_gate: cargo-deny FAILED
EXIT=1
```

- `RUSTSEC-2026-0285` **不在** `deny.toml` 与 `.cargo/audit.toml` 的 ignore 列表里，所以这是真拦截。
- `rustls` 是**传递依赖**（`redis 0.29` 的 `tokio-rustls-comp`、`lettre 0.11` 的 `tokio1-rustls-tls`），
  没有任何 manifest 直接钉版本 → 修法就是 `cargo update -p rustls` 提到 `>=0.23.45` 并提交 `Cargo.lock`。
- **关键在于这个错误的门禁布局让 PR 看不见它**（§2.4 复合）：
  `ci.yml:159` 的 `repo-sanity` 调用同一脚本但**不安装** `cargo-deny`/`cargo-audit`，
  脚本走 `else` 分支打印 "not installed, skipping" 后 `exit 0`；真安装只在
  `ci.yml:719-730` 的 `security-audit`（仅 `schedule` 或非 docs 的 PR/push 到 main|develop 才跑）。
  ⇒ **同一个漏洞，PR 门禁永远绿、定时任务才红**。修法应当是二选一：要么 repo-sanity 也装工具
  （或用预编译二进制缓存），要么让"工具缺失"**失败**而不是跳过。

#### M-2 [P1] `check_baseline_consolidation.py`：**仍未接线**，且它的"吸收"轴永远空转

上一轮记为"空转且未接线"。本轮复核：**能力变了，接线问题没变**。

- 新增了两个真正有用的检查器：`duplicate_indexes()`（`:115`）、`duplicate_foreign_keys()`（`:130`），
  并在 **文件头注释里被 baseline 自身引用**（`00000000_unified_schema_v12.sql:7`、`migrations/INDEXES.md:7`）。
- **它现在能变红**（本轮做了变异自证，这是上一轮没做的）：
  ```bash
  # 在 baseline 的内联 fk_events_room 之后插一个同列 CASCADE FK
  $ python3 scripts/check_baseline_consolidation.py ; echo $?
  ❌ … 存在重复对象 …: - 外键 [events.room_id → rooms]: fk_events_room, fk_events_room_mutant
     同一列上的多个外键会让删除动作重复触发（CASCADE 会压过 NO ACTION），只保留一个。
  1                    # 还原后 EXIT=0
  ```
- **但依旧没有任何 workflow / Makefile / 脚本调用它**：
  `grep -rn check_baseline_consolidation .github Makefile scripts/` → 除自身与 `migrations/*.md` 说明外 **0 命中**。
- 且"增量迁移是否被吸收"这条轴现在**结构上不可验证**了：`migrations/` 下只剩
  `00000000_unified_schema_v12.sql` 一个文件（v11 已从 git 移除），所以输出恒为
  `✅ 已吸收全部 0 个增量迁移的对象`。
  ⇒ **当年造成 §1 那次事故的判据（有序活集模拟）已经失效**，现在的安全网完全依赖
  "同一对象是否出现两次"这类静态检查。**建议：接线 + 在 README 里写明"吸收轴已退役、新基线只能靠实灌验证"**。

#### M-3 [P1] `check_schema_blind_guards.py`：判据**双向都错**，且仍未接线

上一轮的结论（未接线 + warning-only + 真/假阳性并存）**全部复现**，只是行号漂移：

```bash
$ python3 scripts/check_schema_blind_guards.py ; echo $?
Summary: 0 errors, 15 warnings
⚠️  WARNINGS only: Review schema-blind patterns
0
```

- **假阳性（3 处）**：`migrations/00000000_unified_schema_v12.sql:4490 / :4722 / :5015`
  被报为 "missing schema qualification in condition"，但三处实际写的都是
  `table_schema = current_schema()` —— 也就是**已经 schema-aware 的正确写法**。
- **假阴性（1 处）**：baseline 里**唯一**真正硬编码的 `n.nspname = 'public'`（`:4951`，`uq_%` 去重循环）
  **一条都没报**（`grep -c nspname` 在它的输出里 = 0）。
- 本轮新增能力仅是把扫描面扩到了 `.github/workflows/*.yml`（新增 4 条 warning 指向
  `prepare_test_db.sh:62`、`drift-detection.yml:332`、`db-tests-manual.yml:79`、`ci.yml:602`——
  这些**大多是合理的**：CI 断言 public 表数本来就该点名 `public`）。
- ⇒ **仍然不能接线**：既有的 15 条 warning 里混着正确代码，接线后会变成噪音门禁；
  而它该抓的那 1 处又抓不到。**先重写判据（按"是否 `current_schema()` / 是否裸字面量"区分），再谈接线。**

#### M-4 [P1] 本机 `synapse_test.public` 是**旧基线快照**——直接查库会得到错误结论

这是本轮最容易踩的坑，也是"用实库反证代码"这一方法的**反例**：

```bash
$ psql -d synapse_test -tAc "SELECT conname, confdeltype FROM pg_constraint
    WHERE conrelid='public.events'::regclass AND contype='f'"
 fk_events_room    | a     # NO ACTION
 fk_events_room_id | c     # CASCADE   <-- 本轮 baseline 里已经删掉了
```

`fk_events_room_id` 在**当前 baseline 文本里已被删除**（见 §19.2），但本机 `public` 仍然装着它，
因为它是在 `e210c72e`（2026-09-17）之前建的，而 baseline 是**一次性文件**——
`sqlx migrate run` 见到 `schema_migrations` 已有记录就整份跳过，**不会重放**。

同一现象还有：`public` 仍带 5 组真实重复索引（`idx_threepid_session_*` 与 `*_v8` 三组、
`room_aliases`、`room_memberships`、`account_data`、`device_lists_stream`），
而全新实灌的 `audit_v12` 里是 **0**。

⇒ **凡在本机查库取证的结论，必须先确认库比 baseline 新**。
正确姿势：`psql -c "CREATE SCHEMA scratch"` 再用 `SET search_path` 实灌 baseline
（本轮即 `audit_v12`），或跑 `scripts/reset_database_v12.sh` 重建。

#### M-5 [P2] 残留测试 schema 从 `synapse_test` **转移**到了另外两个库

第二轮记 T-7 为"`synapse_test` 残留 25 个 `test_*`"。本轮实测：

| 库 | 残留 `test_*` schema | public BASE TABLE | 说明 |
|---|---|---|---|
| `synapse_test` | **0** ✅ | **230** ✅ | 已彻底清理；public 也不再是 0 表 |
| `synapse_ship_ci` | **231** ❌ | 249 | 新出现的重灾区 |
| `synapse_test_p1p2` | **3** ❌ | 0 | 另一个旁路库 |
| `synapse_bench` | 1（`bench`，设计如此） | 0 | 正常 |

⇒ T-7 **没有被根治，只是换了库**。真正的问题仍是"janitor 只覆盖进程内正常退出"；
新增的 CI seed 步骤用的是 `synapse_ship_ci` 之外的库，所以这些残留不会让 CI 变红。

#### M-6 [P2] `deny.toml` 里 **3 条 advisory 白名单已经失效**（`advisory-not-detected`）

cargo-deny 输出里除 M-1 的 error 外，还有 3 条 `warning[advisory-not-detected]`：

```text
deny.toml:31  "RUSTSEC-2024-0388"    no crate matched advisory criteria
deny.toml:34  "RUSTSEC-2024-0436"    no crate matched advisory criteria
deny.toml:37  "RUSTSEC-2026-0173"    no crate matched advisory criteria
```

三者都已不在依赖图里（对应的 `derivative` / `paste` / `proc-macro-error2` 已消失或升级），
但白名单没跟着删。这与 AGENTS.md 铁律 1「禁止兼容残留」同型：
**白名单只增不减 = 未来同名 advisory 会被静默放行**。修法：删掉这 3 条
（`deny.toml` 与 `.cargo/audit.toml` 两处都要改，且 `deny.toml` 是 advisories 的唯一真相源）。

> 附：`warning[wildcard]` 报了 20 处 `{ path = "…" }`（无 `version`）——那是 workspace 内部 path 依赖，
> 属 `[bans] wildcards = "warn"` 的预期行为，**不是缺陷**，不要"修"。

### 19.2 ✅ 已修复（本轮实测确认，不要再重测）

#### 🔴 §1 整章（schema 吸收缺口）——**全部关闭**

这是本文件里最严重的一章，本轮确认已彻底解决。判定方法不是读文档，而是
**把当前 baseline 实灌进一个全新 schema 再直接查 `pg_catalog`**：

```bash
$ psql -d synapse_test -c "DROP SCHEMA IF EXISTS audit_v12 CASCADE; CREATE SCHEMA audit_v12;"
$ psql -d synapse_test -c "SET search_path TO audit_v12, public;" \
       -f migrations/00000000_unified_schema_v12.sql
$ grep -c ERROR /tmp/v12_apply.log        # → 0
# 结果：230 张 BASE TABLE / 711 索引 / 593 约束 / 2 触发器，零报错
```

| 原条目 | 判定 | 实灌证据 |
|---|---|---|
| **N-2** `events.room_id` 双 FK（CASCADE 存活、NO ACTION 形同虚设） | ✅ 已修复 | `audit_v12.events` 上 `contype='f'` 只有两条：`fk_events_room`(room_id, `confdeltype='a'` NO ACTION) 与 `fk_events_redacted_by`(redacted_by, `'n'` SET NULL)。baseline `:364` 内联声明 + `:5-8` 与 `:4092-4097` 两处注释记录了"删除重复 CASCADE 约束"的理由 |
| **N-3** 5 组不同名同列重复 FK | ✅ 已修复 | `GROUP BY (表,列) HAVING count(*)>1` → **0 行**（修复前 6 组）。baseline 头 `:1-8` 明确列出被删的 5 个 `*_id` 命名 FK |
| **§1.6** baseline 自带重复索引 | ✅ 已修复 | 按 `(表, 列序, 唯一性, 方法, 谓词)` 聚合 → 全新 schema **1 组**，且该组是 `users` 上的**表达式索引**（`idx_users_lower_email` 等 `lower(x)`）被我的 `attnum` 提取聚成一组导致的**假阳性**，人工核对为 3 个不同表达式 → **真实重复 = 0**（修复前本机 public 有 5 组） |
| **§1.4** 8 个完整性约束缺失 | ✅ 全部折入 | `fk_event_edges_prev`、`fk_events_redacted_by`、`fk_backup_keys_room`、`uq_backup_keys_room_session`、`ck_events_depth_nonneg`、`ck_events_not_before_nonneg`、`ck_room_memberships_valid`、`uq_device_keys_user_device_algorithm_keyid` —— **8/8 present**。baseline 里都有真实 DDL（`:5148-5378`），不是只在注释里提及 |
| **§1.5** 10 个热点索引缺失 | ✅ 9/10 折入 | 除 `idx_federation_queue_dest_created` 外 9 个 present（baseline `:5133-5389`）。第 10 个**没有按原名折入**，但 `:3601` 的 `idx_federation_queue_pending ON federation_queue(destination, created_ts) WHERE status='pending'` 覆盖了同一意图（destination 先进先出）——**建议在 baseline 里补一行注释说明这次改名，否则下轮复核会重复报"缺失"** |
| **F-1** burn 批量写 `42P10` | ✅ 未回归 | `audit_v12.burn_after_read_log` 上 `ux_burn_log_user_event UNIQUE (user_id, event_id)` 存在 |
| **F-2** v12/v13 房间 CHECK 拒绝 | ✅ 未回归 | `ck_rooms_room_version_valid` = `CHECK (room_version IS NULL OR room_version ~ '^[0-9]+(\.[0-9]+)*$')` |
| **F-3** audit append-only 触发器 | ✅ 未回归 | `trg_prevent_audit_delete` 挂在 `audit_v12.audit_events` 上 |
| **§0.1 的"23 个对象不在 baseline 中"** | ✅ 已证伪（作为现状） | 有序活集模拟的 23 个对象里，8 个约束 + 9 个索引已折入，其余为改名/等价覆盖。**但注意 M-2：这条判据本身已退役**（没有增量迁移可模拟了） |

> 交叉验证：`docs/audit/DB_REVIEW_2026-09-17.md` §12.6 独立给出的量化（索引 720→**709**、
> 无前导索引的外键列 18→**0**、重复索引组 28→**0**、重复 FK 组 6→**0**）与本轮实灌计数一致。

#### 🔴 N-1 两半——**均已修复**

| 原条目 | 判定 | 证据 |
|---|---|---|
| fmt 棘轮与 `derived_routes.rs` 漂移门禁**互锁** | ✅ **互锁解除** | 生成器 `scripts/contract/gen_derived_routes.py` 现在把输出**过一遍 rustfmt** 再写盘（`:216-245` 的 `_rustfmt()`、`:515-516` 对数据文件同样处理），注释自述"rustfmt（同一 `rustfmt.toml`、同一 edition）makes both gates agree"。实测 `python3 scripts/contract/gen_derived_routes.py --check` → **EXIT=0**（`derived_route_table.inc.rs is up to date`），同时 `bash scripts/contract/check_route_contract.sh` → **EXIT=0**。**两道门禁现在可以同时绿** |
| clippy 在 HEAD 因 `dead_code` 转红 | ✅ **已修复（HEAD 全绿）** | 三个孤儿 manifest 函数（`base_route_manifest` / `assembly_compat_manifest` / `vendor_route_manifest`）已删（全仓 `grep fn base_route_manifest` → 0，仅剩一个测试注释提及）。在 `git archive HEAD` 快照上跑 `SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → **EXIT=0**、`Finished` |
| fmt 债总量 | 🟠 **大幅下降但仍为红** | HEAD 纯净快照 `./scripts/check_fmt_ratchet.sh` → `fmt debt: current=163 baseline=0`（第二轮 **4010**）。分布极集中：`friend_room_service/groups.rs` **75** + `friend_room_service/mod.rs` **68** = 143/163；其余 `rtc/mod.rs` 8、`rtc/call.rs` 6、`migration_consistency_tests.rs` 2、`synapse-services/src/lib.rs` 2、`error_conversion_tests.rs` 2。工作树含 WIP 时为 **478** |

> ⚠️ **工作树 clippy 目前是红的，但那不是 HEAD 的问题**：16 处 `redundant_closure`
> （`map_err(|e| XError::Database(e))` 应为 `map_err(XError::Database)`）+ 1 处 `E0599`，
> 全部落在 B3-5 WIP 改动的 `synapse-services/src/room/{messaging,state}/*.rs`。
> 提交前跑 `cargo clippy --workspace --all-targets --all-features -- -D warnings` 即可看到。

#### 其余本轮确认已修复

| 原条目 | 判定 | 证据 |
|---|---|---|
| **N-6（后半）** 两个危险/散落脚本 | ✅ 已删除 | `optimize-route-manifests.py`（仓库根）、`scripts/replace_manifest_wrappers.py` 均 `No such file`。全仓无引用 |
| **A-1** 路由层穿透分层直连 storage（原 48 文件） | ✅ **0** | `python3 scripts/ci/check_web_layering.py` → `0 offending file(s), allowlist 0`，EXIT=0；`bash scripts/quality/check_route_layering.sh` → `PASS`，EXIT=0。白名单文件 `scripts/ci/route_storage_exceptions.txt` 15 行**全是注释** |
| **A-2** 手抄 route manifest（原 69 个 / 935 `.route(`） | ✅ 结构上收敛（数字 933） | `synapse-web/src` 内唯一 manifest 是 `derived_routes.rs:83 derived_route_manifest`（**派生**）。数据文件 `derived_route_table.inc.rs` = 6,326 行 / 1,148 条 `RouteEntry::new`。`.route(` 仍 933 处——但那是**注册点**，不再是"手抄元数据"，两者语义已不同，**不要再拿 933 当缺陷指标** |
| **A-11** `/r0/push/*` legacy 路由 | ✅ 未回归 | `push_notification.rs:336-339` 全 `/_matrix/client/v3/push/*`、`:342-345` `/admin/v1/push/*`；全仓 `grep '/r0/push'` 在 `*.rs` 中 = 0 |
| **A-12** `src/services` 薄壳 | ✅ 已删（未回归） | `src/services` 不存在；`src/storage/mod.rs` 仍 66 行纯 re-export，**有真实使用者**（`src/bin/schema_health_check.rs:34`、`src/bin/synapse_worker.rs:11`、`benches/…sliding_sync…:47-50`）→ 符合铁律 6，不必改 |
| **T-1** 跑测试后 `public` 被清空为 0 表 | 🟠 **现象消失，机制仍在** | 实测 `synapse_test.public` = **230** 表。但代码路径**没变**：`synapse-test-utils/src/lib.rs:574-575` 仍无条件 `DROP SCHEMA IF EXISTS public CASCADE` + `CREATE SCHEMA IF NOT EXISTS public`，**不回填**（`:551-573` 的守卫只是"拒绝执行"，不是"执行后恢复"）。现在的兜底来自 **CI seed 断言**：`scripts/ci/prepare_test_db.sh:65-66` 在 `public < 200` 或 `模板 < 200` 时报 `::error::Seed incomplete` 并失败。⇒ **非 CI 环境（本地 `cargo nt`）仍会复现 T-1** |
| **T-7** `synapse_test` 残留 `test_*` schema（原 25） | 🟠 **本库归零，问题转移** | `synapse_test` 0 个；但 `synapse_ship_ci` **231** 个、`synapse_test_p1p2` **3** 个（见 M-5） |
| **H-11** mock 漂移（2 处） | 🟠 **部分修复** | `synapse-storage/src/test_mocks/member.rs:348,364` 已加 `limit.clamp(1,1000)`（含回归测试 `:704 paginated_mock_clamps_non_positive_and_huge_limits`）；**但 `:181 get_joined_rooms_page` 与 `:319 get_membership_history` 仍裸 `truncate(limit as usize)`**。`test_mocks/device_list.rs:100-125` 仍是忽略 `from/to/requester_id` 的 stub（有注释自述理由） |
| **§2.4** 供应链门禁在 repo-sanity 恒绿 | ❌ 仍存在（且升级为 M-1） | 见 M-1 |
| **§2.5** 集成测试静默跳过 | 🟠 部分 | 18 处已 fail-closed；残余 `tests/unit/test_schema_housekeeping_tests.rs:42,86` 仍是裸 `return;`（**与第二轮同两处，未修**） |
| **H-12 / H-14 / H-15 / H-16** | ✅ 未回归 | 端口链只剩 `5432/synapse_test`（守卫 `tests/unit/test_db_url_convention_tests.rs`）；`docker/db_migrate.sh:162 host_psql_target_is_implicit_loopback()` 在位（守卫 `migration_consistency_tests.rs:296`）；`Makefile` 只剩 `:90 migrate-status` / `:94 migrate-audit` 只读目标（守卫 `:264`、`:232`） |
| **H-9** | ✅ **本轮修复** | `cargo doc --no-deps` **0 warnings**（上一轮 ~3.5k）；具体修复：`auth_source.rs`/`route_ledger.rs`/`ledger_export.rs`/`admin/policy.rs` 等模块文档中 `/// See [X]` → 描述性文本，bare URLs → `<https://...>` 格式 |
| **H-16** | ✅ **本轮确认已修复** | SDK fork `@langkebo/matrix-js-sdk` 中 `AccountManager.setGuestAccess` 死代码已被移除（`check_sdk_route_coverage.py` 显示仅 SDK-SDK-1 豁免仍存） |

### 19.3 仍然存在（合并清单，按严重度）

**P0**

1. **M-1** `rustls 0.23.43` 命中 `RUSTSEC-2026-0285`（修法 `cargo update -p rustls` 到 `>=0.23.45`）。
   叠加 §2.4：`repo-sanity` 不装 `cargo-deny` → 该漏洞在 PR 门禁里**看不见**，只在 `security-audit` 红。
   > **✅ 本轮已修复**：已 `cargo update -p rustls` 升级到 0.23.45。

**P1**

2. **fmt 棘轮仍红**：HEAD `current=163 baseline=0`（第二轮 4010）。收敛得很集中——
   只要 `cargo fmt --all` 一次即可归零，**但它现在会同时满足派生表漂移门禁**（互锁已解除），
   所以这是一次「低风险、一次性」的收尾，没有理由再挂着。
3. **M-2** `check_baseline_consolidation.py` 未接线（能力已具备、能自证变红）；且"吸收轴"已结构性退役。
   > **✅ 本轮已修复**：已接入 `db-migration-gate.yml` 作为独立 job。
4. **M-3** `check_schema_blind_guards.py` 判据双向错误（3 假阳性 + 1 假阴性）且未接线。
   > **✅ 本轮已修复**：完全重写脚本（修正判据逻辑、添加文件级豁免清单、Rust 源码纳入扫描范围）；
   > 已接入 `db-migration-gate.yml`；当前 0 errors / 8 warnings（合理的人工复核项）。
5. **M-4** 本机 `synapse_test.public` 落后于 baseline → **取证方法陷阱**（不是代码缺陷，但会产生假结论）。
6. **T-2** 模板/迁移错误不传播：`synapse-services/src/database_initializer/mod.rs:531` 在 `error_count>0`
   时仍返回 `Ok`，而 `initialize()` 只在 `Err` 分支清 `is_success`（`:82/:106/:135`）→ 半成品被标记 ready。
   **新增缓解**：运行时初始化现在默认禁用（`:85-92`，需 `RUNTIME_DB_INIT_ENV=true`），
   迁移主链已统一到 `docker/db_migrate.sh`。⇒ 风险面收窄，但 `step_migrations()` 本身仍未 fail-fast。
7. **T-3 / T-8** 测试隔离仍有 **5 个实现文件**：`synapse-common/src/test_isolation.rs`(2446)、
   `synapse-services/src/test_utils.rs`(717)、`synapse-test-utils/src/lib.rs`(1713)、
   `synapse-storage/src/test_isolation.rs`(376)、`synapse-storage/src/test_utils.rs`(210)。
   后三者均声称委派给 `synapse_common::test_isolation`（引用计数 5/8/5），
   但 `synapse-storage/src/test_utils.rs` 对 common 的引用 = **0** 且自带 2 处 `CREATE SCHEMA` → **确属第四份实现**。
8. **T-5** `SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE` 逃生门仍在生产代码里
   （`synapse-test-utils/src/lib.rs:551`）。CI 未开，但代码路径存在。
9. **T-4** `SET search_path TO "<schema>", public` 回退仍遍布
   （`synapse-common/src/test_isolation.rs:614` 及大量测试、`synapse-storage/src/test_utils.rs:158`、
   `synapse-storage/src/test_isolation.rs:118`）。这是为 `pg_trgm` 而设计的**有意行为**，
   风险随 `public` 被正确清空而降低 ⇒ 降级为"设计约束，需注释说明"。
10. **M-5** 残留 schema 迁移到 `synapse_ship_ci`(231) / `synapse_test_p1p2`(3)。
11. **A-3** 双前缀并存且规模上升：`r0=0`，**v1=223 / v3=412**（第二轮 181/337；上升源于
    派生表补齐了 `spaces` 等模块的 v1 孪生条目）。全仓 `/_matrix/client/r0/` 为 0，
    另有 6 条 `/_matrix/media/r0/*`（媒体族，属规范本身）。
    > **本轮已制定退役策略**：详见 `docs/audit/A3_v1_deprecation_strategy.md`。
    > **本轮分类**：标准 Matrix v1 = 114 条，Tjg 专有 v1 = 60 条（friends/voice/burn/spaces/threads/widgets）。
12. **A-10** 推送假投递未动：`synapse-services/src/push/service.rs:30 push_gateway` 字段（`:103` 置 `None`、
    `:138` 由 `with_push_gateway` 赋值，而该 setter **全仓 0 调用方**）→ 字段只写不读；
    `:486 send_upstream` 在 `:495` 仍 `return Ok(PushResult::success_with_response("Upstream accepted"))`
    —— **对 `"upstream"` 假装成功却不发送**。
13. **§2.7** 迁移守卫仍无候选：`migrations/` 只剩一个 baseline 文件，`check_migration_consistency.py`
    报 `{"status":"ok","model":"single-source","issues":0,"primary_forward_files":1}`。
    `migration_replayability_guard_tests.rs` 是**静态**守卫，只对未来新增迁移生效 ⇒ **降级为"设计使然"**，不是缺陷。
14. **§2.8** `docs/audit/` 下 4 个 baseline 日志仍**全部缺失**
    （`00_test_baseline.log` / `00_clippy_baseline.log` / `05_performance_baseline.log` / `11_performance_after.log`）；
    `scripts/.missing-docs-baseline` 内容仍为 `6`。
15. ~~**H-1**~~ `pool_size` 废弃字段仍在发布配置。
    > **✅ 本轮已修复**：已从 `homeserver.yaml` 移除（DB max_size=50 保持不变）；新增 serde 反序列化测试。
16. ~~**H-6**~~ `Cargo.toml:32` 声明 `server = ["dep:axum", "dep:tower-http", "synapse-web/server"]`，
    全仓 `#[cfg(feature = "server")]` 出现 **0** 次。
    > **✅ 本轮已修复**：已从所有 crate 移除 `server` feature 声明（axum/tower-http 改为必选依赖）；
    > CI 工作流同步清理。
17. ~~**H-8**~~ 仍**没有**任何测试把 `docker/config/homeserver.yaml` 反序列化进 Rust `Config`。
    > **✅ 本轮已修复**：新增 `homeserver_yaml_deserializes_into_config` 测试（`deny_unknown_fields` 门禁）
    > + `homeserver_yaml_has_no_deprecated_pool_size` 测试。
18. ~~**H-9**~~ `ci.yml:337-338` 自认 "~3.5k unresolved-intra-doc-link warnings"，
    ~~但仍无棘轮/baseline~~（`scripts/` 下只有 `.fmt-baseline` 与 `.missing-docs-baseline`）。
    > **✅ 前一轮已修复**：`cargo doc --no-deps` 0 warnings。
19. ~~**H-10**~~ god-file 未拆且**变大**。
    > **🟢 前一轮已修复**：B6-4 神文件拆分已在 2026-09-15 完成（见 OPTIMIZATION_EXECUTION_PLAN H-10）。
    > 当前 `friend_room_service/mod.rs` 为装配器，实际逻辑已拆至子模块。
20. **H-13** 允许列表计数仍与文档不符。本轮实测（`git grep`，排除 `target/` 等）：
    `#[allow(dead_code)]` = **29**、`allow(clippy::` = **265**、`#[ignore]` = **27**（含 `#[ignore = "…"]`；
    纯 `#[ignore]` 为 13）、TODO/FIXME/XXX/HACK = **12**、`#[deprecated]` = **0**。
21. **M-6** `deny.toml:31,34,37` 三条已失效的 advisory 白名单（`advisory-not-detected`）。
22. **N-7** 文档/守卫漂移（**本轮全部复核，均未修**）：
    - **ledger 计数**：文档/脚本仍写 `default=1320 / SDK=1407`（`scripts/generate_sdk_ledger_fixtures.sh:15`、
      `docs/synapse-rust/LEDGER_EXPORT_SCHEMA.md:46,91,92`）；**实测** `ledger_export/all.json entry_count = 1065`、
      `default.json = 1047`、SDK 车道 `all.json = 1146`、`default.json = 1127`。
    - **`executed_at` 默认值三者不一致**：baseline `:2966` =
      `BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)`（**先取整再乘** → 截断到秒），
      而 `docker/db_migrate.sh:329,338` 与 `synapse-services/src/database_initializer/mod.rs:298,340`
      写的是 `(EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT`（先乘再取整）。同一秒内多次迁移仍会撞值，
      而守卫 `migration_consistency_tests.rs` 只断言类型/不含 `TIMESTAMPTZ`，**两种写法都能过**。
    - **过时 runbook**：仍在调已删的 `scripts/check_config_consistency.py`
      （`P4_ci_gate_integrity_2026-09-11.md:188`、`P4_perf_gate_honesty_2026-09-11.md:205`、
      `P5_migration_search_path_shadowing_2026-09-12.md:232`、`P5_config_consistency_gate_2026-09-11.md`、
      `S_series_verification_2026-09-11.md:287`），以及
      `P4_performance_baseline_2026-09-11.md:222,223,226,243,244,625,627` 对已删
      `docker/deploy/config/rate_limit.yaml` 的 `cp`/`sed -i` 操作。

**长期债（A 系列，本轮实测值，全部按"是否仍是缺陷"重新定性）**

23. **A-4** storage `pub trait` **37** 个；trait 棘轮 `scripts/ci/check_trait_ratchet.py` →
    `TOTAL=65 (baseline 65) STORE_API=33`，EXIT=0。（第二轮 70 → 现在 65，**已有棘轮守住不增**）
24. **A-5** `pub use …::*` **149** 处（第二轮 176）。分布：storage 48 / 根 `src` 44 / e2ee 24 /
    **services 18** / web 11 / federation 4。B4-5a 标题称"清零"但 body 自述 "30 → 18" ⇒ **标题与事实不符**，
    残留见 `synapse-services/src/infra/mod.rs:18-26`、`room/mod.rs:67-69`、`push/mod.rs:12`、`account/mod.rs:18-24`。
25. **A-6** 名字含 `Error` 的 `pub enum`：HEAD **26**（工作树 29，B3-5 新增 3）。
    **注意口径**：B3-5 正在**主动增加**域错误枚举（`FriendRoomError` 等），
    所以"减少错误枚举数量"这个目标本身已被进程**否决**（转向"每域一个错误类型 + 统一转换"）。
    ⇒ A-6 应重新定义为"是否存在**多份**同义错误类型"，不再是"数量要少"。
26. **A-7 / A-8**（口径不稳定，仅给可复现近似值）：8 个 src crate 内 `^///` 行 = **25,339**，
    其中 `/// The \`x\` field.` 模板行 = **7,890**；上下文对象 pub 字段 = **249**
    （`synapse-web/src/routes/context.rs` 的 11 个 `*Context`：Admin 55 / Room 36 / Federation 36 /
    Device 24 / Auth 22 / Sso 20 / Media 14 / Sync 13 / Friend 13 / Core 9 / E2eeRoom 7）。第二轮 287。
27. **A-9** 根 crate **已缩到 4,326 行**（37 个 `.rs`），`src/web` 与 `src/services` 均已不存在；
    `synapse-web/src` **60,368 行**，其中 `**/mod.rs` **3,129** 行（17 个文件）。
    ⇒ "单 crate 模块装配巨大"已转化为"独立 crate 体积大"，**是结构改善而非新缺陷**。

### 19.4 证伪 / 降级 / 口径修正

- **§1.5 `idx_federation_queue_dest_created`**：不是"缺失"，而是**被等价索引替换**（见 §19.2）。
  下轮复核请勿再用原名 grep 判定缺失。
- **`migrations/` 只剩 1 个 `.sql`**：v11 baseline 已在 `bddd6109` 移出 git，
  `00000001_extensions_v10.sql` 也已内联进 v12。§2.7 的"0 候选"、§0.1 的"吸收 0 个"都是这一点的**推论**，
  不是两个独立缺陷——**合并为 M-2 一条**。
- **索引计数口径**：`§19.2` 的实灌计数（711 索引 / 593 约束）与 DB_REVIEW §12.6 的
  "全新库 709 索引"相差 2，原因是我的 scratch schema 里 `CREATE INDEX CONCURRENTLY`
  与迁移内 `DO $$` 块的执行次数差异 + 我的查询含 `pg_indexes` 全量（含 `pg_` 系统视图过滤差异）。
  **两者都指向"重复组 = 0"这一结论，故不影响判定**；需要精确数字时以 DB_REVIEW 的脚本为准。
- **A-2 的 `935 → 933`**：**不要再当作"手抄 manifest 仍未清"的证据**。manifests 已从 69 个降到 1 个派生表，
  `933` 现在是 `.route(` **调用点**的数量（每个路由注册一行），与"手抄元数据"是两件事。
- **H-4 / H-5 / H-8（结构性部分）**：沿用第二轮结论，本轮未发现回归
  （`docker/deploy` 仍 ~2 MB、`git worktree list` 仅 main、`docker/deploy/config/` 不存在）。
- **工作树的 clippy 红**：**不是 HEAD 的缺陷**，是并发会话 B3-5 WIP 的中间态。
  本文件所有"HEAD 全绿"的结论都在 `git archive HEAD` 快照上复跑，请勿用工作树结果推翻。
- **`warning[wildcard]` 20 处**：workspace 内部 path 依赖，`[bans] wildcards = "warn"` 的预期输出，**非缺陷**。

### 19.5 建议的执行顺序（按收益/风险）

| 序 | 动作 | 理由 | 风险 |
|---|---|---|---|
| 1 | `cargo update -p rustls` 提到 `>=0.23.45` + 提交 `Cargo.lock` | M-1 是唯一的真实安全漏洞，且已被门禁拦下（只是拦在错误的地方） | 极低；改 lockfile |
| 2 | `cargo fmt --all` + 提交（含 `friend_room_service` 的 143 处） | fmt 债已从 4010 降到 163，互锁已解除，一次收敛到 0 | 极低；纯格式，`check_fmt_ratchet` 会自证 |
| 3 | 让 `repo-sanity` 的供应链步骤**在工具缺失时失败**（或预置二进制） | 否则第 1 条的同类问题下次仍会被 PR 门禁放行 | 低；需确认 runner 上装工具耗时 |
| 4 | 删 `deny.toml` / `.cargo/audit.toml` 里 3 条失效白名单（M-6） | 铁律 1：白名单只增不减 = 未来同名 advisory 静默放行 | 极低 |
| 5 | 重写 `check_schema_blind_guards.py` 判据后接线（M-3） | 现在接线只会产生噪音门禁；基线里那 1 处真硬编码仍无守卫 | 中；需先定义正确判据 |
| 6 | 接线 `check_baseline_consolidation.py`（M-2），并在 README 写明"吸收轴已退役" | 它是 baseline 唯一的重复对象守卫，且已能自证变红 | 低 |
| 7 | 给 `Config` 加一个"`homeserver.yaml` 反序列化"测试（H-8） | 一次同时覆盖 H-1（`pool_size` 假旋钮）与"发布配置含已删字段" | 低；`deny_unknown_fields` 已就位 |
| 8 | 修 T-2（`step_migrations` 在 `error_count>0` 时返回 `Err`） | 半成品模板被标记 ready 是"假绿"根源之一 | 中；需确认调用方对 `Err` 的处理 |
| 9 | 统一 `executed_at` 默认值写法（三处 → 一处） | 同一秒内迁移撞值；当前守卫两种写法都放行 | 低；仅改 SQL 文本 + 加守卫 |
| 10 | 清 `synapse_ship_ci`(231) / `synapse_test_p1p2`(3) 残留 schema（M-5） | 顺带确认 janitor 对非 CI 库的覆盖 | 低；纯清理 |
| 11 | 其余（M-4 文档提醒、A-3/A-10/H-13 长债） | 按迭代节奏处理 | — |

> **本轮完成（H-9/H-16/H-17）**：`cargo doc` 0 warnings（上一轮 ~3.5k）；SDK fork 死代码已清理；
> `GET /_matrix/client/v3/auth/{loginType}/fallback/web` 端点已实现，ledger 已同步。
> **H-18 判定为设计合理**：`saml_enabled` 用于离线 profile 派生，不应删除。
> **H-10 已由前一轮 B6-4 拆分完成**，当前 `mod.rs` 仅为装配器。

> **一句话总结本轮**：**schema 章（§1，本文档最严重的一章）已全部关闭并有实灌证据**；
> 门禁诚信从"6 项修复 / 5 项仍存"收敛到**只剩供应链门禁布局这 1 项**（且它以 M-1 的形式变红）；
> 剩下的东西里，**只有 M-1 是安全漏洞**，其余是"已具备能力但没接线"（M-2/M-3）、
> "取证方法陷阱"（M-4）与长期架构债。

---

## 附录 B：H-12 复现（2026-09-15）

```bash
# B.1 证明 15432 是死的、应用库不存在
nc -z localhost 15432 && echo OPEN || echo "CLOSED（无监听）"
grep -n 'ports:' -A 1 docker/docker-compose.dev-host-access.yml   # ${DB_EXPOSE_PORT:-5432}:5432
PGPASSWORD=synapse psql -h localhost -p 5432 -U synapse -d synapse -tAc "select 1"
#   → FATAL: database "synapse" does not exist

# B.2 修改前后：五份链与脚本默认值
grep -rn 'localhost:15432' --include='*.rs' src/ synapse-*/src/ tests/     # 期望：无
grep -rn 'localhost:5432/synapse"' --include='*.rs' src/ synapse-*/src/ tests/   # 期望：无
for f in scripts/init_test_public_schema.sh scripts/cleanup_test_schemas.sh \
         scripts/tune_test_db.sh scripts/seed_test_db.sh \
         scripts/run_bench_server.sh scripts/run_local_coverage.sh; do
  bash -n "$f" && echo "OK $f"
done

# B.3 守卫（6 项）与变异自证
cargo test --test unit --features test-utils test_db_url_convention        # 6/6 绿
# 变异：把 synapse-storage/src/test_isolation.rs 的链改回
#   ["…:15432/synapse_test", "…:5432/synapse"]
# → rust_resolvers_share_one_fallback_chain 与
#   no_target_uses_the_dead_port_or_the_application_database 转红，并报出 file:line
```
