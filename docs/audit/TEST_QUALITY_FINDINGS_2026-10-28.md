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

共性：A1 与 A2/A3 是**同一个脚本第三次**"判据写死单一合法输出形态"。根因是判据建立在
**日志文本**上，而不是工具自己的退出码 —— 见 §5 待办 6。

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

其余条目**尚无逐条定性**，**仍然禁止 `--update`** —— 按当前测量重写基线等于把未定性的值
永久固化成新地板。6 条 core-path 全部是 `was no baseline`：

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
CI 语义，需另立决策。

## 5 一条可复用的判据

排查"门禁红"时，先给这条红归类，再决定动手：

1. **判据自己造的** ⇒ 修判据，不要改被守的代码（A 类）。
2. **判据根本没跑** ⇒ 先修触发条件（B 类）。`skipped` 永远不是绿。
3. **判据是对的** ⇒ 才轮到改代码（C 类）。

跳步的代价本轮全部见过：把 A1 当成"测试真没跑"会去补测试；把 C4 当成"覆盖率真降了"
会去 `--update` 降地板，把一次合法的死代码删除永久记成欠账。
