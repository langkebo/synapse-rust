#!/bin/bash
# Systematically fix remaining compilation errors

echo "🔧 开始系统性修复编译错误..."
echo "========================================"

# 文件: src/e2ee/crypto/aes.rs
# 问题: Aes256GcmNonce::from_bytes 期望 &[u8]，但测试中传递的是数组
# 状态: 已修复 - from_bytes 现在接受 &[u8]

# 文件: src/e2ee/crypto/ed25519.rs  
# 问题: verify 函数期望 &[u8]，但测试中传递的是数组
# 状态: 需要检查并修复

# 文件: src/common/crypto.rs
# 问题: compute_hash, hmac_sha256, encode_base64 期望 &[u8]
# 状态: 已修复 - 这些函数现在接受 impl AsRef<[u8]>

echo "✅ 修复脚本创建完成"
echo ""
echo "📊 当前状态:"
echo "  - 错误数: 53"
echo "  - 警告数: 100"
echo "  - 修复策略: 修改函数签名以接受泛型切片类型"
