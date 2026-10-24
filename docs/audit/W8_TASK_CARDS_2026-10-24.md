# W8 任务卡（两名成员可直接领取）

> 来源：`COMPREHENSIVE_AUDIT_2026-10-22.md` §0.2（唯一权威清单，32 项）。
> 本文件只拆解**可排期**的 15 张卡；登记项见 §4，不允许自行领取。
> 编号与审计文档一一对应，关闭时必须在附录 D 增加一行并给出可复现证据。

## 0. 公共约束（两人都适用）

| 项 | 规则 |
|---|---|
| 分支 | `w8/a-<编号>` 或 `w8/b-<编号>`，一卡一分支 |
| 提交 | 一卡可多个提交；提交信息 scope 只能取 `core/ui/chat/mobile/plugin/hook/service/util/i18n/config/ci/test/tauri` |
| 门禁 | 提交前必须本地跑 `bash scripts/quality/preflight.sh`；**禁止** `--no-verify`、`--no-gpg-sign` |
| 判据 | 每张卡必须提供“把实现删掉/改错 ⇒ 用例变红”的实测输出，仅贴绿跑不算完成 |
| 审计文档 | 只允许在附录 D 增加**自己这张卡**的关闭行，禁止改他人条目与总览计数 |
| 共享文件 | `docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md`、`Cargo.lock`、`scripts/ci/schema_timestamp_baseline`、`scripts/ci/sleep_baseline` 属高冲突文件，改动前先在任务卡里写明 |
| 冲突处理 | 若必须修改对方所有权目录，先在卡内注明并让对方 review，不得静默改 |

### 文件所有权

| 成员 | 拥有（可自由改） | 只读（需对方同意） |
|---|---|---|
| A | `tests/**`、`scripts/ci/**`、`docs/**`（DOC-04 指定的三份除外见卡） | `synapse-*/src/**`、`src/bin/**`、`migrations/**` |
| B | `synapse-*/src/**`、`src/bin/**`、`migrations/**` | `tests/**`（新建专用文件除外）、`scripts/ci/**` |

---

## 1. 成员 A：测试可信度与门禁

### A1 · NEW-05（含 TEST-02 余项）「自证测试」面

- **级别**：P2｜**依赖**：无｜**前置**：先读 §七 TEST-02 的逐文件表
- **背景**：21 个候选文件存在“断言自己刚构造的字面量”，其中 3 处已逐行确认：
  `tests/unit/msc4108_rendezvous_route_tests.rs`（0 次 HTTP 调用、12 处 `let status/body/headers = <字面量>`）、
  `tests/integration/coverage_tests.rs`（51 个用例只构造 JSON）、
  `tests/unit/account_compat_route_tests.rs::test_deactivate_account_*`（3 条）。
  启发式扫描会把 insta 快照误判，**21 是候选数不是判决数**。
- **允许改动**：`tests/**`（可新建文件）
- **禁止改动**：`synapse-*/src/**`；不得为让测试变绿而改生产路由；不得删除既有用例（只能改写或改名并保留覆盖意图）
- **做法**：逐文件二选一——①接真 `Router`（`tower::ServiceExt::oneshot` + 真实层）；②改名并在文件头声明“只锁响应形状”，同时补一条接管真实 router 的用例
- **验收判据**：
  1. `tests/unit/msc4108_rendezvous_route_tests.rs` 至少 1 次真实 HTTP 调用：`rg -n 'oneshot|ServiceExt' tests/unit/msc4108_rendezvous_route_tests.rs` 非空，且 `rg -n '^\s*let (status|body|headers) = ' tests/unit/msc4108_rendezvous_route_tests.rs` 为 0
  2. 任取一条用例，把期望 `errcode` 改错 ⇒ 必须红（贴输出）
  3. 21 个候选文件每个都有“接真 / 声明只锁形状”的结论，写入审计 TEST-02 条目
- **完成定义**：审计 TEST-02 + NEW-05 两行可关闭；`cargo nextest run --test unit --test integration` 全绿

### A2 · SEC-14 联邦请求缺 `ts` 只告警

- **级别**：P3｜**依赖**：A 需先与 B 确认不触碰 `synapse-federation/src/signing.rs` 以外的生产文件
- **背景**：入站联邦鉴权对缺失 `ts` 只 `warn`，与 fail-closed 原则不符。
- **允许改动**：`tests/**`（红用例）、`synapse-federation/src/**` 中**仅**缺失 `ts` 判定所在的鉴权函数
- **禁止改动**：`synapse-web/src/federation/**` 路由层；不得放宽其他签名校验；不得改 `docker/**` 配置
- **验收判据**：
  1. 先提交“缺 `ts` 当前返回 2xx/放行”的红用例（提交信息注明 red）
  2. 改为拒绝后返回 401 + `M_UNAUTHORIZED`，同一用例转绿
  3. 变异：删掉拒绝分支 ⇒ 红（贴输出）
  4. 回归：现有联邦签名用例全绿（`cargo nextest run -E 'test(federation)'`）
- **完成定义**：审计 SEC-14 关闭；无既有联邦用例被弱化

### A3 · TEST-06 5 个 feature 门控文件

- **级别**：P2｜**依赖**：RD-14 已把 feature 矩阵接 CI（`--compile`）
- **背景**：5 个整文件以 feature 门控，默认测试车道不执行，改坏了也绿。
- **允许改动**：`tests/**`、`.github/workflows/ci.yml`（仅测试矩阵行）、`scripts/ci_backend_validation.sh`（仅测试批次）
- **禁止改动**：生产 crate 的 `Cargo.toml` feature 定义（属 B 范围）；不得用 `--all-features` 掩盖某个文件其实没被编译
- **验收判据**：
  1. 列出 5 个文件及其门控 feature，写入审计 TEST-06
  2. 默认车道至少执行其中每个文件的 1 条用例（`cargo nextest run` 输出中可见文件路径）
  3. 变异：把任一文件中的一个断言改错 ⇒ 默认车道红（贴输出）
- **完成定义**：审计 TEST-06 关闭；CI 默认车道日志能定位到这 5 个文件

### A4 · TEST-05 测试域 81 处 `sleep`

- **级别**：P2｜**依赖**：无｜**前置**：`python3 scripts/ci/check_sleep_ratchet.py --print` 取当前真值
- **背景**：总 107 处（PROD 26 / 测试域 81）。只改测试域，PROD 26 处不在本卡范围。
- **允许改动**：`tests/**`；必要时可新增 `tests/common/` 下的等待助手（不得改现有 helper 语义）
- **禁止改动**：`synapse-*/src/**` 中的 26 处 PROD `sleep`；不得调大 `sleep` 数值换绿；不得删用例
- **做法**：三桶——①条件轮询（带 deadline，超时即失败）；②`oneshot`/通知等待；③确有节流语义的保留并注明原因（计入棘轮不变）
- **验收判据**：
  1. `check_sleep_ratchet.py --print` 的测试域计数严格下降（目标 ≤81，逐批下降并更新 `scripts/ci/sleep_baseline`）
  2. 每批改动后全量 `cargo nextest run` 绿，且用例总时长不显著增加
  3. 变异：把任一处改回 `sleep` ⇒ 棘轮红（贴输出）
- **完成定义**：审计 TEST-05 关闭；baseline 文件与脚本输出一致

### A5 · CODE-11 `#[allow]` 缺理由

- **级别**：P3｜**依赖**：不单独开分支，随 A1/A2/A3/A4 触达文件顺手做
- **允许改动**：A 本轮触达的测试文件与脚本
- **禁止改动**：生产 crate 的 `#[allow]`（属 B 触达范围）；不得为凑数删除必要的 allow
- **验收判据**：`rg -c '#\[allow' <= 160`（无 reason 处数）只降不升；新增 allow 必须带 `// reason:`
- **完成定义**：审计 CODE-11 计数更新；无新增无理由 allow

### A6 · DOC-04 三份文档 9 组行号漂移

- **级别**：P3｜**依赖**：无
- **允许改动**：审计指定的三份文档
- **禁止改动**：`COMPREHENSIVE_AUDIT_2026-10-22.md` 正文（仅附录 D）
- **做法**：把 `path#L123` 改为符号名（函数/结构体/常量名），避免下次漂移
- **验收判据**：目标三份文档中 `rg -n '\.md#L[0-9]+'` 为 0；所有改为符号名的引用都能 `git grep` 到对应符号
- **完成定义**：审计 DOC-04 关闭；`check_doc_metrics.py` 通过

### A7 · NEW-03 依赖分裂跟踪表过期

- **级别**：P3｜**依赖**：`Cargo.lock` 冻结（改锁会重算）
- **允许改动**：`docs/synapse-rust/DEPENDENCY_UPGRADE_TRACKER.md`
- **禁止改动**：`Cargo.toml` / `Cargo.lock`；不得顺手升级依赖
- **验收判据**：以 `cargo tree -d --workspace --locked` 现算，写入 **47 crate 名 / 32 分裂组 / 15 同版本多来源**，并写明复现命令与统计口径；数字块与命令输出逐项一致
- **完成定义**：审计 NEW-03 关闭

### A8 · NEW-04 30 处悬挂 `scripts/...` 引用

- **级别**：P3｜**依赖**：无
- **允许改动**：引用所在的文档；确属错误的调用点
- **禁止改动**：不得新建“出现即要求存在”的朴素门禁（假阳性 30 条）
- **验收判据**：30 条逐条归类为「相对路径 / 历史叙述 / 真错」三桶；真错为 0；把分类口径写入审计条目
- **完成定义**：审计 NEW-04 关闭

---

## 2. 成员 B：运行时正确性与结构债

### B1 · CODE-01b `hash_token` panic + 8 个 `src/bin` 不校验

- **级别**：P2｜**依赖**：无｜**前置**：确认 `TOKEN_HASH_SECRET` 的注入方式（env / 配置）
- **背景**：缺 secret 时 `hash_token` panic；8 个二进制入口完全不校验，空密钥会 fail-open。
- **允许改动**：`synapse-services/src/**`、`synapse-common/src/**`、8 个 `src/bin/**` 入口
- **禁止改动**：`synapse-web/src/auth/**` 的认证链顺序；不得改 `TOKEN_HASH_SECRET` 的语义或默认值
- **验收判据**：
  1. 空/缺失 secret ⇒ `hash_token` 返回 `Err`（或启动期拒绝），**不 panic**
  2. 8 个 bin 在缺 secret 时启动即非零退出，stderr 指明缺失项
  3. 用例：空 secret 单测 + 每个 bin 的启动校验用例；变异：去掉 guard ⇒ 红（贴输出）
  4. 回归：认证相关用例全绿
- **完成定义**：审计 CODE-01b 关闭；无新增 `panic!`/`unwrap` 在安全路径

### B2 · CODE-08 恒 403 端点 + 无理由 `allow(dead_code)`

- **级别**：P2｜**依赖**：需先判定该端点是否应存在（查契约 `docs/synapse-rust/ROUTE_CONTRACT.md`）
- **允许改动**：该端点所在路由文件、对应存储/服务方法、契约文档对应行
- **禁止改动**：不得只把 `allow` 换成 `expect` 蒙混；不得删端点却不改契约
- **验收判据**：二选一落地——①端点实现并有用例（200/403 分支都覆盖）；②删除端点 + 契约同步删除 + 守卫用例断言“不再注册”
- **完成定义**：审计 CODE-08 关闭；`check_route_contract.sh` 绿

### B3 · ARCH-03 “未实现”两套错误语义

- **级别**：P2｜**依赖**：无｜**前置**：已在审计 §0.2.2 登记（本次补充）
- **背景**：`synapse-common/src/error.rs:314`（`not_implemented` → 404 `M_UNRECOGNIZED`）与 `:449`
  （`not_supported` → 501 `M_UNKNOWN`）并存，且都有使用者（`voice.rs` 用前者，
  `account_compat.rs` / `room/events.rs` 用后者）。**404 与 501 的区分是刻意的**
  （端点不存在 vs 端点存在但字段/扩展不支持），本卡**不合并状态码**；问题是客户端拿不到统一
  的对外契约，且 `M_UNRECOGNIZED`/`M_UNKNOWN` 在 404/501 之间混用。
- **允许改动**：`synapse-common/src/error.rs`、`synapse-web/src/**` 中这两个错误的映射、
  `docs/synapse-rust/ROUTE_CONTRACT.md`（新增语义表）、对应测试文件
- **禁止改动**：不得把两种状态码合并成一种；不得把真实内部错误映射成 404/501（会把缺陷伪装成
  “端点不存在/不支持”）；不得改 `voice.rs` / `account_compat.rs` 之外的业务分支
- **验收判据**：
  1. `ROUTE_CONTRACT.md` 有一张“未实现语义表”：404 配 `M_UNRECOGNIZED`、501 配 `M_UNKNOWN`（或定稿后的唯一取值），
     并写明“端点不存在 / 端点存在但不支持”的判定条件
  2. 12 个调用点全部对齐该表（逐个列出文件:行，改前/改后 errcode）
  3. 用例：两个代表端点（`voice` 的 404 分支、`account_compat` 的 501 分支）各断言 `status + errcode`；
     变异：把 404 改成 501 ⇒ 红（贴输出）
  4. 反向用例：真实内部错误仍为 500
- **完成定义**：审计 ARCH-03 关闭；契约与实现一致

### B4 · RD-27 `to_device` 4 个零调用方法

- **级别**：P2｜**依赖**：`synapse-e2ee` 是库 crate，删除属破坏性变更
- **允许改动**：`synapse-e2ee/src/storage/to_device.rs` 及 crate 根 re-export
- **禁止改动**：`to_device` 的真实投递路径；不得顺手改 SQLX 棘轮基线（除非确属删除后计数下降，需在卡内注明）
- **验收判据**：
  1. 贴出 4 个方法的 `git grep` 证据（生产 0 调用），以及“无外部消费者”的判定依据
  2. 删除或降级为 `pub(crate)`；其中一个语义与名字相反的方法必须改名或删除
  3. 全量 `cargo nextest run` 绿；`check_sqlx_dynamic_ratio.sh` 通过
- **完成定义**：审计 RD-27 关闭

### B5 · NEW-08 `device_sync.rs` 736 行死模块

- **级别**：P3｜**依赖**：B4 完成（同为 e2ee/federation 死代码清理）
- **允许改动**：`synapse-federation/src/device_sync.rs`、crate 根 re-export、只测死代码的 2 条用例、`scripts/ci/sqlx_dynamic_baseline`（如计数下降）
- **跨所有权改动（2026-10-24 补充，需 A 复核）**：删模块会连带两处 A 所有权文件变红，已确认按下列方式处理：
  1. `tests/unit/federation_device_list_writer_tests.rs::the_dead_device_revocation_job_does_not_come_back`
     把 `synapse-federation/src/device_sync.rs` 写死在扫描列表里，`read()` 在文件缺失时直接 panic
     ⇒ 改为“文件不存在即跳过”，保留“重新加回来仍会被扫描”的语义；
  2. `scripts/ci/file_size_baseline`、`scripts/ci/clippy_allow_baseline` 各有一条该文件的过期条目，
     删文件后两条棘轮会以 `stale baseline entry` 判红 ⇒ 删除该条目（两个棘轮都只许下降方向）。
  3. 原卡写的 `scripts/ci/sqlx_dynamic_baseline` 是笔误，实际文件为 `scripts/ci/sqlx_dynamic_ratio_baseline`：
     该模块含 3 条**生产静态查询**，删除后 `BASELINE_STATIC` 须按实测下调并写明“只因查询确实被删”。
  4. 随之必须同步两处文档数字块（`check_plan_numbers_fresh.sh` / `check_doc_metrics.py` 会判红）：
     `docs/audit/SQLX_STATICIZATION_PLAN_2026-09-23.md` 的 `sqlx-plan-numbers` 块、
     根目录 `TESTING.md` 的 `doc-metrics` 块（clippy allow 因删文件降 2 处）。
  5. 删文件后 `format_check.sh`/`rustfmt_all.sh --check` 会把**已从索引移除但仍在工作区删除**的路径当作
     待格式化文件而报错；提交前把删除一并 stage 即可通过（脚本行为，未改动门禁）。
  6. 顺带修复一条**他人遗留**的过期断言（CODE-03 `076309e7c` 把两笔写入事务化后未同步）：
     `tests/integration/e2ee_device_keys_storage_tests.rs::record_device_list_change_propagates_a_changes_insert_failure`
     原断言 `streamed == 1` 与实现的“同生共死”相反，HEAD 上稳定红（连跑 3 次）；改为 `streamed == 0`。
     §0 链条上属 `tests/**`（A 所有权），在此登记待复核。
  7. 全量车道还暴露 5 条**队友提交**留下的过期守卫（HEAD 上即红，与 B5 改动无关），已同批同步：
     `ci_test_scope_tests` 两条计数随 TEST-06 新车道调整（批次 3→5、矩阵环境 2→3，并明确 e2e 车道
     不参与逐字比对）；`workflow_pipefail_tests` 把 `cargo clippy --workspace --lib` 排除出
     “lib 测试运行器”判定（clippy 不执行用例）；`sqlx_cache_tooling_guard_tests` 两条 converge 用例
     补 `TEST_DB_TEMPLATE_SCHEMA`，让栏杆 7b 的 public / 参考集计数走桩的不同分支。
  8. 另有一条 **B6 自身遗留**（`1093f265f` 引入）：报告新增 MSC3245 未在 `MSC_SEMANTICS.md` 登记；
     `e2ee/devices.rs` 加 8 行注释后 `scripts/shell_routes_allowlist.txt` 的 `:386` 未跟改。
     两处已在本卡一并修正（属 `docs/**` 与 `scripts/`）。
- **禁止改动**：`routes/device.rs::broadcast_device_list_update` 的真实广播路径
- **验收判据**：
  1. 模块与 re-export 删除后 `cargo nextest run --all-features` 绿
  2. `check_sqlx_dynamic_ratio.sh` 绿，基线按“只降不升”更新并注明原因
  3. 守卫：`federation_device_list_writer_tests` 仍绿（真实路径未被误删）
- **验证记录（2026-10-25，隔离 worktree + 独立 target）**：
  preflight 全绿（fmt / format_check / aspell / markdownlint / plan numbers / doc metrics / route contract /
  doc_credibility）；`unit` 车道 **1980/1980 通过**；`federation_service_tests_migrated` 3/3；
  另取全量首轮报红最集中的 6 个套件（auth / widget / worker-replication / sliding-sync-storage /
  beacon / e2ee-device-keys）单跑 **130/130 通过**。
  全量 3738 条在本机不可用作判据：`max_locks_per_transaction=64`，并发建/删 230+ 表 schema 时
  PostgreSQL 报 `out of shared memory`（日志原文见 `Shared test schema clone failed … out of shared memory`），
  与代码无关；清理 173 个残留 schema 并限并发（`--test-threads 6`）后即全绿。
- **完成定义**：审计 NEW-08 关闭

### B6 · MAINT-04 + FUNC-05 能力台账失真

- **级别**：P2 / P3｜**依赖**：无，两张卡合并做
- **允许改动**：`docs/synapse-rust/API_COVERAGE_REPORT.md`、能力台账生成/校验脚本、契约对应行
- **禁止改动**：不得把“未实现”直接改成“已实现”来消除偏差；不得删除 RD-15/16 已登记的 9 条
- **验收判据**：
  1. 台账每一行都能追溯到路由注册（脚本或守卫），或显式标注“设计决策/未实现”
  2. 变异：改坏台账一行 ⇒ 守卫红（贴输出）
  3. FUNC-05 的联邦能力项全部登记为设计决策，不残留“未登记”
- **完成定义**：审计 MAINT-04、FUNC-05 关闭

### B7 · ARCH-02 `sync` / `sliding_sync` 共享语义基座（最后做）

- **级别**：P2｜**依赖**：A4（确定性等待设施）完成后才开工
- **允许改动**：`synapse-services/src/sync/**`、`sliding_sync/**`、新建等价断言测试文件
- **禁止改动**：不得改变 `/sync` 与 sliding sync 对外的响应形状；不得删任一实现
- **验收判据**：
  1. **先**写“两条路径可见事件集合一致”的等价断言（同一夹具下事件 ID 集合相同），并在当前实现上验证它能失败（人为制造差异 ⇒ 红）
  2. 抽取共享基座后，该断言绿；两条路径各自既有用例全绿
  3. 变异：只改一条路径的过滤条件 ⇒ 等价断言红（贴输出）
- **完成定义**：审计 ARCH-02 关闭；等价断言长期留库

---

## 3. 领取顺序

| 轮次 | A | B |
|---|---|---|
| 1 | A2 SEC-14（最小、先红后绿样板） → A1 NEW-05 | B1 CODE-01b → B2 CODE-08 |
| 2 | A3 TEST-06 → A4 TEST-05（分批） | B3 ARCH-03 → B4 RD-27 |
| 3 | A6 / A7 / A8（低优先，穿插） | B5 NEW-08 → B6 MAINT-04+FUNC-05 |
| 4 | 为 B7 提供等价断言的等待助手 | B7 ARCH-02 |

说明：A2 排第一是为了先建立“红 → 绿 + 变异”的团队样板；B1 排第一是因为它是唯一触及 fail-open 的项。

---

## 4. 登记项（不可领取）

| 项 | 原因 |
|---|---|
| ARCH-04 `events` 未分区 | 需生产级数据量，等外部条件 |
| ARCH-05 7 个 >2000 行文件 | 拆完行为不变，拿不出可失败判据 |
| PERF-01 `to_device_messages` 按龄清理 | 产品语义待定（未确认算不算已投递） |
| PERF-10 / PERF-11 / PERF-12 + MAINT-07 | 部署配置 / 无实测劣化 / 重生成会退化治理 |
| MAINT-01 feature 隔离 | 收益低于改动面 |
| SEC-09 / SEC-11 / SEC-12 / SEC-13 / SEC-15 | 部署形态依赖 / 不可达 / legacy 语义 / 运维手册 / 白名单口径 |
| CODE-10 837 处吞错 | 设配额会诱导写 `unwrap()` 凑数 |
| TEST-02 中“安全头 / 路由台账”2 项 | 判定不适用，已在审计说明 |

---

## 5. 审计登记状态

`ARCH-03` 原先在审计 §0.2 三张表中缺失，已补入 **§0.2.2（需要决策）**——原因是第一步要定稿对外
错误码语义表，而不是直接改状态码。B3 领取时无需再补登记。
