#!/usr/bin/env bash
# 提交前把「会阻断 CI 的格式 / 拼写 / 契约」门禁一次跑全。
#
# 存在理由（2026-10-02 实测两次事故，都不是"改错了"而是"跑漏了"）：
#   * `416ece3c8` 漏跑 `cargo fmt` ⇒ Format Governance 红；
#   * `cf845cb35` 漏跑 `ruff format` ⇒ 同一条车道红。
# 单跑某一个门的习惯不可靠，故把 CI 的同名入口按改动集一次跑完。
#
# 用法：
#   bash scripts/quality/preflight.sh            # 快档（默认）
#   bash scripts/quality/preflight.sh --clippy   # 追加两档 clippy（约 7 分钟）
# 退出码：0 = 全部通过；非 0 = 有失败项（明细已在 stdout）。
set -uo pipefail
cd "$(git rev-parse --show-toplevel)"

# `npm_config_cache` 可能被环境预设成一个**当前用户不可写**的目录：实测
# `npm_config_cache=$HOME/.npm` 而该目录含 root 属主的 `_cacache` 文件 ⇒ `npx` 以
# EPERM 退出、markdownlint 步报 FAIL。那看起来像"文档格式红了"，实际是环境问题，
# 而且会拦住**所有人**的提交（2026-10-02 实测）。
#
# 探针打在 npm 自己会写的位置（`<_cacache>/tmp`），而不是缓存目录本身 —— 目录本身
# 可写、内层被 root 占有时，只探外层会误判为"可写"。
npm_cache_probe="${npm_config_cache:-/tmp/npm-cache}"
if ! mkdir -p "$npm_cache_probe/_cacache/tmp" 2>/dev/null ||
    ! (: >"$npm_cache_probe/_cacache/tmp/.preflight-write-probe") 2>/dev/null; then
    npm_cache_probe="$(git rev-parse --show-toplevel)/target/tmp/npm-cache"
    mkdir -p "$npm_cache_probe/_cacache/tmp" 2>/dev/null || true
    echo "NOTE npm_config_cache 原值不可写，本步改用 $npm_cache_probe"
fi
rm -f "$npm_cache_probe/_cacache/tmp/.preflight-write-probe" 2>/dev/null || true
export npm_config_cache="$npm_cache_probe"

fail=0
step() { printf '\n== %s ==\n' "$1"; }
run() { # run <描述> <命令...>
    local desc="$1"
    shift
    if "$@" >/tmp/preflight_step.log 2>&1; then
        echo "OK   $desc"
    else
        echo "FAIL $desc"
        tail -15 /tmp/preflight_step.log
        fail=1
    fi
}
# advise <命令...>：**只打印、不参与结论**。用于"该提醒但不该阻断"的步骤
# （例：改动面 → 必跑套件映射）。为什么必须在 preflight 里而不是只在文档里：
# 2026-10-24 的 NEW-17 改了 `rooms` 的序列化结构，当次只跑 routes 测试，
# 三条跨路径等价用例静默红了近两小时 —— 提示要出现在**提交前**才有用。
# 它也不能用 run：全量 integration 是 42 分钟，塞进 pre-commit 会把提交变成不可用。
advise() {
    local desc="$1"
    shift
    if "$@" >/tmp/preflight_advise.log 2>&1; then
        cat /tmp/preflight_advise.log
    else
        echo "NOTE $desc —— 提示步骤自身失败，不影响本次结论（请单独跑一遍看原因）"
        tail -5 /tmp/preflight_advise.log
    fi
}

# 改动集 = 已暂存 ∪ 未暂存（pre-commit 时改动已暂存，故两者都要看）
# 未跟踪的新文件也必须进改动集：`git diff` 看不到它们，而"新增一个 .py/.md 忘了格式化"
# 正是本脚本要防的场景之一。
#
# ⚠️ `-c core.quotePath=false` 不可省：该配置默认 true，非 ASCII 路径会被 Git 输出成
# `"docs/\346\226\207..."`（带引号 + 八进制转义），下面 `grep -E '\.md$'` 之类的
# **后缀**匹配于是全部落空 —— 实测 2026-10-08：本仓的核心方案文档
# `docs/前缀命名空间治理方案-2026-10-08.md`（中文名）被改了多轮，
# markdownlint / aspell / doc_credibility 三道文档守卫**每一次都静默 SKIP**，
# 而该文档当时真实带着 18 处 markdownlint 违规（AGENTS.md 铁律 8 的
# "门禁长期全绿 ⇒ 先怀疑它没在工作"，这里是"门禁根本没跑"）。
changed=$({
    git -c core.quotePath=false diff --name-only
    git -c core.quotePath=false diff --cached --name-only
    git -c core.quotePath=false ls-files --others --exclude-standard
} | sort -u)
md=$(printf '%s\n' "$changed" | grep -E '\.md$' | grep -v '/archive/' || true)
py=$(printf '%s\n' "$changed" | grep -E '\.py$' || true)
rs=$(printf '%s\n' "$changed" | grep -E '\.rs$' || true)

step "Rust 格式化"
run "cargo fmt --all" cargo fmt --all
run "./scripts/check_fmt_ratchet.sh" ./scripts/check_fmt_ratchet.sh

if [ -n "$py" ] || [ -n "$rs" ] || [ -n "$(printf '%s\n' "$changed" | grep -E '\.(json|ya?ml|sh)$' || true)" ]; then
    step "Format Governance 同款入口（ruff / shfmt / yamllint / check-json / format_audit）"
    run "scripts/quality/format_check.sh" bash scripts/quality/format_check.sh
fi

step "文档：逐文件 aspell + markdownlint"
for f in $md; do
    [ -f "$f" ] || continue
    run "aspell $f" bash scripts/check_doc_spelling.sh "$f"
done
if [ -n "$md" ]; then
    # shellcheck disable=SC2086
    run "markdownlint" npx --yes markdownlint-cli@0.49.1 -c .markdownlint.json $md
fi

# 章节棘轮：`docs/audit/UPSTREAM_CHANGELOG_ALIGNMENT_PLAN_2026-10-13.md` 反复（5 次）被一份
# **陈旧且重排过表格的副本**覆盖，每次都**整段吞掉**最新章节。根因是宿主 app 进程回放它自己
# 缓存的那份旧副本（证据见文档 §11.9 / §11.12），仓库侧无法阻止写入 ⇒ 唯一能做的是让它
# **不可能静默**：markdownlint / aspell 对"少了一整节"完全无感（文件仍能解析、格式也合法）。
#
# ⚠️ 2026-10-14 修正：本步骤原先**只在文档进入改动集时**才跑（理由："文件没变结论不可能变"）。
# 那个门控恰好绕过了真正的故障形态 —— 覆盖发生在你**正在改别的文件**的时候，于是下一次提交
# （改的不是这份文档）会 SKIP 掉这条守卫，损伤就一直躺在工作区里。脚本本身 ~30ms，改为**每次都跑**。
# 判据有两条：基线棘轮（`scripts/ci/plan_doc_sections_baseline`）+ **对照 `git show HEAD:<doc>`**
# （不依赖基线，因此基线万一被误 `--update` 也拦得住）。
step "文档：章节棘轮（防整段被吞，含对照 HEAD）"
run "check_plan_doc_sections.py" python3 scripts/ci/check_plan_doc_sections.py

# 文档数字随访：方案文档引用的棘轮数字必须与基线/实测一致。
# 与上面那条**同理每次都跑**（~10ms）：2026-10-15 的故障形态正是"棘轮口径改了/基线动了，
# 而文档照旧声称全绿"—— 只在该文档进入改动集时才跑，就恰好绕过它。
step "文档：方案数字随访（对照棘轮基线 + .sqlx 条数）"
run "check_plan_numbers_fresh.sh" bash scripts/ci/check_plan_numbers_fresh.sh

# 文档度量一致性：测试文件数 / 基线索引数必须等于脚本从真源现算的值。
# 与上面两条**同理每次都跑**（~0.2s）：漂移的形态就是"改了别的文件、没动文档"——
# 新增一个测试文件或一条 CREATE INDEX，四处手抄的数字同时失效且无人察觉
# （实测漂移过 300/238/111/116 与 342/347/349/351）。审计 TEST-07/PERF-12 的修法。
step "文档：度量随访（测试文件数 / 索引数，对照真源）"
run "check_doc_metrics.py" python3 scripts/ci/check_doc_metrics.py

# 部署隐私（B-7）：仓库内任何部署配置都不得启用服务端 megolm。
# 配了 `megolm_device_id` 服务端就持有房间密钥、能读所有加密房间，
# 「私聊是 E2EE」立刻变成假话。这条事实运行时可观测（/health +
# security_audit 日志），但**默认值**只能靠静态检查守住。
# 与上面两条文档棘轮同理每次都跑（~50ms）：故障形态恰恰是"改的是别的文件"。
step "部署隐私：服务端不得持有房间密钥（B-7）"
run "check_deploy_privacy.sh" bash scripts/check_deploy_privacy.sh

# 时序列决胜键棘轮：`ORDER BY <x>_ts` 单键会让同毫秒事件在页边界重复/漏掉。
# 每次都跑（~30ms）：故障形态是"新写了一条按 ts 排序的查询"，与改动集无关。
# 此前它只存在于脚本自身 —— CI 与 preflight 都没调用，等于没有门禁（审计 TEST-04）。
step "时序列决胜键棘轮（ORDER BY <x>_ts）"
run "check_ts_order_tiebreak.py" python3 scripts/ci/check_ts_order_tiebreak.py

# 元门禁：每个 `check_*` 脚本都必须真的被某条车道调用，或显式登记为手动入口。
# 每次都跑（~0.2s，只做 git ls-files + 文本闭包）：上面那一步之所以能被发现"从来没跑"，
# 靠的就是它。删掉任意一处调用，它立刻把对应脚本报成 DEAD。
step "门禁可达性（每个 check_* 都要有人调用，或显式登记）"
run "check_gate_reachability.py" python3 scripts/ci/check_gate_reachability.py

# `updated_ts` 可空性棘轮（ARCH-06）：两种形态并存是现状，规则已改口为"新表必须显式选"，
# 本步只是不让计数悄悄朝可空方向漂。~20ms，且只读基线 SQL，与改动集无关。
# `sleep` 站点棘轮（TEST-05）：分三桶计数，只许下降。~0.3s，只读 .rs 源码。
step "测试：sleep 站点棘轮"
run "check_sleep_ratchet.py" python3 scripts/ci/check_sleep_ratchet.py

# 整文件 feature 门控的测试文件（TEST-06）：`#![cfg(feature = "X")]` 写在测试文件里时，
# X 不在测试车道的 feature 集里就**整个文件不参与编译**，nextest 连"0 个用例"都不报。
# 静态检查（~20ms，只读 git ls-files + ci.yml）：每个这样的文件都必须在
# `scripts/ci/feature_gated_test_matrix` 登记，且登记的车道真的显式启用了该 feature。
# 与上面那条同理每次都跑：故障形态是"新增/改名一个门控测试文件"，与改动集无关。
step "测试：整文件 feature 门控的覆盖（TEST-06）"
run "check_test_feature_gates.py" python3 scripts/ci/check_test_feature_gates.py

# 「自证测试」面（NEW-05）：`*_shape_only_tests.rs` 必须登记 + 文件头声明，且登记的真覆盖文件
# 真的发请求/碰 DB；`*_{api,route}_tests.rs` 要么调生产符号、要么发请求，否则就是"名字在
# 声称覆盖、内容在自己造字面量"。静态检查（~40ms），与改动集无关 ⇒ 每次都跑。
step "测试：只锁形状的测试文件必须自报家门（NEW-05）"
run "check_shape_only_tests.py" python3 scripts/ci/check_shape_only_tests.py

# 测试夹具不得 fail-open（R1）。缺库/建池失败时返回 None 会被调用点的 `else { return; }`
# 折叠成 `ok` —— 实测让语音上传体积上限（含 CVE-2026-102592 的大小写绕过项）、媒体 2 MiB
# 边界、worker surface gate 共 9 条安全用例在 DB 不可达时**全部 PASS 且一条断言不跑**，
# 其中一条耗时还比真跑更长（30s vs 6s），"跑得快 = 没跑"的巡检看不见它。
# 见 `docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md` §2；守的是夹具本体而不是 559 个调用点。
step "测试：夹具不得 fail-open（R1）"
run "check_test_setup_fail_closed.py" python3 scripts/ci/check_test_setup_fail_closed.py

# 测试替身不得再"忽略参数却假装成功"（审计 §4.2.1）。
# `unimplemented!()` 是 fail-loud（调用即 panic ⇒ 测试红），安全；真正危险的是
# `fn get_user_rooms(_user_id) -> Ok(所有房间)` 这类：参数被忽略却返回成功，
# 于是"用户只看得到自己的房间""typing 被正确记录""泄露族已被封禁"这些断言在 mock 下**恒真**。
# 实测 106 处 / 429 个方法（24.7%），棘轮只许下降；`--print` 看完整清单。
step "测试替身：忽略参数假装成功的方法不得增加（棘轮）"
run "check_mock_stub_drift.py" python3 scripts/ci/check_mock_stub_drift.py

# 改动面 → 必跑套件（**提示，不阻断**）。
# 2026-10-24 的 NEW-17 改了 `rooms` 的序列化分桶，当次只跑 routes 测试，三条跨路径等价用例
# 静默红了近两小时 —— 这类"改了 A 却该跑 B 的测试"不在任何单文件的视野里，只能在提交前提示。
# 用 advise 而不是 run：跑不跑得成不该决定这次提交，漏跑才是真问题。
step "测试：改动面 → 必跑套件（提示，不阻断）"
advise "check_test_impact.py" python3 scripts/ci/check_test_impact.py

# 活文档里的 `scripts/...` 引用必须解析到真实文件（NEW-04）。归档与 .workbuddy 笔记不查：
# 那里写"当年调用过 X，已删除"是**历史叙述**，要求它存在就是错的。部署文档的相对路径
# 按 `docker/` 与 `docker/deploy/` 两个工作目录解析。当前未解析数由
# `scripts/ci/script_ref_baseline` 钉住，只许下降。~90ms。
step "文档：scripts 引用可达性（NEW-04）"
run "check_script_refs.py" python3 scripts/ci/check_script_refs.py

# 文档行号引用的漂移信号（DOC-04）：正文里的 `path:line` 已全部改成符号名；表格里的保留
# （能力台账的 C2 判据要解析 `path:line`），但**指向空行/纯注释/孤立 `}`** 说明那段代码挪过位置。
# 基线 `scripts/ci/doc_line_ref_baseline` 钉住当前 56 处，只许下降。~60ms。
step "文档：行号引用的漂移信号（DOC-04）"
run "check_doc_line_refs.py" python3 scripts/ci/check_doc_line_refs.py

step "Schema：updated_ts 可空性棘轮"
run "check_schema_timestamp_convention.py" python3 scripts/ci/check_schema_timestamp_convention.py

step "契约 gate"
if [ -z "$(printf '%s\n' "$changed" | grep -E 'ROUTE_CONTRACT|ledger_annotations|extract_registered|derived_route|ledger_export|route-table\.json|route_ledger' || true)" ]; then
    run "check_route_contract.sh" bash scripts/contract/check_route_contract.sh
else
    echo "SKIP 本次改动了契约链文件 ⇒ 该 gate 在提交前构造性红（它把重生成的 ROUTE_CONTRACT.md 与 HEAD 比），"
    echo "     改在提交后复核（参见 docs/audit/OPTIMIZATION_EXECUTION_PLAN_2026-09-15.md §8.3 L-6）"
    # D-5：sidecar 新鲜度不依赖 HEAD（只把源码抽取结果与已提交的 ROUTE_HANDLERS.json 比），
    # 因此必须在这里也跑。跳过的恰好是「改了路由/ledger 注解但忘了同提交重生成」这一形态。
    run "gen_route_handlers.py --check" python3 scripts/contract/gen_route_handlers.py --check
fi

# 路由鉴权口径（SEC-15）：每条路由要么由 `AuthenticatedUser`/`AdminUser` 等提取器把住，
# 要么在 `scripts/contract/route_auth_public.txt` 里逐条登记不这么做的理由。
# 为什么独立成 step 而不是并进上面的契约 gate：`route_layer` 上的鉴权中间件
# （federation X-Matrix / admin / replication / webhook guard）在路由抽取器里是**不可见**的
# （`apply_call` 把 `layer`/`route_layer` 当透明装饰器），所以判据不能靠抽取器推断，
# 只能靠逐条白名单；而这一步只读源码 + 白名单、不与 HEAD 比，因此在"改了契约链文件"的
# 分支里也必须照跑 —— 放进上面的 if/else 会被 SKIP 分支吞掉。
# 双向：漏登记 ⇒ 红；登记了但路由已消失 / 现已带提取器 ⇒ 也红。~1s。
step "路由鉴权口径（SEC-15）"
run "check_route_auth.py" python3 scripts/contract/check_route_auth.py

# 文档可信度守卫：`docs/synapse-rust-vs-synapse-comparison.md` 的计数必须等于
# `ROUTE_CONTRACT.md`，引用的路径必须存在，五个历史章节必须带 §18 指针。
#
# 存在理由（2026-10-02 实测）：该守卫**早就存在**，但从未进提交前脚本；L-6 把
# `ROUTE_CONTRACT.md` 更新到 1,154 后，对比报告三处仍写 1,152，守卫长期**判红**
# 而无人察觉 —— "守卫存在"不等于"守卫在跑"（AGENTS.md 铁律 8 推论）。
# 只在文档或契约链变动时运行：守卫读的两个文件都没变时它的结论不可能变。
step "文档可信度守卫（计数 / 路径 / 历史章节指针）"
if [ -n "$md" ] || [ -n "$(printf '%s\n' "$changed" | grep -E 'ROUTE_CONTRACT|route_ledger|ledger_export|derived_route' || true)" ]; then
    run "doc_credibility_guard_tests" env SQLX_OFFLINE=true cargo nextest run --test unit --features test-utils -E 'test(doc_credibility_guard_tests)'
else
    echo "SKIP 本次未改动 .md / 契约链 ⇒ 守卫读的文件未变"
fi

if [ "${1:-}" = "--clippy" ]; then
    step "clippy（两档，CI 同款）"
    run "clippy (test-utils)" env SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --locked -- -D warnings
    run "clippy (all-features)" env SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings
    # CODE-01：生产代码（lib+bins）禁 `unreachable!()`。作用域刻意不含测试目标 ——
    # 与 ci.yml / .githooks/pre-push 的同名步骤逐字相同，三处任一漂移都会被
    # `tests/unit/ci_test_scope_tests.rs::production_code_denies_unreachable_macro` 抓住。
    run "clippy (无 unreachable!)" env SQLX_OFFLINE=true cargo clippy --workspace --lib --bins --all-features --locked -- -D warnings -D clippy::unreachable
fi

printf '\n== 结论 ==\n'
if [ "$fail" -eq 0 ]; then
    # ⚠️ 不能笼统写"全部通过"：默认**不跑** clippy（`--clippy` 才跑）。
    # 2026-10-24 实测两次：`media_thumbnail` 加到 8 个参数（too_many_arguments）、
    # 删掉 `notify_device_revocation` 后 `json`/`BackgroundJob` 变成未用导入 ——
    # 两次都是 `-D warnings` 会红，而提交时看到的是"OK 一大片 + 全部通过"，
    # 直到手工跑 CI 那条 clippy 才发现。结论行必须把这件事说出来。
    if [ "${1:-}" = "--clippy" ]; then
        echo "全部通过（CI 同名门禁 + clippy 两档）"
    else
        echo "通过（⚠️ 未跑 clippy：加 --clippy 或设 SYNAPSE_PRECOMMIT_CLIPPY=1 才会跑；CI 每 push 跑两档）"
    fi
else
    echo "有失败项 —— 修完再提交"
fi
exit "$fail"
