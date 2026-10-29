# CI 车道盲区审计：22 个门禁没跑 + 一个永远失败的 artifact 步骤（2026-10-27）

> **审计问题**：CI 里"红"和"没跑"被混为一谈。本文记两条**长期拿不到有效读数**的车道：
>
> * `Repo Sanity` 长期是红的，但没人说得清红在哪 —— 因为**它红在第一步**，而同 job 后面
>   22 个门禁**全部 `skipped`**，CI 页面上只剩一句 `Repo Sanity failed`。这个 job 的"红"
>   没有提供任何定位信息，于是它被当成背景噪音，一年多没人碰。
> * `OpenAPI Artifact` 的 `client.yaml` 一步**在干净 checkout 上永远失败** —— 它读一个
>   `.gitignore` 掉的路径，而 CI 里那个路径从来不存在。
>
> **审计口径**：只认能复现的判据 —— 每条结论给出 `文件:行` 或实跑命令。

## 0 结论

| # | 事实 | 判定 |
|---|---|---|
| 1 | `Schema table coverage` 自 `15efc8dc6`（2026-10-17）起**恒红**，红因是**测试夹具**里的占位表名 `t`，不是真表 | ✅ 已修（按仓库**已有约定**改夹具，不动门禁） |
| 2 | 它一红，**同 job 后面 22 个门禁全部 `skipped`**（GitHub 顺序步骤语义，`set -e` 式短路） | 已实测：`failure 1 / skipped 22 / success 8`（§4） |
| 3 | 修掉 #1 后立刻撞上第二个阻塞：`File-size ratchet` 的基线没跟上 `f2031f559` 的 `synapse-test-utils/src/lib.rs` 增长 | ✅ 已修（`--update`，diff 2 行，§2） |
| 4 | `OpenAPI Artifact` 的 `client.yaml` 一步用 `--skip-export`，而该分支读 `scripts/api_test/reports/ledger_default.json` —— `reports/` 在 `.gitignore:44` ⇒ **干净树上永远不存在** ⇒ 该步从未成功过 | ✅ 已修（新增 `--ledger`，CI 指向刚导出的 `$RUNNER_TEMP` 清单，§3） |
| 5 | `Test & Lint (--all-features)` 的 `Run unit test target` 自 2026-09-23 起红，红因是**负向样本里的 gitignored 路径**没登记进 `INTENTIONALLY_ABSENT_DOCS` ⇒ 同样"本地绿、CI 红" | ✅ 已修（表 5 条 → 6 条，§4） |
| 6 | 本地把 27 个门禁逐个实跑：**26 过 / 1 假红**（假红仅存在于本地工作区，CI 上该步一直是 `success`） | ✅ §5 |
| 7 | 顺带证实上一轮的融合改动在真实 CI 上**生效**：`Code Coverage` 不再是 `skipped`，而是 `in_progress` | ✅（不在本文范围，见 `TEST_COMPROMISE_AUDIT_2026-10-25.md` §7.5 H） |
| 8 | 修完 #1 后 **23 个门禁首次真正拿到读数**（`ea9a57af1` 的 `Repo Sanity`）：**18 绿 / 1 红 / 4 skipped**，红的是 `File-size ratchet` —— 缩略图那两笔把 `media_service.rs` 1462→1620、`admin_media.rs` 1300→1443，与本文无关，是**新暴露的真实增长** | ✅ 经确认后 `--update` 让渡（基线 222 条不变，仅这 2 个数值变化，§6.2） |
| 9 | 同期把「`missing_docs` 独立红灯」这条登记复核掉了：本地 `check_missing_docs_ratchet.sh` ⇒ `EXIT=0`、`debt 0 = baseline 0` | ✅ 该红已是历史状态，登记作废（§6.3） |
| 10 | `gen_client_yaml.py --check` 的 `--expected` 默认指向 gitignored 文件（死分支），且**三处文档把它写成已接线的 CI 门禁** | ✅ 已修（去掉死默认 + 三处声明改实情，§6.1） |
| 11 | 让渡后 `d297b2937` 上 `Repo Sanity` **31/31 步全绿**（0 failure / 0 skipped）—— 该 job 自 `15efc8dc6` 起第一次真正全绿 | ✅ 车道闭合 |
| 12 | 同一批缩略图提交还让 **`baseline_tables` 的硬编码例外**过期 ⇒ `Test & Lint (×all-features)` 红在 `--workspace --lib`、`Code Coverage` 红在步骤 1/3（同一根因，错误信息却指向"DB 测试"） | ✅ 已修（模型改为**从基线解析**例外列表，§6.4） |
| 13 | 被当作"存量"放过的 `DB Migration Gate` 红，实为**门禁上线那天就红**：`WARNING_BUDGET` 只登记 1 条 SBG-03，实际 3 条，另两条是合法的测试库前置断言且从未登记；且 `SAFE_FILES` 是**整文件**豁免，让"计数查询是有意的"顺带换来对 `DROP SCHEMA public CASCADE` 免疫 | ✅ 已修（补登记 + 豁免收窄到**逐规则** + 豁免清单自检，§7.1） |
| 14 | 被当作"存量"放过的 `Docker Security Scan` 红**是真 CVE，但落在没人构建的目标上**：`distroless/cc-debian12` 里 `libssl3 3.0.20` 带 `CVE-2026-84782`（HIGH，修复版 `3.0.22`）；该 pin 经实测**就是上游 `latest`**（换 digest 无用），且 `runtime-distroless` 全仓无构建/部署入口 | ✅ 已裁决（删 `runtime-distroless` / `runtime-libs` 死路径，**不用** `.trivyignore` 压制，§7.2） |
| 15 | `Appservice statistics cost gate` 自 `ea8276d8e` 引入起**没有绿过**：`require_tests_ran.sh` 只匹配复数 `Starting N tests`，而该步骤的过滤器恰好只选中 **1** 条 ⇒ nextest 打的 `Starting 1 test` 被判"空转"，日志里却是 `PASS [3.331s] (1/1)` | ✅ 已修（正则加 `tests?`，补单数正控与 `Starting 0 tests` 反控，§8.1） |
| 16 | `Security Audit` 的 `advisories FAILED` 是**真公告**：`tokio 1.53.1` / `RUSTSEC-2026-0386`（`AsyncFd` I/O safety，`unsound`），补丁区间 `>=1.53.3` | ✅ 已升 `1.54.1`（全仓 `AsyncFd` 零引用 ⇒ 实际暴露面为零）；CI 复验待下一轮 `Security Audit` 真跑（§8.2） |
| 17 | 同 job 的 `licenses FAILED` 是 `xxhash-rust 0.8.19` 的 `BSL-1.0` 不在名单内 —— 它由 `022cbd67f`（redis 0.29 → 1.7）作为**传递依赖**带入；该 job 是路径过滤触发的，多数提交上 `skipped`，所以从 10-22 起一直没被读到 | ✅ 已按 `webpki-roots` 同口径加 **crate 级例外**（不塞全局 `allow`，§8.2） |
| 18 | 同一运行里另外两红不是代码问题：`Benchmark` 的 `Run benchmarks` 是 runner 被回收（`shutdown signal` / `exit 143`，日志停在 `Compiling synapse-rust`）；`Assert no leftover test databases` 是 `out of shared memory` 打断用例后的**次生**残留 | ⏸ 分别归"基础设施"与"次生"，§8.3 |

一句话：**这些车道不是"没发现问题"，是"根本没运行/根本不可能运行"**。它们此前的沉默不能作为
任何"已合规"的证据。四个缺陷里有三个同源（§1 / §3 / §4）：**判据依赖"我这台机器上有什么"，而不是
"仓库里有什么"**；§2 只是漏跑 `--update`。

§7 补上一类更隐蔽的：**门禁红了很久、于是被习惯性归为"存量"**。这类红有两种完全不同的成因 ——
一是门禁自己的清单没配齐（§7.1，天生红，从没有过绿的状态），二是被守的对象真的坏了（§7.2，
真 CVE）。**"存量红"这个标签本身会掩盖两者的区别**，所以判断前必须先看是哪一种。排查时统一问一句：*这条判据在新克隆的树里还成立吗？*

§8 把同一个问法用到剩下的红上，又分出第三、第四种成因：**门禁把绿判成红**（§8.1，
`require_tests_ran.sh` 只认复数，恰跑 1 条就恒红 —— 与它 2026-09-20 / 09-30 两次假红同源：
判据写死了某一种合法输出形态），以及**把基础设施抖动读成代码回归**（§8.3，`exit 143` +
`shutdown signal`）。另外 §8.2 的两条供应链条目说明：一个 job 长期 `skipped` 和它长期
`failure` 一样，都会让真实的公告躺在树里没人处理。

## 1 缺陷一：夹具里的 `UPDATE t` 被当成真表

门禁 `scripts/check_schema_table_coverage.py` 的做法是**对文件原文做正则**：抓
`sqlx::query*(` 后面的字符串字面量，再在字面量里找 `FROM` / `JOIN` / `INSERT INTO` /
`UPDATE` / `DELETE FROM` 后面的标识符，与 `migrations/*.sql` 里
`CREATE TABLE|VIEW|MATERIALIZED VIEW` 的集合比对（`REF_PATTERNS` 见脚本 40–48 行）。

它**不区分**两件事：

* 真正要执行的 SQL；
* **测试夹具里当作文本存在的 SQL** —— 而本仓大量门禁测试恰恰要内嵌 Rust 源码字符串。

`tests/unit/sqlx_ratio_gate_tests.rs` 的 `CONTINUATION_SOURCE` 就是后者：一个内嵌 Rust 源码的
raw string，里面有一句 `sqlx::query!("UPDATE t SET a = $1 \`（行尾续行）。门禁二次扫到它，
`t` 就成了"引用了未在迁移中定义的表"。

复现（HEAD 修复前）：

```bash
python3 scripts/check_schema_table_coverage.py
# Missing schema coverage for referenced tables:
# - t: tests/unit/sqlx_ratio_gate_tests.rs
```

引入点：

```bash
git log --oneline -S CONTINUATION_SOURCE -- tests/unit/sqlx_ratio_gate_tests.rs
# 15efc8dc6 fix(gates): 闭环 REMAINING_ISSUES_SWEEP 的 R-1…R-6 / L-1…L-3，unit 车道转全绿
```

该提交的意图是**把 unit 车道修绿**，代价是把 `Repo Sanity` 的第一步弄红 —— 而那条车道的红
不显示细节，于是没有任何人发现。

### 为什么是改夹具、不是改门禁

仓库**早就有这条约定**，写在 `tests/unit/sqlx_dynamic_literal_guard_tests.rs:474-478`：

> 夹具里的表名刻意用 `events`（真实表）而不是 `t`：本文件整体会被
> `scripts/check_schema_table_coverage.py` 逐字扫一遍，任何 `FROM <标识符>`
> 都会被当成"引用了未在迁移中定义的表"，一个占位表名会直接让那条门禁变红。

`sqlx_ratio_gate_tests.rs` 是这条约定**唯一的违反者**。所以修法是把三处 `t` 改成真实表
`events`（`CONTINUATION_SOURCE` / `CONTINUATION_COLLAPSED` / `CONTINUATION_RAW`），
并把这条约定**写进该文件的注释**，让下一个写夹具的人不用再踩一次。

摘要全部在运行时计算（`sha256_hex(CONTINUATION_COLLAPSED)`），没有硬编码，改文本是安全的；
实跑确认那条用例的双向断言仍然成立：

```bash
cargo test --test unit --features "test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications" \
  sqlx_cache_fresh_honours_rust_line_continuation
# test result: ok. 1 passed（折叠形态必须命中缓存 / 未折叠形态必须报缺失）
```

### 变异自证：修的是假阳性，不是把门禁弄瞎

在 `synapse-storage/src/` 放一个临时探针文件，内容是一句引用不存在表的查询：

```rust
pub fn probe(pool: &sqlx::PgPool) {
    let _ = sqlx::query("SELECT id FROM definitely_missing_table_xyz WHERE 1 = 1");
}
```

| 步骤 | 门禁输出 | 退出码 |
|---|---|---|
| 只有探针 | `- definitely_missing_table_xyz: synapse-storage/src/zz_schema_probe.rs` | **1** |
| 删掉探针 | `Schema table coverage passed: 208 referenced tables checked, 223 schema tables found.` | **0** |

⇒ 门禁仍然抓得到真表；被去掉的只有"夹具文本里的表"。

## 2 缺陷二：`file_size_baseline` 没跟上真实增长

`Schema table coverage` 一修绿，同 job 的 `File-size ratchet` 立刻报：

```text
FAIL: 1 grandfathered file(s) grew past their ceiling:
  synapse-test-utils/src/lib.rs: 2974 -> 2981
```

来自 `f2031f559`（2026-10-27，"把守卫清单对齐实施现状"）：该提交给
`synapse-test-utils/src/lib.rs` 加了 7 行（+9/−2），但没有跑 `--update`。这条增长本身是
**合理的、已评审的**改动，按棘轮自己给的处置方式（"for a deliberate, reviewed change, run --update"）
更新基线。

diff 只有 2 行，逐行核对过：

| 变化 | 原因 |
|---|---|
| `2974 → 2981 synapse-test-utils/src/lib.rs` | `f2031f559` 的真实增长（本次要修的那条） |
| `944 → 925 synapse-storage/src/pruning.rs`（位置重排，**未删除**） | `b7ed3fc47` 把它缩到 925 行，条目按行数排序后下移 |

条目总数 **222 → 222**，没有任何文件被"顺便祖父化"（`--update` 之前先 `cp` 备份、之后跑
`git diff` 逐行核对 —— 多写者仓库里 `--update` 不过脑会把别人在飞的增长一并吞下）。

## 3 缺陷三：`client.yaml` 那一步在干净 checkout 上不可能成功

`OpenAPI Artifact` job 的顺序是：先导出 `$RUNNER_TEMP/ledger_default_ci.json`（固定时间戳），
再 `gen_route_table.py --check --ledger <那份>`，然后生成 `client.yaml`。前两步一直绿，
第三步 `Generate client.yaml (OpenAPI 3.0.3)` 一直红。

原因在 `gen_client_yaml.py` 的 `--skip-export` 分支：它读的是**硬编码**的
`scripts/api_test/reports/ledger_default.json`，而 `reports/` 是**被忽略的**：

```bash
git check-ignore -v scripts/api_test/reports/ledger_default.json
# .gitignore:44:reports/	scripts/api_test/reports/ledger_default.json
```

⇒ `actions/checkout` 的干净树里没有这个文件 ⇒ 这一步**从设计上就不可能通过**，与代码是否
正确无关。它与"本地能跑"并存，因为本地工作区早就跑过 `export_ledger.sh`，那份清单躺在
`reports/` 里没人删。

修法：给脚本加 `--ledger PATH`（隐含跳过 export），CI 指向刚导出的那份：

```diff
-          python3 scripts/api_test/gen_client_yaml.py --skip-export
+          python3 scripts/api_test/gen_client_yaml.py --ledger "$RUNNER_TEMP/ledger_default_ci.json"
```

验证（三条路径都实跑）：

| 场景 | 结果 |
|---|---|
| `--ledger <临时副本>` | exit 0，且产出与 `--skip-export` **逐字节相同**（`diff -q` = identical） |
| `--ledger /tmp/nope.json` | exit **1**，`--ledger 指定的清单不存在` |
| `--skip-export`（旧路径，本地有那份清单） | exit 0，行为不变 |

> 这条与 §1 是同一类问题的两种表现：**门禁/步骤的输入，在 CI 的干净树上并不存在**。
> 区别是 §1 是"扫描面比预期宽"（把夹具文本当真），这里是"输入面比预期窄"（依赖本地残留）。

## 4 缺陷四：`--all-features` 的 unit 车道，红在一个 gitignored 的负向样本上

`Test & Lint (1.93.0, --all-features)` 的 `Run unit test target (--test unit)` 自 2026-09-23
（`24f123805`）起一直红，而同 job 的 `default-features` 变体是绿的 —— 所以它被当成"all-features 专属的
怪问题"，谁也没去碰。实际失败只有一条：

```text
test doc_credibility_guard_tests::code_referenced_docs_exist ... FAILED
源码引用了不存在的文档（1 处）：
  docs/superpowers/plans/2026-09-22-protocol-correctness-phase1.md  <-  tests/unit/doc_credibility_guard_tests.rs
```

那处引用是**负向样本**（`the_path_checker_rejects_a_fabricated_reference` 用来证明路径检查器真的在跑的那句话），
而它指向的目录被 `.gitignore:113` 忽略：

```bash
ls docs/superpowers/plans/2026-09-22-protocol-correctness-phase1.md   # 本地：存在
git ls-files docs/superpowers/plans/                                   # 空 —— 一个文件都没入库
```

⇒ **本地永远绿、CI 永远红**。守卫自己早就给了处置方式（"③ 确属「故意引用不存在路径」的负向样本 ⇒
加进 `INTENTIONALLY_ABSENT_DOCS` 并写明理由"），但那张表当时只有 5 条，漏了这第 6 条 —— 因为
加它的人在本机看得见那个文件，红证明跑不出来。

修法：`INTENTIONALLY_ABSENT_DOCS` 从 5 条扩到 6 条，并写明"该路径只在干净 checkout 上不存在"的判据。

> 至此本文四个缺陷里有三个同源：**判据依赖"我这台机器上有什么"，而不是"仓库里有什么"**。
> 排查时统一问一句：*这条判据在新克隆的树里还成立吗？*

## 5 复核：本地把 27 个门禁全跑一遍

`Repo Sanity` 的 27 个步骤全部是 `run:` 块，不依赖别的 job，唯一外部依赖是两个 `promtool` 步骤
（本机已有）。抽出来逐个执行（每步独立 `bash -e`，干净 `PATH` + 去掉 `BASH_ENV`）：

| 结论 | 数量 | 说明 |
|---|---|---|
| **OK** | **26** | 含本次修掉的 `Schema table coverage` 与 `File-size ratchet` |
| FAIL（**假红**） | 1 | `No private keys (key files)` —— 见下 |
| 合计 | 27 | |

假红的成因，已确认不是缺陷：

```bash
git check-ignore -v docker/federation-test/certs/ca.key
# .gitignore:153:certs/	docker/federation-test/certs/ca.key
```

`docker/federation-test/certs/*.key` 是**本地生成的联邦测试证书**，被 `.gitignore` 排除 ⇒
`actions/checkout` 的干净树里没有它们 ⇒ CI 上该步一直是 `success`（本次运行亦然）。
只有本地工作区同时具备"证书存在"与"该步骤 grep 整棵树"两个条件才会红。

CI 上这个 job 的步骤结论分布（修复前那次运行）：

```bash
gh run view <run-id> --json jobs \
  --jq '.jobs[] | select(.name=="Repo Sanity") | [.steps[] | .conclusion] | group_by(.) | map({(.[0]//"pending"): length}) | add'
# {"failure":1,"skipped":22,"success":8}
```

## 6 仍未解决（附本轮四条复核记录：§6.1 修好 / §6.2 让渡 / §6.3 作废 / §6.4 修好）

* ~~**结构性**：GitHub 的步骤是顺序短路语义 —— 前一步失败，后面全部 `skipped`~~ → **已修**（`c30a097e8`，2026-10-28）。
  修法是给 `repo-sanity` 的**每一步**加 `if: ${{ !cancelled() }}`。依据是 GitHub 官方语义：
  「A default status check of `success()` is applied unless you include one of these functions」——
  `cancelled()` 属于该函数族，因此显式写 `!cancelled()` 会**抑制**隐式的 `success()`，
  前一步失败后该步**照常执行**（取消仍中止整 job，所以没有选 `always()`）。
  job 结论仍是 `failure`（step 失败就是 job 失败），**不需要**末尾聚合步，也不需要拆 job；
  "一个门禁红 ⇒ 其余 31 个门禁那次等于没跑"这条后果不再成立。
  历史读数对照：`ea9a57af1` 上 `success 26 / failure 1 / skipped 4`、`d297b2937` 上 `31/31`，
  都是修法**之前**的形态；修复后 `repo-sanity` 共 32 步、32 步带守卫。
  **守卫**：`scripts/ci/check_ci_summary_dependencies.py` 判据 5（`ISOLATED_GATE_JOBS = ("repo-sanity",)`）
  逐 `- name:` 切段检查守卫，少一条即红，且 `--mutation-check` 会注入"某个门禁步没带守卫"自证。
  本地实测该脚本 `EXIT=0`。**真实 CI 读数待下一次 run 复核**（本轮 `gh run list` TLS 超时，
  无法本地取证，故此处不宣称已见到修复后的 step 分布）。
* **约定靠人记**：`check_schema_table_coverage.py` 仍然逐字扫描夹具文本，所以"夹具里用真实表名"
  这条约定只能靠注释提醒。要不要让门禁自己识别"内嵌源码的字符串"是另一个决策 ——
  可用的精确判据是「该 `sqlx::query(` 的起始位置落在另一个字符串字面量区间内」，
  但这会改动全仓扫描语义，未做。
* **`client.yaml` 整 job 转绿已在真实 CI 确认**：`40464882348` 的 `OpenAPI Artifact` = `success`
  （`--ledger` 修法生效；此前 `Generate route-table.json` 与两个上传步都没被走到过）。
* ~~**`--check` 分支其实是死的**~~ → **已修**（§6.1）。
* `Test & Lint` 的 `unit target` 灯已由 §4 定性并修掉；与之并列登记的 `missing_docs` 经复核**已是绿的**
  （见 §6.3），该登记作废。原先登记四条：schema coverage / client.yaml 已由 §1、§3 修掉。

### 6.1 `gen_client_yaml.py --check`：死默认 + 三处「CI 有这个门禁」的失实声明（已修）

原状：`--expected` 默认指向 `docs/openapi/client.yaml`，而该文件在 `.gitignore:118` ⇒ 干净
checkout 上永远不存在 ⇒ `--check` 在任何机器上只能报 `reference artifact not found`。
它也确实不在 CI 路径上（`grep -E 'gen_client_yaml' .github/` 只有 ci.yml:371 的**生成**步）。

更要紧的是顺着这条线查出的**声明失实**：三个地方把 `--check` 描述成已接线的门禁 ——

| 位置 | 原话（失实） |
|---|---|
| `scripts/api_test/scan_handler_schemas.py:17-18` | 「CI `ci.yml` 有 `gen_client_yaml.py --skip-export --check` 门禁」 |
| `scripts/api_test/README.md:170` | 「CI 跑 `gen_client_yaml.py --skip-export --check` 守新鲜度」 |
| `scripts/api_test/artifact_common.py:25-27` | 「Drift is still caught by the wired `--check` steps (`gen_client_yaml.py --check`, …)」 |

**这三处都不能成立**：CI 里带 `--check` 的只有 `gen_route_table.py --check`（对账**入库**的
`docs/openapi/route-table.json`，ci.yml:362）。`client.yaml` 不入库 ⇒ 没有参照物 ⇒ 不可能有对账门禁。
连带的效果：原警告「Stage D 覆盖 client.yaml 会弄坏门禁」也是空的 —— 手改一个 gitignored 生成物
对仓库没有任何后果，只影响本地那份。

处置：

* `--expected` 去掉默认值，`--check` 时必须显式给参照物；缺省时报错并说明「产出是生成物、
  干净 checkout 上不存在」，同时点明 `docs/openapi/*.yaml` 里入库的都是**分变体** spec
  （`client-all` / `client-oidc` / `client-saml` / `client-worker`），与 default profile 不可逐字节比对；
* 三处声明改成实情（保留「Stage D 别覆盖它」这条**仍然成立**的建议，理由换成「生成物 + 禁止手改」）；
* 报错里原先还建议「regenerate **and commit** it」—— 让用户去提交一个 gitignored 文件，一并改掉。

### 6.2 `File-size ratchet`：22 个门禁解锁后暴露的真实增长（已让渡）

§1 修掉 `Schema table coverage` 之后，同 job 的第 24 步 `File-size ratchet` 第一次真正执行，红了：

```
FAIL: 2 grandfathered file(s) grew past their ceiling:
  synapse-services/src/media_service.rs: 1462 -> 1620
  synapse-storage/src/admin_media.rs: 1300 -> 1443
```

归因：`6956f1315..HEAD` 的缩略图闭环两笔（`1ab01c9a9` / `ea9a57af1`）新增 301 行，
`git diff --stat` 逐文件对得上（+158 / +143），与本文的四个缺陷无关 —— **不是假红，是真实增长**。

处置（**经确认后按棘轮自己的 escape hatch 让渡**，并记录在此以便日后收紧）：

* `python3 scripts/ci/check_file_size_ratchet.py --update`，基线 `222` 条 → `222` 条；
* 按内容 `sort` 后比对，差异**只有这两条数值**（`1300→1443`、`1462→1620`），无增删、无排序位移以外的改动；
* 复核 `EXIT=0`：`file_size: 827 files scanned, 222 over 600 (baseline 222)` / `OK: file sizes at baseline`。

**为什么没选"抽文件"**：`media_service.rs` 的 +158 里有 34 行是**散在既有函数体内的补丁**
（`git diff -U0` 的 `@@ -857,0 +945,9 @@` 等 5 处），无法整块搬走；把可移动的 124 行搬到同级模块后
仍有 `1496 > 1462` 上限 ⇒ 单靠抽取回不到上限内，只能动既有代码做真重构。
该文件正被另一会话改动，真重构留作独立事项。**这是让渡、不是修好**：两个文件此后仍不得再涨。

**CI 复核**：`d297b2937` 上 `File-size ratchet` = `success`，该 job 随之 **31/31 全绿**。

### 6.3 `missing_docs` 棘轮：复核结论是绿的（登记作废）

`CI_LANE_BLINDNESS` 早先把 `missing_docs` 与 unit target 并列为"两条独立红灯"，但那条红是
`GATE_INTEGRITY_FOLLOWUP_2026-09-19_LOG.md:1058` 的历史读数（"debt decreased 3 < 6"）。实跑：

```
[1/2] scanning 0 new + 4 modified .rs files vs HEAD~1
[2/2] counting clippy missing_docs warnings across workspace...
  current missing_docs debt: 0
  baseline: 0
OK: missing_docs debt at baseline (0), no regression.        # EXIT=0
```

它在 CI 里一直拿不到读数的原因只是**步骤顺序**：`Check missing_docs ratchet` 排在
`Run unit test target` 之后，前者一红它就被 `skipped`（正是 §0 #2 说的一红全盲）。
它在 `--all-features` 车道、`SQLX_OFFLINE=true` 下跑，本地需 `HEAD~1` 可解析（`fetch-depth: 0` 已配）。

### 6.4 缩略图提交让 `baseline_tables` 的硬编码例外过期（lib 车道 + Code Coverage 双红，已修）

`ea9a57af1` 上两条车道同时红，**而且都在 §4 那条 doc 守卫之前就断了**：

* `Test & Lint (stable, all-features)` 红在 `Run library unit tests (--workspace --lib)`
  —— 排在 `Run unit test target` **之前**，所以 §4 修的那个守卫这次根本没轮到执行；
* `Code Coverage` 红在 `Run coverage` 的**步骤 1/3**（`cargo llvm-cov -p synapse-storage --lib`）。

同一根因，`baseline_tables.rs:185`：

```
the v11-10 dynamic `DROP INDEX` cleanup (markers ["DROP INDEX IF EXISTS %I.%I",
"c.relname LIKE 'uq_%'", "NOT IN ('uq_to_device_txn_msgid')"]) is no longer present in the baseline
```

`1ab01c9a9`（缩略图）把基线的 v11-10 例外列表从 `NOT IN ('uq_to_device_txn_msgid')` 改成
`NOT IN ('uq_to_device_txn_msgid', 'uq_thumbnails_variant')`，而模型把这行**整句**当 marker、
把例外集合 `V11_10_KEPT_INDEXES` 硬编码成 1 条 ⇒ 基线**每加一个例外就假红一次**。
更糟的是方向错了：模型会反过来说"`uq_thumbnails_variant` 会被 v11-10 清掉"，而基线实际保留它 ——
这正是该文件存在的目的（让 reference 集与部署实际 schema 一致）所反对的"静默高估删除"。

处置：**让模型从基线里解析例外列表**，不再复制一份。

| 改动 | 内容 |
|---|---|
| `V11_10_DYNAMIC_DROP_MARKERS` | 3 → 2：去掉那句会被追加的 `NOT IN (...)`，只留两个结构 marker |
| `v11_10_kept_indexes()`（新） | 锚 `c.relname LIKE 'uq_%'` → 找 `NOT IN (` → 取到 `)` → 按逗号拆、去引号 |
| `V11_10_REQUIRED_KEEP`（新） | 仍钉 `uq_to_device_txn_msgid`；`:234` 断言解析结果必须含它 |
| 新测试 | `parsed_exception_list_keeps_plain_unique_indexes_the_baseline_spares` —— 钉 `uq_thumbnails_variant` 既在解析结果、也在 reference 集里 |

`V11_10_REQUIRED_KEEP` 那条断言是**解析失效的兜底**：锚点一旦漂移，`v11_10_kept_indexes()`
会返回空表，若不兜底就会静默变成"没有例外"（比假红更坏的假绿）。

实测：`cargo test -p synapse-storage --lib baseline_tables` ⇒ **14 passed**
（含新测试）；`cargo clippy -p synapse-storage --lib --all-features -D warnings` 干净。

**变异自证**：把解析锚点改成 `c.relname LIKE 'uq_%ZZZ_MUTANT'` ⇒
**6 条测试同时 FAILED**（新测试 + `:234` 的 required-keep 兜底 + 4 条间接依赖 reference 集的），
还原后 `grep ZZZ_MUTANT` = 0。

## 7 两个被当成"存量"放过的红：一个门禁天生红，一个红了 8 天却修不了

`DB Migration Gate` 与 `Docker Security Scan` 从 `eb8f40b7a`（2026-10-19）起**每轮都红**，
长期以"存量、与代码无关"绕过。逐条查清后两者性质完全不同：前者是**我们的**门禁从上线那天就红
（可修），后者是**真 CVE 落在没人构建的目标上**（pin 已等于上游 latest，本仓修不了 ⇒ 删死路径）。

### 7.1 `Schema Blind Guards`：门禁上线那天就已经是红的（已修）

`DB Migration Gate` 的七个子 job 里，`db_migrate.sh Migrate Run` / `Unified Schema Apply` /
`Baseline Consolidation Check` / `Schema Contract Coverage` / `Schema Table Coverage` 全绿，
红的只有 `Schema Blind Guards`。

判据是**告警预算**：`WARNING_BUDGET` 只登记 `SBG-03` 一条（`allowed: 1`），实际有 3 条：

| 命中 | 引入 | 真实语义 | 修复前 |
| --- | --- | --- | --- |
| `scripts/ci/converge_public_schema.sh:245` | — | 收敛脚本清点 public 剩余表数，必须写死 public | `WARNING`（预算内） |
| `scripts/dev-test-setup.sh:61` | `ff56f36384`（10-18） | 断言 dev 容器里 `synapse_test.public` 真的收到了迁移 | `ERROR`（未登记） |
| `scripts/run_full_integration.sh:136` | `94d9e78bb8`（10-22） | 集成测试前置：目标库 public 至少 100 张基线表 | `ERROR`（未登记） |

后两条是**合法的前置断言**，而且改成 `current_schema()` 恰恰会让它们失去意义 —— 它们要证明的就是
"迁移打进了**那个固定库**"。问题只是**没登记**；而门禁上线（10-19）时 `dev-test-setup.sh` 那条
已经在树里 ⇒ **这个门禁从第一次运行起就是红的，从来没有过绿的状态**。

顺着查出第二个问题：`SAFE_FILES` 是**整文件**豁免，对所有规则生效。于是
`.github/workflows/ci.yml` 仅因一条 `table_schema='public'` 计数查询被豁免，
连带对 `SBG-01`（`DROP SCHEMA public CASCADE`，本门禁唯一要拦的破坏性 DDL）也免疫 ——
守卫存在的理由被它自己的白名单吃掉了。

修法：

1. 两条合法命中登记为 `SAFE_FILES` 条目，但**豁免收窄到逐规则**（新增 `rules` 字段）：
   只降级它真实命中的 `SBG-03`，其余规则照原 severity 计。
2. 新增 `validate_exemptions()`：条目缺 `reason`/`rules`，或 `rules` 写了不存在的规则编号
   ⇒ **启动即失败**。否则"登记了但登记错"会静默变成"什么都没做"。
3. 顺带修 `WARNING_BUDGET` 里 `converge_public_schema.sh:242` → `:245` 的行号漂移。

读数（同一份代码，修前 / 修后）：命中 **20 → 20 条不变**（`(规则, 文件)` 集合逐条相同）、
`INFO 17 → 19`、`WARNING 1 → 1`、`ERROR 2 → 0`、`EXIT 1 → 0`。

**A/B 自证（同一份变异、两个脚本）**：往 `scripts/dev-test-setup.sh` 尾部加一条真的
`DROP SCHEMA public CASCADE`：

* 旧脚本（整文件豁免）⇒ `[INFO ] SBG-01 scripts/dev-test-setup.sh:89` —— **降级、不可见**
* 新脚本（逐规则豁免）⇒ `[ERROR] SBG-01 scripts/dev-test-setup.sh:89` —— 拦住

`validate_exemptions()` 五个分支逐个实测：正常清单无问题；规则编号写错 / 漏 `rules` /
`rules` 为空集 / 漏 `reason` 各自给出对应报错。

### 7.2 `Base Image Scan`：真 CVE，但落在没人构建的目标上（已裁决：删死路径）

**症状**：`Docker Security Scan` 每轮红，唯一失败步骤是
`Scan distroless runtime base — blocking`。从 `trivy-base-sarif` artifact（22 MB 日志里只有
摘要，明细在 SARIF 的 `runs[0].results[]`）解析出的**唯一一条**：

```text
Package: libssl3
Installed Version: 3.0.20-1~deb12u2
Vulnerability: CVE-2026-84782   Severity: HIGH
Fixed Version:  3.0.22-1~deb12u2
Target: distroless/cc-debian12
```

**第一轮判断，一半作废**：当时定性为"`RUNTIME_BASE_IMAGE` 的 pin 落后于 Debian 12 安全更新"，
修法是换 digest；障碍是本机 `gcr.io` / `registry-1.docker.io` 都被代理拦成
`CONNECT tunnel failed, response 502`。于是先补了 `resolve-base-image-digests`
（`workflow_dispatch` only）把"拿 digest"自动化，再用它读出的值升了两个 pin。
**换完仍然红** —— 这才逼出下面这组读数。

#### 读数（2026-10-27，全部实测）

| # | 问题 | 读数 | 取法 |
| --- | --- | --- | --- |
| 1 | 换上去的 `e5d81dd…` 落后吗 | **不落后**，它就是上游当前最新 | `docker pull …:latest` 后 `docker inspect .RepoDigests` 与 pin 逐字相同 |
| 2 | 上游为什么不重建 | distroless 层内文件时间仍是 2026-06-07，`libssl3` 元数据写死 `3.0.20` | `docker export` 后读 `var/lib/dpkg/status.d/libssl3` |
| 3 | 谁在出货 | `docker/docker-compose.yml` `target: tools`、`docker/deploy/deploy.sh` `--target tools`、`trivy-scan` 也是 `--target tools` | 三处逐一核对 |
| 4 | 谁会用到被扫的镜像 | **没有任何入口**。全仓无 `--target runtime-distroless`（只有 Dockerfile 自身、守卫测试、归档审计提到） | `grep -rn runtime-distroless`（排除 `.worktrees/`） |
| 5 | 出货镜像干净吗 | 干净：`Trivy Image Scan` **success**。`tools` 阶段在构建期 `apt-get install libssl3` | 同 run 的 job 结论 + 容器内 `apt-cache policy libssl3` = `3.0.22-1~deb12u2` |
| 6 | 覆盖 `.so` 能不能把门禁救绿 | **不能**。两边路径同为 `/usr/lib/aarch64-linux-gnu/`（`runtime-libs` 确实会覆盖成 3.0.22），但 distroless 的 `var/lib/dpkg/status.d/libssl3` 仍写 `3.0.20`，而 Trivy 判版本只读**包元数据** ⇒ 扫"覆盖后的产物"照样红 | 本地 export 两个根文件系统逐项比对 |

**结论**：这条红是"**pin 里的层内容落后**"，不是"任何会被构建或运行的产物的暴露面"；
而且**本仓无从修复**（pin 已等于上游 latest，第 1 条）。它此前被当成"存量红"放过，
实质上是一条**永远绿不了的门禁** —— 与 §7.1 同型，只是成因在别人手里。

#### 处置：删死路径，而不是压制

`docs/audit/archive/DOCKER_REVIEW_2026-09-19.md` 的 P0-1 / O1-1 早就给过两条路：
「A. 让 `runtime-distroless` 真能跑」或「B. 明确废弃、删掉 Stage 2/3」。当时取了 A，
但**随后没有任何构建/部署入口接上它**，于是它以"永久红门禁"的形式还了债。

本次取 **B**：

| 文件 | 改动 |
| --- | --- |
| `docker/Dockerfile` | 删 `RUNTIME_BASE_IMAGE` ARG 与 `runtime-libs` / `runtime-distroless` 两个 stage；原 `Stage 4` 重编号为 `Stage 2` |
| `scripts/ci/read_base_image_pins.sh` | `ARGS` 3 → 2（`DEBIAN_BASE_IMAGE` / `RUST_BUILDER_IMAGE`）；对已删除的 ARG 仍 `exit 2`（实测） |
| `.github/workflows/docker-security-scan.yml` | 删那条阻断扫描 + 它的 SARIF upload；job 注释改为实情 |
| `tests/unit/workflow_pipefail_tests.rs` | 两处计数守卫 3 → 2；三处文档注释同步 |

顺带修掉一条**指向不存在测试的注释**：`base-image-scan` job 头原写
"守卫：`tests/unit/ci_test_scope_tests.rs::docker_security_scan_base_images_are_read_from_the_dockerfile`"
—— 该文件里没有这个函数（真实守卫在 `workflow_pipefail_tests.rs`）。这与本仓反复出现的
"声明判据 ≠ 执行判据"是同一个病：**守卫的名字写错了，读者会以为有守卫**。

#### 自证

* 改后三条守卫实跑 **`PASS` ×3**（`every_pinned_base_image_is_scanned_and_report_only_is_an_explicit_exception`、
  `base_image_digests_are_read_from_the_dockerfile_not_copied_into_workflows`、
  `blocking_trivy_steps_must_not_let_sarif_silently_drop_the_severity_filter`）。
* 变异 ①：把 debian 那条扫描的 `exit-code` 从 `'1'` 改成 `'0'` ⇒ **2 条守卫 `FAIL`**
  （`report-only` 变成 2 个 + 阻断数不足）。
* 变异 ②：在 job 里再插一条 `image-ref: …RUNTIME_BASE_IMAGE` 的阻断扫描 ⇒
  `base-image-scan 必须恰好扫两个 pinned 基础镜像（debian / builder）；实际扫描：[…] 3 条` ⇒ `FAIL`。
  两次变异后均已还原（`git diff` 只剩本节所述的预期改动）。
* 脚本面：`bash scripts/ci/read_base_image_pins.sh RUNTIME_BASE_IMAGE` 仍 `exit 2` 并打印
  `does not define ARG RUNTIME_BASE_IMAGE` —— **删掉的 ARG 不会静默变成空值**。

#### 两条别读错的推论

1. 删的是"**我们的**死路径"，不是"distroless 有问题"。若要重做 distroless 变体，正确姿势是
   **先 build 再扫产物**（`--target` 出真镜像），而不是扫裸基座 —— 后者会把"没人构建的层"
   当成"我们的暴露面"，正是本节的病根。
2. `Digest Pin Integrity` 与本节的关系没变：它只验"pin 在 registry 上仍有效"，
   **不验"pin 里有没有补丁"**；"pin 是否落后"由 `resolve-base-image-digests` 负责（dispatch only）。
   三个 job 的分工见 §7.2 上文的读数表。

#### 仍未闭合

`DB Migration Gate` 之外，`Docker Security Scan` 现在应全绿（`hadolint` / `trivy-scan` /
`base-image-scan` / `digest-pin-check`）；`Resolve Base Image Digests` 按设计只在
`workflow_dispatch` 上跑，push 时是 `skipped` —— **`skipped` 不是绿**，读结论时别混。

## 8 复查"存量红"时又抓到三类：门禁把绿判成红、真实供应链条目、把基础设施抖动当成代码回归

§7 的结论是"存量红这个标签会掩盖成因差别"。按它去复查 `e1379be27`（2026-10-27 13:35 的
`CI` 运行 `40513351117`）剩下的四红，又分出三类完全不同的东西 —— 其中一类是**门禁自己
把一次真跑过的测试判成空转**，它从上线那天起就没绿过。

### 8.1 `Appservice statistics cost gate`：`require_tests_ran.sh` 只认复数，恰跑 1 条就恒红

`CI` 的失败清单（`40513351117`）：`Code Coverage`、`Integration Tests (database)`、
`Integration Tests (schema)`、`Security Audit`。其中两条集成车道的失败步骤**完全相同**，
都是第 14 步 `Appservice statistics cost gate (relative to a DB round-trip)`。

该步骤的命令（`.github/workflows/ci.yml:1131`）是

```bash
bash scripts/ci/require_tests_ran.sh cargo nextest run --test performance_manual \
  --features "performance-tests,test-utils" --locked --test-threads 1 \
  -E 'test(statistics_per_service_cost)'
```

而 `-E 'test(statistics_per_service_cost)'` 在 `performance_manual` 里**恰好只选中 1 条**
用例。`Integration Tests (schema)` 的原话（job 日志，`15:42:43` → `15:42:47`）：

```text
    Starting 1 test across 1 binary (22 tests skipped)
        PASS [   3.331s] (1/1) synapse-rust::performance_manual appservice_scheduler_perf_tests::tests::statistics_per_service_cost_does_not_grow_with_service_count
     Summary [   3.331s] 1 test run: 1 passed, 22 skipped

##[error]this step ran ZERO tests. The target/module filter no longer matches
  anything, so the step is a no-op that reports success. Fix the target/module
  name (or delete the step) — do not leave it green-by-emptiness.
```

**测试 PASS 了，守卫说"一条都没跑"。** 原因在守卫的正则里：

```bash
elif ! grep -qE '(^[[:space:]]*test result: (ok|FAILED)\. [1-9][0-9]* passed|Starting [1-9][0-9]* tests)'
```

nextest 的计数行是**单复数变化**的：`Starting 3 tests across 1 binary`，但只选中一条时是
`Starting 1 test across 1 binary`。正则只写了复数形式 ⇒ 任何"恰好跑 1 条"的步骤 100% 被判
空转。该步骤由 `ea8276d8e`（补性能门禁那条提交）引入，**自引入起就没有绿过**。

这与该脚本自己的历史是同一类故障的第三次变形：2026-09-20 是 ANSI 颜色（`Starting` 被
SGR 序列打断）、2026-09-30 是 `grep -q` + `pipefail` 的 SIGPIPE（日志大于 64 KB 时误判），
这次是**单复数**。三次都是"判据写死了某一种输出形态"，而输出形态有多个合法变体。

**处置**：`Starting [1-9][0-9]* tests?`（单数、复数都认），并在
`tests/unit/ci_test_scope_tests.rs::require_tests_ran_sees_through_ansi_color` 里补两条断言
（单数必须绿、`Starting 0 tests` 必须仍红）。

| 证据 | 读数 |
|---|---|
| A/B（旧脚本 vs 新脚本，同一份 nextest 单数输出） | 旧 `exit 1` + `this step ran ZERO tests`；新 `exit 0` + `OK: this step actually ran tests.` |
| 反控 `Starting 0 tests across 1 binary` | 新旧都 `exit 1`（放宽到单数没有把真空放过） |
| 正控 `Starting 3 tests` | 新旧都 `exit 0` |
| 守卫测试 | `require_tests_ran_sees_through_ansi_color` **PASS**、`the_zero_tests_judgement_goes_red_and_a_real_run_passes` **PASS** |

### 8.2 `Security Audit` 的两条都是真的：一条 `unsound` 公告、一条许可缺口

同一运行里 `Security Audit` 红在 `Supply-chain gate (cargo-deny + cargo-audit)`，摘要两行失败：

```text
   advisories FAILED: 1 errors, 0 warnings, 2 notes
      licenses FAILED: 1 errors, 0 warnings, 552 notes
```

| 条目 | 原文 | 判定 |
|---|---|---|
| `RUSTSEC-2026-0386`（`tokio 1.53.1`） | `error[unsound]: AsyncFd::new and related constructors violate I/O safety`；`Solution: Upgrade to >=1.51.6, <1.52.0 OR >=1.53.3` | ✅ 真公告，已升 `tokio 1.53.1 → 1.54.1`（`cargo update -p tokio`）。本仓全仓 `AsyncFd` **零引用**，故实际暴露面为零，但门禁要的是"版本在补丁区间内" |
| `xxhash-rust 0.8.19` 的 `license = "BSL-1.0"` | `error[rejected]: failed to satisfy license requirements`；`BSL-1.0 - Boost Software License 1.0: OSI approved / FSF Free/Libre` | ✅ 真缺口，已按 `webpki-roots` 的**同一口径**做 crate 级例外 |

`xxhash-rust` 不是直接依赖：它是 `022cbd67f`（redis 0.29 → 1.7）带进来的**传递依赖**，
`BSL-1.0` 是宽松许可（义务只有保留声明）。这里刻意**不**把它塞进全局 `allow` ——
全局放行会让未来任意依赖的 `BSL-1.0` 静默通过，与"不留静默额度"的原则相反。

本地复验的边界必须写清：`cargo deny check licenses` ⇒ `licenses ok`（可离线跑）。
`cargo deny check advisories` 手动跑会失败：

```text
[ERROR] failed to fetch advisory database https://github.com/rustsec/advisory-db:
failed to prepare fetch: ... error sending request for url (.../info/refs?service=git-upload-pack)
```

（本机到 `github.com` 的 git 协议被拦，而 cargo-deny 只在缓存副本"该更新"时才去 fetch。）
但 **pre-push 钩子里的同一条 `cargo deny check advisories` 用的是缓存副本，它报 `advisories ok`**，
而那份缓存的 HEAD 是 `99670cfc`（2026-10-27，比该公告新），
`crates/tokio/RUSTSEC-2026-0386.md` 就在册 ⇒ **`1.54.1` 确实清掉了这条**。
最终仍以 CI 为准（CI 每次新拉 db），但它已经不是"只有 CI 能确认"的状态了。

⚠️ 另注：`Security Audit` 是**路径过滤**触发的，多数提交上它是 `skipped`（近 12 次
`ci.yml` 里 5 次 `skipped`、5 次 `failure`、1 次 `in_progress`）。**`skipped` 不是绿**，
所以这两条从 `022cbd67f`（2026-10-22）起就存在，只是一直没被这个 job 读到。

### 8.3 两个看起来像回归、其实不是的东西

| 现象 | 实测 | 性质 |
|---|---|---|
| `Integration Tests (database)` 第 15 步 `Assert no leftover test databases` 也红 | 该步 `if: always()`，与第 14 步同时红；日志里大量 `ERROR: out of shared memory`（锁表耗尽） | **次生**：共享内存耗尽把用例打断 ⇒ janitor 来不及回收 `test_*` 库。不是独立的卫生缺陷，跟着 §8.1 与覆盖率车道的并发问题一起解决 |
| `Benchmark` 的 `Run benchmarks` 红 | `##[error]The runner has received a shutdown signal` + `Process completed with exit code 143`，日志停在 `Compiling synapse-rust`（13:42:55 → 13:48:26 被杀） | **基础设施**：runner 被回收，不是断言失败、不是超时（该 job 没配 `timeout-minutes`）。`37b74872c` 与 `e1379be27` 两次同型，属复发型抖动，需单独决策（降并发 codegen / 拆 bench 构建），不在本轮改 |

### 8.4 一个必须说清的大盘事实

近 12 次 `ci.yml` 运行**没有一次 `success`**（`40596915572` 之前全部 `failure`）。
所以"某个 job 红了"这件事在本仓已经**失去信息量** —— 这正是 §7 说的
"存量红标签掩盖成因"的上游原因。逐条把红拆成"门禁自身缺陷 / 真实缺陷 / 基础设施"三类，
比追求"把红刷绿"更有意义。

## 9 复核命令（一键）

```bash
# ① 门禁本身
python3 scripts/check_schema_table_coverage.py
python3 scripts/ci/check_file_size_ratchet.py

# ② 27 个步骤逐个跑（抽出 ci.yml 的 repo-sanity 块，去掉 BASH_ENV、用干净 PATH）
python3 - <<'PY'
import re, pathlib, subprocess, os
block = pathlib.Path('.github/workflows/ci.yml').read_text().splitlines()[53:319]
steps, cur, i = [], None, 0
while i < len(block):
    m = re.match(r'^      - name: (.+)$', block[i])
    if m:
        cur = {'name': m.group(1), 'lines': []}; steps.append(cur); i += 1; continue
    if re.match(r'^        run: \|', block[i]) and cur:
        i += 1; body = []
        while i < len(block) and (block[i].startswith('          ') or not block[i].strip()):
            body.append(block[i][10:]); i += 1
        cur['lines'].extend(body); continue
    if re.match(r'^        run: (?!\|)', block[i]) and cur:
        cur['lines'].append(block[i].split('run: ', 1)[1]); i += 1; continue
    i += 1
env = {k: v for k, v in os.environ.items() if k != 'BASH_ENV'}
env['PATH'] = '/usr/bin:/bin:/opt/homebrew/bin:' + env.get('HOME', '') + '/.cargo/bin'
for s in steps:
    if not s['lines'] or s['name'] == 'Checkout code':
        continue
    p = subprocess.run(['bash', '-c', 'set -e\n' + '\n'.join(s['lines'])],
                       capture_output=True, text=True, env=env)
    print(('OK  ' if p.returncode == 0 else 'FAIL'), s['name'])
PY
```

> ⚠️ 本地跑第 1 步（`No private keys (key files)`）必红，那是 §4 说明的假红；
> 判 CI 是否绿只看 CI，不要拿本地这一条当缺陷。
