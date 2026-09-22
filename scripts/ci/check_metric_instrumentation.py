#!/usr/bin/env python3
"""埋点可达性门禁：`ServerMetrics` 的埋点方法必须在生产代码里真的被调用。

用法:
    python3 scripts/ci/check_metric_instrumentation.py            # 校验
    python3 scripts/ci/check_metric_instrumentation.py --update   # 收紧基线

背景（2026-09-22 实测的 P0 观测面缺陷）：
- `ServerMetrics::record_http_request` / `record_db_query` / `record_federation_request`
  以及 `http_request_started` / `http_request_finished`、`update_pool_metrics`
  都**定义完整、单测齐全**，但在生产代码里**零调用点**
  （只在 `synapse-common/src/server_metrics.rs` 的 `#[cfg(test)]` 段里被调）。
- 后果是静默的：指标注册成功、`/metrics` 每次都把这些序列渲染出来（值为 0）、
  Prometheus 抓取正常、规则语法合法 —— 但 `rate()` 恒为 0，
  `HighHTTPErrorRate` / `HTTPRequestDurationHigh` / `HTTPActiveRequestsGrowing` /
  `DatabaseQueryDurationHigh` / `DatabasePoolUtilizationHigh` / `DatabasePoolExhausted`
  这些告警**永远不会触发**。
  实测：对 `/_matrix/client/versions` 连打 20 次（全 200）后
  `http_requests_total` 仍为 0，而同期 `rate_limit_requests_total` 从 7 涨到 30。

判据：
1. 从 `impl ServerMetrics` 抽出埋点方法（前缀 `record_` / `observe_` / `update_`，
   外加 `http_request_started` / `http_request_finished`）。
2. **同名冲突**方法直接跳过并告警：`record_failure` 这类名字大量存在于
   `CircuitBreaker` / 配额服务 / storage DAO 上，纯文本扫描无法区分接收者类型，
   跳过比给出假结论诚实。
3. 在生产代码（排除定义文件、测试路径、`#[cfg(test)]` 块）里查找 `.<name>(` 调用。
4. **棘轮语义**：未接通且不在基线中 → 失败；基线条目已接通或已消失 → 失败
   （提示收紧基线）。基线只允许单向变短。

⚠️ 已知局限（诚实记录，勿当它是完备证明）：
- 判据是"存在调用点"，**不**校验调用点是否真的在热路径上（例如把埋点放在
  错误分支里也能通过）。这是刻意的取舍：它挡住的是"注册了却永不调用"这一整类
  缺陷，而那正是本仓实际踩过的坑。
- 基线里的每一条都是**已知未接通的埋点**，不是豁免。清理它们才是目标。

实现说明（性能）：内容检索走 `git grep`（索引加速），枚举走 `git ls-files`。
初版用 `Path.rglob` + 逐文件 `read_text`，在本仓根目录（含大体量数据目录、
单文件读取约 0.4s）实测 **400s 都跑不完**，会被 CI 超时杀掉。
`--untracked` 是必需的：新写的埋点文件在被 `git add` 之前也必须参与判定，
否则"刚接通"会被误判成"仍未接通"。
"""

import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Dict, List, Set, Tuple

ROOT = Path(__file__).resolve().parent.parent.parent
SERVER_METRICS_SRC = ROOT / "synapse-common" / "src" / "server_metrics.rs"
BASELINE = ROOT / "scripts" / "ci" / "metric_instrumentation_baseline"

# 埋点方法名前缀（这些方法存在就应当被生产代码调用）。
CHECKED_PREFIXES = ("record_", "observe_", "update_")
# 不按前缀命名、但同样是"必须被调用"的埋点方法。
CHECKED_EXTRA = frozenset({"http_request_started", "http_request_finished"})
# 不是埋点，不参与检查（构造函数与读取器）。
EXCLUDED_NAMES = frozenset({"new", "get_collector", "get_summary"})

# 测试路径豁免（**路径谓词**，不用 glob 字符串）。
TEST_DIR_SEGMENTS = ("tests", "benches", "test_mocks")
TEST_FILE_SUFFIXES = ("_tests.rs", "_test.rs")
TEST_FILE_NAMES = ("tests.rs", "test_utils.rs", "db_tests.rs", "test_exit_hook.rs")

# 基线里各条"已知未接通"的埋点为何仍未接通（打印时附上，避免有人误以为它是豁免）。
BASELINE_REASONS = {
    "record_db_query": (
        "sqlx 的逐语句事件只给 elapsed、不给成功标志，因此计时走 observe_db_query_duration，"
        "失败计数走 From<sqlx::Error>；本方法保留给「调用方自己知道成败」的场景，暂无此类调用点"
    ),
    "record_auth_attempt": "登录/鉴权路径未接（需在 auth 服务层明确成败点）",
    "record_token_validation": "token 校验路径未接",
    "record_cache_operation": "CacheManager 命中/未命中未接",
    "record_csrf_validation": "CSRF 中间件未接",
    "record_security_validation": "安全校验中间件未接",
    "record_replay_attack_blocked": "联邦重放保护拦截点未接",
    "record_room_operation": "room create/join/leave 服务层未接",
    "record_sync_request": "sync 服务层未接（现有 record_sync_latency_metrics 走的是另一条指标）",
    "record_message_send": "消息发送路径未接",
    "record_presence_update": "presence 更新路径未接",
    "record_state_group_resolve": "state group 解析路径未接",
    "record_megolm_vodozemac_pickle_persist": "vodozemac pickle 持久化路径未接",
    "record_megolm_dual_write_promotion": "megolm 双写迁移路径未接",
    "record_megolm_lazy_migration_batch": "megolm 惰性迁移批处理未接",
    "update_pool_metrics": (
        "连接池状态回填未接 ⇒ pool_utilization / db_connections_active / db_connections_idle "
        "/ pool_health_status 恒为 0，DatabasePoolUtilizationHigh 与 DatabasePoolExhausted 无法触发"
    ),
}


class GateError(RuntimeError):
    """扫描基础设施不可用时抛出（不得被吞成"通过"）。"""


def is_test_path(rel_path: str) -> bool:
    """测试 / 压测 / mock 路径判定。"""
    normalized = rel_path.replace(os.sep, "/")
    parts = normalized.split("/")
    if any(segment in TEST_DIR_SEGMENTS for segment in parts):
        return True
    name = parts[-1]
    if name in TEST_FILE_NAMES:
        return True
    return any(name.endswith(suffix) for suffix in TEST_FILE_SUFFIXES)


def run_git(args: List[str]) -> str:
    """执行 git 子命令；失败即抛 GateError（宁 fail-loud 也不静默通过）。"""
    try:
        completed = subprocess.run(
            ["git", *args],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
    except FileNotFoundError as exc:  # git 未安装
        raise GateError("git is not available; this gate needs an index-backed scan") from exc
    if completed.returncode not in (0, 1):  # grep 无匹配返回 1，属正常
        raise GateError(f"git {' '.join(args)} failed rc={completed.returncode}: {completed.stderr.strip()[:300]}")
    return completed.stdout


def list_rust_files() -> List[str]:
    """列出参与判定的生产 .rs 文件（tracked + untracked，排除测试路径与定义文件）。"""
    out = run_git(["ls-files", "--cached", "--others", "--exclude-standard", "-z", "--", "*.rs"])
    files = [p for p in out.split("\0") if p]
    kept = []
    for rel in files:
        rel = rel.replace(os.sep, "/")
        if (ROOT / rel).resolve() == SERVER_METRICS_SRC.resolve():
            continue
        if is_test_path(rel):
            continue
        kept.append(rel)
    return kept


def grep(pattern: str) -> List[Tuple[str, int, str]]:
    """在生产 .rs 文件里检索正则，返回 (相对路径, 行号, 行内容)。

    `--untracked` 保证尚未 `git add` 的新文件也参与判定。
    """
    out = run_git(
        [
            "grep",
            "--untracked",
            "-n",
            "-I",
            "-E",
            "--no-color",
            "-e",
            pattern,
            "--",
            "*.rs",
        ]
    )
    hits: List[Tuple[str, int, str]] = []
    for line in out.splitlines():
        # 形如 "path/to/file.rs:123:content"
        parts = line.split(":", 2)
        if len(parts) < 3:
            continue
        rel, line_no, text = parts
        try:
            number = int(line_no)
        except ValueError:
            continue
        hits.append((rel.replace(os.sep, "/"), number, text))
    return hits


def extract_instrumentation_methods() -> List[str]:
    """从 `impl ServerMetrics { … }` 中抽出埋点方法名。"""
    if not SERVER_METRICS_SRC.exists():
        raise GateError(f"missing {SERVER_METRICS_SRC}")

    lines = SERVER_METRICS_SRC.read_text(encoding="utf-8", errors="ignore").splitlines()
    impl_start = None
    for index, line in enumerate(lines):
        if re.match(r"^impl ServerMetrics \{", line):
            impl_start = index
            break
    if impl_start is None:
        raise GateError("could not locate `impl ServerMetrics {` — the scan anchor moved")

    names: List[str] = []
    fn_pattern = re.compile(r"^\s+pub fn ([a-z_][a-z0-9_]*)\s*\(")
    for line in lines[impl_start + 1 :]:
        if re.match(r"^\}", line):
            break
        match = fn_pattern.match(line)
        if match:
            names.append(match.group(1))

    checked = [n for n in names if n not in EXCLUDED_NAMES and (n.startswith(CHECKED_PREFIXES) or n in CHECKED_EXTRA)]
    if not checked:
        raise GateError("no instrumentation methods found — the anchor or prefixes are stale")
    return checked


def find_ambiguous_names(names: Set[str]) -> Set[str]:
    """找出在同仓其它类型上同名的函数，纯文本扫描无法安全判定这些名字。"""
    pattern = r"\bfn\s+(" + "|".join(sorted(map(re.escape, names))) + r")\s*[(<]"
    ambiguity: Dict[str, Set[str]] = {name: set() for name in names}
    for rel, _line_no, text in grep(pattern):
        if is_test_path(rel):
            continue
        for name in names:
            if re.search(r"\bfn\s+" + re.escape(name) + r"\s*[(<]", text):
                ambiguity[name].add(rel)
    return {name for name, files in ambiguity.items() if files}


def cfg_test_lines(rel_path: str) -> Set[int]:
    """返回该文件中位于 `#[cfg(test)]` 块内的行号集合（花括号配平）。

    只对**有候选命中**的少数文件调用，避免全仓逐文件读取。
    """
    try:
        lines = (ROOT / rel_path).read_text(encoding="utf-8", errors="ignore").splitlines()
    except OSError:
        return set()
    cfg_test_pattern = re.compile(r"#\[cfg\s*\(\s*test\s*\)\s*\]")
    inside: Set[int] = set()
    depth = 0
    block_start_depth = None
    pending = False
    in_block_comment = False
    for line_num, line in enumerate(lines, 1):
        stripped = line.strip()
        if in_block_comment:
            if "*/" in line:
                in_block_comment = False
            if block_start_depth is not None:
                inside.add(line_num)
            continue
        if stripped.startswith("//"):
            if block_start_depth is not None:
                inside.add(line_num)
            continue
        if "/*" in line:
            in_block_comment = "*/" not in line.split("/*", 1)[1]
            if block_start_depth is not None:
                inside.add(line_num)
            continue
        if cfg_test_pattern.search(line):
            pending = True
        elif pending and stripped and not stripped.startswith("#") and not stripped.startswith("mod "):
            pending = False
        opens = line.count("{")
        closes = line.count("}")
        if pending and opens:
            block_start_depth = depth
            pending = False
        depth += opens - closes
        if block_start_depth is not None:
            if depth <= block_start_depth:
                block_start_depth = None
            else:
                inside.add(line_num)
    return inside


def find_production_call_sites(names: Set[str]) -> Tuple[Dict[str, List[str]], int]:
    """在生产代码里查找 `.<name>(` 调用点。

    Returns:
        (call_sites, scanned)：call_sites 映射方法名到 "相对路径:行号" 列表；
        scanned 是参与判定的 .rs 文件数（供空扫描断言）。
    """
    production_files = list_rust_files()
    scanned = len(production_files)
    if scanned == 0:
        raise GateError("git ls-files returned 0 .rs files — the tree or the invocation is wrong")

    pattern = r"\.\s*(" + "|".join(sorted(map(re.escape, names))) + r")\s*\("
    candidates: Dict[str, List[Tuple[str, int]]] = {name: [] for name in names}
    for rel, line_no, text in grep(pattern):
        if is_test_path(rel):
            continue
        for name in names:
            if re.search(r"\.\s*" + re.escape(name) + r"\s*\(", text):
                candidates[name].append((rel, line_no))

    # 只读取有候选命中的文件，判断该行是否落在 `#[cfg(test)]` 块内。
    needed = {rel for hits in candidates.values() for rel, _ in hits}
    test_line_cache = {rel: cfg_test_lines(rel) for rel in sorted(needed)}

    call_sites: Dict[str, List[str]] = {name: [] for name in names}
    for name, hits in candidates.items():
        for rel, line_no in hits:
            if line_no in test_line_cache.get(rel, set()):
                continue
            call_sites[name].append(f"{rel}:{line_no}")
    return call_sites, scanned


def read_baseline() -> Set[str]:
    if not BASELINE.exists():
        return set()
    entries = set()
    for raw in BASELINE.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        entries.add(line.split()[0])
    return entries


def write_baseline(unreachable: List[str], ambiguous: Set[str]) -> None:
    body = [
        "# 埋点可达性 ratchet 基线 —— 已知**尚未接通**的 ServerMetrics 埋点方法。",
        "# 由 scripts/ci/check_metric_instrumentation.py --update 生成/收紧。",
        "# 每一条都是待还的债，不是豁免：接通后请重跑 --update 让它从列表里消失。",
        "# 语法：每行一个方法名，`#` 起注释。",
        "",
    ]
    body.extend(sorted(unreachable))
    body.append("")
    body.append("# 因同名冲突而跳过检查（纯文本扫描无法区分接收者类型）的方法：")
    for name in sorted(ambiguous):
        body.append(f"#   - {name}")
    BASELINE.write_text("\n".join(body) + "\n", encoding="utf-8")
    print(f"metric_instrumentation: baseline rewritten with {len(unreachable)} entries -> {BASELINE}")


def main() -> int:
    update = "--update" in sys.argv

    print("-" * 35)
    print("埋点可达性门禁：ServerMetrics 埋点是否在生产路径上被调用")
    print(f"扫描目录：{ROOT}")
    print("-" * 35 + "\n")

    try:
        methods = extract_instrumentation_methods()
        ambiguous = find_ambiguous_names(set(methods))
        checkable = [name for name in methods if name not in ambiguous]
        call_sites, scanned = find_production_call_sites(set(checkable))
    except GateError as exc:
        print(f"::error::{exc}", file=sys.stderr)
        return 2

    if ambiguous:
        print(f"跳过 {len(ambiguous)} 个同名冲突方法（无法安全判定）：{', '.join(sorted(ambiguous))}\n")

    reachable = sorted(name for name in checkable if call_sites[name])
    unreachable = sorted(name for name in checkable if not call_sites[name])

    print(f"埋点方法：{len(methods)} 个（可判定 {len(checkable)} 个）")
    print(f"已接通：{len(reachable)} 个")
    for name in reachable:
        first = call_sites[name][0]
        print(f"  + {name}  <- {first}  (共 {len(call_sites[name])} 处)")
    print(f"未接通：{len(unreachable)} 个")
    for name in unreachable:
        reason = BASELINE_REASONS.get(name)
        print(f"  - {name}" + (f"  [{reason}]" if reason else ""))
    print()

    if update:
        write_baseline(unreachable, ambiguous)
        return 0

    baseline = read_baseline()

    new_gaps = [name for name in unreachable if name not in baseline]
    stale = sorted(name for name in baseline if name not in set(unreachable))

    failed = False

    if new_gaps:
        failed = True
        print("FAIL 新增未接通埋点（指标会注册但永不产生数据 ⇒ 依赖它的告警永不触发）")
        print("-" * 35)
        for name in new_gaps:
            print(f"{name}")
            print("  建议：在真实路径上调用它（并把调用点写进本方法的文档注释），")
            print("        或若它确实是内部/测试专用，就从 ServerMetrics 移除。")
            print()

    if stale:
        failed = True
        print("FAIL 基线已过期（对应埋点已接通或已删除，基线必须收紧）")
        print("-" * 35)
        for name in stale:
            print(f"{name}  —— 已不在未接通集合中")
        print()
        print("  执行：python3 scripts/ci/check_metric_instrumentation.py --update")
        print()

    print("-" * 35)
    print(f"参与判定文件：{scanned} 个 .rs    （tracked + untracked，已排除测试路径）")
    print(f"已接通：{len(reachable)}    未接通：{len(unreachable)}    基线：{len(baseline)}")
    print("-" * 35)

    if failed:
        return 1

    print("通过（未接通集合与基线一致，无新增缺陷）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
