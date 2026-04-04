# Schema Validation Guide

> 日期：2026-04-04  
> 版本：v1.0  
> 工具：`scripts/validate_schema_all.sh`, `scripts/check_schema_contract_coverage.py`

---

## 一、概述

本指南说明如何使用自动化 Schema 验证工具来确保数据库迁移的正确性、完整性和一致性。

### 1.1 验证工具套件

| 工具 | 功能 | 用途 |
|------|------|------|
| `validate_schema_all.sh` | 综合验证脚本 | 运行所有验证检查 |
| `check_schema_table_coverage.py` | 表覆盖检查 | 验证所有引用的表都有定义 |
| `check_schema_contract_coverage.py` | 合约覆盖检查 | 验证关键表的列/索引/约束 |
| `audit_migration_layout.py` | 迁移布局审计 | 检查重复定义和冲突 |
| `verify_migration_manifest.py` | 清单完整性验证 | 验证迁移清单 |
| `run_pg_amcheck.py` | 物理完整性检查 | PostgreSQL 物理层检查 |
| `generate_logical_checksum_report.py` | 逻辑校验和 | 生成 Schema 校验和 |

### 1.2 验证层次

```
┌─────────────────────────────────────┐
│   静态验证（无需数据库）              │
│   - 表覆盖检查                       │
│   - 合约覆盖检查                     │
│   - 迁移布局审计                     │
│   - 清单完整性验证                   │
└─────────────────────────────────────┘
              ↓
┌─────────────────────────────────────┐
│   数据库验证（需要数据库连接）        │
│   - 连接性检查                       │
│   - 物理完整性检查 (pg_amcheck)      │
│   - 逻辑校验和生成                   │
└─────────────────────────────────────┘
```

---

## 二、快速开始

### 2.1 运行完整验证

```bash
# 运行所有验证检查
bash scripts/validate_schema_all.sh
```

输出示例：
```
========================================
  Schema Validation Suite
========================================

Running: Table Coverage
  Verify all expected tables are defined in migrations

✓ Table Coverage passed

Running: Contract Coverage
  Verify schema contracts meet 90% coverage threshold

✓ Contract Coverage passed

...

========================================
✓ All validation checks passed
========================================

Summary report: artifacts/schema_validation/validation_summary_20260404_120000.md
```

### 2.2 运行单个验证

```bash
# 表覆盖检查
python3 scripts/check_schema_table_coverage.py

# 合约覆盖检查（带阈值）
python3 scripts/check_schema_contract_coverage.py --threshold 90

# 生成详细报告
python3 scripts/check_schema_contract_coverage.py \
  --threshold 90 \
  --report artifacts/contract_coverage.md
```

---

## 三、Schema 合约覆盖检查

### 3.1 什么是 Schema 合约

Schema 合约定义了关键表必须具备的：
- **列（Columns）**：必需的字段
- **索引（Indexes）**：性能优化索引
- **约束（Constraints）**：数据完整性约束

### 3.2 合约定义

合约在 `scripts/check_schema_contract_coverage.py` 中定义：

```python
TABLE_CONTRACTS = {
    "room_summary_state": {
        "columns": ["room_id", "event_type", "state_key", "event_id", "content", "updated_ts"],
        "indexes": ["idx_room_summary_state_room"],
        "constraints": [
            "uq_room_summary_state_room_type_state",
            "fk_room_summary_state_room",
        ],
    },
    # ... 更多表合约
}
```

### 3.3 覆盖率计算

覆盖率 = (通过的检查数 / 总检查数) × 100%

每个表的检查包括：
1. 表是否存在（1 次检查）
2. 每个必需列是否存在（N 次检查）
3. 每个必需索引是否存在（M 次检查）
4. 每个必需约束是否存在（K 次检查）

### 3.4 阈值设置

默认阈值：**90%**

```bash
# 使用默认阈值（90%）
python3 scripts/check_schema_contract_coverage.py

# 自定义阈值
python3 scripts/check_schema_contract_coverage.py --threshold 95

# 宽松阈值（用于开发）
python3 scripts/check_schema_contract_coverage.py --threshold 80
```

### 3.5 生成详细报告

```bash
python3 scripts/check_schema_contract_coverage.py \
  --threshold 90 \
  --report artifacts/contract_coverage.md
```

报告包含：
- 覆盖率统计
- 失败的检查列表
- 每个表的合约详情

---

## 四、综合验证脚本

### 4.1 validate_schema_all.sh 功能

`scripts/validate_schema_all.sh` 按顺序运行所有验证：

1. **表覆盖检查**：验证所有引用的表都有定义
2. **合约覆盖检查**：验证关键表的完整性（90% 阈值）
3. **迁移布局审计**：检查重复定义和冲突
4. **迁移清单验证**：验证清单完整性（如果存在）
5. **数据库连接检查**：验证数据库可访问性（如果 DATABASE_URL 已设置）
6. **物理完整性检查**：运行 pg_amcheck（如果可用）
7. **逻辑校验和**：生成 Schema 校验和

### 4.2 环境变量

```bash
# 设置数据库连接（可选）
export DATABASE_URL="postgresql://synapse:synapse@localhost:5432/synapse_test"

# 运行验证
bash scripts/validate_schema_all.sh
```

如果未设置 `DATABASE_URL`，数据库相关检查将被跳过。

### 4.3 输出和报告

验证脚本生成以下报告：

```
artifacts/schema_validation/
├── validation_summary_20260404_120000.md  # 综合摘要
├── contract_coverage_20260404_120000.md   # 合约覆盖详情
└── logical_checksum_20260404_120000.md    # 逻辑校验和
```

---

## 五、CI/CD 集成

### 5.1 GitHub Actions 集成

#### db-migration-gate.yml

```yaml
schema-contract-coverage:
  name: Schema Contract Coverage
  runs-on: ubuntu-latest
  steps:
    - name: Checkout
      uses: actions/checkout@v4

    - name: Validate critical schema contracts with threshold
      run: |
        mkdir -p artifacts
        python3 scripts/check_schema_contract_coverage.py \
          --threshold 90 \
          --report artifacts/contract_coverage_report.md

    - name: Upload coverage report
      uses: actions/upload-artifact@v4
      if: always()
      with:
        name: contract-coverage-report
        path: artifacts/contract_coverage_report.md
```

#### drift-detection.yml

增强了漂移检测，生成详细的 Markdown 报告：

```yaml
- name: Run drift detection
  id: drift
  run: |
    python scripts/db/diff_schema.py \
      expected_schema.json \
      actual_schema.json \
      --format markdown \
      --output drift_report.md
    
    cat drift_report.md >> $GITHUB_STEP_SUMMARY
```

### 5.2 本地 CI 模拟

```bash
# 模拟 CI 环境运行验证
bash scripts/ci_backend_validation.sh
```

---

## 六、验证失败处理

### 6.1 表覆盖失败

**错误示例**：
```
Missing table definition: room_summary_state
```

**解决方法**：
1. 检查表是否在迁移中定义
2. 如果表已废弃，从引用中移除
3. 如果表是新增的，添加迁移脚本

### 6.2 合约覆盖失败

**错误示例**：
```
- room_summary_state: missing column 'updated_ts'
- room_summary_state: missing index 'idx_room_summary_state_room'
```

**解决方法**：
1. 检查迁移脚本是否定义了缺失的列/索引/约束
2. 如果定义存在但未被识别，检查命名是否匹配
3. 如果合约过时，更新 `TABLE_CONTRACTS` 定义

### 6.3 覆盖率低于阈值

**错误示例**：
```
❌ Coverage 85.5% is below threshold 90%
```

**解决方法**：
1. 查看详细报告找出失败的检查
2. 修复缺失的定义
3. 或者调整阈值（如果合理）

### 6.4 迁移布局冲突

**错误示例**：
```
Duplicate table definition: room_summary_state
  - migrations/20260330000003_align_retention_and_room_summary_schema.sql
  - migrations/20260404000001_consolidated_schema_alignment.sql
```

**解决方法**：
1. 确定哪个迁移是权威来源
2. 从其他迁移中移除重复定义
3. 或者使用 `CREATE TABLE IF NOT EXISTS` 实现幂等性

---

## 七、最佳实践

### 7.1 开发阶段

1. **本地验证**
   ```bash
   # 修改迁移后立即验证
   bash scripts/validate_schema_all.sh
   ```

2. **增量验证**
   ```bash
   # 只运行快速检查
   python3 scripts/check_schema_table_coverage.py
   python3 scripts/check_schema_contract_coverage.py
   ```

3. **生成报告**
   ```bash
   # 生成详细报告用于审查
   python3 scripts/check_schema_contract_coverage.py \
     --threshold 90 \
     --report review_report.md
   ```

### 7.2 PR 提交前

1. **完整验证**
   ```bash
   bash scripts/validate_schema_all.sh
   ```

2. **检查 CI 门禁**
   - 确保所有静态检查通过
   - 确保合约覆盖率 ≥ 90%
   - 确保无迁移布局冲突

3. **生成清单**
   ```bash
   python3 scripts/generate_migration_manifest.py \
     --release local \
     --jira SYS-XXXX \
     --owner your-name \
     --output artifacts/MANIFEST-local.txt
   ```

### 7.3 定期维护

1. **更新合约定义**
   - 当添加新的关键表时，更新 `TABLE_CONTRACTS`
   - 当表结构变化时，更新对应的合约

2. **审查覆盖率趋势**
   - 定期检查覆盖率是否下降
   - 调查覆盖率下降的原因

3. **清理过时定义**
   - 移除已废弃表的合约
   - 更新不再相关的约束

---

## 八、故障排查

### 8.1 常见问题

#### 问题 1：Python 脚本找不到模块

**错误**：
```
ModuleNotFoundError: No module named 'psycopg2'
```

**解决**：
```bash
pip install psycopg2-binary pg8000
```

#### 问题 2：数据库连接失败

**错误**：
```
Failed to connect to database
```

**解决**：
```bash
# 检查数据库是否运行
docker compose -f docker/docker-compose.yml ps db

# 启动数据库
docker compose -f docker/docker-compose.yml up -d db

# 验证连接
psql "$DATABASE_URL" -c "SELECT 1"
```

#### 问题 3：pg_amcheck 不可用

**警告**：
```
⚠ pg_amcheck not available, skipping
```

**解决**：
```bash
# Ubuntu/Debian
sudo apt-get install postgresql-contrib

# macOS
brew install postgresql
```

#### 问题 4：合约检查误报

**场景**：迁移中定义了列/索引，但检查仍然失败

**原因**：
- 命名不匹配（大小写、下划线）
- 定义在注释中或条件块中
- 使用了 ALTER TABLE 而非 CREATE TABLE

**解决**：
1. 检查实际定义的名称
2. 确保定义在主 SQL 语句中
3. 更新合约定义以匹配实际名称

---

## 九、扩展和自定义

### 9.1 添加新的合约

编辑 `scripts/check_schema_contract_coverage.py`：

```python
TABLE_CONTRACTS = {
    # ... 现有合约
    "your_new_table": {
        "columns": ["id", "name", "created_ts"],
        "indexes": ["idx_your_new_table_name"],
        "constraints": ["pk_your_new_table"],
    },
}
```

### 9.2 调整验证流程

编辑 `scripts/validate_schema_all.sh`：

```bash
# 添加自定义检查
run_check \
    "Custom Check" \
    "python3 '$SCRIPT_DIR/your_custom_check.py'" \
    "Your custom validation description"
```

### 9.3 自定义报告格式

修改 `generate_coverage_report()` 函数以自定义报告格式：

```python
def generate_coverage_report(passed, total, failures, output_path):
    # 自定义报告生成逻辑
    pass
```

---

## 十、参考资料

- [迁移工具指南](MIGRATION_TOOLS_GUIDE.md)
- [数据库审计报告](DATABASE_AUDIT_REPORT_2026-04-04.md)
- [P2 长期改进计划](../synapse-rust/P2_LONG_TERM_IMPROVEMENT_PLAN.md)
- [性能基准测试指南](../PERFORMANCE_BASELINE.md)

---

**文档版本**：v1.0  
**创建日期**：2026-04-04  
**维护者**：数据库团队
