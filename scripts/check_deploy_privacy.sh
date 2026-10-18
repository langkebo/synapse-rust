#!/usr/bin/env bash
# B-7：断言「服务端不持有房间密钥」是仓库内所有部署形态的默认值。
#
# 为什么需要机器检查而不是文档：
#   配了 `server.megolm_device_id`，服务端就以该设备身份 encrypt/decrypt 房间事件，
#   于是**能读所有加密房间** —— 端到端加密对该部署不成立。
#   这条事实在运行时是可见的（/health 的 server_holds_room_keys、启动时的
#   security_audit 日志），但**默认值**只能靠静态检查守住：配置文件里多一行注释
#   开关，就可能把「我们的私聊是 E2EE」变成假话。
#
# 判定口径：
#   * 扫描仓库内所有部署配置（yaml / yml / toml / env），找 `megolm_device_id`
#   * 允许出现，但值必须为空、注释掉、或 `none`
#   * 任何一处给了真设备号 ⇒ 失败（退出码 1）
#
# 用法：
#   bash scripts/check_deploy_privacy.sh
# 退出码：0 = 默认关闭；1 = 有部署配置打开了服务端 megolm（输出文件:行号）。
set -uo pipefail
cd "$(git rev-parse --show-toplevel)"

fail=0

while IFS= read -r hit; do
    file="${hit%%:*}"
    rest="${hit#*:}"
    line="${rest%%:*}"
    text="${rest#*:}"

    # 去掉行首空白后判断：
    #  - `#` 开头 ⇒ 注释，跳过
    #  - 值为空 ⇒ 留空，跳过
    #  - 值为 none / null / ~ / "" / '' ⇒ 显式关闭，跳过
    trimmed="${text#"${text%%[![:space:]]*}"}"
    if [ "${trimmed:0:1}" = "#" ]; then
        continue
    fi

    value="$(printf '%s' "$trimmed" | sed -E 's/^[^:=]*[:=][[:space:]]*//')"
    # 去尾部注释、去引号、去空白：全部用参数展开，避免 sed/tr 引号嵌套
    # （实测 `tr -d '"'"'"' \t'` 这种写法会被 shell 解析成一堆碎片）。
    value="${value%%#*}"
    value="${value//\"/}"
    value="${value//\'/}"
    value="${value//[[:space:]]/}"

    case "$(printf '%s' "$value" | tr '[:upper:]' '[:lower:]')" in
        "" | none | null | "~") continue ;;
    esac

    echo "FAIL ${file}:${line} 部署配置启用了服务端 megolm（值: ${value}）"
    echo "     该部署将持有房间密钥并可读所有加密房间；请置空或删除该项。"
    fail=1
done < <(grep -rn --include='*.yaml' --include='*.yml' --include='*.toml' --include='*.env*' \
    -E '^[[:space:]]*#?[[:space:]]*megolm_device_id[[:space:]]*[:=]' . 2>/dev/null | grep -v '^\./target/')

if [ "$fail" -eq 0 ]; then
    echo "OK   所有部署配置均未启用服务端 megolm（server_holds_room_keys=false）"
fi
exit "$fail"
