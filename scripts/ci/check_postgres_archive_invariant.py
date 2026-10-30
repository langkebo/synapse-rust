#!/usr/bin/env python3
"""RD-16：WAL 归档的三个开关必须始终自洽。

背景（2026-10-30 实测事故，两件事同时成立）
------------------------------------------
`docker/config/postgres.conf` 曾经开着 `archive_mode = on`，但：

1. **只写不删**：`archive_command` 往 `./wal_archive` 里 `cp`，而全仓没有
   `archive_cleanup_command`、也没有任何 cron/systemd 清理。实测该目录到 **59 GB /
   3760 个 16 MB 段**（10-15 → 10-24，≈6.5 GB/天；`archive_timeout = 900s` 让低写入
   时段也每 15 分钟切一段）。该目录与 `./backups`、Docker 数据卷同盘 ⇒ 写满即
   "DB 无法写 WAL（PANIC）" 与 "备份写不进去" 同时发生。
2. **归档永久死锁**：`archive_command` 用 `test ! -f DEST/%f && cp …` 做守卫。一旦目标
   目录已有同名段（实测 `000000010000000E000000E8`），守卫恒假 ⇒ 命令恒返回 1 ⇒
   PG 无限重试同一段、**WAL 永不回收**。实测 `pg_stat_archiver.failed_count=26967`、
   `archive_status` 里 994 个 `.ready`、`pg_wal` 涨到 15.5 GB / 996 段。

处置（495110202）：关掉归档（本项目备份走 `pg_dump -Fc`，归档**没有消费方**），
随之一并删除两条归档告警与 4 个 promtool 用例（关闭后计数冻结，留着会永久 critical）、
卸掉 compose 的 `./wal_archive` 挂载、Grafana 面板标注"已关闭"。

**问题在于**：那次处置的所有约束只写在注释里。任何人把 `archive_mode` 改回 `on`
而漏掉清理 / 幂等 / 告警，就会把上面两起事故原样重演一遍 —— 而注释不会红。
本脚本把那 5 条"重启用清单"变成可失败的判据。

判据（对称：on 与 off 各有一组）
--------------------------------
**通用**
* A1 `archive_mode` 必须存在且取值合法（`on` / `off`）。缺失即红（fail-closed）。

**归档关闭时（当前状态）** —— 防止"关了但留下会永久红的告警"与"死目录复活"
* A2 不得存在生效的 `PostgresArchiveFailing` / `PostgresArchiveStalled` 告警规则；
* A3 不得存在引用这两个告警名的 promtool 用例；
* A4 `docker/deploy/docker-compose.yml` 不得再挂 `wal_archive`（否则目录被重建，
  且让后来的人以为归档在工作）。

**归档开启时** —— 每一条对应一次实际故障
* A5 `archive_command` 必须存在（生效、未注释）；
* A6 `archive_cleanup_command` 必须存在 —— 缺它就是"只写不删"（事故 1）；
* A7 `archive_command` 必须幂等：出现 `test ! -f` 时必须同时出现 `exit 0` 或 `|| true`
  （"目标已存在"是成功，不是失败）—— 缺它就是"归档永久死锁"（事故 2）；
* A8 两条告警规则必须存在（否则归档悄悄挂掉没人知道）；
* A9 promtool 必须为这两条告警各留用例（阈值/表达式一改就有人管）；
* A10 compose 必须把归档目录挂到 **PGDATA 之外**（挂进数据卷等于"归档与数据同损"）。

注释里的举例不算（本仓多处注释引用了旧命令与旧告警名）。`.conf` 的注释判定要
跳过单引号内内容（`archive_command = '…'` 的值里可能有 `#`）；YAML 沿用
`tests/unit/workflow_pipefail_tests.rs` 的判据（`#` 须在行首或空白之后）。

用法
----
    python3 scripts/ci/check_postgres_archive_invariant.py             # 门禁
    python3 scripts/ci/check_postgres_archive_invariant.py --self-test # 正/负样本自检
"""

from __future__ import annotations

import re
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CONF = "docker/config/postgres.conf"
RULES = "docker/deploy/prometheus/alerting-rules.yml"
PROMTEST = "docker/deploy/prometheus/promtool/alerting-rules.test.yml"
DEPLOY_COMPOSE = "docker/deploy/docker-compose.yml"

ARCHIVE_ALERTS = ("PostgresArchiveFailing", "PostgresArchiveStalled")

CONF_ASSIGN_RE = re.compile(
    r"^\s*(?P<key>archive_mode|archive_command|archive_cleanup_command)\s*=\s*(?P<value>.*)$"
)
YAML_ALERT_RE = re.compile(r"^\s*-?\s*alert:\s*(?P<name>[A-Za-z0-9_]+)\s*$")
# promtool 的用例里 `alertname:` 有两种写法：映射键（仓库现状）与列表项
# （`- alertname: X`，见 promtool 文档示例），两种都必须认。
PROMTEST_ALERT_RE = re.compile(r"^\s*-?\s*alertname:\s*(?P<name>[A-Za-z0-9_]+)\s*$")


def strip_yaml_comment(line: str) -> str:
    """去掉行尾 `# …`（`#` 需在行首或空白之后）。"""
    for index, char in enumerate(line):
        if char == "#" and (index == 0 or line[index - 1].isspace()):
            return line[:index]
    return line


def strip_conf_comment(line: str) -> str:
    """postgresql.conf 注释：`#` 出现在**单引号之外**且位于行首/空白之后。"""
    in_quote = False
    for index, char in enumerate(line):
        if char == "'":
            in_quote = not in_quote
            continue
        if char == "#" and not in_quote and (index == 0 or line[index - 1].isspace()):
            return line[:index]
    return line


def read_conf(root: Path) -> dict[str, tuple[int, str]]:
    """返回 {key: (行号, 值)}，只含**生效**（未注释）的赋值；同一 key 取最后一次。"""
    path = root / CONF
    if not path.exists():
        return {}
    found: dict[str, tuple[int, str]] = {}
    for lineno, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = strip_conf_comment(raw)
        match = CONF_ASSIGN_RE.match(line)
        if match:
            found[match.group("key")] = (lineno, match.group("value").strip())
    return found


def effective_lines(root: Path, rel: str, stripper) -> list[tuple[int, str]]:
    path = root / rel
    if not path.exists():
        return []
    out: list[tuple[int, str]] = []
    for lineno, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        code = stripper(raw)
        if code.strip():
            out.append((lineno, code))
    return out


def alert_names(root: Path, rel: str, pattern: re.Pattern[str]) -> dict[str, int]:
    names: dict[str, int] = {}
    for lineno, code in effective_lines(root, rel, strip_yaml_comment):
        match = pattern.match(code)
        if match:
            names[match.group("name")] = lineno
    return names


def compose_mentions_wal_archive(root: Path) -> int | None:
    for lineno, code in effective_lines(root, DEPLOY_COMPOSE, strip_yaml_comment):
        if "wal_archive" in code:
            return lineno
    return None


def check(root: Path) -> list[str]:
    problems: list[str] = []
    conf = read_conf(root)

    if "archive_mode" not in conf:
        return [
            (
                f"A1 {CONF} 里找不到生效的 `archive_mode`（缺失即红：默认值随大版本变化，"
                "不能让归档状态变成隐式的）"
            )
        ]

    mode_lineno, raw_mode = conf["archive_mode"]
    mode = raw_mode.split()[0].lower() if raw_mode else ""
    if mode not in ("on", "off", "true", "false"):
        return [
            f"A1 {CONF}:{mode_lineno} `archive_mode` 取值非法：{raw_mode!r}（应为 on/off）"
        ]
    enabled = mode in ("on", "true")

    alerts = alert_names(root, RULES, YAML_ALERT_RE)
    prom_cases = alert_names(root, PROMTEST, PROMTEST_ALERT_RE)
    mount_line = compose_mentions_wal_archive(root)

    if not enabled:
        for name in ARCHIVE_ALERTS:
            if name in alerts:
                problems.append(
                    f"A2 {RULES}:{alerts[name]} 归档已关闭却仍有生效的 `{name}` 告警 —— "
                    "关闭后归档计数冻结，该告警会永久 critical/warning，最终被静音（= 白装）"
                )
            if name in prom_cases:
                problems.append(
                    f"A3 {PROMTEST}:{prom_cases[name]} 归档已关闭却仍有 `{name}` 的 promtool 用例 —— "
                    "规则不存在时该用例会以 'no alert found' 失败，或掩盖规则被误删"
                )
        if mount_line is not None:
            problems.append(
                f"A4 {DEPLOY_COMPOSE}:{mount_line} 归档已关闭却仍挂载 `wal_archive` —— "
                "目录会被重新创建，且让后来的人以为归档在工作（2026-10-30 那次 59 GB 就是这么来的）"
            )
        return problems

    # ── 归档开启：逐条对应事故 ──────────────────────────────────────────────
    if "archive_command" not in conf:
        problems.append(
            f"A5 归档已开启但 {CONF} 里没有生效的 `archive_command` —— "
            "PG 会持续重试并让 pg_wal 无限增长"
        )
    else:
        cmd_lineno, cmd = conf["archive_command"]
        if "test ! -f" in cmd and not re.search(r"\|\|\s*(exit\s+0|true)", cmd):
            problems.append(
                f"A7 {CONF}:{cmd_lineno} `archive_command` 不幂等：用 `test ! -f` 做守卫却没有 "
                "`|| exit 0` 兜底 —— 目标已存在时命令恒返回 1，PG 会无限重试同一段、WAL 永不回收"
                "（实测 failed_count=26967 / 994 个 .ready）"
            )

    if "archive_cleanup_command" not in conf:
        problems.append(
            f"A6 归档已开启但 {CONF} 里没有生效的 `archive_cleanup_command` —— "
            "只写不删会把归档目录撑爆（实测 59 GB / 3760 段 / 9 天）"
        )

    for name in ARCHIVE_ALERTS:
        if name not in alerts:
            problems.append(
                f"A8 归档已开启但 {RULES} 里没有 `{name}` 告警 —— 归档挂掉（失败/停滞）将无人知晓"
            )
        if name not in prom_cases:
            problems.append(
                f"A9 归档已开启但 {PROMTEST} 里没有 `{name}` 的用例 —— 表达式或阈值被改坏时没有守卫"
            )

    if mount_line is None:
        problems.append(
            f"A10 归档已开启但 {DEPLOY_COMPOSE} 没有把归档目录挂到 PGDATA 之外"
            "（挂进数据卷等于「归档与数据同损」，PITR 失去意义）"
        )

    return problems


def run(root: Path | None = None) -> int:
    global ROOT
    if root is not None:
        ROOT = root

    problems = check(ROOT)
    if problems:
        for problem in problems:
            print(f"FAIL {problem}", file=sys.stderr)
        print(
            "\n  修法：`archive_mode` 与「清理命令 / 幂等命令 / 两条告警 / promtool 用例 / "
            "PGDATA 外挂载」必须同进同退。重启用清单见 docker/config/postgres.conf 的归档段注释。",
            file=sys.stderr,
        )
        return 1

    conf = read_conf(ROOT)
    mode = conf["archive_mode"][1].split()[0].lower()
    print(
        f"OK archive_mode = {mode}；"
        + (
            "关闭态：无归档告警、无 promtool 用例、无 wal_archive 挂载"
            if mode in ("off", "false")
            else "开启态：清理命令 / 幂等 / 两条告警 / promtool 用例 / 外挂载 齐全"
        )
    )
    return 0


def self_test() -> int:
    import contextlib
    import io
    import shutil

    GOOD_CMD = "test ! -f /var/lib/postgresql/wal_archive/%f && cp %p /var/lib/postgresql/wal_archive/%f || exit 0"
    DEADLOCK_CMD = "test ! -f /var/lib/postgresql/wal_archive/%f && cp %p /var/lib/postgresql/wal_archive/%f"

    def build(mutator) -> int:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for rel in (CONF, RULES, PROMTEST, DEPLOY_COMPOSE):
                (root / rel).parent.mkdir(parents=True, exist_ok=True)

            (root / CONF).write_text(
                "archive_mode = off\n"
                "# archive_command = 'test ! -f x && cp %p x'\n"
                "wal_keep_size = 256MB\n",
                encoding="utf-8",
            )
            (root / RULES).write_text(
                "groups:\n  - name: g\n    rules:\n"
                "      # 归档类告警已于 2026-10-30 随 archive_mode=off 一并删除："
                "PostgresArchiveFailing / PostgresArchiveStalled\n"
                "      - alert: SomethingElse\n        expr: up == 0\n",
                encoding="utf-8",
            )
            (root / PROMTEST).write_text(
                "tests:\n  # PostgresArchiveFailing 的用例已同步移除\n  - alert_rule_test: []\n",
                encoding="utf-8",
            )
            (root / DEPLOY_COMPOSE).write_text(
                "services:\n  postgres:\n    volumes:\n"
                "      - postgres_data:/var/lib/postgresql/data\n",
                encoding="utf-8",
            )
            mutator(root)
            sink = io.StringIO()
            with contextlib.redirect_stdout(sink), contextlib.redirect_stderr(sink):
                code = run(root)
            shutil.rmtree(root, ignore_errors=True)
            return code

    def write_all(
        root: Path,
        mode: str,
        cmd: str,
        cleanup: bool,
        alerts: bool,
        prom: bool,
        mount: bool,
    ) -> None:
        conf = [f"archive_mode = {mode}"]
        if cmd:
            conf.append(f"archive_command = '{cmd}'")
        if cleanup:
            conf.append(
                "archive_cleanup_command = 'pg_archivecleanup /var/lib/postgresql/wal_archive %r'"
            )
        (root / CONF).write_text("\n".join(conf) + "\n", encoding="utf-8")

        if alerts:
            (root / RULES).write_text(
                "groups:\n  - name: g\n    rules:\n"
                + "".join(
                    f"      - alert: {name}\n        expr: x > 0\n        for: 15m\n"
                    for name in ARCHIVE_ALERTS
                ),
                encoding="utf-8",
            )
        if prom:
            (root / PROMTEST).write_text(
                "tests:\n"
                + "".join(
                    f"  - alert_rule_test:\n      - alertname: {name}\n        exp_alerts: []\n"
                    for name in ARCHIVE_ALERTS
                ),
                encoding="utf-8",
            )
        if mount:
            (root / DEPLOY_COMPOSE).write_text(
                "services:\n  postgres:\n    volumes:\n"
                "      - postgres_data:/var/lib/postgresql/data\n"
                "      - ./wal_archive:/var/lib/postgresql/wal_archive\n",
                encoding="utf-8",
            )

    cases = [
        ("关闭态且干净（注释提及不算）", lambda r: None, 0),
        (
            "关闭态但残留告警规则",
            lambda r: write_all(r, "off", "", False, True, False, False),
            1,
        ),
        (
            "关闭态但残留 promtool 用例",
            lambda r: write_all(r, "off", "", False, False, True, False),
            1,
        ),
        (
            "关闭态但仍挂 wal_archive",
            lambda r: write_all(r, "off", "", False, False, False, True),
            1,
        ),
        (
            "开启态且五项齐全",
            lambda r: write_all(r, "on", GOOD_CMD, True, True, True, True),
            0,
        ),
        (
            "开启态缺 archive_command",
            lambda r: write_all(r, "on", "", True, True, True, True),
            1,
        ),
        (
            "开启态缺清理命令（只写不删）",
            lambda r: write_all(r, "on", GOOD_CMD, False, True, True, True),
            1,
        ),
        (
            "开启态命令不幂等（归档死锁）",
            lambda r: write_all(r, "on", DEADLOCK_CMD, True, True, True, True),
            1,
        ),
        (
            "开启态缺告警",
            lambda r: write_all(r, "on", GOOD_CMD, True, False, True, True),
            1,
        ),
        (
            "开启态缺 promtool 用例",
            lambda r: write_all(r, "on", GOOD_CMD, True, True, False, True),
            1,
        ),
        (
            "开启态但归档目录在 PGDATA 内",
            lambda r: write_all(r, "on", GOOD_CMD, True, True, True, False),
            1,
        ),
        (
            "archive_mode 缺失",
            lambda r: (r / CONF).write_text("wal_keep_size = 1GB\n", encoding="utf-8"),
            1,
        ),
        (
            "archive_mode 取值非法",
            lambda r: (r / CONF).write_text("archive_mode = maybe\n", encoding="utf-8"),
            1,
        ),
    ]

    failed = 0
    for name, mutator, expect in cases:
        got = build(mutator)
        ok = got == expect
        if not ok:
            failed += 1
        print(f"  {'OK  ' if ok else 'FAIL'} {name}: exit={got} (期望 {expect})")
    if failed:
        print(f"self-test: {failed} 个用例不符合预期", file=sys.stderr)
        return 1
    print(f"self-test: {len(cases)}/{len(cases)} 通过")
    return 0


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()
    return run()


if __name__ == "__main__":
    sys.exit(main())
