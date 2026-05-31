# Database Audit And Remediation

审查日期: 2026-05-29

范围:
- `migrations/`
- `docker/deploy/migrations/`
- storage/service 中的 SQL 字符串引用
- schema coverage 与 schema contract 检查脚本
- API 与 schema 对应验证脚本

## Findings Fixed

### 1. Migration / deploy mirror drift

`scripts/check_migration_consistency.py` 发现主迁移目录与 Docker deploy 迁移目录不同步。

修复:
- 补齐 `migrations/20260515120000_burn_after_read_persistence.undo.sql`
- 补齐 `migrations/20260516000001_key_rotation_pending_tables.undo.sql`
- 补齐 `migrations/20260518000001_performance_indexes.undo.sql`
- 将缺失的 forward / undo 文件同步到 `docker/deploy/migrations/`

验证:

```bash
python3 scripts/check_migration_consistency.py
```

结果: `status: ok`, issues `0`, warnings `0`.

### 2. Search SQL referenced a non-existent table

`scripts/check_schema_table_coverage.py` 发现 `src/services/search_service.rs` 引用了 `room_members`。

实际 schema 表为 `room_memberships`。这会导致 Postgres-backed search 在运行时查询失败，或者绕过成员状态过滤。

修复:
- 将两处 `INNER JOIN room_members` 改为 `INNER JOIN room_memberships`
- 同时增加 `rm.membership = 'join'` 条件，避免 left/invite/ban 等非 joined 状态获得搜索可见性

验证:

```bash
cargo test --lib search_service -- --nocapture
```

结果: 22 passed.

### 3. Schema table coverage false positives

coverage 脚本将 SQL 注释中的 `outer query` 识别为表 `outer`，并将 `EXTRACT(EPOCH FROM rotated_at)` 中的字段识别为表 `rotated_at`。

修复:
- 在 `scripts/check_schema_table_coverage.py` 中剥离 SQL line/block comments
- 在表引用提取前屏蔽 `EXTRACT(... FROM ...)` 表达式

验证:

```bash
python3 scripts/check_schema_table_coverage.py --json-report -
```

结果: `status: pass`, missing `[]`.

### 4. Schema contract still expected deleted redundant table indexes

`scripts/check_schema_contract_coverage.py` 仍期望 `room_children` 的旧索引:
- `idx_room_children_parent_suggested`
- `idx_room_children_child`

但项目冗余清理文档已将 `room_children` 标记为被 `space_children` 替代并删除的冗余表。

修复:
- 从 schema contract 期望中移除 `room_children`

验证:

```bash
python3 scripts/check_schema_contract_coverage.py --json-report -
```

结果: `status: pass`, coverage `100.0%`.

### 5. API/schema verification used stale media and container mappings

`scripts/api_schema_verify.sh` 仍默认连接旧容器名 `synapse_db_prod`，并将媒体 API 映射到不存在的旧表 `media`。

实际 Docker/Compose 容器为 `synapse-postgres`，媒体元数据落在 `media_metadata`。

修复:
- 将默认 `DB_CONTAINER` 调整为 `synapse-postgres`
- 将媒体上传、下载、admin media API 映射到 `media_metadata`
- 将 `set -e` 下的计数器递增改为 `+=1`，避免第一次递增表达式返回 0 时提前退出

验证:

```bash
bash scripts/api_schema_verify.sh
```

结果: 29 passed, 0 failed.

### 6. Full schema audit script was pinned to obsolete migration names

`scripts/audit_db_schema.py` 仍只扫描 v6 / 202604 迁移，导致本轮首次运行时提取到 0 张 SQL 表，审计报告实际为空跑。

修复:
- 改为扫描 `migrations/` 顶层全部 active forward SQL
- 在提取 schema 前剥离 SQL line/block comments
- 支持 `ALTER TABLE ... ADD COLUMN ..., ADD COLUMN ...` 多列语句
- 正确识别字段上一行的 `#[sqlx(rename = "...")]`
- 区分整表 `FromRow` 模型和查询投影，降低聚合/alias 查询误报

验证:

```bash
python3 scripts/audit_db_schema.py
```

结果: 已能提取 255 张 SQL 表、169 个 `FromRow` 结构体、150 组匹配关系。

### 7. Module-management schema drift

审计发现模块管理相关表与 storage 层实际读写字段不一致:
- `modules` 缺少 `version`、执行计数与错误字段
- `module_execution_logs` 缺少 `module_name`、`module_type`、`success`、`executed_ts` 等 storage 写入字段
- `media_callbacks` 缺少创建接口写入字段和响应结构读取字段
- `refresh_token_usage` 代码写入 `success`，但 schema 列名为 `is_success`

修复:
- 新增 `20260529000001_module_schema_alignment.sql` 及 undo，并同步到 `docker/deploy/migrations/`
- 为模块、模块执行日志、媒体回调补齐 storage 运行时字段和查询索引
- 修复 `RefreshTokenUsage.success` 的 `sqlx(rename = "is_success")`
- 将 refresh token usage 写入 SQL 改为 `is_success`

验证:

```bash
bash docker/db_migrate.sh migrate
bash docker/db_migrate.sh validate
cargo test --lib refresh_token -- --nocapture
cargo test --lib module -- --nocapture
```

结果:
- 本地 Docker Postgres 已成功应用 `20260529000001_module_schema_alignment.sql`
- `docker/db_migrate.sh validate` 通过
- refresh token 单测 7 passed
- module 单测 14 passed

### 8. Module result storage was stubbed and not persisted

模块服务会生成 spam-check 和 third-party-rule 结果，但 storage 层此前只返回内存对象:
- `create_spam_check_result` 返回 `id: 0`，不写入 `spam_check_results`
- `get_spam_check_result` / `get_spam_check_results_by_sender` 永远返回空
- `create_third_party_rule_result` 返回内存对象，不写入 `third_party_rule_results`
- `get_third_party_rule_results` 永远返回空

修复:
- 新增 `20260529000002_module_result_persistence.sql` 及 undo，并同步到 `docker/deploy/migrations/`
- 迁移兼容已有部署: 表不存在时创建，表存在时补齐字段和索引
- `spam_check_results` 持久化 sender、event_type、content、result、score、checker、checked_ts 等查询/响应字段
- `third_party_rule_results` 持久化 sender、event_type、rule_name、is_allowed、reason、modified_content、checked_ts
- storage 的 create/get 方法改为真实 SQL，不再返回 stub 空结果

验证:

```bash
bash docker/db_migrate.sh migrate
bash docker/db_migrate.sh validate
cargo test --lib module -- --nocapture
python3 scripts/audit_db_schema.py
```

结果:
- 本地 Docker Postgres 已成功应用 `20260529000002_module_result_persistence.sql`
- `docker/db_migrate.sh validate` 通过
- module 单测 14 passed
- full schema audit 的 CRITICAL 从 34 降至 23

### 9. Removed redundant module admin surfaces for dropped tables

`presence_routes` 和 `rate_limit_callbacks` 已在冗余表删除迁移中明确标记为过度设计并 DROP，但 module admin router 仍暴露对应 POST/GET 路由，storage 方法也仍保留 stub。

修复:
- 删除 `PresenceRoute` / `RateLimitCallback` 相关 `FromRow` 结构体、请求/响应类型和 storage stub
- 从 module admin router 与 route manifest 移除 `/_synapse/admin/v1/presence_routes`
- 从 module admin router 与 route manifest 移除 `/_synapse/admin/v1/rate_limit_callbacks`

验证:

```bash
python3 scripts/audit_db_schema.py
cargo test --lib module -- --nocapture
```

结果:
- full schema audit 的 CRITICAL 从 23 降至 16
- `presence_routes` / `rate_limit_callbacks` 不再作为 code/schema mismatch 出现

## Remaining Risks

- 本轮已在现有 Docker Postgres 上执行 `docker/db_migrate.sh migrate` / `validate` 并通过；尚未执行完整 integration/e2e 测试矩阵。
- API/schema 映射脚本仍是人工维护清单，后续可以从 route ledger 与 storage SQL 引用自动生成或交叉校验，减少漂移。
- `scripts/audit_db_schema.py` 已修复明显空跑和 rename/multi-ALTER 问题，但仍是启发式工具；聚合查询、stub storage、跨表投影仍需要人工复核。
- module admin route ledger 快照如覆盖全部 admin surface，需在完整 integration snapshot gate 中随路由删除同步更新。
- Deploy mirror drift 已修复，但后续新增迁移必须同时添加 undo，并保持 `migrations/` 与 `docker/deploy/migrations/` 同步。

## Recommended Follow-Up Gates

```bash
python3 scripts/check_migration_consistency.py
python3 scripts/check_schema_table_coverage.py --json-report -
python3 scripts/check_schema_contract_coverage.py --json-report -
bash scripts/api_schema_verify.sh
cargo test --lib search_service -- --nocapture
cargo test --lib refresh_token -- --nocapture
cargo test --lib module -- --nocapture
bash docker/db_migrate.sh validate
python3 scripts/audit_db_schema.py
```

Extended integration gate:

```bash
cargo test --features test-utils --test integration database_integrity_tests -- --nocapture
```
