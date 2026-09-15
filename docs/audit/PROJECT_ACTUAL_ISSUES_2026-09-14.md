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

### 0.2 严重度分布

| 级别 | 数量 | 代表性条目 |
|---|---|---|
| 🔴 P0 功能/数据正确性 | **3（已修）** | burn 批量写入 `42P10`、v12/v13 联邦加入被 CHECK 拒、audit append-only 失效 |
| 🔴 P0 门禁诚信 | **8** | 覆盖率基线无法提交、perf 门禁纯 echo、CI 集成测试指向应用库、18 处测试静默跳过、sqlx 棘轮 FAIL |
| 🟠 P1 架构冗余/过度开发 | **12** | 48 个路由文件穿透分层、70 storage trait 中 68 个单实现、近 5k 处样板注释 |
| 🟠 P1 测试隔离/模板构建 | **8** | 根模板构建无条件清空 `public`、模板构建吞错、测试隔离仍多头 |
| 🟡 P2 安全/协议残留 | **12（6 已修 / 6 未决）** | 已修：S-1、S-2（B5-1 自签名 + `server_name` 校验）、S-4（B5-2 隔离变更流保留期清理）、S-12、S-15（B5-3 MSC4108 DELETE 补头 + 去自证）、S-13（B5-5 契约提取器）。未决：threepid 孤儿路由、速率限制碎片化、X-Matrix 头朴素解析、cache 读写不对称、无"真实 router == ledger"测试 |
| 🟡 P2 配置/仓库/文档卫生 | **11** | `.scratch` 97 文件入库、3 个 worktree、`cargo doc` ~3.5k 警告、god-file 1833 行 |

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

`idx_device_signatures_user_device`、`idx_device_signatures_target`、`idx_event_edges_prev_room`、
`idx_e2ee_audit_log_device`、`idx_e2ee_audit_log_room_event`、`idx_push_queue_user_pending`、
`idx_push_queue_retry`、`idx_federation_queue_dest_created`、`idx_federation_queue_retry`、`idx_federated`。

部分由等价索引覆盖（`idx_device_signatures_unique`、`idx_federation_queue_dest_status`、
`idx_event_edges_prev`），但目标查找/重试扫描/部分索引（`WHERE is_processed=false`）无等价物。
`migrations/INDEXES.md:67,123,124` 仍在描述其中两个索引 → 文档与 baseline 不一致（已知待办）。

### 1.6 🟡 附带发现：baseline 自身违反"单一真相源"

- **5 对完全重复的索引定义**（`IF NOT EXISTS` 使运行期无害）：
  `idx_rooms_name_trgm`(3294/3772)、`idx_rooms_canonical_alias_trgm`(3295/3773)、
  `idx_users_email_trgm`(3265/3770)、`idx_users_user_id_trgm`(3264/3771)、
  `idx_users_lower_email`(3261/3785)。
- **3 处 schema-blind 硬编码 `'public'` 仍残留**（上一轮修了 5 处，漏了 3 处）：
  `v11:4941`、`:5005`、`:5056`（后两处是 `message_log` 守卫；行号随本批插入而下移）。
  测试隔离按 `search_path = <test_schema>, public` 应用 baseline，故这三处只清 `public`、不清目标 schema。
- 其他硬编码 `public`：`docker/db_migrate.sh:192,203,501,507`；
  `scripts/generate_logical_checksum_report.py:129,143`（无 env 覆盖）。

---

## 2. 🔴 P0：门禁诚信问题

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
| S-5 | federation knock **丢弃 `via`**（已知缺口，非隐藏 bug） | `src/web/routes/handlers/room/members.rs:229-236` 明确注释：knock 目前 local-only，`via` 被**接受并记录日志**而非静默丢弃 → 降级为"已文档化的功能缺口" |
| S-6 | X-Matrix 头解析用朴素 `split(',')` | `src/web/middleware/federation_auth.rs:299`。**理论问题**：所有字段值都不会含逗号，且解析结果参与签名校验，误解析 fail-closed |
| S-7 | 速率限制碎片化 | 已核实 `src/web/routes/friend_room.rs:631-632` 自建 key 并直接 `ctx.cache.rate_limit_token_bucket_take(...)`；同类模式另见 `handlers/search/search.rs`、`auth_compat.rs`（子代理复核） |
| S-8 | cache 读写不对称（结构性陷阱，代码内已标注） | `synapse-cache/src/manager.rs:398` `set_raw` 写 L1+L2（异步）；`:408` 同步 `get_raw` **只读 L1**；L2 回退需显式 `:421 get_raw_shared().await`。当前无生产误用 |
| S-9 | threepid 路由**孤儿**（且已进契约文档/未进 ledger） | `src/web/routes/threepid.rs:19` 定义 `create_threepid_router`，`src/web/routes/mod.rs:255` 仅 re-export，**无装配点**；`ROUTE_CONTRACT.md:20` 自认其"孤儿/死代码"，但 `:29-30` 仍列出 `/requestToken`、`/submitToken`；这两条在 ledger fixture 中不存在 |
| S-12 | MSC4108 `DELETE` 204 响应缺 3 个 required 头 | `src/web/routes/msc4108_rendezvous.rs:275` 返回 `(StatusCode::NO_CONTENT, Body::empty())`，**无 header tuple**、router 无补头 layer；`Last-Modified`/`Cache-Control: no-store`/`Pragma: no-cache` 在 POST/GET/PUT 都有（`:103-105`、`:148-150`、`:220-222`），DELETE 缺失。文档 `§16` 却记 10/10 已补齐 |
| S-13 | 契约提取器结构性缺陷 | ✅ **已修复（B5-5）**。原缺陷三项全部复现并消除：①链式方法只记录第一个 method（msc4108 `get().put().delete()` 只出 `GET`）；②`nest_map` 收集后从未使用；③`ROUTE_CONTRACT.md` 有 **15** 条相对 `/spaces/...`、**0** 条带前缀。修复后实测：MSC4108 出全 4 条（POST/GET/PUT/DELETE）；spaces 相对路径 **15→0**、带前缀 **0→48**（24 路由 × v1/v3 两前缀，B1 已删 r0 故为 2 而非 §17 预估的 3×15=45）。详见 §9 |
| S-14 | 无任何测试校验"真实 router == ledger" | `grep -rn '\.routes()\|into_make_service' src/ tests/` 只有 3 处命中，全在生产 `src/server/mod.rs`；`assembly_route_tests.rs` 的 27 个测试只读 manifest |
| S-15 | MSC4108 响应头测试**自证** | `tests/unit/msc4108_rendezvous_route_tests.rs:222-338` 构造**本地 header 数组**再对其断言，从不调用 handler → DELETE 缺头也会通过 |

> **降权/未复核**：`S-3`（文档 `CFG-5/CFG-6` 称 51/27 处 legacy hash 引用，按所给符号名只能命中 9/1 → 符号名不确定，
> 不作为结论）；`S-10`（MSC4108 双路径族）、`S-11`（SDK 93 条中 64 条死条目）为子代理复核，未由主复核独立确认。

---

## 6. 🟡 P2：配置 / 仓库 / 文档卫生

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

## 附录 A：复现命令

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
