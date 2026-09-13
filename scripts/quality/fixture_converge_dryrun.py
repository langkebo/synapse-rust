#!/usr/bin/env python3
"""Dry-run classifier for P5 fixture convergence.

Identifies module-local `async fn test_pool()` fixtures that are plain shared
connection pools (hard-coded TEST_DATABASE_URL fallback + PgPoolOptions) and
would be converted to delegate to a shared helper. Isolated-schema pools
(IsolatedTestPool / prepare_empty_isolated_test_pool / prepare_isolated_test_pool)
are reported but NOT targeted.

Run with --apply to actually rewrite. Default is dry-run (report only).
"""
import re
import sys
import pathlib

ROOT = pathlib.Path("/Users/ljf/Desktop/hu_ts/synapse-rust")
SCANS = [ROOT / "synapse-storage/src", ROOT / "synapse-services/src"]

ISO_MARKERS = ("IsolatedTestPool", "prepare_empty_isolated_test_pool", "prepare_isolated_test_pool")

# Match `async fn test_pool() -> RETYPE {` ... up to the matching closing brace
# at the same indentation. We use a brace counter instead.
FN_RE = re.compile(r"async fn test_pool\(\)\s*->\s*([^{]+?)\s*\{")


def find_fn_bodies(text):
    """Yield (start, end, rettype) spans for each test_pool function."""
    for m in FN_RE.finditer(text):
        rettype = m.group(1).strip()
        # brace-match from the opening brace m.end()-1
        i = m.end() - 1
        depth = 0
        while i < len(text):
            c = text[i]
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
                if depth == 0:
                    break
            i += 1
        yield m.start(), i + 1, rettype


def classify():
    targets = []   # (path, rettype, has_iso)
    iso = []       # skipped: isolated pools
    other = []     # skipped: does not look like a plain shared pool
    for base in SCANS:
        for path in sorted(base.rglob("*.rs")):
            text = path.read_text()
            if "async fn test_pool()" not in text:
                continue
            for start, end, rettype in find_fn_bodies(text):
                body = text[start:end]
                is_iso = any(mk in body for mk in ISO_MARKERS)
                looks_plain = ("TEST_DATABASE_URL" in body and "PgPoolOptions" in body and not is_iso)
                rec = (str(path.relative_to(ROOT)), rettype)
                if is_iso:
                    iso.append(rec)
                elif looks_plain:
                    targets.append(rec)
                else:
                    other.append((rec[0], rec[1], body[:80]))
    return targets, iso, other


def main():
    apply = "--apply" in sys.argv
    targets, iso, other = classify()
    print(f"=== PLAIN SHARED POOLS (target: {len(targets)}) ===")
    arcount = {}
    for p, r in targets:
        arcount[r] = arcount.get(r, 0) + 1
        print(f"  {p}  ->  {r}")
    print("\n--- return-type histogram (targets) ---")
    for r, c in sorted(arcount.items(), key=lambda x: -x[1]):
        print(f"  {c:3d}  {r}")
    print(f"\n=== ISOLATED POOLS (skip: {len(iso)}) ===")
    for p, r in iso:
        print(f"  {p}  ->  {r}")
    print(f"\n=== OTHER / unrecognised (skip: {len(other)}) ===")
    for o in other:
        print(f"  {o[0]}  ->  {o[1]}   head={o[2]!r}")
    if apply:
        print("\n[apply mode not implemented in dry-run script]")


if __name__ == "__main__":
    main()
