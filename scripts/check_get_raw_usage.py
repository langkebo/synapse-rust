#!/usr/bin/env python3
"""S-8: 检测生产路径上的 `get_raw(...)` 使用（L1-only 读）。

用法:
    ./scripts/check_get_raw_usage.py

背景（AGENTS.md 的已知关键陷阱之一）：
- `get_raw()` 只读 L1 本地缓存，**无法跨实例共享**；跨实例正确性要求
  `get_raw_shared().await`。
- 因此除缓存 crate 自身实现、测试与明确豁免的路径外，生产代码不得使用 `get_raw`。

⚠️ 本门禁在 2026-09-19 之前是**假门禁**（门禁诚实性清查发现）：
- 正则写作 `\\bget_raw\\s*\\(\\s*\\)`，要求**空参数**；而真实 API 是
  `pub fn get_raw(&self, key: &str)`（`synapse-cache/src/manager.rs`），所有真实调用都带
  key ⇒ **结构上永不命中**，"0 违规"只是巧合。
- `is_allowed_path()` 的 glob 实现全坏（`**/tests/**`、`*/src/tests/*.rs` 实测恒 False），
  唯一真正生效的是字面量路径；且旧代码只要某行出现 `get_raw_shared()` 就整行豁免。
- 违规分支打印的是源文件里写成**孤立代理对**的 emoji（`"\\ud83d\\udea8"`），
  在 UTF-8 stdout 上抛 `UnicodeEncodeError`，看不到任何信息。

现在的判据（可红）：
- 正则 `\\bget_raw\\s*\\(`（**不**要求空参数；`get_raw_shared(` 因其后是 `_` 而非 `(` 天然不匹配）；
- 排除定义行（`fn get_raw`）；
- 按**路径谓词**豁免：测试文件、`benches/`、缓存 crate 自身实现、显式 CLI；
- `#[cfg(test)]` 块用**花括号配平**精确跳过（旧版是"一旦出现就把后续整文件都跳过"，
  而 431 个文件里有 `#[cfg(test)]` 出现在文件前部，会把生产代码一起放过）；
- 扫不到任何 `.rs` 文件即 fail-loud（空扫描不得算通过）。
"""

import os
import re
import sys
from pathlib import Path
from typing import Dict, List, Tuple

# 豁免规则（**路径谓词**，不是 glob 字符串 —— 旧版的 glob 实现实测全部失效）。
# 每条都写明理由；新增前请确认它真的不可能跨实例。
ALLOWED_DIR_PREFIXES = (
    # 缓存 crate 自身：`get_raw` 就是这里实现的 L1 访问器，
    # `get_raw_shared` 内部也要用它回填 L1。跨实例规则约束的是**其它 crate 的调用方**。
    "synapse-cache/src/",
    # 单进程 CLI：无并发实例。
    "synapse-web/src/bin/cli.rs",
)
ALLOWED_DIR_SEGMENTS = ("tests", "benches")


def is_allowed_path(rel_path: str) -> bool:
    """豁免判定：测试 / benches / 缓存 crate 实现 / 显式 CLI。"""
    normalized = rel_path.replace(os.sep, "/")
    for prefix in ALLOWED_DIR_PREFIXES:
        if normalized == prefix or normalized.startswith(prefix):
            return True
    parts = normalized.split("/")
    if any(segment in ALLOWED_DIR_SEGMENTS for segment in parts):
        return True
    name = parts[-1]
    # `tests.rs` / `*_tests.rs`（旧版的 `*/src/tests/*.rs` 与 `**/*_tests.rs` 都匹配不到它们）
    return name == "tests.rs" or name.endswith("_tests.rs")


def find_get_raw_usages(root_dir: str) -> Tuple[Dict[str, List[Tuple[int, str]]], int]:
    """
    递归扫描 Rust 源文件，找出生产路径上的 `get_raw(...)` 调用。

    Returns:
        (usages, scanned)：usages 映射文件路径到 [(行号, 代码)]；
        scanned 是实际读到的 .rs 文件数，供调用方做"空扫描"断言。
    """
    usages: Dict[str, List[Tuple[int, str]]] = {}
    root = Path(root_dir)
    scanned = 0

    # 匹配真实调用 `get_raw(...)`；`get_raw_shared(` 不会命中（其后是 `_` 而非 `(`）。
    pattern_get_raw = re.compile(r"\bget_raw\s*\(")
    pattern_test_cfg = re.compile(r"#\[cfg\s*\(\s*test\s*\)\s*\]")

    for rs_file in root.rglob("*.rs"):
        if "/target/" in str(rs_file):
            continue
        rel_path = str(rs_file.relative_to(root)).replace(os.sep, "/")
        try:
            with open(rs_file, "r", encoding="utf-8", errors="ignore") as f:
                lines = f.readlines()
        except IOError:
            continue
        scanned += 1

        # 精确跟踪 `#[cfg(test)]` 块：记住该 item 的块开始时的花括号深度，
        # 深度回落到该值即认为块结束。不再"一旦出现就跳过整个文件剩余部分"。
        depth = 0
        test_block_start_depth = None
        pending_cfg_test = False
        in_block_comment = False

        for line_num, line in enumerate(lines, 1):
            stripped = line.strip()

            # 注释里的 `get_raw` 提及不是调用（首版修好正则后就误报了
            # synapse-services/src/auth/token.rs 的两行说明性注释）。
            if in_block_comment:
                if "*/" in line:
                    in_block_comment = False
                continue
            if stripped.startswith("//"):
                continue
            if "/*" in line:
                in_block_comment = "*/" not in line.split("/*", 1)[1]
                continue

            if pattern_test_cfg.search(line):
                pending_cfg_test = True
            elif (
                pending_cfg_test
                and stripped
                and not stripped.startswith("#")
                and not stripped.startswith("mod ")
            ):
                # 属性后面不是 `mod …` 声明（例如 `#[cfg(test)] use …;`）：不构成需要跳过的
                # "测试模块块"，撤销 pending，避免误吞后续生产代码。
                pending_cfg_test = False

            opens = line.count("{")
            closes = line.count("}")
            if pending_cfg_test and opens:
                test_block_start_depth = depth
                pending_cfg_test = False
            depth += opens - closes
            if test_block_start_depth is not None:
                if depth <= test_block_start_depth:
                    test_block_start_depth = None
                else:
                    continue  # 位于 `#[cfg(test)]` 块内，跳过检查

            if pattern_get_raw.search(line):
                if "fn get_raw" in line:
                    continue  # 定义行，不是调用
                if "// ignore-getraw" in line.lower():
                    continue
                usages.setdefault(rel_path, []).append((line_num, stripped))

    return usages, scanned


def main() -> int:
    root_dir = Path(__file__).parent.parent.resolve()

    print("-" * 35)
    print("S-8: get_raw() 滥用检测")
    print(f"扫描目录：{root_dir}")
    print("-" * 35 + "\n")

    usages, scanned = find_get_raw_usages(str(root_dir))

    # 空扫描不得算通过：扫描根写错 / 源码树消失时会一个文件都读不到，
    # 旧版此时打印"未发现滥用"并 exit 0（同一类"空集即成功"缺陷）。
    if scanned == 0:
        print(
            "::error::scan found 0 .rs files — the scan root or the tree is wrong; refusing to "
            "report success.",
            file=sys.stderr,
        )
        return 2

    if not usages:
        print(f"OK 未发现 get_raw() 滥用（已扫描 {scanned} 个 .rs 文件）")
        return 0

    print(f"发现 {sum(len(v) for v in usages.values())} 处 get_raw() 调用\n")

    warnings = []
    allowed = []
    for file_path, occurrences in sorted(usages.items()):
        if is_allowed_path(file_path):
            allowed.append((file_path, len(occurrences)))
            continue
        for line_num, code in occurrences:
            warnings.append((file_path, line_num, code))

    if warnings:
        print("FAIL 警告：以下生产路径使用了 get_raw()")
        print("-" * 35)
        for file_path, line_num, code in warnings:
            print(f"{file_path}:{line_num}")
            print(f"  {code[:100]}{'...' if len(code) > 100 else ''}")
            print("  建议：改用 get_raw_shared().await 以确保跨实例缓存一致性")
            print()

    if allowed:
        print(f"OK 豁免：{len(allowed)} 个文件符合允许路径")
        for file_path, count in allowed:
            print(f"  - {file_path} ({count} 处)")
        print()

    print("-" * 35)
    print(f"扫描文件：{scanned} 个 .rs")
    print(f"总计：{sum(len(v) for v in usages.values())} 处 get_raw() 调用")
    print(f"警告：{len(warnings)} 处")
    print(f"豁免：{sum(v[1] for v in allowed)} 处")
    print("-" * 35)

    if warnings:
        # 旧版这里是孤立代理对 "\ud83d\udea8"，打印即 UnicodeEncodeError。
        print("\n!! 请在提交前修复上述问题，或在豁免规则中添加合理豁免（并写明理由）。")
        return 1

    print("通过")
    return 0


if __name__ == "__main__":
    sys.exit(main())
