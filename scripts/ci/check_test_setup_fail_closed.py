#!/usr/bin/env python3
"""R1 守卫：测试夹具（setup/fixture）不得 fail-open —— 缺库必须 panic，不许返回 `None`。

## 为什么守这条

`docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md` §2 的根因不是"少写了测试"，而是
**夹具把基础设施故障折叠成"跳过"**：夹具 `prepare_shared_test_pool().await.ok()?` 返回
`None`，调用点清一色是 `let Some(..) else { return; };`，libtest 记 `ok` —— 一条断言都不跑。

实测代价：DB 不可达时语音上传体积上限 4 条、媒体 2 MiB 边界 3 条、worker surface gate
2 条**全部 PASS**（其中一条耗时 30 s，比真跑的 6 s 还慢，所以"跑得快 = 没跑"的巡检看不见它）。
其中包含 CVE-2026-102592 的 `Content-Type` 大小写绕过项。

## 判定口径（守根源，不守调用点）

* 扫 `tests/**/*.rs` 中**返回 `Option<...>`** 的函数；
* 只对**名字表明是测试基础设施**的函数生效
  （`setup*` / `prepare*` / `connect_*` / `*_pool` / `*database*` / `*fixture*` /
  `*context*` / `test_app*` / `enabled_app`）—— 业务数据的 `Option` 解构（`room_encryption`、
  `read_or_write` 之类）**不在**本判据内，它们返回 `None` 是语义；
* 这些函数体内出现 `.ok()?` 或裸 `return None` ⇒ 红。

为什么不守那 559 个调用点：调用点改成 `skip_or_fail_without_db()` 只是把炸弹搬到别处；
只要还有 fail-open 的夹具，任何新调用点都会重新引入假绿。守夹具本体是**一处改动、全局生效**。

## 白名单

* `tests/performance/manual_smoke_tests.rs`：手工冒烟，全部 `#[ignore]`，设计上就允许"无库不跑"
  （`analyze_skipped_tests.py` 把它们登记为 `design_intended`）；
* `scripts/ci/test_setup_fail_closed_allowlist`：逐行 `路径  理由`，新增必须给出理由。
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ALLOWLIST = ROOT / "scripts/ci/test_setup_fail_closed_allowlist"

# 名字表明"这是夹具/基础设施"的前缀/片段。业务 `Option` 解构不应命中。
FIXTURE_NAME = re.compile(
    r"(?:^|_)(?:setup|prepare|fixture)(?:_|$)"
    r"|^connect_"
    r"|_pool$"
    r"|database"
    r"|context"
    r"|test_app"
    r"|enabled_app",
    re.IGNORECASE,
)
FAIL_OPEN = (".ok()?",)
RETURN_NONE = re.compile(r"^\s*return\s+None\s*;", re.MULTILINE)
FN_DEF = re.compile(r"^(\s*)(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)")
SIG_LOOKAHEAD = 4


def load_allowlist() -> dict[str, str]:
    if not ALLOWLIST.is_file():
        return {}
    allowed: dict[str, str] = {}
    for raw in ALLOWLIST.read_text().splitlines():
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        parts = line.split(None, 1)
        allowed[parts[0]] = parts[1] if len(parts) > 1 else ""
    return allowed


def strip_line_comments(text: str) -> str:
    """去掉整行 `//` 注释。

    必须去：修好一处后，注释里往往仍留着"此前是 `.ok()?`"的说明（本仓库的习惯是写清
    历史原因），不剥就会把自己刚修好的函数再报一遍 —— 首次运行就踩到（`mod.rs:650`）。
    只剥**整行**注释，不动行内 `//`（避免误伤 `"http://…"` 之类的字符串字面量）。
    """
    return "\n".join(line for line in text.split("\n") if not line.lstrip().startswith("//"))


def function_body(lines: list[str], start: int, indent: int) -> str:
    """从函数签名行往下取函数体（到下一个同级 `fn` 或文件尾）。"""
    body: list[str] = []
    for index in range(start + 1, min(start + 400, len(lines))):
        current = lines[index]
        if current.strip() and len(current) - len(current.lstrip()) <= indent and FN_DEF.match(current):
            break
        body.append(current)
    return strip_line_comments("\n".join(body))


def main() -> int:
    allowlist = load_allowlist()
    violations: list[tuple[str, int, str, str]] = []
    scanned = 0

    for path in sorted((ROOT / "tests").rglob("*.rs")):
        rel = path.relative_to(ROOT).as_posix()
        if rel in allowlist:
            continue
        lines = path.read_text(errors="ignore").split("\n")
        for index, line in enumerate(lines):
            match = FN_DEF.match(line)
            if not match:
                continue
            signature = "\n".join(lines[index : index + SIG_LOOKAHEAD])
            if not re.search(r"->\s*Option\s*<", signature):
                continue
            name = match.group(2)
            if not FIXTURE_NAME.search(name):
                continue
            scanned += 1
            body = function_body(lines, index, len(match.group(1)))
            reasons = [token for token in FAIL_OPEN if token in body]
            if RETURN_NONE.search(body):
                reasons.append("return None")
            if reasons:
                violations.append((rel, index + 1, name, " / ".join(reasons)))

    print(f"扫描 tests/**/*.rs：夹具型 `-> Option<…>` 函数 {scanned} 个；白名单 {len(allowlist)} 项")
    if violations:
        print(f"\nFAIL fail-open 夹具 {len(violations)} 处（缺库时会把空跑记成 ok）：")
        for rel, line, name, reason in violations:
            print(f"  {rel}:{line}  {name}  [{reason}]")
        print(
            "\n修法：改成 `match … { Ok(x) => x, Err(e) => panic!(\"<哪个夹具> failed: {e} — refusing to skip-as-pass\") }`，\n"
            "或直接 `super::skip_or_fail_without_db();`。仅当『无库不跑』是设计意图时才登记白名单并写明理由。\n"
            "背景：docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md §2"
        )
        return 1

    print("OK 没有 fail-open 的测试夹具：缺库/建池失败一律 panic，不会静默把空跑记成通过")
    return 0


if __name__ == "__main__":
    sys.exit(main())
