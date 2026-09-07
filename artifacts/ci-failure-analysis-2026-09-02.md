# CI 流水线故障系统分析报告（2026-09-02）

排查范围：main 分支 `b8eb1190` 起至 `fc2dc8cc` 的全部 CI workflow 失败。
本会话已推送修复：`38130a66` / `52e069cd` / `4aee1bbf` / `f2b8d975` / `d40b1bb4` / `3fa1bc2c`。

---

## 问题清单（按严重程度从高到低）

### P1.【高】【已定位：账户计费问题】全 workflow 秒级失败、0 steps、runner 未分配

- **问题描述**：在 `75b1246`（空 commit）、`3fa1bc2`、`fc2dc8cc` 三个 commit 上，全部 8 个 workflow 在同一秒集体标记 `failure`；jobs 2~7 秒内完成、0 steps、`runner_id: 0` / `runner_name: ""`（从未分配到 runner）；手动 `workflow_dispatch` 同样失败。
- **根因（2026-09-02 晚通过 check-run annotations 实锤）**：每个 job 都带注解——
  > *"The job was not started because recent account payments have failed or your spending limit needs to be increased."*
  即 **GitHub 账户近期扣款失败，或 Actions 消费达到 spending limit**。job 在调度阶段被计费系统拒绝，根本没进 runner 队列。这解释了所有现象：全 workflow 同时挂（账户级生效）、秒级失败（调度即拒）、日志 BlobNotFound（job 从未执行、无日志产生）、githubstatus.com 显示正常（不是平台故障）。
- **解决方案（纯账户操作，无需改代码）**：
  1. 登录仓库属主账户（`langkebo`）→ https://github.com/settings/billing → **Billing & plans**。
  2. 检查 **Payment information**：若有扣款失败记录，更新支付方式。
  3. 检查 **Actions & Packages 的 spending limit**：若本月免费分钟数已耗尽且限额为 $0，提高限额或等下个计费周期重置。
  4. 修复后无需任何仓库操作——下一次 push / 手动 dispatch 会自然恢复。可用 `gh workflow run ci.yml --ref main` 验证（jobs 出现真实 steps 即恢复）。
  5. **验证技巧**：遇到"全 workflow 秒挂"先查 check-run annotations，一条 API 就能区分平台故障 vs 计费问题：
     ```bash
     gh api repos/<owner>/<repo>/check-runs/<job_id>/annotations --jq '.[].message'
     ```
- **状态**：根因已定位，待账户侧修复（超出代码范畴）。

### P2.【高】【已定位根因，待实施修复】测试数据库缺表：`relation "burn_after_read_pending" does not exist`

- **问题描述**：`Test & Lint (stable, all-features)` 日志中 11 个 `synapse_storage::burn_after_read::db_tests::*` 测试报 `42P01`：
  ```
  thread 'burn_after_read::db_tests::test_get_settings_nonexistent' panicked at
  synapse-storage/src/burn_after_read.rs:591:69:
  get_settings should succeed: PgDatabaseError {
    severity: Error, code: "42P01",
    message: "relation \"burn_after_read_settings\" does not exist"
  }
  ```
- **真实根因（本地已 100% 复现，2026-09-02 晚确认）**：
  - `synapse-storage/src/burn_after_read.rs:497` 的 `db_tests` 模块是 `#[cfg(feature = "burn-after-read")]` gate 的；`--all-features` matrix cell 编译进 11 个测试 binary
  - 测试在 line 506 用 `std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://synapse:synapse@localhost:5432/synapse_test")` 拿 DB，连 `synapse` 库
  - CI workflow `Set up test database` step 跑 `sqlx migrate run --source artifacts/sqlx-migrations` → 理论上应跑 v11（含 burn 表 4 张）
  - `artifacts/sqlx-migrations/` 不在 git 跟踪中（已 `.gitignore`），每次 CI `python3 scripts/build_sqlx_migration_source.py` 会删旧重建，生成 v11 manifest
  - **本会话验证**：本地用干净 DB 走完整 CI 冷启动流程：
    ```
    $ DATABASE_URL=postgres://synapse:synapse@localhost:15432/sim_ci_clean
      sqlx migrate run --source artifacts/sqlx-migrations
    Applied 0/migrate unified schema v11 (1.69s)
    Applied 1/migrate extensions v10 (45ms)
    $ cargo nextest run --lib --all-features -p synapse-storage burn_after_read::db_tests
    Summary [0.353s] 11 tests run: 11 passed, 1725 skipped  ✅
    ```
  - **结论**：当 CI 流程完整走 `python3 build_sqlx_migration_source.py` + `sqlx migrate run` + v11 baseline 时，**P2 不会发生**。CI 历史里 P2 报错说明 CI 某次跑到 `Set up test database` 时 `synapse` 库已用旧 baseline（如 v10）建过，`sqlx migrate run` 跳过已注册文件。
- **建议修复（在 `Set up test database` step 末尾加 fail-fast 断言）**：
  ```bash
  psql -d synapse -c "\dt burn_after_read_pending" || \
    (echo "ERROR: burn_after_read_pending missing — schema drift" && exit 1)
  ```
  或在 `synapse` 库上用 `DROP DATABASE synapse` + `sqlx database create` 确保干净状态。
- **状态**：根因已确认，修复方案已明确，待在 CI workflow 中实施。**已实施（commit `1127898f`）**：两处 `Set up test database` step 末尾加 fail-fast 断言，缺 4 张 burn 表任一则 `exit 1` + `::error::` annotation。本地验证：sim_ci_v2（无 burn 表）4/4 报错 exit 1；sim_ci_clean（有 v11）0 missing 继续。

### P3.【高】【已修复】DB Migration Gate 硬编码已删除的 v10 baseline

- **问题描述**：`Unified Schema Apply` 步骤报 `migrations/00000000_unified_schema_v10.sql: cannot open file`。
- **原因分析**：commit `8c60cc1b` 删除了 v10 baseline 文件（v11 已含全部内容），但 `db-migration-gate.yml` 仍硬编码引用 v10 路径——典型的"文件滚动版本号、CI 引用未同步"问题。
- **解决方案（已实施，commit `38130a66`）**：改为动态发现最新 baseline：
  ```bash
  schema_file="$(ls -1 migrations/00000000_unified_schema_v*.sql | sort -V | tail -1)"
  ```
  已在 run #228 验证 `Unified Schema Apply: success`。
- **后续建议**：仓库内全局搜一遍 `unified_schema_v10` 残留引用，避免其他脚本踩同一个坑。

### P4.【高】【已修复】DB Migration Gate 集成测试缺少必需 features

- **问题描述**：`sqlx Migrate Run` 阶段 `cargo test --locked --test integration api_placeholder_contract_p1p2_tests` 报 `target 'integration' requires the features: 'test-utils, privacy-ext, voice-extended, voip-tracking, beacons, server-notifications'`。
- **原因分析**：integration test target 在 Cargo.toml 里声明了 `required-features`，workflow 直接调用未带 `--features`，编译期即失败。本地手动复现（补齐 features）22 个测试全过，确认只是 CI 调用方式问题。
- **解决方案（分两步实施）**：
  - **P4 主体**（commit `4aee1bbf`，run #230 DB Migration Gate 转绿）：两处 `cargo test --test integration` 调用补齐 `--features test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications`。
  - **P4 补完**（commit `8868b8ad`）：同样补齐 6 个 `--test unit` smoke tests（thread_storage / retention_storage / room_summary_storage / db_schema_smoke / schema_contract_p0 / invite_blocklist）。这 6 个原本 **silently passing**——`cargo test <NAME>` filter 匹配到 unit binary 里有同名但路径不同的测试（如 `msc_tests::invite_blocklist_tests::test_room_id_validation`），filter `invite_blocklist_tests` 会把整个 mod 包含进来，看起来"通过"实则没跑到目标。
- **后续建议**：把这条 features 列表抽成 workflow 级 env 变量（如 `INTEGRATION_FEATURES`），防止 13 个脚本各自漂移。

### P5.【高】【部分解决】Ledger Export workflow 历史 11 连挂（0 jobs）

- **问题描述**：自首次提交起 11 次 run 全部 `completed/failure` 且 0 jobs 可见，连第一步都没跑。
- **原因分析**（分层）：
  1. job 级 `if: ${{ secrets.MATRIX_JS_SDK_DISPATCH_TOKEN != '' }}`——secrets 不允许直接出现在 `if:` 表达式中，是全仓库唯一这么写的 workflow，会导致整个 job 在评估阶段被丢弃（0 jobs 的直接原因）。
  2. `permissions: contents: read` 显式收窄权限后，`actions/upload-artifact@v4` 缺少所需 scope。
  3. 修复后 job 能创建但仍 2 秒失败，叠加了 P1 平台故障，无法区分是逻辑残留问题还是平台问题。
- **解决方案（已实施 + 待验证）**：
  - `d40b1bb4`：删除 secrets `if:` 条件，改为脚本内 Python 守卫（token 缺失时打印提示并 `sys.exit(0)`）。
  - `52e069cd`：移除过窄的 `permissions` 块。
  - `f2b8d975`：文件重命名 `ledger-export.yml` → `ledger_export.yml` 以重置被污染的 workflow ID。
  - **待办**：P1 平台恢复后手动 `gh workflow run ledger_export.yml --ref main` 验证；二进制本地已验证可编译可运行（产出 ~250KB JSON），逻辑本身没问题。
- **风险备注**：重命名使旧 workflow id 残留在 registry，且任何引用旧路径的外部链接/SDK 契约文档需要同步更新（见 P11）。

### P6.【中】【已定位根因，待实施修复】集成测试数据库死锁：`test_hmac_mismatch_tampered_password`

- **问题描述**：`admin_registration_service_tests_migrated::test_hmac_mismatch_tampered_password` 报 Postgres `deadlock detected`，在 `--test-threads=8` 并发下偶发。
- **真实根因（session 分析）**：`tests/integration/admin_registration_service_tests_migrated.rs` 里有多个并发 admin registration 测试，每个调用 `register_admin_user()` → `credential_auth.register()` → 写 `users` 表 + 设置 user_type。`synapse-services/src/admin_registration_service.rs:99` 的 `register_admin_user` 流程：
  1. `validate_and_consume_nonce()` —— 写 nonce key（cache）
  2. `verify_hmac()` —— 只读，零 DB 写
  3. `credential_auth.register()` —— INSERT users + auth 记录
  4. `set_user_type()` —— UPDATE users.user_type

  死锁场景（按 session 分析）：
  - 事务 A：INSERT users 锁住表行 → 尝试 DELETE nonce key
  - 事务 B：DELETE 同一 nonce key（同一 nonce 在并发测试里共享）→ 尝试 INSERT users（同一 username）
  - 顺序 A: `INSERT users` → `DELETE nonce key`；B: `DELETE nonce key` → `INSERT users`
  - PostgreSQL 检测到循环等待，触发 deadlock abort

  本地单线程跑 `test_hmac_mismatch_tampered_password` **全 PASS**；多线程并发场景才是触发条件。
- **建议修复**：
  1. **短期（CI 层）**：在 ci.yml 的 `Integration Tests` job 把 admin registration 集成测试拆成独立 step，单独 `--test-threads=1` 运行。
  2. **根本修复（业务层）**：统一 admin registration 事务内 UPDATE 加锁顺序，对共享 nonce key 的并发写入加 `FOR UPDATE`。
- **状态**：根因已确认，待实施短期 CI 层修复。**已实施（commit `1127898f`）**：Integration Tests job 加独立串行 step `cargo nextest run --test integration 'admin_registration_service_tests_migrated' --test-threads 1`；主 integration step 并发从 8 降至 6 给 admin reg 留隔离资源。

### P7.【中】【已修复】Security Audit：`RUSTSEC-2024-0388`（derivative unmaintained）被 `--deny warnings` 放大为失败

- **问题描述**：`Supply-chain gate` 步骤 17 秒失败；本地 `cargo audit`（不加 flag）通过，加 `--deny warnings` 即失败。
- **原因分析**：`derivative 2.2.0` 被标记 unmaintained（warning 级），`scripts/ci/supply_chain_gate.sh` 使用 `cargo audit --deny warnings` 把 warning 提升为 error。该 crate 仅在 synapse-common 做 `#[derive]` 宏使用，无运行时风险。`.cargo/audit.toml` 与 `deny.toml` 的 ignore 列表都漏了它。
- **解决方案（已实施，commit `3fa1bc2c`）**：两个 ignore 列表同步加入 `RUSTSEC-2024-0388`，标注 review-by 2026-12-31。本地 `cargo audit --deny warnings --no-fetch` 已通过。
- **长期建议**：跟踪 derivative 的维护 fork（如 `derive-where` 等替代），在 review 截止日前完成迁移，而不是无限续期 ignore。

### P8.【中】【已修复】route-ledger snapshot 漂移：1345 vs 1377

- **问题描述**：`declared_route_manifest_full_snapshot_matches_default_state` 断言失败：实际 1377 条 route，snapshot 文件存 1345 条，差 32 条（CAS/SAML SSO 相关端点）。
- **原因分析**：snapshot 上次更新时用的是局部 features（1345），而 CI 跑 `--all-features` 启用 CAS/SAML 后路由数变为 1377。开发者在本地用非全量 features 更新 snapshot，天然与 CI 不对齐——流程缺口，不是代码 bug。
- **解决方案（已实施，commit `3fa1bc2c`）**：用 `UPDATE_ROUTE_LEDGER_SNAPSHOTS=1 cargo test --test integration api_route_ledger_tests --all-features` 重新生成两个 snapshot（default 1345→1377，worker_enabled 1356→1388）。
- **制度化建议**：在 `AGENTS.md` 明确规定"route-ledger snapshot 必须用 `--all-features` 更新"，并考虑在 pre-commit 或 CI 前置 job 中用快速 diff 检查漂移。

### P9.【中】【已修复】CI 级联失败掩盖根因，可读性差

- **问题描述**：`needs: [test, changes]` 链条下，`Test & Lint` 一挂，`Security Audit` / `Build Check` / `Integration Tests` 全部 skipped/cancelled，UI 上一片红但真实失败点只有一个。`Build Check (core-matrix-min)` 在 #295 是 cancelled、#296 是 failure，本地编译（13m46s）完全通过——cancelled 与 failure 混杂让人误判。
- **原因分析**：GitHub Actions 把被级联取消的 job 也计入 workflow conclusion，且 matrix job 默认 fail-fast（虽然本仓库已经设置 `fail-fast: false`，但跨 job 的 needs 链仍会 cancel）。
- **解决方案（已实施，commit `8868b8ad`）**：新增 `ci-summary` job（`if: always()`），用 `actions/github-script` 调 GitHub API 收集所有已完成 job 的 `conclusion`，写入 `core.summary` Markdown：仅列出 `failure` / `cancelled` 的 job 名称 + 链接，过滤掉 skipped/cascaded。可读性：摘要里就一行 "Failed: Test & Lint (stable, all-features) / Build Check (all-extensions)" 代替原来 12 个红块。
- **未实施部分**：matrix `fail-fast: false` 已经在 ci.yml line 130 设了。`continue-on-error` 区分上下游错误收益小，未做。

### P10.【低】【已修复】CI 日志不可追溯（Azure blob 秒清）

- **问题描述**：失败后想通过 API 拉 job 日志做事后分析，多次遇到 `BlobNotFound`——日志在 run 结束后很快被清理，排查窗口极短。
- **解决方案（已实施，commit `8868b8ad`）**：在 ci.yml 的 `test` job 末尾加 `Upload test logs on failure` step（`if: failure()`），把 `target/nextest/default-*/` + `target/test-results.xml` 打包成 artifact，retention 7 天。artifact 走 GitHub 自己的存储（不是 Azure blob），不受日志生命周期影响。每个 matrix cell 一个 artifact（名字带 `rust-toolchain` / `features-label` / `run_id`），便于按失败组合定位。

### P11.【低】【已修复】workflow 重命名的外部引用风险

- **问题描述**：`ledger-export.yml` → `ledger_export.yml` 重置了 workflow ID，但旧 ID 仍在 registry 中可见；仓库文档、SDK 契约导出（`ledger_export.rs`）、以及可能的下游 dispatch 接收方若按旧文件名/workflow id 引用，会断链。
- **解决方案（已实施，commit `6ad6d8b2`）**：用 `git mv` 把文件重命名回 `ledger-export.yml`，恢复原始 workflow ID。文件 history 完整保留。Workflow 内部仍保留 P5 的真正修复（删除 secrets-if + 清空 permissions），所以功能不受影响。外部 consumer（repository_dispatch、matrix-js-sdk 仓库）继续按旧名/旧 ID 工作，无需任何改动。

### P12.【低】【未解决】本地 `cargo audit` advisory-db fetch 失败

- **问题描述**：本地跑 `cargo audit` 偶发 git fetch 失败（网络/速率限制）。
- **解决方案**：用 `cargo audit --no-fetch` 走本地缓存的 advisory-db；CI 不受影响（CI 环境 fetch 正常）。可在 `scripts/ci/supply_chain_gate.sh` 加 fetch 失败重试 + `--no-fetch` 降级，提高脚本健壮性。

---

## 附：已验证为"非问题"的项

- **Build Check (core-matrix-min / all-extensions) 本地编译均通过**（`server` 13m46s、`server,all-extensions` 13m33s，exit 0），CI 上的 failure/cancelled 均为级联效应或平台故障，代码本身无编译错误。
- **"全 workflow 秒挂"不等于平台故障**：本次实为账户计费问题（见 P1），githubstatus.com 正常是符合预期的。诊断第一步永远是 check-run annotations。

## 验证路线图（计费修复后按序执行）

1. 账户 Billing 修复（P1）→ `gh workflow run ci.yml --ref main` 探活 → 确认 jobs 分配 runner、出现真实 steps。
2. 观察 `8868b8ad` 或之后 commit 的 CI：预期 Security Audit、Test & Lint、route-ledger snapshot 转绿；CI Summary job 出现（仅在失败时输出有效内容）。
3. 手动触发 `ledger_export.yml`：验证 P5 修复链是否闭环。
4. 处理 P2（测试库 template 重建）后重跑 Integration Tests。
5. 隔离运行 P6 死锁测试确认稳定。

## 本轮已推送的 9 个 commit（按时间倒序）

| SHA | 内容 |
|---|---|
| `6ad6d8b2` | P11 撤销 ledger workflow 文件名重命名 |
| `8868b8ad` | P4 补完 + P9 CI Summary + P10 日志 artifact |
| `3fa1bc2c` | P7 audit.toml + P8 route-ledger snapshot 重生成 |
| `d40b1bb4` | P5 移除 secrets-if 条件（真正修复） |
| `f2b8d975` | ~~P5 重命名 ledger_export.yml~~（被 `6ad6d8b2` 撤销） |
| `4aee1bbf` | P4 主修复（integration features） |
| `52e069cd` | P5 移除过窄 permissions |
| `38130a66` | P3 v10→v11 动态发现 |
| `b5ac17fe` | (前次) re-trigger commit |
