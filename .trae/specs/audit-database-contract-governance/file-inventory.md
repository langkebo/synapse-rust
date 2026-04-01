# 数据库迁移治理最小实现文件清单

## 使用说明
本清单基于已完成的仓库排查结果整理，目标是为“数据库契约治理最小闭环”提供范围收敛依据。分类原则如下：

- 可复用：已有能力可直接进入最小实现，只需沿用或轻微口径补充
- 待修改：适合纳入最小实现，但存在规范、覆盖范围或实现细节不一致，需要修正后再进入主链
- 暂不纳入最小实现：具备价值，但当前超出最小闭环范围，或与现行元数据模型不一致，留待后续阶段处理

## 可复用

| 文件 | 当前作用 | 当前状态 | 最小实现结论 |
| --- | --- | --- | --- |
| `docs/db/MIGRATION_GOVERNANCE.md` | 治理总文档，定义目标、门禁矩阵、发布准入 | 骨架完整，适合作为主说明 | 继续复用，仅回写与索引文档不一致的命名口径 |
| `migrations/MIGRATION_INDEX.md` | 迁移资产索引与操作台账入口 | 已明确目录模型、命名、回滚、manifest | 作为唯一规范源保留 |
| `docs/synapse-rust/DEPLOYMENT_GUIDE.md` | 生产部署指南 | 已对齐统一迁移入口并补充兼容开关说明 | 继续复用，避免出现旧 Rust bin 入口口径 |
| `scripts/generate_migration_manifest.py` | 生成迁移 manifest | 能力可用，但需补目录扫描范围 | 复用现有结构，补扫 `incremental/`、`hotfix/` |
| `scripts/verify_migration_manifest.py` | 校验 manifest 完整性与摘要 | 与发布物校验目标一致 | 直接复用 |
| `migrations/MANIFEST-template.txt` | manifest 发布模板 | 可直接作为发布物模板 | 直接复用 |
| `.github/workflows/db-migration-gate.yml` | 迁移治理专项门禁 | 已覆盖 contract、layout audit、manifest、unified apply、sqlx migrate、checksum | 作为唯一迁移治理门禁保留 |
| `.github/workflows/docs-quality-gate.yml` | 文档质量门禁 | 已覆盖数据库治理文档 | 直接复用 |
| `.github/workflows/db-replica-consistency.yml` | 主从一致性与发布前校验 | 已具备雏形 | 保留为发布前或定时任务 |
| `docker/db_migrate.sh` | Docker 运维迁移入口 | 现阶段可用，但应避免扩展成第二套治理体系 | 继续复用为运维入口，不上升为新的治理规范源 |
| `docker/docker-compose.yml` | 本地与容器部署环境入口 | 已显式关闭运行时 Rust 迁移主链 | 继续复用为部署入口 |
| `docker/config/.env.example` | 部署环境变量模板 | 已补充运行时迁移兼容开关 | 继续复用为环境模板 |
| `migrations/README.md` | 迁移说明文档（用户侧口径） | 已对齐统一入口与容器内脚本路径 | 继续复用，随治理文档同步维护 |

## 待修改

| 文件 | 当前问题 | 风险 | 最小实现修改建议 |
| --- | --- | --- | --- |
| `docs/db/MIGRATION_GOVERNANCE.md` | 仍保留旧时间戳命名口径 | 文档与人工执行分叉 | 以 `MIGRATION_INDEX.md` 规则回写 |
| `scripts/generate_migration_manifest.py` | 只扫根目录 migration、rollback、archive | 后续目录治理推进后 manifest 漏文件 | 补扫 `migrations/*.sql`、`migrations/incremental/*.sql`、`migrations/hotfix/*.sql` |
| `scripts/audit_migration_layout.py` | 主要审计根目录模式 | `incremental/`、`hotfix/` 缺少 rollback 配套检查 | 扩展目录覆盖并保留现有根目录逻辑 |
| `Makefile` | 查询 `schema_migrations` 使用过时字段 | 本地运维命令误导 | 改为当前实际列，如 `applied_ts`、`executed_at` |
| `scripts/db/rollback_drill.py` | 仍使用 `installed_rank`、`installed_on` | 回滚演练无法与现状元数据表对齐 | 修正为当前 `schema_migrations` 实际列 |
| `.github/workflows/db-migration-gate.yml` | `unified-schema-apply` 仍维护硬编码关键补丁列表 | 新增关键迁移时易漏改 workflow | 最小实现阶段先保留，新增迁移时同步维护 |
| `.github/workflows/ci.yml` | 与专项门禁都涉及迁移初始化 | 主 CI 与治理门禁职责边界模糊 | 保留通用测试，将治理责任明确归口到 `db-migration-gate.yml` |

## 暂不纳入最小实现

| 文件 | 当前作用 | 暂不纳入原因 | 后续建议 |
| --- | --- | --- | --- |
| `src/services/database_initializer.rs` | 运行时数据库初始化入口 | 已默认降级为显式开关控制的兼容入口，不再默认执行迁移主链 | 后续继续评估是否仅保留只读健康检查 |
| `src/bin/run_migrations.rs` | 独立 Rust 迁移入口 | 已改为委托 `docker/db_migrate.sh` 的兼容包装器 | 后续如无外部依赖，可进一步归档或删除 |
| `migrations/99999999_unified_incremental_migration.sql` | 历史汇总增量脚本 | 已明确为兼容资产，不再作为唯一升级入口 | 后续视外部依赖决定是否归档 |
| `scripts/check_schema_contract_coverage.py` | 契约覆盖静态检查 | 已对齐当前时间字段标准，并声明仅校验迁移源码覆盖 | 后续如需要可增强 ALTER TABLE 解释能力 |
| `src/storage/schema_validator.rs` | 运行时 schema 自检与修补 | 偏在线修库能力，不适合最小治理主链 | 作为运行保障层保留 |
| `src/storage/schema_health_check.rs` | 启动时健康检查与自动修复 | 更偏运行保障而非迁移治理闭环 | 保留为运行保障层 |
| `scripts/db/extract_schema.py` | schema 抽取 | 属于 drift diff 增强能力 | 二期再纳入 |
| `scripts/db/diff_schema.py` | schema diff | 属于漂移检测增强项 | 二期再纳入 |
| `scripts/run_pg_amcheck.py` | 物理完整性检查 | 更适合作为发布前增强门禁 | 保留在 release gate |
| `scripts/generate_logical_checksum_report.py` | 逻辑 checksum 报告 | 更适合作为副本一致性或发布门禁 | 保留在 release gate |
| `scripts/db/lifecycle_manager.py` | 生命周期管理 | 属于治理提效工具 | 后续再纳入 |
| `scripts/db/compress_migrations.py` | 压缩与归档迁移 | 属于治理增强工具 | 后续再纳入 |

## 最小实现落地顺序

1. 统一 `MIGRATION_GOVERNANCE.md` 与 `MIGRATION_INDEX.md` 的命名口径
2. 补齐 `generate_migration_manifest.py` 对 `incremental/`、`hotfix/` 的扫描
3. 补齐 `audit_migration_layout.py` 对新增目录的 rollback 审计
4. 修正 `Makefile` 与 `scripts/db/rollback_drill.py` 的过时列名
5. 将 `db-migration-gate.yml` 固化为 PR 必过治理门禁，并记录关键补丁列表维护要求

## 交付口径

- 本清单用于界定“最小实现”边界，不等同于完整数据库治理路线图
- 本清单优先保证现有骨架可持续、可执行、可审计
- 自动生成 critical increment 列表、重写整套迁移链、引入第二套执行入口不属于本轮交付
