# 数据库迁移说明

> 最后更新: 2026-09-18

## 唯一真相源（single source of truth）

`migrations/` 是**唯一**的迁移目录。`docker/deploy/` 的 migrator 直接绑定挂载本目录：

```yaml
# docker/deploy/docker-compose.yml
- ../../migrations:/migrations:ro
```

历史上 `docker/deploy/migrations/` 曾是一份**手工同步的副本**，并因此静默漂移。
**漂移数字的唯一真相源就是本节**（`AGENTS.md` / `CLAUDE.md` / `docker/deploy/README.md` /
`docs/audit/DB_REVIEW_2026-09-17.md` §15.4 M10 只做简短引用，不重述口径）：

| 口径 | 数量 |
|---|---|
| 副本独有文件总数（相对权威 `migrations/`） | **131** |
| 其中 `archive/` 子目录（权威目录没有 `archive/`） | **49** |
| 其中非 `archive/` 的副本独有文件 | **82** |
| 其中 v7 血统**正向**迁移（再去掉 `.undo.sql` 回滚文件） | **42** |

同时该副本**缺失 13 个新迁移**（`schema_p1_federation_and_integrity`、
`schema_p2_data_integrity`、`schema_p3_perf`、`schema_cleanup_dedup_and_dead_code`、
`extend_room_version_check`、`event_relations_pagination_index` 等），
导致**全新部署建出的 schema 缺少这些修复**。该副本已删除（2026-09-11）。

复算命令（在死副本尚存的最后一个提交 `2b16dc3c^` 上；实测 `p=db386a51`）：

```console
$ p=$(git rev-parse 2b16dc3c^)
$ comm -23 <(git ls-tree -r --name-only $p -- docker/deploy/migrations | sed 's|docker/deploy/migrations/||' | sort) \
           <(git ls-tree -r --name-only $p -- migrations | sed 's|migrations/||' | sort) > /tmp/copy_only.txt
$ grep -c . /tmp/copy_only.txt                                    # 131  副本独有总数
$ grep -c '^archive/' /tmp/copy_only.txt                          #  49  其中 archive/ 子目录
$ grep -vc '^archive/' /tmp/copy_only.txt                         #  82  非 archive 的副本独有文件
$ grep -v '^archive/' /tmp/copy_only.txt | grep -vc '\.undo\.sql$' #  42  再去掉回滚文件的正向迁移
```

> ⚠️ **不要再创建 `docker/deploy/migrations/` 副本。**
> `scripts/check_migration_consistency.py` 会在检测到陈旧副本或 compose 未挂载权威目录时失败，
> 该检查是 CI 的阻塞步骤（`.github/workflows/ci.yml` 的 `Migration consistency`）。

> ℹ️ 曾尝试用符号链接替代副本，**不可行**：BSD/macOS `find` 不跟随作为搜索根的符号链接，
> `find "$MIGRATIONS_DIR" -maxdepth 1 -type f -name '00000000_unified_schema_v*.sql'`
> 会匹配为空，migrator 直接报 "找不到统一基线脚本"。

## 目录结构

```
migrations/
├── 00000000_unified_schema_v12.sql           # v12 统一基线（当前活跃，新环境唯一建库入口）
├── INDEXES.md                                # 索引治理文档（部分索引/复合索引/设计原则）
└── README.md                                 # 本文件
```

**当前活跃链路**: **只有 1 个 forward SQL 文件**（`00000000_unified_schema_v12.sql`，已含全部扩展表）。历史时间戳迁移与扩展文件均已折入/删除，目录下不存在时间戳增量文件、扩展文件、`.undo.sql` 文件，也不存在 `archive/` 子目录。（`scripts/build_sqlx_migration_source.py` 仍保留 `00000001_extensions*` 的识别分支：找不到就是空操作，但文中不再声称存在该文件。）

> **目录下必须只有一个基线文件。** 历史基线（v8/v10/v11）一旦与最新基线并存，
> 迁移器会把它们当"增量迁移"再执行一遍并写进 `schema_migrations`，导致本地机器
> （磁盘可能有残留）与 CI 全新检出的 schema 分叉。两个入口都会按
> `00000000_unified_schema_v*.sql` 模式跳过所有历史基线，`tests/unit/migration_consistency_tests.rs`
> 也守着"只能有一个基线"这条不变式。v11 已随 v12 迭代从磁盘移除，
> 需要查阅时用 `git show bddd6109^:migrations/00000000_unified_schema_v11.sql` 取回；
> 下文凡引用 "v11 第 N 行" 的审计记录均为历史快照。

> 校验脚本位于 **`scripts/`**（本目录下没有）：
> - `scripts/check_migration_consistency.py` — 检查单一真相源、compose 挂载、undo 配对与命名一致性
> - `scripts/check_baseline_consolidation.py` — 检查 v* baseline 是否吸收所有增量迁移
> - `scripts/build_sqlx_migration_source.py` — 生成 forward-only migration source

> 目录下不存在 `archive/` 子目录：v8 及更早基线已随 v11/v12 迭代从仓库移除。
> schema 健康回归统一走当前基线 —— `scripts/ci_schema_health_check.sh` 通过
> `docker/db_migrate.sh migrate` 建库，不再引用任何旧基线文件。新环境使用 v12 基线建库。

### 为什么只有一个 baseline：`ENABLED_EXTENSIONS` 不能裁剪表结构

`00000000_unified_schema_v12.sql` 是**全特性基线**，已包含全部扩展表（cas / saml /
friends）。历史上还有一个 `00000001_extensions_v10.sql`，它定义的 14 张表 + 1 个索引
**逐对象都已在 v12 中**、且同样使用 `IF NOT EXISTS` —— 也就是说应用它是个**空操作**，
它只是同一份 DDL 的第二份副本（违反"同一职责只允许一份实现"）。该文件及其过滤映射
`extension_map.conf` 已**删除**：

- `docker/db_migrate.sh`、`docker/deploy/scripts/container-migrate.sh`、
  `scripts/reset_database_v12.sh` 现在都只按文件名顺序应用 `migrations/*.sql`，
  没有任何扩展过滤分支；
- 测试模板（`synapse_common::test_isolation`）的基线字符串现在就是 v12 本身
  （内容指纹随之变化，会铸造一次新模板，属预期行为）。

`ENABLED_EXTENSIONS` 仍然存在，但它**只**决定 `deploy.sh` 编译哪些 cargo feature
（见 `--features` 选择），**不参与**建表。想要按特性裁剪表结构，唯一正确的做法是把
扩展表从 baseline 中拆出去（当前未做，也不建议在没有明确需求时做）。

## 新增迁移流程（务必同步折入 baseline）

`build_sqlx_migration_source.py` 生成的 forward-only source 只含 baseline +
extension（CI 用它建库），因此**每次新增时间戳迁移后，必须把幂等的增量变更
（新表/新列/新索引，须用 `IF NOT EXISTS` / `ADD COLUMN IF NOT EXISTS`）同步
折入 `00000000_unified_schema_v12.sql` 尾部**，否则 CI 的 DB 会缺表/列/索引。

提交前请运行一致性检查：

```bash
python3 scripts/check_baseline_consolidation.py
python3 scripts/check_migration_consistency.py
```

历史上曾漏吸收 10 个迁移（federation_dead_letter_queue、login_tokens、
saml_pending_requests、room_event_txn_dedup 等，commit 2d453089/bf85f23f 已折入），
此检查脚本即为防止复发而设。

### 改 baseline 后必须让测试模板重新铸造（2026-09-18 修复）

集成测试的 schema 来自 `synapse-test-utils` 铸造的**共享模板**
（`test_template_v<rev>_<fingerprint>`），模板名由 `template_schema_fingerprint()`
计算，其输入**必须**包含 `migrations/` 下每个 `.sql` 的**内容**：

- 该函数原先写作 `PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations")`，
  即 `synapse-test-utils/migrations` —— 该目录不存在，`read_dir` 失败后指纹退化成
  常量 `migrations-dir-missing`。后果是**改 baseline 不会重建模板**：本次把
  `e2ee_audit_log.device_id` 改成可空后，测试仍跑在旧 schema 上，必须手动
  `DROP SCHEMA test_template_v2_*` 才生效。
- 现已改为从 `CARGO_MANIFEST_DIR` 向上查找含 `.sql` 的 `migrations/`（找不到即
  fail loudly，而不是静默沿用陈旧模板），并以**文件内容哈希**而非 `(长度, mtime)`
  参与指纹 —— `git checkout` 会改 mtime 而不改内容（多余重建），等长编辑则可能
  两者都不变（陈旧模板继续被使用）。
- 推论：**baseline 的任何改动都会让下一次集成测试重新铸造模板**（首次约 35–60s），
  这是预期行为，不要"优化"掉；反之，若改 baseline 后测试行为毫无变化，
  先怀疑模板指纹没有真的改变。

### 迁移器校验和是**内容** md5，内容漂移时容错重放（2026-09-18 修复）

`schema_migrations.checksum` 曾写入 `md5(filename)` —— 每个文件一个常量，因此
**看不出 baseline 被编辑过**。本仓库的约定是把 schema 变更直接折入
`00000000_unified_schema_v12.sql`，于是迁移器看到"版本相同、已应用"就静默跳过，
已部署的库根本无法通过 `migrate` 升级（commit `43c29105` 修复）。

现状（`docker/db_migrate.sh`）：

- `file_content_checksum()` 对文件**内容**取 md5（`md5sum` / BSD `md5` / `cksum` 兜底）；
- `init_database()` 比对 `schema_migrations` 记录值与当前内容哈希：不相同即打印
  `基线内容已变化，重放基线以应用变更`，并**以容错模式重放**基线（基线幂等：
  只有 `IF NOT EXISTS` / `DROP IF EXISTS` 与一次幂等的去重 `DELETE`）；
- 记录值缺失（列或行为空）同样走重放路径，不会静默放过。

判据：`grep -n 'file_content_checksum\|基线内容已变化，重放' docker/db_migrate.sh`。

## 死表清理（已于 2026-09-14 完成，原计划留待 v12）

v11 baseline 曾包含 `openclaw_connections` / `ai_conversations` / `ai_connections`
三张表及其触发器（openclaw 源码已于 commit 67e66bf4 彻底删除）。

**已于 2026-09-14 直接在本 baseline 中删除，未等到 v12**，理由：

1. 项目**未发布、无外部用户、无生产数据**（`AGENTS.md` 铁律 1），不存在"已应用过
   的迁移不可修改"的兼容义务 —— 该义务的前提是有存量部署需要保护。
2. 即使保留独立的 DROP 迁移也无效：`build_sqlx_migration_source.py` 只选
   baseline + extension + `V*` 迁移，时间戳命名的 DROP 迁移**根本不会被执行**。
   因此"留待 v12"在实践中等于永久不清。

本次共删除 **23 张零引用表**（Rust 全仓 `\b<表名>\b` 引用计数为 0）：
`user_account_data`、`voice_messages`、`user_reputations`、`typing_stream`、
`security_events`、`room_stats_current`、`room_parents`、`receipts_linearized`、
`reaction_aggregations`、`presence_stream`、`password_history`、
`openclaw_connections`、`migration_audit`、`ip_blocks`、`federation_inbound_events`、
`federation_blacklist_config`、`event_forward_extremities`、`destination_retry_timings`、
`ai_messages`、`ai_generations`、`ai_conversations`、`ai_connections`、`ai_chat_roles`。

同时删除 25 条显式 `CREATE INDEX`、54 条 `COMMENT ON COLUMN`、4 处
`pg_constraint` FK 补丁、3 个 AI 触发器 DO 块。

> **验证方式**（这是迁移改动唯一可靠的证明）：把改动前/后的 baseline 分别应用到
> 两个**全新数据库**的 `public` schema，比对结果：
> 两侧均 0 error；表数 255 → 232（差恰好 23）；`EXCEPT` 双向差集为 23 / 0 ——
> 即除了这 23 张表，schema 完全一致。另验证 baseline + extensions 组合应用
> 亦为 0 error，且 `voice_messages` 不再被扩展文件重建。

> ⚠️ **顺带发现两个既有缺陷（与本次删除无关，尚未修复）**：
> 1. `-- typing composite PK` 的守卫写作
>    `WHERE table_schema = 'public' AND ...`，**硬编码 public**。若把 baseline 应用到
>    非 `public` schema（测试隔离正是如此），守卫会误判为"不存在"而重复添加主键，
>    报 `multiple primary keys for table "typing"`。
> 2. `-- user 表其他 user_id 字段` 的 DO 循环同样硬编码 `table_schema = 'public'`，
>    且是**全文件唯一没有 `IF NOT EXISTS` 守卫**的约束块 —— 重复执行必然报
>    `constraint ... already exists`。二者都应在 v12 重构时改为按实际 schema 判定。

> 审计补充（2026-09-04）：v11 baseline 同样存在 `events.reference_image` 字段
> （v11 第 343 行），仅在 `test_mocks/event.rs` 中作为 fixture 写入，业务代码无任何
> 读/写访问，属于死字段。
>
> `idx_rooms_name_trgm` / `idx_rooms_canonical_alias_trgm` 等 5 条索引在 v11 中
> 各重复定义两次（后者由 append-only 生成策略导致），**已于 2026-09-16 去重时清除**，
> 见下文"baseline 去重"。

## baseline 去重（2026-09-16，6771 → 5426 行）

`00000000_unified_schema_v12.sql` 曾把**同一段内容重复 3 遍**：
3 个生成器 header、3 份 extensions 块（内联的 14 张扩展表）、3 份 p0 折入块。
加上主体内 5 条重复索引语句，全文有 1353 行纯冗余。

**根因**：`scripts/generate_next_baseline.py` 把 `current_path` 与输出路径都指向
`00000000_unified_schema_v12.sql`，执行 `body = header + current + ext_body + p0_sql`
后 `write_text(body)` —— **每跑一次就在自己的输出上再追加一段**，天然不幂等。
跑三次的净效果就是 3 份副本。该脚本与其输入片段 `scripts/p0_constraints_indexes.sql`
（已逐字节内联进 baseline）均已删除。

**危害不只是行数**：重复副本全是 `IF NOT EXISTS`，在"首次生效者决定 schema"的语义下
整体空转，于是"文件写了多少"与"库里实际有什么"脱钩。去重时按"这段看着像复制体"
直接删就会出事 —— 尾部那 3 份副本里**藏了主体没有的 10 个索引**
（`idx_device_signatures_user_device`、`idx_event_edges_prev_room`、
`idx_e2ee_audit_log_device`、`idx_push_queue_user_pending`、
`idx_federation_queue_pending`、`idx_rooms_federated` 等）与 9 个约束 DO 块。

**去重后的结构**：1 个 header + 主体（含 14 张扩展表）= 230 张表 / 369 个索引，
尾部保留唯一一份"完整性约束与性能索引折入块"。实测 230 表名、369 索引名**均无重复**。

> **验证方式**（与死表清理同一判据）：改动前（HEAD 版）与改动后的 baseline 各自
> 应用到**全新数据库**，`pg_dump --schema-only` 归一化后**逐行完全一致**
> （15643 行，`SCHEMA IDENTICAL`），两侧均 0 error；折入块的 10 个索引在库中齐全。

> 守卫：`tests/unit/migration_consistency_tests.rs::baseline_declares_each_object_exactly_once`
> 逐行解析 `CREATE TABLE` / `CREATE [UNIQUE] INDEX` 名，断言无重复，并逐个断言
> 10 个折入索引与 8 个折入约束仍存在（防止"下次去重把唯一定义一起删掉"）。

## v11 变更摘要 (2026-09-04) — 历史，v11 文件已从磁盘移除

> 本节记录的 v11 基线文件已随 v12 迭代删除；行号引用见
> `git show bddd6109^:migrations/00000000_unified_schema_v11.sql`。

v11 基线相对 v8/v10 的主要变更：

- 吸收 36 个时间戳迁移（2026-06-19 ~ 2026-09-10），含：
  - **Matrix 字段扩展**：`events.redacts`/`redacted_by` 字段、`events` 不级联修复
    （原 `20260831060000_events_no_cascade.sql`）、MSC4242 state DAG prev_state
  - **认证流**：SAML/CAS pending requests、login tokens、QR 登录码、dehydrated
    devices 等新表
  - **同步**：`sliding_sync_*` 表、`thread_subscriptions`/`thread_read_receipts`
  - **E2EE**：megolm_vodozemac dual-write 吸收到 baseline、`burn_after_read_*`
  - **运维**：MV 刷新可配、room CHECK 约束、审计日志 append-only
- 物化视图 `rooms_summaries_mv` 与索引治理（参见 `INDEXES.md`）
- **⚠️ 吸收缺口（曾存在 23 个对象未折入，已全部补齐）**：原来 72 个时间戳迁移文件
  （36 forward + 36 undo）已在 `a0f2819d` 删除，但**有 23 个对象并未真正折入本 baseline**
  ——用有序活集模拟被删迁移得到的最终对象集中，23 个在 baseline 中不存在。完整清单与
  复现脚本见 `docs/audit/PROJECT_ACTUAL_ISSUES_2026-09-14.md` §1。
  - **第一批（2026-09-14）**：`ux_burn_log_user_event`（burn 批量写入 `ON CONFLICT` 的仲裁索引
    ——缺失时每次 burn 清理都报 `42P10`）、`ck_rooms_room_version_valid` 正则化
    （硬编码 1..11 白名单会拒绝 v12/v13 联邦加入）、`trg_prevent_audit_delete`
    （`audit_events` 的 DB 级 append-only 强制）。
  - **第二批（2026-09-16）**：剩余 20 项已由 baseline 尾部的"完整性约束与性能索引
    折入块"提供 —— `fk_event_edges_prev`、`fk_events_redacted_by`、`fk_backup_keys_room`、
    `uq_backup_keys_room_session`、`ck_events_depth_nonneg`、`ck_events_not_before_nonneg`、
    `ck_room_memberships_valid`、`uq_device_keys_user_device_algorithm_keyid`，
    以及 10 个 P1/P2/P3 性能索引（清单见 `INDEXES.md`）。
    折入块内的 `CREATE INDEX CONCURRENTLY` 不能在事务内执行，故该段保留
    `--no-transaction` 标记（`docker/db_migrate.sh` 用 `psql < file` 应用，本就是 autocommit）。
- `events.depth` / `events.not_before` CHECK 约束：✅ 已折入（`ck_events_depth_nonneg` /
  `ck_events_not_before_nonneg`）。

> **这些对象此前长期没被发现的原因**：测试路径
> （`scripts/build_sqlx_migration_source.py`）只选 baseline + extensions + `V*`，
> 时间戳迁移压根不参与；而部署路径（`docker/db_migrate.sh`）会应用目录下全部正向 SQL。
> 于是"CI 建出来的库缺对象"只在生产路径被掩盖为"恰好有对象"。时间戳迁移文件既已删除，
> 两条路径现在都以 baseline 为准。

## 迁移执行顺序

1. `00000000_unified_schema_v12.sql` — 唯一基线（`IF NOT EXISTS`，可重复执行）

> `migrations/` 目前只有上述**唯一**一个正向文件（扩展文件已于 v12 折入后删除）；所有时间戳迁移已删除，
> 不存在"按时间戳顺序逐一应用"的步骤。测试路径
> （`scripts/build_sqlx_migration_source.py`）也显式只选 baseline + extensions + `V*`。
> 部署路径（`docker/db_migrate.sh`）会应用目录下全部正向 SQL，因此**任何未折入 baseline
> 的对象都只存在于部署路径**——这正是 §1 缺口长期未被测试发现的原因。

## 首次部署

```bash
bash docker/db_migrate.sh migrate
```

## 升级已有环境

```bash
bash docker/db_migrate.sh migrate
bash docker/db_migrate.sh validate
```

## 回滚（undo）

每个时间戳迁移配套同名 `.undo.sql`，用于回滚到上一个版本：

```bash
# 顺序倒序执行 .undo.sql（仅 dev/staging 环境）
ls -r migrations/2026*.undo.sql | xargs -I {} bash -c 'echo "--- {}"; psql "$DATABASE_URL" -f "{}"'
```

> ⚠️ undo 文件不含 baseline 内部变更回滚；baseline 内的对象丢失只能 forward fix。

## 扩展迁移选择（仅影响编译的 cargo feature）

`ENABLED_EXTENSIONS` 只决定 `deploy.sh` 用哪套 feature 编译，**不改变建表结果**：

```bash
# 全部功能（默认）
ENABLED_EXTENSIONS=all ./deploy.sh

# 仅核心 Matrix
ENABLED_EXTENSIONS=none ./deploy.sh

# 选择性启用
ENABLED_EXTENSIONS=voice-extended,friends ./deploy.sh
```

可用功能名称（与 Cargo feature flags 一致）：
`friends`, `voice-extended`, `saml-sso`, `cas-sso`,
`beacons`, `voip-tracking`, `widgets`, `server-notifications`,
`burn-after-read`, `privacy-ext`, `external-services`

## 字段命名规范

- 必填毫秒时间戳: `*_ts` (BIGINT NOT NULL)
- 可选时间戳: `*_at` (BIGINT NULLABLE)
- 布尔字段: `is_*` 前缀

## 合并历史

### 第六轮合并 (2026-08-31) — v11 基线

将 v10 baseline + 22 个时间戳迁移（2026-06-19 ~ 2026-08-31，共 25 个文件）合并
为 1 个统一基线 v11。详见 v11 头部注释和本目录的 27 个 timestamp 迁移。

### 第五轮合并 (2026-06-12) — v10 基线（已废弃）

将 v8 系列归档，升级至 v10 双文件基线。

### 第四轮合并 (2026-06-04) — v8 基线（已归档）

v8 基线将 v7 基线 + 8 个批次迁移 + 14 个增量迁移（共 25 个文件）合并为 2 个文件。（v8 文件已随 v11/v12 基线迭代从仓库移除，本目录下不再有 `archive/`，本段仅存历史。）

### 历史合并记录

- 第一轮 (2026-04-22): 26 个增量 → 4 个分组
- 第二轮 (2026-05-07): 5 个扩展 → 1 个，创建 v7 批次
- 第三轮 (2026-05-09): 14 个增量 → 3 个分组

## 相关文档

- `INDEXES.md` — 索引治理文档（partial / composite / 设计原则 / 维护指南）
- `docs/synapse-rust/COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md` — 全面技术审查报告（v7.0）
- `.scratch/db-schema-audit-2026-09-04.md` — v11 schema 审计报告（2026-09-04）