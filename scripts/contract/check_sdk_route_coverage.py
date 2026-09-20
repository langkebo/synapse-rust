#!/usr/bin/env python3
"""B2-4b — assert the endpoints the SDK actually calls are served by the ledger.

方向与 B2-4a 相反：B2-4a 断言"后端已服务的 ⊆ 已登记"（后端别偷偷多出端点），
这里断言"SDK 真实调用的 ⊆ 后端已登记"（后端别欠 SDK 端点）。

为什么不能用 route-table 作判据
--------------------------------
SDK 里每个模块都有 `__generated__/route-table.ts`，看起来是现成的"SDK 调用了什么"
清单。但 H-5 与 B1-2 都实测过：**它只增不减**（r0 拆除后仍留着 r0 条目），是
"曾经登记过"而不是"现在在调"。所以本脚本只读 **manager 源码里的 URL 字面量**。

为什么"对齐匹配"而不猜前缀
--------------------------------
SDK 有三种构造风格：
  1. 请求对象内联：`{ method: Method.Post, path, prefix: ClientPrefix.V3 }`
  2. 路径构造器：`buildXPath()` 只返回相对路径，prefix 由**调用方**给
     （`client-*-requests.ts` 一族）
  3. 浏览器 URL：`http.getUrl(path)`，前缀来自 client.baseUrl
用"取附近的 prefix:"猜前缀在风格 2/3 上会**串味**（实测把后面某个请求的
VendorPrefix 错配到前面的 v3 路径上），既会假阴也会假阳。所以主判据是：
**SDK 相对路径必须能对齐到某条 ledger 路径的某一段起点上**（等价于"存在某个前缀，
后端真的服务它"）。只有当 method 与 prefix 出现在**同一行**的请求对象里（风格 1，
无歧义）时才额外校验 (method, 完整路径)。

对齐规则（见 `path_match`）：拿 SDK 相对路径的**首段**去 ledger 路径里找落点，
落点之后剩余段数必须与 SDK 段数相等，且逐段相等（`{}` 通配、字面量必须相同）。
"首段锚定 + 剩余段数相等"这两条一起把假阳堵死：
`/rooms/{}/guest_access` 不会命中 `/rooms/{}/state/{event_type}`
（在 `rooms` 落点后后端还剩 4 段，SDK 只有 3 段），而
`/rooms/$roomId/messages` 能正常命中 `/_matrix/client/v3/rooms/{room_id}/messages`。

用法
----
  python3 scripts/contract/check_sdk_route_coverage.py
  SDK_ROOT=/path/to/matrix-js-sdk python3 scripts/contract/check_sdk_route_coverage.py
  SDK_CONTRACT_STRICT=1 ...   # SDK 缺失时由"报告并跳过"升级为硬失败（CI 用）

不必依赖 SDK 的部分（任何环境都会跑）
--------------------------------------
  * 谓词自检：4 项断言，同时覆盖漏报与误报两个方向；
  * 豁免清单卫生检查：被豁免的形状必须**仍然不被 ledger 服务** —— 后端补上端点后
    豁免若不删，此后真实的不一致会被它静默吃掉。CI 只 checkout 本仓、拿不到 SDK，
    这两项是那时唯一的护栏，所以刻意放在 SDK 存在性判断之前。

退出码
------
  0 = 全部被覆盖（或 SDK 不存在且未开 STRICT，且豁免清单卫生检查通过）
  1 = 存在未被覆盖的端点，或 method 不匹配，或 allowlist 有 stale/无意义条目，
      或 enable 了 STRICT 但 SDK 缺失
"""

from __future__ import annotations

import json
import os
import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
LEDGER = REPO / "tests/unit/fixtures/ledger_export_sdk/all.json"
ALLOWLIST = REPO / "scripts/contract/sdk_uncovered_allowlist.txt"

# ledger 路径里 {name} 与 SDK 路径里 $name 都归一成通配段
_PLACEHOLDER = re.compile(r"\{\w+\}")
_SDK_VAR = re.compile(r"\$\w+")
_ENCODE_URI = re.compile(r'encodeUri\(\s*"(/[^"]*)"')
# 请求对象里的 method / prefix（不再要求同行，改为在站点窗口内唯一出现）
_INLINE_METHOD = re.compile(r"method:\s*Method\.(\w+)")
_INLINE_PREFIX = re.compile(r"prefix:\s*([A-Za-z_][\w.]*)\s*[,}\n\r]")
# prefix 拼接（`ClientPrefix.Unstable + "/org.matrix.msc2965"`）→ 放弃收紧
_PREFIX_CONCAT = re.compile(r"prefix:[^\n]*\+")
# prefix.ts 的三种定义形态
_ENUM_OPEN = re.compile(r"^\s*(?:export\s+)?enum\s+(\w+)\s*\{")
_ENUM_MEMBER = re.compile(r'^\s*(\w+)\s*=\s*"(/_matrix[^"]*)"')
_CONST_PREFIX = re.compile(r'^\s*(?:export\s+)?const\s+(\w+)\s*=\s*"(/_matrix[^"]*)"')


def segments(path: str) -> list[str]:
    """Split into path segments, collapsing both placeholder styles to `{}`."""
    path = _PLACEHOLDER.sub("{}", path)
    path = _SDK_VAR.sub("{}", path)
    return [part for part in path.split("/") if part]


def canonical_shape(path: str) -> str:
    """`/rooms/$roomId/x` 与 `/rooms/{room_id}/x` → `/rooms/{}/x`。

    allowlist 用它与站点做比较，这样 SDK 里把 `$roomId` 改名成 `$id` 不会让
    豁免条目悄悄失效（失效会导致误报，或更糟：一条 stale 条目被当成有效而放过）。
    """
    return "/" + "/".join(segments(path))


def path_match(relative: str, backend_path: str) -> bool:
    """Can `relative` (SDK side) be aligned onto `backend_path`'s tail?

    SDK relative paths omit the `/_matrix/client/v3` style head, so we look for
    an alignment start whose **first segment equals `relative`'s first segment**
    and whose remaining segment count equals `relative`'s. Then every pair must
    agree: `{}` on either side is a wildcard, two literals must be identical.

    Anchoring on the first segment (and demanding equal remaining counts) is
    what stops `/rooms/{}/guest_access` from matching
    `/rooms/{}/state/{event_type}` — the `rooms` alignment leaves 4 segments
    on the backend but only 3 on the SDK side, so the pair is rejected instead
    of matching `state`/`guest_access` against wildcards.
    """
    want, have = segments(relative), segments(backend_path)
    if not want:
        return False
    for start, segment in enumerate(have):
        if segment != want[0]:
            continue
        if len(have) - start != len(want):
            continue
        if all(w == h or w == "{}" or h == "{}" for w, h in zip(want, have[start:])):
            return True
    return False


def load_backend() -> list[tuple[str, str]]:
    ledger = json.loads(LEDGER.read_text())
    return [(entry["method"].upper(), entry["path"]) for entry in ledger["entries"]]


def load_allowlist() -> dict[str, str]:
    """`<METHOD|*> <路径形状> # 理由` → {「方法 归一形状」: 理由}。"""
    if not ALLOWLIST.exists():
        return {}
    allowed: dict[str, str] = {}
    for line in ALLOWLIST.read_text().splitlines():
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        body, _, reason = line.partition("#")
        parts = body.split()
        if len(parts) < 2:
            raise SystemExit(f"{ALLOWLIST}: 无法解析的条目: {line!r}")
        if not reason.strip():
            raise SystemExit(f"{ALLOWLIST}: 条目缺少理由（`# ...`）: {line!r}")
        allowed[f"{parts[0]} {canonical_shape(parts[1])}"] = reason.strip()
    return allowed


# 站点后续多少行内寻找请求对象的 method（实测 `const path = encodeUri(...)` 之后
# 紧跟 `await this.request({ method: Method.X, path })`，窗口取 15 行足够且不会跨到下个站点）
_WINDOW = 15


def load_prefixes(sdk: pathlib.Path) -> dict[str, str]:
    """从 `src/http-api/prefix.ts` 解析前缀表。

    只认**带限定名**的键（`ClientPrefix.V3` / `VendorPrefix`），因为 `V1`/`V3`
    在 `ClientPrefix`/`MediaPrefix`/`AdminPrefix` 里重名 —— 用裸名会串味。
    解析不出来的前缀一律返回 None，调用方会跳过收紧校验（宁松勿错）。
    """
    table = sdk / "src/http-api/prefix.ts"
    prefixes: dict[str, str] = {}
    if not table.is_file():
        return prefixes
    enum_name: str | None = None
    for line in table.read_text(errors="ignore").splitlines():
        opened = _ENUM_OPEN.match(line)
        if opened:
            enum_name = opened.group(1)
            continue
        if enum_name is not None and line.strip().startswith("}"):
            enum_name = None
            continue
        member = _ENUM_MEMBER.match(line)
        if member:
            prefixes[f"{enum_name}.{member.group(1)}"] = member.group(2)
            continue
        const = _CONST_PREFIX.match(line)
        if const:
            prefixes[const.group(1)] = const.group(2)
    return prefixes


def load_methods(sdk: pathlib.Path) -> dict[str, str]:
    text = (sdk / "src/http-api/method.ts").read_text(errors="ignore")
    return {
        m.group(1): m.group(2)
        for m in re.finditer(r'^\s*(\w+)\s*=\s*"([A-Z]+)"', text, re.M)
    }


def collect_sites(sdk: pathlib.Path) -> list[dict[str, object]]:
    sites: list[dict[str, object]] = []
    for path in sorted(sdk.glob("src/**/*.ts")):
        if "__tests__" in path.parts or "__generated__" in path.parts:
            continue
        lines = path.read_text(errors="ignore").splitlines()
        relative_file = str(path.relative_to(sdk))
        for index, line in enumerate(lines):
            match = _ENCODE_URI.search(line)
            if not match:
                continue
            # 站点窗口：本行起，到下一个 encodeUri 之前（或 _WINDOW 行为止）
            window: list[str] = []
            for later in range(index, min(len(lines), index + _WINDOW)):
                if later > index and _ENCODE_URI.search(lines[later]):
                    break
                window.append(lines[later])
            text = "\n".join(window)

            site: dict[str, object] = {
                "file": relative_file,
                "line": index + 1,
                "relative": match.group(1).split("?")[0],
                "method": None,
                "full": None,
            }
            # 窗口内必须恰好一个 method 才认；多个（如三元表达式）放弃收紧
            names = set(_INLINE_METHOD.findall(text))
            if len(names) == 1:
                site["method"] = methods.get(names.pop())
            # prefix 只在窗口内唯一出现、且没有 `+` 拼接时才认（拼接会漏 MSC 段）
            prefix_tokens = set(_INLINE_PREFIX.findall(text))
            if len(prefix_tokens) == 1 and not _PREFIX_CONCAT.search(text):
                prefix = prefixes.get(prefix_tokens.pop())
                if prefix:
                    site["full"] = prefix + site["relative"]
            sites.append(site)
    return sites


def self_test(backend: list[tuple[str, str]]) -> None:
    """非空转自检：谓词必须能拒绝后端不存在的路径，也必须认得真实路径。"""
    bogus = "/rooms/$roomId/B2_4B_NONEXISTENT_PROBE"
    assert not any(path_match(bogus, path) for _, path in backend), (
        "谓词没有拒绝伪造路径 —— 下面的结论全部是空转的"
    )
    # 相对路径（省略 /_matrix/client/v3 头）必须能锚定到真实 ledger 路径上
    assert path_match(
        "/rooms/$roomId/messages", "/_matrix/client/v3/rooms/{room_id}/messages"
    ), "谓词认不出真实存在的相对路径 —— 谓词写错了"
    # 完整路径（SDK 风格 3）也必须认得
    assert path_match(
        "/_matrix/client/v3/rooms/{room_id}/messages",
        "/_matrix/client/v3/rooms/{room_id}/messages",
    ), "谓词认不出完整路径 —— 谓词写错了"
    # 假阳防线：剩余段数不等必须不匹配，否则 /rooms/{}/xxx 会命中 /rooms/{}/state/{y}
    assert not path_match(
        "/rooms/{}/guest_access",
        "/_matrix/client/v3/rooms/{room_id}/state/{event_type}",
    ), "首段锚定/段数谓词失效 —— 不存在的端点会被误判为已覆盖"
    # 假阳防线：末段字面量不同必须不匹配
    assert not path_match(
        "/rooms/$roomId/messages", "/_matrix/client/v3/rooms/{room_id}/join"
    ), "末段字面量没有参与比对 —— 谓词过宽"
    # allowlist 键的归一化：SDK 变量改名不能让豁免条目失效
    assert (
        canonical_shape("/rooms/$roomId/x")
        == canonical_shape("/rooms/{room_id}/x")
        == "/rooms/{}/x"
    ), "canonical_shape 没有把 $var 与 {var} 同时折成 {} —— 豁免条目会因改名静默失效"


def audit_allowlist_without_sdk(
    backend: list[tuple[str, str]], allowed: dict[str, str]
) -> list[str]:
    """SDK 不在场时也能做的豁免清单体检。

    无法判断「这条豁免还被 SDK 用到吗」（那要读 SDK 源码），但能判断**更致命**的
    一种腐烂：后端已经把端点补上了，豁免却还留着 —— 此后真实的 SDK↔后端不一致
    会被这条豁免静默吃掉。这里用 `path_match` 反向验证：被豁免的形状必须仍然
    **不被 ledger 服务**，否则条目已无意义，必须删掉。
    """
    rotten: list[str] = []
    for key in allowed:
        method, _, shape = key.partition(" ")
        for served_method, served_path in backend:
            if not path_match(shape, served_path):
                continue
            if method not in ("*", served_method):
                continue
            rotten.append(
                f"{key}   ← 后端现在已服务 {served_method} {served_path}，豁免应删除"
            )
            break
    return rotten


def main() -> int:
    sdk_root = os.environ.get("SDK_ROOT")
    sdk = pathlib.Path(sdk_root) if sdk_root else REPO.parent / "matrix-js-sdk"
    strict = os.environ.get("SDK_CONTRACT_STRICT") == "1"

    # ── 不依赖 SDK 的部分：先跑，任何环境下都必须过 ──
    global methods, prefixes
    backend = load_backend()
    self_test(backend)
    allowed = load_allowlist()
    rotten = audit_allowlist_without_sdk(backend, allowed)

    if not (sdk / "src").is_dir():
        message = f"SDK 源码不在 {sdk}（用 SDK_ROOT 指定）"
        if rotten:
            print(f"\n❌ 豁免清单有 {len(rotten)} 条已无意义：", file=sys.stderr)
            for item in rotten:
                print(f"  {item}", file=sys.stderr)
            return 1
        if strict:
            print(f"ERROR: {message}，而 SDK_CONTRACT_STRICT=1", file=sys.stderr)
            return 1
        print(f"SKIPPED: {message}")
        print("  已跑：谓词自检（4 项断言）+ 豁免清单卫生检查")
        print("  **未跑**：SDK 站点逐条比对 —— 本次结论不覆盖 SDK ⊆ ledger")
        return 0

    methods = load_methods(sdk)
    prefixes = load_prefixes(sdk)
    sites = collect_sites(sdk)

    uncovered: list[str] = []
    method_mismatch: list[str] = []
    stale_keys: list[str] = []
    used_keys: set[str] = set()
    tightened = 0

    for site in sites:
        relative = str(site["relative"])
        method = site["method"]
        full = site["full"]

        # 主判据：相对路径能对齐到某条 ledger 路径的某段起点（前缀无关）
        candidates = [(m, p) for m, p in backend if path_match(relative, p)]

        if not candidates:
            shape = full or relative
            key = f"{method or '*'} {canonical_shape(shape)}"
            # allowlist 的 key 允许只写相对形状（前缀不可靠时更稳）
            alt_key = f"{method or '*'} {canonical_shape(relative)}"
            if key in allowed:
                used_keys.add(key)
            elif alt_key in allowed:
                used_keys.add(alt_key)
            else:
                uncovered.append(
                    f"{method or '*':6} {shape}\n         {site['file']}:{site['line']}"
                )
            continue

        # 次判据（收紧）：窗口内解析出唯一 method 时，方法必须真的被后端服务
        if method:
            tightened += 1
            if full:
                scoped = [
                    (m, p) for m, p in candidates if path_match(full, p)
                ] or candidates
            else:
                scoped = candidates
            if method not in {m for m, _ in scoped}:
                served = "/".join(sorted({m for m, _ in scoped}))
                method_mismatch.append(
                    f"{method:6} {relative}   ← 后端只服务 [{served}]\n"
                    f"         {site['file']}:{site['line']}"
                )

    for key in allowed:
        if key not in used_keys:
            stale_keys.append(key)

    print(f"SDK 站点 {len(sites)}（其中 {tightened} 个解出唯一 method，做了方法校验）")
    print(f"后端 ledger 路径 {len(backend)}")
    print(f"allowlist 条目 {len(allowed)}，命中 {len(used_keys)}")

    status = 0
    if uncovered:
        print(f"\n❌ 未被后端覆盖的端点 {len(uncovered)} 条：", file=sys.stderr)
        for item in uncovered:
            print(f"  {item}", file=sys.stderr)
        status = 1
    if method_mismatch:
        print(
            f"\n❌ 方法不匹配 {len(method_mismatch)} 条（路径存在但后端不服务这个 method）：",
            file=sys.stderr,
        )
        for item in method_mismatch:
            print(f"  {item}", file=sys.stderr)
        status = 1
    if stale_keys:
        print(
            f"\n❌ allowlist 中有 {len(stale_keys)} 条已失效（对应端点现在已被覆盖，应删掉）：",
            file=sys.stderr,
        )
        for key in stale_keys:
            print(f"  {key}", file=sys.stderr)
        status = 1
    # 与 stale 重叠的不重复报（stale 的判定更强）
    extra_rot = [item for item in rotten if item.partition("   ←")[0] not in stale_keys]
    if extra_rot:
        print(
            f"\n❌ allowlist 中有 {len(extra_rot)} 条已无意义（后端已服务该形状）：",
            file=sys.stderr,
        )
        for item in extra_rot:
            print(f"  {item}", file=sys.stderr)
        status = 1
    if status == 0:
        print("✅ 所有 SDK 调用的端点都在 ledger 中（且 method 一致）")
    return status


if __name__ == "__main__":
    sys.exit(main())
