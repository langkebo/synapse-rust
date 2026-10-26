#!/usr/bin/env python3
"""Every route must either take an auth extractor in its handler signature, or
be declared with a reason in `route_auth_public.txt` (SEC-15).

背景
----
SEC-15 的原记法是「鉴权覆盖率约 94.4%」——一个**静态快照**，没有判定规则、
没有不可绕过的检查。于是它无法失败：新加一条完全不要凭证的路由，覆盖率
数字只是变小，没有任何门禁变红；而「变小」这件事谁也不会注意到。

本脚本把「鉴权」变成一个可判定的不变式：

    每条路由，要么 handler 签名里出现 `AuthenticatedUser` / `AdminUser` /
    `OptionalAuthenticatedUser`（标准提取器），要么在
    `scripts/contract/route_auth_public.txt` 里登记 kind + 理由。

为什么需要白名单而不是只看签名
------------------------------
两种「已鉴权」在源码里看不见：

* **route_layer 中间件**。`federation_auth_middleware`（X-Matrix 签名）、
  `admin_auth_middleware`、`replication_http_auth_middleware`、
  `webhook_auth_guard` 都是 `.route_layer(...)` 挂的。抽取器把 `layer` /
  `route_layer` 当透明装饰器（`extract_registered.apply_call`），handler 签名
  里自然什么都没有。
* **handler 自行校验凭证**。`proxy_to_as` / `whoami` / appservice 的
  `extract_as_token + validate_token` 都在函数体里。

所以白名单是「本仓对『这条路由凭什么不需要标准提取器』的显式回答」，
不是豁免清单：**没登记就是红**。

双向判定
--------
* **漏登记**：路由既无提取器、又不在白名单 ⇒ 红。
* **陈旧登记**：白名单条目对应的路由已不存在 ⇒ 红。
* **陈旧登记**：白名单条目对应的 handler 现在带了提取器 ⇒ 红（该删条目）。
* **格式错误**：字段数 / kind 取值 / 空理由 / 重复条目 ⇒ 红。

覆盖率口径（本脚本打印，审计文档引用同一组数字）
----------------------------------------------
`kind = public` 是**真正无凭证**的路由；`federation` / `middleware` /
`secret` / `manual` 都要求某种凭证。因此

    鉴权覆盖率 = 1 - public / 全部路由

用法
----
    python3 scripts/contract/check_route_auth.py             # 门禁
    python3 scripts/contract/check_route_auth.py --self-test # 红/绿自证
"""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
WHITELIST = ROOT / "scripts" / "contract" / "route_auth_public.txt"

# Standard extractors: their presence in the handler's parameter list is the
# falsifiable evidence that the route authenticates.
AUTH_EXTRACTORS = (
    "AuthenticatedUser",
    "AdminUser",
    "OptionalAuthenticatedUser",
)

# `public` is the only kind that means "no credential at all".
KINDS = ("public", "federation", "middleware", "secret", "manual")


def has_extractor(params: str) -> bool:
    """True when a handler's rendered parameter list takes a standard extractor."""
    return any(token in params for token in AUTH_EXTRACTORS)


def parse_entries(text: str) -> tuple[dict, list]:
    """`{(METHOD, path): (kind, reason)}` plus a list of format problems.

    Format is pinned by the header of `route_auth_public.txt`:
    `METHOD PATH<TAB>kind<TAB>reason`. Exactly two tabs, so a stray tab cannot
    silently swallow part of the reason or become part of the kind.
    """
    entries: dict = {}
    problems: list = []
    for lineno, raw in enumerate(text.splitlines(), 1):
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        parts = raw.split("\t")
        if len(parts) != 3:
            problems.append(
                f"route_auth_public.txt:{lineno}: expected 3 tab-separated "
                f"fields (address, kind, reason), got {len(parts)}"
            )
            continue
        address, kind, reason = (p.strip() for p in parts)
        tokens = address.split()
        if len(tokens) != 2:
            problems.append(
                f"route_auth_public.txt:{lineno}: address must be "
                f"`METHOD PATH`, got {address!r}"
            )
            continue
        method, path = tokens[0].upper(), tokens[1]
        if kind not in KINDS:
            problems.append(
                f"route_auth_public.txt:{lineno}: unknown kind {kind!r} "
                f"(expected one of {', '.join(KINDS)})"
            )
            continue
        if not reason:
            problems.append(
                f"route_auth_public.txt:{lineno}: empty reason for {method} {path}"
            )
            continue
        key = (method, path)
        if key in entries:
            problems.append(
                f"route_auth_public.txt:{lineno}: duplicate entry for {method} {path}"
            )
            continue
        entries[key] = (kind, reason)
    return entries, problems


def analyze(sig_auth: set, entries: dict, known: set) -> list:
    """Failure strings (empty == green). Pure, so `--self-test` can drive it."""
    failures = []
    for method, path in sorted(known - sig_auth - set(entries)):
        failures.append(
            f"{method} {path}: handler takes no auth extractor and the route is "
            f"not declared — add `AuthenticatedUser`/`AdminUser`, or record why "
            f"it needs no standard extractor in scripts/contract/route_auth_public.txt"
        )
    for method, path in sorted(set(entries) - known):
        failures.append(
            f"{method} {path}: declared in route_auth_public.txt but the route no "
            f"longer exists — drop the entry"
        )
    for method, path in sorted(set(entries) & sig_auth):
        failures.append(
            f"{method} {path}: declared in route_auth_public.txt but the handler "
            f"now takes an auth extractor — drop the entry"
        )
    return failures


def load_resolver():
    """Import the shared extractor and return a fully-resolved route table.

    `all_routes()` must run before `handlers` is populated; it is the extractor's
    documented contract, not an accident.
    """
    spec = importlib.util.spec_from_file_location(
        "extract_registered",
        str(Path(__file__).resolve().parent / "extract_registered.py"),
    )
    extract_registered = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(extract_registered)

    resolver = extract_registered.Resolver(extract_registered.load_sources())
    return resolver, resolver.all_routes()


def signature_auth(resolver, key) -> bool:
    """True when *every* owner of `key` takes a standard auth extractor.

    `all()` (not `any()`): a route merged from two registrars where only one
    variant authenticates is not authenticated, so it must be declared.
    Unresolvable handlers (closures, cross-file ambiguity) are not evidence of
    auth either.
    """
    owners = sorted(resolver.handlers.get(key) or set())
    if not owners:
        return False
    for owner, handler in owners:
        index = resolver.resolve_handler_def(handler, owner)
        if index is None:
            return False
        file, name, _body = resolver.fn_all[index]
        params = " ".join(resolver.fn_sigs.get((file, name), []))
        if not has_extractor(params):
            return False
    return True


def self_test() -> int:
    checks = []

    def expect(name, got, want):
        checks.append((name, got, want))

    # --- signature classification -------------------------------------------
    expect(
        "AuthenticatedUser counts as an extractor",
        has_extractor("State(ctx): State<AppState>, user: AuthenticatedUser"),
        True,
    )
    expect(
        "AdminUser counts as an extractor",
        has_extractor("State(ctx): State<AdminContext>, _admin: AdminUser"),
        True,
    )
    expect(
        "OptionalAuthenticatedUser counts as an extractor",
        has_extractor("user: OptionalAuthenticatedUser"),
        True,
    )
    expect(
        "HeaderMap alone is not an extractor",
        has_extractor("State(ctx): State<AppState>, headers: HeaderMap"),
        False,
    )
    expect(
        "a path param named user_id is not an extractor",
        has_extractor("Path(user_id): Path<UserId>"),
        False,
    )

    # --- whitelist parsing ---------------------------------------------------
    good = "GET /_x\tpublic\tpublic by design\n"
    expect(
        "well-formed entry parses",
        parse_entries(good)[0],
        {("GET", "/_x"): ("public", "public by design")},
    )
    expect("comments and blanks are skipped", parse_entries("# c\n\n")[0], {})
    expect(
        "missing tab field is a problem",
        len(parse_entries("GET /_x\tpublic\n")[1]),
        1,
    )
    expect(
        "unknown kind is a problem",
        len(parse_entries("GET /_x\tbogus\treason\n")[1]),
        1,
    )
    expect(
        "empty reason is a problem",
        len(parse_entries("GET /_x\tpublic\t\n")[1]),
        1,
    )
    expect(
        "duplicate entry is a problem",
        len(parse_entries("GET /_x\tpublic\tr\nGET /_x\tsecret\tr2\n")[1]),
        1,
    )

    # --- bidirectional guard -------------------------------------------------
    expect(
        "undeclared unauthenticated route is RED",
        bool(analyze(set(), {}, {("GET", "/_open")})),
        True,
    )
    expect(
        "declared unauthenticated route is GREEN",
        analyze(set(), {("GET", "/_open"): ("public", "r")}, {("GET", "/_open")}),
        [],
    )
    expect(
        "signature-authenticated route needs no entry",
        analyze({("GET", "/_me")}, {}, {("GET", "/_me")}),
        [],
    )
    expect(
        "entry for a vanished route is RED",
        bool(analyze(set(), {("GET", "/_gone"): ("public", "r")}, set())),
        True,
    )
    expect(
        "entry for a now-authenticated route is RED",
        bool(
            analyze(
                {("GET", "/_me")}, {("GET", "/_me"): ("public", "r")}, {("GET", "/_me")}
            )
        ),
        True,
    )

    bad = [c for c in checks if c[1] != c[2]]
    for name, got, want in checks:
        print(f"  {'ok  ' if got == want else 'FAIL'} {name} — got {got!r}")
    if bad:
        print(f"❌ self-test failed for {len(bad)} case(s)")
        return 1
    print(f"✅ self-test: {len(checks)} classifier/guard cases behave as declared")
    return 0


def main(argv) -> int:
    if "--self-test" in argv:
        return self_test()

    text = WHITELIST.read_text(encoding="utf-8") if WHITELIST.exists() else ""
    entries, problems = parse_entries(text)

    resolver, routes = load_resolver()
    sig_auth = {(m, p) for (m, p) in routes if signature_auth(resolver, (m, p))}
    failures = problems + analyze(sig_auth, entries, routes)

    per_kind = {kind: 0 for kind in KINDS}
    for kind, _reason in entries.values():
        per_kind[kind] += 1
    total = len(routes)
    open_routes = per_kind["public"]
    coverage = (total - open_routes) / total * 100 if total else 100.0
    breakdown = " · ".join(f"{k} {per_kind[k]}" for k in KINDS)

    if failures:
        print("❌ route auth coverage is not declared consistently:")
        for failure in failures:
            print(f"   - {failure}")
        print(
            "\n   Source of truth: scripts/contract/route_auth_public.txt. "
            "See audit SEC-15."
        )
        return 1

    print(
        f"✅ {total} route(s): {len(sig_auth)} declare an auth extractor, "
        f"{len(entries)} declared ({breakdown})"
    )
    print(
        f"   credential required by {total - open_routes}/{total} = {coverage:.1f}%; "
        f"open {open_routes} ({open_routes / total * 100:.1f}%)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
