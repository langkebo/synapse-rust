#!/usr/bin/env python3
"""Config consistency gate: keep the two config trees semantically in sync.

## Why

The repo ships two config trees:

  docker/deploy/config/   used by docker/deploy/docker-compose.yml (deployment)
  docker/config/          used by docker/docker-compose.yml (dev) + baked into the image

They are kept in sync **by hand**.  docker/deploy/README.md documents the
convention ("修改 canonical 后需同步 cp docker/config/<file> config/<file>"),
but nothing enforces it: deploy.sh does not copy and no CI step compares.

That is the *same* failure mode the migrations duplicate directory had -- a
hand-synced copy that silently drifted.  Migrations were fixed in 2b16dc3c
(single source + check_migration_consistency.py blocking in CI); configs were not.

Concrete harm: on 2026-09-11 the two rate_limit.yaml files disagreed on
`sync.enabled` (false vs true) while both homeserver.yaml files declared true.
Because the file config replaces the whole `rate_limit:` section, the file won
and /sync ended up with **no** rate limiting at all (120/120 requests -> 200).
See docs/audit/S_series_verification_2026-09-11.md section 2.

## What it checks

For each config pair, compares **semantic** content: comment-only and
blank-line-only differences are ignored (the two trees legitimately carry
different explanatory comments).  Any remaining difference must be registered in
ALLOWED_DIFFERENCES below, with a reason.

Unknown/dangling config files on either side are reported too: a new file added
to only one tree is almost always a mistake.

Usage:
    python3 scripts/check_config_consistency.py
    python3 scripts/check_config_consistency.py --explain
    python3 scripts/check_config_consistency.py --json-report artifacts/config_consistency.json

Exits 0 when consistent, 1 otherwise.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


# ---------------------------------------------------------------------------
# Config trees
# ---------------------------------------------------------------------------

DEV_DIR = Path("docker/config")
DEPLOY_DIR = Path("docker/deploy/config")

#: Files compared across both trees.
COMPARED_FILES = ("homeserver.yaml", "rate_limit.yaml", "postgres.conf")


# ---------------------------------------------------------------------------
# Intentional differences
# ---------------------------------------------------------------------------
#
# Every entry is (file, yaml-ish key) -> reason.  Keys are compared as
# "section.subsection.key" leaf paths so a difference must be named precisely;
# a blanket "whole file differs" allowlist would defeat the gate.
#
# Adding an entry is a deliberate act that shows up in review. If you are here
# because CI failed: prefer syncing the two files. Only allowlist when the
# difference is *intentional* dev-vs-prod behaviour, and say why.

ALLOWED_DIFFERENCES: dict[tuple[str, str], str] = {
    (
        "rate_limit.yaml",
        "sync.enabled",
    ): (
        "开发环境刻意关闭专用 /sync 限流，避免本地反复登录/压测时吃假阳性 429；"
        "生产（deploy）必须为 true —— /sync 在路由 ledger 中被标记 rate_limit_exempt，"
        "不受通用 IP 限流约束，本段是它唯一的限流来源。"
        "曾因两侧不一致导致 /sync 零限流（见 S_series_verification_2026-09-11.md §2）。"
    ),
}


def strip_comments(text: str) -> str:
    """Remove full-line and trailing comments plus blank lines.

    Deliberately simple: these configs do not contain '#' inside string values.
    """
    out: list[str] = []
    for raw in text.splitlines():
        line = raw.split("#", 1)[0].rstrip()
        if line.strip():
            out.append(line)
    return "\n".join(out)


def leaf_paths(text: str) -> dict[str, str]:
    """Flatten a simple nested-YAML document into {dotted.path: value}.

    Only handles the mapping/list subset used by these config files: no anchors,
    no multi-line scalars, no flow collections spanning comments.  List items are
    keyed by their own scalar value (e.g. ``endpoints[]=/_matrix/...``) so that
    reordering entries is not silently ignored.
    """
    result: dict[str, str] = {}
    stack: list[tuple[int, str]] = []  # (indent, key)
    pending_item = 0

    for raw in strip_comments(text).splitlines():
        if not raw.strip():
            continue
        indent = len(raw) - len(raw.lstrip())
        body = raw.strip()

        while stack and stack[-1][0] >= indent:
            stack.pop()

        if body.startswith("- "):
            # A list entry; key it positionally under the current prefix.
            prefix = ".".join(k for _, k in stack)
            key = f"{prefix}[{pending_item}]"
            pending_item += 1
            rest = body[2:].strip()
            if ":" in rest:
                k, _, v = rest.partition(":")
                result[f"{key}.{k.strip()}"] = v.strip()
                stack.append((indent, f"[{pending_item - 1}].{k.strip()}"))
            else:
                result[key] = rest
            continue

        pending_item = 0
        if ":" not in body:
            continue
        key, _, value = body.partition(":")
        key = key.strip()
        value = value.strip()
        prefix = ".".join(k for _, k in stack)
        full = f"{prefix}.{key}" if prefix else key
        if value:
            result[full] = value
        else:
            stack.append((indent, key))
    return result


def allowed_key(file_name: str, path: str) -> str | None:
    """Returns the ALLOWED_DIFFERENCES key matching `path`, if any.

    Matches on the leaf suffix so that structural prefixes introduced by the
    flattener (list indices) do not defeat the allowlist.
    """
    for (f, key) in ALLOWED_DIFFERENCES:
        if f != file_name:
            continue
        if path == key or path.endswith("." + key):
            return key
    return None


def compare_file(name: str, dev_path: Path, deploy_path: Path) -> tuple[list[str], list[str], list[str]]:
    """Returns (issues, warnings, explained_differences)."""
    issues: list[str] = []
    warnings: list[str] = []
    explained: list[str] = []

    dev = leaf_paths(dev_path.read_text(encoding="utf-8"))
    deploy = leaf_paths(deploy_path.read_text(encoding="utf-8"))

    for path in sorted(set(dev) | set(deploy)):
        if dev.get(path) == deploy.get(path):
            continue
        key = allowed_key(name, path)
        detail = f"{name}: {path}: dev={dev.get(path)!r} deploy={deploy.get(path)!r}"
        if key:
            explained.append(f"{detail}  [允许: {ALLOWED_DIFFERENCES[(name, key)][:60]}...]")
        else:
            issues.append(detail)

    # A file present on only one side is nearly always an oversight.
    dev_files = {p.name for p in DEV_DIR.iterdir() if p.is_file()}
    deploy_files = {p.name for p in DEPLOY_DIR.iterdir() if p.is_file()}
    for only in sorted(dev_files - deploy_files):
        warnings.append(f"{only}: 只存在于 {DEV_DIR}，不在 {DEPLOY_DIR}")
    for only in sorted(deploy_files - dev_files):
        warnings.append(f"{only}: 只存在于 {DEPLOY_DIR}，不在 {DEV_DIR}")

    return issues, warnings, explained


def main() -> int:
    parser = argparse.ArgumentParser(description="Assert the two config trees agree semantically.")
    parser.add_argument("--json-report", default=None, help="write a JSON report to this path")
    parser.add_argument("--explain", action="store_true", help="print per-file comparison detail")
    args = parser.parse_args()

    if not DEV_DIR.is_dir() or not DEPLOY_DIR.is_dir():
        print(f"ERROR: 找不到配置目录 {DEV_DIR} 或 {DEPLOY_DIR}", file=sys.stderr)
        return 1

    all_issues: list[str] = []
    all_warnings: list[str] = []
    all_explained: list[str] = []
    compared: list[str] = []

    for name in COMPARED_FILES:
        dev_path = DEV_DIR / name
        deploy_path = DEPLOY_DIR / name
        if not dev_path.is_file() or not deploy_path.is_file():
            all_issues.append(f"{name}: 缺少文件 (dev={dev_path.is_file()}, deploy={deploy_path.is_file()})")
            continue
        compared.append(name)
        issues, warnings, explained = compare_file(name, dev_path, deploy_path)
        all_issues.extend(issues)
        all_warnings.extend(warnings)
        all_explained.extend(explained)

    print(f"check_config_consistency: compared={compared}")
    print(f"check_config_consistency: allowed_differences={len(ALLOWED_DIFFERENCES)}")
    print(f"check_config_consistency: intentional_diffs_found={len(all_explained)}")
    if args.explain:
        for line in all_explained:
            print(f"  aligned-diff: {line}")
    for line in all_warnings:
        print(f"  warning: {line}")
    for line in all_issues:
        print(f"  issue: {line}", file=sys.stderr)

    if args.json_report:
        report = {
            "compared": compared,
            "allowed_differences": [f"{f}:{k}" for (f, k) in ALLOWED_DIFFERENCES],
            "intentional_differences": all_explained,
            "warnings": all_warnings,
            "issues": all_issues,
        }
        Path(args.json_report).parent.mkdir(parents=True, exist_ok=True)
        Path(args.json_report).write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")

    if all_issues:
        print(
            "check_config_consistency: FAIL\n"
            "  两份配置出现语义漂移。首选做法是同步：\n"
            f"      cp {DEV_DIR}/<file> {DEPLOY_DIR}/<file>\n"
            "  若确属有意的开发/生产差异，请登记进 ALLOWED_DIFFERENCES 并写明理由。",
            file=sys.stderr,
        )
        return 1

    print("check_config_consistency: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
