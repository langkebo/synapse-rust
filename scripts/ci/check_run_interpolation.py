#!/usr/bin/env python3
"""RD-17：workflow 的 `run:` 脚本里不得内插**用户可控**的上下文。

背景
----
GitHub Actions 的 `${{ … }}` 是**脚本落盘之前**做的文本替换，所以

    run: |
      FILTER="${{ github.event.inputs.test_filter }}"

看起来"加了引号"，实际等价于把输入原样拼进 shell 源码：输入 `$(id)` 会**真的执行**，
`a"; rm -rf /` 会真的逃出引号。双引号只防空格，不防命令替换 —— 这正是 Actions
文档里的 script injection，把"能 dispatch 的人"升级成"能执行任意 shell 的人"。

2026-10-30 实测本仓有 4 处（`db-tests-manual.yml:98`、`mutation-testing.yml:98/117/121`）；
`e2ee-interop.yml:178` 那处本就在 `env:` 里，安全。修法统一是：输入先进 `env:`，
脚本里只读 `"$VAR"` —— env 的值不参与 shell 解析，天然免疫。

顺带查出另一类"配了但没接"：`e2ee-interop.yml` 里那个 env 键叫
`ELEMENT_HARNESS_SCRIPT`，而 harness 读的是 `TEST_SCRIPT` ⇒ 该 dispatch 输入
**从未生效**，永远落到 harness 默认值。

判据
----
* **S1** 任何 `run:` 脚本体内都不得出现用户可控上下文的插值：
  `github.event.inputs.*` / `inputs.*` / `github.head_ref` /
  `github.event.pull_request.{title,body,head.*}` / `github.event.{issue,comment}.body`。
* **S2** fail-closed：扫到的 `run` 块少于下限即红（扫描器坏了不许空转通过）。

不纳入（避免误伤）：`${{ github.workspace }}` / `${{ runner.os }}` / `${{ github.ref }}`
这类由 GitHub 决定、调用方无法控制的值，它们出现在 `run:` 里是常见且必要的。

实现说明：为什么不用 `import yaml`
----------------------------------
本仓 `scripts/ci/` 下**没有任何脚本依赖 PyYAML**，CI 也没有 `pip install` 步骤。
引入 `import yaml` 就等于给"每次 PR 的 fast tier"埋一个"runner 上没装就红"的隐患。
所以这里用零依赖的**缩进扫描**：定位 `run:` 行，把它之后所有缩进更深的行吃成脚本体，
遇到缩进回到 `run:` 同级即结束。env / with / if 的值在结构上就不在脚本体内，
因此"输入放 env 就安全"这条不需要额外规则 —— 这是缩进扫描与正则裸扫的关键区别。

用法
----
    python3 scripts/ci/check_run_interpolation.py              # 门禁
    python3 scripts/ci/check_run_interpolation.py --self-test   # 正/负样本自检
"""

from __future__ import annotations

import re
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW_GLOB = ".github/workflows/*.yml"

USER_CONTROLLED = re.compile(
    r"\$\{\{\s*(?:"
    r"github\.event\.inputs\.[A-Za-z0-9_]+"
    r"|inputs\.[A-Za-z0-9_]+"
    r"|github\.head_ref"
    r"|github\.event\.pull_request\.(?:title|body|head\.ref|head\.label)"
    r"|github\.event\.(?:issue|comment)\.body"
    r")\s*\}\}"
)

RUN_KEY = re.compile(r"^(?P<indent>[ \t]*)(?:-[ \t]+)?run:[ \t]*(?P<rest>.*)$")
BLOCK_SCALARS = ("|", "|-", "|+", ">", ">-", ">+")

# 低于这个数说明扫描出了问题（当前实测 191 个 run 块）
MIN_RUN_BLOCKS = 120


def iter_run_blocks(text: str):
    """按缩进切出 `run:` 的脚本体，返回 [(起始行号, 名字提示, 脚本体)]。"""
    lines = text.splitlines()
    index = 0
    while index < len(lines):
        match = RUN_KEY.match(lines[index])
        if not match:
            index += 1
            continue
        key_indent = len(match.group("indent"))
        rest = match.group("rest").strip()
        start = index + 1
        index += 1
        if rest and rest not in BLOCK_SCALARS:
            yield start, rest  # 单行 run: 也要查
            continue
        body: list[str] = []
        while index < len(lines):
            line = lines[index]
            if not line.strip():
                body.append("")
                index += 1
                continue
            indent = len(line) - len(line.lstrip())
            if indent <= key_indent:
                break
            body.append(line)
            index += 1
        yield start, "\n".join(body)


def scan(root: Path) -> tuple[list[str], int]:
    problems: list[str] = []
    scanned = 0
    for path in sorted(root.glob(WORKFLOW_GLOB)):
        rel = path.relative_to(root)
        text = path.read_text(encoding="utf-8", errors="replace")
        for line_no, body in iter_run_blocks(text):
            scanned += 1
            for match in USER_CONTROLLED.finditer(body):
                problems.append(
                    f"S1 {rel}:{line_no} 起的 `run:` 脚本内插了用户可控上下文 "
                    f"`{match.group(0)}`\n"
                    '      改为放到该 step 的 `env:` 里，脚本中只读 "$VAR" —— '
                    "env 的值不参与 shell 解析，不会被执行"
                )
    if scanned < MIN_RUN_BLOCKS:
        problems.append(
            f"S2 只扫到 {scanned} 个 run 块（下限 {MIN_RUN_BLOCKS}）—— 扫描器或 glob 坏了，"
            "空转通过比红更危险"
        )
    return problems, scanned


def run(root: Path | None = None) -> int:
    global ROOT
    if root is not None:
        ROOT = root

    problems, scanned = scan(ROOT)
    if problems:
        for problem in problems:
            print(f"FAIL {problem}", file=sys.stderr)
        return 1

    print(f"OK {scanned} 个 `run:` 块均无用户可控上下文内插")
    return 0


def self_test() -> int:
    import contextlib
    import io
    import shutil

    def build(extra_steps: str, filler: int = MIN_RUN_BLOCKS) -> int:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / ".github/workflows").mkdir(parents=True)
            pad = "".join(
                f"      - name: ok{i}\n        run: echo hi\n" for i in range(filler)
            )
            text = (
                "name: t\non:\n  workflow_dispatch:\njobs:\n"
                "  j:\n    runs-on: ubuntu-latest\n    steps:\n" + pad + extra_steps
            )
            (root / ".github/workflows/t.yml").write_text(text, encoding="utf-8")
            sink = io.StringIO()
            with contextlib.redirect_stdout(sink), contextlib.redirect_stderr(sink):
                code = run(root)
            shutil.rmtree(root, ignore_errors=True)
            return code

    cases = [
        ("干净 workflow", "", 0),
        (
            "run 块里插 dispatch 输入",
            '      - name: bad\n        run: |\n          FILTER="${{ github.event.inputs.test_filter }}"\n',
            1,
        ),
        (
            "run 块里插 workflow_call 输入",
            '      - name: bad2\n        run: |\n          cargo mutants --package "${{ inputs.package }}"\n',
            1,
        ),
        (
            "单行 run 里插输入",
            '      - name: bad3\n        run: echo "${{ github.head_ref }}"\n',
            1,
        ),
        (
            "同样输入改放 env（应通过）",
            (
                "      - name: good\n        env:\n"
                "          FILTER: ${{ github.event.inputs.test_filter }}\n"
                '        run: |\n          echo "$FILTER"\n'
            ),
            0,
        ),
        (
            "run 里的静态上下文（github.workspace，应通过）",
            '      - name: good2\n        run: |\n          echo "${{ github.workspace }}"\n',
            0,
        ),
    ]
    failed = 0
    for name, steps, expect in cases:
        got = build(steps)
        ok = got == expect
        if not ok:
            failed += 1
        print(f"  {'OK  ' if ok else 'FAIL'} {name}: exit={got} (期望 {expect})")
    if failed:
        print(f"self-test: {failed} 个用例不符合预期", file=sys.stderr)
        return 1
    print(f"self-test: {len(cases)}/{len(cases)} 通过")
    return 0


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()
    return run()


if __name__ == "__main__":
    sys.exit(main())
