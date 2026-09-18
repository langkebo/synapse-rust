#!/usr/bin/env bash
#
# 跑一条测试命令，并**要求它真的跑了至少一个测试**。
#
# 为什么需要它（门禁诚实性清查 A2–A4，实测 2026-09-19）：
# libtest 在过滤器什么都没匹配到时打印 `running 0 tests` 并 **exit 0**。
# 于是 CI 里 `--test unit <模块名>` 这类步骤，只要模块名/目标名漂移（本仓库发生过
# 6 次：模块其实在 tests/integration 且名为 `*_migrated`；还有一条测试名全仓不存在），
# 步骤就变成"每次必绿的空转"，而且看不出任何异常。nextest 同理，除非显式
# `--no-tests=fail`。这里把"必须跑过测试"变成退出码，一处实现、多处复用（铁律 2）。
#
# 用法：
#   bash scripts/ci/require_tests_ran.sh cargo test --locked --test integration foo -- --test-threads=1
#
# 退出码：被测命令失败 → 原样透传；跑了 0 个测试 → 1；正常 → 0。
set -uo pipefail

if [ "$#" -eq 0 ]; then
    echo "usage: $0 <command> [args...]" >&2
    exit 2
fi

log="$(mktemp "${TMPDIR:-/tmp}/require_tests_ran.XXXXXX")"
trap 'rm -f "$log"' EXIT

# 保留完整输出（CI 日志里仍然可见），同时落盘用于判定。
"$@" 2>&1 | tee "$log"
rc=${PIPESTATUS[0]}

if [ "$rc" -ne 0 ]; then
    echo "::error::test command failed with exit $rc" >&2
    exit "$rc"
fi

# libtest:  `test result: ok. 12 passed; 0 failed; …`
# nextest:  `Starting 12 tests across 1 binary`
ran_zero=0
if grep -qE '^test result: (ok|FAILED)\. 0 passed' "$log"; then
    ran_zero=1
elif ! grep -qE '(^test result: (ok|FAILED)\. [1-9][0-9]* passed|Starting [1-9][0-9]* tests)' "$log"; then
    # 既看不到"0 passed"也看不到任何正数 —— 说明命令根本没跑测试框架
    # （例如目标名写错、被 feature 门控掉、或输出格式变了）。
    ran_zero=1
fi

if [ "$ran_zero" -eq 1 ]; then
    echo "" >&2
    echo "::error::this step ran ZERO tests. The target/module filter no longer matches" >&2
    echo "  anything, so the step is a no-op that reports success. Fix the target/module" >&2
    echo "  name (or delete the step) — do not leave it green-by-emptiness." >&2
    echo "  --- last lines of output ---" >&2
    tail -5 "$log" >&2
    exit 1
fi

echo "OK: this step actually ran tests."
exit 0
