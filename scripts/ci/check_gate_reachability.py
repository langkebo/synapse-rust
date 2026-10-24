#!/usr/bin/env python3
"""每个 `check_*` 门禁脚本都必须真的被某条车道调用，或者显式登记为"手动入口"。

## 为什么需要这条门禁

审计 TEST-04 记的是"8 个 CI 脚本从未被任何 workflow 调用"。逐条查可达链之后，
真正的危险不是"多写了几个用不上的脚本"，而是**两个棘轮被当成已在生效**：

- `check_ts_order_tiebreak.py` —— W4 第二层（`0d18fede0`）的落地说明里写着
  "修复会让本脚本的逐文件计数收缩，按脚本约定必须 `--update` 收紧（67 处 / 28 文件），
  否则门禁红"。它确实红了、也 `--update` 了，**但没有任何 workflow 调用它**；
- `check_plan_numbers_fresh.sh` —— 只在 `preflight.sh` 里跑（本地），CI 侧从未跑过。

"本地跑过"与"CI 会跑"是两件事：任何跳过 preflight 的提交（或用 `--no-verify`、
或在没有配钩子的 checkout 上提交）都不会被它们拦住。而"门禁存在"读起来与"门禁在跑"
完全一样 —— 这正是 AGENTS.md 铁律 8 的"门禁长期全绿 ⇒ 先怀疑它没在工作"。

## 判据

1. 从**唯一入口**（workflow / git 钩子 / `preflight.sh`）出发做引用闭包；
2. 闭包外的 `check_*` 脚本必须出现在 `scripts/ci/gate_reachability_optin.txt`，且**带理由**；
3. 登记表里的条目一旦变得可达（说明已经接进去了），必须从登记表删除 —— 否则登记表会
   慢慢变成"第二个真相"，而没人再看得懂哪些是真豁免。

## 用法

    python3 scripts/ci/check_gate_reachability.py            # 校验（CI / preflight）
    python3 scripts/ci/check_gate_reachability.py --print    # 只打印当前闭包
    python3 scripts/ci/check_gate_reachability.py --why <脚本>  # 打印从根到该脚本的一条链

退出码：0 = 全部可达或已登记；1 = 有未登记的死门禁 / 过期的登记项。
"""

from __future__ import annotations

import re
import subprocess
import sys
from collections import deque
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent

# 唯一入口：能自己主动跑起来的东西。不要往里加"另一个 check 脚本"——
# 那等于把"谁调用它"这个问题往后挪一层，而这一层恰恰是本脚本要回答的。
ROOT_FILES = ["scripts/quality/preflight.sh"]
ROOT_DIRS = [".github/workflows", ".githooks"]

OPTIN = "scripts/ci/gate_reachability_optin.txt"

# 引用形态：仓库相对路径（本仓脚本一律这么写，含 workflow 的 `run:` 行），
# 以及 `"$(dirname "$0")/foo.py"` 这种同目录相对引用。
REF_RE = re.compile(r"(?<![\w/.-])((?:\./)?scripts/[A-Za-z0-9_./-]+\.(?:py|sh|mjs|js))")
SIBLING_RE = re.compile(r"\$\(dirname\s+\"?\$0\"?\)/([A-Za-z0-9_.-]+\.(?:py|sh))")


def tracked(patterns: list[str]) -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", *patterns],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return [line for line in out.splitlines() if line]


def tracked_or_new(patterns: list[str]) -> list[str]:
    """受控文件 + 尚未 `git add` 的新文件。

    `git ls-files` **看不到未跟踪文件**，于是"本地新建一个 check_*.sh 却没接线"在
    preflight 里是绿的、推到 CI 才红 —— 而 CI 红的时候人已经在做别的事了。
    本条门禁的全部意义是"提交前就发现"，所以必须把未跟踪的新文件算进来
    （实测：只用 `git ls-files` 时，新增一个未跟踪的 check 探针脚本不报错）。
    """
    out = subprocess.run(
        ["git", "ls-files", "--others", "--exclude-standard", *patterns],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return sorted(set(tracked(patterns)) | {line for line in out.splitlines() if line})


def text_of(rel: str) -> str:
    try:
        return (REPO / rel).read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


def edges_from(rel: str) -> set[str]:
    body = strip_non_code(rel, text_of(rel))
    found: set[str] = set()
    for match in REF_RE.finditer(body):
        found.add(match.group(1).removeprefix("./"))
    for match in SIBLING_RE.finditer(body):
        sibling = str(Path(rel).parent / match.group(1))
        found.add(sibling)
    # 自引用（脚本注释里的 usage 示例）不算边：它不让任何人跑起来。
    found.discard(rel)
    return found


TRIPLE_QUOTED = re.compile(r'"""(?:.|\n)*?"""|\'\'\'(?:.|\n)*?\'\'\'')


def strip_non_code(rel: str, body: str) -> str:
    """去掉注释与文档字符串，再做引用匹配。

    不剥的话，"描述某个脚本"会被读成"调用某个脚本"，而且这种假边**极难发现** ——
    本轮实测：本文件自己的 docstring 里举了一个 `scripts/ci/check_<probe>.sh` 的例子，
    于是那个根本不存在的探针脚本被判成"可达"。同一形态在 shell 侧也踩过
    （`--run-ignored only` 曾经被注释与 `log` 行命中，使守卫在该红的时候绿）。
    """
    suffix = Path(rel).suffix
    if suffix in {".sh", ".yml", ".yaml"}:
        return "\n".join(
            line for line in body.splitlines() if not line.lstrip().startswith("#")
        )
    if suffix == ".py":
        body = TRIPLE_QUOTED.sub("", body)
        return "\n".join(line.split("#", 1)[0] for line in body.splitlines())
    return body


def closure() -> tuple[set[str], dict[str, str]]:
    """返回 (可达集合, 每个可达脚本的父节点)。"""
    roots = [f for f in ROOT_FILES if (REPO / f).exists()]
    for directory in ROOT_DIRS:
        roots.extend(tracked([directory]))

    seen: set[str] = set()
    parent: dict[str, str] = {}
    queue = deque()
    for root in roots:
        seen.add(root)
        queue.append(root)

    while queue:
        current = queue.popleft()
        for target in edges_from(current):
            if not (REPO / target).exists():
                continue
            if target in seen:
                continue
            seen.add(target)
            parent[target] = current
            queue.append(target)
    return seen, parent


def subjects() -> list[str]:
    """受本条门禁约束的脚本：`check_*` 名字就是"这是一道门"的声明。"""
    out = [
        path
        for path in tracked_or_new(["scripts/**"])
        if Path(path).name.startswith("check_") and Path(path).suffix in {".py", ".sh"}
    ]
    return sorted(out)


def load_optin() -> tuple[dict[str, str], list[str]]:
    entries: dict[str, str] = {}
    problems: list[str] = []
    path = REPO / OPTIN
    if not path.exists():
        return entries, problems
    for lineno, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        script, _, reason = line.partition("#")
        script = script.strip()
        reason = reason.strip()
        if not script:
            problems.append(f"{OPTIN}:{lineno} 只有理由没有脚本路径")
            continue
        if not reason:
            problems.append(
                f"{OPTIN}:{lineno} `{script}` 没有写理由 —— 无理由的豁免等于没登记"
                "（同一纪律见 CODE-11 的 `// reason:` 强制）"
            )
        if not (REPO / script).exists():
            problems.append(
                f"{OPTIN}:{lineno} `{script}` 不存在（改名/删除后忘了同步登记表）"
            )
        entries[script] = reason
    return entries, problems


def why(target: str, parent: dict[str, str]) -> str:
    chain = [target]
    while chain[-1] in parent:
        chain.append(parent[chain[-1]])
    return " <- ".join(chain) if len(chain) > 1 else f"{target}（不在任何车道的闭包里）"


def main(argv: list[str]) -> int:
    reachable, parent = closure()
    gates = subjects()
    optin, problems = load_optin()

    if "--print" in argv:
        print(
            f"可达文件 {len(reachable)} 个；门禁脚本 {len(gates)} 个；登记豁免 {len(optin)} 条"
        )
        for gate in gates:
            mark = (
                "REACHABLE"
                if gate in reachable
                else ("OPT-IN" if gate in optin else "DEAD")
            )
            print(f"  {mark:9s} {gate}")
        return 0

    if "--why" in argv:
        target = argv[argv.index("--why") + 1]
        print(why(target, parent))
        return 0

    dead = [gate for gate in gates if gate not in reachable and gate not in optin]
    stale = [script for script in optin if script in reachable]

    if problems:
        print("FAIL 登记表自身有问题：")
        for problem in problems:
            print(f"  - {problem}")
    if dead:
        print(
            "FAIL 以下门禁脚本没有被任何车道调用（workflow / git 钩子 / preflight.sh）："
        )
        for gate in dead:
            print(f"  - {gate}")
        print(
            "\n两种正确做法，选一个：\n"
            "  ① 接进车道：在 `.github/workflows/ci.yml`（或对应 workflow）与 "
            "`scripts/quality/preflight.sh` 里加调用；\n"
            f"  ② 确实是手动入口：写进 `{OPTIN}`，格式 `<路径>  # <为什么不能自动化>`。\n"
            "不要既不接线也不登记 —— 那样的脚本会让读代码的人以为这道门在保护他们。"
        )
    if stale:
        print("FAIL 登记表里有条目已经可达（说明已经接线了，登记项该删）：")
        for script in stale:
            print(f"  - {script}  ← 现在被 {parent.get(script, '?')} 调用")
    if problems or dead or stale:
        return 1

    print(
        f"OK 门禁可达性：{len(gates)} 个 check_* 脚本全部可达或已登记（豁免 {len(optin)} 条）"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
