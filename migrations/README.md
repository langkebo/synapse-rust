# 数据库迁移说明

> 最后更新: 2026-09-04

## 目录结构

```
migrations/
├── 00000000_unified_schema_v11.sql           # v11 统一基线（当前活跃，新环境唯一建库入口）
├── 00000001_extensions_v10.sql               # Feature-gated: 扩展表（沿用 v10 extension 文件，未随 v11 改名）
├── 2026XXXXXXXXXX_*.sql (+ .undo.sql)        # 27 个增量迁移 + 对应 undo（按时间戳追加，append-only）
├── archive/                                  # v8 历史基线（仅 `ci_schema_health_check.sh` 用于历史 schema 健康回归，不再作为活跃链路）
│   ├── 00000000_unified_schema_v8.sql
│   ├── 00000001_extensions_v8.sql
│   ├── 20260605120000_megolm_vodozemac_dual_write_v8.sql
│   └── 20260606120000_m26_drop_redundant_module_columns.sql
├── INDEXES.md                                # 索引治理文档（部分索引/复合索引/设计原则）
├── README.md                                 # 本文件
├── build_sqlx_migration_source.py            # 脚本：生成 forward-only migration source
├── check_baseline_consolidation.py           # 脚本：检查 v* baseline 是否吸收所有增量迁移
└── check_migration_consistency.py            # 脚本：检查 undo 配对与命名一致性
```

**当前活跃链路**: `v11 baseline + 1 extension + 27 个时间戳迁移 = 29 个 forward 文件 + 27 个 undo 文件`。

> v8 系列已归档至 `archive/`，不再作为活跃迁移链路。新环境应使用 v11 基线建库。`extension_map.conf` 已废弃（最后生效于 v8 baseline），目前没有调用方引用。

## 新增迁移流程（务必同步折入 baseline）

`build_sqlx_migration_source.py` 生成的 forward-only source 只含 baseline +
extension（CI 用它建库），因此**每次新增时间戳迁移后，必须把幂等的增量变更
（新表/新列/新索引，须用 `IF NOT EXISTS` / `ADD COLUMN IF NOT EXISTS`）同步
折入 `00000000_unified_schema_v11.sql` 尾部**，否则 CI 的 DB 会缺表/列/索引。

提交前请运行一致性检查：

```bash
python3 scripts/check_baseline_consolidation.py
python3 scripts/check_migration_consistency.py
```

历史上曾漏吸收 10 个迁移（federation_dead_letter_queue、login_tokens、
saml_pending_requests、room_event_txn_dedup 等，commit 2d453089/bf85f23f 已折入），
此检查脚本即为防止复发而设。

## 已知死表（待 v12 baseline 重构清理）

v11 baseline 仍包含 `openclaw_connections` / `ai_conversations` / `ai_connections`
三张表及其触发器。openclaw 源码已于 commit 67e66bf4 彻底删除，但这些表定义留在
consolidated baseline 中，新装实例会建出死表。因迁移文件遵循 append-only（不可
修改已有迁移），且时间戳命名的 DROP 迁移不会被 `build_sqlx_migration_source.py`
选中（该脚本只选 baseline + extension + `V*` 迁移），故**暂不清理**；待下一次
consolidated baseline 重构（v12）时移除即可。

> 审计补充（2026-09-04）：v11 baseline 同样存在 `events.reference_image` 字段
> （v11 第 343 行），仅在 `test_mocks/event.rs` 中作为 fixture 写入，业务代码无任何
> 读/写访问，属于死字段。`idx_rooms_name_trgm` 和 `idx_rooms_canonical_alias_trgm`
> 各重复定义两次（v11 第 3542/3543 行和 4035/4036 行），后者由 append-only 策略
> 导致。两项均已纳入 P1/P3 范围，待本次审计迁移落地后由 v12 baseline 重构时
> 彻底清理。

## v11 变更摘要 (2026-09-04)

v11 基线相对 v8/v10 的主要变更：

- 吸收 27 个时间戳迁移（2026-06-19 ~ 2026-09-04），含：
  - **Matrix 字段扩展**：`events.redacts`/`redacted_by` 字段、`events` 不级联修复
    （`20260831060000_events_no_cascade.sql`）、MSC4242 state DAG prev_state
  - **认证流**：SAML/CAS pending requests、login tokens、QR 登录码、dehydrated
    devices 等新表
  - **同步**：`sliding_sync_*` 表、`thread_subscriptions`/`thread_read_receipts`
  - **E2EE**：megolm_vodozemac dual-write 吸收到 baseline、`burn_after_read_*`
  - **运维**：MV 刷新可配、room CHECK 约束、审计日志 append-only
- 物化视图 `rooms_summaries_mv` 与索引治理（参见 `INDEXES.md`）
- `events.depth` / `events.not_before` CHECK 约束补齐
- 审计新增 P1（联邦+完整性）、P2（数据完整性）、P3（性能）、4.1/4.3 四批迁移：
  - `20260904010000_schema_p1_federation_and_integrity.sql`
  - `20260904020000_schema_p2_data_integrity.sql`
  - `20260904030000_schema_p3_perf.sql`
  - `20260904040000_schema_cleanup_dedup_and_dead_code.sql`（reference_image 字段清理）
  - `20260904050000_extend_room_version_check.sql`（room_version CHECK 正则化）

## 迁移执行顺序

1. `00000000_unified_schema_v11.sql` — 基线 (IF NOT EXISTS，幂等)
2. `00000001_extensions_v10.sql` — 按 ENABLED_EXTENSIONS 过滤
3. `2026XXXXXXXXXX_*.sql` — 按时间戳顺序逐一应用

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

## 扩展迁移选择

通过 `ENABLED_EXTENSIONS` 环境变量控制：

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

v8 基线将 v7 基线 + 8 个批次迁移 + 14 个增量迁移（共 25 个文件）合并为 2 个文件。详见 `archive/` 目录。

### 历史合并记录

- 第一轮 (2026-04-22): 26 个增量 → 4 个分组
- 第二轮 (2026-05-07): 5 个扩展 → 1 个，创建 v7 批次
- 第三轮 (2026-05-09): 14 个增量 → 3 个分组

## 相关文档

- `INDEXES.md` — 索引治理文档（partial / composite / 设计原则 / 维护指南）
- `docs/synapse-rust/COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md` — 全面技术审查报告（v7.0）
- `.scratch/db-schema-audit-2026-09-04.md` — v11 schema 审计报告（2026-09-04）