#!/usr/bin/env python3
"""Extract declared route contract from synapse-rust source manifests (non-test code).

This is the *declared* side of the contract (RouteEntry / *_route_manifest), i.e. what the
route_ledger asserts at startup. It is emitted to artifacts/route_contract.json for auditing
and diffing against the *registered* surface (extract_registered.py).

Paths are resolved relative to the repo root (SYNAPSE_RUST_ROOT) so the script is portable in CI.
"""

import os, re, json

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.environ.get("SYNAPSE_RUST_ROOT") or os.path.dirname(
    os.path.dirname(SCRIPT_DIR)
)
ROUTES_DIR = os.environ.get("SYNAPSE_RUST_ROUTES") or os.path.join(
    REPO_ROOT, "src", "web", "routes"
)

re_new = re.compile(
    r'RouteEntry::new\(\s*Method::(\w+)\s*,\s*"([^"]+)"\s*,\s*([A-Za-z0-9_]+|"[^"]+")'
)
re_expand = re.compile(
    r'expand_under_prefixes\(\s*([A-Za-z0-9_]+|"[^"]+")\s*,\s*&\[(.*?)\]\s*,\s*&\[(.*?)\]',
    re.S,
)
re_const = re.compile(r'const\s+([A-Za-z0-9_]+)\s*:\s*&str\s*=\s*"([^"]+)"')
re_manifest_fn = re.compile(
    r"fn\s+\w*(?:route_manifest|manifest_for|assembly_compat_manifest|top_level_inline_manifest)\w*\s*\("
)


def unquote(s):
    s = s.strip()
    if s.startswith('"') and s.endswith('"'):
        return s[1:-1]
    return s


def parse_file(path):
    with open(path) as f:
        src = f.read()
    # only non-test code
    test_idx = src.find("#[cfg(test)]")
    if test_idx != -1:
        src = src[:test_idx]
    consts = {}
    for m in re_const.finditer(src):
        consts[m.group(1)] = m.group(2)
    has_manifest = bool(re_manifest_fn.search(src))
    entries = []
    for m in re_new.finditer(src):
        mod = m.group(3)
        mod = consts.get(mod, unquote(mod))
        entries.append((m.group(1), m.group(2), mod))
    for m in re_expand.finditer(src):
        mod = m.group(1)
        mod = consts.get(mod, unquote(mod))
        prefixes = [unquote(p) for p in re.findall(r'"([^"]+)"', m.group(2))]
        for pm in re.finditer(r'Method::(\w+)\s*,\s*"([^"]+)"', m.group(3)):
            meth, rel = pm.group(1), pm.group(2)
            for pfx in prefixes:
                full = pfx.rstrip("/") + rel if rel.startswith("/") else pfx + rel
                entries.append((meth, full, mod))
    return entries, has_manifest


def main():
    declared = []
    manifest_files = set()
    all_route_files = set()
    for dp, _, fns in os.walk(ROUTES_DIR):
        for f in fns:
            if not f.endswith(".rs"):
                continue
            fp = os.path.join(dp, f)
            rel = os.path.relpath(fp, ROUTES_DIR)
            if "tests" in rel.split(os.sep):
                continue
            entries, hm = parse_file(fp)
            if hm:
                manifest_files.add(rel)
            if entries:
                all_route_files.add(rel)
            declared += entries
    decl_set = {}
    for meth, path, mod in declared:
        decl_set.setdefault((meth, path), set()).add(mod)
    by_mod = {}
    for (meth, path), mods in decl_set.items():
        for mod in mods:
            by_mod.setdefault(mod, []).append((meth, path))
    report = {"declared_total": len(decl_set), "modules": {}}
    for mod in sorted(by_mod):
        report["modules"][mod] = sorted(by_mod[mod])
    os.makedirs(os.path.join(REPO_ROOT, "artifacts"), exist_ok=True)
    out_path = os.path.join(REPO_ROOT, "artifacts", "route_contract.json")
    with open(out_path, "w") as f:
        json.dump(report, f, indent=1, ensure_ascii=False)
    print(f"declared tuples: {len(decl_set)}")
    print(f"modules with manifests: {len(by_mod)}")
    print(f"files with manifest fn: {len(manifest_files)}")
    print(f"wrote {out_path}")
    print("\nAll modules:")
    for mod, cnt in sorted(
        ((m, len(v)) for m, v in by_mod.items()), key=lambda x: (-x[1], x[0])
    ):
        print(f"  {cnt:4d}  {mod}")
    # modules that register routes but no manifest fn
    print(f"\nFiles with routes but NO manifest fn (contract gap candidates):")
    gaps = sorted(all_route_files - manifest_files)
    for g in gaps:
        print(f"  {g}")


if __name__ == "__main__":
    main()
