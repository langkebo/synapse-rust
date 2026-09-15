#!/usr/bin/env python3
"""Extract the ACTUAL served route surface from `src/web/routes/**`.

This is the contract's source-of-truth extractor. A naive line scan is not
enough — this module resolves, in one pass:

* **Every method in a chained handler list.** `.route(p, get(a).put(b).delete(c))`
  yields GET, PUT *and* DELETE. The previous implementation recorded only the
  first one, which silently under-reported the surface (see S-13).
* **`.nest("/prefix", expr)` prefix propagation**, transitively, including
  local variables, same-file router-builder functions, cross-file router-builder
  functions (e.g. `space.rs` nesting `space/children_hierarchy.rs`) and
  `.merge(...)` chains.
* **Multi-line registrations.** `.route(\n  "/path",\n  get(h).put(h2),\n)` is
  the dominant formatting style in this repo.

Outputs (both under `artifacts/`, which is git-ignored):

* `registered_routes.json` — the router-derived surface, attributed to the file
  that owns the path **literal**. Consumed by `gen_contract_doc.py`.
* `manifest_routes.json` — the surface declared by the `*_route_manifest()`
  functions. Used as an **independent oracle**: the manifests are written in
  absolute-path form via `expand_under_prefixes`, so they do not depend on this
  script's nest resolution at all. Comparing the two is a real cross-check of
  the resolver, not a self-proving assertion.

Paths are resolved relative to the repo root (`SYNAPSE_RUST_ROOT`) so the script
is portable in CI.
"""

from __future__ import annotations

import json
import os
import re
import sys
from collections import defaultdict

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
ROOT = os.environ.get("SYNAPSE_RUST_ROOT") or os.path.dirname(os.path.dirname(SCRIPT_DIR))
ROUTES_DIR = os.path.join(ROOT, "src", "web", "routes")
# `registered_by` overrides — the routes whose ledger origin is not derivable
# from their file path. See `load_ledger_origins`.
LEDGER_ORIGINS = os.path.join(SCRIPT_DIR, "ledger_origins.txt")

HTTP_METHODS = ("get", "post", "put", "delete", "patch", "head", "options")
HTTP_METHODS_SET = set(HTTP_METHODS)


# --------------------------------------------------------------------------
# Lexing helpers
# --------------------------------------------------------------------------


def strip_comments(src: str) -> str:
    """Remove `//` and `/* */` comments, preserving string and char literals.

    Rust comments can contain `"` and `.route(` text (doc comments in this repo
    do exactly that), so comment bodies must not reach the parser.
    """
    out = []
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        if c == '"':
            j = i + 1
            while j < n:
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] == '"':
                    break
                j += 1
            out.append(src[i : j + 1])
            i = j + 1
            continue
        if c == "'":
            # char literal (`'a'`, `'\n'`) vs lifetime (`'static`)
            if i + 1 < n and src[i + 1] == "\\":
                j = i + 2
                while j < n and src[j] != "'":
                    j += 1
                out.append(src[i : j + 1])
                i = j + 1
                continue
            if i + 2 < n and src[i + 2] == "'":
                out.append(src[i : i + 3])
                i += 3
                continue
            out.append(c)
            i += 1
            continue
        if c == "/" and i + 1 < n and src[i + 1] == "/":
            j = src.find("\n", i)
            i = n if j == -1 else j
            continue
        if c == "/" and i + 1 < n and src[i + 1] == "*":
            depth, j = 1, i + 2
            while j < n and depth:
                if src.startswith("/*", j):
                    depth += 1
                    j += 2
                elif src.startswith("*/", j):
                    depth -= 1
                    j += 2
                else:
                    j += 1
            i = j
            continue
        out.append(c)
        i += 1
    return "".join(out)


_CLOSER = {"(": ")", "[": "]", "{": "}"}


def match_delim(src: str, i: int) -> int:
    """Given `src[i]` is an opening delimiter, return the index of its match."""
    opener = src[i]
    closer = _CLOSER[opener]
    depth = 0
    n = len(src)
    while i < n:
        c = src[i]
        if c == '"':
            i += 1
            while i < n:
                if src[i] == "\\":
                    i += 2
                    continue
                if src[i] == '"':
                    break
                i += 1
            i += 1
            continue
        if c == "'":
            if i + 1 < n and src[i + 1] == "\\":
                i += 2
                while i < n and src[i] != "'":
                    i += 1
                i += 1
                continue
            if i + 2 < n and src[i + 2] == "'":
                i += 3
                continue
            i += 1
            continue
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return -1


def split_top_level(text: str, sep: str = ",") -> list[str]:
    """Split `text` on `sep` occurring at delimiter depth 0."""
    parts, cur, depth = [], [], 0
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if c == '"':
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"':
                    break
                j += 1
            cur.append(text[i : j + 1])
            i = j + 1
            continue
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
        if c == sep and depth == 0:
            parts.append("".join(cur))
            cur = []
            i += 1
            continue
        cur.append(c)
        i += 1
    parts.append("".join(cur))
    return parts


_BLOCK_KEYWORDS = ("if", "match", "for", "while", "loop")


def split_statements(body: str) -> list[str]:
    """Split a Rust block body into statements at depth 0.

    Splits on `;`, and also right after a `}` that closes a *statement-level*
    block (`if` / `for` / `match` / `while` / `loop` / bare `{ ... }`). Without
    that second rule a trailing registration chain glued to a preceding `for`
    loop is swallowed whole:

        for module in route_modules() { router = module.merge_into(..); }
        router = router.merge(create_captcha_router(&state))   // <- 6 registrations

    `let x = if c { a } else { b };` is unaffected: the statement starts with
    `let`, not with a block keyword.
    """
    stmts, cur, depth = [], [], 0
    i, n = 0, len(body)
    while i < n:
        c = body[i]
        if c == '"':
            j = i + 1
            while j < n:
                if body[j] == "\\":
                    j += 2
                    continue
                if body[j] == '"':
                    break
                j += 1
            cur.append(body[i : j + 1])
            i = j + 1
            continue
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
            if depth == 0 and c == "}":
                cur.append(c)  # the closing brace belongs to the statement
                _preds, head = strip_leading_attrs("".join(cur))
                head = head.lstrip()
                if head.startswith("{") or re.match(r"(?:%s)\b" % "|".join(_BLOCK_KEYWORDS), head):
                    stmts.append("".join(cur))
                    cur = []
                    i += 1
                    # swallow the statement terminator, if any
                    while i < n and body[i] in " \t\n\r":
                        i += 1
                    if i < n and body[i] == ";":
                        i += 1
                    continue
        if depth == 0 and c == ";":
            stmts.append("".join(cur))
            cur = []
            i += 1
            continue
        cur.append(c)
        i += 1
    tail = "".join(cur).strip()
    if tail:
        stmts.append(tail)
    return [s for s in (s.strip() for s in stmts) if s]


# --------------------------------------------------------------------------
# Top-level item discovery
# --------------------------------------------------------------------------


def predicates_before(src: str, pos: int) -> list[str]:
    """`cfg(...)` bodies of the attributes immediately preceding `pos`.

    Walks *backwards* over `#[…]` attributes so a compile-time gate can be
    attached to the item it guards::

        #[cfg(feature = "voice-extended")]
        pub mod voice;

    Non-`cfg` attributes (`#[derive(..)]`, `#[allow(..)]`) are skipped without
    stopping the walk, so `#[cfg(feature = "x")] #[serde(..)] fn f()` still sees
    the gate. The walk stops at anything that is not an attribute, which is what
    keeps it from reaching across a `;` into the previous statement.
    """
    preds: list[str] = []
    i = pos
    while True:
        j = i
        while j > 0 and src[j - 1] in " \t\r\n":
            j -= 1
        if j < 2 or src[j - 1] != "]":
            return preds
        start = src.rfind("#[", max(0, j - 4096), j)
        if start == -1:
            return preds
        body = src[start + 2 : j - 1].strip()
        if body.startswith("cfg("):
            preds.append(body[4:-1])
        i = start


def strip_leading_attrs(text: str) -> tuple[list[str], str]:
    """Split leading `#[...]` / `///` attributes from a statement.

    Returns `(cfg_predicates, remainder)`.

    `#[allow(unused_mut)] let mut router = ...` must still be recognised as a
    `let`, and `#[cfg(feature = "voip-tracking")] { router = ... }` as a block.

    The predicates used to be dropped on the floor, which is exactly how
    `#[cfg(feature = "server-notifications")] { router = router.route(..) }`
    contributed its routes to *every* feature lane: the `#[cfg]` was parsed as
    noise and the block recursed unconditionally.
    """
    preds: list[str] = []
    s = text.lstrip()
    while True:
        if s.startswith("#["):
            close = match_delim(s, 1)
            if close == -1:
                return preds, s
            body = s[2:close].strip()
            if body.startswith("cfg("):
                preds.append(body[4:-1])
            s = s[close + 1 :].lstrip()
            continue
        if s.startswith("///") or s.startswith("//!"):
            nl = s.find("\n")
            if nl == -1:
                return preds, ""
            s = s[nl + 1 :].lstrip()
            continue
        return preds, s


def extract_blocks(text: str) -> list[str]:
    """Return the inner text of every top-level `{ ... }` block in `text`."""
    out, i, n = [], 0, len(text)
    while i < n:
        c = text[i]
        if c == '"':
            i += 1
            while i < n:
                if text[i] == "\\":
                    i += 2
                    continue
                if text[i] == '"':
                    break
                i += 1
            i += 1
            continue
        if c == "{":
            end = match_delim(text, i)
            if end == -1:
                break
            out.append(text[i + 1 : end])
            i = end + 1
            continue
        i += 1
    return out


def strip_test_mods(src: str) -> str:
    """Excise every `#[cfg(test)] mod ... { ... }` block from `src`.

    Truncating at the first `#[cfg(test)]` is not safe: some files declare a
    nested test module *before* the production code
    (`space/lifecycle_query.rs` has `mod cursor_tests` at line 19 and the router
    builder at line 200), so truncation would silently delete real routes.
    """
    out = src
    while True:
        m = re.search(r"#\[cfg\(test\)\]", out)
        if not m:
            return out
        j = m.end()
        while True:
            mm = re.match(r"\s*(#\[[^\]]*\]|///[^\n]*|//![^\n]*)\s*", out[j:])
            if not mm:
                break
            j += mm.end()
        mm = re.match(r"(?:pub(?:\([^)]*\))?\s+)?mod\s+[A-Za-z_][A-Za-z0-9_]*\s*\{", out[j:])
        if not mm:
            # not a test module — drop just the attribute
            out = out[: m.start()] + out[m.end() :]
            continue
        brace = j + mm.end() - 1
        end = match_delim(out, brace)
        out = out[: m.start()] + ("" if end == -1 else out[end + 1 :])


def iter_fns(src: str):
    """Yield `(name, body, cfg_predicates)` for every `fn name(...) { body }`.

    The predicates are the `cfg(...)` gates on the item itself. They matter:
    `#[cfg(feature = "saml-sso")] impl RouteModule for SamlModule` is a whole
    conditional surface, and a definition that cannot compile in a lane must not
    be treated as a callable root in that lane.
    """
    for m in re.finditer(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:<[^>]*>)?\s*\(", src):
        # locate the body brace: skip the parameter list, then any return type
        p = m.end() - 1
        close = match_delim(src, p)
        if close == -1:
            continue
        j = close + 1
        # skip `-> Type` / `where ...` up to the body brace, at depth 0
        depth = 0
        brace = -1
        while j < len(src):
            c = src[j]
            if c == '"':
                j += 1
                while j < len(src) and src[j] != '"':
                    j += 2 if src[j] == "\\" else 1
                j += 1
                continue
            if c in "([{":
                if c == "{" and depth == 0:
                    brace = j
                    break
                if c == "{":
                    depth += 1
                else:
                    depth += 1
            elif c in ")]}":
                depth -= 1
            elif c == ";" and depth == 0:
                break  # trait method declaration without a body
            j += 1
        if brace == -1:
            continue
        end = match_delim(src, brace)
        if end == -1:
            continue
        yield m.group(1), src[brace + 1 : end], predicates_before(src, m.start())


def iter_consts(src: str):
    """Yield `(name, value_text)` for every top-level `const NAME ... = VALUE;`."""
    for m in re.finditer(r"\bconst\s+([A-Za-z_][A-Za-z0-9_]*)\s*:", src):
        eq = src.find("=", m.end())
        if eq == -1:
            continue
        semi = src.find(";", eq)
        if semi == -1:
            continue
        yield m.group(1), src[eq + 1 : semi]


# --------------------------------------------------------------------------
# Compile-time feature lanes and runtime profile guards (B2-1)
# --------------------------------------------------------------------------
#
# The served surface depends on two independent axes and the extractor used to
# collapse both into one union:
#
#   * COMPILE TIME — `#[cfg(feature = "…")]`. This is the *lane*: identical
#     source yields a different surface per feature set. The two that matter are
#     the `default` set (the golden fixtures) and `default + all-extensions`
#     (the maximally complete lane the SDK ingests).
#   * RUN TIME — `ProfileFlags`. This is the *profile*: `default` / `worker` /
#     `all`. Exactly two routers are merged only when their flag is on
#     (`worker::create_worker_body_router`, `oidc::create_oidc_router`).
#
# Collapsing them hides two different lies: a route the ledger promises in a
# lane that cannot compile it, and a route it promises in a profile whose router
# is never merged.

_CARGO_FEATURES_HEAD = re.compile(r"^\[features\]\s*$", re.M)
_CARGO_FEATURE_ENTRY = re.compile(r"^([A-Za-z0-9_-]+)\s*=\s*\[(.*?)\]", re.M | re.S)
_CFG_FEATURE = re.compile(r'feature\s*=\s*"([^"]+)"')

# Flag stems as they appear in a `merge_into` condition -> the `ProfileFlags`
# field they stand for.
_PROFILE_FLAG_STEMS = (("oidc", "oidc_enabled"), ("worker", "worker_enabled"), ("saml", "saml_enabled"))


def cargo_feature_table(cargo_toml: str) -> dict:
    """Parse `[features]` into `{name: [bare feature names]}`.

    `synapse-services/voice-extended` style entries are *dependency* features;
    they can never appear in `cfg(feature = "…")`, so only bare names are kept.
    Reading the manifest instead of hard-coding the sets keeps the lanes honest
    when a feature is added to `default` or to `all-extensions`.
    """
    head = _CARGO_FEATURES_HEAD.search(cargo_toml)
    if not head:
        return {}
    body = cargo_toml[head.end() :]
    nxt = re.search(r"^\[", body, re.M)
    if nxt:
        body = body[: nxt.start()]
    table: dict = {}
    for entry in _CARGO_FEATURE_ENTRY.finditer(body):
        table[entry.group(1)] = [i for i in re.findall(r'"([^"]+)"', entry.group(2)) if "/" not in i]
    return table


def feature_closure(seeds, table) -> frozenset:
    """Transitive closure of `seeds` over the `[features]` table."""
    seen: set = set()
    stack = list(seeds)
    while stack:
        name = stack.pop()
        if name in seen:
            continue
        seen.add(name)
        stack.extend(table.get(name, ()))
    return frozenset(seen)


def cfg_allows(predicate: str, features) -> bool:
    """Evaluate a `cfg(...)` body against an enabled-feature set.

    `features is None` is union mode: every predicate holds. Bare flags (`test`,
    `debug_assertions`, custom cfgs) evaluate to *off* — a production extraction
    has no `cfg(test)` code left after `strip_test_mods`, so treating them as on
    could only ever add phantoms.
    """
    if features is None:
        return True
    text = predicate.strip()
    for keyword, combine in (("all", all), ("any", any)):
        if text.startswith(keyword + "("):
            close = match_delim(text, len(keyword))
            if close == -1:
                return True
            return combine(cfg_allows(part, features) for part in split_top_level(text[len(keyword) + 1 : close]))
    if text.startswith("not("):
        close = match_delim(text, 3)
        if close == -1:
            return True
        return not cfg_allows(text[4:close], features)
    literal = _CFG_FEATURE.fullmatch(text)
    if literal:
        return literal.group(1) in features
    return False


def cfg_all_allow(predicates, features) -> bool:
    """`#[cfg(a)] #[cfg(b)]` is a conjunction, matching Rust's semantics."""
    return all(cfg_allows(p, features) for p in predicates)


def gated_router_builders(files: dict) -> dict:
    """`router-builder fn name -> ProfileFlags field` for flag-gated merges.

    Read out of `route_module.rs::*::merge_into`, the one place where a runtime
    flag chooses *between* two routers::

        if oidc::oidc_enabled(&sso_ctx) { router.merge(oidc::create_oidc_router(state)) }
        else                           { router.merge(oidc::create_oidc_fallback_router()) }

    Only the `if` branch is gated: the `else` branch's router is merged whenever
    the module is, so its routes are always-on. The map is not taken on faith —
    a wrong entry makes the per-profile sets disagree with the committed
    fixtures, which is a hard gate failure.
    """
    gated: dict = {}
    for src in files.values():
        for name, body, _preds in iter_fns(src):
            if name != "merge_into":
                continue
            for match in re.finditer(r"\bif\s+(.+?)\s*\{", body):
                brace = match.end() - 1
                end = match_delim(body, brace)
                if end == -1:
                    continue
                merges = re.findall(
                    r"\.merge\s*\(\s*(?:[A-Za-z_][A-Za-z0-9_]*::)*([A-Za-z_][A-Za-z0-9_]*)\s*\(",
                    body[brace + 1 : end],
                )
                hits = [flag for stem, flag in _PROFILE_FLAG_STEMS if stem in match.group(1)]
                if not merges or len(hits) != 1:
                    continue
                for callee in merges:
                    gated[callee] = hits[0]
    return gated


def default_origin(owner: str) -> str:
    """`room.rs` → `room`, `admin/room/mod.rs` → `admin::room`.

    This is the *fallback* only. The value that ships is the one the manifests
    declared, and those declarations are not all derivable from a path — a
    module may register under a shorter name (`e2ee/keys.rs` → `e2ee`), a
    sub-router may register under a different name than its file (`worker.rs`
    registers `worker` *and* `worker_body`), and `assembly.rs` registers eleven
    distinct names from one file. Those live in `ledger_origins.txt`, and a
    route whose origin cannot be resolved is a hard failure, never a guess.
    """
    stem = owner[:-3] if owner.endswith(".rs") else owner
    parts = [part for part in stem.split("/") if part and part != "mod"]
    return "::".join(parts)


def resolve_label(path: str, registrars, table: list):
    """`registered_by` for one absolute route, or `None` when undecidable.

    Rules are consulted **in file order** and the first match wins, so a rule
    may be listed above a broader one on purpose. Two passes: a path-qualified
    rule for one of this route's registrars (most specific), then an
    unqualified per-function rule, then a file-level rule.

    File order — rather than "highest priority wins" — is what lets the ledger
    differ by lane. `/.well-known/jwks.json` is `oidc_fallback` in the default
    lane and `oidc` in `all-extensions`: the latter compiles the full OIDC
    router as well, so the route has two registrars there, and the ledger
    records whichever registered first. Listing the `create_oidc_router` rule
    above the fallback one reproduces exactly that, without the extractor
    needing to know anything about lanes.

    Returns `None` when nothing decides: rules disagree with nothing to order
    them by. The caller fails loudly — an arbitrary pick would silently rename
    an SDK codegen directory.
    """
    if not registrars:
        return None
    for require_qualifier in (True, False):
        for owner, who, qual, origin in table:
            if require_qualifier and not qual:
                continue
            if qual and not path.startswith(qual):
                continue
            if who == "*":
                if any(file == owner for file, _fn in registrars):
                    return origin
                continue
            if (owner, who) in registrars:
                return origin

    defaults = {default_origin(file) for file, _fn in registrars}
    return defaults.pop() if len(defaults) == 1 else None


def load_ledger_origins() -> list:
    """`[(file, fn|"*", path_prefix|"", registered_by)]` from `ledger_origins.txt`.

    `registered_by` is authored data, not derived data: it is what the SDK's
    contract-sync uses to pick an SDK directory for each route (see
    `LEDGER_MODULE_ALIASES` in the SDK fork's `scripts/contract-module-map.mjs`),
    so renaming one silently relocates generated files.

    Function granularity is the coarsest key that fits, but it is not always
    enough: the same *relative* route can be mounted under two prefixes by two
    different routers (`/search_rooms` is mounted under v3 by the search router
    and under `/_matrix/vendor/v1` by the vendor router), and the ledger names
    those two mounts differently. So a rule may carry a `@<path-prefix>`
    qualifier, which wins over the unqualified rule for that same function.
    """
    table: list = []
    if not os.path.exists(LEDGER_ORIGINS):
        return table
    with open(LEDGER_ORIGINS, encoding="utf-8") as fh:
        for lineno, line in enumerate(fh, 1):
            body = line.split("#", 1)[0].strip()
            if not body:
                continue
            parts = body.split()
            if len(parts) != 2 or "::" not in parts[0]:
                raise SystemExit(f"{LEDGER_ORIGINS}:{lineno}: expected `<file>::<fn|*>[@<prefix>]  <registered_by>`, got {line!r}")
            where, origin = parts
            owner, _, fn = where.rpartition("::")
            fn, _, qual = fn.partition("@")
            table.append((owner, fn, qual, origin))
    return table


# --------------------------------------------------------------------------
# Expression evaluation
# --------------------------------------------------------------------------

# A route entry is `(method, path, owner_file)`.
# `method` is None only for intermediate prefix/string values.

_RE_TUPLE = re.compile(r"\(\s*(?:axum::http::)?Method::([A-Za-z]+)\s*,\s*\"([^\"]*)\"")
_RE_STR = re.compile(r'"([^"]*)"')


def parse_chain(expr: str):
    """Split `base.m1(a).m2(b)` into `(base_text, [(name, args_text), ...])`."""
    i, n, depth = 0, len(expr), 0
    splits = []
    while i < n:
        c = expr[i]
        if c == '"':
            i += 1
            while i < n:
                if expr[i] == "\\":
                    i += 2
                    continue
                if expr[i] == '"':
                    break
                i += 1
            i += 1
            continue
        if c == "'":
            if i + 2 < n and expr[i + 2] == "'":
                i += 3
                continue
            i += 1
            continue
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
        elif (
            c == "."
            and depth == 0
            and i + 1 < n
            and (expr[i + 1].isalnum() or expr[i + 1] == "_")
            and not (i > 0 and expr[i - 1] == ".")
        ):
            splits.append(i)
        i += 1

    if not splits:
        return expr.strip(), []

    base = expr[: splits[0]].strip()
    calls = []
    for idx, pos in enumerate(splits):
        end = splits[idx + 1] if idx + 1 < len(splits) else n
        seg = expr[pos + 1 : end].strip()
        mm = re.match(r"([A-Za-z_][A-Za-z0-9_]*)\s*(?:::<[^>]*>)?\s*\(", seg)
        if not mm:
            continue
        args_start = seg.index("(", mm.end() - 1)
        close = match_delim(seg, args_start)
        if close == -1:
            continue
        calls.append((mm.group(1), seg[args_start + 1 : close]))
    return base, calls


class Resolver:
    def __init__(self, files: dict[str, str], features=None, module_gates: dict | None = None):
        self.files = files  # relpath -> comment-stripped source
        # `features is None` is *union mode*: every `#[cfg]` predicate is
        # treated as satisfied, reproducing the historical extraction that
        # reconciled against the union of both ledger lanes. Pass a frozenset of
        # enabled feature names (see `LANES`) to resolve one compile lane.
        self.features = features
        # `relpath -> [cfg predicates]` implied by the `mod` declarations up the
        # tree. Needed only to *emit* the predicate for each route (the
        # generated manifest must carry it so the compiler does the filtering);
        # lane resolution itself uses it earlier, in `load_sources`.
        self.module_gates = module_gates or {}
        # `(method, path) -> {frozenset of cfg predicates}` that must hold for
        # the route to exist. Recorded only for defining fns, same as
        # `registrars`, and used to gate the generated table.
        self.cfg_of: dict[tuple[str, str], set] = defaultdict(set)
        self._current_cfg: frozenset = frozenset()
        # `fn name -> ProfileFlags field` for the router builders that a runtime
        # flag decides whether to merge. See `gated_router_builders`.
        self.gated = gated_router_builders(files)
        # `(method, path) -> {guard}`. A guard is `""` (always merged) or the
        # ProfileFlags field that gates the production. A route produced under
        # several guards keeps all of them: `/…/.well-known/openid-configuration`
        # is served by both the always-merged fallback router and the
        # OIDC-only router, so it must count as always-on. Keyed by the
        # `(method, path)` pair consumers compare on, not by the tuple the
        # extractor carries around (which also holds the owning file).
        self.guards: dict[tuple[str, str], set] = defaultdict(set)
        # Guard of the router builder currently being evaluated (`""` = always
        # merged). Maintained by `eval_fn_body`, consumed by `apply_call`.
        self._current_guard = ""
        # `fn name -> registered_by` overrides, plus the file-level ones.
        self.origin_table = load_ledger_origins()
        # `(method, path) -> {(file, fn)}`: the registering functions, innermost
        # first. Recorded from the *router* fns only — a manifest fn is the
        # oracle being replaced, so letting it define the answer would make the
        # comparison circular. The `registered_by` label is derived from this in
        # one place (`resolve_label`), so the label stays policy and the fact stays
        # derived.
        self.registrars: dict[tuple[str, str], set] = defaultdict(set)
        self._current_fn = ""
        self._current_owner = ""
        # `self.fn_all` keeps every definition distinct. Keying by
        # `(file, name)` would collide: `route_module.rs` defines 11 separate
        # `merge_into` methods, and collapsing them loses 10 modules' routes.
        self.fn_all: list[tuple[str, str, str]] = []  # (file, name, body)
        self.consts: dict[str, list[tuple[str, str]]] = defaultdict(list)
        for rel, src in files.items():
            for name, body, preds in iter_fns(src):
                # A definition that cannot compile in this lane is not callable
                # here; keeping it would let a `#[cfg(feature = "saml-sso")] impl`
                # leak its routes into the default-feature lane.
                if not cfg_all_allow(preds, features):
                    continue
                self.fn_all.append((rel, name, body))
            for name, val in iter_consts(src):
                self.consts[name].append((rel, val))
        self.fn_defs: dict[str, list[tuple[str, str]]] = defaultdict(list)
        for rel, name, body in self.fn_all:
            self.fn_defs[name].append((rel, name, body))
        self.unresolved: set[str] = set()
        # Memo key is the *content* of the definition, not `id(body)`.
        # `id()` is reused once a temporary string is collected, so an
        # id-keyed cache can hand a fresh block body a stale `[]` result —
        # which silently dropped every route behind an `if`/`for` block.
        self._memo: dict[tuple[str, str, str], list] = {}

    # -- symbol lookup -----------------------------------------------------

    def lookup_fn(self, name: str, owner: str):
        """Same-file definition first, then a globally unique one.

        Returns `(defining_file, name, body)` so the caller can attribute the
        routes to the file that owns the path *literal*, not to the file doing
        the nesting.
        """
        defs = self.fn_defs.get(name, [])
        same = [d for d in defs if d[0] == owner]
        if len(same) == 1:
            return same[0]
        if len(same) > 1:
            self.unresolved.add(f"ambiguous same-file fn {name} in {owner}")
            return same[0]
        if len(defs) == 1:
            return defs[0]
        if len(defs) > 1:
            self.unresolved.add(f"ambiguous fn {name} ({len(defs)} defs)")
        return None

    def lookup_const(self, name: str, owner: str) -> str | None:
        same = [v for (f, v) in self.consts.get(name, []) if f == owner]
        if len(same) == 1:
            return same[0]
        defs = self.consts.get(name, [])
        if len(defs) == 1:
            return defs[0][1]
        return None

    def prefix_list(self, text: str, env: dict, owner: str) -> list[str]:
        """Resolve an expression to a list of path prefixes."""
        t = text.strip()
        while t.startswith("&"):
            t = t[1:].strip()
        t = t.rstrip(",").strip()
        if not t:
            return []
        if t.startswith("[") or t.startswith("vec!["):
            inner = t[t.index("[") + 1 : match_delim(t, t.index("["))]
            return [s for s in _RE_STR.findall(inner)]
        if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_:]*", t):
            name = t.split("::")[-1]
            if name in env and env[name][0] == "prefixes":
                return list(env[name][1])
            val = self.lookup_const(name, owner)
            if val is not None:
                vv = val.strip()
                if vv.startswith("[") or vv.startswith("&["):
                    inner = vv[vv.index("[") + 1 : match_delim(vv, vv.index("["))]
                    return [s for s in _RE_STR.findall(inner)]
                m = _RE_STR.search(vv)
                if m:
                    return [m.group(1)]
            self.unresolved.add(f"prefix {t}")
        return []

    def const_str(self, text: str, env: dict, owner: str) -> str | None:
        t = text.strip().lstrip("&").strip().rstrip(",").strip()
        m = re.fullmatch(r'"([^"]*)"', t)
        if m:
            return m.group(1)
        if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", t):
            if t in env and env[t][0] == "str":
                return env[t][1]
            val = self.lookup_const(t, owner)
            if val is not None:
                m = _RE_STR.search(val)
                if m:
                    return m.group(1)
            self.unresolved.add(f"const str {t}")
        return None

    # -- evaluation --------------------------------------------------------

    def eval_value(self, text: str, env: dict, owner: str):
        """Evaluate to a tagged value: ('routes', [...]) | ('prefixes', [...]) | ('str', s)."""
        t = text.strip()
        while t.startswith("&") or t.startswith("*"):
            t = t[1:].strip()
        t = t.rstrip(",").strip()
        if not t:
            return ("routes", [])

        # `vec![(Method::GET, "/p"), ...]` / `&[(Method::GET, "/p"), ...]`
        if t.startswith("vec![") or t.startswith("["):
            return ("routes", self._tuples(t, owner))

        # declarative manifest helper
        if re.match(r"^(?:crate::web::routes::route_ledger::|route_ledger::)?expand_under_prefixes\s*\(", t):
            return ("routes", self._expand_under_prefixes(t, env, owner))

        # bare identifier: local binding or const
        if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", t):
            if t in env:
                return env[t]
            val = self.lookup_const(t, owner)
            if val is not None:
                return self.eval_value(val, env, owner)
            return ("routes", [])

        # `RouteEntry::new(Method::X, "/abs", ..)` literal
        if "RouteEntry::new" in t:
            return ("routes", self._tuples(t, owner))

        base, calls = parse_chain(t)
        acc = self.eval_atom(base, env, owner)
        for name, args in calls:
            acc = self.apply_call(acc, name, args, env, owner)
        return ("routes", acc)

    def _tuples(self, t: str, owner: str) -> list:
        out = [(m.group(1).upper(), m.group(2), owner) for m in _RE_TUPLE.finditer(t)]
        self._record_guards(out)
        return out

    def _expand_under_prefixes(self, t: str, env: dict, owner: str) -> list:
        i = t.index("(")
        close = match_delim(t, i)
        if close == -1:
            return []
        args = split_top_level(t[i + 1 : close])
        if len(args) < 3:
            return []
        prefixes = self.prefix_list(args[1], env, owner)
        sub = self.eval_value(args[2], env, owner)
        routes = sub[1] if sub[0] == "routes" else []
        out = []
        for pfx in prefixes:
            for meth, path, own in routes:
                out.append((meth, pfx + path, own))
        self._record_guards(out, tag_registrar=False)
        self._inherit_registrars(out, routes)
        return out

    def eval_atom(self, base: str, env: dict, owner: str) -> list:
        """Evaluate the head of a chain to a list of route tuples."""
        b = base.strip()
        if not b or b in ("Router::new()", "Router::<AppState>::new()"):
            return []
        # array literal of `(Method::X, "/path")` tuples, e.g.
        # `[(Method::GET, "/"), ..].into_iter().map(..)`
        if b.startswith("[") or b.startswith("vec!["):
            return self._tuples(b, owner)
        # bare local variable (possibly `router` shell)
        if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", b):
            if b in env and env[b][0] == "routes":
                return list(env[b][1])
            return []
        # function call, possibly qualified: `crate::a::b::f(args)`
        m = re.match(r"^(?:[A-Za-z_][A-Za-z0-9_]*::)*([A-Za-z_][A-Za-z0-9_]*)\s*\(", b)
        if m:
            name = m.group(1)
            args_start = b.index("(", m.end() - 1)
            close = match_delim(b, args_start)
            args = b[args_start + 1 : close] if close != -1 else ""
            # a router-builder fn call
            found = self.lookup_fn(name, owner)
            if found is not None:
                def_file, _def_name, def_body = found
                return self.eval_fn_body(name, def_body, def_file, memo_key=found)
            # `MethodRouter`: `get(h).put(h2)` as a chain base — not a route set
            return []
        return []

    def eval_fn_body(self, name: str, body: str, owner: str, memo_key=None) -> list:
        """Evaluate a function body to a route set.

        `memo_key` must be supplied only for real definitions; block
        recursions deliberately skip the cache because their body text is a
        short-lived temporary.
        """
        if memo_key is not None:
            if memo_key in self._memo:
                return list(self._memo[memo_key])
            self._memo[memo_key] = []  # cycle guard
        # Routes registered inside a flag-gated router are recorded together with
        # that guard, which is what lets the per-profile sets be derived. Saved
        # and restored rather than pushed on a stack: there is no early `return`
        # past this point, so no `try`/`finally` around the loop is needed.
        prev_guard = self._current_guard
        self._current_guard = self.gated.get(name) or prev_guard
        prev_fn, prev_owner = self._current_fn, self._current_owner
        self._current_fn, self._current_owner = name, owner
        # A callee's routes inherit the caller's `cfg` scope: reaching them at
        # all required those predicates. Union rather than assign, so an
        # enclosing `#[cfg(feature = "x")] { .. }` still applies to a route the
        # block reaches through a call.
        prev_cfg = self._current_cfg
        self._current_cfg = prev_cfg | frozenset(self.module_gates.get(owner, ()))
        env: dict = {}
        result: list = []

        for stmt in split_statements(body):
            preds, s = strip_leading_attrs(stmt)
            if not s:
                continue
            if s.startswith("//") or s.startswith("#"):
                continue
            # `#[cfg(feature = "x")] { router = router.route(..) }`, and the same
            # gate on a plain statement, only exist in a lane whose feature set
            # satisfies it.
            if not cfg_all_allow(preds, self.features):
                continue

            # `#[cfg(..)] { router = router.route(..) }` and plain blocks:
            # recurse so feature-gated registrations are not dropped.
            if s.startswith("{"):
                inner = match_delim(s, 0)
                nested = s[1:inner] if inner != -1 else s[1:]
                result.extend(self._recurse(name, nested, owner, preds))
                continue

            # `if cond { .. }` / `match` / `for` / `while` / `else { .. }`
            if re.match(r"(if|match|for|while|loop|else)\b", s):
                for blk in extract_blocks(s):
                    result.extend(self._recurse(name, blk, owner, preds))
                continue

            # `let [mut] name[: Type] = EXPR`
            m = re.match(r"let\s+(?:mut\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*(?::[^=]*)?=", s, re.S)
            if m:
                name_b = m.group(1)
                eq = s.index("=", m.start() + len(name_b))
                env[name_b] = self.eval_value(s[eq + 1 :], env, owner)
                continue

            # `X.push(..)` / `X.extend(..)`: a *local* mutation only.
            #
            # It must not also feed the function result: manifests keep
            # intermediate relative-route lists (`v3_relative.extend(..)`) that
            # are only meaningful after `expand_under_prefixes` has prefixed
            # them. Leaking them into the result would emit unprefixed paths.
            # The result comes from the tail expression (`entries` / `out`).
            m = re.match(r"([A-Za-z_][A-Za-z0-9_]*)\.(push|extend)\s*\(", s)
            if m:
                var, op = m.group(1), m.group(2)
                astart = s.index("(", m.end() - 1)
                aclose = match_delim(s, astart)
                args = s[astart + 1 : aclose] if aclose != -1 else ""
                val = self.eval_value(args, env, owner)
                added = val[1] if val[0] == "routes" else []
                if var in env and env[var][0] == "routes":
                    env[var][1].extend(added)
                continue

            # `name = EXPR` (rebind, e.g. `router = router.merge(x)`)
            m = re.match(r"([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(?!=)", s)
            if m and m.group(1) in env:
                var = m.group(1)
                eq = s.index("=", m.start() + len(var))
                new_val = self.eval_value(s[eq + 1 :], env, owner)
                # An opaque right-hand side (dynamic dispatch such as
                # `module.merge_into(..)`) must not erase registrations we
                # already resolved; those routes come from the `merge_into`
                # roots anyway. Replacing with `[]` would drop live endpoints.
                if new_val[0] == "routes" and not new_val[1] and env[var][1]:
                    self.unresolved.add(f"opaque rebind of {var} kept previous value ({owner})")
                else:
                    env[var] = new_val
                continue

            # `out.extend(` handled above; a bare `return EXPR` or tail expression
            if s.startswith("return "):
                val = self.eval_value(s[len("return ") :], env, owner)
                result.extend(val[1] if val[0] == "routes" else [])
                continue

            # tail / bare expression: contributes its routes
            val = self.eval_value(s, env, owner)
            if val[0] == "routes":
                result.extend(val[1])

        self._current_guard = prev_guard
        self._current_fn, self._current_owner = prev_fn, prev_owner
        self._current_cfg = prev_cfg
        if memo_key is not None:
            self._memo[memo_key] = result
        return list(result)

    def _recurse(self, name: str, body: str, owner: str, preds) -> list:
        """Evaluate a nested block, carrying its `#[cfg]` predicates inward.

        `#[cfg(feature = "voip-tracking")] { router = router.route(..) }` is the
        only way to gate part of a function, so the predicate has to travel with
        the block rather than sit on an item. Without this the generated table
        would advertise routes the current build does not compile, and the
        405-probe test would report a live router missing from the ledger.
        """
        saved = self._current_cfg
        self._current_cfg = saved | frozenset(preds)
        try:
            return self.eval_fn_body(name, body, owner)
        finally:
            self._current_cfg = saved

    def gate_of(self, row) -> frozenset:
        """`cfg` predicates that must hold for `row` to be compiled.

        This is what the generated table has to carry: the compiler — not the
        extractor — does the feature filtering at build time, so each emitted
        row needs the predicate that reproduces the lane it came from.

        Composed of two things that are each insufficient alone:

        * **the module gates of every registering file.** A route defined in
          `cas.rs` is compiled only with `cas-sso`, no matter who mounts it.
          This is also what fixes `/login`: the relative path `/login` is
          registered by both the CAS router (gated) and the auth-compat router
          (not), and the ledger key is the absolute path — so the ungated claim
          leaks in. Unioning the registering files' gates restores the
          condition that actually governs the row.
        * **the narrowest recorded `cfg` scope.** The defining function's own
          `#[cfg(feature = "voip-tracking")] { .. }` block is not a module gate,
          so it can only come from here. Narrowest rather than union: the
          recorded contexts are alternative mounting paths, and the shortest one
          is the least-gated. Picking the most permissive option cannot hide a
          row from a lane it belongs to; the reverse would, and the per-lane
          fidelity gate below is what proves neither happens.
        """
        contexts = self.cfg_of.get(row, set())
        base = set(min(sorted(contexts, key=lambda c: (len(c), sorted(c))))) if contexts else set()
        for file, _fn in self.registrars.get(row, set()):
            base |= set(self.module_gates.get(file, ()))
        return frozenset(base)

    def _record_guards(self, routes, tag_registrar: bool = True) -> None:
        """Tag freshly-created route tuples with the guard in force.

        Recorded at *creation*, not at the root that reached them: the same
        route can be reached through an always-merged root and a gated one, and
        recording both is what makes "served in every profile" (`""` in the set)
        distinguishable from "served only when the flag is on".

        The registering function is captured at the same moment, for the same
        reason. `tag_registrar=False` for re-tagging sites (`nest`) where the
        inner routes were already stamped by the router that actually creates
        them: stamping them again with the *nesting* fn would name the wrong
        module.
        """
        if tag_registrar and self._current_fn and "manifest" not in self._current_fn:
            for route in routes:
                self.registrars[(route[0], route[1])].add((self._current_owner, self._current_fn))
                self.cfg_of[(route[0], route[1])].add(self._current_cfg)
        for route in routes:
            self.guards[(route[0], route[1])].add(self._current_guard)

    def _inherit_registrars(self, added, subs) -> None:
        """Carry the registrar and the `cfg` scope across a prefix transform.

        `nest` / `expand_under_prefixes` build a *new* absolute tuple from a
        relative one, so the new tuple has never been through `_record_guards`
        as itself. The registering function is still the one that created the
        relative route — the prefix is applied by whoever mounted it, and that
        is not the same thing as who owns the endpoint. Without this the
        absolutised rows (486 of them) would have no origin at all.

        The `cfg` scope is inherited for the same reason, and additionally
        unioned with the *mounting* scope: mounting a router inside
        `#[cfg(feature = "x")]` makes everything it serves conditional on `x`
        too. Each context stays a separate frozenset — they are alternatives
        (the route exists if *any* mounting path is compiled), not a
        conjunction.
        """
        for (meth, path, _own), (am, ap, _ao) in zip(subs, added):
            regs = self.registrars.get((meth, path))
            if regs:
                self.registrars[(am, ap)] |= set(regs)
            contexts = self.cfg_of.get((meth, path))
            if contexts:
                self.cfg_of[(am, ap)] |= {ctx | self._current_cfg for ctx in contexts}

    def apply_call(self, acc: list, name: str, args: str, env: dict, owner: str) -> list:
        if name == "route":
            parts = split_top_level(args)
            if not parts:
                return acc
            pm = _RE_STR.search(parts[0])
            if not pm:
                self.unresolved.add(f"non-literal route path in {owner}")
                return acc
            path = pm.group(1)
            methods = self._methods_of(parts[1] if len(parts) > 1 else "")
            if not methods:
                self.unresolved.add(f"no method for route {path!r} in {owner}")
            added = [(m, path, owner) for m in methods]
            self._record_guards(added)
            return acc + added

        if name == "nest":
            parts = split_top_level(args)
            if len(parts) < 2:
                return acc
            pm = _RE_STR.search(parts[0])
            if not pm:
                self.unresolved.add(f"non-literal nest prefix in {owner}")
                return acc
            prefix = pm.group(1)
            sub = self.eval_value(parts[1], env, owner)
            subs = sub[1] if sub[0] == "routes" else []
            if not subs:
                self.unresolved.add(f"nest {prefix} -> unresolved {parts[1].strip()[:60]} in {owner}")
            added = [(meth, prefix + path, own) for (meth, path, own) in subs]
            self._record_guards(added, tag_registrar=False)
            self._inherit_registrars(added, subs)
            return acc + added

        if name == "merge":
            sub = self.eval_value(args, env, owner)
            return acc + (sub[1] if sub[0] == "routes" else [])

        # Transparent wrappers: `with_state`, `layer`, `merge` on the same router,
        # `clone`, `to_vec`, `into_iter`, `without_v07_checks`, `fallback`, ...
        if name in (
            "with_state",
            "clone",
            "to_vec",
            "iter",
            "into_iter",
            "without_v07_checks",
            "fallback",
            "layer",
            "route_layer",
            "boxed",
            "map",
            "map_err",
            "with_state_arc",
            "into_make_service",
            "into_make_service_with_connect_info",
        ):
            return acc

        if name in ("get", "post", "put", "delete", "patch", "head", "options", "on"):
            # a `MethodRouter` chain sitting where a route set was expected
            return acc

        self.unresolved.add(f"chain method .{name}() in {owner}")
        return acc

    @staticmethod
    def _methods_of(text: str) -> list[str]:
        """Extract every HTTP method in a `get(a).put(b)` handler expression."""
        found = []
        for m in re.finditer(r"(?:^|[.:\s(])([a-z]+)\s*\(", text):
            name = m.group(1)
            if name in HTTP_METHODS_SET and name not in found:
                found.append(name)
        if not found:
            m = re.fullmatch(r"\s*(?:axum::routing::)?([a-z]+)\s*", text)
            if m and m.group(1) in HTTP_METHODS_SET:
                found.append(m.group(1))
        return [f.upper() for f in found]

    # -- roots -------------------------------------------------------------

    def resolve_def(self, name: str, owner: str):
        """Resolve a callee `name` seen in `owner` to one definition index.

        Mirrors `lookup_fn`: same-file wins, then a globally unique definition.
        An ambiguous name resolves to nothing — marking *all* same-named
        definitions as "called" would wrongly disqualify every one of them from
        being a root, and their routes would silently vanish.
        """
        ids = [i for i, (f, n, _b) in enumerate(self.fn_all) if n == name]
        same = [i for i in ids if self.fn_all[i][0] == owner]
        if len(same) == 1:
            return same[0]
        if len(ids) == 1:
            return ids[0]
        return None

    def candidates(self) -> list[int]:
        """Indices of definitions that register routes."""
        return [
            i
            for i, (_f, _n, body) in enumerate(self.fn_all)
            if ".route(" in body or ".merge(" in body or ".nest(" in body
        ]

    def roots(self) -> list[tuple[str, str, str]]:
        """Router-builder functions that no other router-builder statically calls.

        Feature-gated modules are wired through `RouteModule::merge_into`, which
        is dispatched dynamically, so their entry points are unreferenced and
        therefore become roots naturally.

        Evaluating only roots is what keeps relative paths out of the contract:
        a sub-router is never evaluated standalone, so each path literal appears
        exactly once, carrying the full prefix chain of its parent.
        """
        cand = set(self.candidates())
        skip_names = {"Router", "new", "vec", "format", "Some", "Ok", "Err", "Method"}

        callees: set[int] = set()
        for i in cand:
            owner = self.fn_all[i][0]
            for stmt in split_statements(self.fn_all[i][2]):
                # a callee may hide inside a `#[cfg(..)] { .. }` block too
                for _inner in [stmt] + extract_blocks(stmt):
                    for cm in re.finditer(
                        r"(?:[A-Za-z_][A-Za-z0-9_]*::)*([A-Za-z_][A-Za-z0-9_]*)\s*\(", _inner
                    ):
                        cname = cm.group(1)
                        if cname in skip_names:
                            continue
                        target = self.resolve_def(cname, owner)
                        if target is not None and target in cand:
                            callees.add(target)

        return [self.fn_all[i] for i in sorted(cand - callees)]

    # -- manifest oracle ---------------------------------------------------

    def manifest_routes(self) -> list:
        """Evaluate every `*_route_manifest*` / `*_manifest*` function."""
        out: list = []
        for _file, name, body in self.fn_all:
            if "manifest" not in name:
                continue
            out.extend(self.eval_fn_body(name, body, _file, memo_key=(_file, name, body)))
        return out


# --------------------------------------------------------------------------
# Driver
# --------------------------------------------------------------------------


_MOD_DECL = re.compile(r"^\s*(?:pub\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;", re.M)


def mod_gated_files(rel_sources: dict) -> dict:
    """`relpath -> [cfg predicates]` implied by the module declarations.

    Walks the module tree from `mod.rs`, accumulating every ancestor's gate::

        #[cfg(feature = "voice-extended")] pub mod voice;   // mod.rs
        #[cfg(feature = "x")]              pub mod detail;  // voice/mod.rs
        -> voice/detail.rs requires BOTH

    This is the only gate that works for a whole conditional surface. Gating the
    `impl RouteModule` instead would leave the router builder with no static
    caller, promote it to an unreferenced root, and *add* its routes to the very
    lane that cannot compile them.
    """
    gates: dict = {}
    stack = [("mod.rs", [])]
    while stack:
        rel, inherited = stack.pop()
        if rel in gates:
            continue
        gates[rel] = inherited
        src = rel_sources.get(rel)
        if src is None:
            continue
        base = os.path.dirname(rel)
        for m in _MOD_DECL.finditer(src):
            child = m.group(1)
            preds = predicates_before(src, m.start())
            for cand in (os.path.join(base, child + ".rs"), os.path.join(base, child, "mod.rs")):
                if cand in rel_sources:
                    stack.append((cand, inherited + preds))
                    break
    return gates


def raw_sources() -> dict:
    """Comment-stripped source of every route module, before lane filtering."""
    raw: dict = {}
    for dp, _, fns in os.walk(ROUTES_DIR):
        for f in sorted(fns):
            if not f.endswith(".rs"):
                continue
            fp = os.path.join(dp, f)
            rel = os.path.relpath(fp, ROUTES_DIR)
            if "tests" in rel.split(os.sep):
                continue
            with open(fp, encoding="utf-8", errors="replace") as fh:
                raw[rel] = strip_comments(fh.read())
    return raw


def load_sources(features=None) -> dict[str, str]:
    """Comment-stripped source of every route module, keyed by relpath.

    With `features` set, modules whose `mod` declaration is gated by a feature
    outside that set are dropped entirely, so a lane only ever sees what it
    could actually compile.
    """
    raw = raw_sources()
    gates = mod_gated_files(raw)
    files = {}
    for rel, src in raw.items():
        if features is not None and not cfg_all_allow(gates.get(rel, []), features):
            continue
        files[rel] = strip_test_mods(src)
    return files


def load_lanes() -> dict:
    """`{fixture lane dir: enabled feature set}` read from Cargo.toml.

    `ledger_export/` is generated by a default-feature build and
    `ledger_export_sdk/` by `--features all-extensions`; which in cargo means
    the default set *plus* the meta-feature, not instead of it.
    """
    with open(os.path.join(ROOT, "Cargo.toml"), encoding="utf-8") as fh:
        table = cargo_feature_table(fh.read())
    if not table:
        return {}
    return {
        "ledger_export": feature_closure(["default"], table),
        "ledger_export_sdk": feature_closure(["default", "all-extensions"], table),
    }


def profile_sets(res: "Resolver") -> dict:
    """`{profile: {(method, path)}}` from a guarded extraction.

    A route is served in a profile when at least one router that registers it is
    merged there:
      * `default` — always-merged routers only
      * `worker`  — plus the `worker_enabled` router
      * `all`     — plus the `oidc_enabled` router

    Monotone by construction. The committed fixtures were produced by the
    hand-written `*_route_manifest()` functions, *not* by this code, so agreeing
    with them is a real cross-check of both the guards and the resolver.
    """
    per: dict = {}
    for owner, name, body in res.roots():
        for meth, path, own in res.eval_fn_body(name, body, owner, memo_key=(owner, name, body)):
            if meth and path:
                per.setdefault(own, set()).add((meth, path))
    routes = {t for rs in per.values() for t in rs}
    guards = {r: res.guards.get(r, set()) for r in routes}
    always = {r for r in routes if "" in guards[r]}
    worker = {r for r in routes if "worker_enabled" in guards[r]}
    oidc = {r for r in routes if "oidc_enabled" in guards[r]}
    return {"default": always, "worker": always | worker, "all": always | worker | oidc}


def load_lane(subdir: str) -> set:
    """Every `(method, path)` tuple declared by one fixture lane.

    Lanes are deliberately separate directories because they come from
    different compiles — see the cross-check block in `main()`.
    """
    lane: set = set()
    for prof in ("default", "worker", "all"):
        fp = os.path.join(ROOT, "tests", "unit", "fixtures", subdir, f"{prof}.json")
        if not os.path.exists(fp):
            continue
        with open(fp) as fh:
            for e in json.load(fh)["entries"]:
                lane.add((e["method"], e["path"]))
    return lane


def undeclared_routes(router_set: set, ledger_all: set) -> list:
    """Real routes that no ledger lane declares — the S-14 property.

    Sorted so the diagnostic is stable and reviewable as a diff. Deliberately a
    set difference and not a warning: a route that is served but undeclared is
    invisible to every consumer of the contract (SDK codegen, ROUTE_CONTRACT.md,
    the ledger snapshots) while still working, which is the quietest possible
    way for the contract to become wrong.
    """
    return sorted(router_set - ledger_all)


def main() -> int:
    files = load_sources()
    res = Resolver(files)

    per_module: dict[str, list] = defaultdict(list)
    for owner, _name, body in res.roots():
        for meth, path, own in res.eval_fn_body(_name, body, owner, memo_key=(owner, _name, body)):
            if not meth or not path:
                continue
            per_module[own].append((meth, path))

    # deterministic, deduplicated per module
    out = {m: sorted(set(v)) for m, v in per_module.items()}
    out = {m: [[meth, path] for meth, path in v] for m, v in sorted(out.items()) if v}

    # Registrations that never landed under a Matrix namespace. Two very
    # different things used to look like this, and the doc must not conflate
    # them:
    #   * intentional host-root protocol endpoints (CAS `/login`, `/logout`, …;
    #     the liveness probes `/`, `/health`, `/_health`)
    #   * routers defined but never merged into any tree
    # B5-4 removed the only entry of the second kind: the `threepid.rs` orphan
    # (S-9) defined `create_threepid_router()` in a bare `/requestToken` form
    # that is not a Matrix path, was never merged, duplicated nothing (the real
    # endpoints live in `account_compat.rs` under `/account/3pid/...`), and
    # duplicated no caller — it had been dead since it was introduced.
    # So today this bucket must contain *only* the intentional root surface,
    # and `test_extract_registered.py::check_non_namespace_bucket` pins it to
    # exactly that set: a new member means either a resurrected unwired router
    # or a new non-Matrix root endpoint, and both deserve an explicit decision
    # rather than silently appearing in the doc.
    non_ns = sorted(
        (mod, meth, path)
        for mod, routes in out.items()
        for meth, path in routes
        if not path.startswith(("/_matrix/", "/_synapse/", "/.well-known/"))
    )

    os.makedirs(os.path.join(ROOT, "artifacts"), exist_ok=True)
    reg = {
        "modules": out,
        "total_routes": sum(len(v) for v in out.values()),
        "non_namespace_routes": [[m, meth, p] for m, meth, p in non_ns],
    }
    with open(os.path.join(ROOT, "artifacts", "registered_routes.json"), "w") as f:
        json.dump(reg, f, indent=1, ensure_ascii=False)

    # -- independent oracle: the declarative manifests --------------------
    man = res.manifest_routes()
    man_set = sorted({(m, p) for (m, p, _o) in man if m and p})
    with open(os.path.join(ROOT, "artifacts", "manifest_routes.json"), "w") as f:
        json.dump({"routes": man_set, "total_routes": len(man_set)}, f, indent=1, ensure_ascii=False)

    router_set = {(m, p) for v in out.values() for m, p in v}
    # `(method, path) -> module`, so the S-14 diagnostic names the owning file
    # rather than just the tuple. First-wins is fine: a path derived under two
    # modules is reported once, and the first is representative.
    mods_for: dict = {}
    for mod, routes in out.items():
        for meth, path in routes:
            mods_for.setdefault((meth, path), mod)

    print(f"source files scanned:        {len(files)}")
    print(f"router roots evaluated:      {len(list(res.roots()))}")
    print(f"modules with routes:         {len(out)}")
    print(f"router-derived tuples:       {reg['total_routes']}")
    print(f"manifest-declared tuples:    {len(man_set)}")
    print(f"wrote {os.path.join(ROOT, 'artifacts', 'registered_routes.json')}")

    only_manifest = sorted(set(man_set) - router_set)
    only_router = sorted(router_set - set(man_set))
    print(f"\n-- reconciliation (router-derived vs manifest-declared) --")
    print(f"declared but not derived:    {len(only_manifest)}")
    print(f"derived but not declared:    {len(only_router)}")
    if only_manifest[:25]:
        print("  declared-not-derived sample:")
        for m, p in only_manifest[:25]:
            print(f"    {m:6} {p}")
    if only_router[:25]:
        print("  derived-not-declared sample:")
        for m, p in only_router[:25]:
            print(f"    {m:6} {p}")

    if res.unresolved:
        print(f"\n!! unresolved ({len(res.unresolved)}):")
        for u in sorted(res.unresolved)[:40]:
            print(f"    {u}")

    # Ratchet: the unresolved set must not grow. Every entry here is a known,
    # benign non-resolution (same-name overloads resolved by same-file-first,
    # or opaque `RouteModule` dynamic dispatch). A *new* entry means the parser
    # met a construct it cannot follow, which is exactly how the chained-method
    # defect went unnoticed — so it must fail rather than accumulate.
    allow_path = os.path.join(SCRIPT_DIR, "extract_unresolved_allowlist.txt")
    allowed: set = set()
    if os.path.exists(allow_path):
        with open(allow_path) as fh:
            for ln in fh:
                # strip the trailing `# justification` comment
                entry = ln.split("#", 1)[0].strip()
                if entry:
                    allowed.add(entry)
    new_unresolved = sorted(res.unresolved - allowed)
    if new_unresolved:
        print(f"\n!! NEW unresolved constructs ({len(new_unresolved)}) — parser gap:", file=sys.stderr)
        for u in new_unresolved:
            print(f"    {u}", file=sys.stderr)
        print(
            "    Fix the parser, or add the line to scripts/contract/extract_unresolved_allowlist.txt "
            "with a justification comment.",
            file=sys.stderr,
        )
    stale_allowed = sorted(allowed - res.unresolved)
    if stale_allowed:
        print(f"\nnote: {len(stale_allowed)} allowlist entries are no longer produced — please prune:")
        for u in stale_allowed:
            print(f"    {u}")

    print(f"\n-- registrations outside the Matrix namespaces ({len(non_ns)}) --")
    print("   intentional host-root protocol/probe endpoints only (CAS, liveness);")
    print("   an unwired-router orphan appearing here again must be a deliberate choice")
    for mod, meth, path in non_ns:
        print(f"    {mod:16} {meth:6} {path}")

    # -- authoritative cross-check against the committed ledger fixtures ---
    # The fixtures are produced by the real Rust assembly (`synapse_ledger_export`)
    # and kept honest by the ledger golden tests, so they are an independent
    # oracle: a route we fail to derive is a real parser defect, not a taste
    # difference.
    #
    # TWO lanes, and the union is what matters:
    #   ledger_export/      DEFAULT-feature compile (golden lane)
    #   ledger_export_sdk/  `all-extensions` compile — the maximally complete
    #                       lane the SDK ingests
    # Judging completeness against the golden lane alone makes every
    # feature-gated router look "undeclared" — voice, cas, saml,
    # server-notifications, voip-tracking and builtin-oidc are simply not
    # compiled there. That is how the real omissions hid: 102 entries, 102 of
    # which were noise, so nobody looked at the list. Against the union the
    # same measurement is 22, every one a genuinely missing declaration.
    ledger = load_lane("ledger_export")
    ledger_sdk = load_lane("ledger_export_sdk")
    ledger_all = ledger | ledger_sdk
    missed: list = []
    undeclared: list = []
    if ledger_all:
        missed = sorted(ledger_all - router_set)
        undeclared = undeclared_routes(router_set, ledger_all)
        print(
            f"\n-- cross-check vs ledger fixtures "
            f"(golden {len(ledger)}, sdk {len(ledger_sdk)}, union {len(ledger_all)}) --"
        )
        print(f"ledger routes NOT derived  : {len(missed)}   <- must be 0")
        for m, p in missed[:20]:
            print(f"    {m:6} {p}")
        print(f"derived but not in ledger  : {len(undeclared)}   <- must be 0 (S-14)")
        for m, p in undeclared[:40]:
            print(f"    {m:6} {p}   <- {mods_for.get((m, p), '?')}")

    # -- per-lane, per-profile cross-check (B2-1) ---------------------------
    # The union check above cannot see two distinct lies, because it throws both
    # axes away:
    #   * LANE — `#[cfg(feature = "…")]`. A route can be promised by the golden
    #     lane while only compiling in `all-extensions`; the union hides it.
    #     Each lane is now resolved with its own feature set, read from
    #     Cargo.toml so that adding a feature to `default` / `all-extensions`
    #     moves this gate with it instead of silently widening a hard-coded set.
    #   * PROFILE — `ProfileFlags`. Two routers are merged only when their flag
    #     is on, so the same source serves 1047 or 1065 routes depending on
    #     runtime config. The union only ever sees the widest profile.
    # The fixtures on the other side were produced by the hand-written
    # `*_route_manifest()` functions, so agreement is a real two-implementation
    # cross-check: six sets, exact equality, deliberately no allowlist.
    profile_mismatches: list = []
    lanes = load_lanes()
    if lanes and ledger_all:
        print("\n-- per-lane / per-profile cross-check (B2-1) --")
        for lane_name, feats in sorted(lanes.items()):
            lane_sets = profile_sets(Resolver(load_sources(feats), feats))
            for prof, got in sorted(lane_sets.items()):
                fp = os.path.join(ROOT, "tests", "unit", "fixtures", lane_name, f"{prof}.json")
                if not os.path.exists(fp):
                    continue
                with open(fp) as fh:
                    want = {(e["method"], e["path"]) for e in json.load(fh)["entries"]}
                if got == want:
                    print(f"   ok   {lane_name:18} {prof:8} {len(got):5} routes")
                    continue
                profile_mismatches.append(f"{lane_name}/{prof}")
                print(f"   FAIL {lane_name:18} {prof:8} derived {len(got):5}, fixture {len(want):5}")
                for m, p in sorted(want - got)[:10]:
                    print(f"        derived is missing: {m:6} {p}")
                for m, p in sorted(got - want)[:10]:
                    print(f"        derived has extra:  {m:6} {p}")

    # -- registered_by fidelity (B2-1 step 2) -------------------------------
    # The surface is not the whole contract. `registered_by` is the key the
    # SDK's contract-sync uses to choose a generated directory (see
    # `LEDGER_MODULE_ALIASES` in the fork's `scripts/contract-module-map.mjs`),
    # so a renamed origin silently relocates generated files — a change no
    # route-count gate can see. It is derived from the registering functions
    # (`Resolver.registrars`) plus the 32-rule `ledger_origins.txt`, and this
    # block proves that derivation reproduces every label in both committed
    # fixtures. Without it, deleting the hand-written manifests would trade a
    # verified duplication for an unverified derivation.
    label_mismatches: list = []
    if lanes and ledger_all:
        origins = load_ledger_origins()
        print("\n-- registered_by fidelity (B2-1 step 2) --")
        for lane_name, feats in sorted(lanes.items()):
            fp = os.path.join(ROOT, "tests", "unit", "fixtures", lane_name, "all.json")
            if not os.path.exists(fp):
                continue
            res_lane = Resolver(load_sources(feats), feats)
            profile_sets(res_lane)  # force every root through the tagger
            with open(fp) as fh:
                entries = json.load(fh)["entries"]
            bad = 0
            for e in entries:
                key = (e["method"], e["path"])
                got = resolve_label(e["path"], res_lane.registrars.get(key, set()), origins)
                if got == e["registered_by"]:
                    continue
                bad += 1
                label_mismatches.append(f"{lane_name}: {e['method']} {e['path']}: {got!r} != {e['registered_by']!r}")
            if bad:
                print(f"   FAIL {lane_name:18} {bad} of {len(entries)} labels wrong")
                for line in label_mismatches[-bad:][:10]:
                    print(f"        {line}")
            else:
                print(f"   ok   {lane_name:18} {len(entries):5} labels reproduced")

    # -- emitted cfg gates (B2-1 step 2b) ----------------------------------
    # The generated table carries a `#[cfg(..)]` per group so that the
    # *compiler* does the feature filtering, exactly as the `mod` declarations
    # do today. That only works if `gate_of` reproduces each lane by itself, so
    # evaluate the gates against every lane's feature set and require that
    # lane's committed fixture to come back exactly. The source here is the
    # union resolver, because the emitted table has to cover every lane at once.
    gate_mismatches: list = []
    if lanes and ledger_all:
        union = Resolver(load_sources(), None, mod_gated_files(raw_sources()))
        union_rows = profile_sets(union)["all"]
        print("\n-- emitted cfg gates (B2-1 step 2b) --")
        for lane_name, feats in sorted(lanes.items()):
            fp = os.path.join(ROOT, "tests", "unit", "fixtures", lane_name, "all.json")
            if not os.path.exists(fp):
                continue
            with open(fp) as fh:
                want = {(e["method"], e["path"]) for e in json.load(fh)["entries"]}
            got = {r for r in union_rows if cfg_all_allow(list(union.gate_of(r)), feats)}
            if got == want:
                print(f"   ok   {lane_name:18} gate-filtered == fixture ({len(got)} routes)")
                continue
            gate_mismatches.append(f"{lane_name}: gate-filtered {len(got)} vs fixture {len(want)}")
            print(f"   FAIL {lane_name:18} gate-filtered {len(got)}, fixture {len(want)}")
            for m, p in sorted(want - got)[:8]:
                print(f"        gate hides a real route: {m:6} {p}  gate={sorted(union.gate_of((m, p)))}")
            for m, p in sorted(got - want)[:8]:
                print(f"        gate admits a foreign route: {m:6} {p}")

    # Strict gate. All four properties must hold exactly; there is deliberately
    # no allowlist for the first three, because each one is a statement that the
    # contract is telling the truth and "mostly true" is the failure mode this
    # whole gate exists to catch. The fourth is a ratchet over known-benign
    # non-resolutions.
    #
    # The third property is S-14: without it, a route can be served, work
    # perfectly, and be invisible to every downstream consumer (SDK codegen,
    # ROUTE_CONTRACT.md, the ledger snapshots) forever. `derived - ledger` used
    # to be printed here and then ignored on the grounds that manifests are
    # hand-written and incomplete — which is exactly the condition being fixed,
    # not a reason to tolerate it.
    strict_failures = []
    if only_manifest:
        strict_failures.append(f"{len(only_manifest)} manifest-declared routes were not derived")
    if missed:
        strict_failures.append(f"{len(missed)} ledger routes were not derived")
    if undeclared:
        strict_failures.append(
            f"{len(undeclared)} real routes are absent from both ledger lanes (S-14): "
            "add them to the owning *_route_manifest(), or stop serving them"
        )
    if profile_mismatches:
        strict_failures.append(
            f"{len(profile_mismatches)} lane/profile set(s) disagree with the fixtures (B2-1): "
            f"{profile_mismatches} — a cfg lane or a runtime profile guard is wrong"
        )
    if new_unresolved:
        strict_failures.append(f"{len(new_unresolved)} new unresolved parser constructs")
    if label_mismatches:
        strict_failures.append(
            f"{len(label_mismatches)} routes resolve to the wrong `registered_by` (B2-1 step 2): "
            "fix a rule in scripts/contract/ledger_origins.txt (renaming one moves SDK codegen output)"
        )
    if gate_mismatches:
        strict_failures.append(
            f"{len(gate_mismatches)} lane(s) disagree with the emitted cfg gates (B2-1 step 2b): "
            f"{gate_mismatches} — the generated table would compile the wrong route surface"
        )
    if os.environ.get("EXTRACT_STRICT") == "1" and strict_failures:
        print("\nEXTRACT_STRICT=1: parser self-check failed:", file=sys.stderr)
        for f in strict_failures:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print("\nTop modules by route count:")
    for mod, cnt in sorted(((m, len(v)) for m, v in out.items()), key=lambda x: -x[1])[:15]:
        print(f"  {cnt:4d}  {mod}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
