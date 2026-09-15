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
                head = strip_leading_attrs("".join(cur)).lstrip()
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


def strip_leading_attrs(text: str) -> str:
    """Drop leading `#[...]` / `///` attribute lines from a statement.

    `#[allow(unused_mut)] let mut router = ...` must still be recognised as a
    `let`, and `#[cfg(feature = "voip-tracking")] { router = ... }` as a block.
    """
    s = text.lstrip()
    while True:
        if s.startswith("#["):
            close = match_delim(s, 1)
            if close == -1:
                return s
            s = s[close + 1 :].lstrip()
            continue
        if s.startswith("///") or s.startswith("//!"):
            nl = s.find("\n")
            if nl == -1:
                return ""
            s = s[nl + 1 :].lstrip()
            continue
        return s


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
    """Yield `(name, body)` for every `fn name(...) { body }` in `src`."""
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
        yield m.group(1), src[brace + 1 : end]


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
    def __init__(self, files: dict[str, str]):
        self.files = files  # relpath -> comment-stripped source
        # `self.fn_all` keeps every definition distinct. Keying by
        # `(file, name)` would collide: `route_module.rs` defines 11 separate
        # `merge_into` methods, and collapsing them loses 10 modules' routes.
        self.fn_all: list[tuple[str, str, str]] = []  # (file, name, body)
        self.consts: dict[str, list[tuple[str, str]]] = defaultdict(list)
        for rel, src in files.items():
            for name, body in iter_fns(src):
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
        return [(m.group(1).upper(), m.group(2), owner) for m in _RE_TUPLE.finditer(t)]

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
        env: dict = {}
        result: list = []

        for stmt in split_statements(body):
            s = strip_leading_attrs(stmt)
            if not s:
                continue
            if s.startswith("//") or s.startswith("#"):
                continue

            # `#[cfg(..)] { router = router.route(..) }` and plain blocks:
            # recurse so feature-gated registrations are not dropped.
            if s.startswith("{"):
                inner = match_delim(s, 0)
                nested = s[1:inner] if inner != -1 else s[1:]
                result.extend(self.eval_fn_body(name, nested, owner))
                continue

            # `if cond { .. }` / `match` / `for` / `while` / `else { .. }`
            if re.match(r"(if|match|for|while|loop|else)\b", s):
                for blk in extract_blocks(s):
                    result.extend(self.eval_fn_body(name, blk, owner))
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

        if memo_key is not None:
            self._memo[memo_key] = result
        return list(result)

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
            return acc + [(m, path, owner) for m in methods]

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
            return acc + [(meth, prefix + path, own) for (meth, path, own) in subs]

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


def load_sources() -> dict[str, str]:
    files = {}
    for dp, _, fns in os.walk(ROUTES_DIR):
        for f in sorted(fns):
            if not f.endswith(".rs"):
                continue
            fp = os.path.join(dp, f)
            rel = os.path.relpath(fp, ROUTES_DIR)
            if "tests" in rel.split(os.sep):
                continue
            with open(fp, encoding="utf-8", errors="replace") as fh:
                src = strip_comments(fh.read())
            files[rel] = strip_test_mods(src)
    return files


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
    # difference. The reverse difference is expected — this script reads source
    # text and therefore sees feature-gated routers the default build omits.
    ledger: set = set()
    for prof in ("default", "worker", "all"):
        fp = os.path.join(ROOT, "tests", "unit", "fixtures", "ledger_export", f"{prof}.json")
        if not os.path.exists(fp):
            continue
        with open(fp) as fh:
            for e in json.load(fh)["entries"]:
                ledger.add((e["method"], e["path"]))
    missed: list = []
    if ledger:
        missed = sorted(ledger - router_set)
        print(f"\n-- cross-check vs ledger_export fixtures ({len(ledger)} distinct tuples) --")
        print(f"ledger routes NOT derived  : {len(missed)}   <- must be 0")
        for m, p in missed[:20]:
            print(f"    {m:6} {p}")
        print(f"derived but not in ledger  : {len(router_set - ledger)}   <- feature-gated / manifest gaps")

    # Strict gate: two properties that must hold exactly. The residual
    # `derived-but-not-declared` set is expected (manifests are hand-written and
    # incomplete) and is reported rather than enforced.
    strict_failures = []
    if only_manifest:
        strict_failures.append(f"{len(only_manifest)} manifest-declared routes were not derived")
    if missed:
        strict_failures.append(f"{len(missed)} ledger routes were not derived")
    if new_unresolved:
        strict_failures.append(f"{len(new_unresolved)} new unresolved parser constructs")
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
