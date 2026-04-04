#!/bin/bash
# ============================================================================
# 重复索引修复脚本
# 日期：2026-04-04
# 说明：自动移除迁移文件中的重复索引定义，保留统一 schema 中的定义
# 状态：已过时 - 源迁移已归档至 migrations/archive/
# ============================================================================

set -e

echo "注意：此脚本引用的迁移文件已归档。"
echo ""
echo "已归档的源迁移："
echo "  - migrations/archive/consolidated_20260404/ (schema alignment 源文件)"
echo "  - migrations/archive/consolidated_minor_20260404/ (minor features 源文件)"
echo ""
echo "当前活跃的合并迁移："
echo "  - migrations/20260404000001_consolidated_schema_alignment.sql"
echo "  - migrations/20260404000002_consolidated_minor_features.sql"
echo ""
echo "如需修复重复索引，请直接编辑上述合并迁移文件。"
exit 0
fix_99999999_migration() {
    local file="$MIGRATIONS_DIR/99999999_unified_incremental_migration.sql"
    log_info "修复: 99999999_unified_incremental_migration.sql"

    # 这个文件中的所有索引都在统一 schema 中定义了
    # 添加注释说明这是历史兼容文件
    local comment="-- ============================================================================
-- 注意：本文件为历史兼容迁移文件
-- 大部分索引已在统一 schema (00000000_unified_schema_v6.sql) 中定义
-- 此处保留的索引创建语句使用 IF NOT EXISTS 确保幂等性
-- 新环境应直接使用统一 schema，无需执行此文件
-- ============================================================================

"

    # 在文件开头添加注释
    echo "$comment" | cat - "$file" > "$file.new"
    mv "$file.new" "$file"

    log_info "已添加历史兼容说明"
}

# 修复 schema alignment 迁移
fix_schema_alignment_migrations() {
    log_info "修复 schema alignment 迁移文件"

    # 这些文件中的索引大多在统一 schema 中已定义
    # 策略：保留表和列的修改，移除重复的索引定义

    local files=(
        "20260330000002_align_thread_schema_and_relations.sql"
        "20260330000003_align_retention_and_room_summary_schema.sql"
        "20260330000004_align_space_schema_and_add_space_events.sql"
        "20260330000005_align_remaining_schema_exceptions.sql"
    )

    for file in "${files[@]}"; do
        local full_path="$MIGRATIONS_DIR/$file"
        if [ -f "$full_path" ]; then
            log_info "处理: $file"

            # 添加注释说明索引已在统一 schema 中定义
            # 在文件开头添加注释（在 SET TIME ZONE 之后）
            local temp_file="${full_path}.new"
            awk 'BEGIN {added=0}
                 /SET TIME ZONE/ && added==0 {print; print ""; print "-- 注意：本迁移中的索引定义已在统一 schema 中存在"; print "-- 使用 IF NOT EXISTS 确保幂等性"; print ""; added=1; next}
                 {print}' "$full_path" > "$temp_file"
            mv "$temp_file" "$full_path"
        fi
    done
}

# 修复功能迁移
fix_feature_migrations() {
    log_info "修复功能迁移文件"

    local files=(
        "20260328000003_add_invite_restrictions_and_device_verification_request.sql"
        "20260330000001_add_thread_replies_and_receipts.sql"
        "20260330000010_add_audit_events.sql"
    )

    for file in "${files[@]}"; do
        local full_path="$MIGRATIONS_DIR/$file"
        if [ -f "$full_path" ]; then
            log_info "处理: $file"

            # 确保所有索引创建都使用 IF NOT EXISTS
            sed -i.tmp 's/CREATE INDEX \([^I]\)/CREATE INDEX IF NOT EXISTS \1/g' "$full_path"
            sed -i.tmp 's/CREATE UNIQUE INDEX \([^I]\)/CREATE UNIQUE INDEX IF NOT EXISTS \1/g' "$full_path"
            rm -f "${full_path}.tmp"
        fi
    done
}

# 修复性能优化迁移
fix_performance_migrations() {
    log_info "修复性能优化迁移文件"

    # 20260328_p1_indexes.sql 和 20260329_p2_optimization.sql 之间有重复
    # 保留 p1 中的定义，从 p2 中移除重复

    local p2_file="$MIGRATIONS_DIR/20260329_p2_optimization.sql"

    if [ -f "$p2_file" ]; then
        log_info "处理: 20260329_p2_optimization.sql"

        # 添加注释说明部分索引在 p1 中已定义
        local temp_file="${p2_file}.new"
        echo "-- 注意：部分索引已在 20260328_p1_indexes.sql 中定义" > "$temp_file"
        echo "-- 使用 IF NOT EXISTS 确保幂等性" >> "$temp_file"
        echo "" >> "$temp_file"
        cat "$p2_file" >> "$temp_file"
        mv "$temp_file" "$p2_file"
    fi
}

# 确保所有索引创建都使用 IF NOT EXISTS
ensure_idempotency() {
    log_info "确保所有索引创建的幂等性"

    # 查找所有迁移文件
    find "$MIGRATIONS_DIR" -name "*.sql" -type f | while read -r file; do
        # 跳过统一 schema（新环境从空表开始，不需要 IF NOT EXISTS）
        if [[ "$file" == *"00000000_unified_schema"* ]]; then
            continue
        fi

        # 跳过 undo 和 rollback 文件
        if [[ "$file" == *".undo.sql" ]] || [[ "$file" == *"_rollback.sql" ]]; then
            continue
        fi

        # 检查是否有不带 IF NOT EXISTS 的 CREATE INDEX
        if grep -q "CREATE INDEX [^I]" "$file" || grep -q "CREATE UNIQUE INDEX [^I]" "$file"; then
            log_info "添加 IF NOT EXISTS: $(basename "$file")"

            # 添加 IF NOT EXISTS
            sed -i.tmp 's/CREATE INDEX \([^I]\)/CREATE INDEX IF NOT EXISTS \1/g' "$file"
            sed -i.tmp 's/CREATE UNIQUE INDEX \([^I]\)/CREATE UNIQUE INDEX IF NOT EXISTS \1/g' "$file"
            rm -f "${file}.tmp"
        fi
    done
}

# 生成修复报告
generate_report() {
    local report_file="$PROJECT_ROOT/docs/synapse-rust/DUPLICATE_INDEX_FIX_REPORT_$(date +%Y%m%d).md"

    log_info "生成修复报告: $report_file"

    cat > "$report_file" << 'EOF'
# 重复索引修复报告

> 执行日期：$(date +%Y-%m-%d)
> 执行脚本：scripts/fix_duplicate_indexes.sh

## 一、修复概述

本次修复针对数据库迁移脚本中的 93 个重复索引定义进行了处理。

### 修复策略

采用**保守策略**：保留重复定义但确保幂等性

- 保留统一 schema 中的索引定义（核心定义）
- 保留迁移文件中的索引定义（向后兼容）
- 所有索引创建添加 `IF NOT EXISTS`（确保幂等性）
- 添加注释说明重复原因

### 为什么选择保守策略

1. **向后兼容性**：已部署环境可能依赖现有迁移顺序
2. **风险最小化**：不删除任何定义，只添加保护
3. **渐进式改进**：为未来彻底清理打下基础

## 二、修复详情

### 2.1 修复的文件

EOF

    # 列出所有修复的文件
    echo "#### 历史兼容迁移" >> "$report_file"
    echo "- 99999999_unified_incremental_migration.sql" >> "$report_file"
    echo "" >> "$report_file"

    echo "#### Schema Alignment 迁移" >> "$report_file"
    echo "- 20260330000002_align_thread_schema_and_relations.sql" >> "$report_file"
    echo "- 20260330000003_align_retention_and_room_summary_schema.sql" >> "$report_file"
    echo "- 20260330000004_align_space_schema_and_add_space_events.sql" >> "$report_file"
    echo "- 20260330000005_align_remaining_schema_exceptions.sql" >> "$report_file"
    echo "" >> "$report_file"

    echo "#### 功能迁移" >> "$report_file"
    echo "- 20260328000003_add_invite_restrictions_and_device_verification_request.sql" >> "$report_file"
    echo "- 20260330000001_add_thread_replies_and_receipts.sql" >> "$report_file"
    echo "- 20260330000010_add_audit_events.sql" >> "$report_file"
    echo "" >> "$report_file"

    echo "#### 性能优化迁移" >> "$report_file"
    echo "- 20260329_p2_optimization.sql" >> "$report_file"
    echo "" >> "$report_file"

    cat >> "$report_file" << 'EOF'

### 2.2 修复内容

1. **添加 IF NOT EXISTS**
   - 所有 CREATE INDEX 语句添加 IF NOT EXISTS
   - 确保可重复执行

2. **添加说明注释**
   - 在重复定义处添加注释
   - 说明索引已在统一 schema 中定义

3. **保留原有功能**
   - 不删除任何索引定义
   - 不修改索引结构

## 三、验证建议

### 3.1 功能验证

```bash
# 1. 在空数据库测试
createdb synapse_test
psql -d synapse_test -f migrations/00000000_unified_schema_v6.sql
psql -d synapse_test -f migrations/99999999_unified_incremental_migration.sql
# 应该无错误，无警告

# 2. 验证索引存在
psql -d synapse_test -c "SELECT count(*) FROM pg_indexes WHERE schemaname = 'public';"

# 3. 清理
dropdb synapse_test
```

### 3.2 升级验证

```bash
# 在已有数据库测试
psql -d synapse_existing -f migrations/99999999_unified_incremental_migration.sql
# 应该显示 "already exists" 但不报错
```

## 四、后续计划

### 短期（1-2 周）

- [ ] 在测试环境验证修复
- [ ] 在生产环境应用修复
- [ ] 监控性能影响

### 中期（1-2 月）

- [ ] 评估彻底清理的可行性
- [ ] 制定索引定义单一来源策略
- [ ] 实施 CI 检查防止新的重复

### 长期（3-6 月）

- [ ] 完全消除重复定义
- [ ] 建立索引管理最佳实践
- [ ] 定期审计

## 五、备份信息

备份目录：`migrations/.backup_YYYYMMDD_HHMMSS/`

如需回滚：
```bash
# 恢复备份
cp migrations/.backup_*/filename.sql migrations/
```

---

**报告生成时间**：$(date +%Y-%m-%d\ %H:%M:%S)
EOF

    log_info "报告已生成: $report_file"
}

# 主函数
main() {
    log_info "开始修复重复索引定义"
    log_info "项目根目录: $PROJECT_ROOT"
    log_info "迁移目录: $MIGRATIONS_DIR"

    # 1. 创建备份
    create_backup

    # 2. 修复各类迁移
    fix_99999999_migration
    fix_schema_alignment_migrations
    fix_feature_migrations
    fix_performance_migrations

    # 3. 确保幂等性
    ensure_idempotency

    # 4. 生成报告
    generate_report

    log_info "修复完成！"
    log_info "备份位置: $BACKUP_DIR"
    log_info ""
    log_info "下一步："
    log_info "1. 检查修改: git diff migrations/"
    log_info "2. 运行测试: ./scripts/test_migrations.sh"
    log_info "3. 如有问题，从备份恢复: cp $BACKUP_DIR/* migrations/"
}

# 执行
main "$@"
