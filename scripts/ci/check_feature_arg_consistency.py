#!/usr/bin/env python3
"""RD-15：出货 feature 串必须**只有一个真相源**。

背景（2026-10-30 实测）
----------------------
同一份「出货镜像带哪些 feature」被 6 个入口各写了一份，而且全都停在"7 项"：

    --features core-private-chat,widgets,external-services,voice-extended,cas-sso,saml-sso,friends --no-default-features

分别是 `docker/Dockerfile`、`docker/complement/Dockerfile`（两处）、
`docker/docker-compose.yml`、`docker-security-scan.yml`、`e2e-http.yml`、
`scripts/test/run_element_web_browser_harness.sh`、`scripts/federation-test/cleanup_and_deploy.sh`。

后果不是"不好看"，而是三条**结论失效**：

1. `docker compose build` 出来的镜像比 `deploy.sh --all` 少 5 项能力
   （beacons / voip-tracking / server-notifications / privacy-ext / builtin-oidc）——
   本地与 CI 起栈验证的功能 ≠ 出货的功能；
2. **Trivy 扫的就是那 7 项镜像**，于是"镜像扫描通过"对出货镜像不成立；
3. Complement 协议兼容性测试跑的是更小的集合，「测过了」覆盖不到出货组合。

现在：`docker/Dockerfile` 的 `ARG CARGO_FEATURE_ARGS` 默认值是唯一真相源，
其余入口要么不传（继承默认），要么必须与它**逐字相等**。本脚本守这条。

判据（全部可失败）
------------------
* **F1** 真相源本身可解析：`--features <列表>` + `--no-default-features`，
  且列出的每个 feature 都在根 `Cargo.toml` 的 `[features]` 里（拼错即红）。
* **F2** 任何 Dockerfile 里的 `ARG CARGO_FEATURE_ARGS=` 默认值 == 真相源。
* **F3** `docker/**/docker-compose*.yml` **不得**在 build args 里覆盖
  `CARGO_FEATURE_ARGS`（"compose 不覆盖"是这条的显式不变式）。
* **F4** `.github/workflows/*.yml` 与 `scripts/**/*.sh` 里凡是**字面量**给
  `CARGO_FEATURE_ARGS` 赋值的地方，值必须 == 真相源。
  （`deploy.sh` 是按模式拼出来的字符串，不含字面量，不在此列 —— 这正是
    "单一默认 + 显式覆盖"的分工：默认只有一个，覆盖必须发生在调用点。）
* **F5** fail-closed：扫到的站点数少于下限就红，避免扫描器坏掉时空转通过。

注释与字符串里的举例不算（本文件与 Dockerfile 的说明性注释里都引用了旧串）。
剥注释复用 `tests/unit/workflow_pipefail_tests.rs` 的同一条判据：`#` 只有出现在
行首或空白之后才开始注释（`--build-arg FOO=a#b` 里的 `#` 属于值）。

用法
----
    python3 scripts/ci/check_feature_arg_consistency.py            # 门禁（红=exit 1）
    python3 scripts/ci/check_feature_arg_consistency.py --self-test # 正/负样本自检
"""

from __future__ import annotations

import re
import sys
import tempfile
from pathlib import Path

import tomllib

ROOT = Path(__file__).resolve().parents[2]
CANONICAL_FILE = ROOT / "docker/Dockerfile"
ROOT_TOML = ROOT / "Cargo.toml"

# 只扫这些入口。显式列表而不是全树 rglob：`docker/` 下有 59GB 的 wal_archive
# 与 postgres 数据目录，全树遍历既慢又容易被误伤。
DOCKERFILES = ("docker/Dockerfile", "docker/complement/Dockerfile")
COMPOSE_GLOBS = ("docker/*.yml", "docker/deploy/*.yml", "docker/*/docker-compose*.yml")
WORKFLOW_GLOB = ".github/workflows/*.yml"
SCRIPT_GLOBS = ("scripts/*.sh", "scripts/**/*.sh")

# F5 下限：至少要在 2 个 Dockerfile 里看到 ARG 默认值，否则说明真相源或扫描坏了。
MIN_DOCKERFILE_SITES = 2

ARG_RE = re.compile(r"^\s*ARG\s+CARGO_FEATURE_ARGS\s*=\s*(?P<value>.+?)\s*$")
# `[built-in] CARGO_FEATURE_ARGS=…` / `--build-arg CARGO_FEATURE_ARGS=…` 两种写法都覆盖。
# 负向后视排除 `DOCKER_CARGO_FEATURE_ARGS=…` 这类**另一个**变量（那是 compose 的直通口，
# 已随单一真相源一起删除；若将来又被加回来，这里会以"找不到该 arg"的形式在别处暴露，
# 而不是被误报成"值与真相源不等"）。
ASSIGN_RE = re.compile(r"(?<![A-Za-z0-9_])CARGO_FEATURE_ARGS\s*=\s*(?P<value>.+?)\s*$")
FEATURES_FLAG_RE = re.compile(r"--features\s+(?P<list>[A-Za-z0-9_,.-]+)")


def strip_comment(line: str) -> str:
    """去掉行尾 `# …` 注释（`#` 需在行首或空白之后）。"""
    for index, char in enumerate(line):
        if char == "#" and (index == 0 or line[index - 1].isspace()):
            return line[:index]
    return line


def unquote(value: str) -> str:
    value = value.strip()
    if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
        return value[1:-1].strip()
    # YAML 里可能带行内注释以外的尾随空白，已由 strip_comment 处理
    return value


def declare(value: str) -> str:
    """把一条赋值规整成可比较的形式（去引号、压缩空白）。"""
    return " ".join(unquote(value).split())


def canonical() -> str:
    text = CANONICAL_FILE.read_text(encoding="utf-8")
    for line in text.splitlines():
        match = ARG_RE.match(line)
        if match:
            return declare(match.group("value"))
    raise SystemExit(
        f"FAIL 在 {CANONICAL_FILE.relative_to(ROOT)} 里找不到 ARG CARGO_FEATURE_ARGS"
    )


def root_feature_names() -> set[str]:
    data = tomllib.loads(ROOT_TOML.read_text(encoding="utf-8"))
    return set(data.get("features", {}))


def check_canonical(value: str) -> list[str]:
    problems: list[str] = []
    if "--no-default-features" not in value.split():
        problems.append(
            f"F1 真相源必须带 `--no-default-features`（Cargo 的 default 里还有 beacons，"
            f"留着它等于多一个隐形来源）：{value}"
        )
    match = FEATURES_FLAG_RE.search(value)
    if not match:
        problems.append(f"F1 真相源里没有 `--features <列表>`：{value}")
        return problems
    listed = [f for f in match.group("list").split(",") if f]
    known = root_feature_names()
    unknown = [f for f in listed if f not in known]
    if unknown:
        problems.append(f"F1 真相源引用了 Cargo.toml 未声明的 feature：{unknown}")
    if "all-extensions" not in listed:
        problems.append(
            f"F1 真相源应使用 `all-extensions` 聚合名（否则新增能力时又会漏加）：{listed}"
        )
    return problems


def scan_dockerfiles(value: str) -> tuple[list[str], int]:
    problems: list[str] = []
    sites = 0
    for rel in DOCKERFILES:
        path = ROOT / rel
        if not path.exists():
            problems.append(f"F5 预期存在的 Dockerfile 找不到：{rel}")
            continue
        for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            match = ARG_RE.match(strip_comment(line))
            if not match:
                continue
            sites += 1
            got = declare(match.group("value"))
            if got != value:
                problems.append(
                    f"F2 {rel}:{lineno} ARG 默认值与真相源不等\n      期望: {value}\n      实际: {got}"
                )
    return problems, sites


def scan_compose_setting() -> list[str]:
    """F3：compose 不得覆盖该 build arg。"""
    problems: list[str] = []
    for pattern in COMPOSE_GLOBS:
        for path in sorted(ROOT.glob(pattern)):
            for lineno, line in enumerate(
                path.read_text(encoding="utf-8").splitlines(), 1
            ):
                code = strip_comment(line)
                if "CARGO_FEATURE_ARGS" not in code:
                    continue
                problems.append(
                    f"F3 {path.relative_to(ROOT)}:{lineno} compose 不得覆盖 CARGO_FEATURE_ARGS"
                    "（不传即继承 Dockerfile 默认值；需要小镜像请在 build 命令上显式 --build-arg）\n"
                    f"      实际: {code.strip()}"
                )
    return problems


def scan_literals(value: str) -> tuple[list[str], int]:
    """F4：workflow 与脚本里凡字面量赋值，必须等于真相源。"""
    problems: list[str] = []
    sites = 0
    targets: set[Path] = set()
    for pattern in (WORKFLOW_GLOB, *SCRIPT_GLOBS):
        targets.update(ROOT.glob(pattern))
    for path in sorted(targets):
        for lineno, line in enumerate(
            path.read_text(encoding="utf-8", errors="replace").splitlines(), 1
        ):
            code = strip_comment(line)
            if "CARGO_FEATURE_ARGS" not in code:
                continue
            match = ASSIGN_RE.search(code)
            if not match:
                # 只出现变量名（如 `"$CARGO_FEATURE_ARGS"`）不是赋值，跳过
                continue
            sites += 1
            got = declare(match.group("value"))
            if got != value:
                problems.append(
                    f"F4 {path.relative_to(ROOT)}:{lineno} 字面量与真相源不等\n"
                    f"      期望: {value}\n      实际: {got}"
                )
    return problems, sites


def run(root: Path | None = None) -> int:  # root 参数供 self-test 复用
    global ROOT, CANONICAL_FILE, ROOT_TOML
    if root is not None:
        ROOT = root
        CANONICAL_FILE = root / "docker/Dockerfile"
        ROOT_TOML = root / "Cargo.toml"

    value = canonical()
    problems = check_canonical(value)
    if problems:
        for problem in problems:
            print(f"FAIL {problem}", file=sys.stderr)
        return 1

    dockerfile_problems, dockerfile_sites = scan_dockerfiles(value)
    compose_problems = scan_compose_setting()
    literal_problems, literal_sites = scan_literals(value)
    problems = dockerfile_problems + compose_problems + literal_problems

    if dockerfile_sites < MIN_DOCKERFILE_SITES:
        problems.append(
            f"F5 只扫到 {dockerfile_sites} 处 ARG 默认值（下限 {MIN_DOCKERFILE_SITES}）—— 扫描器坏了，"
            "空转通过比红更危险"
        )

    if problems:
        for problem in problems:
            print(f"FAIL {problem}", file=sys.stderr)
        print(
            "\n  修法：feature 集的唯一真相源是 docker/Dockerfile 的 "
            "`ARG CARGO_FEATURE_ARGS`；其余入口要么不传，要么逐字照抄。",
            file=sys.stderr,
        )
        return 1

    print(
        f"OK 真相源 = {value}\n"
        f"   守护 {dockerfile_sites} 处 Dockerfile ARG / "
        f"{literal_sites} 处字面量赋值 / compose 零覆盖"
    )
    return 0


def self_test() -> int:
    """正/负样本：证明每条判据真的会红，且注释不会误伤。"""
    import contextlib
    import io
    import shutil

    def build(mutator) -> int:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "docker").mkdir()
            (root / "scripts/ci").mkdir(parents=True)
            (root / ".github/workflows").mkdir(parents=True)
            good = "--features all-extensions --no-default-features"
            (root / "Cargo.toml").write_text(
                "[features]\nall-extensions = []\nfriends = []\n", encoding="utf-8"
            )
            (root / "docker/Dockerfile").write_text(
                f'ARG CARGO_FEATURE_ARGS="{good}"\n', encoding="utf-8"
            )
            (root / "docker/complement/Dockerfile").parent.mkdir(parents=True)
            (root / "docker/complement/Dockerfile").write_text(
                f'ARG CARGO_FEATURE_ARGS="{good}"\n', encoding="utf-8"
            )
            (root / "docker/docker-compose.yml").write_text(
                "services:\n  app:\n    build:\n      args:\n        CARGO_BUILD_JOBS: 2\n",
                encoding="utf-8",
            )
            (root / ".github/workflows/ci.yml").write_text(
                "jobs:\n  build:\n    steps:\n      - run: echo hi\n", encoding="utf-8"
            )
            (root / "scripts/ci/dummy.sh").write_text("# 无赋值\n", encoding="utf-8")
            mutator(root, good)
            # 期望失败的用例会往 stderr 打 FAIL 行，这里吞掉，否则看起来像自检失败
            sink = io.StringIO()
            with contextlib.redirect_stdout(sink), contextlib.redirect_stderr(sink):
                code = run(root)
            shutil.rmtree(root, ignore_errors=True)
            return code

    def ok_untouched(root, good):
        pass

    def bad_compose(root, good):
        root.joinpath("docker/docker-compose.yml").write_text(
            f"services:\n  app:\n    build:\n      args:\n        CARGO_FEATURE_ARGS: {good}\n",
            encoding="utf-8",
        )

    def bad_dockerfile(root, good):
        root.joinpath("docker/complement/Dockerfile").write_text(
            'ARG CARGO_FEATURE_ARGS="--features friends --no-default-features"\n',
            encoding="utf-8",
        )

    def bad_literal(root, good):
        root.joinpath(".github/workflows/ci.yml").write_text(
            "build-args: |\n  CARGO_FEATURE_ARGS=--features friends --no-default-features\n",
            encoding="utf-8",
        )

    def bad_comment_should_pass(root, good):
        # 注释里出现旧串**必须**通过（否则说明剥注释没生效）
        root.joinpath("scripts/ci/dummy.sh").write_text(
            '# 历史: CARGO_FEATURE_ARGS="--features friends --no-default-features"\n',
            encoding="utf-8",
        )

    def bad_canonical(root, good):
        root.joinpath("docker/Dockerfile").write_text(
            'ARG CARGO_FEATURE_ARGS="--features all-extensions"\n', encoding="utf-8"
        )
        root.joinpath("docker/complement/Dockerfile").write_text(
            'ARG CARGO_FEATURE_ARGS="--features all-extensions"\n', encoding="utf-8"
        )

    cases = [
        ("干净树", ok_untouched, 0),
        ("注释里的旧串", bad_comment_should_pass, 0),
        ("compose 覆盖", bad_compose, 1),
        ("Dockerfile 漂移", bad_dockerfile, 1),
        ("workflow 字面量漂移", bad_literal, 1),
        ("真相源缺 --no-default-features", bad_canonical, 1),
    ]
    failed = 0
    for name, mutator, expect in cases:
        got = build(mutator)
        status = "OK  " if got == expect else "FAIL"
        if got != expect:
            failed += 1
        print(f"  {status} {name}: exit={got} (期望 {expect})")
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
