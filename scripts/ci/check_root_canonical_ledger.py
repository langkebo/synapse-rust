#!/usr/bin/env python3
"""
Root/canonical overlap ledger for P1-03.

This script inventories overlapping Rust source files between the root crate and
the canonical crates, classifies the root side as a thin facade or a real
implementation, and hard-fails if the service layer re-introduces the forbidden
`pub use crate::storage::*` glob export.
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import Counter
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SERVICE_ROOT = REPO_ROOT / "src" / "services"
CANONICAL_SERVICE_ROOT = REPO_ROOT / "synapse-services" / "src"
STORAGE_ROOT = REPO_ROOT / "src" / "storage"
CANONICAL_STORAGE_ROOT = REPO_ROOT / "synapse-storage" / "src"

FORBIDDEN_STORAGE_GLOB = re.compile(r"pub\s+use\s+crate::storage::\*")
FACADE_EXPORT = re.compile(r"pub\s+use\s+synapse_(services|storage|common|cache|e2ee|federation)[^;]*;")
ATTRIBUTE_LINE = re.compile(r"^\s*#(!)?\[[^\]]+\]\s*$")
LINE_COMMENT = re.compile(r"//.*$")


def find_overlaps(root_dir: Path, canonical_dir: Path) -> list[str]:
    root_files = {
        str(path.relative_to(root_dir)).replace("\\", "/")
        for path in root_dir.rglob("*.rs")
        if path.is_file()
    }
    canonical_files = {
        str(path.relative_to(canonical_dir)).replace("\\", "/")
        for path in canonical_dir.rglob("*.rs")
        if path.is_file()
    }
    return sorted(root_files & canonical_files)


def strip_cfg_test_modules(source: str) -> str:
    lines = source.splitlines()
    kept: list[str] = []
    pending_cfg_test = False
    skipping_test_module = False
    brace_depth = 0

    for line in lines:
        stripped = line.strip()

        if skipping_test_module:
            brace_depth += line.count("{")
            brace_depth -= line.count("}")
            if brace_depth <= 0:
                skipping_test_module = False
            continue

        if stripped.startswith("#[cfg(test)]"):
            pending_cfg_test = True
            continue

        if pending_cfg_test and re.match(r"^\s*mod\s+\w+\s*\{", line):
            skipping_test_module = True
            brace_depth = line.count("{") - line.count("}")
            pending_cfg_test = False
            continue

        pending_cfg_test = False
        kept.append(line)

    return "\n".join(kept)


def normalize_source(source: str) -> str:
    filtered_lines: list[str] = []
    for line in strip_cfg_test_modules(source).splitlines():
        if ATTRIBUTE_LINE.match(line):
            continue
        line = LINE_COMMENT.sub("", line).strip()
        if not line:
            continue
        filtered_lines.append(line)
    return "".join(filtered_lines)


def classify_root_file(file_path: Path) -> str:
    normalized = normalize_source(file_path.read_text(encoding="utf-8"))
    if not normalized:
        return "empty"
    exports = FACADE_EXPORT.findall(normalized)
    if exports:
        stripped = FACADE_EXPORT.sub("", normalized)
        if not stripped:
            return "thin_facade"
    return "full_impl"


def collect_layer_summary(layer_name: str, root_dir: Path, canonical_dir: Path) -> tuple[list[str], Counter[str]]:
    overlaps = find_overlaps(root_dir, canonical_dir)
    categories: Counter[str] = Counter()
    for rel_path in overlaps:
        category = classify_root_file(root_dir / rel_path)
        categories[category] += 1

    print(f"[ledger] {layer_name}: overlapping Rust files = {len(overlaps)}")
    for category in ("thin_facade", "full_impl", "empty"):
        if categories[category]:
            print(f"[ledger]   - {category}: {categories[category]}")

    full_impl_paths = [path for path in overlaps if classify_root_file(root_dir / path) == "full_impl"]
    if full_impl_paths:
        print(f"[ledger]   - full_impl sample ({min(20, len(full_impl_paths))}/{len(full_impl_paths)}):")
        for rel_path in full_impl_paths[:20]:
            print(f"[ledger]     * {layer_name}/{rel_path}")

    return overlaps, categories


def find_storage_glob_violations() -> list[str]:
    violations: list[str] = []
    for path in SERVICE_ROOT.rglob("*.rs"):
        content = path.read_text(encoding="utf-8")
        if FORBIDDEN_STORAGE_GLOB.search(content):
            violations.append(str(path.relative_to(REPO_ROOT)).replace("\\", "/"))
    return sorted(violations)


def main() -> int:
    parser = argparse.ArgumentParser(description="Inventory root/canonical overlap debt and guard service storage re-export leaks.")
    parser.add_argument(
        "--fail-on-full-impl",
        action="store_true",
        help="Fail if any overlapping file on the root side is still classified as a full implementation.",
    )
    args = parser.parse_args()

    print("[ledger] root/canonical overlap inventory")
    service_overlaps, service_categories = collect_layer_summary("services", SERVICE_ROOT, CANONICAL_SERVICE_ROOT)
    storage_overlaps, storage_categories = collect_layer_summary("storage", STORAGE_ROOT, CANONICAL_STORAGE_ROOT)

    violations = find_storage_glob_violations()
    if violations:
        print("[ledger] FAIL: forbidden `pub use crate::storage::*` found in service layer", file=sys.stderr)
        for rel_path in violations:
            print(f"[ledger]   * {rel_path}", file=sys.stderr)
        return 1

    print("[ledger] storage glob gate: OK")
    print(
        "[ledger] summary: "
        f"services={len(service_overlaps)} "
        f"(facade={service_categories['thin_facade']}, full_impl={service_categories['full_impl']}), "
        f"storage={len(storage_overlaps)} "
        f"(facade={storage_categories['thin_facade']}, full_impl={storage_categories['full_impl']})"
    )

    if args.fail_on_full_impl and (service_categories["full_impl"] or storage_categories["full_impl"]):
        print("[ledger] FAIL: full_impl overlaps remain", file=sys.stderr)
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
