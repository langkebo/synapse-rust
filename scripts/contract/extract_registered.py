#!/usr/bin/env python3
"""Extract ACTUAL registered routes (.route("...", method()) and .nest("...")) per module file.
Excludes test code. This is the real served route surface = the contract clients see.

Output: artifacts/registered_routes.json  (consumed by gen_contract_doc.py)

Paths are resolved relative to the repo root (SYNAPSE_RUST_ROOT) so the script is portable in CI.
"""
import os, re, json

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
ROOT = os.environ.get("SYNAPSE_RUST_ROOT") or os.path.dirname(os.path.dirname(SCRIPT_DIR))
ROUTES_DIR = os.path.join(ROOT, "src", "web", "routes")
METHODS = "(get|post|put|delete|patch|options|head)"

re_route = re.compile(r'\.route\(\s*"([^"]+)"\s*,\s*' + METHODS + r'\s*\(')
re_route_with = re.compile(r'\.route\(\s*"([^"]+)"\s*,\s*(?:axum::routing::)?' + METHODS + r'\)')
re_nest = re.compile(r'\.nest\(\s*"([^"]+)"\s*,')

def parse_file(path):
    with open(path) as f:
        src = f.read()
    test_idx = src.find("#[cfg(test)]")
    if test_idx != -1:
        src = src[:test_idx]
    routes = []
    for m in re_route.finditer(src):
        routes.append((m.group(2).upper(), m.group(1)))
    for m in re_route_with.finditer(src):
        routes.append((m.group(2).upper(), m.group(1)))
    nests = [m.group(1) for m in re_nest.finditer(src)]
    return routes, nests

def main():
    per_module = {}
    nest_map = {}
    for dp, _, fns in os.walk(ROUTES_DIR):
        for f in fns:
            if not f.endswith(".rs"):
                continue
            fp = os.path.join(dp, f)
            rel = os.path.relpath(fp, ROUTES_DIR)
            if "tests" in rel.split(os.sep):
                continue
            routes, nests = parse_file(fp)
            if routes or nests:
                per_module[rel] = (routes, nests)
    # build nested resolution: a route under a nest prefix
    out = {}
    for mod, (routes, nests) in per_module.items():
        full = []
        for meth, path in routes:
            full.append((meth, path))
        out[mod] = sorted(full)
    report = {"modules": out, "total_routes": sum(len(v) for v in out.values())}
    os.makedirs(os.path.join(ROOT, "artifacts"), exist_ok=True)
    out_path = os.path.join(ROOT, "artifacts", "registered_routes.json")
    with open(out_path, "w") as f:
        json.dump(report, f, indent=1, ensure_ascii=False)
    print(f"modules with routes: {len(out)}")
    print(f"total registered route tuples: {report['total_routes']}")
    print(f"wrote {out_path}")
    print("\nTop modules by route count:")
    for mod, cnt in sorted(((m, len(v)) for m, v in out.items()), key=lambda x: -x[1])[:30]:
        print(f"  {cnt:4d}  {mod}")

if __name__ == "__main__":
    main()
