#!/usr/bin/env bash
# A1 / P1-1 残余：`SYNAPSE_ALLOW_STATE_TS_FALLBACK` 只允许活在回填窗口里，
# 不得进入仓库内的任何部署配置。
#
# 为什么需要机器门禁而不是发布说明：
#   状态决议的读路径已改为 fail-closed（`f3a91641a`）：房间没有 `state_groups`
#   记录就读不到当前状态，而不是回落到 `ORDER BY origin_server_ts DESC` 的推导。
#   `SYNAPSE_ALLOW_STATE_TS_FALLBACK` 是 `scripts/migration/backfill_state_groups.py`
#   跑完之前的临时逃生开关 —— 它一旦留在部署清单里，整进程就**永久**回到旧推导，
#   等价于 fail-closed 从未生效；而这件事在运行时完全静默（照常出结果，只是状态
#   顺序由发送方时钟决定，联邦事件后到时可能与 DAG 顺序相反）。
#
# 判定口径（与 check_deploy_privacy.sh 不同：这里没有"安全值"）：
#   * 实现是 `std::env::var_os(..).is_some()` —— 只看存在性，写成空值也等于开启；
#   * 因此任何激活行（首个非空白字符不是 `#`）一律失败；纯注释行允许（供文档说明）。
#
# 用法：
#   bash scripts/ci/check_state_ts_fallback_absent.sh
# 退出码：0 = 部署配置里没有该开关；1 = 有（逐条输出 文件:行号）。
set -uo pipefail
cd "$(git rev-parse --show-toplevel)"

VAR='SYNAPSE_ALLOW_STATE_TS_FALLBACK'
fail=0

while IFS= read -r hit; do
    file="${hit%%:*}"
    rest="${hit#*:}"
    line="${rest%%:*}"
    text="${rest#*:}"

    # 去掉行首空白后判断：`#` 开头 ⇒ 注释，跳过（其余一律算激活，含空值）。
    trimmed="${text#"${text%%[![:space:]]*}"}"
    if [ "${trimmed:0:1}" = "#" ]; then
        continue
    fi

    echo "FAIL ${file}:${line} 部署配置启用了状态决议的时间戳兜底（值: ${trimmed}）"
    echo "     ${VAR} 只看存在性（空值也算开启），会让整进程永久回到 origin_server_ts 推导；"
    echo "     请删除该项，改跑 scripts/migration/backfill_state_groups.py。"
    fail=1
done < <(grep -rn --include='*.yaml' --include='*.yml' --include='*.toml' --include='*.env*' \
    -e "$VAR" . 2>/dev/null | grep -v '^\./target/')

if [ "$fail" -eq 0 ]; then
    echo "OK   部署配置均未启用状态决议时间戳兜底（${VAR} 仅限回填窗口使用）"
fi
exit "$fail"
