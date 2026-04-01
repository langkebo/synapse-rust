# 数据库契约治理最小闭环 Spec

## Why
仓库已经具备迁移治理的可复用骨架，包括治理总文档、迁移索引、manifest 生成与校验脚本、数据库专项门禁工作流、文档门禁以及主从一致性检查流程。当前主要问题不是能力缺失，而是文档口径、脚本扫描范围、CI 维护方式和运维脚本字段名存在少量不一致，导致“规范已存在、实现仍分叉”。本次规格因此收敛为最小实现，优先完成口径统一与少量脚本补齐，避免重写整套迁移治理流程。

## What Changes
- 统一 `MIGRATION_GOVERNANCE.md` 与 `MIGRATION_INDEX.md` 的命名规则与目录模型，以迁移索引文档现行规则为唯一规范源
- 明确 `db-migration-gate.yml` 为唯一迁移治理门禁，主 CI 仅保留通用构建与测试职责
- 补齐 manifest 生成脚本对 `migrations/` 根目录、`migrations/incremental/`、`migrations/hotfix/` 的扫描覆盖
- 补齐布局审计脚本对 `incremental/`、`hotfix/` 的 rollback 配套检查
- 修正 `Makefile` 与回滚演练脚本中对 `schema_migrations` 历史列名的错误引用，统一到当前实际列
- 暂时保留 `db-migration-gate.yml` 中硬编码关键增量列表，但将其定义为最小实现期允许存在的人工维护点
- 将 Rust 运行时迁移入口降级为兼容路径，默认关闭运行时数据库初始化并统一委托 `docker/db_migrate.sh`
- 同步部署文档与迁移说明文档的执行入口口径，并补充 `SYNAPSE_ENABLE_RUNTIME_DB_INIT` 的兼容定位与相关环境变量说明
- 新增最小实现文件清单，明确“可复用 / 待修改 / 暂不纳入最小实现”的资产边界

## Reuse Baseline
- 可直接复用的主说明文档：`docs/db/MIGRATION_GOVERNANCE.md`
- 可直接复用的迁移台账入口：`migrations/MIGRATION_INDEX.md`
- 可直接复用的 manifest 能力：`scripts/generate_migration_manifest.py`、`scripts/verify_migration_manifest.py`
- 可直接复用的发布模板：`migrations/MANIFEST-template.txt`
- 可直接复用的专项门禁：`.github/workflows/db-migration-gate.yml`
- 可直接复用的文档门禁：`.github/workflows/docs-quality-gate.yml`
- 可直接复用的副本一致性流：`.github/workflows/db-replica-consistency.yml`

## Impact
- Affected specs: 数据库迁移治理、CI 质量门禁、运维审计入口、回滚演练流程
- Affected deliverables: `file-inventory.md`
- Affected code: `docs/db/`, `docs/synapse-rust/DEPLOYMENT_GUIDE.md`, `migrations/`, `scripts/generate_migration_manifest.py`, `scripts/audit_migration_layout.py`, `scripts/rollback_drill.py`, `.github/workflows/db-migration-gate.yml`, `Makefile`, `src/server.rs`, `src/services/database_initializer.rs`, `src/bin/run_migrations.rs`, `docker/docker-compose.yml`, `docker/config/.env.example`
- Affected systems: PostgreSQL、GitHub Actions、Docker 运维入口、本地迁移审计命令

## ADDED Requirements

### Requirement: 以最小实现方式收敛迁移治理口径
系统 SHALL 以现有治理骨架为基础完成最小实现，只修正文档口径不一致、脚本覆盖缺口与错误字段引用，不得在本轮重写整套迁移流程。

#### Scenario: 定义最小实现边界
- **WHEN** 审查数据库迁移治理现状
- **THEN** 必须先复用现有文档、manifest、CI 门禁与校验脚本
- **AND** 只允许引入口径统一、扫描范围补齐、字段名修正和职责边界澄清
- **AND** 不得把 Docker 运维脚本扩展成第二套新的治理体系

### Requirement: 统一迁移命名与目录规范源
系统 SHALL 以 `MIGRATION_INDEX.md` 当前采用的命名规则与目录模型作为最小实现阶段的唯一规范源，并回写所有仍保留旧口径的治理文档。

#### Scenario: 同步治理文档口径
- **WHEN** `MIGRATION_GOVERNANCE.md` 与 `MIGRATION_INDEX.md` 对命名规则或目录结构描述不一致
- **THEN** 必须以 `MIGRATION_INDEX.md` 的现行规则为准
- **AND** 必须同步更新治理总文档、维护说明和任何引用旧时间戳格式的描述
- **AND** CI 规则、人工执行步骤与文档示例必须共享同一命名口径

### Requirement: 补齐 manifest 与布局审计的目录覆盖
系统 SHALL 让 manifest 生成与布局审计脚本覆盖根目录迁移、`incremental/` 与 `hotfix/` 目录，避免治理目录推进后出现漏检。

#### Scenario: 扫描完整迁移目录
- **WHEN** 执行 manifest 生成或布局审计
- **THEN** 必须同时纳入 `migrations/*.sql`、`migrations/incremental/*.sql`、`migrations/hotfix/*.sql`
- **AND** 对 `incremental/`、`hotfix/` 中的正向脚本执行配套 rollback 检查
- **AND** 未纳入 manifest 或缺少 rollback 配对的脚本必须被标记为失败

### Requirement: 固化最小治理门禁职责
系统 SHALL 将 `db-migration-gate.yml` 定义为唯一迁移治理门禁工作流，主 CI 继续承担通用构建与测试，不再承载迁移治理口径。

#### Scenario: 划分迁移治理与主 CI 职责
- **WHEN** 评估 `db-migration-gate.yml` 与 `ci.yml` 的职责边界
- **THEN** 必须把 schema coverage、contract、layout audit、manifest、unified apply、sqlx migrate、checksum 等治理责任归口到 `db-migration-gate.yml`
- **AND** 允许 `ci.yml` 保留通用 `sqlx migrate run` 初始化，但不得把它作为迁移治理规范源
- **AND** `db-migration-gate.yml` 必须被定义为 PR 必过检查

#### Scenario: 保留人工维护点但显式记录
- **WHEN** `db-migration-gate.yml` 仍使用硬编码关键增量列表
- **THEN** 最小实现阶段允许继续保留该机制
- **AND** 必须在文档或任务中明确新增关键迁移时需要同步维护工作流
- **AND** 后续如改为由 manifest 或索引自动生成，应视为下一阶段优化而非本轮前置条件

### Requirement: 修正运维与回滚脚本的版本记录字段
系统 SHALL 统一所有运维查询与回滚演练脚本对 `schema_migrations` 实际列名的引用，避免继续使用不存在的 Flyway 风格字段。

#### Scenario: 对齐 schema_migrations 实际结构
- **WHEN** `Makefile`、`rollback_drill.py` 或相关脚本查询迁移历史
- **THEN** 必须使用当前 `schema_migrations` 的实际列，如 `applied_ts`、`executed_at`
- **AND** 不得继续引用 `installed_rank`、`installed_on` 等仓库现状不存在的列
- **AND** 所有示例命令、回滚演练与维护文档必须同步修正

### Requirement: 明确暂不纳入最小实现的范围
系统 SHALL 在本轮治理规格中显式声明不会纳入最小实现的事项，避免再次扩大改造范围。

#### Scenario: 排除非最小实现改造
- **WHEN** 评估第二阶段或长期治理事项
- **THEN** 必须把“重写整套迁移流程”“把关键增量列表自动生成化”“新增第二套运维入口”“扩展 Docker 迁移链职责”标记为后续优化
- **AND** 最小实现仅要求现有骨架可持续、可审计、可执行

## MODIFIED Requirements

### Requirement: 数据库迁移治理从“全面重构规范”调整为“最小闭环执行规范”
现有迁移治理要求不再以重建整套版本链、重写全部脚本或新增第二套入口为目标，而是以统一规范源、补齐扫描覆盖、修正错误脚本和固化专项门禁为首要目标。

## REMOVED Requirements

### Requirement: 本轮必须先完成完整契约问题台账与全量根因分析
**Reason**: 用户已完成一轮完整排查，当前规格阶段应直接承接审计结论，转向最小实现闭环，而不是重复构建全量问题台账。
**Migration**: 审计结论作为现有输入保留，后续任务直接围绕文档统一、脚本补齐、门禁固化与运维修正展开。
