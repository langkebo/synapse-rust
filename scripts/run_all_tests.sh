#!/bin/bash
# 运行完整测试套件脚本

cd /home/hula/synapse_rust

echo "================================================================================"
echo "运行完整测试套件"
echo "================================================================================"
echo "测试时间: $(date '+%Y-%m-%d %H:%M:%S')"
echo "服务器地址: http://localhost:8008"
echo ""

# 定义测试脚本列表
test_scripts=(
    "scripts/test_core_client_api.py"
    "scripts/test_admin_api.py"
    "scripts/test_federation_api.py"
    "scripts/test_e2e_encryption_api.py"
    "scripts/test_voice_message_api.py"
    "scripts/test_friend_system_api.py"
    "scripts/test_media_file_api.py"
    "scripts/test_private_chat_api.py"
    "scripts/test_key_backup_api.py"
    "scripts/test_authentication_error_handling.py"
)

# 定义测试结果文件
result_files=(
    "test_results.json"
    "admin_api_test_results.json"
    "federation_api_test_results.json"
    "e2e_encryption_api_test_results.json"
    "voice_message_api_test_results.json"
    "friend_system_api_test_results.json"
    "media_file_api_test_results.json"
    "private_chat_api_test_results.json"
    "key_backup_api_test_results.json"
    "authentication_error_handling_test_results.json"
)

# 运行所有测试
total_tests=0
total_passed=0
total_failed=0

for i in "${!test_scripts[@]}"; do
    echo "================================================================================"
    echo "运行测试: ${test_scripts[$i]}"
    echo "================================================================================"
    
    if [ -f "${test_scripts[$i]}" ]; then
        python3 "${test_scripts[$i]}"
        if [ $? -eq 0 ]; then
            echo "✅ 测试脚本执行成功"
        else
            echo "❌ 测试脚本执行失败"
        fi
    else
        echo "❌ 测试脚本不存在: ${test_scripts[$i]}"
    fi
    
    echo ""
done

# 汇总测试结果
echo "================================================================================"
echo "测试结果汇总"
echo "================================================================================"

# 统计所有测试结果
for result_file in "${result_files[@]}"; do
    if [ -f "$result_file" ]; then
        echo "分析测试结果: $result_file"
        
        # 使用Python解析JSON并统计
        python3 << EOF
import json
import sys

try:
    with open('$result_file', 'r') as f:
        results = json.load(f)
    
    passed = sum(1 for r in results if r.get('success', False))
    failed = sum(1 for r in results if not r.get('success', False))
    total = len(results)
    
    print(f"  总测试数: {total}")
    print(f"  通过数: {passed}")
    print(f"  失败数: {failed}")
    print(f"  成功率: {passed/total*100:.2f}%")
    print()
    
    sys.exit(0)
except Exception as e:
    print(f"  错误: {e}")
    sys.exit(1)
EOF
    fi
done

echo "================================================================================"
echo "完整测试套件运行完成"
echo "================================================================================"
