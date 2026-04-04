#!/bin/bash
# ============================================================================
# 索引并发创建转换脚本
# 日期：2026-04-04
# 说明：将迁移文件中的索引创建改为 CONCURRENTLY 方式
# ============================================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MIGRATIONS_DIR="$PROJECT_ROOT/migrations"
BACKUP_DIR="$PROJECT_ROOT/migrations/.backup_concurrent_$(date +%Y%m%d_%H%M%S)"

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 日志函数
log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_debug() {
    echo -e "${BLUE}[DEBUG]${NC} $1"
}

# 创建备份
create_backup() {
    log_info "创建备份目录: $BACKUP_DIR"
    mkdir -p "$BACKUP_DIR"

    # 备份所有迁移文件（排除统一 schema）
    find "$MIGRATIONS_DIR" -name "*.sql" -type f | while read -r file; do
        # 跳过统一 schema（新环境从空表开始，不需要 CONCURRENTLY）
        if [[ "$file" == *"00000000_unified_schema"* ]]; then
            continue
        fi

        # 跳过 undo 和 rollback 文件
        if [[ "$file" == *".undo.sql" ]] || [[ "$file" == *"_rollback.sql" ]]; then
            continue
        fi

        # 检查是否包含 CREATE INDEX
        if grep -q "CREATE.*INDEX" "$file"; then
            local basename=$(basename "$file")
            cp "$file" "$BACKUP_DIR/"
            log_debug "已备份: $basename"
        fi
    done

    log_info "备份完成"
}

# 转换单个文件
convert_file() {
    local file="$1"
    local basename=$(basename "$file")

    log_info "处理: $basename"

    # 统计转换前的索引数量
    local before_count=$(grep -c "CREATE.*INDEX" "$file" || true)

    # 转换策略：
    # 1. CREATE INDEX -> CREATE INDEX CONCURRENTLY
    # 2. CREATE UNIQUE INDEX -> CREATE UNIQUE INDEX CONCURRENTLY
    # 3. 保留已有的 CONCURRENTLY
    # 4. 确保有 IF NOT EXISTS

    # 创建临时文件
    local temp_file="${file}.concurrent_tmp"

    # 使用 sed 进行转换
    sed -E \
        -e 's/CREATE INDEX IF NOT EXISTS/CREATE INDEX CONCURRENTLY IF NOT EXISTS/g' \
        -e 's/CREATE UNIQUE INDEX IF NOT EXISTS/CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS/g' \
        -e 's/CREATE INDEX ([^CI])/CREATE INDEX CONCURRENTLY IF NOT EXISTS \1/g' \
        -e 's/CREATE UNIQUE INDEX ([^CI])/CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS \1/g' \
        "$file" > "$temp_file"

    # 清理重复的 CONCURRENTLY
    sed -i.bak 's/CONCURRENTLY CONCURRENTLY/CONCURRENTLY/g' "$temp_file"
    rm -f "${temp_file}.bak"

    # 统计转换后的索引数量
    local after_count=$(grep -c "CREATE.*INDEX.*CONCURRENTLY" "$temp_file" || true)

    # 替换原文件
    mv "$temp_file" "$file"

    log_info "  转换前: $before_count 个索引"
    log_info "  转换后: $after_count 个并发索引"
}

# 转换所有迁移文件
convert_all_migrations() {
    log_info "开始转换索引创建为并发模式"

    local total_files=0
    local converted_files=0

    # 查找所有需要转换的迁移文件
    find "$MIGRATIONS_DIR" -name "*.sql" -type f | while read -r file; do
        # 跳过统一 schema
        if [[ "$file" == *"00000000_unified_schema"* ]]; then
            continue
        fi

        # 跳过 undo 和 rollback 文件
        if [[ "$file" == *".undo.sql" ]] || [[ "$file" == *"_rollback.sql" ]]; then
            continue
        fi

        # 跳过归档目录（可选）
        if [[ "$file" == *"/archive/"* ]]; then
            log_debug "跳过归档文件: $(basename "$file")"
            continue
        fi

        # 检查是否包含 CREATE INDEX
        if grep -q "CREATE.*INDEX" "$file"; then
            ((total_files++))
            convert_file "$file"
            ((converted_files++))
        fi
    done

    log_info "转换完成: 处理了 $converted_files 个文件"
}

# 验证转换结果
verify_conversion() {
    log_info "验证转换结果"

    local issues=0

    # 检查是否还有非并发的索引创建
    find "$MIGRATIONS_DIR" -name "*.sql" -type f | while read -r file; do
        # 跳过统一 schema
        if [[ "$file" == *"00000000_unified_schema"* ]]; then
            continue
        fi

        # 跳过 undo 和 rollback 文件
        if [[ "$file" == *".undo.sql" ]] || [[ "$file" == *"_rollback.sql" ]]; then
            continue
        fi

        # 跳过归档目录
        if [[ "$file" == *"/archive/"* ]]; then
            continue
        fi

        # 查找没有 CONCURRENTLY 的 CREATE INDEX
        if grep -E "CREATE (UNIQUE )?INDEX [^C]" "$file" | grep -v "CONCURRENTLY" > /dev/null 2>&1; then
            log_warn "发现非并发索引: $(basename "$file")"
            grep -n "CREATE.*INDEX" "$file" | grep -v "CONCURRENTLY"
            ((issues++))
        fi
    done

    if [ $issues -eq 0 ]; then
        log_info "✓ 验证通过：所有索引都使用 CONCURRENTLY"
    else
        log_warn "⚠ 发现 $issues 个文件仍有非并发索引"
    fi
}

# 生成统计报告
generate_statistics() {
    log_info "生成统计报告"

    local report_file="$PROJECT_ROOT/docs/synapse-rust/INDEX_CONCURRENCY_REPORT_$(date +%Y%m%d).md"

    cat > "$report_file" << EOF
# 索引并发创建转换报告

> 执行日期：$(date +%Y-%m-%d)
> 执行脚本：scripts/convert_indexes_to_concurrent.sh

## 一、转换概述

本次转换将所有迁移文件中的索引创建改为 CONCURRENTLY 模式，避免在生产环境中锁表。

### 转换策略

- **统一 schema**：不转换（新环境从空表开始，无需 CONCURRENTLY）
- **迁移文件**：全部转换为 CONCURRENTLY
- **归档文件**：跳过（不再使用）

### 转换规则

\`\`\`sql
-- 转换前
CREATE INDEX idx_name ON table(column);

-- 转换后
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_name ON table(column);
\`\`\`

## 二、转换统计

### 2.1 文件统计

EOF

    # 统计各类文件
    local total_migrations=$(find "$MIGRATIONS_DIR" -name "*.sql" -type f | grep -v "00000000_unified_schema" | grep -v ".undo.sql" | grep -v "_rollback.sql" | grep -v "/archive/" | wc -l)
    local files_with_indexes=$(find "$MIGRATIONS_DIR" -name "*.sql" -type f | grep -v "00000000_unified_schema" | grep -v ".undo.sql" | grep -v "_rollback.sql" | grep -v "/archive/" | xargs grep -l "CREATE.*INDEX" | wc -l)
    local concurrent_indexes=$(find "$MIGRATIONS_DIR" -name "*.sql" -type f | grep -v "00000000_unified_schema" | grep -v ".undo.sql" | grep -v "_rollback.sql" | grep -v "/archive/" | xargs grep -c "CONCURRENTLY" | awk '{sum+=$1} END {print sum}')

    cat >> "$report_file" << EOF
- 迁移文件总数：$total_migrations
- 包含索引的文件：$files_with_indexes
- 并发索引总数：$concurrent_indexes

### 2.2 转换详情

EOF

    # 列出所有转换的文件
    find "$MIGRATIONS_DIR" -name "*.sql" -type f | grep -v "00000000_unified_schema" | grep -v ".undo.sql" | grep -v "_rollback.sql" | grep -v "/archive/" | while read -r file; do
        if grep -q "CREATE.*INDEX.*CONCURRENTLY" "$file"; then
            local basename=$(basename "$file")
            local count=$(grep -c "CREATE.*INDEX.*CONCURRENTLY" "$file" || true)
            echo "- $basename: $count 个并发索引" >> "$report_file"
        fi
    done

    cat >> "$report_file" << EOF

## 三、性能影响

### 3.1 优势

1. **避免锁表**：CONCURRENTLY 模式不会阻塞表的读写操作
2. **生产友好**：可以在生产环境安全执行
3. **降低风险**：减少部署时的停机时间

### 3.2 注意事项

1. **执行时间更长**：CONCURRENTLY 模式比普通模式慢 2-3 倍
2. **需要更多资源**：占用更多 CPU 和内存
3. **不能在事务中**：CONCURRENTLY 不能在事务块中执行

### 3.3 建议

- 在维护窗口执行大表的索引创建
- 监控索引创建进度
- 预留足够的执行时间

## 四、验证建议

### 4.1 语法验证

\`\`\`bash
# 检查 SQL 语法
for f in migrations/*.sql; do
    psql -d postgres -c "\\i \$f" --single-transaction --set ON_ERROR_STOP=on
done
\`\`\`

### 4.2 功能验证

\`\`\`bash
# 在测试数据库验证
createdb synapse_test
psql -d synapse_test -f migrations/00000000_unified_schema_v6.sql

# 执行迁移
for f in migrations/202*.sql; do
    echo "执行: \$f"
    psql -d synapse_test -f "\$f"
done

# 验证索引
psql -d synapse_test -c "SELECT schemaname, tablename, indexname FROM pg_indexes WHERE schemaname = 'public' ORDER BY tablename, indexname;"

# 清理
dropdb synapse_test
\`\`\`

## 五、回滚方案

如需回滚：

\`\`\`bash
# 恢复备份
cp migrations/.backup_concurrent_*/* migrations/

# 验证
git diff migrations/
\`\`\`

## 六、后续行动

- [ ] 在测试环境验证
- [ ] 监控索引创建性能
- [ ] 更新部署文档
- [ ] 培训运维团队

---

**报告生成时间**：$(date +%Y-%m-%d\ %H:%M:%S)
**备份位置**：$BACKUP_DIR
EOF

    log_info "报告已生成: $report_file"
}

# 主函数
main() {
    log_info "=========================================="
    log_info "索引并发创建转换脚本"
    log_info "=========================================="
    log_info "项目根目录: $PROJECT_ROOT"
    log_info "迁移目录: $MIGRATIONS_DIR"
    log_info ""

    # 1. 创建备份
    create_backup

    # 2. 转换所有迁移
    convert_all_migrations

    # 3. 验证转换结果
    verify_conversion

    # 4. 生成统计报告
    generate_statistics

    log_info ""
    log_info "=========================================="
    log_info "转换完成！"
    log_info "=========================================="
    log_info "备份位置: $BACKUP_DIR"
    log_info ""
    log_info "下一步："
    log_info "1. 检查修改: git diff migrations/"
    log_info "2. 验证语法: psql -d postgres -f migrations/xxx.sql --dry-run"
    log_info "3. 测试执行: 在测试数据库运行迁移"
    log_info "4. 如有问题，从备份恢复: cp $BACKUP_DIR/* migrations/"
}

# 执行
main "$@"
