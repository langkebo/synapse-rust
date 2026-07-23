#!/usr/bin/env python3
"""Parse tarpaulin JSON report and print coverage summary."""
import json
import sys
from collections import defaultdict

report_path = sys.argv[1] if len(sys.argv) > 1 else "coverage/tarpaulin-report.json"

with open(report_path) as f:
    data = json.load(f)

covered = data.get("covered", 0)
coverable = data.get("coverable", 0)
rate = covered / max(coverable, 1)
print(f"=== Overall Coverage ===")
print(f"  {covered}/{coverable} = {rate:.2%}")
print(f"  Files: {len(data.get('files', []))}")
print()

files = data.get("files", [])

def get_path(e):
    p = e.get("path", "")
    if isinstance(p, list):
        return "/".join(p)
    return str(p)

def is_test_file(path):
    return "/tests/" in path or path.endswith("_test.rs") or "/test_" in path

# Split into source vs test files
src_files = []
test_files = []
for e in files:
    path = get_path(e)
    if is_test_file(path):
        test_files.append(e)
    else:
        src_files.append(e)

src_covered = sum(e.get("covered", 0) for e in src_files)
src_coverable = sum(e.get("coverable", 0) for e in src_files)
test_covered = sum(e.get("covered", 0) for e in test_files)
test_coverable = sum(e.get("coverable", 0) for e in test_files)

print(f"=== Source vs Test ===")
print(f"  Source: {src_covered}/{src_coverable} = {src_covered/max(src_coverable,1):.2%}  ({len(src_files)} files)")
print(f"  Test:   {test_covered}/{test_coverable} = {test_covered/max(test_coverable,1):.2%}  ({len(test_files)} files)")
print()

# Group by crate
crate_stats = defaultdict(lambda: {"covered": 0, "coverable": 0, "files": 0})
for e in files:
    path = get_path(e)
    fc = e.get("covered", 0)
    fcb = e.get("coverable", 0)

    parts = path.split("/")
    crate = "root"
    for i, p in enumerate(parts):
        if p == "src" and i > 0:
            crate = parts[i - 1]
            break
        if p.startswith("synapse-"):
            crate = p

    crate_stats[crate]["covered"] += fc
    crate_stats[crate]["coverable"] += fcb
    crate_stats[crate]["files"] += 1

print("=== Coverage by crate ===")
for crate, stats in sorted(crate_stats.items(), key=lambda x: x[1]["coverable"], reverse=True):
    cr = stats["covered"] / max(stats["coverable"], 1)
    print(f"  {crate:40s} {stats['covered']:6d}/{stats['coverable']:6d} = {cr:6.1%}  ({stats['files']} files)")

print()
print("=== Lowest 20 source files (coverable >= 30) ===")
eligible = [e for e in src_files if e.get("coverable", 0) >= 30]
for e in sorted(eligible, key=lambda x: x.get("covered", 0) / max(x.get("coverable", 1), 1))[:20]:
    path = get_path(e)
    fc = e.get("covered", 0)
    fcb = e.get("coverable", 0)
    cr = fc / max(fcb, 1)
    # Shorten path for display
    short = path.split("synapse-rust/")[-1] if "synapse-rust/" in path else path
    print(f"  {cr:6.1%}  {fc:4d}/{fcb:4d}  {short}")

print()
print("=== Highest 10 source files (coverable >= 30) ===")
for e in sorted(eligible, key=lambda x: x.get("covered", 0) / max(x.get("coverable", 1), 1))[-10:]:
    path = get_path(e)
    fc = e.get("covered", 0)
    fcb = e.get("coverable", 0)
    cr = fc / max(fcb, 1)
    short = path.split("synapse-rust/")[-1] if "synapse-rust/" in path else path
    print(f"  {cr:6.1%}  {fc:4d}/{fcb:4d}  {short}")
