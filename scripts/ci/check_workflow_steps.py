#!/usr/bin/env python3
"""CI gate: 每个 GitHub Actions step 必须真的有 `run:` 或 `uses:`。

── 为什么需要这个门禁 ────────────────────────────────────────────────────────

GitHub Actions 的 step schema 要求 `run` 与 `uses` 二者恰有其一。缺了两者，
**整个 workflow 文件校验失败**（"Required property is missing: run"），于是该
workflow 里所有 job、所有步骤一起不生效 —— 包括那些本来写对了的步骤。

这不是假想问题：`db-migration-gate.yml` 在 2026-09-15 曾经真实处于这个状态。
Phase 0 的意图是正确的（把 22 个 `run: echo "..."` 恒绿占位门禁删掉），但做法是
只删 `run:` 而保留 `- name:` 骨架，于是把"空转门禁"升级成了"非法 workflow"。
即：一个补丁让门禁**更**不可信，而任何本地 `python3 -c "yaml.safe_load(...)"`
都发现不了（纯注释在 YAML 里完全合法）。

── 判据 ─────────────────────────────────────────────────────────────────────

遍历 `.github/workflows/*.yml|yaml`，对每个 job 的每个 step 断言
`'run' in step or 'uses' in step`。同时拦截另外两种同型退化：
  * step 不是 mapping（写成裸字符串）
  * `run:` 的值去掉空白后为空（等价于原来的 `echo` 占位）

── 用法 ─────────────────────────────────────────────────────────────────────

    python3 scripts/ci/check_workflow_steps.py            # 退出码 0/1
    python3 scripts/ci/check_workflow_steps.py --json-report artifacts/workflow_steps.json

── 自证能变红 ───────────────────────────────────────────────────────────────

按铁律 8，本脚本必须用故意违规证明会失败。在任一 workflow 里插入：

    - name: probe
      # no run/uses here

→ 脚本报 "missing run/uses" 并 EXIT=1；移除后恢复 EXIT=0。
"""

from __future__ import annotations

import argparse
import glob
import json
import sys

try:
    import yaml
except ImportError:  # pragma: no cover - CI 环境已装 pyyaml
    print("FAIL: PyYAML is required (pip install pyyaml)", file=sys.stderr)
    sys.exit(2)

WORKFLOW_GLOBS = (".github/workflows/*.yml", ".github/workflows/*.yaml")


def check_file(path: str) -> list[str]:
    """返回该 workflow 的违规描述列表（空表示合规）。"""
    problems: list[str] = []
    try:
        with open(path, encoding="utf-8") as handle:
            doc = yaml.safe_load(handle)
    except yaml.YAMLError as error:
        problems.append(f"{path}: YAML 解析失败: {error}")
        return problems

    if not isinstance(doc, dict):
        # 空文件 / 非 mapping：交给别的门禁处理，这里不重复报。
        return problems

    jobs = doc.get("jobs") or {}
    if not isinstance(jobs, dict):
        problems.append(f"{path}: `jobs` 不是 mapping")
        return problems

    for job_name, job in jobs.items():
        if not isinstance(job, dict):
            problems.append(f"{path}: job `{job_name}` 不是 mapping")
            continue
        steps = job.get("steps")
        if steps is None:
            # 复用型 job（`uses:`）本来就没有 steps。
            continue
        if not isinstance(steps, list):
            problems.append(f"{path}: job `{job_name}` 的 `steps` 不是 list")
            continue

        for index, step in enumerate(steps):
            where = f"{path}: job `{job_name}` step#{index}"
            if not isinstance(step, dict):
                problems.append(f"{where}: step 不是 mapping（必须含 run 或 uses）")
                continue
            name = step.get("name")
            label = f"{where} ({name!r})" if name else where

            has_run = "run" in step
            has_uses = "uses" in step
            if not has_run and not has_uses:
                problems.append(
                    f"{label}: 缺 `run:` 或 `uses:` —— 这会让**整个 workflow 文件**"
                    f"校验失败，该 workflow 的所有步骤一起不生效"
                )
                continue

            if has_run:
                body = step.get("run")
                if not isinstance(body, str) or not body.strip():
                    problems.append(
                        f"{label}: `run:` 为空 —— 恒绿占位门禁等同于没有门禁"
                    )

    return problems


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Assert every Actions step has run/uses."
    )
    parser.add_argument("--json-report", help="可选：把结果写成 JSON 供 CI 归档")
    args = parser.parse_args()

    files = sorted({path for pattern in WORKFLOW_GLOBS for path in glob.glob(pattern)})
    if not files:
        print("FAIL: 未找到任何 workflow 文件（路径或工作目录错误）", file=sys.stderr)
        return 2

    problems: list[str] = []
    for path in files:
        problems.extend(check_file(path))

    if args.json_report:
        with open(args.json_report, "w", encoding="utf-8") as handle:
            json.dump(
                {
                    "checked_files": files,
                    "violations": problems,
                    "count": len(problems),
                },
                handle,
                ensure_ascii=False,
                indent=2,
            )

    print(f"==> check_workflow_steps: 检查了 {len(files)} 个 workflow 文件")
    if problems:
        print(f"FAIL: {len(problems)} 处违规")
        for problem in problems:
            print(f"  - {problem}")
        return 1

    print("OK: 所有 step 都带 `run:`/`uses:`，且没有空 `run:`")
    return 0


if __name__ == "__main__":
    sys.exit(main())
