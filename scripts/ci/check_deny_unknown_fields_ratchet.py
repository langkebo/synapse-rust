#!/usr/bin/env python3
"""`deny_unknown_fields` 覆盖率的**反向棘轮**（只许长、不许缩）。

背景
----
`docs/audit/BACKEND_CONTRACT_SURFACE_VERIFICATION_2026-10-19.md` 的 N-1 指出
「有 struct ≠ 有校验」：`#[derive(Deserialize)]` 默认**静默丢弃**未知字段。
补齐的路径（admin/vendor 面 body 一律 deny、query 面逐条登记）已经在
`docs/audit/BACKEND_CONTRACT_SURFACE_PLAN_REVIEW_2026-10-19.md` §8 落地，
但「补完这一次」不等于「以后不会掉」——重构合文件、删一个看似没用的属性、
把 struct 挪到别的模块，都会让覆盖率静默回退，而没有任何测试会红。

所以这里加的是**反向**棘轮：与 clippy-allow（只许降）相反，deny 站点只许升。

口径（唯一实现，文档禁止手抄）
------------------------------
* **扫描范围**：与 `check_clippy_allow_ratchet.py` / `check_trait_ratchet.py`
  相同的 9 个生产源码根（`tests/`、`benches/` 不在内：那里的 deny 是夹具）。
* **计数单位 = 站点（site）**：一个容器级 `#[serde(… deny_unknown_fields …)]`
  属性算 1 个站点。`#[serde(default, deny_unknown_fields)]` 同样算 1 个；
  跨行属性算 1 个；同一个 struct 上重复写两次算 2 个（本来也是错的）。
* **必须先剥注释与字符串**：仓库里有 15 处注释写着
  *Deliberately **not** `deny_unknown_fields`*，还有测试夹具把属性写进字符串。
  文本匹配若不剥注释/字符串，这些都会被算成"已经 deny"——这正是 PR #P2-C
  第一版守卫踩过的坑（plan review §8.4）。
* **基线按文件**（`scripts/ci/deny_unknown_fields_baseline`）：每行
  `<count> <path>`。判据只有两条：
    1. 基线里的文件仍在、且当前站点数 **>=** 基线值；
    2. 文件消失 ⇒ 红（守卫被整文件删除时不能静默通过）。
  站点数**上升**是目标，不判红；`--update` 把它收紧成新基线。

为什么不是"数没有 deny 的 struct"：client / federation 面有 66 个 struct
按协议**必须**保持开放（任意 JSON、上游本就忽略未知字段），把它们算进
"反债"会让分母变成一个永远降不下去的数字，棘轮随即退化成噪音。这里只钉
"已经 deny 的不许丢"，增量靠 N-1 / P2 的清单推进。

用法
----
    python3 scripts/ci/check_deny_unknown_fields_ratchet.py              # 门禁
    python3 scripts/ci/check_deny_unknown_fields_ratchet.py --update     # 收紧基线
    python3 scripts/ci/check_deny_unknown_fields_ratchet.py --self-test  # 红/绿自证
"""

from __future__ import annotations

import os
import re
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
BASELINE = ROOT / "scripts" / "ci" / "deny_unknown_fields_baseline"

# 与 check_clippy_allow_ratchet.py / check_trait_ratchet.py 完全一致的"我们拥有的代码"。
ROOTS = [
    "synapse-common/src",
    "synapse-cache/src",
    "synapse-storage/src",
    "synapse-e2ee/src",
    "synapse-federation/src",
    "synapse-services/src",
    "synapse-web/src",
    "synapse-test-utils/src",
    "src",
]

EXCLUDED = (
    "/target/",
    "/.claude/",
    "/.worktrees/",
    "/vendor/",
    "/artifacts/",
)

SERDE_OPEN = re.compile(r"#\[\s*serde\s*\(")
DENY_TOKEN = re.compile(r"(?<![A-Za-z0-9_])deny_unknown_fields(?![A-Za-z0-9_])")


def mask_comments_and_strings(text: str) -> str:
    """把注释与字符串字面量替换成空格（保留换行，长度不变）。

    长度不变是为了让行号可以直接由偏移量换算。处理：行注释、可嵌套块注释、
    普通字符串（含转义）、原始字符串 `r"…"` / `r#"…"#`、字符字面量 `'"'`。
    """
    out = list(text)
    n = len(text)
    i = 0

    def blank(start: int, end: int) -> None:
        for k in range(start, min(end, n)):
            if out[k] != "\n":
                out[k] = " "

    while i < n:
        ch = text[i]
        if ch == "/" and i + 1 < n and text[i + 1] == "/":
            j = text.find("\n", i)
            j = n if j == -1 else j
            blank(i, j)
            i = j
            continue
        if ch == "/" and i + 1 < n and text[i + 1] == "*":
            depth = 1
            j = i + 2
            while j < n and depth:
                if text.startswith("/*", j):
                    depth += 1
                    j += 2
                elif text.startswith("*/", j):
                    depth -= 1
                    j += 2
                else:
                    j += 1
            blank(i, j)
            i = j
            continue
        # 原始字符串：r"…" / r#"…"# / br"…" / br#"…"#
        raw = re.match(r'b?r(#*)["\']', text[i:])
        if raw and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            hashes = raw.group(1)
            opener_len = raw.end()
            closer = ('"' + hashes) if raw.group(0).endswith('"') else ("'" + hashes)
            j = text.find(closer, i + opener_len)
            j = n if j == -1 else j + len(closer)
            blank(i, j)
            i = j
            continue
        if ch == '"':
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"':
                    j += 1
                    break
                j += 1
            blank(i, j)
            i = j
            continue
        if ch == "'":
            # 字符字面量：'x' / '\n' / '\''。生命周期 'a 不进入这个分支。
            m = re.match(r"'(\\.[^']*|[^'\\])'", text[i:])
            if m:
                blank(i, i + m.end())
                i += m.end()
                continue
        i += 1
    return "".join(out)


def sites_in_text(masked: str) -> list[int]:
    """返回该文件中每个 deny 站点的行号（1-based）。`masked` 已剥注释/字符串。"""
    lines: list[int] = []
    for match in SERDE_OPEN.finditer(masked):
        i = match.end()
        depth = 1
        while i < len(masked) and depth:
            if masked[i] == "(":
                depth += 1
            elif masked[i] == ")":
                depth -= 1
            i += 1
        body = masked[match.end() : i - 1]
        if DENY_TOKEN.search(body):
            lines.append(masked.count("\n", 0, match.start()) + 1)
    return lines


def scan_file(path: Path, relpath: str, counts: dict[str, int]) -> int:
    text = path.read_text(encoding="utf-8", errors="replace")
    sites = sites_in_text(mask_comments_and_strings(text))
    if sites:
        counts[relpath] = len(sites)
    return len(sites)


def collect(root: Path = ROOT) -> dict[str, int]:
    counts: dict[str, int] = {}
    for rel in ROOTS:
        base = root / rel
        if not base.is_dir():
            continue
        for dirpath, _dirnames, filenames in os.walk(base):
            marker_path = dirpath.replace(os.sep, "/") + "/"
            if any(marker in marker_path for marker in EXCLUDED):
                continue
            for name in sorted(filenames):
                if not name.endswith(".rs"):
                    continue
                full = Path(dirpath) / name
                relpath = full.relative_to(root).as_posix()
                scan_file(full, relpath, counts)
    return counts


def read_baseline(path: Path = BASELINE) -> dict[str, int]:
    baseline: dict[str, int] = {}
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split(None, 1)
        if len(parts) != 2:
            continue
        baseline[parts[1]] = int(parts[0])
    return baseline


def write_baseline(counts: dict[str, int], path: Path = BASELINE) -> None:
    total = sum(counts.values())
    lines = [
        "# `deny_unknown_fields` 反向棘轮基线（只许升，不许降）。",
        "# 生成/收紧：python3 scripts/ci/check_deny_unknown_fields_ratchet.py --update",
        "# 口径：容器级 `#[serde(… deny_unknown_fields …)]` 属性算 1 个站点；",
        "#       已剥离注释与字符串；扫描 9 个生产源码根（不含 tests/、benches/）。",
        f"# 实测：{total} 个站点 / {len(counts)} 个文件。",
        "# 格式：<count> <path>",
        "",
    ]
    for rel in sorted(counts, key=lambda p: (-counts[p], p)):
        lines.append(f"{counts[rel]} {rel}")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def evaluate(current: dict[str, int], baseline: dict[str, int]) -> list[str]:
    """返回所有回退问题（空列表 = 绿）。"""
    problems: list[str] = []
    for rel, was in sorted(baseline.items()):
        now = current.get(rel, 0)
        if now == 0:
            problems.append(
                f"{rel}: 基线 {was} 个站点，现在一个都没有（文件被删或守卫被整批移除）"
            )
        elif now < was:
            problems.append(f"{rel}: {was} -> {now}（丢了 {was - now} 个守卫）")
    return problems


def self_test() -> int:
    """正/负样本 + 端到端红证明。任何一条不成立返回 1。"""
    failures: list[str] = []

    good = """
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct A { x: u32 }

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct B { y: u32 }

#[derive(Deserialize)]
#[serde(
    rename_all = "camelCase",
    deny_unknown_fields,
)]
struct C { z: u32 }
"""
    if len(sites_in_text(mask_comments_and_strings(good))) != 3:
        failures.append("正样本：单行 / 组合 / 跨行 三种写法应各记 1 个站点（共 3）")

    prose = """
/// Deliberately **not** `deny_unknown_fields`: the handler ignored unknown params.
// Deliberately not deny_unknown_fields
/* Deliberately not deny_unknown_fields */
#[derive(Deserialize)]
struct Open { a: u32 }
"""
    if sites_in_text(mask_comments_and_strings(prose)):
        failures.append("负样本：注释里的 deny_unknown_fields 不应计数")

    fixture = r"""
#[test]
fn guard() {
    let src = r#"
#[serde(deny_unknown_fields)]
struct Fake { a: u32 }
"#;
    assert!(src.contains("deny_unknown_fields"));
}
"""
    if sites_in_text(mask_comments_and_strings(fixture)):
        failures.append("负样本：字符串字面量里的属性不应计数")

    if sites_in_text(
        mask_comments_and_strings('#[serde(rename_all = "x")] struct D { a: u32 }')
    ):
        failures.append("负样本：不含 deny 的 serde 属性不应计数")

    if evaluate({"a.rs": 2}, {"a.rs": 2}):
        failures.append("绿样本：站点数持平应通过")
    if not evaluate({"a.rs": 1}, {"a.rs": 2}):
        failures.append("红样本：站点数下降必须判红")
    if not evaluate({}, {"a.rs": 2}):
        failures.append("红样本：文件消失必须判红")

    # 端到端：临时树 + 同一 collect/evaluate，证明门禁真的会因为删一个属性而翻红。
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        src = root / "synapse-web" / "src"
        src.mkdir(parents=True)
        target = src / "probe.rs"
        target.write_text(
            "#[derive(Deserialize)]\n#[serde(deny_unknown_fields)]\nstruct A { x: u32 }\n"
            "#[derive(Deserialize)]\n#[serde(deny_unknown_fields)]\nstruct B { y: u32 }\n",
            encoding="utf-8",
        )
        baseline = {"synapse-web/src/probe.rs": 2}
        if evaluate(collect(root), baseline):
            failures.append("端到端：2 个站点对基线 2 应判绿")
        target.write_text(
            "#[derive(Deserialize)]\n#[serde(deny_unknown_fields)]\nstruct A { x: u32 }\n"
            "#[derive(Deserialize)]\nstruct B { y: u32 }\n",
            encoding="utf-8",
        )
        if not evaluate(collect(root), baseline):
            failures.append("端到端：删掉 1 个属性后必须判红")

    if failures:
        print("FAIL: deny_unknown_fields 棘轮自证未通过：", file=sys.stderr)
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1
    print("OK: deny_unknown_fields 棘轮自证通过（正/负样本 + 删属性翻红）")
    return 0


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()

    current = collect()
    if not current:
        print(
            "FAIL: 扫到 0 个 deny 站点 —— 扫描器坏了（fail-closed）",
            file=sys.stderr,
        )
        return 2

    if "--update" in sys.argv:
        write_baseline(current)
        print(
            f"deny_unknown_fields: 基线已更新 —— {sum(current.values())} 个站点 / "
            f"{len(current)} 个文件"
        )
        return 0

    if not BASELINE.exists():
        print(
            f"FAIL: 基线 {BASELINE.relative_to(ROOT)} 不存在 —— "
            "先跑 --update 生成（fail-closed）",
            file=sys.stderr,
        )
        return 2

    baseline = read_baseline()
    if not baseline:
        print(
            f"FAIL: 基线 {BASELINE.relative_to(ROOT)} 为空 —— "
            "先跑 --update 生成（fail-closed）",
            file=sys.stderr,
        )
        return 2

    print(
        f"deny_unknown_fields: {sum(current.values())} 个站点 / {len(current)} 个文件 "
        f"(基线 {sum(baseline.values())} / {len(baseline)})"
    )

    problems = evaluate(current, baseline)
    if problems:
        print(
            f"FAIL: {len(problems)} 处 deny 覆盖回退 —— 守卫只许加不许删；"
            "确属有意删除时先改代码评审结论，再跑 --update：",
            file=sys.stderr,
        )
        for item in problems:
            print(f"  {item}", file=sys.stderr)
        return 1

    grown = [
        (rel, baseline[rel], current[rel])
        for rel in baseline
        if current.get(rel, 0) > baseline[rel]
    ]
    fresh = [rel for rel in current if rel not in baseline]
    if grown or fresh:
        print(
            f"OK: 高于基线（{len(grown)} 个文件增加守卫，{len(fresh)} 个新文件）。"
            "收紧基线：python3 scripts/ci/check_deny_unknown_fields_ratchet.py --update"
        )
    else:
        print("OK: deny_unknown_fields 覆盖与基线一致")
    return 0


if __name__ == "__main__":
    sys.exit(main())
