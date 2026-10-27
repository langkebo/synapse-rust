#!/usr/bin/env python3
"""
gen_client_yaml.py — 生成 docs/openapi/client.yaml（CI artifact）

执行流:
  1. (可选) cargo run --bin synapse_ledger_export -- --profile=default --timestamp=<fixed>
  2. python3 scripts/api_test/generate_openapi.py --ledger <ledger> --output docs/openapi/client.yaml
  3. 在文件头部插入"禁止手改"注释

用法:
  # 完整流程 (需要 cargo)
  python3 scripts/api_test/gen_client_yaml.py

  # 跳过 export (假定 ledger.json 已就绪)
  python3 scripts/api_test/gen_client_yaml.py --skip-export

  # 跳过 export，但用**指定**的 ledger（CI 用它指向刚导出的 $RUNNER_TEMP/…）
  python3 scripts/api_test/gen_client_yaml.py --ledger /tmp/ledger_default_ci.json
"""

from __future__ import annotations

import argparse
import subprocess
import sys
import tempfile
from pathlib import Path

from artifact_common import FIXED_TIMESTAMP, describe_drift

SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parent.parent

LEDGER_EXPORT_BIN = "synapse_ledger_export"
GENERATOR_SCRIPT = SCRIPT_DIR / "generate_openapi.py"
OUTPUT_DIR = PROJECT_ROOT / "docs" / "openapi"
OUTPUT_FILE = OUTPUT_DIR / "client.yaml"
LEDGER_DEFAULT = (
    SCRIPT_DIR / "reports" / "ledger_default.json"
)  # export_ledger.sh 的默认输出

# `FIXED_TIMESTAMP` lives in artifact_common.py (the single literal, also read by
# `.github/workflows/ci.yml`'s export step and carried by
# `docs/openapi/route-table.json`); it is imported above so this file cannot
# drift from the committed artifact.

FORBIDDEN_HEADER = """# ============================================================================
# 本文件由 CI gen_client_yaml.py 自动生成，禁止手改。
# 生成命令: cargo run --bin synapse_ledger_export -- --profile=default
#          && python3 scripts/api_test/generate_openapi.py
# 如需刷新: python3 scripts/api_test/refresh_openapi_specs.py --profile default
# ============================================================================

"""


def run_cargo_export(commit: str | None, timestamp: str) -> Path:
    """Run synapse_ledger_export binary for default profile."""
    ledger_path = SCRIPT_DIR / "ledger_default_gen.json"
    cmd = [
        "cargo",
        "run",
        "--quiet",
        "--bin",
        LEDGER_EXPORT_BIN,
        "--",
        "--profile=default",
        f"--output={ledger_path}",
        f"--timestamp={timestamp}",
    ]
    if commit:
        cmd.append(f"--commit={commit}")

    print(f"[gen_client_yaml] Running: {' '.join(cmd)}")
    result = subprocess.run(cmd, capture_output=True, text=True, cwd=PROJECT_ROOT)
    if result.returncode != 0:
        print(
            f"[gen_client_yaml] cargo export FAILED:\n{result.stderr}", file=sys.stderr
        )
        sys.exit(1)
    print(f"[gen_client_yaml] ledger exported to {ledger_path}")
    return ledger_path


def run_openapi_generator(ledger_path: Path, output_path: Path) -> None:
    """Run generate_openapi.py to produce client.yaml at `output_path`."""
    cmd = [
        sys.executable,
        str(GENERATOR_SCRIPT),
        "--ledger",
        str(ledger_path),
        "--output",
        str(output_path),
    ]
    print(f"[gen_client_yaml] Running: {' '.join(cmd)}")
    result = subprocess.run(cmd, capture_output=True, text=True, cwd=PROJECT_ROOT)
    if result.returncode != 0:
        print(f"[gen_client_yaml] generator FAILED:\n{result.stderr}", file=sys.stderr)
        sys.exit(1)
    print(f"[gen_client_yaml] generated {output_path}")


def add_forbidden_header(file_path: Path) -> None:
    """Prepend the '禁止手改' header to the generated YAML file."""
    content = file_path.read_text(encoding="utf-8")
    if content.startswith(
        "# ============================================================================"
    ):
        print("[gen_client_yaml] header already present, skipping")
        return
    file_path.write_text(FORBIDDEN_HEADER + content, encoding="utf-8")
    print(f"[gen_client_yaml] added forbidden header to {file_path}")


def main() -> int:
    ap = argparse.ArgumentParser(description="Generate client.yaml as CI artifact")
    ap.add_argument("--commit", default=None, help="synapse-rust commit SHA to record")
    ap.add_argument(
        "--timestamp", default=FIXED_TIMESTAMP, help="Fixed generated_at timestamp"
    )
    ap.add_argument(
        "--skip-export",
        action="store_true",
        help="Skip cargo export, reuse reports/ledger_default.json (from export_ledger.sh)",
    )
    ap.add_argument(
        "--ledger",
        default=None,
        help=(
            "Path to an existing ledger JSON; implies --skip-export. "
            "Needed by CI: reports/ is gitignored, so on a clean checkout "
            "--skip-export alone can only fail — the fresh export lives in "
            "$RUNNER_TEMP instead."
        ),
    )
    ap.add_argument(
        "--check",
        action="store_true",
        help="Do not write: generate into a temp path and diff against --expected; exit 1 on drift",
    )
    ap.add_argument(
        "--expected",
        default=None,
        help=(
            "Reference artifact for --check. REQUIRED with --check — there is no default, "
            "because the generated file itself (`docs/openapi/client.yaml`) is gitignored "
            "(.gitignore:118) and therefore never exists on a clean checkout. Point this at a "
            "snapshot you generated earlier, not at the generated path."
        ),
    )
    args = ap.parse_args()

    # Fail fast on the one argument error that would otherwise cost a full
    # `synapse_ledger_export` run before being reported (phase 1 below).
    if args.check and args.expected is None:
        print(
            "[gen_client_yaml] CHECK FAILED: --check 必须显式给 --expected。\n"
            f"  本脚本的产出 `{OUTPUT_FILE}` 是**生成物**（`.gitignore:118` 排除），"
            "干净 checkout 上永远不存在 ⇒ 不能当参照物。\n"
            "  参照物应是你先前生成并留存的快照，例如：\n"
            "    python3 scripts/api_test/gen_client_yaml.py --ledger <ledger> "
            "--check --expected /tmp/client.prev.yaml\n"
            "  注意 `docs/openapi/*.yaml` 里入库的都是**分变体** spec "
            "（client-all / client-oidc / client-saml / client-worker），"
            "与 default profile 的产出不可逐字节比对。",
            file=sys.stderr,
        )
        return 1

    # Phase 1: Export ledger JSON (unless skipped)
    if args.ledger:
        ledger_path = Path(args.ledger)
        if not ledger_path.exists():
            print(
                f"[gen_client_yaml] --ledger 指定的清单不存在：{ledger_path}",
                file=sys.stderr,
            )
            return 1
        print(f"[gen_client_yaml] --ledger: using {ledger_path}")
    elif args.skip_export:
        ledger_path = LEDGER_DEFAULT
        if not ledger_path.exists():
            print(
                f"[gen_client_yaml] --skip-export 需要现成清单，但 {ledger_path} 不存在：\n"
                f"  先生成它：bash scripts/api_test/export_ledger.sh --profile=default\n"
                f"  或去掉 --skip-export 让本脚本自己 cargo 导出。",
                file=sys.stderr,
            )
            return 1
        print(f"[gen_client_yaml] --skip-export: using {ledger_path}")
    else:
        ledger_path = run_cargo_export(args.commit, args.timestamp)

    if args.check:
        # `--expected` already validated right after parse_args (fail fast).
        expected_path = Path(args.expected)
        if not expected_path.exists():
            print(
                f"[gen_client_yaml] CHECK FAILED: reference artifact not found: {expected_path}",
                file=sys.stderr,
            )
            return 1
        # Generate to a temp path: --check must never overwrite the reference artifact.
        with tempfile.TemporaryDirectory(prefix="client-yaml-check-") as tmpdir:
            generated_path = Path(tmpdir) / "client.yaml"
            run_openapi_generator(ledger_path, generated_path)
            add_forbidden_header(generated_path)
            rendered = generated_path.read_text(encoding="utf-8")
        expected = expected_path.read_text(encoding="utf-8")
        if expected == rendered:
            print(
                f"[gen_client_yaml] OK: {expected_path} matches a fresh generation from {ledger_path}"
            )
            return 0
        print(
            f"[gen_client_yaml] CHECK FAILED: {expected_path} is stale — it differs from a fresh "
            f"generation from {ledger_path}.",
            file=sys.stderr,
        )
        print(describe_drift(expected, rendered), file=sys.stderr)
        print(
            "    Regenerate the snapshot you are diffing against with:\n"
            f"      python3 scripts/api_test/gen_client_yaml.py --ledger <ledger> "
            f"--output <snapshot>\n"
            f"    (不要“commit 它”：`{OUTPUT_FILE}` 是生成物，从不入库)",
            file=sys.stderr,
        )
        return 1

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)

    # Phase 2: Generate OpenAPI YAML
    run_openapi_generator(ledger_path, OUTPUT_FILE)

    # Phase 3: Add forbidden header
    add_forbidden_header(OUTPUT_FILE)

    print(f"[gen_client_yaml] done: {OUTPUT_FILE}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
