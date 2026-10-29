#!/usr/bin/env python3
"""写事件的路由必须在 `write_stream_map.txt` 里被登记成某一路流（P1-3 的静态那一半）。

## 为什么需要它

`write_stream_map.txt` 的双向棘轮（`tests/unit/worker_write_stream_tests.rs`）比的是
**路由集合**，不是 handler 的行为。它自己的头注释就写着这个盲区：

> 一条已经存在的路由**开始**写事件（例如 `v1/invite` 将来补上落库），这份清单不会变红。

于是"某个 admin handler 顺手写了一条 room event"可以完全无声地穿过单写者约束：
路由没变、清单没变、`stream_for_write()` 没变，而那一路流已经悄悄变成多写者。
本脚本把 handler 行为拉进判据。

## 判据

对路由表里的每条 `(method, path)`：

1. 取 `Resolver.handlers` 记录的 `(owner_file, handler)`，解析到函数体；
2. 函数体里出现写事件入口的**方法调用**（`.create_event(` / `.send_message(` /
   `.join_room(` 等，见 `WRITER_METHODS`），或经由 route 树内 helper（最多 3 跳，
   同文件优先、全局唯一名次之）出现 ⇒ 该路由是「事件写路由」；
3. 事件写路由必须在 `write_stream_map.txt` 命中至少一条，且**每条命中项的 stream
   都不能是 `-`**（`-` 的含义是"这不是任何一路流的写"，见该文件头注释）；
4. 例外登记在 `scripts/contract/event_writer_allowlist.txt`（三列
   `handler METHOD path`，每列可写 `*`，`#` 后必须写理由）。

## 为什么用白名单而不是把匹配做窄

`admin_audit_service.create_event(...)` 与 `RoomService::create_event` 同名，但写的是
审计表；`space_service.invite_user(...)` 写 `space_events` 而不是房间 DAG。把匹配改成
"看 receiver 链条"会让判据变成一份同样要维护、却更容易被穿透的规则（换个 service
名就绕过了）。宁可**误报一次、登记一条带理由的例外**：白名单是显式回答，漏登记就是红。

## 双向

* **漏登记**：事件写路由既没登记成流，也不在白名单 ⇒ 红；
* **陈旧登记**：白名单条目没有命中任何"本来会红"的路由 ⇒ 红（路由删了 / 挪了 /
  不再写事件了）。

## 已知边界

只解析静态可解析的 handler：闭包、跨 crate 的动态派发不可见；helper 最多跟 3 跳。
解析不到的 handler 不参与判定，其条数打印在输出里（`--print` 列明细）。

## 用法

    python3 scripts/ci/check_event_writer_routes.py             # 门禁
    python3 scripts/ci/check_event_writer_routes.py --print     # 打印事件写路由与归属
    python3 scripts/ci/check_event_writer_routes.py --self-test # 判据自证（纯函数）

退出码：0 = 全部登记一致；1 = 有违规；2 = 输入缺失/解析失败（fail-closed）。
"""

from __future__ import annotations

import importlib.util
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
WRITE_STREAM_MAP = ROOT / "scripts" / "contract" / "write_stream_map.txt"
ALLOWLIST = ROOT / "scripts" / "contract" / "event_writer_allowlist.txt"

# 写事件入口：`RoomService` / `EventStorage` / `MembershipService` 上会落 room DAG 的方法。
# 宁可宽（白名单兜底）也不要窄：漏一个方法的代价是整条守卫对它失效。
WRITER_METHODS = (
    "create_event",
    "create_event_with_graph",
    "create_outlier_event",
    "send_message",
    "send_message_with_txn",
    "join_room",
    "join_room_with_reason",
    "join_room_with_via_servers_and_reason",
    "leave_room",
    "invite_user",
    "ban_user",
    "kick_user",
    "unban_user",
    "knock_room",
    "create_room",
    "redact_event",
    "send_state_event",
    "put_state_event",
)

_WRITER_RE = re.compile(r"\.\s*(" + "|".join(WRITER_METHODS) + r")\s*\(")

# 裸调用（helper 转发）：`invite_by_3pid(...)`、`record_audit_event(...)`。
# 排除方法调用（前面是 `.`）、路径（前面是 `:`）与关键字。
_CALL_RE = re.compile(r"(?<![\w.:])([a-z_][a-z0-9_]*)\s*\(")
_KEYWORDS = frozenset(
    {
        "if",
        "match",
        "while",
        "for",
        "loop",
        "return",
        "else",
        "let",
        "vec",
        "json",
        "format",
        "println",
        "write",
        "writeln",
        "debug",
        "info",
        "warn",
        "error",
        "trace",
    }
)

MAX_HELPER_DEPTH = 3


def normalize(path: str) -> str:
    """`{room_id}` / `{room_id_or_alias}` ⇒ `{}`，使路由树与清单可比。"""
    return re.sub(r"\{[^}]*\}", "{}", path)


def parse_map(text: str):
    """`[(METHOD, normalized_path, raw_path, stream)]` plus format problems.

    The file is TAB-separated with exactly three fields; a different shape would
    silently shift a stream into the path column and make every lookup "unmapped"
    (which reads like a real finding). So a malformed line is a hard problem.
    """
    entries = []
    problems = []
    for lineno, raw in enumerate(text.splitlines(), 1):
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        parts = raw.split("\t")
        if len(parts) != 3:
            problems.append(
                f"write_stream_map.txt:{lineno}: expected 3 tab-separated fields "
                f"(METHOD, path, stream), got {len(parts)}"
            )
            continue
        method, path, stream = (p.strip() for p in parts)
        entries.append((method.upper(), normalize(path), path, stream))
    return entries, problems


def parse_allowlist(text: str):
    """`[(handler, method, path, reason, lineno)]` plus format problems."""
    entries = []
    problems = []
    for lineno, raw in enumerate(text.splitlines(), 1):
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        body, sep, reason = raw.partition("#")
        fields = body.split()
        if len(fields) != 3:
            problems.append(
                f"event_writer_allowlist.txt:{lineno}: expected `handler METHOD path` "
                f"before `#`, got {len(fields)} field(s): {body.strip()!r}"
            )
            continue
        if not sep or not reason.strip():
            problems.append(
                f"event_writer_allowlist.txt:{lineno}: entry needs a `# reason`"
            )
            continue
        handler, method, path = fields
        entries.append((handler, method.upper(), path, reason.strip(), lineno))
    return entries, problems


def path_matches(route_path: str, pattern: str) -> bool:
    """`*`, exact (normalized), or a `/`-boundary suffix (nested registrars)."""
    if pattern == "*":
        return True
    route = normalize(route_path)
    want = normalize(pattern)
    if route == want:
        return True
    return route.endswith("/" + want.lstrip("/"))


def lookup(entries, method: str, path: str):
    """Every map row addressed by `(method, path)`, exact matches first."""
    exact = [e for e in entries if e[0] == method and e[1] == normalize(path)]
    if exact:
        return exact
    return [e for e in entries if e[0] == method and path_matches(path, e[2])]


def matches_entry(entry, method: str, path: str, handler: str) -> bool:
    pattern_handler, pattern_method, pattern_path = entry[0], entry[1], entry[2]
    if pattern_handler != "*" and pattern_handler != handler:
        return False
    if pattern_method != "*" and pattern_method != method:
        return False
    return path_matches(path, pattern_path)


def analyze(findings, map_entries, allow, map_problems, allow_problems):
    """Failure strings (empty == green). Pure, so `--self-test` can drive it."""
    failures = list(map_problems) + list(allow_problems)
    used = set()
    for finding in findings:
        method, path, file, handler, writer, via = finding
        hits = lookup(map_entries, method, path)
        if hits and all(row[3] != "-" for row in hits):
            continue
        exempt = [
            index
            for index, entry in enumerate(allow)
            if matches_entry(entry, method, path, handler)
        ]
        if exempt:
            used.update(exempt)
            continue
        target = f"{method} {path} (handler `{handler}` in {file})"
        if via:
            chain = f"reaches `.{writer}(` via {via}"
        else:
            chain = f"calls `.{writer}(`"
        if not hits:
            failures.append(
                f"{target} {chain} but is not in scripts/contract/write_stream_map.txt "
                f"— register the stream it writes, or record why it is not an "
                f"`events` write in scripts/contract/event_writer_allowlist.txt"
            )
        else:
            raw = ", ".join(row[2] for row in hits)
            failures.append(
                f"{target} {chain} but write_stream_map.txt declares `-` for {raw} "
                f"— fix the stream column, or record the local-write exception in "
                f"scripts/contract/event_writer_allowlist.txt"
            )
    for index, entry in enumerate(allow):
        if index in used:
            continue
        handler, method, path, _reason, lineno = entry
        failures.append(
            f"event_writer_allowlist.txt:{lineno}: `{handler} {method} {path}` no "
            f"longer exempts anything (route gone, moved, or no longer writes events) "
            f"— drop the entry"
        )
    return failures


class FnIndex:
    """`(file, name) -> body` plus `name -> files`, built once from the resolver."""

    def __init__(self, resolver) -> None:
        self.bodies: dict = {}
        self.by_name: dict = {}
        for file, name, body in resolver.fn_all:
            self.bodies.setdefault((file, name), body)
            self.by_name.setdefault(name, set()).add(file)

    def body(self, file: str, name: str):
        return self.bodies.get((file, name))

    def unique_file(self, name: str):
        files = self.by_name.get(name) or set()
        return next(iter(files)) if len(files) == 1 else None


def writer_of(index: FnIndex, file: str, name: str, seen: set, depth: int = 0):
    """`(writer, via)` when `file::name` writes room events, else `None`.

    `via` is the helper chain (empty for a direct call), so a failure message can
    say *how* the write was reached instead of pointing at a handler that looks
    innocent.
    """
    key = (file, name)
    if key in seen or depth > MAX_HELPER_DEPTH:
        return None
    seen.add(key)
    body = index.body(file, name)
    if body is None:
        return None
    hit = _WRITER_RE.search(body)
    if hit:
        return hit.group(1), ""
    for call in _CALL_RE.finditer(body):
        callee = call.group(1)
        if callee in _KEYWORDS or callee in WRITER_METHODS:
            continue
        target = (
            file if index.body(file, callee) is not None else index.unique_file(callee)
        )
        if target is None:
            continue
        found = writer_of(index, target, callee, seen, depth + 1)
        if found:
            writer, tail = found
            chain = f"{callee}->{tail}" if tail else callee
            return writer, chain
    return None


def scan(resolver):
    """`(findings, unresolved)` over the whole route table."""
    findings = []
    unresolved = []
    routes = resolver.all_routes()
    for method, path in sorted(routes):
        owners = sorted(resolver.handlers.get((method, path)) or [])
        if not owners:
            unresolved.append((method, path, "<no handler recorded>"))
            continue
        for owner, handler in owners:
            index = resolver.resolve_handler_def(handler, owner)
            if index is None:
                unresolved.append((method, path, f"{owner}::{handler}"))
                continue
            file, name, _body = resolver.fn_all[index]
            found = writer_of(FnIndexCache.get(resolver), file, name, set())
            if found:
                writer, via = found
                findings.append((method, path, file, handler, writer, via))
    return findings, unresolved


class FnIndexCache:
    """`FnIndex` is O(fn count); rebuilding it per route would dominate the run."""

    _index = None

    @classmethod
    def get(cls, resolver) -> FnIndex:
        if cls._index is None:
            cls._index = FnIndex(resolver)
        return cls._index


def load_resolver():
    spec = importlib.util.spec_from_file_location(
        "extract_registered",
        str(
            Path(__file__).resolve().parent.parent
            / "contract"
            / "extract_registered.py"
        ),
    )
    extract_registered = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(extract_registered)
    resolver = extract_registered.Resolver(extract_registered.load_sources())
    return resolver


def self_test() -> int:
    checks = []

    def expect(name, got, want):
        checks.append((name, got, want))

    expect("param placeholders normalize", normalize("/a/{room_id}/b"), "/a/{}/b")
    expect("path exact match", path_matches("/a/b", "/a/b"), True)
    expect("path suffix match under a prefix", path_matches("/v1/a/b", "/a/b"), True)
    expect(
        "suffix must land on a segment boundary", path_matches("/v1/xb", "/a/b"), False
    )
    expect("wildcard path matches", path_matches("/v1/a/b", "*"), True)

    rows, problems = parse_map("# c\nPOST\t/a/b\tevents\nGET\t/a/c\t-\n")
    expect(
        "map parses",
        [(r[0], r[1], r[3]) for r in rows],
        [("POST", "/a/b", "events"), ("GET", "/a/c", "-")],
    )
    expect("map comments skipped", problems, [])
    expect("map shape problem", len(parse_map("POST\t/a/b\n")[1]), 1)

    allow, aproblems = parse_allowlist("h POST /a/b  # local write\n")
    expect(
        "allowlist parses", [(a[0], a[1], a[2]) for a in allow], [("h", "POST", "/a/b")]
    )
    expect("allowlist needs a reason", len(parse_allowlist("h POST /a/b\n")[1]), 1)
    expect(
        "allowlist needs three fields",
        len(parse_allowlist("h POST /a/b extra # r\n")[1]),
        1,
    )

    findings = [
        ("POST", "/a/b", "f.rs", "handler", "create_event", ""),
        ("POST", "/a/c", "f.rs", "other", "create_event", "helper"),
        ("POST", "/a/d", "f.rs", "exempt", "create_event", ""),
    ]
    map_entries = [
        ("POST", "/a/b", "/a/b", "events"),
        ("POST", "/a/c", "/a/c", "-"),
        ("POST", "/a/d", "/a/d", "-"),
    ]
    allow_entries = [("exempt", "POST", "/a/d", "writes the audit table", 1)]
    expect(
        "declared stream passes, dash and unmapped are RED, allowlist silences one",
        analyze(findings, map_entries, allow_entries, [], []),
        [
            "POST /a/c (handler `other` in f.rs) reaches `.create_event(` via helper but "
            "write_stream_map.txt declares `-` for /a/c — fix the stream column, or record "
            "the local-write exception in scripts/contract/event_writer_allowlist.txt"
        ],
    )
    expect(
        "stale allowlist entry is RED",
        len(analyze([], map_entries, allow_entries, [], [])),
        1,
    )
    expect(
        "unmapped writer is RED",
        len(analyze([findings[0]], [], [], [], [])),
        1,
    )
    expect(
        "an exact map row wins over a suffix row",
        analyze(
            findings[:1],
            [("POST", "/a/b", "/a/b", "events"), ("POST", "/x/a/b", "/x/a/b", "-")],
            [],
            [],
            [],
        ),
        [],
    )
    expect(
        "several suffix rows, one dash ⇒ RED (fail-closed on ambiguity)",
        len(
            analyze(
                findings[:1],
                [
                    ("POST", "/x/a/b", "/x/a/b", "events"),
                    ("POST", "/y/a/b", "/y/a/b", "-"),
                ],
                [],
                [],
                [],
            )
        ),
        1,
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
    if not WRITE_STREAM_MAP.is_file():
        print(f"❌ missing {WRITE_STREAM_MAP}")
        return 2

    map_entries, map_problems = parse_map(WRITE_STREAM_MAP.read_text(encoding="utf-8"))
    if not map_entries:
        print(f"❌ {WRITE_STREAM_MAP} has no entries")
        return 2
    allow_text = ALLOWLIST.read_text(encoding="utf-8") if ALLOWLIST.is_file() else ""
    allow, allow_problems = parse_allowlist(allow_text)

    try:
        resolver = load_resolver()
        findings, unresolved = scan(resolver)
    except Exception as error:  # noqa: BLE001 - fail-closed: an extractor bug must not read as green
        print(f"❌ route extraction failed: {error}")
        return 2

    if "--print" in argv:
        for method, path, file, handler, writer, via in findings:
            chain = f".{writer}(" if not via else f".{writer}( via {via}"
            print(f"  {method:6} {path:60} {handler:28} {chain}   [{file}]")
        for method, path, why in unresolved:
            print(f"  ??      {method:6} {path:60} {why}")

    failures = analyze(findings, map_entries, allow, map_problems, allow_problems)
    if failures:
        print("❌ event-writing routes are not declared consistently:")
        for failure in failures:
            print(f"   - {failure}")
        print(
            "\n   Source of truth: scripts/contract/write_stream_map.txt. "
            "See audit P1-3 / §4-A3."
        )
        return 1

    print(
        f"✅ {len(findings)} event-writing route(s) all declare a stream "
        f"({len(allow)} exception(s), {len(unresolved)} handler(s) not statically resolvable)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
