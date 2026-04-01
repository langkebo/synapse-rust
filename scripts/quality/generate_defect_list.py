import argparse
import os
import re
from dataclasses import dataclass


@dataclass(frozen=True)
class Finding:
    kind: str
    name: str
    reason: str


def read_lines(path: str) -> list[str]:
    if not os.path.exists(path):
        return []
    with open(path, "r", encoding="utf-8", errors="replace") as f:
        return [line.rstrip("\n") for line in f.readlines() if line.strip()]


def parse_line(line: str) -> tuple[str, str]:
    if "\t" in line:
        name, reason = line.split("\t", 1)
        return name.strip(), reason.strip()
    m = re.match(r"^(.*)\((.*)\)\s*$", line)
    if m:
        return m.group(1).strip(), m.group(2).strip()
    return line.strip(), ""


def load_findings(results_dir: str) -> list[Finding]:
    findings: list[Finding] = []
    for kind, file_name in [
        ("FAILED", "api-integration.failed.txt"),
        ("MISSING", "api-integration.missing.txt"),
    ]:
        for line in read_lines(os.path.join(results_dir, file_name)):
            name, reason = parse_line(line)
            findings.append(Finding(kind=kind, name=name, reason=reason))
    return findings


def is_core_client_path(name: str) -> bool:
    core_keywords = [
        "Sync",
        "Presence",
        "Profile",
        "Account Data",
        "Device",
        "Devices",
        "Push",
        "Keys",
        "Room Timeline",
        "Room Members",
        "Room Member",
        "Room Receipt",
        "Room Receipts",
        "Room Typing",
        "Room Read",
        "Room Account Data",
        "User Directory",
    ]
    return any(k.lower() in name.lower() for k in core_keywords)


def priority_of(f: Finding) -> str:
    r = f.reason.lower()
    n = f.name.lower()
    if f.kind == "FAILED":
        return "P0"
    if "http 5" in r or "500" in r or "panic" in r or "crash" in r:
        return "P0"
    if "http 4" in r and is_core_client_path(f.name):
        return "P1"
    if "endpoint not available" in r or "not implemented" in r:
        return "P1" if is_core_client_path(f.name) else "P2"
    if "not found" in r:
        return "P1" if is_core_client_path(f.name) else "P2"
    if "admin" in n:
        return "P2"
    if "federation" in n or "sso" in n or "openid" in n:
        return "P2"
    return "P3"


def module_hint(name: str) -> str:
    n = name.lower()
    if "admin" in n:
        return "admin"
    if "federation" in n:
        return "federation"
    if "sso" in n or "openid" in n or "oidc" in n or "saml" in n or "cas" in n:
        return "sso"
    if "push" in n:
        return "push"
    if "device" in n:
        return "device"
    if "key" in n:
        return "e2ee"
    if "presence" in n:
        return "presence"
    if "profile" in n:
        return "profile"
    if "space" in n:
        return "space"
    if "room" in n:
        return "room"
    return "unknown"


def write_report(out_path: str, results_dir: str, findings: list[Finding]) -> None:
    os.makedirs(os.path.dirname(out_path), exist_ok=True)

    by_p: dict[str, list[Finding]] = {"P0": [], "P1": [], "P2": [], "P3": []}
    for f in findings:
        by_p[priority_of(f)].append(f)

    def rel(p: str) -> str:
        return os.path.relpath(p, os.getcwd()).replace("\\", "/")

    with open(out_path, "w", encoding="utf-8") as w:
        w.write("# 质量缺陷清单（来自 API 集成测试）\n\n")
        w.write("## 复现方式（统一）\n\n")
        w.write("- 运行：`SERVER_URL=http://localhost:28008 TEST_ENV=dev bash scripts/test/api-integration_test.sh`\n")
        w.write(f"- 产物：`{rel(os.path.join(results_dir, 'api-integration.failed.txt'))}` / `{rel(os.path.join(results_dir, 'api-integration.missing.txt'))}`\n\n")
        w.write("## 分级说明\n\n")
        w.write("- P0：阻塞/崩溃/500/FAILED\n")
        w.write("- P1：核心 Matrix Client 路径缺失或 4xx/不兼容\n")
        w.write("- P2：非核心但重要能力缺失（admin/federation/sso/第三方等）\n")
        w.write("- P3：优化与增强项（不阻塞主流程）\n\n")

        for p in ["P0", "P1", "P2", "P3"]:
            items = by_p[p]
            w.write(f"## {p}（{len(items)}）\n\n")
            if not items:
                w.write("- 无\n\n")
                continue
            for idx, f in enumerate(items, start=1):
                defect_id = f"{p}-API-{idx:03d}"
                w.write(f"### {defect_id}: {f.name}\n\n")
                w.write(f"- 来源：{f.kind}\n")
                if f.reason:
                    w.write(f"- 现象：{f.reason}\n")
                else:
                    w.write("- 现象：无详细原因（需补齐脚本断言信息）\n")
                w.write(f"- 影响范围：{module_hint(f.name)}\n")
                w.write("- 重现步骤：运行集成测试脚本，定位产物中同名条目\n")
                w.write("- 根因分析：待补齐（建议从对应路由是否存在/返回码与响应 schema 不一致入手）\n")
                w.write("- 修复建议：\n")
                w.write("  - 补齐端点实现或将现有实现与 Matrix/Synapse 行为对齐\n")
                w.write("  - 补充回归用例（单元/集成）覆盖成功与失败路径\n")
                w.write("- 验收标准：该条目从 MISSING/FAILED 变为 PASSED，且无新增回归\n\n")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--results-dir", default=os.environ.get("RESULTS_DIR", "test-results"))
    parser.add_argument("--out", default="docs/quality/defects_api_integration.md")
    args = parser.parse_args()

    findings = load_findings(args.results_dir)
    write_report(args.out, args.results_dir, findings)


if __name__ == "__main__":
    main()

