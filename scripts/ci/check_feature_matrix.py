#!/usr/bin/env python3
"""RD-14 / TEST-04：feature 能力矩阵门禁（**可失败**）。

为什么重写（2026-10-23，W6 批次 5）
------------------------------------
旧版有两处**伪断言**，运行多年、从未真正工作过：

* `[4/5] --no-default-features` 只 `print("KNOWN-ISSUE ...")`，从不执行 cargo；
* `[5/5] shipped == tested == default` 只 `print("OK")`，没有任何比较。

实测 `cargo check -p synapse-rust --no-default-features` 早已通过（~57s），
旧注释里「synapse-common 硬依赖 axum 所以 no-default 编不过」的前提**已不成立**，
该豁免就此过期。本版把它变成真车道。

两种模式
--------
    python3 scripts/ci/check_feature_matrix.py             # 静态（默认，零 cargo，<1s）
    python3 scripts/ci/check_feature_matrix.py --compile   # 追加 cargo 编译车道（CI job）

静态不变式 S1–S5（任一失败 → exit 1）
------------------------------------
* S1 `[features]` 可解析；`default` / `all-extensions` 引用的标志都存在；
* S2 `all-extensions` 闭包覆盖所有可单独开关的扩展特性（豁免须在
  `ALL_EXTENSIONS_EXEMPT` 登记原因）；
* S3 每个 crate 内**真正参与编译**的 `cfg(feature = "X")` 必须指向该 crate
  自己声明的特性（含 optional 依赖产生的隐式特性，`dep:` 抑制规则已处理）——
  捕获「删了特性却把 cfg 门留下」与拼写错误。注释与字符串里的举例不算：
  `tests/unit/gated_module_test_gate_tests.rs` 的文档注释就写着
  `#[cfg(feature = "X")] pub mod m;` 这种占位符，把它们算进来只会制造误报；
* S4 能力台账 `FEATURE_STATUS` 与 `[features]` **双向一致**：
  新增一个特性而不登记它是 supported / experimental / meta / internal 就红；
* S5 CI 接线：`ci.yml` 必须真的以 `--compile` 调用本脚本，删掉这条接线就红
  （防止「脚本存在但从未接入」，即 RD-14 的原始形态）。

编译车道 C1–C5（`--compile`）
-----------------------------
* C1 default：`cargo check --workspace`
* C2 all：`cargo check --workspace --all-features --all-targets`
* C3 no-default：`cargo check --workspace --no-default-features`
* C4 隔离：每个 supported / experimental 特性单独
  `cargo check -p synapse-rust --no-default-features --features <f>`
* C5 出货组合：`cargo check -p synapse-rust --no-default-features --features all-extensions --bins`
  —— 与 `docker/Dockerfile` 的 `ARG CARGO_FEATURE_ARGS` 默认值同口径（见
  `scripts/ci/check_feature_arg_consistency.py`）。C1–C4 都不覆盖它，而它此前只在
  `build` job（`if` 排除 PR）里编过。

退出码：0 = 全部通过；1 = 有失败项（明细在 stdout）。
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
ROOT_TOML = REPO_ROOT / "Cargo.toml"
WORKFLOWS_DIR = REPO_ROOT / ".github" / "workflows"
TARGET_DIR_NAME = "target"

# COMPAT-08：冷 workspace 下 `cargo check` 要跑多条车道，慢 runner 上 300s 不够。
# 默认 900s，CI 可用 `FEATURE_MATRIX_TIMEOUT_SECS` 覆盖而无需改文件。
DEFAULT_TIMEOUT_SECS = int(os.environ.get("FEATURE_MATRIX_TIMEOUT_SECS", "900"))

# 不是「能力开关」的三个根特性：`test-utils` / `performance-tests` 是测试基建，
# `all-extensions` 是元特性。
NON_CAPABILITY = {"default", "test-utils", "performance-tests", "all-extensions"}

# `all-extensions` 不必包含的特性：元特性（其成员已被 all-extensions 覆盖）。
ALL_EXTENSIONS_EXEMPT = {
    "core-private-chat": "元特性：成员 friends / burn-after-read 已在 all-extensions 内",
}

# 能力台账（S4）：每个 `[features]` 键都必须在这里登记，且只登记一次。
#   supported    —— 默认部署可开、CI 隔离编译并通过
#   experimental —— 有实现但**不承诺兼容**，仅保证可编译（RD-14 要求显式声明）
#   meta         —— 只是若干特性的聚合名
#   internal     —— 测试/性能基建，不面向部署
FEATURE_STATUS = {
    "default": (
        "meta",
        "默认组合：core-private-chat + widgets + external-services + beacons",
    ),
    "core-private-chat": ("meta", "friends + burn-after-read + synapse-web 私有聊天门"),
    "friends": ("supported", "好友关系与好友房间"),
    "voice-extended": ("supported", "MSC3245 voice 扩展（服务端仅配置/消息转换占位）"),
    "saml-sso": ("experimental", "SAML 登录：实现存在，未承诺互通性"),
    "cas-sso": ("experimental", "CAS 登录：实现存在，未承诺互通性"),
    "beacons": ("supported", "MSC3488 beacon 位置共享"),
    "voip-tracking": ("supported", "VoIP 通话事件跟踪"),
    "widgets": ("supported", "MSC4261 widget 端点（默认开启）"),
    "server-notifications": ("supported", "服务端通知广播"),
    "burn-after-read": ("supported", "阅后即焚事件"),
    "privacy-ext": ("supported", "隐私扩展"),
    "external-services": ("supported", "admin external_service 端点（默认开启）"),
    "builtin-oidc": ("supported", "内建 OIDC Provider"),
    "all-extensions": ("meta", "全部扩展特性的聚合名"),
    "test-utils": ("internal", "测试基建（EnvGuard / env_lock）"),
    "performance-tests": ("internal", "性能用例开关"),
}


class Failure(Exception):
    """单条不变式失败（收集完一起报，不早退）。"""


def load_toml(path: Path) -> dict:
    with path.open("rb") as fh:
        return tomllib.load(fh)


def workspace_crates() -> dict[str, Path]:
    """{crate 名: Cargo.toml 路径}，含根 crate。"""
    crates = {}
    for toml in sorted(REPO_ROOT.glob("*/Cargo.toml")):
        data = load_toml(toml)
        name = data.get("package", {}).get("name")
        if name:
            crates[name] = toml
    root = load_toml(ROOT_TOML)
    crates[root["package"]["name"]] = ROOT_TOML
    return crates


def declared_features(toml_path: Path) -> set[str]:
    """crate 可被 `cfg(feature = …)` 引用的全部名字。

    `[features]` 的键，加上 optional 依赖产生的**隐式特性**（被 `dep:` 引用过则抑制）。
    """
    data = load_toml(toml_path)
    features: dict[str, list[str]] = data.get("features", {})
    names = set(features)

    optional: set[str] = set()
    for section in ("dependencies", "build-dependencies"):
        for dep, spec in data.get(section, {}).items():
            if isinstance(spec, dict) and spec.get("optional"):
                optional.add(dep)

    suppressed: set[str] = set()
    for items in features.values():
        for item in items:
            if isinstance(item, str) and item.startswith("dep:"):
                suppressed.add(item[len("dep:") :])

    return names | (optional - suppressed)


def root_features() -> dict[str, list[str]]:
    return load_toml(ROOT_TOML).get("features", {})


def closure(features: dict[str, list[str]], seeds: list[str]) -> set[str]:
    """同 crate 内的特性闭包（忽略 `crate/feature` 与 `dep:` 转发项）。"""
    seen: set[str] = set()
    stack = list(seeds)
    while stack:
        item = stack.pop()
        if item in seen or "/" in item or item.startswith("dep:"):
            continue
        if item != "default" and item in features:
            seen.add(item)
            stack.extend(features[item])
    return seen


FEATURE_EXPR = re.compile(r'feature\s*=\s*"([^"]+)"')
CFG_TOKEN = re.compile(r"\bcfg(?:_attr)?\b")
STATEMENT_BOUNDARY = re.compile(r"[;{}\]]")

# 每个 .rs 只做一次掩码（根 crate 的 rglob 会重复走到成员 crate 的文件）。
_MASK_CACHE: dict[Path, tuple[str, str]] = {}


def mask_rust(src: str) -> tuple[str, str]:
    """返回 `(no_comments, code_only)`，两者与 `src` 等长。

    * `no_comments`：注释替换为空格，字符串**保留**（`feature = "friends"` 的名字在里面）；
    * `code_only`：注释与字符串内容都替换为空格，用来判断某个位置是否真在代码里。

    行/块注释、普通/原始字符串、字符字面量都要认，否则 `"http://x"` 里的 `//`
    会被当成注释起点、把后半行的真 `cfg` 一起吃掉。
    """
    n = len(src)
    no_comments = list(src)
    code_only = list(src)
    i = 0
    while i < n:
        ch = src[i]
        nxt = src[i + 1] if i + 1 < n else ""
        if ch == "/" and nxt == "/":
            j = src.find("\n", i)
            j = n if j < 0 else j
            for k in range(i, j):
                no_comments[k] = " "
                code_only[k] = " "
            i = j
        elif ch == "/" and nxt == "*":
            depth, j = 1, i + 2
            while j < n and depth:
                if src.startswith("/*", j):
                    depth += 1
                    j += 2
                elif src.startswith("*/", j):
                    depth -= 1
                    j += 2
                else:
                    j += 1
            for k in range(i, min(j, n)):
                no_comments[k] = " "
                code_only[k] = " "
            i = j
        elif ch in "rb" and re.match(r'(?:b?r)(#*)"', src[i:]):
            match = re.match(r'(?:b?r)(#*)"', src[i:])
            start = i + len(match.group(0))
            end_marker = '"' + match.group(1)
            j = src.find(end_marker, start)
            j = n if j < 0 else j + len(end_marker)
            for k in range(start, j):
                code_only[k] = " "
            i = j
        elif ch == '"':
            j = i + 1
            while j < n:
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] == '"':
                    j += 1
                    break
                j += 1
            for k in range(i + 1, min(j, n)):
                code_only[k] = " "
            i = j
        elif ch == "'":
            literal = re.match(r"'(?:\\.|[^\\'])'", src[i:])
            if literal:  # 字符字面量；否则是生命周期标注 `'a`
                for k in range(i + 1, i + literal.end() - 1):
                    code_only[k] = " "
                i += literal.end()
            else:
                i += 1
        else:
            i += 1
    return "".join(no_comments), "".join(code_only)


def in_cfg_context(no_comments: str, pos: int) -> bool:
    """`pos` 处是否落在某个 `#[cfg(...)]` / `#[cfg_attr(...)]` / `cfg!(...)` 里。

    从 `pos` 往回找最近的 `cfg`，若它们之间已经出现语句/属性终结符，说明那个
    `cfg` 属于上一段代码，不是包住 `pos` 的。
    """
    seg = no_comments[max(0, pos - 500) : pos]
    last = None
    for match in CFG_TOKEN.finditer(seg):
        last = match
    if last is None:
        return False
    return STATEMENT_BOUNDARY.search(seg[last.end() :]) is None


def masked_rust(path: Path) -> tuple[str, str]:
    cached = _MASK_CACHE.get(path)
    if cached is None:
        cached = mask_rust(path.read_text(errors="ignore"))
        _MASK_CACHE[path] = cached
    return cached


def static_checks() -> list[str]:
    problems: list[str] = []
    features = root_features()
    if not features:
        raise Failure("根 Cargo.toml 没有 [features] 节")

    # S1：引用完整性
    for key in ("default", "all-extensions"):
        if key not in features:
            problems.append(f"S1 缺少 `{key}` 特性")
            continue
        for item in features[key]:
            if "/" in item or item.startswith("dep:"):
                continue
            if item not in features:
                problems.append(f"S1 `{key}` 引用了未声明的特性 `{item}`")

    # S2：all-extensions 覆盖全部可单独开关的能力
    covered = closure(features, features.get("all-extensions", []))
    missing = sorted(
        set(features) - NON_CAPABILITY - covered - set(ALL_EXTENSIONS_EXEMPT)
    )
    for name in missing:
        problems.append(
            f"S2 特性 `{name}` 既不在 all-extensions 闭包内、也没在 ALL_EXTENSIONS_EXEMPT 登记"
        )

    # S3：cfg 门必须指向本 crate 声明的特性（只看真正参与编译的 cfg 语境）
    crates = workspace_crates()
    # 根 crate 的 rglob 会走进成员 crate 的目录，会让同一处问题报两遍（且是以
    # 根 crate 的 `[features]` 去判成员 crate 的门），这里把成员目录排除掉。
    member_dirs = [p.parent for p in crates.values() if p != ROOT_TOML]
    for crate, toml_path in crates.items():
        allowed = declared_features(toml_path)
        crate_dir = toml_path.parent
        hits: dict[str, list[str]] = {}
        for rs in crate_dir.rglob("*.rs"):
            if TARGET_DIR_NAME in rs.parts:
                continue
            if crate_dir == REPO_ROOT and any(
                rs.is_relative_to(member) for member in member_dirs
            ):
                continue
            try:
                text = rs.read_text(errors="ignore")
            except OSError:
                continue
            if "feature" not in text:  # 绝大多数文件没有 cfg 门，跳过掩码
                continue
            no_comments, code_only = masked_rust(rs)
            line_starts = [0]
            for idx, ch in enumerate(no_comments):
                if ch == "\n":
                    line_starts.append(idx + 1)
            for match in FEATURE_EXPR.finditer(no_comments):
                pos = match.start()
                if pos >= len(code_only) or code_only[pos].isspace():
                    continue  # 注释或字符串里的举例，不是编译期门
                if not in_cfg_context(no_comments, pos):
                    continue  # 不是 cfg 语境（如日志文案、测试断言字符串）
                name = match.group(1)
                if name in allowed:
                    continue
                lineno = 1
                for idx, start in enumerate(line_starts):
                    if start > pos:
                        break
                    lineno = idx + 1
                rel = rs.relative_to(REPO_ROOT)
                hits.setdefault(name, []).append(f"{rel}:{lineno}")
        for name, where in sorted(hits.items()):
            first = where[0]
            more = f"（另 {len(where) - 1} 处）" if len(where) > 1 else ""
            problems.append(f"S3 `{crate}` 引用了未声明的特性 `{name}`：{first}{more}")

    # S4：能力台账双向一致
    for name in sorted(set(features) - set(FEATURE_STATUS)):
        problems.append(
            f"S4 特性 `{name}` 未在 FEATURE_STATUS 登记（supported / experimental / meta / internal）"
        )
    for name in sorted(set(FEATURE_STATUS) - set(features)):
        problems.append(f"S4 FEATURE_STATUS 登记了已不存在的特性 `{name}`")

    # S5：CI 必须真的以 --compile 调用本脚本
    wired = False
    for wf in sorted(WORKFLOWS_DIR.glob("*.y*ml")):
        for line in wf.read_text(errors="ignore").splitlines():
            if "check_feature_matrix.py" in line and "--compile" in line:
                wired = True
                break
    if not wired:
        problems.append(
            "S5 没有任何 workflow 以 `--compile` 调用 check_feature_matrix.py"
            "（RD-14 要求的「接入 CI」必须可验证）"
        )

    return problems


def cargo(args: list[str]) -> tuple[int, str]:
    cmd = ["cargo", "check", "--locked", *args]
    proc = subprocess.run(
        cmd, cwd=REPO_ROOT, capture_output=True, text=True, timeout=DEFAULT_TIMEOUT_SECS
    )
    return proc.returncode, (proc.stdout + proc.stderr).strip()


def compile_lanes() -> list[str]:
    features = root_features()
    lanes: list[tuple[str, list[str]]] = [
        ("C1 default", ["--workspace"]),
        ("C2 all-features", ["--workspace", "--all-features", "--all-targets"]),
        ("C3 no-default-features", ["--workspace", "--no-default-features"]),
        # C5：**出货组合**。`docker/Dockerfile` 的 `ARG CARGO_FEATURE_ARGS` 默认值是
        # `--features all-extensions --no-default-features`，而 C1–C4 里没有任何一条
        # 覆盖它：
        #   * C1 是 default（`beacons` 之类靠默认值来，掩盖"漏在 all-extensions 外"）；
        #   * C2 是 `--all-features`（多带 test-utils / performance-tests，属另一套
        #     cfg 组合，证明不了出货组合能编译）；
        #   * C3 是 `--no-default-features`（一个 feature 都不开）；
        #   * C4 是逐个特性单独开（证明不了"12 个一起开"）。
        # 此前这条组合只在 `ci.yml` 的 `build` job 里编，而那个 job 的 `if` 把 PR 排除在外
        # ⇒ **PR 期从来没验过出货组合**，要等合并后才知道。本车道挂在 `feature-matrix`
        # job 上（该 job 无 `if`，每次 PR 都跑），把这段补回来。
        # `-p synapse-rust --bins` 与 Dockerfile 的
        # `cargo build --bin synapse-rust --bin synapse_worker --bin healthcheck` 同口径。
        (
            "C5 shipped (all-extensions, no-default)",
            [
                "-p",
                "synapse-rust",
                "--no-default-features",
                "--features",
                "all-extensions",
                "--bins",
            ],
        ),
    ]
    for name in sorted(features):
        status = FEATURE_STATUS.get(name, ("", ""))[0]
        if status not in ("supported", "experimental"):
            continue
        lanes.append(
            (
                f"C4 isolated {name}",
                ["-p", "synapse-rust", "--no-default-features", "--features", name],
            )
        )

    problems: list[str] = []
    for label, args in lanes:
        rc, out = cargo(args)
        if rc == 0:
            print(f"  OK   {label}")
        else:
            tail = "\n".join(out.splitlines()[-15:])
            print(f"  FAIL {label}")
            problems.append(f"{label} 编译失败：\n{tail}")
    return problems


def main(argv: list[str]) -> int:
    compile_mode = "--compile" in argv
    unknown = [a for a in argv if a not in ("--compile",)]
    if unknown:
        print(f"::error::未知参数 {unknown}；用法：check_feature_matrix.py [--compile]")
        return 1

    features = root_features()
    caps = sorted(set(features) - NON_CAPABILITY - set(ALL_EXTENSIONS_EXEMPT))
    print(
        f"[feature-matrix] 根特性 {len(features)} 个，可单独开关的能力 {len(caps)} 个："
        f"{', '.join(caps)}"
    )

    problems = static_checks()
    if problems:
        for p in problems:
            print(f"  FAIL {p}")
        print(f"\n[feature-matrix] 静态不变式失败 {len(problems)} 条")
        return 1
    print("  OK   S1–S5 静态不变式")

    if not compile_mode:
        print("[feature-matrix] 静态通过（编译车道需 --compile）")
        return 0

    problems = compile_lanes()
    if problems:
        for p in problems:
            print(f"  FAIL {p}")
        print(f"\n[feature-matrix] 编译车道失败 {len(problems)} 条")
        return 1

    print("[feature-matrix] 静态 + 编译车道全部通过")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except subprocess.TimeoutExpired:
        print(
            f"::error::cargo check timed out ({DEFAULT_TIMEOUT_SECS}s); "
            "set FEATURE_MATRIX_TIMEOUT_SECS to raise it on slow runners"
        )
        sys.exit(1)
    except Failure as exc:
        print(f"::error::{exc}")
        sys.exit(1)
