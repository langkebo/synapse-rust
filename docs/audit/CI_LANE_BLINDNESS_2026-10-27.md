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

一句话：**这些车道不是"没发现问题"，是"根本没运行/根本不可能运行"**。它们此前的沉默不能作为
任何"已合规"的证据。四个缺陷里有三个同源（§1 / §3 / §4）：**判据依赖"我这台机器上有什么"，而不是
"仓库里有什么"**；§2 只是漏跑 `--update`。排查时统一问一句：*这条判据在新克隆的树里还成立吗？*

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

## 6 仍未解决（附本轮三条复核记录：§6.1 修好 / §6.2 让渡 / §6.3 作废）

* **结构性**：GitHub 的步骤是顺序短路语义 —— **前一步失败，后面全部 `skipped`，且不产生任何读数**。
  这意味着"一个门禁红"会顺带让 22 个门禁**失去观测**，而页面上只呈现一条失败。可选的改法：
  ①把门禁拆成多个 job（失败面独立可见）；②对非阻塞步骤加 `if: always()` 并在末尾汇总。
  **未实施** —— 改 CI 语义属独立决策；且"全盲"这个直接后果已经消失：`ea9a57af1` 上该 job 实测
  **`success 26 / failure 1 / skipped 4`**（唯一红是 §6.2 的 `File-size ratchet`，已让渡；
  4 个 `skipped` 正是它后面的四步），让渡后 `d297b2937` 上 **31/31 全绿 / 0 skipped**。
  历史上那句 `failure 1 / skipped 22 / success 8` 不再成立。
  **注意这只治好了"看不见"，没治好"一红全盲"** ——
  下一次任何门禁红，后面仍会整片 `skipped`。
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

## 7 复核命令（一键）

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
