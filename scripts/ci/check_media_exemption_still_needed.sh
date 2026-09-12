#!/usr/bin/env bash
# 守卫：main CI 里对 media 套件的豁免是否还应该存在？
#
# ── 背景 ─────────────────────────────────────────────────────────────────────
# `.github/workflows/ci.yml` 的主 lib 门禁排除了
# `synapse-services::media::tests`（13 个用例），并在单独的
# 独立的守卫步骤里跑它。原因是该套件存在**进程内串扰**：
# 实测同一命令连跑 4 次得到 0 / 1 / 3 / 3 个失败，失败集还每次漂移；
# 单独跑某个用例又通过。让它留在 blocking 门禁里会让 CI 随机红。
#
# 但"临时豁免"最容易变成"永久豁免"：一旦有人把串扰修好，那个
# `continue-on-error` 和排除式往往没人回头删，等于**永久缩小了覆盖**。
# 本脚本就是那个"回头删"的自动化。
#
# ── 判定 ─────────────────────────────────────────────────────────────────────
# 把 media 套件连跑 $RUNS 次（默认 3）：
#
#   * 只要**有任何一次失败** → 豁免仍然必要 → exit 0（保持现状）
#   * **每一次都通过**      → 串扰很可能已修好 → exit 1，要求：
#                             1) 从主 lib 步骤的 `-E 'not test(/^media::tests::/)'` 里移除该模式
#                             2) 删除 `Check media exemption is still necessary (self-cleaning guard)` 步骤
#
# 注意（能力边界，如实写明）：本脚本**不能证明**不存在偶发失败，只能证明
# "连跑 $RUNS 次未复现"。若某次偶发失败没被抽到，它会误报"可以收回了"——
# 这是刻意选择的偏向：宁可让人去看一眼，也不要让豁免无声永续。
# 想更保守就调大 RUNS，例如：RUNS=20 bash scripts/ci/check_media_exemption_still_needed.sh
#
# 用法：
#   bash scripts/ci/check_media_exemption_still_needed.sh          # 默认 3 次
#   RUNS=10 bash scripts/ci/check_media_exemption_still_needed.sh
#
# 需要可用的 TEST_DATABASE_URL（与 CI 的 test job 一致）。

set -uo pipefail

RUNS="${RUNS:-3}"
MEDIA_FILTER='test(/^media::tests::/)'

# 默认跑真实的 media 套件。允许覆盖**仅为自测本脚本**（验证"全通过 → exit 1"
# 这条分支不需要真造一个全绿的 media 套件）。CI 不设置它。
MEDIA_TEST_CMD_OVERRIDE="${MEDIA_TEST_CMD:-}"
MEDIA_TEST_CMD="${MEDIA_TEST_CMD_OVERRIDE:-cargo nextest run -p synapse-services --lib --all-features --locked --test-threads 1 -E $MEDIA_FILTER}"

echo "==> 检查 media 豁免是否仍必要：连跑 ${RUNS} 次 \`${MEDIA_FILTER}\`"

# The override is only used by the script's own regression test; when it is set we
# skip the DB precondition so the guard's branching can be verified without a DB.
if [ -z "$MEDIA_TEST_CMD_OVERRIDE" ] && [ -z "${TEST_DATABASE_URL:-}${DATABASE_URL:-}" ]; then
    echo "WARN: TEST_DATABASE_URL / DATABASE_URL 未设置 —— media 套件需要数据库。" >&2
    echo "      无数据库时无法判定，按'豁免仍必要'处理（exit 0）。" >&2
    exit 0
fi

passed_runs=0
failed_runs=0
for i in $(seq 1 "$RUNS"); do
    if eval "$MEDIA_TEST_CMD" >/dev/null 2>&1; then
        passed_runs=$((passed_runs + 1))
        echo "    run ${i}/${RUNS}: PASS"
    else
        failed_runs=$((failed_runs + 1))
        echo "    run ${i}/${RUNS}: FAIL（预期——豁免仍必要）"
    fi
done

echo "==> 结果：passed=${passed_runs} failed=${failed_runs}（共 ${RUNS}）"

if [ "$failed_runs" -eq 0 ]; then
    echo "" >&2
    echo "ERROR: media 套件连跑 ${RUNS} 次全部通过，豁免很可能已经不需要了。" >&2
    echo "" >&2
    echo "请做两件事，然后本守卫会继续绿：" >&2
    echo "  1) .github/workflows/ci.yml 主 lib 步骤：从" >&2
    echo "     -E 'not test(/^media::tests::/)' 里移除该排除模式" >&2
    echo "     （即主门禁恢复为不带 -E 的 \`--workspace --lib\`）" >&2
    echo "  2) 删除 'Check media exemption is still necessary (self-cleaning guard)' 这一步" >&2
    echo "  3) 同步更新 TESTING.md §1.2 里关于 13 个 skipped 的说明" >&2
    echo "" >&2
    echo "若你确信偶发失败仍存在（只是这 ${RUNS} 次没抽到），请调大采样：" >&2
    echo "  RUNS=20 bash scripts/ci/check_media_exemption_still_needed.sh" >&2
    exit 1
fi

echo "==> 豁免仍然必要（至少有 1 次失败）。保持现状。"
exit 0
