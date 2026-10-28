#!/usr/bin/env bash
#
# 跑一条测试命令，并**要求它真的跑了至少一个测试**。
#
# 为什么需要它（门禁诚实性清查 A2–A4，实测 2026-09-19）：
# libtest 在过滤器什么都没匹配到时打印 `running 0 tests` 并 **exit 0**。
# 于是 CI 里 `--test unit <模块名>` 这类步骤，只要模块名/目标名漂移（本仓库发生过
# 6 次：模块其实在 tests/integration 且名为 `*_migrated`；还有一条测试名全仓不存在），
# 步骤就变成"每次必绿的空转"，而且看不出任何异常。
#
# ── 判据来源：机制，不是日志文本（2026-10-28 重写）──────────────────────────
#
# 旧实现用正则去匹配**人类可读的进度文案**：
#   `Starting 3 tests across 1 binary`（nextest）、`test result: ok. 12 passed`（libtest）
# 这条路线已经连续制造三次假红，每次都只是把正则补一下：
#   ① `CARGO_TERM_COLOR=always` 让进度行带 SGR 序列 ⇒ 匹配不到（2026-09-20）
#   ② `strip_ansi | grep -q` 命中即退 + `pipefail` ⇒ 上游 EPIPE 141（2026-09-30）
#   ③ nextest 单数 `Starting 1 test across 1 binary` ⇒ 只写复数的正则必红（2026-10-27）
# 文案是给**人**看的，随时会变（加颜色、改单复数、换措辞），把它当判据就是把门禁
# 建在别人的排版上。现在改为问**框架自己**：
#
#   * `cargo nextest`：追加 `--no-tests=fail`。空集由 nextest 直接返回非零退出码，
#     不需要读任何输出。（`--no-tests=pass` 会被本脚本拒绝：那等于关掉本门禁。）
#   * `cargo test`（libtest，无等价旗标）：先跑一次 `--list` 问"注册表里有几条"，
#     数 `^<name>: test$` 行。`--list` 输出的是**用例清单**（机器可解析的结构），
#     不是汇总文案，因此不受颜色/单复数/措辞影响；命中 0 条就不必再跑真正的测试。
#   * 其它命令：无法机制化，退化为旧日志判据，并打印 ⚠️ 提示不要依赖它。
#
# 用法：
#   bash scripts/ci/require_tests_ran.sh cargo test --locked --test integration foo -- --test-threads=1
#
# 退出码：被测命令失败 → 原样透传；跑 0 个测试 → 1；用法/配置错误 → 2；正常 → 0。
set -uo pipefail

if [ "$#" -eq 0 ]; then
    echo "usage: $0 <command> [args...]" >&2
    exit 2
fi

# 认出 cargo 子命令：跳过 `cargo` 本体与 `+toolchain` 覆盖，取第一个实义子命令。
sub="__not_cargo__"
saw_cargo=0
for a in "$@"; do
    if [ "$saw_cargo" -eq 0 ]; then
        case "$a" in
            cargo | */cargo)
                saw_cargo=1
                continue
                ;;
            *) break ;;
        esac
    fi
    case "$a" in
        +*) continue ;;
        *)
            sub="$a"
            break
            ;;
    esac
done

# ── nextest：让框架自己判空集 ────────────────────────────────────────────────
if [ "$sub" = "nextest" ]; then
    have_flag=0
    for a in "$@"; do
        case "$a" in
            --no-tests=*)
                have_flag=1
                if [ "$a" != "--no-tests=fail" ]; then
                    echo "::error::'$a' would let this gate pass on an empty selection." >&2
                    echo "  This wrapper exists precisely to fail on 0 tests; drop the flag" >&2
                    echo "  (it is injected as --no-tests=fail) or change it to =fail." >&2
                    exit 2
                fi
                ;;
        esac
    done
    if [ "$have_flag" -eq 0 ]; then
        set -- "$@" --no-tests=fail
    fi
    # 不 tee：nextest 自己的退出码就是判据，劫持 stdout 只会再引入一次 SIGPIPE 风险。
    "$@"
    exit $?
fi

# ── libtest：问注册表，不读文案 ──────────────────────────────────────────────
if [ "$sub" = "test" ]; then
    cargo_args=()
    libtest_args=()
    pass_through=0
    for a in "$@"; do
        if [ "$pass_through" -eq 0 ] && [ "$a" = "--" ]; then
            pass_through=1
            continue
        fi
        if [ "$pass_through" -eq 0 ]; then
            cargo_args+=("$a")
        else
            libtest_args+=("$a")
        fi
    done

    # `--exact` 改变过滤语义（精确 vs 子串）。预检必须用同一语义，否则
    # `filter 写短了` 这种最常见的漂移会被子串匹配掩盖成"有测试"。
    list_args=(--list)
    if [ "${#libtest_args[@]}" -gt 0 ]; then
        for a in "${libtest_args[@]}"; do
            case "$a" in
                --exact) list_args=(--exact --list) ;;
            esac
        done
    fi

    # `grep -c` 读完整个输入（不像 `grep -q` 命中即退），配合 pipefail 也不会 EPIPE。
    # `--list` 只编译+列举，不执行用例，所以不会触碰 DB/Redis。
    n_listed="$(
        "${cargo_args[@]}" -- "${list_args[@]}" 2>/dev/null |
            grep -cE '^[^ ].*: test$'
    )" || true
    case "$n_listed" in
        '' | *[!0-9]*) n_listed=0 ;;
    esac

    if [ "$n_listed" -eq 0 ]; then
        echo "" >&2
        echo "::error::this step ran ZERO tests. The target/module filter no longer matches" >&2
        echo "  anything, so the step is a no-op that reports success. Fix the target/module" >&2
        echo "  name (or delete the step) — do not leave it green-by-emptiness." >&2
        echo "  --- probed with ---" >&2
        echo "  ${cargo_args[*]} -- ${list_args[*]}" >&2
        exit 1
    fi

    echo "OK: filter selects ${n_listed} test(s); running them."
    "$@"
    exit $?
fi

# ── 其它命令：没有机制判据，退回日志文本（并明说这是弱判据）────────────────
echo "::warning::'$sub' is not cargo test/nextest — falling back to log-text matching." >&2
echo "  This is layout-dependent (colour, singular/plural, wording). Prefer wrapping a" >&2
echo "  cargo test/nextest invocation, or add a mechanism-level check for this command." >&2

log="$(mktemp "${TMPDIR:-/tmp}/require_tests_ran.XXXXXX")"
trap 'rm -f "$log"' EXIT

"$@" 2>&1 | tee "$log"
rc=${PIPESTATUS[0]}
if [ "$rc" -ne 0 ]; then
    echo "::error::test command failed with exit $rc" >&2
    exit "$rc"
fi

ANSI_ESC=$'\033'
stripped="$(sed -E "s/${ANSI_ESC}\\[[0-9;]*[A-Za-z]//g" <"$log")"
# 三种已知布局都认：libtest 汇总行 / nextest 进度行 / nextest 结尾 Summary 行。
if ! grep -qE '(^[[:space:]]*test result: (ok|FAILED)\. [1-9][0-9]* passed|Starting [1-9][0-9]* tests?|Summary.*[^0-9][1-9][0-9]* tests? run)' <<<"$stripped"; then
    echo "" >&2
    echo "::error::this step ran ZERO tests (log-text fallback; see the warning above)." >&2
    tail -5 "$log" >&2
    exit 1
fi

echo "OK: this step actually ran tests."
exit 0
