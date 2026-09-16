#!/usr/bin/env python3
"""
B2 棘轮：missing_docs 增量门禁。

仓库现状：7 个 crate 都开了 crate 级 `#![allow(missing_docs)]`，累积约
~1900 个 warning（每个 crate 200-500 不等）。如果直接打开 `-W missing_docs`，
CI 会立刻红一长串——长红 CI 等于无 CI。

棘轮策略：
  - 存量不动（保持 crate 级 `allow`）
  - 只卡**新增** `pub` 项：增量代码必须配 doc
  - 每修一点存量，必须收紧 baseline（防止 baseline 形同虚设）

具体规则：
  1. 新增文件（git diff --diff-filter=A 出来的 .rs）: 任何 pub item
     （pub fn / pub async fn / pub struct / pub enum / pub trait / pub const /
     pub static / pub type / pub mod）上方必须有 `///` doc 注释（同一文件内
     的前几行）。
  2. 修改文件（git diff --diff-filter=M）: 在 diff hunk 中**新增的 pub 项**也
     必须配 doc。
  3. baseline 记录当前 debt 数字（对每个 crate 单独记），debt 数字减少时 CI
     失败，强制收紧 baseline。

退出码：
  0: 通过（debt 未增）
  1: 失败（debt 增了 / 新增 pub 缺 doc）
  2: 通过（debt 减了，提示更新 baseline）
"""
from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent  # scripts/check_missing_docs_ratchet.py -> synapse-rust/
BASELINE_FILE = REPO_ROOT / "scripts" / ".missing-docs-baseline"

# pub item 正则：pub fn / pub async fn / pub struct / pub enum / pub trait /
# pub const / pub static / pub type / pub mod。注意 pub(crate) / pub(super)
# 不算 crate-public，按规则不算 doc 必需项（但本文脚本先严格按 pub 处理，
# 因为 crate-public 也对外暴露 trait bound）。
PUB_ITEM_RE = re.compile(
    r"^\s*pub\s+(?:async\s+)?(?:fn|struct|enum|trait|const|static|type|mod|union)\b"
)
DOC_LINE_RE = re.compile(r"^\s*///")
DOC_BLOCK_OPEN_RE = re.compile(r"^\s*/\*\*")


@dataclass
class Violation:
    file: str
    line: int
    pub_text: str
    reason: str


def run(cmd: list[str], cwd: Path | None = None) -> str:
    result = subprocess.run(cmd, cwd=cwd or REPO_ROOT, capture_output=True, text=True, check=False)
    if result.returncode != 0:
        print(f"::error::command failed: {' '.join(cmd)}\n{result.stderr}", file=sys.stderr)
        sys.exit(2)
    return result.stdout


def list_changed_rs_files(base_ref: str = "HEAD~1") -> tuple[list[Path], list[Path]]:
    """返回 (新增文件, 修改文件) 的 .rs 列表。"""
    out = run(["git", "diff", "--name-only", "--diff-filter=A", base_ref])
    added = [REPO_ROOT / p for p in out.splitlines() if p.endswith(".rs") and (REPO_ROOT / p).exists()]
    out2 = run(["git", "diff", "--name-only", "--diff-filter=M", base_ref])
    modified = [REPO_ROOT / p for p in out2.splitlines() if p.endswith(".rs") and (REPO_ROOT / p).exists()]
    return added, modified


def check_pub_has_doc(file: Path, pub_line: int, lines: list[str]) -> str | None:
    """检查第 pub_line 行的 pub 项上方是否有 /// doc。返回 None 表示通过；字符串表示失败原因。"""
    for prev in range(pub_line - 1, -1, -1):
        if pub_line - prev > 30:
            return f"no `///` doc found within 30 lines above line {pub_line + 1}"
        line = lines[prev]
        # 空行、属性行（#[...]）、模块级属性（#![...]）允许穿过
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        # pub fn 前是另一段代码（不是 doc）—— 失败
        # 但要注意 `pub fn` 上面可能紧跟 `///` 注释块（多行 doc）
        if DOC_LINE_RE.match(line):
            return None
        if DOC_BLOCK_OPEN_RE.match(line):
            return None
        # 找到非 doc、非属性、非空行 —— 上方没有 doc
        return f"no `///` doc above line {pub_line + 1} (got: {line.strip()[:60]!r})"
    return f"no `///` doc found above line {pub_line + 1}"


def scan_file_for_pub(file: Path) -> list[Violation]:
    """扫整个文件所有 pub 项，看是否都有 doc。仅用于新增文件。"""
    text = file.read_text()
    lines = text.splitlines()
    violations = []
    for i, line in enumerate(lines):
        if PUB_ITEM_RE.match(line):
            reason = check_pub_has_doc(file, i, lines)
            if reason:
                violations.append(Violation(str(file.relative_to(REPO_ROOT)), i + 1, line.strip(), reason))
    return violations


def scan_diff_for_new_pub(file: Path, base_ref: str) -> list[Violation]:
    """仅扫文件 diff 中新增行里的 pub 项（修改文件专用）。"""
    rel = file.relative_to(REPO_ROOT)
    out = run(["git", "diff", base_ref, "--", str(rel)])
    new_pub_lines: set[int] = set()
    cur_line = 0
    for raw in out.splitlines():
        if raw.startswith("@@"):
            m = re.search(r"\+(\d+)", raw)
            if m:
                cur_line = int(m.group(1)) - 1  # 0-indexed
            else:
                cur_line = 0
        elif raw.startswith("+") and not raw.startswith("++"):
            cur_line += 1
            if PUB_ITEM_RE.match(raw):
                new_pub_lines.add(cur_line)
        elif not raw.startswith("-"):
            cur_line += 1

    if not new_pub_lines:
        return []

    text = file.read_text()
    lines = text.splitlines()
    violations = []
    for i, line in enumerate(lines):
        if (i + 1) in new_pub_lines:
            reason = check_pub_has_doc(file, i, lines)
            if reason:
                violations.append(Violation(str(rel), i + 1, line.strip(), reason))
    return violations


def list_changed_rs_files(base_ref: str = "HEAD~1") -> tuple[list[Path], list[Path]]:
    """返回 (新增文件, 修改文件) 的 .rs 列表。"""
    out = run(["git", "diff", "--name-only", "--diff-filter=A", base_ref])
    added = [REPO_ROOT / p for p in out.splitlines() if p.endswith(".rs") and (REPO_ROOT / p).exists()]
    out2 = run(["git", "diff", "--name-only", "--diff-filter=M", base_ref])
    modified = [REPO_ROOT / p for p in out2.splitlines() if p.endswith(".rs") and (REPO_ROOT / p).exists()]
    return added, modified


def count_total_debt() -> int:
    """统计整个 workspace 缺 doc 的 pub 项总数。

    由于各 crate 用 `#![allow(missing_docs)]` 抑制了警告，本函数通过
    对每个 crate 临时加 `-A missing_docs -D missing_docs` 让编译器严格
    报告所有缺 doc 的项。需要 SQLX_OFFLINE。
    """
    env = os.environ.copy()
    env.setdefault("SQLX_OFFLINE", "true")
    total = 0
    for crate in [
        "synapse-cache",
        "synapse-common",
        "synapse-storage",
        "synapse-federation",
        "synapse-e2ee",
        "synapse-services",
        "synapse-web",
        "synapse-test-utils",
        "synapse-rust",
    ]:
        proc = subprocess.run(
            [
                "cargo",
                "clippy",
                "-p",
                crate,
                "--all-features",
                "--locked",
                "--message-format=short",
                "--",
                "-A",
                "missing_docs",  # 抵消 crate 级 allow
                "-D",
                "missing_docs",  # 强制报缺 doc
            ],
            cwd=REPO_ROOT,
            env=env,
            capture_output=True,
            text=True,
        )
        combined = (proc.stdout or "") + (proc.stderr or "")
        n = sum(1 for ln in combined.splitlines() if "missing documentation" in ln)
        total += n
    return total


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--update", action="store_true", help="重算并写入 baseline")
    parser.add_argument("--base", default="HEAD~1", help="对比的 base ref (默认 HEAD~1，方便本地测试)")
    parser.add_argument("--no-clippy", action="store_true", help="跳过 clippy debt 计数（仅做新增检查）")
    args = parser.parse_args()

    # 1. 增量文件扫描：新增文件（全量检查），修改文件（仅新增行中的 pub）
    added, modified = list_changed_rs_files(args.base)
    total = len(added) + len(modified)
    print(f"[1/2] scanning {len(added)} new + {len(modified)} modified .rs files vs {args.base}")

    violations: list[Violation] = []
    for f in added:
        violations.extend(scan_file_for_pub(f))
    for f in modified:
        violations.extend(scan_diff_for_new_pub(f, args.base))
    if violations:
        print(f"\n::error::新增 pub 项缺少 doc 注释 ({len(violations)} 处):", file=sys.stderr)
        for v in violations:
            print(f"  - {v.file}:{v.line}  {v.pub_text[:80]}", file=sys.stderr)
            print(f"      reason: {v.reason}", file=sys.stderr)
        return 1

    # 2. debt baseline 棘轮
    if args.no_clippy:
        print("[2/2] skipping clippy debt count (--no-clippy)")
        return 0

    print(f"[2/2] counting clippy missing_docs warnings across workspace...")
    current = count_total_debt()
    print(f"  current missing_docs debt: {current}")

    if args.update:
        BASELINE_FILE.parent.mkdir(parents=True, exist_ok=True)
        BASELINE_FILE.write_text(f"{current}\n")
        print(f"  baseline updated: {BASELINE_FILE}")
        return 0

    if not BASELINE_FILE.exists():
        print(f"\n::error::missing baseline file: {BASELINE_FILE}", file=sys.stderr)
        print(f"  Run '{sys.argv[0]} --update' to create it.", file=sys.stderr)
        return 1

    baseline = int(BASELINE_FILE.read_text().strip())
    print(f"  baseline: {baseline}")

    if current > baseline:
        print(f"\n::error::missing_docs debt increased: {current} > {baseline}", file=sys.stderr)
        return 1
    if current < baseline:
        print(f"\n::error::missing_docs debt decreased: {current} < {baseline}", file=sys.stderr)
        print(f"  Good — tighten the baseline:", file=sys.stderr)
        print(f"    {sys.argv[0]} --update", file=sys.stderr)
        return 1

    print(f"OK: missing_docs debt at baseline ({current}), no regression.")
    return 0


if __name__ == "__main__":
    sys.exit(main())