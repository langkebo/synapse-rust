# 测试降标审计：被压缩 / 跳过 / 降低标准的测试（2026-10-25）

> **审计问题**：是否存在"为给产品缺陷让路或赶进度，测试环节被压缩、跳过或降低标准"。
> **审计口径**：只认**能复现**的判据 —— 每条结论给出 `文件:行` 与实跑命令；不能复现的一律进 §5（已排除）。
> **与前几轮的关系**：C-7 那轮已确认过一次"测试替产品缺陷让路"的**已发生实例**（`api_widget_tests`
> 的 PL helper 为绕开 403 而只写非创建者，产品缺陷直到 `7d58ac648` 才修）。本文件回答的是
> **现在还剩哪些同类形态**，以及哪些验收条件被**制度性**地降低了。

## 0 结论

有，而且集中在四处。最严重的一条不是"少写了测试"，而是**测试基础设施自己 fail-open** ——
在数据库不可达时，一批**安全边界用例会静默报 PASS，一条断言都不跑**，且**耗时比真跑更长**
（30 s vs 6 s），所以"跑得快 = 没跑"这个常用判据在这里是反的。

| 等级 | 条目 | 受影响功能 | 状态 |
|---|---|---|---|
| **R1** | `setup_fresh_test_app_with_config` fail-open ⇒ 9 条安全边界用例可静默假绿 | 语音上传体积上限（含 CVE-2026-102592 缓解）、媒体 2 MiB 边界、worker surface gate 窄化授权 | **✅ 已修复**（入口改 fail-closed + 9 处换 `skip_or_fail_without_db`，双向验证见 §2.4） |
| **R2** | 覆盖率门禁不在 PR 车道；新文件达标线 30%；已入库文件无绝对下限 | 全仓覆盖率回归（事后才发现） | 制度性降标 |
| **R2** | 性能验收只打印不断言（`appservice_scheduler_perf_tests` 全 `#[ignore]`） | appservice 调度器吞吐 | **🟡 已补 1 条**：`get_statistics` 每行成本折算往返次数，进 CI（§3.2.1）；其余仍 ignore |
| **R2** | `cargo test --doc` 是编译门禁，不是执行覆盖（4 个 doc test 全 `#[ignore]`） | 文档示例 | 已知并登记 |
| **R3** | 572 处 `else { return; }` 静默跳过**残留模式**（当前恒 `Some`，但 R1 已证明该模式会复活） | 全 integration 面 | **✅ 已止损**：7 处 fail-open 夹具改 panic + 守卫禁新增（§4.1.1）；调用点本身保留为死代码 |
| **R3** | 测试替身与生产 crate 未做特性隔离（MAINT-01） | unit 层 mock 语义 | 已登记不修 |
| **R3** | 回归范围按"改了哪个文件"选，而非"影响哪些契约" | 跨路径契约（ARCH-02 等价用例曾静默腐烂） | **✅ 已建机制**：`test_impact_map` + 提交前提示（§4.3.1） |

**没有发现**的（同样重要，见 §5）：无裸 `#[ignore]`、无"形状测试冒充覆盖"、无生产代码
`cfg!(test)` 短路、by-design unsupported 有双向门禁、棘轮上调有逐条归因。

## 1 排查方法（可复现）

```bash
# ① 静默跳过 vs fail-closed 的分布
python3 - <<'PY'   # 见 §3.1，统计 let Some(..) else { return; }
PY

# ② 跳过用例是否有覆盖车道
python3 scripts/quality/analyze_skipped_tests.py

# ③ 形状测试是否冒充覆盖
python3 scripts/ci/check_shape_only_tests.py

# ④ 覆盖率门禁的触发条件与达标线
grep -n -A6 '^  coverage:' .github/workflows/ci.yml
grep -nE 'global-floor|new-file-floor|core-threshold' scripts/ci/run_coverage.sh scripts/check_file_coverage.py

# ⑤ 假绿复现（R1 的决定性实验，见 §2.1）
CI=1 TEST_DATABASE_URL='postgresql://synapse:x@127.0.0.1:59999/nope' \
  cargo nextest run --test integration --test-threads 1 \
  --features 'test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications,widgets' \
  -E 'test(test_voice_upload_accepts_body_above_the_axum_default_limit)'
```

## 2 R1：安全边界用例可静默假绿（活缺陷）

### 2.1 现象与实跑证据

`tests/integration/mod.rs:650` 的 `setup_fresh_test_app_with_config` 用
`prepare_shared_test_pool().await.ok()?` 把基础设施故障折叠成 `None`，而它的调用点全部是
`let Some(..) = ... else { return; }` ⇒ libtest 记为 `ok`。

同一台机器、同一个不可达 URL，两种 setup 入口给出相反结论：

| 用例 | setup 入口 | 耗时 | 结果 |
|---|---|---|---|
| `voice_routes_tests::test_voice_upload_accepts_body_above_the_axum_default_limit` | `with_config`（`.ok()?`） | **30.036 s** | **PASS**（0 断言） |
| `api_friend_routes_tests::friend_endpoints_require_authentication` | `TestContext::new`（panic） | 30.439 s | **FAIL** + `[integration] No reachable test database` |

对照真库（`localhost:55432`）同 4 条：

```text
PASS [17.767s] test_voice_upload_accepts_body_above_the_axum_default_limit
PASS [ 6.024s] test_voice_upload_body_limit_is_content_type_case_insensitive
PASS [ 6.663s] test_voice_upload_rejects_body_above_the_configured_limit
PASS [ 4.148s] test_voice_upload_streamed_body_without_content_length_is_still_rejected
Summary: 7 tests run: 7 passed
```

⇒ 定性是**间歇性假绿**：正常有库时真跑真断言；DB 抖动 / 模板建池失败时**静默变绿**。
危险点在于**假绿比真跑还慢**（30 s vs 6 s），任何"耗时异常短"的巡检都发现不了它。

### 2.4 修复与双向自证（2026-10-25）

改动面只有 4 个文件共 +28/−1：

- `tests/integration/mod.rs`：入口先 `require_test_database().await`，再把 `.ok()?` 换成
  `Err` ⇒ `panic!`（与 `TestContext::build` 同口径）；`Option` 返回值保留（兼容调用点），但恒 `Some`。
- 9 个调用点：`else { return; }` ⇒ `else { skip_or_fail_without_db(); return; }`
  （`let ... else` 要求分支发散，而该辅助函数签名是 `()`，所以按仓库既有 42 处的写法补 `return;`）。

| 验证 | 修复前 | 修复后 |
|---|---|---|
| 假绿实验（`CI=1` + 不可达 URL） | **PASS 30.036 s**（0 断言） | **FAIL 30.029 s** + `[integration] No reachable test database … refusing to run` |
| 真库 9 条 | — | **9 passed**（1.0–4.8 s，逐条真跑） |
| 变异自证 ①（`voice_multipart_body_limit` 退回 axum 默认 2 MiB） | — | **红**：`2.5 MiB 语音必须被接受：/voice/config 宣告的上限是 4194304 字节，响应 {"errcode":"M_BAD_JSON",…}`；还原后 7 passed |
| 变异自证 ②（`worker_surface_middleware` 的 `role_owns_path` 恒真） | — | **红**：`/_matrix/client/v3/media/config must be refused for a synchrotron: {"errcode":"M_UNAUTHORIZED",…}`，`left: 401 / right: 404` |
| 变异自证 ③（全局 `RequestBodyLimitLayer` 抬到 `usize::MAX`） | — | **红**：`大小写变体不得绕过请求体上限…响应 {"errcode":"M_BAD_JSON","error":"Voice message too large. Max size is 524288 bytes"}`，`left: 400 / right: 413` |

三条变异各自钉住了不同的东西，其中两条给出了**额外信息**：

- ② 的 401 vs 404 证明那个 404 **确实由 gate 返回**——门禁失效后请求会穿透到 handler 变成 401，
  而不是被别的层顺手拒掉。这类"404 到底是谁给的"的歧义，只有变异能分清。
- ③ 去掉全局层后请求**没有变成 200**，而是被 voice 自己的业务判据兜住（`Voice message too large`
  → 400）。⇒ 请求体上限是**纵深防御**（全局 layer + 业务判据），不是单点。测试钉的是
  "必须是 **413**（规范状态码）"，比"必须被拒"更严格 —— 这一条顺带否掉了"这个 CVE 缓解只有单层
  保护"的猜想。

### 2.2 受影响清单（9 条，全部为安全边界）

| 文件:行 | 用例 | 该用例守的边界 |
|---|---|---|
| `voice_routes_tests.rs:599` | `test_voice_upload_accepts_body_above_the_axum_default_limit` | 2.5 MiB 必须被接受（axum 默认 2 MiB 不能成为事实上限） |
| `voice_routes_tests.rs:622` | `test_voice_upload_rejects_body_above_the_configured_limit` | 超 4 MiB 必须拒绝 |
| `voice_routes_tests.rs:649` | `test_voice_upload_body_limit_is_content_type_case_insensitive` | **CVE-2026-102592 对应项**：`Multipart/Form-Data` 大小写变体不得绕过上限 |
| `voice_routes_tests.rs:676` | `test_voice_upload_streamed_body_without_content_length_is_still_rejected` | 无 `Content-Length` 的流式体仍须受限 |
| `api_media_routes_tests.rs:123` | `test_media_upload_with_id_accepts_over_2mb_body` | 媒体 2 MiB 边界（接受侧） |
| `api_media_routes_tests.rs:177` | `test_media_preview_and_delete_boundaries` | 预览/删除边界 |
| `api_media_routes_tests.rs:298` | `test_client_v1_authenticated_media_thumbnail_and_preview_routes_work` | v1 认证缩略图/预览 |
| `api_worker_surface_gate_tests.rs:51` | `a_narrowed_instance_serves_its_own_prefix_and_refuses_the_rest` | W-F 窄化授权：非属主实例必须 404 |
| `api_worker_surface_gate_tests.rs:89` | `without_the_switch_the_same_paths_are_not_refused` | 同上（开关关闭时的反向断言） |

**为什么偏偏是它们**：这 9 条都要改 `ServiceContainer` 配置（如 `server.max_upload_size`），
唯一能改配置的入口就是 `with_config` —— 而它恰好是 fail-open 的那一个。
即**测试基础设施的缺陷，把最关键的安全测试降级了**。

### 2.3 补齐项

| 优先级 | 动作 | 判据 |
|---|---|---|
| **P0 ✅ 已完成** | `setup_fresh_test_app_with_config` 内部先 `require_test_database().await`（与 `TestContext` 同口径），并把 `.ok()?` 改成 `Err` ⇒ `panic!` | 重跑 §1 ⑤ 的假绿实验 ⇒ 已 **FAIL**；真库下 9 条仍 **9 passed**（见 §2.4） |
| **P0 ✅ 已完成** | 9 个 `else { return; }` 换成 `else { skip_or_fail_without_db(); return; }`（双保险） | 同上 |
| **P1 🟡 3/9 已做** | 变异自证 ① voice 体上限 ② worker surface gate ③ 全局上限 vs Content-Type 大小写。覆盖了三种不同机制（业务判据 / 路由门禁 / 全局 layer）。**其余 6 条待补**（多为同机制重复：media 2 MiB 与 ①同层、`without_the_switch` 与 ②互补） | 见 §2.4 |
| **P2** | 全仓 `with_config` 的 12 处 helper 内 `return None` 一并改 fail-closed | `grep -c "else {" tests/integration/*.rs` 复查 |

## 3 R2：制度性降标

### 3.1 覆盖率门禁的三个缺口

| 缺口 | 证据 | 后果 |
|---|---|---|
| **不在 PR 车道** | `ci.yml:1380-1383` 的 `if` 只有 `schedule` / `workflow_dispatch+run_slow_tier` / `push main` | PR 阶段的覆盖回归**看不见**，合并进 main 后才知道 |
| **新文件只需 30%** → **✅ 已改为 50%**（2026-10-25） | `ci.yml:1496` 原 `--new-file-floor 30`（脚本默认 60，被显式下调）。已上调到 50，同步 `run_coverage.sh` 提示、`coverage_policy.md` §1/§2/§3、`TESTING.md` | 用 40% 的假 lcov 喂脚本 ⇒ **exit 1**；50% ⇒ exit 0（不误伤） |
| **已入库文件无绝对下限** | `scripts/check_file_coverage.py:8` 只判 `must not regress`；`ci.yml:1482-1485` 注释写明"absolute floors are deliberately not re-applied to baseline-known files, because…188 files red" | `coverage_baseline.json` 里 **59 个文件 0.0%、109 个 ≤30%** 被永久冻结，再差也不会红 |

基线分布（实测 `coverage_baseline.json`，608 个文件）：中位数 **81.5%**、均值 68.1%，
所以**不是"全仓覆盖率低"**，而是"**差的那批被冻结了，且新代码门槛低**"。

补：

| 优先级 | 动作 |
|---|---|
| **P1 ✅ 已完成** | `--new-file-floor 30 → 50`（先不喊 60，避免一次打红太多）；`core_file_coverage_prefixes.txt` 内的新文件沿用 70 |
| **P2** | 给 0% 文件中**有生产入口**的（`src/server/router.rs`、`src/tasks/mod.rs`、`synapse-common/logging.rs`）建"提升到 30%"的一次性任务，逐文件记账（**不是**一刀切重贴全局下限 —— 那会重演 188 文件红） |
| **P2** | 把 coverage 加进 PR 车道（可只跑受影响 crate 的 lcov，用 `cargo llvm-cov -p <crate>` 降成本） |

### 3.1.1 覆盖率基线可以被一次「不完整报告」清零（本次审计中新发现 + 已修）

**缺陷**：`scripts/check_file_coverage.py` 的 `main()` 里

```python
save_path = args.save_baseline or args.baseline  # 不传 --save-baseline 也写回基线
save_baseline(save_path, current, previous=baseline)
```

而 `save_baseline()` 的实现是 `{p: max(prev, cur) for p, cur in files.items()}` ——
只遍历**本次 lcov** 的键。两者叠加：**拿一份不完整的 lcov 跑一次，基线里没出现在本次报告中的
条目会被整批丢掉**。

**为什么这不只是理论**：CI 的 `Per-file coverage ratchet (B.3)` 步骤**故意不传**
`--save-baseline`，靠的正是这个默认写回在 main 上自动收紧（`--save-baseline` 只有
bootstrap 步骤显式传）。所以"默认写回"是正确的设计，缺的是**合并时保留 previous 的键**。

**实测后果（2026-10-25，我自己踩的）**：做 §3.1 的门槛双向验证时，用一条假记录的 lcov
（`SF:synapse-common/src/ramp_up_probe.rs`）跑脚本 —— `coverage_baseline.json` 从
**608 条变成 1 条**。棘轮当场报废：之后所有历史文件都变"新文件"，只受 ramp-up 门槛约束，
"已入库文件不得回退"那一半彻底失效。而且**整个过程没有一行警告**，`git status` 里只显示
`M scripts/ci/coverage_baseline.json`，看起来像"别人的在飞改动"。

**修法**：`merged` 先从 `previous` 复制全量，再对本次报告里的文件取 `max`；保存后打印
「N 个文件本次测量，M 个条目保留自上次」。代价是被删除/重命名的文件旧条目会留在基线里
（棘轮只增不减，无害；重命名后的新路径按"新文件"受 ramp-up 约束，那本来就是 policy 想要的压力）。

**三向验证**（每次跑完立即 `git checkout` 还原）：

| 场景 | 结果 |
|---|---|
| 不完整报告（1 条假记录） | 修复前 **608 → 1**；修复后 **608 → 609**，并打印「1 个文件本次测量，608 个条目保留自上次」 |
| 某文件覆盖率下降（`synapse-cache/health.rs` 100% → 假报 50%） | 保存后仍 **100%** —— `max` 的单调语义未被破坏 |
| 还原 | `git checkout` 后回到 608 条 |

⚠️ **通用教训**：跑"检查类"脚本前先确认它有没有写副作用。本脚本名字叫 `check_*`，
却在默认参数下**改写**基线 —— 这类"检查即写入"的形态在做门槛验证（喂假数据）时最危险。
本轮之后凡是用假 lcov/假数据跑它，都必须先 `cp` 备份、跑完 `git checkout` 还原。

### 3.1.2 基线里的 0% 有一部分是「陈旧」，不是「缺测试」（2026-10-25 实测）

原本 §6 第 7 项写的是"给 3 个 0% 但**有生产入口**的文件各补一条冒烟"。动手前先做了一次插桩探测，
结论是**前提部分不成立**：

```bash
cargo llvm-cov --test integration --features "…" --lcov --output-path /tmp/router_probe.info -- server_router_assembly
# 3 passed；产物里：
#   SF:…/src/server/router.rs   LF:35  LH:29
```

两条腿各跑一次（`--workspace --lib --all-features` 与 `--test integration`），差异一目了然：

| 文件 | 基线记录 | lib 腿 | integration 腿 | 判定 |
|---|---|---|---|---|
| `src/server/router.rs` | **0.0%** | 0/35 | **29/35 = 82.9%** | **基线陈旧**，不缺测试 |
| `src/tasks/mod.rs` | 0.0% | 0/203 | 0/203 | **真缺口**（203 行，两条腿都不执行） |
| `synapse-common/src/logging.rs` | 0.0% | 0/49 | 该腿不编译它 | 真缺口，但属**进程启动路径**（只在 `src/server/telemetry.rs` 的 `init_logging` 里被调） |
| `src/server/telemetry.rs` | 0.0% | 0/18 | 0/18 | 同上（启动期一次性初始化） |

读数说明：

- `router.rs` 在 lib 腿 0%、integration 腿 82.9% —— **完全对上"基线按 `--lib` 口径生成"的假设**，
  这是本次"基线陈旧"判定最硬的一条证据。
- `tasks/mod.rs` 两条腿都 0/203 ⇒ 不是口径问题，是**真的没有任何测试执行过那 203 行**。
  它是后台维护任务（清理/修剪/监控），正是"出错没人发现"的典型形态——这也是第 7 项里**唯一**
  站得住的"该补测试"目标。
- `logging.rs` / `telemetry.rs` 的 0% 属"进程启动一次性初始化"，补测试要连 tracing subscriber 与
  OTLP exporter，收益低于 `tasks`。**登记为已知缺口，不排期**。

`build_router` 被 `tests/integration/mod.rs::production_app` 调用，而几乎所有集成测试都走那个
装配入口 —— 逻辑上不可能 0%。口径差异的来源，是文档与实现不一致：

- `docs/coverage_policy.md` §2 写「CI 用 `cargo llvm-cov --workspace --lib`（+ `-p synapse-storage --lib`）」
- `scripts/ci/run_coverage.sh` 步骤 2 实际是 `cargo llvm-cov --workspace --exclude synapse-storage`
  —— **不带 `--lib`**，会跑全部 test target（含 integration）
- `check_file_coverage.py` 的注释也沿用旧口径（「`cargo llvm-cov --workspace --lib` never executes
  a binary entry point」）

⇒ 基线很可能是按 `--lib` 口径生成的（那时 integration 不计入，`router.rs` 自然是 0%），之后
`run_coverage.sh` 改成全 target 却没重测基线。**这类"基线记录的是旧口径"的条目，补测试毫无用处**
—— 覆盖率仍按旧数记；只有**重测**才能修正。

而"重测"这件事，正好被 §3.1.1 的修复解锁了：在此之前，任何一次不完整的 lcov 都会把基线截断，
所以"随手跑一次局部覆盖 + save-baseline"是危险操作；现在合并会保留 previous 的键，才可以安全地
用局部报告增量收紧。

**行动改判**：第 7 项从"补 3 条冒烟"拆成两件互不相同的事：

1. **重测基线**（跑一次完整覆盖率腿 + `--save-baseline`）—— 只有这样才能把 `router.rs` 这类
   "陈旧 0%" 抬起来。59 个 0% 里有多少属于这一类，现在还不知道，只能靠重测分流。
2. **补 `src/tasks/mod.rs` 的测试** —— 它是唯一经两条腿验证、确实是 0 执行的生产模块（203 行，
   后台维护任务）。**已补第一批（2026-10-26，3 条，无需数据库）**：
   - `from_config_treats_zero_intervals_as_defaults_not_disabled` —— 钉住 `0` 的语义是
     「回落默认间隔」而**不是**「禁用」。`ServerConfig::default()` 里五个 interval 全是 `0`，
     若把 0 当"关掉"，默认配置的部署会静默失去健康检查/池指标/性能/完整性巡检。
     禁用走的是另一条路（`global_maintenance_tasks_enabled`）。
   - `from_config_uses_each_configured_interval_verbatim` —— 五条独立分支逐字段生效。
   - `new_scheduler_reports_no_results_yet` —— 台账初值必须是 `None`（"还没跑过"），
     不能是 `Some(空报表)`（消费方会当成"跑过且一切正常"）。

   实现上用 `PgPoolOptions::connect_lazy` + `Database::from_pool` 造一个**永不连接**的
   `Database`，所以这几条不依赖数据库；`connect_lazy` 需要 Tokio 上下文，故用 `#[tokio::test]`。

   变异自证：把 `health` 那段改成直接用 `interval_secs`（0 不再回落默认值）⇒ 第一条红、
   另两条仍绿（`left: 0ns / right: 10s`），还原后 3 passed。

   **仍未覆盖**：5 个 `start_*` 循环（约 170 行）—— 补它们需要可控时钟 + 库里造出待清理数据 +
   断言"清了什么、留下什么"，属独立一笔。

`logging.rs` / `telemetry.rs` 登记为已知缺口不排期（启动路径，收益低）。

### 3.2 性能验收只有报告，没有断言

`tests/performance/appservice_scheduler_perf_tests.rs` 的 **2 条用例全部** `#[ignore]`（`reason` 写明
"按墙钟断言在 CI 上会假失败，只报告不断言"）。`TESTING.md:215-217` 自认
"真实、可执行、会变红的性能门禁目前只有 `scripts/ci/compute_perf_gate.sh`（纯计算）与
`scripts/ci/sliding_sync_perf_gate.sh`（需 Postgres）。其余仅作人工参考。"

⇒ 调度器积压、p95/p99 劣化**没有任何自动化拦截**；`P4_ci_gate_integrity` 那张待办表里的
"给 p95 加真实断言（需先定稳定阈值）"至今未做。

#### 3.2.1 已落地（2026-10-25）：一条「折算成数据库往返次数」的门禁

新增 `tests/performance/appservice_scheduler_perf_tests.rs::statistics_per_service_cost_does_not_grow_with_service_count`，
**故意不加 `#[ignore]`**，并接入 CI（`ci.yml` 的 Test & Lint job，`require_tests_ran.sh` 包裹）。

判据：把 `get_statistics` 的**每行读出成本**折算成「几次数据库往返」——

```
reference_roundtrip = 同一 pool 上 SELECT 1::bigint 的平均耗时（机器速度单位）
cost_per_row        = (总耗时 / (N × 迭代)) / reference_roundtrip
断言 cost_per_row < 0.5
```

**为什么不是"复杂度比值"**（这一版是被变异自证推翻的）：

第一版按"每服务成本随规模增长的倍数"写，阈值 1.5（正常实测 0.43–0.78）。注入 N+1 后
每服务成本从 9.9μs 涨到 187μs（**19 倍**），但小规模被同样拖慢，比值只从 0.55 变到 **1.08**
—— **照样 PASS**。原因很朴素：N+1 是"每行固定多一次查询"，每服务成本对 N 仍是常数，
复杂度维度**看不见**它。所以"线性度/复杂度"只能抓"每行扫全表"，抓不到最常见的退化。

实测对照（本机，2026-10-25）：

| 实现 | 每行成本 | 折算往返次数 | 结果 |
|---|---|---|---|
| 正常（一次查询取全部行，摊到 N 行） | 9.9μs（N=40）/ 5.1μs（N=160） | **0.061 / 0.031** | PASS |
| 注入 N+1（每行再 `SELECT COUNT(*)` 扫全表） | 187μs / 195μs | **0.985 / 1.026** | **FAIL** |

阈值 0.5 落在两者之间：正常侧 8 倍余量，退化侧 2 倍超出。分子分母同机同 pool，与机器速度无关。

**能抓什么、抓不到什么**（写清楚，免得被当成万能门禁）：
- 抓：N+1、丢索引导致的全表扫描、把批量化改成逐行往返；
- 抓不到：整体常数变慢（如换了个慢 2 倍的序列化库）—— 那仍需人工基准。

**这仍然只是 1 条门禁**（取代 0 条），覆盖面是"统计查询"一条路径。其余 perf 用例保持 `#[ignore]`：
它们的判据是绝对墙钟，进 CI 只会制造 flake。

### 3.3 `cargo test --doc` 是编译门禁

`AGENTS.md:13` 与 `P5_engineering_2026-09-11.md:56` 都写明：全 workspace 只有 4 个 doc test，
**全部 `#[ignore]`**，曾有一次 rustdoc-only 的 E0106 绿着通过。CI 已改为
`cargo test --doc --locked --workspace`（`ci.yml:612`），但那是**编译**门禁。

⇒ 定性为"已知且登记"，补的价值低（doc 示例本身少）。优先级 **P3**，不建议投入。

## 4 R3：债务（当前不致命，但有复活路径）

### 4.1 572 处 `else { return; }` 残留

实测分布：**静默 `return` 572 处 vs fail-closed 42 处**（比例 13.6 : 1）。

关键定性：`TestContext::build`（`tests/integration/mod.rs:555-560`）已经在 2026-10-18 改成
**panic**，注释写明"`Option` 返回值仅为兼容既有 ~50 个 `let Some(..) else { return; }` 调用点，
实际恒为 `Some`"。所以这 572 处**目前是死代码**，不是活缺陷。

但 §2.1 已经证明：只要未来出现第二个 `.ok()?` 形态的 setup（`with_config` 就是），
这 572 处会**立刻**变成 572 条静默假绿。这是"结构上预留了假绿接口"。

#### 4.1.1 执行方式修正（2026-10-25：改守夹具本体，不盲改 559 个调用点）

原计划是"572 处统一收口"，实际动手时按数据改了做法，理由两条：

1. **559 个调用点里混着"feature 关闭就该跳过"的夹具**。分类后发现 `enabled_app()`、
   `signed_pdu_fixture()` 这类 helper 也返回 `Option`，但它们的 `None` 是**语义**
   （feature 未开 / 前置数据不成立），改成 panic 会把合法跳过变成红。
2. **只要夹具不再 fail-open，调用点就是死代码**。改调用点不增加任何安全性，只是把炸弹搬走；
   而 R1 的根因就在夹具本体（`.ok()?`）。

实际做法：
- 逐个改造了 **7 处**真 fail-open 的夹具（缺库/建池失败 ⇒ `panic!`，与 `TestContext` 同口径）：
  `api_admin_user_lifecycle_tests`、`api_content_scanner_integration_tests`、
  `presence_storage_tests_migrated`、`protocol_compliance_tests`、
  `api_federation_transaction_tests`（MSC4307 fixture）、`migration_search_path_tests::connect_admin`、
  `test_schema_housekeeping_tests::connect_admin`。其中三处原本还带着
  `eprintln!("Skipping …")` —— 明确宣告"要跳过"，却仍被 libtest 记成 `ok`。
- 新增守卫 `scripts/ci/check_test_setup_fail_closed.py`（已接入 preflight）：扫
  `tests/**/*.rs` 里**名字表明是夹具**（`setup*`/`prepare*`/`connect_*`/`*_pool`/`*database*`/
  `*fixture*`/`*context*`/`test_app*`/`enabled_app`）且返回 `Option` 的函数，体内出现 `.ok()?`
  或裸 `return None` 即红。白名单只有 `tests/performance/manual_smoke_tests.rs`（全 `#[ignore]` 的手工冒烟）。
  守卫**先剥整行注释**再加判 —— 首次运行就误报了一次自己刚修好的函数（注释里留着"此前是 `.ok()?`"）。

判据：改造后 `api_admin_user_lifecycle_tests` + `api_content_scanner_integration_tests` +
`presence_storage_tests_migrated` + `protocol_compliance_tests` 共 **40 passed**；
`api_federation_transaction_tests` **13 passed**；两个 unit 族 **4 passed**。
守卫变异自证：临时放一个 `setup_probe_context().await.ok()?` 的探针文件 ⇒ **FAIL/exit 1**，
删除后 ⇒ OK/exit 0。

### 4.2 测试替身未做特性隔离（MAINT-01）

`synapse-storage/src/test_mocks/` 有 33 个 in-memory 替身，`member.rs:129`、`presence.rs:31`、
`room.rs:171` 仍是 `unimplemented!("… has no database pool")`；`admin_media.rs:95`、
`room.rs:120` 标注"stub returns all media/rooms"。

⇒ unit 层用这些替身时，**语义与真 PG 不同**（如 `stub returns all rooms` 忽略过滤）。
审计已登记为 MAINT-01（收益低于改动面），但**没有门禁**禁止新 mock 继续简化。

补（**P2**）：给 `test_mocks/` 加一条守卫测试 —— 每个实现的 doc 注释必须声明"与生产实现的差异"，
且 `unimplemented!` 的方法必须在白名单里（新增即红）。

### 4.3 回归范围按文件选，不按契约选

`NEW-17`（`2377043e5`）把滑动同步的 `rooms` 分桶后，`sync_path_equivalence_tests` 的 3 条
ARCH-02 等价用例**一直红着**，当次只跑了 `api_sliding_sync_routes_tests` + service 单测。
已在 NEW-11 那笔修复（`36242da93`），但**根因没修**：提交时选测试范围的判据是"改了哪个文件"，
不是"影响哪些契约"。

#### 4.3.1 已落地（2026-10-25）

新增 `scripts/ci/test_impact_map`（映射表）+ `scripts/ci/check_test_impact.py`（渲染成可粘贴命令），
接入 preflight 作**提示步骤**（新增 `advise()` 函数，恒 exit 0，与 `run()` 区分）。

设计要点（都是踩过坑才知道的）：

- **默认只看暂存区**，暂存为空才回退"HEAD 差异 + 未跟踪"。本仓常有多会话共用工作区，
  看 `git diff HEAD` 会把别人的在飞改动也报进来 —— 提示一旦变成噪声就会被无视（实测第一次
  试跑就报了同事改的 `tags.rs` 与 `migrations/*`）。需要看全树时用 `--all`。
- **三种动作**：`nextest:<E 表达式>`（拼成完整 `cargo nextest` 命令）、`shell:<命令>`（契约链这种）、
  `note:<文本>`（没有自动化判据的事，如"`ROUTE_CONTRACT.md` 头部日期那 1 行要提交掉")。
- **不做硬门禁**：全量 integration 42 分钟，塞进 pre-commit 的唯一后果是所有人学会 `--no-verify`。
- **表里优先写真踩过坑的行**：`rooms` 结构 → 等价用例（NEW-17）、`power_levels` → v12 创建者
  （C-7）、`voice.rs` → 体积上限（CVE-2026-102592）、`.sqlx`/`migrations` → 生成脚本与文档块。
  "改了 A 就跑 A 的测试"这种本来就没人会忘，不写。
- 表自身也有自检行：改 `test_impact_map` ⇒ 提示用已知命中路径跑一次
  （格式错会让脚本直接 exit 1，比默默不命中好）。

判据：`--files synapse-services/src/sliding_sync_service/timeline.rs` ⇒ 命中并渲染出
`-E 'test(sync_path_equivalence_tests)'`；`--files docs/README.md` ⇒ "未命中"；暂存 1 个文件时
只对比 1 个（不报别人的）；`check_gate_reachability` 56 个 check_* 全可达。

## 5 已排除（看着像，但查证不成立）

| 疑点 | 查证结果 |
|---|---|
| `#[ignore]` 导致用例永久静默 | **16 处全部有 reason 且有覆盖车道**（e2e 7 → `e2e-http.yml`；perf 4 → `benchmark.yml`；bench 2 → `check_pagination_benchmark.py`；generator 3 → 手工入口）。`analyze_skipped_tests.py` 报 `coverage_gap=0` |
| 形状测试冒充真覆盖 | `check_shape_only_tests.py`：29 个只锁形状，**26 有真覆盖文件、3 已登记 NONE**（`admin_extra`/`coverage`/`formatting` 全仓无对应生产模块，属幽灵族）；"名字声称覆盖但不碰生产代码"= **0** |
| 生产代码在测试时走另一条路 | `grep -c "cfg!(test)"` 在 `synapse-*/src` = **0** |
| 有意不实现的端点会假装成功 | `check_unsupported_endpoints.py` 双向门禁：代码恒 `Err` ↔ 登记 `unsupported=true`，漏登红、陈旧登记也红 |
| 棘轮基线被偷偷上调放宽 | `sqlx_dynamic_ratio_baseline` 改过 143 次，但文件头部逐条归因（如"106 → 114 的 8 处一行都不是本批写的"并列出每一处的文件与代码）。属**诚实上调** |
| 契约链守卫静默跳过 | `api_route_ledger_tests.rs` 有 **17 处** `skip_or_fail_without_db()`（历史是 16 处静默 `return`，已修） |
| 集成测试全量被压缩 | CI 的 `Run integration tests (--test integration)` 是**全量** `--all-features --no-fail-fast --test-threads 4`（1424 条 / ~42 min）。本地只跑子集是**成本现实**，不是 CI 降标 |

## 6 补齐顺序（按"能不能立刻写出会失败的验证"排）

| 序 | 项 | 成本 | 会失败的验证 |
|---|---|---|---|
| ~~1~~ | ~~`with_config` 改 fail-closed + 9 处 `else` 换 `skip_or_fail_without_db`~~ **✅ 已完成（+28/−1）** | ~30 行 | 假绿实验 **已从 PASS 变 FAIL**（§2.4） |
| 2 | 9 条安全用例的变异自证 —— **3/9 已做**（三种不同机制各一条），其余 6 条多为同机制重复 | 中 | 每条留一次红-绿记录 |
| ~~3~~ | ~~572 处调用点收口~~ → **✅ 改做「守夹具本体」**：7 处 fail-open 夹具改 panic + 新增 `check_test_setup_fail_closed.py` 守卫（接入 preflight） | 中 | 探针文件 ⇒ 守卫 exit 1；真实 4+1 族 53 passed（详见 §4.1.1） |
| ~~4~~ | ~~`--new-file-floor 30 → 50`~~ **✅ 已完成** | 1 行 + 3 处文档同步 | 已验：40% 假 lcov ⇒ exit 1；50% ⇒ exit 0 |
| ~~5~~ | ~~"改动面 → 必跑套件"映射 + preflight 提示~~ **✅ 已完成**（`test_impact_map` + `check_test_impact.py` + preflight `advise` 步骤） | ~1 h | 改 `rooms` 结构 ⇒ 提交前自动打印 `-E 'test(sync_path_equivalence_tests)'`（详见 §4.3.1） |
| ~~6~~ | ~~性能断言改相对阈值~~ **✅ 已完成**：改成「每行成本折算数据库往返次数」（比 p95 比值更抗噪），注入 N+1 ⇒ 0.985/1.026 红；正常 0.061/0.031 绿（§3.2.1） | 中 | **已实测**：变异体 FAIL / 还原后 PASS，CI 同款命令 2.6s |
| ~~7~~ | ~~覆盖率为 0% 的 3 个文件各补冒烟~~ → 拆成两件：①**重测基线**（未执行，用户中断）②**补 `tasks/mod.rs`** —— **✅ 已补 3 条**（`from_config` 的 `0` 语义 + 逐字段生效 + 台账初值），无需数据库，变异自证通过；**5 个 `start_*` 循环（约 170 行）仍未覆盖**（§3.1.2） | 中 | 3 passed / 变异后第一条红（`left: 0ns / right: 10s`）；其余两条仍绿 |
| 8 | `test_mocks/` 差异声明守卫 | 中 | 新增一个 `unimplemented!` 未登记 ⇒ 红 |
