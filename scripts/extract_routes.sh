#!/bin/bash
# API 契约文档更新辅助脚本
# 用途: 从 synapse-rust 后端代码中提取路由信息

set -e

BACKEND_DIR="/Users/ljf/Desktop/hu/synapse-rust"
OUTPUT_DIR="/Users/ljf/Desktop/hu/matrix-js-sdk/docs/api-contract"

echo "=== API 契约文档更新辅助工具 ==="
echo ""

# 1. 提取所有路由定义
echo "1. 提取路由定义..."
echo ""

# 提取 assembly.rs 中的路由
echo "## 主路由装配 (assembly.rs)" > /tmp/routes_assembly.txt
grep -n "\.route\|\.merge\|\.nest" "$BACKEND_DIR/src/web/routes/assembly.rs" | head -100 >> /tmp/routes_assembly.txt

# 提取各个路由模块
for module in auth account device dm e2ee federation friend key_backup media presence push rendezvous room space sync thread verification voice widget; do
    if [ -f "$BACKEND_DIR/src/web/routes/${module}.rs" ]; then
        echo "提取 ${module}.rs 路由..."
        grep -n "\.route\|Router::new" "$BACKEND_DIR/src/web/routes/${module}.rs" | head -50 > "/tmp/routes_${module}.txt" 2>/dev/null || true
    fi
done

# 提取 admin 路由
if [ -f "$BACKEND_DIR/src/web/routes/admin/mod.rs" ]; then
    echo "提取 admin/mod.rs 路由..."
    grep -n "\.route\|\.merge" "$BACKEND_DIR/src/web/routes/admin/mod.rs" | head -100 > /tmp/routes_admin.txt
fi

echo ""
echo "2. 生成路由清单..."
echo ""

# 生成汇总报告
cat > /tmp/route_summary.md << 'EOF'
# 后端路由提取报告

> 生成时间: $(date +"%Y-%m-%d %H:%M:%S")
> 后端目录: /Users/ljf/Desktop/hu/synapse-rust

## 提取的路由模块

EOF

for file in /tmp/routes_*.txt; do
    if [ -f "$file" ]; then
        module=$(basename "$file" .txt | sed 's/routes_//')
        echo "- $module" >> /tmp/route_summary.md
    fi
done

echo ""
echo "3. 路由提取完成！"
echo ""
echo "提取的文件位于 /tmp/routes_*.txt"
echo "汇总报告位于 /tmp/route_summary.md"
echo ""
echo "下一步:"
echo "1. 查看 /tmp/routes_*.txt 了解各模块的路由"
echo "2. 根据路由信息更新对应的契约文档"
echo "3. 使用 grep 查找具体的处理器实现"
echo ""
echo "示例命令:"
echo "  cat /tmp/routes_auth.txt"
echo "  grep -n 'async fn login' $BACKEND_DIR/src/web/routes/auth.rs"
echo ""
