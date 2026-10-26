#!/usr/bin/env python3
"""守卫：`test_mocks/` 里「忽略参数却假装成功」的方法不得再增加。

## 守的是什么

`synapse-storage/src/test_mocks/` 的 in-memory 替身有两种"未实现"：

1. **fail-loud**：`unimplemented!("… has no database pool")` —— 调用即 panic ⇒ 测试红。
   **这是安全的**，3 处全部如此，本守卫不管。
2. **fail-silent**：参数写成 `_user_id` / `_room_id` 表示"不打算用"，但方法体返回 `Ok(...)`。
   ⇒ 调用方拿到"成功"，任何依赖该参数的断言**恒真**。

第 2 类是真正的风险，且可机械检测。实测（2026-10-26，上一轮审计 `§4.2.1`）106 处 / 429 个方法，
其中最危险的几处会让**安全语义的断言永远成立**：

* `room.rs::get_user_rooms(_user_id)` → 返回**全部**房间 ⇒ "用户只看得到自己的房间"永远通过；
* `presence.rs::set_typing(_room_id, _user_id, _typing)` → 全参数忽略、返回成功
  ⇒ "typing 被正确记录/清除"永远通过；
* `refresh_token.rs::mark_family_compromised(_family_id)` → 忽略族 id、返回成功
  ⇒ "泄露族被封禁"永远通过；
* `quarantine_media.rs::get_media_quarantine_status(_media_id, _server_name)` → 忽略 media
  ⇒ "被隔离的媒体不可下载"永远通过。

## 为什么是「只防新增」而不是一次性清零

106 处里多数是良性的（如 `cleanup_expired_tokens(_now_ts)`：mock 无"过期"概念，
返回 `Ok` 不产生副作用，调用方不会因此得出错误结论）。一次性清零要先逐个裁定"实现 or 改
fail-loud"，那是独立一笔；本守卫先把**增量**钉死，避免问题继续扩大。

## 判定口径

扫 `test_mocks/*.rs` 中**至少一个参数以 `_` 开头**、且函数体**不含**
`unimplemented!` / `todo!` / `panic!` / `unreachable!` 的函数。数量与
`scripts/ci/mock_stub_drift_baseline` 比较：**超过即红**，下降时提示可以下调基线（棘轮）。

白名单：无。要"合法地忽略参数"，请在该方法体里补一行
`// mock-ignores-param: <理由>` —— 守卫认这个标记（比全局白名单更贴近代码，且 review 时一眼可见）。
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MOCKS = ROOT / "synapse-storage/src/test_mocks"
BASELINE = ROOT / "scripts/ci/mock_stub_drift_baseline"
ACK = "mock-ignores-param:"

FN = re.compile(r"^(\s*)(?:pub\s+)?(?:async\s+)?fn\s+(\w+)\s*\(")
FAIL_LOUD = re.compile(r"unimplemented!|todo!|panic!|unreachable!")


def load_baseline() -> int:
    if not BASELINE.is_file():
        return 0
    for line in BASELINE.read_text().splitlines():
        line = line.split("#", 1)[0].strip()
        if line.startswith("MAX_IGNORED_PARAM_METHODS="):
            return int(line.split("=", 1)[1])
    return 0


def scan() -> list[tuple[str, int, str, list[str]]]:
    hits: list[tuple[str, int, str, list[str]]] = []
    for path in sorted(MOCKS.glob("*.rs")):
        lines = path.read_text(errors="ignore").split("\n")
        rel = path.relative_to(ROOT).as_posix()
        index = 0
        while index < len(lines):
            match = FN.match(lines[index])
            if not match:
                index += 1
                continue
            name = match.group(2)
            # 拼接跨行签名（到出现 `)` 为止，最多 6 行）
            signature = lines[index]
            probe = index + 1
            while ")" not in signature and probe < len(lines) and probe < index + 6:
                signature += " " + lines[probe]
                probe += 1
            params = (
                signature.split("(", 1)[1].rsplit(")", 1)[0] if "(" in signature else ""
            )
            ignored = re.findall(r"(?:^|,)\s*(_\w+)\s*:", params)
            # 函数体：到下一个同级 fn 或 40 行
            body: list[str] = []
            for cursor in range(probe, min(probe + 40, len(lines))):
                if FN.match(lines[cursor]):
                    break
                body.append(lines[cursor])
            blob = "\n".join(body)
            if ignored and not FAIL_LOUD.search(blob) and ACK not in blob:
                hits.append((rel, index + 1, name, ignored))
            index = probe
    return hits


def changed_mock_files() -> set[str]:
    """本次改动涉及的 mock 文件（用于把失败输出收敛到"你刚加的那条"）。"""
    try:
        result = subprocess.run(
            ["git", "diff", "--name-only", "HEAD"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
    except OSError:
        return set()
    if result.returncode != 0:
        return set()
    return {
        line.strip() for line in result.stdout.splitlines() if "test_mocks/" in line
    }


def main() -> int:
    hits = scan()
    baseline = load_baseline()

    if "--print" in sys.argv[1:]:
        for rel, line, name, ignored in hits:
            print(f"{rel}:{line}  {name}({', '.join(ignored)})")
        print(f"# 共 {len(hits)} 处；基线 {baseline}")
        return 0

    print(
        f"test_mocks 中「忽略参数且不 fail-loud」的方法：{len(hits)} 处（基线上限 {baseline}）"
    )

    if len(hits) > baseline:
        changed = changed_mock_files()
        focus = [h for h in hits if h[0] in changed]
        print(
            f"\nFAIL 比基线多 {len(hits) - baseline} 处。新增的每一处都会让依赖该参数的断言恒真："
        )
        if focus:
            print("  本次改动文件里的命中（最可能就是新增的那几条）：")
            for rel, line, name, ignored in focus:
                print(f"    {rel}:{line}  {name}({', '.join(ignored)})")
        else:
            print(
                "  （本次没有改动 test_mocks/，说明是别处引入的；完整清单见 `--print`）"
            )
        print(
            f"\n 完整 {len(hits)} 处清单：`python3 scripts/ci/check_mock_stub_drift.py --print`\n"
            "\n修法（三选一）：\n"
            "  1. 真正用它 —— 让 mock 按参数过滤（多数情况几十行）；\n"
            f'  2. 改成 fail-loud：`unimplemented!("why")`，让调用它的测试红；\n'
            f"  3. 确属良性则在方法体加一行 `// {ACK} <理由>`。\n"
            "背景：docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md §4.2.1"
        )
        return 1

    if len(hits) < baseline:
        print(
            f"OK 比基线少 {baseline - len(hits)} 处 —— 可以把 {BASELINE.name} 下调到 {len(hits)}（棘轮只许下降）"
        )
    else:
        print("OK 未超过基线（棘轮：只许下降）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
