#!/usr/bin/env python3
"""`CI Summary` 的哨兵不得把某个 lane 的"必须跑过"挂在**它并不依赖**的 job 结论上。

## 为什么需要它

`ci.yml` 的 `ci-summary` job 里有一个 `Sentinel — required lanes actually ran`
步骤，职责是让"这条车道到底跑了没有"变成退出码。它读 `needs.<job>.result`
环境变量，逐个 `require_lane "<显示名>" "$X_RESULT"`。

2026-10-28 实测到的形态：Code Coverage 的那一行被包在

    case "$INTEGRATION_RESULT" in
      success) require_lane "Code Coverage" "$COVERAGE_RESULT" ;;
      *) echo "info: … not requiring it" ;;

里，依据是一条注释说 "`Code Coverage` needs: integration-test"。但
`coverage.needs` 早在某次改动后只剩 `['changes']` —— **它并不依赖 Integration
Tests**，integration 失败时 coverage 照常执行。于是：

* integration 红 ⇒ 哨兵打印 "not requiring it" 放行，**coverage 即使被
  skip/cancel 也不会有人报**；
* 而这正是"空跑看起来像验证"的形态 —— 一个 job 的结论被另一个 job 的结果
  短路掉了，短路条件还是错的。

注释不是判据。这条门禁把"注释/判据里声明的依赖"与 YAML 里真实的 `needs`
逐条对上，错一条就红。

## 判据（全部从 `ci.yml` 文本结构化解析，不依赖 PyYAML）

1. 哨兵读的每个 `${{ needs.<job>.result }}` 都必须在 `ci-summary.needs` 里
   —— 否则 GitHub 给的是空串，`require_lane` 会把空串当"没跑"或（旧实现的
   比较写法下）当通过；
2. 任何 `case "$<VAR>" in` 门控（VAR 经 `env:` 解析到 job J）内出现的
   `require_lane "<lane>"`，都必须满足 **J ∈ needs(lane 对应的 job)**；
3. `require_lane` / `case` 用到的变量必须真的在 `env:` 里声明过；
4. `require_lane` 的显示名必须能对上一个真实 job 的 `name:`（否则是改名后
   残留的死引用，永远匹配不到）。

## 用法

    python3 scripts/ci/check_ci_summary_dependencies.py
    python3 scripts/ci/check_ci_summary_dependencies.py --print   # 打印解析结果
    python3 scripts/ci/check_ci_summary_dependencies.py --mutation-check

`--mutation-check` 在内存里注入两处已知缺陷，要求本脚本各报一次红 —— 免得
解析器在某次 YAML 重构后安静地匹配不到任何东西（"扫零 = 通过"是这类脚本最
常见的失效方式）。

退出码：0 = 一致；1 = 不一致或有缺陷；2 = 解析失败（fail-closed）。
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"

# 这些 job 是"一堆互不依赖的独立门禁"堆在一起（`repo-sanity` 29 步、每步 <2 s）。
# GitHub 的默认语义是**前一步失败 ⇒ 后面全部 skipped**，于是第一个红会把其余门禁
# 一起藏起来：实测 run 40633362339，`SQLx dynamic/static ratio ratchet` 一红，
# 它后面 12 个门禁（`.sqlx` 缓存、ts 决胜键、web 分层、sleep 棘轮、门禁可达性、
# 路由分层、连接预算、promtool、trait 棘轮、行数棘轮、clippy-allow、deny_unknown_fields、
# feature 矩阵）**全部 skipped** —— 那些门禁那一次等于不存在，而日志里只能看到
# 一条红。"某个 job 失败已经失去信息量"的机制就在这里。
#
# 判据：这些 job 的**每一步**都必须带 `if: ${{ !cancelled() }}`（取消仍然中止，
# 失败不再连带 skip）。用 `!cancelled()` 而不是 `always()`：`always()` 会在取消后
# 也继续跑，那是浪费 runner 的另一头。
ISOLATED_GATE_JOBS = ("repo-sanity",)

JOB_KEY = re.compile(r"^  ([A-Za-z0-9_.-]+):\s*$")
JOB_NAME = re.compile(r"^    name:\s*(.+?)\s*$")
NEEDS_INLINE = re.compile(r"^    needs:\s*\[(.*)\]\s*$")
NEEDS_BLOCK = re.compile(r"^    needs:\s*$")
NEEDS_ITEM = re.compile(r"^      -\s+([A-Za-z0-9_.-]+)\s*$")
ENV_RESULT = re.compile(
    r"^\s+([A-Z0-9_]+):\s*\$\{\{\s*needs\.([A-Za-z0-9_.-]+)\.result\s*\}\}\s*$"
)
NEEDS_REF = re.compile(r"\$\{\{\s*needs\.([A-Za-z0-9_.-]+)\.result\s*\}\}")
CASE_OPEN = re.compile(r'^\s*case\s+"\$([A-Z0-9_]+)"\s+in\s*$')
REQUIRE_LANE = re.compile(r'require_lane\s+"([^"]+)"\s+"\$([A-Z0-9_]+)"')


class Layout:
    """`ci.yml` 里与本次判据相关的结构。"""

    def __init__(self, text: str) -> None:
        self.lines = text.splitlines()
        self.jobs: dict[str, list[str]] = {}
        self.names: dict[str, str] = {}
        self.needs: dict[str, list[str]] = {}
        self._parse_jobs()

    def _parse_jobs(self) -> None:
        try:
            start = next(
                i for i, line in enumerate(self.lines) if line.rstrip() == "jobs:"
            )
        except StopIteration:
            raise SystemExit("cannot find a top-level `jobs:` block in ci.yml")
        starts: list[tuple[int, str]] = []
        for i in range(start + 1, len(self.lines)):
            m = JOB_KEY.match(self.lines[i])
            if m:
                starts.append((i, m.group(1)))
        if not starts:
            raise SystemExit("cannot find any job key under `jobs:`")
        for idx, (line_no, job_id) in enumerate(starts):
            end = starts[idx + 1][0] if idx + 1 < len(starts) else len(self.lines)
            block = self.lines[line_no:end]
            self.jobs[job_id] = block
            for line in block:
                nm = JOB_NAME.match(line)
                if nm and job_id not in self.names:
                    self.names[job_id] = nm.group(1).strip().strip("'\"")
            self.needs[job_id] = self._parse_needs(block)

    @staticmethod
    def _parse_needs(block: list[str]) -> list[str]:
        for i, line in enumerate(block):
            inline = NEEDS_INLINE.match(line)
            if inline:
                return [
                    p.strip().strip("'\"")
                    for p in inline.group(1).split(",")
                    if p.strip()
                ]
            if NEEDS_BLOCK.match(line):
                out: list[str] = []
                for follow in block[i + 1 :]:
                    item = NEEDS_ITEM.match(follow)
                    if not item:
                        break
                    out.append(item.group(1))
                return out
        return []

    def by_display_name(self, label: str) -> str | None:
        """按 `name:` 找 job；矩阵 job 的 `name` 带 `(…)` 后缀，按前缀匹配。

        例：`integration-test` 的 name 是
        `Integration Tests (${{ matrix.isolation }})`，哨兵里写的是
        `require_lane "Integration Tests"` —— 逐字相等会永远对不上，于是改成
        "等于 label，或以 `label (` 开头"。多个 job 同时命中视为歧义（返回 None 之外
        还要让调用方报错），因为那时谁也说不清这条 lane 指的是哪一个。
        """
        exact = [j for j, n in self.names.items() if n == label]
        if len(exact) == 1:
            return exact[0]
        prefix = [j for j, n in self.names.items() if n.startswith(label + " (")]
        if len(prefix) == 1:
            return prefix[0]
        return None

    def sentinel_block(self, summary_job: str = "ci-summary") -> list[str]:
        """`Sentinel` 步骤的整段（含 `env:` 与 `run:`）。"""
        block = self.jobs.get(summary_job)
        if block is None:
            raise SystemExit(f"ci.yml has no `{summary_job}` job")
        begins = [
            i
            for i, line in enumerate(block)
            if re.match(r"^      - name:\s*Sentinel", line)
        ]
        if not begins:
            raise SystemExit(
                f"`{summary_job}` has no step whose name starts with 'Sentinel'"
            )
        if len(begins) > 1:
            raise SystemExit(
                f"`{summary_job}` has {len(begins)} 'Sentinel' steps; the check "
                "assumes exactly one"
            )
        start = begins[0]
        end = len(block)
        for i in range(start + 1, len(block)):
            if re.match(r"^      - (name|uses|run):", block[i]):
                end = i
                break
        return block[start:end]


def sentinel_env(step: list[str]) -> dict[str, str]:
    """`VAR -> job id`，只收 `${{ needs.<job>.result }}` 形态。"""
    mapping: dict[str, str] = {}
    for line in step:
        m = ENV_RESULT.match(line)
        if m:
            mapping[m.group(1)] = m.group(2)
    return mapping


def run_body(step: list[str]) -> list[str]:
    """`run: |` 的内容（按首行的实际缩进去掉 YAML 缩进）。

    ⚠️ 不能硬编码缩进宽度：步骤的 `- name:` 在 6 空格、`run: |` 在 8 空格、内容在
    10 空格，写死 12 会把整个 body 读成空数组 —— 于是判据 2/3/4 全部对空集成立，
    脚本对着任何缺陷都打印 ✅。这正是"扫零 = 通过"。所以缩进取自 body 的第一行。
    """
    for i, line in enumerate(step):
        if re.match(r"^\s+run:\s*\|\s*$", line):
            raw = step[i + 1 :]
            indent = None
            body: list[str] = []
            for follow in raw:
                if follow.strip() == "":
                    body.append("")
                    continue
                current = len(follow) - len(follow.lstrip(" "))
                if indent is None:
                    indent = current
                if current < indent:
                    break
                body.append(follow[indent:])
            while body and body[-1] == "":
                body.pop()
            if not body:
                raise SystemExit("the Sentinel step's `run: |` block is empty")
            return body
    raise SystemExit("the Sentinel step has no `run: |` block")


def audit(text: str, layout: Layout | None = None) -> tuple[list[str], list[str]]:
    """返回 (problems, notes)。problems 非空即判红。"""
    layout = layout or Layout(text)
    problems: list[str] = []
    notes: list[str] = []

    declared = set(layout.needs.get("ci-summary", []))
    if not declared:
        problems.append("`ci-summary` declares no `needs:`; nothing to verify")
        return problems, notes

    step = layout.sentinel_block()
    env = sentinel_env(step)
    if not env:
        problems.append(
            "the Sentinel step reads no `${{ needs.<job>.result }}` value — "
            "it cannot detect a skipped lane at all"
        )

    # 判据 1：读的结论必须是自己声明的依赖。
    body = run_body(step)
    body_text = "\n".join(body)
    if not REQUIRE_LANE.search(body_text):
        problems.append(
            "the Sentinel step calls `require_lane` for no lane at all — a "
            "sentinel that requires nothing always passes"
        )
    for job in sorted({m.group(1) for m in NEEDS_REF.finditer("\n".join(step))}):
        if job not in declared:
            problems.append(
                f"Sentinel reads `needs.{job}.result` but `ci-summary.needs` does "
                f"not list `{job}` — GitHub resolves that to an empty string, so "
                f"the lane check silently degrades"
            )

    # 判据 3：变量必须先声明。
    case_cursor: list[tuple[str, list[str]]] = []
    inner_case: str | None = None
    inner_lines: list[str] = []
    for line in body:
        opened = CASE_OPEN.match(line)
        if opened:
            if inner_case is not None:
                problems.append(
                    f'nested `case "${opened.group(1)}"` inside `case "${inner_case}"` '
                    "— the scanner cannot attribute the inner requirement; flatten it"
                )
            inner_case = opened.group(1)
            inner_lines = []
            continue
        if inner_case is not None:
            inner_lines.append(line)
            if re.match(r"^\s*esac\b", line):
                case_cursor.append((inner_case, inner_lines))
                inner_case = None
                inner_lines = []
    if inner_case is not None:
        problems.append(f'`case "${inner_case}"` is never closed by `esac`')

    # 判据 2：case 门控里的 lane 必须真的依赖门控变量对应的 job。
    for var, lines in case_cursor:
        gate_job = env.get(var)
        if gate_job is None:
            problems.append(
                f'`case "${var}"` gates a requirement on a variable that is not '
                f"derived from `needs.<job>.result` in this step's `env:`"
            )
            continue
        if gate_job not in declared:
            problems.append(
                f'`case "${var}"` gates on `{gate_job}`, which `ci-summary.needs` '
                f"does not list"
            )
            continue
        for line in lines:
            for m in REQUIRE_LANE.finditer(line):
                label, lane_var = m.group(1), m.group(2)
                lane_job = env.get(lane_var)
                if lane_job is None:
                    problems.append(
                        f'`require_lane "{label}" "${lane_var}"` uses an undeclared variable'
                    )
                    continue
                if gate_job not in layout.needs.get(lane_job, []):
                    problems.append(
                        f'`require_lane "{label}"` is gated on `case "${var}"` '
                        f"(= job `{gate_job}`), but job `{lane_job}` does not declare "
                        f"`needs: {gate_job}` — the gate short-circuits a lane that "
                        f"runs independently of it, so 'skipped' goes unreported"
                    )
        notes.append(
            f'case "${var}" gates {len(REQUIRE_LANE.findall(chr(10).join(lines)))} lane(s) on {gate_job}'
        )

    # 判据 4：显示名必须对得上真实 job。
    for m in REQUIRE_LANE.finditer(body_text):
        label = m.group(1)
        if layout.by_display_name(label) is None:
            problems.append(
                f'`require_lane "{label}"` names no job in ci.yml — after a rename '
                "this reference keeps passing against nothing"
            )

    # 判据 5：独立门禁堆在一起的 job，每一步都要 `if: ${{ !cancelled() }}`。
    for job_id in ISOLATED_GATE_JOBS:
        block = layout.jobs.get(job_id)
        if block is None:
            problems.append(
                f"`{job_id}` is declared in ISOLATED_GATE_JOBS but does not exist"
            )
            continue
        unguarded = [step for step in step_names(block) if not step[1]]
        if not step_names(block):
            problems.append(
                f"`{job_id}` has no steps the scanner can see — the check is vacuous"
            )
            continue
        for name, _ in unguarded:
            problems.append(
                f'`{job_id}` step "{name}" has no `if: ${{{{ !cancelled() }}}}` — '
                "one red step skips every later gate in the same job, so those "
                "gates simply do not run"
            )
        notes.append(
            f"{job_id}: {len(step_names(block)) - len(unguarded)}/"
            f"{len(step_names(block))} step(s) isolated from each other's failures"
        )
    return problems, notes


def step_names(block: list[str]) -> list[tuple[str, bool]]:
    """`(步骤名, 是否带 if: ${{ !cancelled() }})` —— 按 `- name:` 切段。"""
    steps: list[tuple[str, bool]] = []
    current: tuple[str, bool] | None = None
    for line in block:
        header = re.match(r"^      - name:\s*(.+?)\s*$", line)
        if header:
            if current is not None:
                steps.append(current)
            current = (header.group(1).strip("'\"")[:60], False)
            continue
        if current is not None and re.match(
            r"^\s+if:\s*\$\{\{\s*!\s*cancelled\(\)\s*\}\}\s*$", line
        ):
            current = (current[0], True)
    if current is not None:
        steps.append(current)
    return steps


def report(problems: list[str], notes: list[str], label: str) -> None:
    for note in notes:
        print(f"   info: {note}")
    if problems:
        print(f"❌ {label}: {len(problems)} problem(s)")
        for p in problems:
            print(f"   - {p}")
    else:
        print(f"✅ {label}: sentinel requirements match the declared `needs` graph")


def mutation_check(text: str) -> int:
    """三处已知缺陷必须各报一次红，否则解析器已经失明。"""
    mutations = [
        (
            "requirements gated on a job the lane does not depend on",
            '            require_lane "Code Coverage" "$COVERAGE_RESULT"',
            (
                '            case "$INTEGRATION_RESULT" in\n'
                '              success) require_lane "Code Coverage" "$COVERAGE_RESULT" ;;\n'
                '              *) echo "info: not requiring it" ;;\n'
                "            esac"
            ),
        ),
        (
            "sentinel reads a result it does not declare",
            "        TEST_RESULT: ${{ needs.test.result }}",
            "        TEST_RESULT: ${{ needs.not-a-real-job.result }}",
        ),
        (
            "a gate step left unguarded, so one red skips the rest of the job",
            (
                "        if: ${{ !cancelled() }}\n"
                "        run: bash scripts/ci/check_sqlx_dynamic_ratio.sh"
            ),
            "        run: bash scripts/ci/check_sqlx_dynamic_ratio.sh",
        ),
    ]
    failures = 0
    for title, needle, replacement in mutations:
        if needle not in text:
            print(
                f"❌ mutation '{title}': the anchor text is gone — update the mutation"
            )
            failures += 1
            continue
        mutated = text.replace(needle, replacement, 1)
        try:
            problems, _ = audit(mutated)
        except SystemExit as exc:
            problems = [str(exc)]
        if problems:
            print(f"✅ mutation '{title}' detected: {problems[0].splitlines()[0]}")
        else:
            print(f"❌ mutation '{title}' NOT detected — the scanner is blind here")
            failures += 1
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--print", dest="show", action="store_true", help="打印解析结果"
    )
    parser.add_argument(
        "--mutation-check", action="store_true", help="自证：注入缺陷并要求变红"
    )
    args = parser.parse_args()

    if not WORKFLOW.is_file():
        print(f"❌ {WORKFLOW} is missing", file=sys.stderr)
        return 2
    text = WORKFLOW.read_text(encoding="utf-8")
    try:
        layout = Layout(text)
        if args.show:
            for job_id, needs in layout.needs.items():
                print(f"{job_id:22s} needs={needs} name={layout.names.get(job_id)!r}")
            print("sentinel env:", sentinel_env(layout.sentinel_block()))
        problems, notes = audit(text, layout)
    except SystemExit as exc:
        print(f"❌ cannot parse ci.yml: {exc}", file=sys.stderr)
        return 2

    report(problems, notes, "ci.yml / CI Summary sentinel")
    rc = 1 if problems else 0
    if args.mutation_check:
        rc |= mutation_check(text)
    return rc


if __name__ == "__main__":
    sys.exit(main())
