#!/usr/bin/env python3
"""P5 fixture convergence — storage rewrite (robust).

Rewrite module-local plain `async fn test_pool()` fixtures to delegate to
`crate::test_utils::connect_shared_test_pool()`, then prune per-file imports
(`PgPoolOptions` / `env` / `Duration`) that are no longer referenced anywhere in
the file after the rewrite.

Isolated-schema pools (IsolatedTestPool / prepare_empty_isolated_test_pool /
prepare_isolated_test_pool) are skipped. Bare (non-Arc) return types get an
explicit `(*pool).clone()` so the call sites that use `&pool` / `&*pool` still
compile (sqlx::Pool is an internal Arc, so clone is cheap).

Default is --check (report only). Pass --apply to write.
"""
import re
import sys
import pathlib

ROOT = pathlib.Path("/Users/ljf/Desktop/hu_ts/synapse-rust")
BASE = ROOT / "synapse-storage/src"
HELPER = "crate::test_utils::connect_shared_test_pool"

FN_RE = re.compile(r"(?P<indent>[ \t]*)async fn test_pool\(\)\s*->\s*(?P<ret>[^{]+?)\s*\{")
ISO = ("IsolatedTestPool", "prepare_empty_isolated_test_pool", "prepare_isolated_test_pool")
# token -> (regex that counts real usages, use-line regex)
IMPORTS = {
    "PgPoolOptions": (r"\bPgPoolOptions\s*::", r"use\s+[^;]*\bPgPoolOptions\b[^;]*;"),
    "env": (r"\benv\s*::", r"use\s+std\s*::\s*env\s*;"),
    "Duration": (r"\bDuration\s*::", r"use\s+std\s*::\s*time\s*::\s*Duration\s*;"),
}


def brace_end(text, open_idx):
    depth = 0
    i = open_idx
    while i < len(text):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    raise ValueError("unbalanced")


def process(path, apply):
    text = path.read_text()
    m = FN_RE.search(text)
    if not m:
        return None
    start = m.start("indent")
    ob = text.index("{", m.start("ret"))
    end = brace_end(text, ob)
    body = text[start:end]
    if HELPER in body:
        return "ALREADY"
    if any(k in body for k in ISO):
        return "SKIP-ISO"
    if "TEST_DATABASE_URL" not in body or "PgPoolOptions" not in body:
        return "SKIP-OTHER"
    ret = " ".join(m.group("ret").split())
    ind = m.group("indent")
    i2 = ind + "    "
    msg = '"test database must be reachable - a swallowed error here surfaces later as an unrelated failure"'
    if ret.startswith("Arc<"):
        new = (f"{ind}async fn test_pool() -> {ret} {{\n"
               f"{i2}{HELPER}().await.expect({msg})\n"
               f"{ind}}}")
    else:
        new = (f"{ind}async fn test_pool() -> {ret} {{\n"
               f"{i2}let pool = {HELPER}().await.expect({msg});\n"
               f"{i2}(*pool).clone()\n"
               f"{ind}}}")
    nt = text[:start] + new + text[end:]

    # prune unused imports
    pruned = []
    lines = nt.splitlines(keepends=False)
    out_lines = []
    for ln in lines:
        should_drop = False
        for tok, (_, usere) in IMPORTS.items():
            if re.search(usere, ln):
                remain = "".join(l for l in lines if l is not ln)
                if not re.search(IMPORTS[tok][0], remain):
                    should_drop = True
                    pruned.append(ln.strip())
                    break
        if not should_drop:
            out_lines.append(ln)
    nt = "\n".join(out_lines) + "\n"
    if apply:
        path.write_text(nt)
    return ("OK" if apply else "WOULD", ret, pruned)


def main():
    apply = "--apply" in sys.argv
    ok = other = 0
    for path in sorted(BASE.rglob("*.rs")):
        if path.name in ("test_utils.rs", "test_isolation.rs"):
            continue
        if "async fn test_pool()" not in path.read_text():
            continue
        r = process(path, apply)
        if r in ("SKIP-ISO", "SKIP-OTHER", "ALREADY", None):
            other += 1
            print(f"  {r:10} {path.relative_to(ROOT)}")
        else:
            status, ret, pruned = r
            ok += 1
            e = f"  prune={pruned}" if pruned else ""
            print(f"  {status} ({ret:30}) {path.relative_to(ROOT)}{e}")
    print(f"\n{ok} targeted, {other} skipped")


if __name__ == "__main__":
    main()