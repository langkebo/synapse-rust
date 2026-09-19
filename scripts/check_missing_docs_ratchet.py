#!/usr/bin/env python3
"""
B2 棘轮：missing_docs 增量门禁。

仓库现状（2026-09-19 实测）：7 个 library target 的 crate 根带 crate 级
`#![deny(missing_docs)]`（synapse-cache / synapse-common / synapse-storage /
synapse-federation / synapse-e2ee / synapse-services / 根 crate 的
`src/lib.rs`）；`synapse-web` 与 `synapse-test-utils` 的 lib 两者皆无；根包的
二进制 target（`src/main.rs` 与 5 个 `src/bin/*.rs`）也没有任何 crate 级 lint
属性。baseline 是 6，这 6 条全部来自那些没有 `//!` crate 文档的二进制入口
（实测 `cargo clippy -p synapse-rust -- -D missing_docs` 报 6，其余 8 个
`-p` 各报 0）。

lint 优先级（rustc 实测，最小复现见 `count_total_debt`）：源码里的 crate 级
`#![deny]` / `#![allow]` **压过**命令行 `-A` / `-D`；只有完全没有 crate 级属性
的 target，命令行 `-D missing_docs` 才说了算。所以本棘轮不是靠"抵消 crate 级
allow"工作（仓库里根本没有 crate 用 `#![allow(missing_docs)]`），而是靠
"对所有 target 强制 `-D`，带属性的 target 由属性自己把关"。

棘轮策略：
  - 存量不动（baseline=6，即 6 个缺 crate 文档的二进制入口）
  - 只卡**新增** `pub` 项：增量代码必须配 doc
  - 每修一点存量，必须收紧 baseline（防止 baseline 形同虚设）

具体规则：
  1. 新增文件（git diff --diff-filter=A 出来的 .rs）: 任何 pub item
     （pub fn / pub async fn / pub struct / pub enum / pub trait / pub const /
     pub static / pub type / pub mod）上方必须有 `///` doc 注释（同一文件内
     的前几行）。
  2. 修改文件（git diff --diff-filter=M）: 在 diff hunk 中**新增的 pub 项**也
     必须配 doc。
  3. baseline 记录当前 debt 总数（`scripts/.missing-docs-baseline` 里的单个整数，
     不是按 crate 分行记录）；debt 数字减少时 CI 失败，强制收紧 baseline。

退出码：
  0: 通过（debt 等于 baseline）
  1: 失败（debt 增了 / 新增 pub 缺 doc / debt 减了但 baseline 未收紧）
  2: 测量失败或 diff base 不可解析（绝不与 baseline 比较，避免"编译坏了"被
     误读成"debt 下降"）
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


class MeasurementFailed(RuntimeError):
    """The debt count cannot be trusted because the measurement itself failed.

    `-D missing_docs` makes a non-zero `cargo clippy` exit code the *expected*
    outcome whenever violations exist, so the exit code alone cannot be used to
    detect a broken build. It was never checked at all, which made a build
    failure indistinguishable from a clean tree:

      * a crate that fails to compile (stale/missing `.sqlx` offline cache, a
        type error in a sibling crate, a concurrent editor's half-finished
        change) emits zero `missing documentation` lines, so it contributed `0`
        to the total;
      * the ratchet then compared that fabricated `0` against a non-zero
        baseline and reported **"debt decreased"**, i.e. it demanded the
        baseline be tightened to a number that was never measured. Following
        that advice bakes `0` in, after which the gate is permanently green and
        permanently blind.

    rustc/clippy report a real error with a diagnostic code (`error[E0282]`),
    while a missing-docs diagnostic has none, so a coded error is treated as a
    measurement failure.
    """

# pub item 正则：pub fn / pub async fn / pub struct / pub enum / pub trait /
# pub const / pub static / pub type / pub mod。注意 pub(crate) / pub(super)
# 不算 crate-public，按规则不算 doc 必需项（但本文脚本先严格按 pub 处理，
# 因为 crate-public 也对外暴露 trait bound）。
PUB_ITEM_RE = re.compile(
    r"^\s*pub\s+(?:async\s+)?(?:fn|struct|enum|trait|const|static|type|mod|union)\b"
)
DOC_LINE_RE = re.compile(r"^\s*///")
DOC_BLOCK_OPEN_RE = re.compile(r"^\s*/\*\*")

# `--message-format=short` renders a missing-docs diagnostic as
# `path:line:col: error: missing documentation for ...` (no code), while a real
# compile failure carries a diagnostic code (`error[E0282]: ...`). Only the
# latter means "the measurement failed".
CODED_ERROR_RE = re.compile(r"error\[[A-Za-z]?\d+\]")

# B6-1：内容型（content-type）模板注释判据。
# 一条 /// doc 块里若**全部**行命中下列模式，视为"零信息"注释：
#   - `See [`x`].` 自指引用
#   - `Represents X.` / `The `y` module.` / `The `y` field.`
#   - 空注释（只有空格）
# 单独出现不违规（可有多行，第一行可能是真实说明）；
# 只要块里**至少一行**是实际信息（不匹配 SELF_REF/TEMPLATE 模式），整块通过。
SELF_REF_DOC_RE = re.compile(r"^\s*///\s*See\s+\[`?[a-zA-Z0-9_:<>& ]*`?\]\s*[.。]*\s*$")
TEMPLATE_DOC_RE = re.compile(
    r"^\s*///\s*(The\s+`[a-zA-Z0-9_]+`\s+(?:module|field|type|struct|enum|trait|constant|static)\s*\.?|Represents\s+[A-Z]\w*\s*\.?)"
)


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
        # A shallow clone cannot resolve `HEAD~1` (or `origin/main`), which is
        # exactly how this gate stopped running in CI: `actions/checkout@v4`
        # defaults to depth 1, the diff failed, and the step exited 2 — so the
        # incremental pub-doc check never evaluated a file (sweep C7). Name the
        # remedy instead of leaving a bare `ambiguous argument` behind.
        if "ambiguous argument" in result.stderr or "unknown revision" in result.stderr:
            print(
                "::error::the diff base is not resolvable in this clone. CI must check out full "
                "history (`actions/checkout@v4` with `fetch-depth: 0`); locally, fetch the base "
                "ref or pass `--base <ref>`.",
                file=sys.stderr,
            )
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
        # B6-1：内容型检测 —— 收集该 doc 块所有行的判据
        if DOC_LINE_RE.match(line):
            # 收集完整 doc 块
            doc_block: list[str] = []
            cursor = prev
            while cursor >= 0 and DOC_LINE_RE.match(lines[cursor]):
                doc_block.insert(0, lines[cursor])
                cursor -= 1
            # 若整块全是自指/模板注释，则违规
            for dl in doc_block:
                if SELF_REF_DOC_RE.match(dl) or TEMPLATE_DOC_RE.match(dl):
                    continue
                # 至少有一行实际内容 -> 通过
                return None
            return f"zero-info doc block above line {pub_line + 1} (self-ref/template only: {doc_block[0].strip()[:80]!r})"
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


def count_total_debt() -> int:
    """统计整个 workspace 的 `missing_docs` 诊断条数（crate 级 + pub 项）。

    这里只传 `-D missing_docs`。rustc 的 lint 优先级是 **源码属性 > 命令行**，
    所以"抵消 crate 级 allow"这个说法描述的机制并不存在——实测（rustc 1.93，
    `rustc --crate-type=lib --emit=metadata` 的最小文件）：

      * `#![allow(missing_docs)]` + `-D missing_docs` → 不报（属性 allow 胜出）
      * `#![deny(missing_docs)]`  + `-A missing_docs` → 照报（属性 deny 胜出）
      * 无属性 + `-A missing_docs -D missing_docs`    → 报（命令行内后一个覆盖前一个）
      * 无属性 + `-D missing_docs -A missing_docs`    → 不报

    于是本函数实际测量到的是：
      * 7 个带 `#![deny(missing_docs)]` 的 lib target —— 由**属性**强制报告，
        命令行给 `-D` / `-A` / 都不给都一样；今天各报 0（存量已补完）。
      * `synapse-web` / `synapse-test-utils` 的 lib —— 两者皆无属性，命令行
        `-D` 是唯一让它们可测的东西；今天各报 0。
      * 根包的二进制 target（无属性）—— `-D` 让它们报出全部 6 条
        "missing documentation for the crate"；baseline=6 就是这 6 条。

    结论：`-D` 是**承重**的。删掉它，根包二进制那 6 条会消失，总数从 6 掉到 0，
    棘轮会误报"debt 下降"并要求把 baseline 收紧到 0 —— 门禁从此永久失明。
    `-A missing_docs` 则是**死参数**：它对带 `#![deny]` 的 target 无效，对无属性
    target 又只是被同一命令行里靠后的 `-D` 覆盖（两者同时给与只给 `-D` 实测同
    结果），且其"抵消 crate 级 allow"的存在理由不成立（没有 crate 用
    `#![allow(missing_docs)]`）。故已删除，只留 `-D`。

    注意：`missing_docs` 对二进制 crate 里的 `pub` 项不诊断，只诊断 crate 本身；
    所以那 6 条全部是 crate 级（crate root `//!`）文档缺失。

    需要 SQLX_OFFLINE。编译失败时抛 [`MeasurementFailed`]，**不返回 0**。
    原因见该异常。
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
                # 承重：不带 crate 级 lint 属性的 target（synapse-web /
                # synapse-test-utils 的 lib、根包的 main.rs 与 src/bin/*）只靠
                # 这个 flag 才会报告。带 `#![deny]` 的 lib 由属性把关，属性压过
                # 命令行；带 `#![allow]` 的（本仓为 0 个）连它也会被属性压掉。
                "-D",
                "missing_docs",
            ],
            cwd=REPO_ROOT,
            env=env,
            capture_output=True,
            text=True,
        )
        combined = (proc.stdout or "") + (proc.stderr or "")
        n = sum(1 for ln in combined.splitlines() if "missing documentation" in ln)

        # A coded rustc/clippy error means the crate did not build, so `n` is not
        # a debt count (see `MeasurementFailed`).
        coded_error = CODED_ERROR_RE.search(combined)
        if coded_error:
            raise MeasurementFailed(
                f"`cargo clippy -p {crate}` failed to compile "
                f"({coded_error.group(0)}); its missing-docs count of {n} is meaningless. "
                "First lines of the failure:\n    "
                + "\n    ".join(combined.strip().splitlines()[:5])
            )
        # Non-zero exit with nothing counted is also a failure: `-D missing_docs`
        # only exits non-zero when it actually reported something.
        if proc.returncode != 0 and n == 0:
            raise MeasurementFailed(
                f"`cargo clippy -p {crate}` exited {proc.returncode} but reported no "
                "missing-docs diagnostic, so the count cannot be trusted. Output:\n    "
                + "\n    ".join(combined.strip().splitlines()[:5])
            )
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
    try:
        current = count_total_debt()
    except MeasurementFailed as error:
        # Exit 2 (not 1) so a broken measurement is distinguishable from a real
        # ratchet violation, and NEVER fall through to the baseline comparison —
        # that is how "build is broken" became "debt decreased, tighten the
        # baseline".
        print(f"\n::error::missing_docs debt could not be measured: {error}", file=sys.stderr)
        print(
            "  Refusing to compare (or update) the baseline against a failed measurement.",
            file=sys.stderr,
        )
        print("  Fix the build, then re-run this gate.", file=sys.stderr)
        return 2
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