#!/usr/bin/env python3
"""check_route_surface_reconciliation.py — BE-1 route-surface reconciliation gate.

后端路由面只有一个机器权威（`synapse_ledger_export` 导出的 ledger）；其余产物都是它在
某个 `(lane, profile)` 坐标上的视图（坐标定义见 `docs/openapi/README.md`「权威口径与
(lane, profile) 坐标轴」）。本门禁把「同一路由面多口径」从一句口头约定变成一条**能失败**
的 CI 门禁（AGENTS.md 铁律 8）。

断言（逐条打印；任一失败即 exit 1）：
  R1  route-table.json 的 (lane, profile, profile_flags, ledger_schema_version)
      与它声称的坐标 (golden, default) 的 fixture ledger 一致；
  R2  route-table.json 的 total_routes 与路由集合
      （method, path, registered_by, path_params, query_params）与
      `ledger_export/default.json` 逐条一致；
  R3  `ROUTE_CONTRACT.md` 总览计数 == `ledger_export_sdk/all.json` 的 entry_count
      （= sdk lane × all profile = 权威计数；数值以该 fixture 为准，不在文档里写死）；
  R4  每个 lane 的 profile 单调性（按 `(method, path)`）：default ⊆ worker ⊆ all；
  R5  6 份 fixture 的 schema_version 一致；`saml_enabled` 仅在 sdk lane × all
      profile 为 true（它只在 `saml`/`all` 两个 profile 由
      `cfg!(feature="saml-sso")` 写出），其余全为 false；
  R6  `route-table-sdk.json` 的坐标是 (sdk, all)，且其路由集合与
      `ledger_export_sdk/all.json` 逐条一致（D-15：SDK 消费的是 sdk lane，
      只给 golden × default 的视图会把整族 feature-gated 路由判成未解析）。

用法:
  python3 scripts/contract/check_route_surface_reconciliation.py
  python3 scripts/contract/check_route_surface_reconciliation.py --mutation-check

`--mutation-check` 注入一个坐标缺陷（把 route-table 的 profile 改成 `worker`）并要求
R1 变红 —— 一个不会失败的门禁不是门禁。
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

ROUTE_TABLE = ROOT / "docs" / "openapi" / "route-table.json"
# D-15: the superset view. `route-table.json` is the golden lane (default
# features), which by construction cannot contain feature-gated families such as
# `/voice/*`; SDK consumers therefore need the sdk lane artifact as well.
ROUTE_TABLE_SDK = ROOT / "docs" / "openapi" / "route-table-sdk.json"
CONTRACT = ROOT / "docs" / "synapse-rust" / "ROUTE_CONTRACT.md"

LANES = ("golden", "sdk")
PROFILES = ("default", "worker", "all")
# lane → fixture 目录名（golden 是默认特性闭包；sdk 额外开 all-extensions）。
LANE_DIR = {"golden": "ledger_export", "sdk": "ledger_export_sdk"}

# route-table.json 的权威坐标（CI 从 fresh default-feature 导出生成）。
RT_LANE = "golden"
RT_PROFILE = "default"
# route-table-sdk.json 的权威坐标（最大完整面，SDK 实际摄入的 lane）。
RT_SDK_LANE = "sdk"
RT_SDK_PROFILE = "all"


def fixture(lane: str, profile: str) -> dict:
    path = ROOT / "tests" / "unit" / "fixtures" / LANE_DIR[lane] / f"{profile}.json"
    return json.loads(path.read_text(encoding="utf-8"))


def _load(path: Path) -> dict | None:
    """Parse a committed artifact, or `None` when it is absent.

    Absence is a violation, not a crash: `reconcile_sdk` turns it into an R6
    line carrying the exact regeneration command.
    """
    if not path.exists():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def entry_key(entry: dict) -> tuple:
    """The reconcilable identity of a route entry (shared by ledger and route-table)."""
    return (
        entry["method"],
        entry["path"],
        entry["registered_by"],
        tuple(entry.get("path_params", [])),
        tuple(entry.get("query_params", [])),
    )


def entry_path_key(entry: dict) -> tuple:
    """(method, path) identity — the dimension on which profile rank is monotone.

    A higher-rank colliding variant *replaces* a lower-rank one at the same
    (method, path) rather than adding a new path: in the sdk lane the two
    `/.well-known/{jwks.json,openid-configuration}` routes are served by
    `oidc_fallback` at lower profiles and by `oidc` at `all` (see
    synapse-web/src/routes/derived_routes.rs). So `default ⊆ worker ⊆ all` must be
    asserted on (method, path); the full entry tuple would falsely flag the twin
    swap as a violation.
    """
    return (entry["method"], entry["path"])


def contract_route_total(text: str) -> int:
    """Parse `<N>` from the `- 注册路由条目…：**<N>**` line of ROUTE_CONTRACT.md."""
    for line in text.splitlines():
        if "注册路由条目" in line:
            match = re.search(r"\*\*([0-9][0-9,]*)\*\*", line)
            if match is None:
                break
            return int(match.group(1).replace(",", ""))
    raise SystemExit(
        "ROUTE_CONTRACT.md: 找不到「注册路由条目 … **<N>**」行，无法读取权威计数"
    )


def reconcile(route_table: dict, route_table_sdk: dict | None) -> list[str]:
    """Return the list of violations (empty == reconciled)."""
    found: list[str] = []
    golden_default = fixture(RT_LANE, RT_PROFILE)

    # R1 — the artifact's declared coordinate must match the fixture it claims.
    if route_table.get("lane") != RT_LANE:
        found.append(
            f"R1 route-table.lane = {route_table.get('lane')!r}，应为 {RT_LANE!r}"
        )
    if route_table.get("profile") != RT_PROFILE:
        found.append(
            f"R1 route-table.profile = {route_table.get('profile')!r}，应为 {RT_PROFILE!r}"
        )
    if route_table.get("profile_flags") != golden_default.get("profile_flags"):
        found.append(
            "R1 route-table.profile_flags "
            f"{route_table.get('profile_flags')!r} != {LANE_DIR[RT_LANE]}/{RT_PROFILE} "
            f"{golden_default.get('profile_flags')!r}"
        )
    if route_table.get("ledger_schema_version") != golden_default.get("schema_version"):
        found.append(
            f"R1 route-table.ledger_schema_version {route_table.get('ledger_schema_version')!r} "
            f"!= {LANE_DIR[RT_LANE]}/{RT_PROFILE} schema_version "
            f"{golden_default.get('schema_version')!r}"
        )

    # R2 — the artifact's routes must equal its declared fixture, entry for entry.
    if route_table.get("total_routes") != golden_default["entry_count"]:
        found.append(
            f"R2 route-table.total_routes {route_table.get('total_routes')} "
            f"!= {LANE_DIR[RT_LANE]}/{RT_PROFILE} entry_count {golden_default['entry_count']}"
        )
    rt_keys = {entry_key(e) for e in route_table["routes"]}
    lg_keys = {entry_key(e) for e in golden_default["entries"]}
    if rt_keys != lg_keys:
        only_rt = sorted(rt_keys - lg_keys)
        only_lg = sorted(lg_keys - rt_keys)
        found.append(
            f"R2 route-table 与 {LANE_DIR[RT_LANE]}/{RT_PROFILE} 的路由集合不一致："
            f"route-table 独有 {len(only_rt)}，fixture 独有 {len(only_lg)}"
        )
        for key in only_rt[:5]:
            found.append(f"    route-table 独有: {key}")
        for key in only_lg[:5]:
            found.append(f"    fixture 独有: {key}")

    # R3 — ROUTE_CONTRACT.md (union/all-feature lane) must equal sdk lane × all.
    contract_total = contract_route_total(CONTRACT.read_text(encoding="utf-8"))
    sdk_all = fixture("sdk", "all")["entry_count"]
    if contract_total != sdk_all:
        found.append(
            f"R3 ROUTE_CONTRACT.md 计数 {contract_total} != sdk lane all profile "
            f"entry_count {sdk_all}（权威口径）"
        )

    # R4 — per-lane monotonicity on (method, path): default ⊆ worker ⊆ all.
    for lane in LANES:
        keys = {
            profile: {entry_path_key(e) for e in fixture(lane, profile)["entries"]}
            for profile in PROFILES
        }
        for low, high in zip(PROFILES, PROFILES[1:]):
            missing = keys[low] - keys[high]
            if missing:
                found.append(
                    f"R4 {lane} lane: {low} ⊄ {high}（按 (method,path) 计，"
                    f"{low} 独有 {len(missing)} 条）"
                )
                for key in sorted(missing)[:5]:
                    found.append(f"    {low} 独有: {key}")

    # R5 — schema_version uniform across fixtures; saml_enabled reflects the
    # compile-time `saml-sso` feature, which `profile_for_name` only writes into
    # the `saml`/`all` profiles. Hence the golden lane is false everywhere and
    # only sdk × all is true. `derive_lane` (gen_route_table.py) relies on
    # "golden ⇒ saml_enabled false", so that direction is pinned here.
    schema_versions = {
        lane: fixture(lane, PROFILES[0])["schema_version"] for lane in LANES
    }
    if len(set(schema_versions.values())) != 1:
        found.append(f"R5 fixture schema_version 不一致：{schema_versions}")
    for lane in LANES:
        for profile in PROFILES:
            saml = fixture(lane, profile)["profile_flags"]["saml_enabled"]
            expected = lane == "sdk" and profile == "all"
            if saml != expected:
                found.append(
                    f"R5 {lane} lane × {profile} profile: saml_enabled={saml}，应 {expected}"
                )

    return found


def reconcile_sdk(route_table_sdk: dict | None) -> list[str]:
    """R6 — the sdk-lane superset artifact must equal `ledger_export_sdk/all.json`.

    D-15: `route-table.json` is deliberately the golden lane × default profile
    (a default-feature export cannot contain `#[cfg(feature = "…")]` routes), so
    a consumer treating it as the universe marks every feature-gated family
    (`/voice/*`, SAML/CAS callbacks, …) as "unresolved" instead of "served".
    `route-table-sdk.json` closes that gap; this assertion keeps it honest.
    """
    found: list[str] = []
    sdk_all = fixture(RT_SDK_LANE, RT_SDK_PROFILE)
    if route_table_sdk is None:
        found.append(
            f"R6 {ROUTE_TABLE_SDK.name} 缺失 —— 生成：python3 "
            f"scripts/api_test/gen_route_table.py --ledger "
            f"tests/unit/fixtures/{LANE_DIR[RT_SDK_LANE]}/{RT_SDK_PROFILE}.json "
            f"--output {ROUTE_TABLE_SDK.relative_to(ROOT)}"
        )
        return found

    if route_table_sdk.get("lane") != RT_SDK_LANE:
        found.append(
            f"R6 {ROUTE_TABLE_SDK.name}.lane = {route_table_sdk.get('lane')!r}，"
            f"应为 {RT_SDK_LANE!r}"
        )
    if route_table_sdk.get("profile") != RT_SDK_PROFILE:
        found.append(
            f"R6 {ROUTE_TABLE_SDK.name}.profile = {route_table_sdk.get('profile')!r}，"
            f"应为 {RT_SDK_PROFILE!r}"
        )
    if route_table_sdk.get("profile_flags") != sdk_all.get("profile_flags"):
        found.append(
            f"R6 {ROUTE_TABLE_SDK.name}.profile_flags "
            f"{route_table_sdk.get('profile_flags')!r} != "
            f"{LANE_DIR[RT_SDK_LANE]}/{RT_SDK_PROFILE} {sdk_all.get('profile_flags')!r}"
        )
    if route_table_sdk.get("ledger_schema_version") != sdk_all.get("schema_version"):
        found.append(
            f"R6 {ROUTE_TABLE_SDK.name}.ledger_schema_version "
            f"{route_table_sdk.get('ledger_schema_version')!r} != "
            f"{LANE_DIR[RT_SDK_LANE]}/{RT_SDK_PROFILE} schema_version "
            f"{sdk_all.get('schema_version')!r}"
        )
    if route_table_sdk.get("total_routes") != sdk_all["entry_count"]:
        found.append(
            f"R6 {ROUTE_TABLE_SDK.name}.total_routes "
            f"{route_table_sdk.get('total_routes')} != "
            f"{LANE_DIR[RT_SDK_LANE]}/{RT_SDK_PROFILE} entry_count "
            f"{sdk_all['entry_count']}"
        )
    rt_keys = {entry_key(e) for e in route_table_sdk["routes"]}
    lg_keys = {entry_key(e) for e in sdk_all["entries"]}
    if rt_keys != lg_keys:
        only_rt = sorted(rt_keys - lg_keys)
        only_lg = sorted(lg_keys - rt_keys)
        found.append(
            f"R6 {ROUTE_TABLE_SDK.name} 与 "
            f"{LANE_DIR[RT_SDK_LANE]}/{RT_SDK_PROFILE} 的路由集合不一致："
            f"route-table-sdk 独有 {len(only_rt)}，fixture 独有 {len(only_lg)}"
        )
        for key in only_rt[:5]:
            found.append(f"    route-table-sdk 独有: {key}")
        for key in only_lg[:5]:
            found.append(f"    fixture 独有: {key}")
    return found


def mutation_check() -> int:
    """A gate that cannot fail is not a gate.

    Inject three independent defects — one at the coordinate level (R1), one at
    the golden entry-set level (R2), and one at the sdk superset entry-set level
    (R6) — and require each to turn the reconciler red.
    """
    base = json.loads(ROUTE_TABLE.read_text(encoding="utf-8"))
    base_sdk = _load(ROUTE_TABLE_SDK)

    clean = reconcile(base, base_sdk) + reconcile_sdk(base_sdk)
    if clean:
        print("❌ mutation-check 预备失败：未注入缺陷时本已不通过：", file=sys.stderr)
        for line in clean:
            print(f"   {line}", file=sys.stderr)
        return 1

    def detects(mutated: dict, mutated_sdk: dict | None = None) -> bool:
        return bool(reconcile(mutated, mutated_sdk) + reconcile_sdk(mutated_sdk))

    # Mutation 1 — coordinate: the artifact lies about its profile.
    coord = json.loads(ROUTE_TABLE.read_text(encoding="utf-8"))
    coord["profile"] = "worker"  # routes are golden/default, not worker
    if not detects(coord, base_sdk):
        print(
            "❌ mutation-check 失败：注入的坐标缺陷（profile→worker）未被检出 —— "
            "R1 无法失败，视为无效",
            file=sys.stderr,
        )
        return 1

    # Mutation 2 — entry set: a phantom route the declared fixture does not have.
    phantom = json.loads(ROUTE_TABLE.read_text(encoding="utf-8"))
    phantom["routes"].append(
        {
            "method": "GET",
            "path": "/__mutation_probe__",
            "registered_by": "mutation_probe",
            "path_params": [],
            "query_params": [],
            "auth": None,
        }
    )
    if not detects(phantom, base_sdk):
        print(
            "❌ mutation-check 失败：注入的幻影路由（/__mutation_probe__）未被检出 —— "
            "R2 无法失败，视为无效",
            file=sys.stderr,
        )
        return 1

    # Mutation 3 — the sdk superset (D-15) loses a route: R6 must notice, or the
    # very gap D-15 describes could silently reappear.
    if not base_sdk or not base_sdk.get("routes"):
        print(
            "❌ mutation-check 失败：route-table-sdk.json 缺失或为空，无法证明 R6 可失败",
            file=sys.stderr,
        )
        return 1
    sdk_missing = json.loads(json.dumps(base_sdk))
    dropped = sdk_missing["routes"].pop(0)
    if not detects(base, sdk_missing):
        print(
            f"❌ mutation-check 失败：route-table-sdk.json 丢掉 {dropped['method']} "
            f"{dropped['path']} 后未被检出 —— R6 无法失败，视为无效",
            file=sys.stderr,
        )
        return 1

    print(
        "✅ mutation-check：坐标缺陷、golden 幻影路由与 sdk 缺路由均被检出，机制可证伪"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="BE-1 route-surface reconciliation gate"
    )
    parser.add_argument(
        "--mutation-check",
        action="store_true",
        help="注入坐标缺陷并要求门禁变红（证明门禁能失败），不检验真实产物",
    )
    args = parser.parse_args()

    if args.mutation_check:
        return mutation_check()

    route_table = json.loads(ROUTE_TABLE.read_text(encoding="utf-8"))
    route_table_sdk = _load(ROUTE_TABLE_SDK)
    violations = reconcile(route_table, route_table_sdk) + reconcile_sdk(
        route_table_sdk
    )
    if violations:
        print("❌ 路由面口径对账失败：", file=sys.stderr)
        for line in violations:
            print(f"   {line}", file=sys.stderr)
        print(
            "   提示：route-table.json 由 golden lane × default profile 的 fresh 导出生成，"
            "route-table-sdk.json 由 sdk lane × all profile 的导出生成；"
            "重生成见 docs/openapi/README.md。",
            file=sys.stderr,
        )
        return 1

    contract_total = contract_route_total(CONTRACT.read_text(encoding="utf-8"))
    print(
        "✅ 路由面口径对账通过：route-table.json = "
        f"{LANE_DIR[RT_LANE]}/{RT_PROFILE}（{route_table['total_routes']} 条）；"
        f"route-table-sdk.json = {LANE_DIR[RT_SDK_LANE]}/{RT_SDK_PROFILE}"
        f"（{route_table_sdk['total_routes']} 条）；"
        f"权威计数 (sdk × all) = ROUTE_CONTRACT.md = {contract_total} 条"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
