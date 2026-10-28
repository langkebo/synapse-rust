#!/usr/bin/env python3
"""改动面 → **受影响的 workspace crate**（PR 覆盖率车道的范围来源）。

## 为什么单独一个脚本

PR 车道要"只跑受影响 crate 的 lcov"（`cargo llvm-cov -p <crate>`），第一步就是把
`git diff` 的路径翻译成 crate 名。这件事有三个容易做错的地方，所以不做成 YAML 里的
`grep`：

1. **根包拥有整个仓库的路径**。`synapse-rust` 的 `manifest_path` 就在仓库根，
   朴素的"最长前缀匹配"会把它套到**每一个**文件上，于是 `synapse-web/src/x.rs`
   被算成根 crate、`docs/*.md` 也成了"改动根 crate"。所以成员目录先匹配，
   根包只认 `src/` `tests/` `benches/` `examples/` 与根 `build.rs`。
2. **crate 列表必须来自 `cargo metadata`**，不能手抄：新增成员 crate 时手抄的清单
   不会红，只会静默少测一个 crate。`--self-check` 拿 metadata 对账。
3. **认不出的 `.rs` 必须 fail-closed**。新开一个顶层目录放源码时，静默跳过等于
   "PR 车道没测它但看起来绿" —— 这是本仓最反复出现的失效形态。

## 用法

    python3 scripts/ci/affected_crates.py                       # HEAD 与工作区/暂存差异
    python3 scripts/ci/affected_crates.py --base origin/main --head HEAD
    python3 scripts/ci/affected_crates.py --format space        # 供 $GITHUB_OUTPUT 用
    python3 scripts/ci/affected_crates.py --map src/server/router.rs   # 单路径查询
    python3 scripts/ci/affected_crates.py --features-flag synapse-web  # `--features …` 或空
    python3 scripts/ci/affected_crates.py --self-check          # 对账 metadata 与真实树

退出码：0 = 正常（可能没有任何 crate）；2 = 有 `.rs` 落在任何规则之外（fail-closed）、
或 `--self-check` 对不上。

## 边界（写清楚，免得被当成"漏了"）

* `.rs` 之外的文件不影响任何行覆盖率 ⇒ 不算"受影响"（`docs/`、`*.md`、`*.sql` 等）。
* `vendor/` `scripts/` `docker/` 下的 `.rs` **刻意排除**：它们不参与任何被测 crate 的编译
  （第三方 vendored 代码、CI 工具、容器构建脚本）。这是决定，不是遗漏。
* 只算**直接影响**：改 `synapse-common` 不会把依赖它的 `synapse-web` 列进来。PR 车道的
  口径是"哪些 crate 的源码被改了"，不是"哪些 crate 需要重编译"（后者由 cargo 决定）。
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# `$CARGO` 是 cargo 自己的约定（测试与 build script 里由 cargo 设置）。走这个变量而不是
# 裸 `cargo`：单测用**假 cargo** 探针驱动 `run_pr_coverage.sh` 时，PATH 首位是假的，
# 而 `cargo metadata` 必须是真的 —— 否则探针测的就成了"假 cargo 会不会输出 metadata"。
CARGO = os.environ.get("CARGO", "cargo")

# 根包（`synapse-rust`）真正拥有源码的目录。它同时是"仓库根"，所以**不能**用
# 目录前缀把它套到所有路径上（见模块文档第 1 条）。
ROOT_CODE_DIRS = ("src/", "tests/", "benches/", "examples/")
ROOT_CODE_FILES = ("build.rs",)

# 刻意排除的 `.rs`：不参与任何被测 crate 的编译。
EXCLUDED_PREFIXES = {
    "vendor/": "第三方 vendored 代码，不参与本仓 crate 的编译",
    "scripts/": "CI / 发布工具，不是被测目标",
    "docker/": "容器构建脚本",
}

NO_CRATE = "-"


def cargo_metadata() -> dict:
    result = subprocess.run(
        [CARGO, "metadata", "--no-deps", "--format-version", "1", "--locked"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise SystemExit(f"cargo metadata failed: {result.stderr.strip()[:400]}")
    return json.loads(result.stdout)


def workspace(meta: dict) -> tuple[str, dict[str, dict]]:
    """`(根包名, {crate: {"dir": 相对目录, "features": set})}`。

    根包的 `dir` 记为空串 —— 它的源码判定走 [`ROOT_CODE_DIRS`]。
    """
    root_dir = Path(meta["workspace_root"]).resolve()
    root_name = ""
    crates: dict[str, dict] = {}
    for package in meta["packages"]:
        directory = Path(package["manifest_path"]).resolve().parent
        try:
            relative = directory.relative_to(root_dir).as_posix()
        except ValueError:
            raise SystemExit(
                f"package {package['name']} lives outside the workspace root "
                f"({directory}); the path→crate mapping assumes a single tree"
            )
        if relative == ".":
            root_name = package["name"]
            crates[package["name"]] = {"dir": "", "features": set(package["features"])}
        else:
            crates[package["name"]] = {
                "dir": relative,
                "features": set(package["features"]),
            }
    if not root_name:
        raise SystemExit("cargo metadata reported no package at the workspace root")
    return root_name, crates


def classify(path: str, root_name: str, crates: dict[str, dict]) -> tuple[str, str]:
    """`(crate 名 或 None, 理由)`。认不出时抛 [`UnknownPath`]。"""
    path = path.strip()
    if not path:
        raise UnknownPath(path, "empty path")

    # 1) 成员目录（最长匹配，避免 `synapse-common` 抢 `synapse-common-extra`）。
    matches = [
        (info["dir"], name)
        for name, info in crates.items()
        if info["dir"] and (path == info["dir"] or path.startswith(info["dir"] + "/"))
    ]
    if matches:
        best = max(matches, key=lambda item: len(item[0]))
        return best[1], f"member dir `{best[0]}/`"

    # 2) 根包：只有明确的源码目录/文件才归它。
    if path in ROOT_CODE_FILES:
        return root_name, f"root file `{path}`"
    for directory in ROOT_CODE_DIRS:
        if path.startswith(directory):
            return root_name, f"root dir `{directory}`"

    # 3) 不是 `.rs` ⇒ 不影响任何行覆盖率。
    if not path.endswith(".rs"):
        return None, "not a Rust source file"

    # 4) 明确排除的 `.rs`。
    for prefix, why in EXCLUDED_PREFIXES.items():
        if path.startswith(prefix):
            return None, f"excluded `{prefix}`: {why}"

    raise UnknownPath(path, "no crate claims this .rs path")


class UnknownPath(Exception):
    """一个 `.rs` 路径落在所有规则之外 —— PR 车道会因此少测一个 crate。"""

    def __init__(self, path: str, reason: str) -> None:
        super().__init__(f"{path}: {reason}")
        self.path = path
        self.reason = reason


def resolve_changed(base: str | None, head: str | None) -> list[str]:
    """改动路径。删除的文件不算（它们不会被测量，也没有行可覆盖）。"""
    spec = []
    if base:
        spec = [f"{base}...{head}" if head else f"{base}...HEAD"]
    command = [
        "git",
        "-c",
        "core.quotePath=false",
        "diff",
        "--name-only",
        "--diff-filter=d",
        *spec,
    ]
    result = subprocess.run(
        command, cwd=ROOT, capture_output=True, text=True, check=False
    )
    if result.returncode != 0:
        raise SystemExit(f"git diff failed: {result.stderr.strip()[:400]}")
    return [line.strip() for line in result.stdout.splitlines() if line.strip()]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--base", default=None, help="对比基线（如 origin/main）")
    parser.add_argument("--head", default=None, help="对比目标（默认 HEAD）")
    parser.add_argument("--format", choices=("lines", "space", "json"), default="lines")
    parser.add_argument("--map", dest="single", default=None, help="只解析这一个路径")
    parser.add_argument(
        "--features-flag", default=None, help="打印该 crate 可用的 feature 参数"
    )
    parser.add_argument(
        "--self-check", action="store_true", help="对账 metadata 与真实树"
    )
    parser.add_argument(
        "--explain", action="store_true", help="把每条判定理由打到 stderr"
    )
    args = parser.parse_args()

    root_name, crates = workspace(cargo_metadata())

    if args.features_flag:
        if args.features_flag not in crates:
            print(f"unknown crate {args.features_flag!r}", file=sys.stderr)
            return 2
        # `--features test-utils` 只在**该 crate 定义了这个 feature** 时才能给：
        # `synapse-cache` / `synapse-test-utils` 没有它，硬传会让 cargo 直接报错。
        print(
            "--features test-utils"
            if "test-utils" in crates[args.features_flag]["features"]
            else ""
        )
        return 0

    if args.single is not None:
        try:
            crate, why = classify(args.single, root_name, crates)
        except UnknownPath as error:
            print(f"FAIL {error}", file=sys.stderr)
            return 2
        print(crate or NO_CRATE)
        if args.explain:
            print(f"  {args.single} -> {crate or NO_CRATE} ({why})", file=sys.stderr)
        return 0

    if args.self_check:
        return self_check(root_name, crates)

    paths = resolve_changed(args.base, args.head)
    selected: set[str] = set()
    unknown: list[str] = []
    for path in paths:
        try:
            crate, why = classify(path, root_name, crates)
        except UnknownPath as error:
            unknown.append(str(error))
            continue
        if args.explain:
            print(f"  {path} -> {crate or NO_CRATE} ({why})", file=sys.stderr)
        if crate:
            selected.add(crate)

    if unknown:
        print(
            "❌ 以下 `.rs` 路径没有归属 crate —— PR 车道会因此静默少测一个 crate：",
            file=sys.stderr,
        )
        for line in unknown:
            print(f"   {line}", file=sys.stderr)
        print(
            "   修法：在 scripts/ci/affected_crates.py 里显式归类（成员 / 根源码目录 / 排除并写理由）。",
            file=sys.stderr,
        )
        return 2

    ordered = sorted(selected)
    if args.format == "json":
        print(json.dumps(ordered, ensure_ascii=False))
    elif args.format == "space":
        print(" ".join(ordered))
    else:
        for name in ordered:
            print(name)
    return 0


def self_check(root_name: str, crates: dict[str, dict]) -> int:
    """三条自证：成员自认、根目录自认、**每个跟踪的 `.rs` 都有归属或明确排除**。

    第三条是重点：它扫 `git ls-files '*.rs'`，任何新顶层目录里的源码都会在这里报红 ——
    而不是等到某个 PR 的覆盖率数字莫名其妙地偏低时才被发现。
    """
    problems: list[str] = []

    for name, info in crates.items():
        sample = f"{info['dir']}/src/lib.rs" if info["dir"] else "src/lib.rs"
        try:
            crate, _ = classify(sample, root_name, crates)
        except UnknownPath as error:
            problems.append(
                f"{name}: sample path {sample!r} is unknown ({error.reason})"
            )
            continue
        if crate != name:
            problems.append(f"{name}: sample path {sample!r} resolved to {crate!r}")

    for directory in ROOT_CODE_DIRS:
        sample = f"{directory}probe.rs"
        try:
            crate, _ = classify(sample, root_name, crates)
        except UnknownPath as error:
            problems.append(f"root dir {directory!r}: unknown ({error.reason})")
            continue
        if crate != root_name:
            problems.append(
                f"root dir {directory!r} resolved to {crate!r}, expected {root_name!r}"
            )

    tracked = subprocess.run(
        ["git", "-c", "core.quotePath=false", "ls-files", "*.rs"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if tracked.returncode != 0:
        problems.append(f"git ls-files failed: {tracked.stderr.strip()[:200]}")
    else:
        unowned: list[str] = []
        for path in tracked.stdout.splitlines():
            path = path.strip()
            if not path:
                continue
            try:
                classify(path, root_name, crates)
            except UnknownPath:
                unowned.append(path)
        if unowned:
            shown = "\n      ".join(unowned[:10])
            problems.append(
                f"{len(unowned)} tracked `.rs` file(s) belong to no crate:\n      {shown}"
            )

    if problems:
        print("❌ affected_crates self-check:")
        for problem in problems:
            print(f"   - {problem}")
        return 2
    scanned = len([p for p in tracked.stdout.splitlines() if p.strip()])
    print(
        f"OK affected_crates：{len(crates)} 个 crate 自认、根目录自认、{scanned} 个跟踪的 .rs 均有归属"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
