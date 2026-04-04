# P2 长期改进详细方案

> 日期：2026-04-04  
> 状态：待执行  
> 优先级：P2（长期改进）  
> 预计时间：1-2 个月

---

## 一、方案概述

### 1.1 目标

基于 `DATABASE_AUDIT_SUMMARY_2026-04-04.md` 中的 P2 任务，制定详细的长期改进方案，包括：
1. 标准化迁移流程和工具
2. 建立性能基准测试体系
3. 实施自动化 Schema 验证
4. 完善运维手册和文档

### 1.2 原则

- ✅ 不影响现有功能实现
- ✅ 符合项目最佳实践
- ✅ 可增量执行，每个任务独立可验证
- ✅ 复用现有工具和脚本
- ✅ 所有改动通过 CI 验证

### 1.3 当前资产盘点

**已有工具脚本**：
- `scripts/migration_manager.sh` - 迁移管理工具（基础版）
- `benches/run_benchmarks.sh` - 性能基准测试脚本
- `scripts/verify_migration.sh` - 迁移验证脚本
- `scripts/db_schema_check.sh` - Schema 检查脚本
- `scripts/check_schema_contract_coverage.py` - Schema 合约覆盖检查
- `scripts/audit_migration_layout.py` - 迁移布局审计
- `.github/workflows/db-migration-gate.yml` - 迁移门禁 CI
- `.github/workflows/drift-detection.yml` - Schema 漂移检测 CI

**已有文档**：
- `migrations/README.md` - 迁移使用说明
- `migrations/MIGRATION_INDEX.md` - 迁移索引
- `docs/db/DATABASE_AUDIT_REPORT_2026-04-04.md` - 审计报告
- `docs/db/CONSOLIDATION_PLAN.md` - 合并计划
- `TESTING.md` - 测试指南

---

## 二、任务分解

### 2.1 Task 1: 增强迁移工具脚本（P2.1）

**目标**：扩展现有 `migration_manager.sh`，提供完整的迁移生命周期管理。

#### 2.1.1 功能增强清单

1. **迁移创建工具**
   - 自动生成迁移文件模板（带时间戳命名）
   - 自动生成对应的 undo/rollback 脚本模板
   - 验证迁移命名符合规范
   - 自动更新 `MIGRATION_INDEX.md`

2. **迁移测试工具**
   - 在隔离环境中测试迁移（使用临时数据库）
   - 测试 up + down 循环（apply + rollback）
   - 验证迁移幂等性
   - 生成测试报告

3. **迁移验证工具**
   - 检查 SQL 语法
   - 检查是否使用 `CREATE INDEX CONCURRENTLY`
   - 检查是否有危险操作（DROP TABLE, TRUNCATE）
   - 检查是否有重复定义

#### 2.1.2 实施步骤

**Step 1**: 创建迁移生成工具
```bash
# 新增命令：scripts/migration_manager.sh create <name> <description>
# 输出：
#   - migrations/YYYYMMDDHHMMSS_<name>.sql
#   - migrations/undo/YYYYMMDDHHMMSS_<name>_undo.sql
#   - 更新 MIGRATION_INDEX.md
```

**Step 2**: 创建迁移测试工具
```bash
# 新增命令：scripts/migration_manager.sh test <migration_file>
# 功能：
#   1. 创建临时测试数据库
#   2. 应用基线 schema
#   3. 应用目标迁移
#   4. 验证 schema 变更
#   5. 应用 undo 脚本
#   6. 验证回滚成功
#   7. 清理临时数据库
```

**Step 3**: 创建迁移验证工具
```bash
# 新增命令：scripts/migration_manager.sh validate <migration_file>
# 检查项：
#   - SQL 语法检查（使用 psql --dry-run）
#   - 索引创建检查（必须使用 CONCURRENTLY）
#   - 危险操作检查
#   - 重复定义检查
```

#### 2.1.3 验收标准

- ✅ `migration_manager.sh create` 能生成符合规范的迁移文件
- ✅ `migration_manager.sh test` 能在隔离环境测试迁移
- ✅ `migration_manager.sh validate` 能检测常见问题
- ✅ 所有新增功能有使用文档
- ✅ 通过手动测试验证

#### 2.1.4 交付物

- `scripts/migration_manager.sh`（增强版）
- `docs/db/MIGRATION_TOOLS_GUIDE.md`（工具使用指南）
- 测试报告

---

### 2.2 Task 2: 建立性能基准测试体系（P2.2）

**目标**：建立可重复的性能基准测试，用于监控性能回归。

#### 2.2.1 基准测试范围

1. **数据库性能基准**
   - 核心查询性能（用户查询、房间查询、事件查询）
   - 索引效率测试
   - 批量操作性能
   - 并发写入性能

2. **API 性能基准**
   - 已有：`benches/performance_api_benchmarks.rs`
   - 需补充：更多端点覆盖

3. **Federation 性能基准**
   - 已有：`benches/performance_federation_benchmarks.rs`
   - 需补充：跨服务器延迟测试

#### 2.2.2 实施步骤

**Step 1**: 创建数据库性能基准测试
```bash
# 新建：benches/performance_database_benchmarks.rs
# 测试项：
#   - 用户查询（按 user_id, username）
#   - 房间查询（按 room_id, 成员列表）
#   - 事件查询（按 event_id, room_id + 时间范围）
#   - 批量插入（users, events, devices）
#   - 并发写入（模拟多客户端）
```

**Step 2**: 创建基准数据生成工具
```bash
# 新建：scripts/generate_benchmark_data.sh
# 功能：
#   - 生成测试用户（1K, 10K, 100K）
#   - 生成测试房间（100, 1K, 10K）
#   - 生成测试事件（10K, 100K, 1M）
#   - 支持可重复生成（固定种子）
```

**Step 3**: 创建基准测试运行脚本
```bash
# 增强：benches/run_benchmarks.sh
# 新增功能：
#   - 自动生成基准数据
#   - 运行所有基准测试
#   - 生成性能报告（Markdown + JSON）
#   - 与历史基准对比
```

**Step 4**: 集成到 CI（可选）
```yaml
# 新建：.github/workflows/performance-baseline.yml
# 触发条件：手动触发或每周定时
# 功能：
#   - 运行基准测试
#   - 保存结果到 artifacts
#   - 与上次基准对比
#   - 性能回归告警（>10% 下降）
```

#### 2.2.3 验收标准

- ✅ 数据库基准测试覆盖核心查询
- ✅ 基准数据生成工具可重复生成
- ✅ 基准测试脚本能生成对比报告
- ✅ 基准测试结果可追溯（保存历史数据）
- ✅ 文档说明如何运行和解读基准测试

#### 2.2.4 交付物

- `benches/performance_database_benchmarks.rs`
- `scripts/generate_benchmark_data.sh`
- `benches/run_benchmarks.sh`（增强版）
- `docs/PERFORMANCE_BASELINE.md`（基准测试指南）
- 初始基准测试报告

---

### 2.3 Task 3: 实施自动化 Schema 验证（P2.3）

**目标**：在 CI/CD 中自动验证 Schema 一致性和合约覆盖。

#### 2.3.1 验证范围

1. **Schema 一致性验证**
   - 已有：`.github/workflows/drift-detection.yml`
   - 需增强：更详细的差异报告

2. **Schema 合约覆盖验证**
   - 已有：`scripts/check_schema_contract_coverage.py`
   - 需增强：覆盖率阈值检查

3. **迁移布局验证**
   - 已有：`scripts/audit_migration_layout.py`
   - 需增强：自动检测违规

#### 2.3.2 实施步骤

**Step 1**: 增强 Schema 漂移检测
```yaml
# 修改：.github/workflows/drift-detection.yml
# 增强：
#   - 生成详细差异报告（表/列/索引/约束）
#   - 差异报告保存为 artifact
#   - 发现漂移时失败（而不是仅警告）
```

**Step 2**: 增强 Schema 合约覆盖检查
```python
# 修改：scripts/check_schema_contract_coverage.py
# 增强：
#   - 添加覆盖率阈值参数（默认 90%）
#   - 覆盖率低于阈值时返回非零退出码
#   - 生成覆盖率报告（Markdown）
```

**Step 3**: 集成到 CI 门禁
```yaml
# 修改：.github/workflows/db-migration-gate.yml
# 新增步骤：
#   - 运行 Schema 合约覆盖检查
#   - 覆盖率 < 90% 时失败
#   - 生成覆盖率报告
```

**Step 4**: 创建 Schema 验证总览脚本
```bash
# 新建：scripts/validate_schema_all.sh
# 功能：
#   - 运行所有 Schema 验证脚本
#   - 生成综合验证报告
#   - 一键本地验证
```

#### 2.3.3 验收标准

- ✅ Schema 漂移检测能生成详细报告
- ✅ Schema 合约覆盖率 ≥ 90%
- ✅ CI 门禁能自动验证 Schema
- ✅ 本地验证脚本可一键运行
- ✅ 文档说明验证流程

#### 2.3.4 交付物

- `.github/workflows/drift-detection.yml`（增强版）
- `scripts/check_schema_contract_coverage.py`（增强版）
- `scripts/validate_schema_all.sh`
- `docs/db/SCHEMA_VALIDATION_GUIDE.md`

---

### 2.4 Task 4: 完善运维手册和文档（P2.4）

**目标**：提供完整的数据库运维手册，降低运维门槛。

#### 2.4.1 文档清单

1. **迁移运维手册**
   - 迁移创建流程
   - 迁移测试流程
   - 迁移部署流程
   - 迁移回滚流程
   - 常见问题排查

2. **性能优化手册**
   - 性能基准测试流程
   - 性能问题诊断
   - 索引优化指南
   - 查询优化指南

3. **故障恢复手册**
   - 数据库备份策略
   - 数据库恢复流程
   - 迁移失败恢复
   - 数据一致性修复

4. **监控告警手册**
   - 关键指标监控
   - 告警规则配置
   - 告警响应流程

#### 2.4.2 实施步骤

**Step 1**: 创建迁移运维手册
```markdown
# 新建：docs/db/MIGRATION_OPERATIONS_GUIDE.md
# 内容：
#   - 迁移创建：使用 migration_manager.sh create
#   - 迁移测试：使用 migration_manager.sh test
#   - 迁移部署：使用 docker/db_migrate.sh migrate
#   - 迁移回滚：使用 undo 脚本
#   - 常见问题：锁表、超时、语法错误
```

**Step 2**: 创建性能优化手册
```markdown
# 新建：docs/db/PERFORMANCE_OPTIMIZATION_GUIDE.md
# 内容：
#   - 基准测试：使用 benches/run_benchmarks.sh
#   - 慢查询诊断：使用 pg_stat_statements
#   - 索引优化：EXPLAIN ANALYZE 分析
#   - 查询优化：常见模式和反模式
```

**Step 3**: 创建故障恢复手册
```markdown
# 新建：docs/db/DISASTER_RECOVERY_GUIDE.md
# 内容：
#   - 备份策略：物理备份 + 逻辑备份
#   - 恢复流程：PITR 恢复步骤
#   - 迁移失败恢复：回滚步骤
#   - 数据一致性修复：外键、索引重建
```

**Step 4**: 创建监控告警手册
```markdown
# 新建：docs/db/MONITORING_ALERTING_GUIDE.md
# 内容：
#   - 关键指标：连接数、QPS、慢查询、锁等待
#   - 告警规则：阈值配置
#   - 告警响应：分级响应流程
#   - 工具推荐：Prometheus + Grafana
```

**Step 5**: 更新主文档索引
```markdown
# 修改：README.md
# 新增章节：数据库运维
#   - 链接到各运维手册
#   - 快速导航
```

#### 2.4.3 验收标准

- ✅ 所有手册文档完整
- ✅ 文档包含实际命令和示例
- ✅ 文档经过技术审查
- ✅ 文档链接正确
- ✅ 主文档索引更新

#### 2.4.4 交付物

- `docs/db/MIGRATION_OPERATIONS_GUIDE.md`
- `docs/db/PERFORMANCE_OPTIMIZATION_GUIDE.md`
- `docs/db/DISASTER_RECOVERY_GUIDE.md`
- `docs/db/MONITORING_ALERTING_GUIDE.md`
- `README.md`（更新）

---

## 三、执行计划

### 3.1 时间规划

| 任务 | 预计时间 | 依赖 | 负责人 |
|------|---------|------|--------|
| Task 2.1: 增强迁移工具 | 1 周 | 无 | 后端开发 |
| Task 2.2: 性能基准测试 | 2 周 | 无 | 后端开发 + DBA |
| Task 2.3: 自动化验证 | 1 周 | Task 2.1 | DevOps |
| Task 2.4: 完善文档 | 1 周 | Task 2.1, 2.2, 2.3 | 技术写作 |

**总计**：4-5 周（可并行执行 Task 2.1 和 2.2）

### 3.2 里程碑

| 里程碑 | 目标日期 | 关键交付物 |
|--------|---------|-----------|
| M1: 工具增强完成 | Week 1 | 增强版 migration_manager.sh |
| M2: 基准测试完成 | Week 3 | 性能基准测试体系 + 初始报告 |
| M3: 自动化验证完成 | Week 4 | CI 集成 + 验证脚本 |
| M4: 文档完善完成 | Week 5 | 完整运维手册 |

### 3.3 风险与缓解

| 风险 | 影响 | 概率 | 缓解措施 |
|------|------|------|---------|
| 基准测试数据生成耗时 | 中 | 中 | 使用较小数据集，支持增量扩展 |
| CI 集成影响现有流程 | 高 | 低 | 先在独立 workflow 测试 |
| 文档编写时间不足 | 低 | 中 | 优先核心文档，其他可后续补充 |

---

## 四、验收标准

### 4.1 功能验收

- ✅ 迁移工具能创建、测试、验证迁移
- ✅ 性能基准测试能生成可重复的报告
- ✅ CI 能自动验证 Schema 一致性和合约覆盖
- ✅ 运维手册完整且可操作

### 4.2 质量验收

- ✅ 所有脚本通过 shellcheck
- ✅ 所有 Python 脚本通过 pylint
- ✅ 所有文档通过拼写检查
- ✅ 所有改动通过 CI 验证

### 4.3 文档验收

- ✅ 每个工具有使用文档
- ✅ 每个流程有操作手册
- ✅ 文档包含实际示例
- ✅ 文档链接正确

---

## 五、后续维护

### 5.1 定期任务

- **每月**：运行性能基准测试，更新基准数据
- **每季度**：审查运维手册，更新最佳实践
- **每半年**：全面数据库审计

### 5.2 持续改进

- 根据实际使用反馈优化工具
- 根据性能数据调整基准阈值
- 根据故障案例更新运维手册

---

## 六、参考文档

- `docs/synapse-rust/DATABASE_AUDIT_SUMMARY_2026-04-04.md`
- `docs/db/DATABASE_AUDIT_REPORT_2026-04-04.md`
- `docs/db/CONSOLIDATION_PLAN.md`
- `migrations/README.md`
- `TESTING.md`

---

**文档版本**：v1.0  
**创建日期**：2026-04-04  
**下次审查**：2026-05-04
