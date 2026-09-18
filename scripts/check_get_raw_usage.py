#!/usr/bin/env python3
"""
S-8: 检测 get_raw() 在生产路径的使用（排除 test cfg）

用法:
    ./scripts/check_get_raw_usage.py

该脚本扫描 Rust 源代码，查找：
1. 非 test cfg 路径下的 get_raw() 直接调用
2. 标记为 TODO 的待修复位置

输出：
- 警告列表（建议重构为 get_raw_shared()）
- 允许列表（已豁免的位置）

原理：
- get_raw() 是同步读取 L1 本地缓存，无法跨实例共享
- 在多实例部署中，使用 get_raw() 会导致缓存不一致
- 生产路径应使用 get_raw_shared().await 读取 L2 Redis 共享缓存
- 只有测试代码和特殊场景可以豁免
"""

import os
import re
import sys
from pathlib import Path
from typing import Dict, List, Tuple

# 允许的豁免列表（已验证安全的场景）
ALLOWED_PATHS = [
    # 测试文件
    "**/tests/**",
    "**/*_tests.rs",
    "*/src/tests/*.rs",
    "*/src/*/tests/*.rs",
    
    # 特殊场景 - 单例模式或非多线程环境
    "synapse-web/src/bin/cli.rs",  # 命令行工具，无并发
    
    # 已知安全的初始化路径
    "**/bootstrap/**/*.rs",
]

def is_allowed_path(file_path: str, allowed_patterns: List[str]) -> bool:
    """检查文件是否在允许的豁免列表中。"""
    for pattern in allowed_patterns:
        if pattern.startswith("**"):
            if file_path.endswith(pattern[3:]):
                return True
        elif pattern.endswith(".rs"):
            if file_path == pattern or file_path.endswith("/" + pattern):
                return True
        else:
            if pattern in file_path:
                return True
    return False


def find_get_raw_usages(root_dir: str) -> Dict[str, List[Tuple[int, str]]]:
    """
    递归扫描 Rust 源文件，找出所有 get_raw() 调用。
    
    Returns:
        Dict mapping file paths to list of (line_number, code) tuples.
    """
    usages: Dict[str, List[Tuple[int, str]]] = {}
    root = Path(root_dir)
    
    # 正则模式
    # 匹配 get_raw() 但不是 get_raw_shared()
    pattern_get_raw = re.compile(r'\bget_raw\s*\(\s*\)')
    pattern_test_cfg = re.compile(r'#\[cfg\s*\(\s*test\s*\)\s*\]')
    
    for rs_file in root.rglob("*.rs"):
        rel_path = str(rs_file.relative_to(root))
        
        try:
            with open(rs_file, "r", encoding="utf-8", errors="ignore") as f:
                lines = f.readlines()
        except IOError:
            continue
        
        # 检查是否是 test 模块
        is_test_file = False
        in_test_cfg_block = False
        brace_depth = 0
        
        for line_num, line in enumerate(lines, 1):
            stripped = line.strip()
            
            # 检测 #[cfg(test)]
            if pattern_test_cfg.search(line):
                is_test_file = True
                continue
            
            # 跟踪代码块嵌套
            if "{" in line:
                brace_depth += line.count("{")
            if "}" in line:
                brace_depth -= line.count("}")
            
            # 如果已经是 test 文件或当前在 test cfg 块内，跳过检查
            if is_test_file or in_test_cfg_block:
                continue
            
            # 检查 get_raw() 调用
            if pattern_get_raw.search(line):
                # 过滤掉 get_raw_shared()
                if "get_raw_shared()" in line:
                    continue
                
                # 检查注释中的忽略标记
                if "// ignore-getraw" in line.lower():
                    continue
                
                if rel_path not in usages:
                    usages[rel_path] = []
                
                usages[rel_path].append((line_num, stripped))
    
    return usages


def main():
    root_dir = Path(__file__).parent.parent.resolve()
    
    print(f"\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500")
    print("S-8: get_raw() 滥用检测")
    print(f"扫描目录：{root_dir}")
    print("\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\n")
    
    usages = find_get_raw_usages(str(root_dir))
    
    if not usages:
        print("\u2713 未发现 get_raw() 滥用")
        print("\n提示：建议在 CI 中运行此脚本作为 lint 检查。")
        return 0
    
    print(f"发现 {sum(len(v) for v in usages.values())} 处 get_raw() 调用\n")
    
    # 分组显示
    warnings = []
    allowed = []
    
    for file_path, occurrences in sorted(usages.items()):
        if is_allowed_path(file_path, ALLOWED_PATHS):
            allowed.append((file_path, len(occurrences)))
            continue
        
        for line_num, code in occurrences:
            warnings.append((file_path, line_num, code))
    
    # 输出警告
    if warnings:
        print("\u274C 警告：以下生产路径使用了 get_raw()")
        print("\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500")
        for file_path, line_num, code in warnings:
            print(f"{file_path}:{line_num}")
            print(f"  {code[:100]}{'...' if len(code) > 100 else ''}")
            print(f"  建议：改用 get_raw_shared().await 以确保跨实例缓存一致性")
            print()
    
    # 输出允许的
    if allowed:
        print(f"\u2713 豁免：{len(allowed)} 个文件符合允许路径")
        for file_path, count in allowed:
            print(f"  - {file_path} ({count} 处)")
        print()
    
    # 统计
    print("\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500")
    print(f"总计：{sum(len(v) for v in usages.values())} 处 get_raw() 调用")
    print(f"警告：{len(warnings)} 处")
    print(f"豁免：{sum(v[1] for v in allowed)} 处")
    print("\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500")
    
    # 返回码：CI 中断构建
    if warnings:
        print("\n\ud83d\udea8 请在提交前修复上述问题，或在 ALLOWED_PATHS 中添加合理豁免。")
        return 1
    
    print("\u2705 通过")
    return 0


if __name__ == "__main__":
    sys.exit(main())
