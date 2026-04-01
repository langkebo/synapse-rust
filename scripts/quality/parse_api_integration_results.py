import argparse
import os
import re
from collections import Counter


def read_lines(path: str) -> list[str]:
    if not os.path.exists(path):
        return []
    with open(path, "r", encoding="utf-8", errors="replace") as f:
        return [line.rstrip("\n") for line in f.readlines() if line.strip()]


def parse_name_reason(lines: list[str]) -> list[tuple[str, str]]:
    out: list[tuple[str, str]] = []
    for line in lines:
        if "\t" in line:
            name, reason = line.split("\t", 1)
            out.append((name.strip(), reason.strip()))
        else:
            out.append((line.strip(), ""))
    return out


def summarize_reasons(items: list[tuple[str, str]], top_n: int) -> list[tuple[str, int]]:
    counter: Counter[str] = Counter()
    for _, reason in items:
        reason = reason.strip()
        if not reason:
            continue
        reason = re.sub(r"\s+", " ", reason)
        counter[reason] += 1
    return counter.most_common(top_n)


def write_md(
    out_path: str,
    results_dir: str,
    passed: list[str],
    failed: list[tuple[str, str]],
    missing: list[tuple[str, str]],
    skipped: list[tuple[str, str]],
    top_n: int,
) -> None:
    os.makedirs(os.path.dirname(out_path), exist_ok=True)

    def rel(p: str) -> str:
        return os.path.relpath(p, os.getcwd()).replace("\\", "/")

    with open(out_path, "w", encoding="utf-8") as f:
        f.write("# API 集成测试结果摘要\n\n")
        f.write(f"- results_dir: `{results_dir}`\n")
        f.write(f"- passed: `{len(passed)}`\n")
        f.write(f"- failed: `{len(failed)}`\n")
        f.write(f"- missing: `{len(missing)}`\n")
        f.write(f"- skipped: `{len(skipped)}`\n\n")

        f.write("## 重点结论\n\n")
        if failed:
            f.write("- 存在 FAILED：应视为实现缺陷/回归，优先排查并修复。\n")
        else:
            f.write("- 无 FAILED：当前失败项为 0。\n")
        if missing:
            f.write("- 存在 MISSING：应视为后端缺口清单（端点缺失/未实现/不对齐），进入缺陷清单并排期补齐。\n")
        else:
            f.write("- 无 MISSING：当前缺口清单为 0。\n")
        f.write("\n")

        if failed:
            f.write("## Failed（Top Reasons）\n\n")
            for reason, count in summarize_reasons(failed, top_n):
                f.write(f"- {count}\t{reason}\n")
            f.write("\n")

        if missing:
            f.write("## Missing（Top Reasons）\n\n")
            for reason, count in summarize_reasons(missing, top_n):
                f.write(f"- {count}\t{reason}\n")
            f.write("\n")

        if skipped:
            f.write("## Skipped（Top Reasons）\n\n")
            for reason, count in summarize_reasons(skipped, top_n):
                f.write(f"- {count}\t{reason}\n")
            f.write("\n")

        f.write("## 产物文件\n\n")
        f.write(f"- passed: `{rel(os.path.join(results_dir, 'api-integration.passed.txt'))}`\n")
        f.write(f"- failed: `{rel(os.path.join(results_dir, 'api-integration.failed.txt'))}`\n")
        f.write(f"- missing: `{rel(os.path.join(results_dir, 'api-integration.missing.txt'))}`\n")
        f.write(f"- skipped: `{rel(os.path.join(results_dir, 'api-integration.skipped.txt'))}`\n")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--results-dir", default=os.environ.get("RESULTS_DIR", "test-results"))
    parser.add_argument("--out", default="reports/quality/api_integration_summary.md")
    parser.add_argument("--top", type=int, default=30)
    args = parser.parse_args()

    results_dir = args.results_dir
    passed = read_lines(os.path.join(results_dir, "api-integration.passed.txt"))
    failed = parse_name_reason(read_lines(os.path.join(results_dir, "api-integration.failed.txt")))
    missing = parse_name_reason(read_lines(os.path.join(results_dir, "api-integration.missing.txt")))
    skipped = parse_name_reason(read_lines(os.path.join(results_dir, "api-integration.skipped.txt")))

    write_md(args.out, results_dir, passed, failed, missing, skipped, args.top)


if __name__ == "__main__":
    main()

