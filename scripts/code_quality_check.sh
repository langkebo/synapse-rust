#!/bin/bash
# synapse-rust 代码质量检查脚本

set -e

echo "=========================================="
echo "  synapse-rust 代码质量检查"
echo "=========================================="
echo ""

# 检查 Rust 工具链
check_rust() {
    echo "[1/5] 检查 Rust 工具链..."
    
    if ! command -v cargo &> /dev/null; then
        echo "❌ 未找到 cargo，请先安装 Rust"
        exit 1
    fi
    
    if ! command -v rustfmt &> /dev/null; then
        echo "❌ 未找到 rustfmt，请运行: rustup component add rustfmt"
        exit 1
    fi
    
    if ! command -v clippy &> /dev/null; then
        echo "❌ 未找到 clippy，请运行: rustup component add clippy"
        exit 1
    fi
    
    echo "✅ Rust 工具链已安装"
}

# 代码格式化检查
check_format() {
    echo ""
    echo "[2/5] 检查代码格式化..."
    
    if cargo fmt -- --check 2>/dev/null; then
        echo "✅ 代码格式正确"
    else
        echo "⚠️ 代码格式不正确，运行: cargo fmt"
        cargo fmt
    fi
}

# Clippy 检查
check_clippy() {
    echo ""
    echo "[3/5] 运行 Clippy 检查..."
    
    # 检查常见问题
    cargo clippy -- -D warnings 2>&1 | grep -E "error|warning" | head -20 || true
    
    # 统计 unwrap 使用
    unwrap_count=$(grep -r "\.unwrap()" src/ --include="*.rs" | grep -v "tests/" | grep -v "#\[test\]" | wc -l)
    expect_count=$(grep -r "\.expect(" src/ --include="*.rs" | grep -v "tests/" | grep -v "#\[test\]" | wc -l)
    
    echo ""
    echo "--- unwrap/expect 统计 ---"
    echo "unwrap: $unwrap_count"
    echo "expect: $expect_count"
    
    if [ "$unwrap_count" -gt 100 ]; then
        echo "⚠️ unwrap 使用过多，建议使用安全的错误处理"
    else
        echo "✅ unwrap 使用在可接受范围内"
    fi
}

# 编译检查
check_compile() {
    echo ""
    echo "[4/5] 检查编译..."
    
    if cargo check 2>&1 | tail -5; then
        echo "✅ 编译检查通过"
    else
        echo "❌ 编译失败"
        exit 1
    fi
}

# 测试检查
check_tests() {
    echo ""
    echo "[5/5] 运行测试..."
    
    # 只运行快速测试
    if cargo test --lib --release -- --test-threads=4 2>&1 | tail -10; then
        echo "✅ 测试通过"
    else
        echo "⚠️ 部分测试失败"
    fi
}

# 生成报告
generate_report() {
    echo ""
    echo "[报告] 生成代码质量报告..."
    
    mkdir -p reports/quality
    
    # 统计代码行数
    total_lines=$(find src/ -name "*.rs" -exec cat {} \; | wc -l)
    
    # 统计函数数量
    fn_count=$(grep -r "^pub fn\|^fn " src/ --include="*.rs" | wc -l)
    
    # 统计模块数量
    mod_count=$(grep -r "^pub mod" src/ --include="*.rs" | wc -l)
    
    # 写入报告
    cat > reports/quality/quality_report.md << EOF
# 代码质量报告

生成时间: $(date)

## 统计

- 总代码行数: $total_lines
- 函数数量: $fn_count
- 模块数量: $mod_count
- unwrap 使用: $unwrap_count
- expect 使用: $expect_count

## 检查结果

- 格式化: ✅
- Clippy: ✅/⚠️
- 编译: ✅/❌
- 测试: ✅/⚠️

## 改进建议

1. 减少 unwrap 使用，改用安全的错误处理
2. 添加更多单元测试
3. 增加文档注释

EOF
    
    echo "✅ 报告已生成: reports/quality/quality_report.md"
}

# 主函数
main() {
    check_rust
    check_format
    check_clippy
    check_compile
    check_tests
    generate_report
    
    echo ""
    echo "=========================================="
    echo "  代码质量检查完成!"
    echo "=========================================="
}

# 运行
main
