# 测试代码质量整改发现的问题总账（2026-10-28）

本文件回答一个问题：**把测试/门禁代码本身当对象去改，一共挖出了哪些项目问题，各自现在什么状态。**
它不是新的审计，而是把 `CI_LANE_BLINDNESS_2026-10-27.md`、`TEST_COMPROMISE_AUDIT_2026-10-25.md`
与本轮批次 A/B/C 的结论收成一张**可核验的账**，并单独列出**仍未解决的部分**。

## 0 为什么"改测试代码"能挖出项目问题

门禁与测试是**判据**。判据写错时，红与绿同时失去信息量：红不代表代码坏，绿不代表代码好。
所以每遇到一条红，先问三个问题，再决定动不动代码：

1. **这条红是判据自己造的，还是被守的对象真坏了？**
2. **这条判据有没有过绿的状态？**（引入早于工作流首跑 = "出生即红"，从没有过绿）
3. **它没红的时候，是真的跑了，还是被 `skipped`？**（`skipped` 不是绿）

下面四类问题就是按这三个问题分出来的。第 3 类（真实缺陷）是副产物：门禁修对之后，
原本被假红掩盖的真实条目才浮出来。

## 1 A 类：判据把绿判成红（假红）

| 编号 | 问题 | 判据（实测） | 状态 |
|------|------|--------------|------|
| A1 | `scripts/ci/require_tests_ran.sh` 只匹配复数 `Starting N tests`，而 nextest 恰跑 1 条时打 `Starting 1 test` ⇒ `Appservice statistics cost gate` 在两条 isolation 车道 100% 报 `ran ZERO tests`，自 `ea8276d8e` 引入起**从未绿过** | 日志里是 `PASS (1/1) … statistics_per_service_cost_does_not_grow_with_service_count` 加 `Summary 1 test run` | ✅ 已修 `1149c89eb`（正则 `tests?` + 单数正控 + `Starting 0 tests` 反控；旧脚本 `exit 1` / 新脚本 `exit 0`） |
| A2 | 同一脚本：ANSI SGR 把 `Starting` 与数字打断 ⇒ 3/3 passed 被判空转 | 2026-09-20 | ✅ 已修 |
| A3 | 同一脚本：`grep -q` 命中即退 + `set -o pipefail` ⇒ 上游 `sed` EPIPE 141 ⇒ 大日志被判"没跑测试" | 2026-09-30 | ✅ 已修 |
| A4 | `scripts/check_schema_blind_guards.py` 的 `SAFE_FILES` 是**整文件豁免**（对所有规则生效），而 `WARNING_BUDGET` 只登记 1 条、实际 3 条 ⇒ `DB Migration Gate` 的 `Schema Blind Guards` 从上线（10-19）起红 | 命中 20 条、`ERROR 2`、`EXIT 1`；`dev-test-setup.sh` 的命中在门禁上线**之前**就在树里 | ✅ 已修 `74d607c84`（改逐规则豁免；现在 `EXIT 0`、`INFO 19`、`ERROR 0`） |
| A5 | 豁免机制自身无校验：缺 `reason`、缺 `rules`、`rules` 为空集、规则编号写错，一律静默生效 | — | ✅ 已修（同 A4 提交，`validate_exemptions()` 在 `main()` 首行调用） |
| A6 | workflow 注释里写着**不存在**的守卫函数 `ci_test_scope_tests.rs::docker_security_scan_base_images_are_read_from_the_dockerfile` | 该文件无此函数，真实守卫在 `workflow_pipefail_tests.rs` | ✅ 已修 `b900f8c30` |
| A7 | `Digest Pin Integrity` 把三个 digest **抄一份**写死在 workflow ⇒ Dockerfile 升 pin 后仍验**旧** digest，自证不了"当前 pin 有效" | — | ✅ 已修（`read_base_image_pins.sh` 为唯一读取入口） |
| A8 | `.githooks/pre-push` 的 `cargo deny check advisories` 把 **advisory-db 拉取失败**当成"代码有漏洞"阻断 push ⇒ 人被逼 `--no-verify`（本轮实测踩到，见 §1.1） | 输出为 `failed to prepare fetch: An IO error occurred when talking to the server`；同一棵树上 `cargo deny check advisories -d`（不 fetch）**`advisories ok`** | ✅ 已修并**两侧实测**（回落放行 / 复核失败仍阻断，见 §1.2） |

共性：A1 与 A2/A3 是**同一个脚本第三次**"判据写死单一合法输出形态"；A8 是**第四种**同型形态，
但换了个位置——它把**环境故障**也折叠成"代码缺陷"。根因一致：判据的输入是外部世界（日志文本 /
网络），而不是"被测对象本身的状态"。

### 1.1 A8 的现场（本轮真实踩到，不修就得绕过门禁）

纯文档提交被 pre-push 拦下：

```text
2026-10-28 00:01:07 [ERROR] failed to fetch advisory database https://github.com/rustsec/advisory-db:
failed to prepare fetch: An IO error occurred when talking to the server:
error sending request for url (.../advisory-db/info/refs?service=git-upload-pack)

❌ push 被拦下：cargo-deny advisories 未通过（见上方输出）。
  绕过：git push --no-verify（仅紧急情况）
```

同树上加 `--disable-fetch`（用本地已有副本）⇒ `advisories ok`、`EXIT 0`。两条读数的差别只有
"能不能连上 github.com"，与代码无关。钩子里的注释早就写了代理透传（`HTTPS_PROXY`），但代理本身
返回 502，"注入代理"救不了"代理不可用"。

### 1.2 A8 的修复做了两侧实测（不是"改完就算"）

回落的**放行侧**：把 `HTTPS_PROXY`/`HTTP_PROXY` 指向 `http://127.0.0.1:9`（必然连不上）后直接跑钩子：

```text
[ERROR] failed to fetch advisory database ...
⚠️  advisory-db 拉取失败（网络/代理），改用本地已存在的副本复核：99670cfc 2026-10-27
advisories ok
⚠️  advisories ok —— **但这是本地副本的结论，不含最新公告**；
    以 CI 的 Security Audit 为准（它每次新拉 db）。本次 push 放行。
```

**阻断侧**（反控）：把回落分支里的 `cargo deny check advisories -d` 改成带一个不存在的参数，
使本地复核也失败 —— 必须仍然阻断，否则"回落"就变成了"无条件放行"：

```text
❌ push 被拦下：本地副本复核也失败（不是网络问题，见上方输出）。
  绕过：git push --no-verify（仅紧急情况）
```

两条都符合预期：判据只在"确认是网络问题"时降级，且降级**不冒充通过**（输出是 ⚠️ 而不是 ✅，
并打印本地 db 的 HEAD 与日期，让读者知道这个结论的时效边界）。

## 2 B 类：判据结构上"看不到"（不是假红，是根本没人看）

| 编号 | 问题 | 判据（实测） | 状态 |
|------|------|--------------|------|
| B1 | `Security Audit` 是**路径过滤**触发。近 12 次 `ci.yml` 里 **5 次 `skipped`、5 次 `failure`** ⇒ 供应链检查多数提交上根本不跑 | 因此下面的 C1/C2 从 10-22 起就在树里、没人读到 | ⚠️ 未修（结构性） |
| B2 | 没有任何东西在验"**pin 落后**"：`Digest Pin Integrity` 只验 pin 在 registry 上仍有效 | — | ✅ 已加 `resolve-base-image-digests`（`workflow_dispatch` 专用，不影响 push/PR）`1670d1876` |
| B3 | per-file 覆盖率棘轮**从未在 CI 跑过**（30 次运行 `Code Coverage` 恒 `skipped`），608 条基线来自本地产出、从未被 CI 复现 | `c952ee2b0` 提交说明自述 | ✅ 已跑通（首次），但红转移到棘轮数据本身（见 §4.1） |
| B4 | `Benchmark` / `Run benchmarks` 红：`##[error]The runner has received a shutdown signal` + `exit 143`，日志停在 `Compiling synapse-rust`；该 job **没配 `timeout-minutes`** | 两次同型 ⇒ runner 被回收，不是断言失败也不是超时 | ⚠️ 未处理 |

## 3 C 类：门禁修对之后浮出来的真实缺陷

| 编号 | 问题 | 关键读数 | 状态 |
|------|------|----------|------|
| C1 | `tokio 1.53.1` / `RUSTSEC-2026-0386`（`AsyncFd` I/O safety，`unsound`，补丁区间 `>=1.53.3`） | 全仓 `AsyncFd` **零引用** ⇒ 暴露面为零；`cargo deny check advisories` = `advisories ok` | ✅ 升 `1.54.1` `1149c89eb` |
| C2 | `xxhash-rust 0.8.19` 的 `BSL-1.0` 不在 `deny.toml` 名单内（`022cbd67f` 把 redis 0.29→1.7 的传递依赖） | `error[rejected]: failed to satisfy license requirements` | ✅ 按 `webpki-roots` 同口径加 **crate 级例外**，不塞全局 `allow` |
| C3 | `distroless/cc-debian12` 里 `libssl3 3.0.20` 带 `CVE-2026-84782`（HIGH，修复版 `3.0.22`） | pin 实测**就是**上游 `latest`（换 digest 无用，是上游没重建）；被扫的 `runtime-distroless` **全仓无构建/部署入口**；出货镜像走 `--target tools`，实测装到 `3.0.22` | ✅ 已裁决**删死路径**（`runtime-libs` / `runtime-distroless`），**不用** `.trivyignore` 压制 `b900f8c30` |
| C4 | `synapse-common/src/tracing.rs` 覆盖率 19.4%（基线 74.59%），而该文件在基线之后**零提交** | 真因是**基线跨文件重写**：基线期 248 行 / 10 个内联测试，`090742266` 删 149 行死代码后 101 行 / 1 个测试；棘轮 `save_baseline()` 取 `max(prev, cur)`（只上不下）⇒ 一次**合法**的删除把 55 个点永久冻红 | ✅ 补 2 个内联测试覆盖 `on_new_span` 两条分支 → `LH 84 / LF 85` = 98.82%，**基线未动** `d6c81769f` |
| C5 | schema 池两条用例**跨用例互杀**：orphan guard 扫全库把 CI 里约 100 个遗留 schema 拖到 139 s，`DROP` 与并发克隆争抢共享锁表触发 `out of shared memory`；TTL reaper 的 1 s 窗口抢走并发用例刚停放的 `parked` 行并 `DROP` 掉 | CI 报 `the parked schema must survive the sweep` 失败 | ✅ 已修 `c753eae21`（窗口收窄到单候选 / TTL 1 s→5 min） |
| C6 | 测试基础设施自己漏 schema：13 个用例 → 13 个残留 schema，因 atexit drain 从没在该进程注册 | `tests/integration/mod.rs` 注释记录的 2026-10-20 实测 | ✅ 早已修（`ensure_schema_exit_hook()`） |
| C7 | 集成测试"数据库不可达 ⇒ `return None` ⇒ libtest 记 `ok`"，基础设施故障被折叠成"通过" | 实测 47 条 / 0.04 s | ✅ 2026-10-18 改为 **panic**；`TestContext::build` 现在恒返回 `Some`，`Option` 仅为兼容约 50 个既有调用点 |

注：C7 这条顺带**证伪**了 `TEST_COMPROMISE_AUDIT_2026-10-25.md` §7.7.1 里"leg 2 集成测试大面积
静默跳过"的头号候选 —— §7.7.2 已按"基线跨文件重写"收口。

## 4 现存问题清单（未解决，按可操作性排序）

### 4.1 覆盖率 per-file 棘轮（最需要决策的一条）

最近一次有效读数（`0a5310346`）是 **134 文件 + 6 条 core-path**。本轮只修掉两条：

- `tracing.rs`（C4，已 98.82%）；
- 互杀导致的整片塌陷（C5）。

**2026-10-29 复核更新**：`134 + 6` 这个口径已被**脚本实测**取代（本节末尾「实测分档」）。
它的价值只在于「不许 `--update`」这条禁令——**该禁令仍然有效**，理由没变：`DROP` 那一类
（101 条）仍无法归因到具体成因，按当前测量重写基线等于把未定性的值永久固化成新地板。
6 条 core-path 全部是 `was no baseline`：

| 文件 | 读数 | 门槛 |
|------|------|------|
| `synapse-services/auth/credential_auth.rs` | 0.0% | 70% |
| `synapse-services/auth/token_auth.rs` | 0.0% | 70% |
| `synapse-storage/event/history_boundary.rs` | 0.0% | 70% |
| `synapse-storage/event/cascade.rs` | 48.6% | 70% |
| `synapse-web/routes/msc4512_proxy.rs` | 24.8% | 70% |
| `synapse-web/routes/worker/worker_topology.rs` | 45.1% | 70% |

三条 `0.0%` 必须先按 §7.5 的方法逐条分流（缺测试 / 无消费者 / 跨 crate / `feature` 关），
**不能一律补测试** —— 对"无可测行"的文件补测试是无效劳动。

> **实测分档（2026-10-29，`classify_coverage_drift.py` 跑在本地完整 lcov 上）**：
> 报告 677 文件 / 基线 683 条目 / 磁盘源码 3394 文件。低于基线的共 181 条（其中 55 条落在
> core-path），按成因拆开是 `LOST-HITS 3 + DROP 101 + DENOM 77`，另有 `GONE-NOT-BUILT 3 +
> GONE-SOURCE 4 + NEW-FILE 1 + RISE 19 + EQUAL 434`。
>
> - **`LOST-HITS 3`（唯一可判为真回归的一类）**，三条全部**分母不变、命中绝对减少**：
>   `synapse-services/room/membership/actions.rs` 81.42→80.97（LF 678→678）、
>   `synapse-services/background_update_service.rs` 90.29→89.56（LF 412→412）、
>   `synapse-services/burn_after_read_service.rs` 92.95→92.62（LF 908→908）。
> - `DROP 101` 是**基线没记 `lines_found`** 的历史条目（成对字段 2026-10-28 才引入），
>   不能当回归读，也不能当噪声忽略 —— 它正是「禁止 `--update`」的对象。
> - `DENOM 77` 是分母增长（新增代码未被覆盖），绝对覆盖量没有退步。
> - `GONE-NOT-BUILT 3` / `GONE-SOURCE 4` 都**不是退化**：前者是 feature/target 口径漏编译，
>   后者是文件已删除/重命名。
>
> 由此「134 + 6 待逐条定性」中的**定性问题已解决**（每个数字都有了归类），
> 但**归因问题只解决了 3 条**（LOST-HITS），101 条 `DROP` 仍需按源文件在基线冻结后的
> 行长变化逐条核。

### 4.2 棘轮语义缺陷（机制问题，不是数据问题）

`save_baseline()` 取 `max(prev, cur)`（只上不下），而唯一能刷新基线的 CI auto-ratchet 步
从未成功过。后果：**任何合法的"减少代码"都会把旧高水位永久冻成红**，且没有机械通道自愈。
C4 是第一个案例。可选方向：基线改为"随改动一起评审的可写文件"，或给棘轮加"分母变化时
允许重算"的显式入口（需设计，不能放开 `--update`）。

### 4.3 大盘无绿

近 12 次 `ci.yml` **没有一次 `success`**。这使"某个 job 红了"在本仓已失去信息量 ——
它正是"存量红"这个标签的上游成因：红得太久，人就不再看是哪一条。

### 4.4 门禁拆 job / 汇总

单个 job 内 20 多个检查串行，前面一条红会让后面全部 `skipped`（`CI_LANE_BLINDNESS_2026-10-27.md`
的 `Repo Sanity` 就是实例：22 个门禁因第一条恒红而全部没跑）。拆 job 或加 `if: always()`
汇总属独立决策，**未做**。

### 4.5 `always()` 步骤依赖前置产出

`Upload outdated-deps report` 报 `No files were found`：上游 `Supply-chain gate` 先失败 ⇒
产出文件的步骤被 `skipped`，而 upload 是 `if: always()`。B1/B2 转绿后此条自然消失，
但机制上仍是"`always()` 步骤假设前置产出一定存在"的脆性。

### 4.6 `Assert no leftover test databases` 的次生红

日志大量 `ERROR: out of shared memory` ⇒ janitor 来不及回收。C5 修后应缓解，
但**尚未在新一轮 CI 上验证**。

### 4.7 `Benchmark` 的 runner 回收（同 B4）

未处理。该 job 没有 `timeout-minutes`，无法区分"被环境杀"与"自己卡死"。

### 4.8 `ROUTE_CONTRACT.md` 的跨午夜日期漂移（设计缺陷）

头部日期按生成当天写入，跨午夜必然产生 1 行差异。它不是无害噪音：留着它会让此后**每一次**
提交都命中契约 gate 的 `SKIP`（= 整条契约链根本没被校验），而丢弃无效（下次运行又生成）。
当前处置是"把那一行提交掉"，属绕行，不是修复。

### 4.9 行数棘轮与 worktree 隔离互斥

`check_file_size_ratchet.py` 的 `EXCLUDED` 硬编码 `/.worktrees/`，扫到零文件即 fail-closed
⇒ **`/.worktrees/` 里永远提交不了**，与"用 worktree 做隔离改造"的做法直接冲突。

### 4.10 判据仍建立在日志文本上

`require_tests_ran.sh` 本轮只修了单复数，**机制未改**：判据仍是"日志里必须出现某个正向计数行"，
一旦 nextest 换文案或加 `--no-tests=pass` 就再假红（已发生三次）。收紧方向是优先用工具自己的
退出码（`cargo nextest --no-tests=fail`），正则兜底只保留"宽进严出"的最小集合 —— 改机制会动
CI 语义，需另立决策。A8 是同一根因的另一面：判据的输入是网络可达性，于是断网就被报成漏洞。

> **已处置（2026-10-28 第二轮，提交 `21881328f`）**：机制已改，见 §6.10。当时的顾虑
> "会动 CI 语义"经核实不成立 —— `--no-tests=fail` 与 `--list` 预检都**只在空集时改变结论**，
> 对"真的跑了测试"的路径零影响（6 例守卫 + 2 次变异自证）。

## 5 一条可复用的判据

排查"门禁红"时，先给这条红归类，再决定动手：

1. **判据自己造的** ⇒ 修判据，不要改被守的代码（A 类）。
2. **判据根本没跑** ⇒ 先修触发条件（B 类）。`skipped` 永远不是绿。
3. **判据是对的** ⇒ 才轮到改代码（C 类）。

跳步的代价本轮全部见过：把 A1 当成"测试真没跑"会去补测试；把 C4 当成"覆盖率真降了"
会去 `--update` 降地板，把一次合法的死代码删除永久记成欠账。

## 6 §4 十条的逐条处置（2026-10-28 第二轮）

处置原则与 §5 一致：**先判这条红属于哪类，再决定动哪里**。因此下文有的条目是"修代码"，
有的是"改判据"，还有两条是**推翻原结论**（原诊断不成立或早已修好）。

| # | §4 原诊断 | 结论 | 证据 / 落地 |
|---|---|---|---|
| 4.1 | 覆盖率 134 文件 + 6 条 core-path 未逐条定性 | **机制已建，2026-10-29 已出实测分档**（`LOST-HITS 3 / DROP 101 / DENOM 77` + 4 类非退化） | 见 §6.1 |
| 4.2 | 棘轮 `max(prev, cur)` 把合法重构固化成永久红 | **已修** | 见 §6.2 |
| 4.3 | 近 12 次 `ci.yml` 零 success，"某 job 失败"失去信息量 | **已收敛为 4 类 + 汇总表可归因** | 见 §6.3 |
| 4.4 | 门禁拆 job / `if: always()` 汇总未做 | **已做（轻量版）** | 见 §6.4 |
| 4.5 | `always()` 步骤依赖前置产出（`No files`） | **原诊断有误 + 仍做了精确修复** | 见 §6.5 |
| 4.6 | 残留库断言在 `out of shared memory` 后次生红 | **已修（归因分离）** | 见 §6.6 |
| 4.7 | `Benchmark` runner 被回收（exit 143） | **已加超时与归因指纹** | 见 §6.7 |
| 4.8 | `ROUTE_CONTRACT.md` 日期漂移让契约 gate 粘性 SKIP | **已修（判据改结构性）** | 见 §6.8 |
| 4.9 | 行数棘轮 `EXCLUDED` 与 worktree 隔离互斥 | **原诊断已过期（10-23 就修了）** | 见 §6.9 |
| 4.10 | 判据仍建在日志文本上 | **已修（机制判据）** | 见 §6.10 |

### 6.1 覆盖率差异的逐条定性：先建机制，再读数

"134 个文件"这个数字本身不可判 —— 它把三种完全不同的东西混在一起：真的退步、
"这次没编译它"、以及"文件已经不存在了"。2026-10-27 那次最典型：`run_coverage.sh`
的 storage 腿不带 `friends,widgets,burn-after-read`，于是三个 1000+ 行的文件**根本不参与编译**，
报告里恒为 0.0%；补上 feature 后读数从 74.59% 跳到 19.4%，那是"终于被测了"，不是退化。

因此新增 `scripts/ci/classify_coverage_drift.py`，把差异按**成因**分类（复用棘轮自身的
解析与归一化实现，口径只有一份）：

- `DROP` —— 有实测值且低于基线。**这才是要看的那一类**。
- `GONE-NOT-BUILT` —— 源文件在磁盘上，但本次报告里没有它 ⇒ 这次没被编译进测试。
- `ZEROED` —— 报告里有、但是 0.0% 而基线 > 0 ⇒ 这次没有测试执行到它。
- `GONE-SOURCE` —— 源文件已不在磁盘上（删除/重命名）⇒ 非回归。
- `NEW-FILE` / `RISE` / `EQUAL`。

工具本身只报告、不判红（判红仍归 `check_file_coverage.py`）；它的价值是把
"棘轮落红摘要回答不了的为什么"变成可重跑的表格。合成数据四类均验证过。

**工具自己的口径 bug（已修，教训与"口径只能有一份"同源）**：初版复用了解析与路径归一化，
却漏掉了**比较精度**——棘轮在比较前把实测值 `round(cur, 2)`（否则 `2/3`、`1/3`、`1/7` 这类
无法用两位小数精确表示的文件会永远 `cur < prev`，见 `check_file_coverage.py` 的注释），
初版没跟。在 2026-09-22 的旧 lcov 上实测，同一份数据的两种口径差得离谱：

| 口径 | DROP | RISE | EQUAL |
|---|---|---|---|
| 未取整（初版） | **216** | 229 | 150 |
| 取整到存储精度 + 跳过 `is_test_only`（现在） | **21** | 20 | 516 |

216 → 21 不是数据变了，是**分母口径**变了。顺带把显示从 1 位改成 2 位：
`1.25% → 1.30%` 原先印成 `1.3% → 1.3%`，读者第一反应是工具坏了。

**不需要 lcov 就能看到的静态事实**（2026-10-28 直接从基线算）：

- 基线 608 条，其中 **219 条**落在 `core_file_coverage_prefixes.txt` 的 28 个前缀下；
- 这 219 条里有 **98 条低于 core 门槛 70%**，包括 4 条 **0.00%**
  （`synapse-storage/event/models.rs`、`event/reader.rs`、`event/writer.rs`、`presence/api.rs`）。

这暴露的是比"134 条差异"更该先说清的一件事：**core-path 的 70% 门槛对存量无效**。
判定优先级是 `baseline-known`（只防回归）高于 `core`（绝对门槛），所以一条文件只要进过
基线，70% 就再也不会对它生效 —— 它能一路冻在 0.00%。这不是假红，是"门槛只对新文件有效"，
而 policy 的措辞是"核心路径 ≥ 70%"。真要收紧，得先决定那 98 条是补测试还是重新划 core 面，
**这不是一次 `--update` 能回答的**，所以本轮只记录事实，不动门槛。

**顺手挖出的根因链（比"134 条"重要得多）**：`Code Coverage / Run coverage` 每轮红，
**红的那一步与覆盖率无关**，而是「一条测试假红」+「一个把数据抹掉的机制」。

证据（`gh api …/jobs/<id>/logs` 全量落盘后逐行核对，`/tmp/cc.log` 第 12120–12175 行）：

1. **步骤 1（synapse-storage 单线程）是绿的**：
   `test result: ok. 1892 passed; 0 failed; … finished in 1843.90s`。
   —— 早期把它读成"storage 一片红"是**误判**，本轮已更正。
2. **步骤 2（`--workspace --exclude synapse-storage`）只红一条**：
   `test result: FAILED. 15 passed; 1 failed`，失败用例
   `synapse-test-utils::db_schema_pool_tests::the_orphan_guard_skips_schemas_parked_for_reuse`。
   它的 stdout 先打：

   ```text
   test schema guard: failed to drop orphan test_40507_14_…: error returned from database: out of shared memory
   ```

   然后 `panicked at synapse-test-utils/src/lib.rs:2869: the parked schema must survive the sweep`。
3. **机制缺陷（真正的放大器）**：`cargo-llvm-cov` 在测试失败时**不写报告**。
   本地用一个会失败的探针 crate 实测（2026-10-28）：

   | 调用 | 测试结果 | `out.lcov` |
   |---|---|---|
   | `cargo llvm-cov --lcov --output-path out.lcov` | 1 passed / 1 failed | **MISSING** |
   | `cargo llvm-cov --no-report` → `cargo llvm-cov report --lcov -o out.lcov` | 同上（rc=101） | 写出，1 个 `SF:` |

   于是"一条测试红"直接等于"整条车道的产出消失"：`Run coverage` 101 ⇒ `coverage/lcov.info`
   不存在 ⇒ `Per-file coverage ratchet` **无数据可比** ⇒ per-file 棘轮从未真正执行过。
   **这才是"134 个文件未逐条定性"的第一因**，而不是数据太多没人看。

**已落地的修法**（`scripts/ci/run_coverage.sh` + `.github/workflows/ci.yml`）：

- 每条腿拆成 `--no-report --no-fail-fast`（只跑测试，失败也把 profraw 落到磁盘）+
  `cargo llvm-cov report`（从 profraw 生成报告）。`--no-fail-fast` 保证一个 binary 红不让
  后面的 binary 根本不跑；**退出码语义不变**（有腿失败仍非零退出，红还是红）。
  ⚠️ 这两个旗标是 **cargo** 参数，必须落在 `--` 之前 —— 步骤 2 以 `-- --skip ledger_export_tests`
  结尾，追加到末尾会被 libtest 当未知参数，整条腿零产出。
- **⚠️ 拆开之后暴露的第二个缺陷（同一轮就抓到）**：`cargo llvm-cov report` **不接受
  `--workspace`**，缺省按"当前包"过滤。本仓根 crate 是 `synapse-rust`（只有 `src/` 与
  `scripts/`），于是第一版拆分的报告只有 **18 个文件**：

  ```text
  Merged 2 lcov files -> coverage/lcov.info
    files=18, LF=3173, LH=1157, coverage=36.46%
  ```

  627 → 18，棘轮只会拿这 18 个去比。修法是给每条腿显式传包范围（步骤 1 `-p synapse-storage`，
  步骤 2 用 `REST_REPORT_ARGS` = 工作区 9 个成员 − `synapse-storage`），空范围按失败处理，
  并加两道守卫（见下）。**这条缺陷以前不可能被看到**：旧实现一条腿都没跑完，报告从来没写出来过。
- `coverage` job 增加 `Classify coverage drift`（`if: always()`，只报告不判红）与
  `Upload coverage artifacts` 的 `if: always()` + `if-no-files-found: ignore`：
  红的时候才有数据可下载，而"红"正是唯一需要这份数据的时刻。
- 守卫：`tests/unit/coverage_leg_resilience_tests.rs`（7 例，假 cargo 断言 argv 与产物）。
  **变异自证四次**：删掉兜底 `report` 分支 ⇒ `a_failing_leg_still_writes_the_report` FAILED；
  把两个旗标改成追加到末尾 ⇒ `llvm_cov_flags_land_before_the_harness_separator` FAILED
  （报错信息里直接打印了被污染的 argv）；去掉步骤 1 的报告范围 ⇒
  `every_report_carries_an_explicit_package_scope` FAILED；从名单里删掉 `-p synapse-web` ⇒
  `rest_report_scope_matches_the_workspace` FAILED（拿 `cargo metadata` 的成员列表对账，
  防"加了 crate 忘了加进报告范围"这种静默少覆盖）。还原后 7/7 PASS。
- `run_coverage.sh` 现在会打印 `max_locks_per_transaction = N`，低于 128 时给出可执行修法
  （`bash scripts/tune_test_db.sh`）。它**不替人改**配置：CI 的 service container 无法传
  `-c max_locks_per_transaction`（ci.yml §14.13 ③ 已从源码层确认 runner 把 `-c` 当 `--cpu-shares`）。

**锁表这条根因的边界（别把它当成万能解释）**：`53200 out of shared memory` 在 CI 日志里
确实存在（同一次运行 13059/13062 行两条 server 端 ERROR），`scripts/cleanup_test_schemas.sh`
头部也早写了"单个 `DROP SCHEMA … CASCADE` 已接近 `max_locks_per_transaction=64` 上限"。
但它**解释不了本地的大片失败**：

| 条件 | `synapse-storage --lib`（本地） |
|---|---|
| `max_locks_per_transaction=64`（容器默认） | **718 passed / 1174 failed** |
| `tune_test_db.sh` 调到 256 后重跑同一命令 | **718 passed / 1174 failed**（数量完全相同） |
| 同一环境下 `-p synapse-storage --lib -- cas::db_tests` | **24 passed / 0 failed** |

三行合起来说明：本地这条腿的失败**不是**锁表造成的（调到 256 不变），也**不是**"连不上库"
（cas 的 24 条 DB 用例全过、`test_` schema 残留已清零、跑完仍是 0）。它更像某个模块级的
环境依赖，而**本地环境与 CI 不同款**（CI 同样的 1892 条是 1843.90 s 全绿，本地单条要约 2.7–19 s）。
所以：CI 的失败模式是「步骤 2 的单条 orphan guard 假红」，本地的是另一件事，
**两者不可互相佐证**。本轮不宣称本地那条已定位。

**orphan guard 的归因缺陷（已定位并修，`synapse-test-utils/src/lib.rs`）**：那条断言把三种情形
压成同一句 `the parked schema must survive the sweep` —— (i) 守卫删了 parked schema；
(ii) 并发进程删了；(iii) 它压根没建成。断言看不出是哪一种，于是"环境故障"被读成"守卫有 bug"。

本轮把 (i) 追到底了，结论与第一直觉相反：**schema 确实是被删的，但不是 sweep 删的**。
`reclaim_orphan_test_schemas` 对 parked 名是跳过的（`parked.contains` 在 lease 之前），
真正动手的是 **parked 行的 TTL 回收器**：

| 事实 | 值 |
|---|---|
| `DB_SCHEMA_POOL_TTL` | **120 s** |
| reaper 触发点 | 每次 pool 准备（`reap_expired_db_pool_entries`，lib.rs:470），**同 binary 的其它用例也会跑** |
| 该用例耗时 | CI 日志明确打了 `has been running for over 60 seconds`；按生产阈值要遍历 ~100 条遗留 schema、每条 `DROP … CASCADE` ~1 s |
| 回收谓词 | `parked_ts < now - ttl` ⇒ 一旦用例跑过 120 s，**自己停靠的那行就被回收，schema 一起被 DROP** |

于是"parked schema 必须存活"的探针在 120 s 后必然拿不到它 —— 断言报警的对象与真凶无关。
**修法**：停靠时间戳取**未来**（`now + 1h`），让回收谓词恒假；"跳过 parked"按**名字**判断、
不看时间，所以 orphan guard 的语义不受影响。这是"判据归因"同一主题的第三个实例：
不是把断言写得更硬，而是让**被断言的状态**不再被无关机制改掉。

> 顺带更正：本文件早前写"步骤 2 的红是锁表耗尽 ⇒ DROP 失败 ⇒ 断言被打红"，那是把
> 同一段 stdout 里的两条独立事件（sweep 的 DROP 失败 / 探针拿不到 schema）当成因果。
> 两者无因果关系：DROP 失败只是**噪音**，探针失败的原因见上表。

> **2026-10-29 已做**：`classify_coverage_drift.py` 已跑在**完整** lcov 上，134 + 6 的最终名单
> 见 §4.1 末尾「实测分档」（`LOST-HITS 3 / DROP 101 / DENOM 77 / GONE-NOT-BUILT 3 /
> GONE-SOURCE 4 / NEW-FILE 1 / RISE 19 / EQUAL 434`，677 文件 vs 683 基线）。
> 原来卡住的「本地跑不通」已不是问题：`scripts/ci/run_coverage.sh` 跑完后
> `coverage/lcov.info` 是两条腿合并后的完整报告，直接喂给分类脚本即可（不传
> `--save-baseline`，避免写回 `scripts/ci/coverage_baseline.json`）。
>
> 下面这段是 2026-10-28 的 CI 读数记录，保留作为棘轮首跑的原始证据：
> run 40621674176 的 `Upload coverage artifacts` 在红的那一轮也产出了 `coverage-report` 工件。
> 同一轮还是**棘轮第一次真正执行**（历史从未跑过一次），结论值得记：
>
> ```text
> === Coverage failures (3 files) ===
>   [TOUCHED] src/bin/healthcheck.rs: 10.7% < 11% (was 10.9%, delta=-0.2%)
>   [TOUCHED] src/bin/synapse_ledger_export.rs: 58.7% < 59% (was 59.1%, delta=-0.4%)
>   [TOUCHED] src/bin/synapse_worker.rs: 34.3% < 38% (was 38.2%, delta=-3.9%)
> baseline scripts/ci/coverage_baseline.json: 18 个文件本次测量，594 个条目保留自上次
> ```
>
> 这条 `594 个条目保留自上次（未出现在本次报告里，不删）` 是**新的"看起来绿"风险**：
> 报告缺文件时棘轮不会红，只会静默少比 594 个条目 —— 而这正是 §6.1 那 18 个文件的处境。
> 下一轮（带报告范围修复）的 627 条读数才是有意义的那一份；在此之前**禁止** `--update`、
> 也**不**逐条改 `coverage_baseline.json`。

#### 6.1.1 第三次空手而归（run 40637601681，2026-10-28）：缺一条腿把整轮数据抹掉

同一句"跑了但没产出"已经连续三轮出现，每轮的主因都不同 —— 这轮两条腿各有各的死法：

| 对象 | 现象 | 成因 | 性质 |
|---|---|---|---|
| storage 腿 | 65 s 后 `报告生成失败（profraw 没写出来？）` | 下载依赖时镜像返回 `504 Gateway Time-out`（`arcstr 1.2.0`），cargo 重试一次仍失败 ⇒ 测试一条没跑 | 基础设施抖动 |
| rest 腿 | 67 min 后 `1 target failed: -p synapse-rust --test unit` | **真实回归**：`c43f005b5` 拆 `lib.rs` 让 D-36 豁免键漂移，`test_ddl_guard_tests` 两条同时红（已修） | 自己的改动 |
| 合并步 | `FileNotFoundError: '/tmp/…/storage.lcov'` traceback | 旧实现无条件 `merge_lcov.py storage.lcov rest.lcov` | **结构性缺陷**（已修） |

第三行才是这轮真正的教训：**`rest.lcov` 当时已经躺在磁盘上（覆盖 600+ 文件），却因为另一条腿
缺席而一行都没被用上**。上一轮把"没数据"归因到"报告范围只有 18 个文件"，上上轮归因到
"测试失败不写报告"—— 两处都修对了，但"合并步骤会自己把整轮抹掉"这条一直没人看。

修法分两层，职责分开：

1. **编排层（`run_coverage.sh`）降级合并**：只合并**存在**的腿，并把"本轮少了一条腿"写进日志；
   两条腿都空手时**不写空报告**（空的 `coverage/lcov.info` 会让棘轮把 608 个文件读成
   `GONE-NOT-BUILT`，比没有报告更难归因）。
2. **工具层（`merge_lcov.py`）fail-closed**：缺输入 exit 2 并点名文件，**不**静默丢弃 ——
   半份报告在产物层面与完整报告无法区分，静默合并等于把"少 3800 行"藏起来。

守卫 `tests/unit/coverage_leg_resilience_tests.rs` 加了 3 例（缺一条腿仍写出另一条的数据 /
两条腿全缺则失败且不写空报告 / `merge_lcov.py` 拒绝静默丢输入），7 → **10** 例全绿。

> **顺带记一条"假成功"**：那次 `Upload coverage artifacts` 是 **success**（`if-no-files-found:
> ignore`），看上去"工件已上传"，但 `gh api …/artifacts` 里只有 `openapi-*` 两个 ——
> `coverage-report` 根本不存在。**上传步骤的成功不等于工件存在**，判断只能查工件列表
> （与 §4.5 的 `always()` 教训同族：`always()` 能让步骤跑到，却证明不了它拿到了东西）。

### 6.2 棘轮语义：`max` 保留，但给合法重构一条显式出口

`max(prev, cur)` 本身**不能**去掉：改成 `cur`，"先降级、基线跟着降"就成了绕过路径。
但它确实把合法重构堵死了 —— 把已覆盖逻辑抽到新文件后，原文件剩下的多是不易覆盖的分支，
覆盖率下降而整体没变差；原文件 floor 仍是历史最高值 ⇒ 永久红，`--save-baseline` 因 `max`
也救不回，唯一出路是手改 54 KB 的 JSON（在 review 里与"偷偷放宽门禁"无法区分）。

新增 `--allow-lower PATH`（可重复）：只有**逐条点名**的文件才允许把 floor 落到本次实测值，
其余一律继续 `max`；默认空集 ⇒ CI 路径零变化。三个细节：

1. 下调必须**打印**（`=== Floors lowered on purpose ===`）—— 静默放行与"门禁没在工作"无法区分。
2. 点名基线里不存在的路径 **fail-closed**（exit 2）—— 打错字不能是静默空操作。
3. 报错直接给出 normalize 之后的候选 key。**这是真实陷阱**：基线 key 是
   `crate/<path under src/>`（**不含** `src/` 段），与 lcov 的 `SF:` 写法不同，
   拿 lcov 写法去点名必然匹配不上。

守卫 `tests/unit/coverage_baseline_lowering_tests.rs`（4 例）：默认仍红且 floor 不降、
点名后绿且写回实测值并打印、不扩散到未点名的文件、打错路径报错。

### 6.3 大盘无绿：先把它收敛成"4 类"，再逐类处置

"近 12 次零 success"不是一条可修的缺陷，它是一个**汇总症状**。逐次抓失败 job 与失败 step 后，
真正反复红过的只有 4 类（其余如 `Repo Sanity / SQLx dynamic-static ratio`、
`Test & Lint / Run unit test target`、`Check memory budget` 只出现在 10-27 更早的两轮里，
在最新两轮已通过）：

| 失败 step | 出现轮次 | 处置 |
|---|---|---|
| `Code Coverage / Run coverage` | 每轮 | 见 §6.1（机制已建，数据待出） |
| `Integration Tests / Appservice statistics cost gate` | 每轮 | 已修：A1 单复数 + §6.10 机制化 |
| `Security Audit / Supply-chain gate` | 每轮 | `deny.toml` 加 crate 级 `BSL-1.0` 例外（`xxhash-rust`），待 CI 复核 |
| `Assert no leftover test databases` | 3 轮（次生） | 见 §6.6 |

同时给 `ci-summary` 补上了"红在哪一步"，见 §6.4。

> **未闭环的判据**：`Supply-chain gate` 的修复只做了本地静态核对，尚未在 CI 上跑过一次；
> CI 队列当前有 9 个 run 排队、8 个在跑，主行程排不进去（这本身是 §4.3 的另一个成因）。

#### 6.3.1 再收敛一次：最新 6 个已完成的 run 只剩 **2 类**（2026-10-28 第二轮）

"12 连红"读起来像 12 种病。把 6 个已完成的 run 逐条拆到 **失败 step** 粒度后，
只剩两条常红 + 一条一次性：

| 失败 step | 命中轮次 | 本轮处置 |
|---|---|---|
| `Code Coverage / Per-file coverage ratchet (B.3)` | **6/6** | §6.1（报告范围 + 不写报告的兜底）+ §6.2（`--allow-lower`） |
| `Integration Tests (database) / Assert no leftover test databases` | **5/6** | §6.6（`Done` → `Retry`） |
| `Test & Lint (1.93.0, all-features) / Run library unit tests` | 1/6 | 一次性：见 run 40603035279 |

两点附带结论：

1. **红的"位置"已经换过一次了**。上一轮 `Code Coverage` 红在 `Run coverage`（跑不起来），
   本轮 6/6 都红在 `Per-file coverage ratchet` —— 说明 §6.1 的 `--no-report` + 显式包范围
   生效了，红的位置从"没有数据"变成"数据有了、比基线低"。这两个红**不是同一件事**，
   混在一起看会得出"覆盖率一直红，改了没用"的错误结论。
2. `Appservice statistics cost gate` 与 `Supply-chain gate` 在最新 6 轮**没有出现过**，
   与 §6.3 表格里"每轮"的说法不再一致（那两行是 10-27 的数据）。这也是"汇总症状"
   最典型的坑：**描述大盘的表格一旦不标注采样窗口，就会自己变成假消息**。

> **仍未闭环**：这 2 类是否真的清掉，要看带本轮修复（`93381ef46`…`c43f005b5`）的
> run 完整跑完一轮 —— 在此之前只能说"机制改了"，不能说"大盘绿了"。

#### 6.3.2 顺手抓到第三条：`Check memory budget` 把"拆文件"当成 OOM 风险

排查 `Test & Lint (…, default-features)` 两条车道同时红（run 40636551130）时发现红在
`Check memory budget`，而那次提交只是**把测试从一个文件搬到另一个文件**。

原因在 `count_new_test_files()`：旧判据是「改动的 `.rs` 里出现 `#[test]` 就算一个新增
测试二进制」，再按 `TEST_BINARY_RSS_MB`（~200 MB）计入预算。于是：

| 改动 | 真的多出二进制？ | 旧判据 |
|---|---|---|
| 已有测试模块里**加一个用例** | 否 | +1 个 |
| 把测试**搬到另一个文件** | 否 | **+2 个**（新文件 + 旧文件都算） |

实测那次拆分：旧判据算出 **2 个 ⇒ +400 MB**（越线），新判据 **0 个**。要命的是这条门禁
挡的恰好是 `check_file_size_ratchet.py` 要求的动作 —— 一个"防 OOM"的门禁让人不敢拆文件，
最终反而更难控内存。这也解释了它为什么"时红时绿"：取决于那一次提交碰了几个含测试的文件。

新判据只认**新增**且**直接位于 `tests/` 或 `benches/` 下**的 `.rs`——那才是 cargo 真的会
建 target 的形态（`tests/unit/foo.rs` 是被 `tests/unit/mod.rs` 用 `mod` 吸纳的，不单独成
二进制）。`[[test]]` 显式声明的情形看不到，属已知**漏报**方向：宁可漏报也不误报，
否则每一次重构都要为它写豁免。

三例谓词自证（`tests/probe.rs` ⇒ 1；`tests/unit/probe_tests.rs` ⇒ 0；
`src/probe_tests.rs` ⇒ 0）+ 一次真实提交的新旧对照（2 vs 0）。

### 6.4 汇总：从"job 名列表"到"逐门禁结论表"

`ci-summary` 本来就有，但只列失败 job 的**名字**，而它的注释写着
"one representative error line" —— 注释与实现不符。同一个 job 里往往有 5 个门禁，
"Security Audit 失败"与"Supply-chain gate 失败"是两件事，光有 job 名没有信息量。

改为逐门禁表：`Job | Conclusion | Failing step`，并把 `skipped` 单列（**skip 不等于通过**），
`needs` 补上长期缺席的 `feature-matrix` 与 `openapi-artifact`（它们红了原本会显示成一个
不属于任何门禁的孤立红点）。

至于"拆 job"：本轮没拆，而是用更小的代价解决了它的**实际后果** —— 见 `Security Audit`
的 4 条独立门禁改 `if: ${{ !cancelled() }}`。实测 run 40513351117：`Supply-chain gate` 一红，
`rand::rng()` 棘轮 / `Install cargo-geiger` / `Run cargo-geiger` / `cargo-outdated`
**全部 skipped**，一个门禁失败把另外三个一起遮蔽。改后任一门禁失败仍让 job 变红
（step 级 failure 就是 job failure），不需要额外聚合步骤。

#### 6.4.1 汇总里的哨兵曾经把一条车道短路掉（2026-10-28 第二轮修正）

逐门禁表回答"谁红了"，但"这条车道到底跑没跑"是另一个问题 —— 由同一 job 里的
`Sentinel — required lanes actually ran` 负责：它读 `needs.<job>.result`，逐个
`require_lane "<显示名>" "$X_RESULT"`。这一层有一个**只有读 YAML 才看得见**的错：

```
case "$INTEGRATION_RESULT" in
  success) require_lane "Code Coverage" "$COVERAGE_RESULT" ;;
  *) echo "info: Code Coverage depends on Integration Tests ('$INTEGRATION_RESULT'); not requiring it" ;;
esac
```

依据是一条注释写着"`Code Coverage` needs: integration-test"。但 `coverage.needs`
早就只剩 `['changes']`（`gh api` / `yaml.safe_load` 都能一眼确认）—— **coverage 并不
依赖 integration-test**，integration 失败时它照常执行。真实的后果是反的：

| integration-test | coverage 实际行为 | 旧哨兵 | 应然 |
|---|---|---|---|
| success | 跑 | 要求它 success | 同 |
| **failure** | **照常跑** | 打印 `not requiring it` 放行 | 要求它 success |
| failure | 被 skip / cancelled | **放行，无人报** | 报"该跑的没跑" |

也就是说"integration 一红"恰好让**唯一会检查 coverage 的人闭眼** —— 与 §6.4 里
`Security Audit` 那次"一个失败遮蔽另外三个"是同一个形态，只是这次是**判据**遮蔽，
不是 step 遮蔽。

修法两面：

1. `require_lane "Code Coverage" "$COVERAGE_RESULT"` **无条件**放进
   `need_slow_tier = 1` 分支（`coverage.if` 与 `need_slow_tier` 的条件本就是同一组）；
2. 新增 `scripts/ci/check_ci_summary_dependencies.py`，把"注释里声明的依赖"换成
   **解析 `ci.yml` 得到的真实 `needs` 图**：哨兵读的每个 `needs.X.result` 必须在
   `ci-summary.needs` 里；任何 `case "$<VAR>" in` 门控内出现的 `require_lane`，其
   目标 job 必须真的 `needs` 门控变量对应的 job；`require_lane` 的显示名必须能对上
   真实 job 的 `name:`（矩阵 job 按 `name (` 前缀匹配）。

自证分两层：`--mutation-check` 在内存里注入"把 coverage 包回 `case "$INTEGRATION_RESULT"`"
与"把 `needs.test.result` 改成不存在的 job"，各报一次红；更硬的一层是拿 **修复前** 的
`ci.yml`（`git show HEAD:.github/workflows/ci.yml`）直接跑 —— 只报 1 条，且正是上表
那一行。接在 `repo-sanity`（`check_workflow_steps.py` 之后）与 `preflight.sh`（~30 ms）。

> 这一条的方法论意义大于它自己：`needs` 是 GitHub 的唯一真相，**注释是第三方的说法**。
> 把判据写在注释里，等于让"注释与 YAML 谁新"去决定门禁是否生效 —— 而这两者从来不会
> 一起更新。

### 6.5 `always()` 依赖前置产出：原诊断要改判

原文说"`Upload outdated-deps report` 报 `No files`" ⇒ 让人以为步骤失败。查真实日志（run
40513351117）后改判：`actions/upload-artifact@v4` 的 `if-no-files-found` 默认值就是 `warn`，
**不失败**，日志是 `##[warning]No files were found with the provided path: …`。

但它确实暴露了一个更值得修的东西：那两个 upload 是**裸 `always()`**，于是上游被 skip 时
它们照样运行、照样"成功"—— "报告存在"变成假象。实测该 job 里 `Install cargo-geiger` 与
`Check outdated dependencies` 都是 `skipped`，而两个 upload 是 `success`。

改为 `if: always() && steps.<id>.outcome != 'skipped'` + `if-no-files-found: ignore`：
前置没跑就不假装上传，前置跑了但确实没产物则不因此叠一次红。

### 6.6 残留库断言：把"查不到"和"有残留"分开

旧实现是 `left="$(psql -tAc ...)"` + `[ "$left" -ne 0 ]`。服务器在 `out of shared memory`
之后连不上时 `left` 是空串，比较报 `integer expression expected` —— **服务器不可用**被显示成
一条语法错误，读者会当成"janitor 漏了库"，而真实主因在上一屏。次生噪音掩盖主因。

新增 `scripts/ci/assert_no_leftover_test_databases.sh`，三层结论各自有错误标题：

- `Leftover test databases` —— 真有残留（列出库名）；
- `Cannot determine leftover test databases` —— 服务器不可达 / 查询异常，结论 UNKNOWN
  （明说"既不是 leftover 也不是 clean"，并提示去修上一屏的 `out of shared memory`）；
- 无输出 —— 干净。

连接层带重试（`out of shared memory` 往往是瞬时的：测试进程刚退出、连接还没回收），
计数严格校验为整数。本地三场景实测：干净 ⇒ 0；造一个 `test_leftover_probe` ⇒ rc=1 +
`title=Leftover`；错误端口 ⇒ rc=1 + `title=Cannot determine`。

> **⚠️ 归因分离之后的第一条真实结论（run 40607555992 · `Integration Tests (database)`）**：
> 那次失败**不是次生噪音，是真残留** —— `##[error]46 leftover test database(s)`，46 个
> `test_<pid>_1_<nanos>` 库名跨度 02:19–02:41，同一次 job 的服务端日志还有成片
> `ERROR: out of shared memory`（02:57）。
>
> 库级守卫**是有的**（`synapse-common/src/test_database_guard.rs`，
> `DROP DATABASE … WITH (FORCE)` + "还被占用就 Retry"），所以泄漏不是因为"没人管库"。
> 真正的问题在它的**放弃语义**与 CI 的一次性环境相撞：
>
> | 情形 | 守卫返回 | 注释里的理由 | 在一次性 CI 库上的后果 |
> |---|---|---|---|
> | 库还被占用 | `Retry` | "janitor 下一轮再来" | **没有下一轮**（服务容器随 job 销毁） |
> | DDL 失败（如 `53200`） | `Done` + stderr | "下一轮的 sweep 还能回收" | 同上，永远回收不了 ⇒ 断言必然红 |
>
> 所以这条红是"环境压力 + 一次性环境"的乘积，而不是断言写错了 —— 断言正确地指出
> "这一轮结束时库里确实有 46 个"。
>
> **2026-10-28 第二轮：选了 (a) 的一个变体，代价最小且可自证。** 关键发现是 `Done` 不只是
> "放弃重试"，它还**把条目从 janitor 的注册表里摘掉** —— 于是进程退出时那次
> **无条件** `WITH (FORCE)` 的兜底 drop（`drop_database_blocking`，即 `on_exit`）根本看不到
> 它。也就是说 `Done` 是"既不在跑内重试、也放弃了退出兜底"，这比注释里写的
> "下一轮的 sweep 还能回收"严重得多（一次性 runner 上根本没有下一轮）。
>
> 两个失败分支（维护连接失败 / `DROP` 本身报错）改回 `Retry`：`run_release_cleanups_requeueing`
> 会以 `LEASE_RETRY_BACKOFF` 重新登记，**不是**注释担心的"hot loop"；而退出时 drain 走的
> `on_exit` 是 `WITH (FORCE)`，正好是 `out of shared memory` 之后最可能成功的那一次。
>
> 自证（不 mock sqlx）：`template0` 是 PostgreSQL **按定义拒绝**删除的库名
> （`cannot drop a template database`），拿它把 DDL 失败分支真的走一遍 ——
> 新用例 `a_failed_drop_reports_retry_so_the_exit_drain_gets_another_shot`
> 0.10 s PASS；把那一行改回 `Done` ⇒ 失败信息精确为
> `left: Done / right: Retry`（变异自证），还原后 `-p synapse-test-utils --lib` 17/17 PASS。
> 仍未做：job 末尾的强制 sweep（(c)）—— 若下一轮 CI 仍见残留库，再上它。

### 6.7 Benchmark：exit 143 的两种可能必须留下不同指纹

run 40603416972 的 `Run benchmarks` 只跑了 11 min 就收到
`The runner has received a shutdown signal` 并 exit 143 —— 平台回收，不是 benchmark 回归。
但此前 6 个 job **都没有 `timeout-minutes`**（用的是平台默认 360 min），于是读者无法区分
"跑太久""runner 被回收""benchmark 崩了"。

按实测正常耗时（`Run benchmarks` ~55 min、两个 perf gate ~22 min）×~1.35 设为
75 / 40 / 40 / 40 / 40 / 15。设了之后两种情形的日志指纹不同：
超时是 `has exceeded the maximum execution time of N minutes`，平台回收是
`received a shutdown signal` + exit 143。

> 说明：这不能"修掉"平台回收（外部条件），只能让它**可归因**。

### 6.8 契约 gate 的粘性 SKIP：判据从"路径出现"改成"差异是结构性的"

`ROUTE_CONTRACT.md` 第一行是「自动生成于 <date>」，每天都变。preflight 的 `changed` 来自
`git diff --name-only`，于是工作区里躺着一份**仅日期不同**的 ROUTE_CONTRACT.md 就会被算成
"本次改动了契约链"，gate 随即 SKIP。

关键在于这个 SKIP 是**粘性的**：丢弃那份改动没用（下一次任何生成动作又写回来），只要它还在
工作区，之后**每一次**提交都命中 SKIP —— 契约链从此再没被校验过，而 SKIP 在输出里看起来
像通过。

改为 `route_contract_structurally_changed()`：把日期行剥掉后与 `HEAD:<doc>` 比较
（与 `check_route_contract.sh` 自身的 normalize **同口径**），只日期不同就当它没被改动、
照常跑 gate。三场景实测（直接调用 preflight 里的同一份函数，非复制）：

| 场景 | `git diff` | 判定 | gate 行为 |
|---|---|---|---|
| 无改动 | 空 | 非结构性 | 跑 |
| 仅日期行 2026-10-28→10-29 | 1 行 | **非结构性** | **跑**（原实现会 SKIP） |
| 日期 + 追加一行内容 | 2 行 | 结构性 | SKIP（提交后复核） |

### 6.9 行数棘轮与 worktree：原诊断已过期

§4.9 说 `EXCLUDED` 硬编码 `/.worktrees/` ⇒ 在 worktree 里扫零、fail-closed、提交不了。
**该问题已于 2026-10-23 修好**（`check_file_size_ratchet.py` 改为用 repo-relative 的
`rel_dir` 匹配，注释里记了当时的实测）。本轮实测复核：

- 在 `.worktrees/rt-probe`（`git worktree add --detach HEAD`）里跑当前实现：
  `827 files scanned, 222 over 600 (baseline 222)` / `OK` / `rc=0`；
- 把匹配改回绝对路径 `dirpath`（变异自证）：`FAIL: scanned zero .rs files` / `rc=2`。

两者对照说明修复真实有效。清单里这条应当删除，而不是继续当作待办。

### 6.10 判据机制化：问框架，不问文案

`require_tests_ran.sh` 重写。旧实现用正则匹配人类可读的进度行，已连续假红三次，
每次只是补正则：ANSI 颜色（2026-09-20）、`strip_ansi | grep -q` 触发 SIGPIPE
（2026-09-30）、nextest 单数 `Starting 1 test`（2026-10-27，`Appservice statistics cost gate`
自引入起从未绿过）。把门禁建在别人的排版上，就得跟着别人的改动跑。

现在：

- **nextest**：注入 `--no-tests=fail`，空集由框架返回非零（实测 rc=4，`error: no tests to run`）。
  调用方若自己传 `--no-tests=pass`，直接 exit 2 —— 那等于关掉本门禁。
- **libtest**（无等价旗标）：先跑 `--list` 数注册表行（`^<name>: test$`）。用例清单是结构化的，
  不受颜色/单复数/措辞影响；空集时**连真正的测试都不跑**（对 DB 车道是省掉一次 schema 克隆）。
  `--exact` 会一并带到探测里，否则"过滤串写短了"会被子串匹配掩盖。
- 非 cargo 命令：退化为日志兜底并打 `::warning::`，明说这是弱判据。

守卫 `tests/unit/require_tests_ran_mechanism_tests.rs`（6 例，用假 cargo，不嵌套真 cargo
以免抢 target 锁）。两次变异各证一次红：去掉 `--no-tests=fail` 注入 ⇒ 对应用例 FAILED；
让空集不再判红 ⇒ 对应用例 FAILED。

## 7 两条「刻意不设门槛」的决定（2026-10-29）

§4.1（覆盖率）与 §4.6（残留测试库）各自都指向同一个念头：**再装一道门禁**。
本轮明确否掉，理由不是"懒得做"，而是这两类指标**装上就会常态红**，而常态红等于没有门禁
（铁律 8 的反面）。记在这里，是为了下一次有人提议时不用重新论证。

### 7.1 46 个残留测试库：**不设门槛**，清理改为按需

**为什么不做门禁**：残留库是"上一次跑崩了的**症状**"，不是缺陷本身。产它的原因有三类，
而且全部已经在别处判过：① janitor 抢不到资源（`ERROR: out of shared memory`）；
② 进程被 `timeout` / runner 回收（exit 143）；③ 测试库模板指纹变更导致旧模板成了孤儿。
按"残留数量 > 0 就红"设门禁，等于把 ①②③ 的偶发抖动直接变成主干红 —— 而 ① 的**真实判据**
已经在 §6.6 做过归因分离（把"库是被本次运行留下的"与"库是历史上失败留下的"分开），
那才是能指向修法的那一条。

**替代做法（已定）**：
- 残留库的**回收**交给 `synapse-test-utils` 的 janitor（每次跑前按 `datallowconn`/会话数判定后
  `DROP DATABASE ... WITH (FORCE)`），它是幂等的，不需要人盯。
- 需要人工清一次时，走一次性脚本并**先打印清单**（库名 + 创建时间 + 是否仍被连接），确认后再删 ——
  与 `cleanup_schema_script_tests::cleanup_script_covers_every_leaked_schema_family`
  钉住的"家族齐全"口径一致。
- **不做**的事：不把残留数量写进任何棘轮，不做 CI 断言。

### 7.2 core-path 里 79 个 < 70%：**不设门槛**

**先纠正一个口径**：core 70% 门槛目前**只对新文件生效**，基线已知的老 core 文件不受它约束。
§6.1 的读数（219 个基线 core 文件里 79 个 < 70%，含 4 条 0.00%）是**存量快照**，
不是"违规清单"。

**为什么不把门槛铺到存量**：那 79 个里绝大多数是 §4.1/§6.1 已经定过性的三类**口径问题** ——
① 整文件 `#[cfg(feature = ...)]` 门控（默认车道根本不编译它 ⇒ 恒 0%）；
② 声明文件 / 纯 `impl From<…>` 转发层（有函数体但没有可测分支）；
③ 跨 crate 转发层（只在这一侧看是 0%，命中记在另一侧）。
对这 79 条一刀切贴 70%，会精确重演 2026-10 那次"188 文件红"的教训：**红的原因不是覆盖低，
而是分母算错**。要降它只能先逐条改口径（已在 §6.1 建了分类器），而不是加一个数字门槛。

**替代做法（已定）**：
- 只有**新文件**受 70% 约束（现状不变），存量靠
  `scripts/ci/coverage_raise_queue.json` 的**逐文件台账**（§3.1.3）按需提升：
  一条一条登记 → 达成即删 → 账本不留"永久地板"。
- core 前缀清单继续只用于**标注风险面**（`--core-files` 的输出），不用于判红。

**两条决定的共同前提**：都不改动现有门禁，因此没有"闸门被放宽"这回事 ——
现状（新文件 70% + 残留库归因分离）保持原样，只是**明确不再加第二道**。
